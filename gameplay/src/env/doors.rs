//! Door movement thinker, controls open/close, locked.
//!
//! Doom source name `p_doors`

use log::{debug, error, warn};
use sound_traits::SfxNum;
use std::{
    fmt::{self, Formatter},
    ptr::null_mut,
};

use crate::{
    doom_def::{Card, TICRATE},
    lang::english::{PD_BLUEK, PD_REDK, PD_YELLOWK},
    level::{
        map_defs::{LineDef, Sector},
        Level,
    },
    thing::MapObject,
    thinker::{Think, Thinker, ThinkerData},
    DPtr, LineDefFlags,
};

use crate::env::{
    specials::{find_lowest_ceiling_surrounding, move_plane, PlaneResult},
    switch::start_sector_sound,
};

const VDOOR: f32 = 2.0;
const VDOORWAIT: i32 = 150;
const VDOORSPEED: f32 = 2.0;

#[derive(Debug, Clone, Copy)]
pub enum DoorKind {
    Normal,
    Close30ThenOpen,
    Close,
    Open,
    RaiseIn5Mins,
    BlazeRaise,
    BlazeOpen,
    BlazeClose,
}

pub struct VerticalDoor {
    pub thinker: *mut Thinker,
    pub sector: DPtr<Sector>,
    pub kind: DoorKind,
    pub topheight: f32,
    pub speed: f32,
    // 1 = up, 0 = waiting, -1 = down
    pub direction: i32,
    // tics to wait at the top
    pub topwait: i32,
    // (keep in case a door going down is reset)
    // when it reaches 0, start going down
    pub topcountdown: i32,
}

impl fmt::Debug for VerticalDoor {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VerticalDoor")
            .field("kind", &self.kind)
            .field("topheight", &self.topheight)
            .field("speed", &self.speed)
            .field("direction", &self.direction)
            .field("topwait", &self.topwait)
            .field("topcountdown", &self.topcountdown)
            .finish()
    }
}

impl Think for VerticalDoor {
    fn think(object: &mut Thinker, level: &mut Level) -> bool {
        let door = object.vdoor_mut();
        #[cfg(null_check)]
        if door.thinker.is_null() {
            std::panic!("NULL");
        }
        let line = door.sector.lines[0].as_ref();

        match door.direction {
            0 => {
                door.topcountdown -= 1;
                if door.topcountdown == 0 {
                    debug!("Door for sector {:?} should go down", door.sector.as_ref());
                    match door.kind {
                        DoorKind::BlazeRaise => {
                            door.direction = -1;
                            start_sector_sound(line, SfxNum::Bdcls, &level.snd_command);
                        }
                        DoorKind::Normal => {
                            door.direction = -1;
                            start_sector_sound(line, SfxNum::Dorcls, &level.snd_command);
                        }
                        DoorKind::Close30ThenOpen => {
                            door.direction = 1;
                            start_sector_sound(line, SfxNum::Doropn, &level.snd_command);
                        }
                        _ => {
                            warn!("Invalid door kind: {:?}", door.kind);
                        }
                    }
                }
            }
            2 => {
                // INITIAL WAIT
                door.topcountdown -= 1;
                if door.topcountdown == 0 {
                    debug!("Door for sector {:?} should go up", door.sector.as_ref());
                    match door.kind {
                        DoorKind::RaiseIn5Mins => {
                            door.direction = 1;
                            door.kind = DoorKind::Normal;
                            start_sector_sound(line, SfxNum::Doropn, &level.snd_command);
                        }
                        _ => {
                            warn!("Invalid door kind: {:?}", door.kind);
                        }
                    }
                }
            }
            -1 => {
                debug!("Lower door for sector {:?}", door.sector.as_ref());
                let res = move_plane(
                    door.sector.clone(),
                    door.speed,
                    door.sector.floorheight,
                    false,
                    1,
                    door.direction,
                );

                if matches!(res, PlaneResult::PastDest) {
                    match door.kind {
                        DoorKind::BlazeRaise | DoorKind::BlazeClose => {
                            start_sector_sound(line, SfxNum::Bdcls, &level.snd_command);
                            unsafe {
                                door.sector.specialdata = None;
                                (*door.thinker).mark_remove();
                            }
                        }
                        DoorKind::Normal | DoorKind::Close => unsafe {
                            door.sector.specialdata = None;
                            (*door.thinker).mark_remove();
                        },
                        DoorKind::Close30ThenOpen => {
                            door.direction = 0;
                            door.topcountdown = TICRATE * 30;
                        }
                        _ => {}
                    }
                } else if matches!(res, PlaneResult::Crushed) {
                    match door.kind {
                        DoorKind::BlazeClose | DoorKind::Close => {}
                        _ => {
                            door.direction = 1;
                            door.kind = DoorKind::Normal;
                            start_sector_sound(line, SfxNum::Doropn, &level.snd_command);
                        }
                    }
                }
            }
            1 => {
                debug!("Raise door for sector {:?}", door.sector.as_ref());
                let res = move_plane(
                    door.sector.clone(),
                    door.speed,
                    door.topheight,
                    false,
                    1,
                    door.direction,
                );

                if matches!(res, PlaneResult::PastDest) {
                    match door.kind {
                        DoorKind::BlazeRaise | DoorKind::Normal => {
                            door.direction = 0; // wait at top
                            door.topcountdown = door.topwait;
                        }
                        DoorKind::Close30ThenOpen | DoorKind::BlazeOpen | DoorKind::Open => unsafe {
                            door.sector.specialdata = None;
                            (*door.thinker).mark_remove();
                        },
                        _ => {}
                    }
                }
            }
            _ => warn!("Invalid door direction of {}", door.direction),
        };

        true
    }

    fn set_thinker_ptr(&mut self, ptr: *mut Thinker) {
        self.thinker = ptr;
    }

    fn thinker_mut(&mut self) -> &mut Thinker {
        #[cfg(null_check)]
        if self.thinker.is_null() {
            std::panic!("vdoor thinker was null");
        }
        unsafe { &mut *self.thinker }
    }

    fn thinker(&self) -> &Thinker {
        #[cfg(null_check)]
        if self.thinker.is_null() {
            std::panic!("vdoor thinker was null");
        }
        unsafe { &*self.thinker }
    }
}

/// EV_DoDoor
/// Can affect multiple sectors via the sector tag
pub fn ev_do_door(line: DPtr<LineDef>, kind: DoorKind, level: &mut Level) -> bool {
    let mut ret = false;
    for sector in level
        .map_data
        .sectors_mut()
        .iter_mut()
        .filter(|s| s.tag == line.tag)
    {
        if sector.specialdata.is_some() {
            continue;
        }
        // Because we need to break lifetimes...
        let mut sec = DPtr::new(sector);

        ret = true;
        let mut door = VerticalDoor {
            thinker: null_mut(),
            sector: DPtr::new(sector),
            kind,
            topheight: 0.0,
            speed: VDOORSPEED,
            direction: 1,
            topwait: VDOORWAIT,
            topcountdown: 0,
        };

        let top = find_lowest_ceiling_surrounding(sec.clone());
        match kind {
            DoorKind::Normal | DoorKind::Open => {
                door.topheight = top;
                door.topheight -= 4.0;
                door.direction = 1;
                if door.topheight != sec.ceilingheight {
                    start_sector_sound(&line, SfxNum::Doropn, &level.snd_command);
                }
            }
            DoorKind::BlazeRaise | DoorKind::BlazeOpen => {
                door.topheight = top;
                door.topheight -= 4.0;
                door.direction = 1;
                door.speed *= 4.0;
                if door.topheight != sec.ceilingheight {
                    start_sector_sound(&line, SfxNum::Bdopn, &level.snd_command);
                }
            }
            DoorKind::BlazeClose => {
                door.topheight = top;
                door.topheight -= 4.0;
                door.direction = -1;
                door.speed *= 4.0;
                start_sector_sound(&line, SfxNum::Bdcls, &level.snd_command);
            }
            DoorKind::Close30ThenOpen => {
                door.topheight = sec.ceilingheight;
                door.direction = -1;
                start_sector_sound(&line, SfxNum::Dorcls, &level.snd_command);
            }
            DoorKind::Close => {
                door.topheight = top;
                door.topheight -= 4.0;
                door.direction = -1;
                start_sector_sound(&line, SfxNum::Dorcls, &level.snd_command);
            }
            _ => {}
        }

        let thinker =
            MapObject::create_thinker(ThinkerData::VerticalDoor(door), VerticalDoor::think);

        if let Some(ptr) = level.thinkers.push::<VerticalDoor>(thinker) {
            ptr.set_obj_thinker_ptr();
            sec.specialdata = Some(ptr);
        }
    }

    ret
}

pub fn ev_vertical_door(mut line: DPtr<LineDef>, thing: &mut MapObject, level: &mut Level) {
    if let Some(player) = thing.player_mut() {
        match line.special {
            26 | 32 => {
                if !player.cards[Card::Bluecard as usize] && !player.cards[Card::Blueskull as usize]
                {
                    player.message = Some(PD_BLUEK);
                    start_sector_sound(&line, SfxNum::Oof, &level.snd_command);
                    return;
                }
            }
            27 | 34 => {
                if !player.cards[Card::Yellowcard as usize]
                    && !player.cards[Card::Yellowskull as usize]
                {
                    player.message = Some(PD_YELLOWK);
                    start_sector_sound(&line, SfxNum::Oof, &level.snd_command);
                    return;
                }
            }
            28 | 33 => {
                if !player.cards[Card::Redcard as usize] && !player.cards[Card::Redskull as usize] {
                    player.message = Some(PD_REDK);
                    start_sector_sound(&line, SfxNum::Oof, &level.snd_command);
                    return;
                }
            }
            _ => {
                // Ignore
            }
        }
    }

    // TODO: if the sector has an active thinker, use it
    // sec = sides[line->sidenum[side ^ 1]].sector;
    if line.flags & LineDefFlags::TwoSided as u32 == 0 {
        error!("ev_vertical_door: tried to operate on a line that is not two-sided");
        return;
    }

    // new door thinker
    let mut sec = line.backsector.clone().unwrap();

    // if the sector has an active thinker, use it
    if let Some(data) = sec.specialdata {
        // TODO:
        let mut door = unsafe { (*data).vdoor_mut() };
        match line.special {
            1 | 26 | 27 | 28 | 117 => {
                if door.direction == -1 {
                    door.direction = 1; // go back up
                } else {
                    if thing.player().is_none() {
                        return; // bad guys never close doors
                    }

                    if matches!(door.thinker().data(), ThinkerData::VerticalDoor(_)) {
                        door.direction = -1;
                    } else if matches!(door.thinker().data(), ThinkerData::VerticalDoor(_)) { // TODO: PLATFORM
                    } else {
                        error!("ev_vertical_door: tried to close something that is not a door or platform");
                        door.direction = -1;
                    }
                }
                return;
                // dfsdf
            }
            _ => {}
        }
    }

    let mut door = VerticalDoor {
        thinker: null_mut(),
        sector: sec.clone(),
        kind: DoorKind::Normal,
        topheight: 0.0,
        speed: VDOORSPEED,
        direction: 1,
        topwait: VDOORWAIT,
        topcountdown: 0,
    };

    match line.special {
        1 | 26 | 27 | 28 => {
            door.kind = DoorKind::Normal;
            start_sector_sound(&line, SfxNum::Doropn, &level.snd_command);
        }
        31 | 32 | 33 | 34 => {
            door.kind = DoorKind::Open;
            line.special = 0;
            start_sector_sound(&line, SfxNum::Doropn, &level.snd_command);
        }
        117 => {
            door.kind = DoorKind::BlazeRaise;
            door.speed = VDOOR * 2.0;
            start_sector_sound(&line, SfxNum::Bdopn, &level.snd_command);
        }
        118 => {
            door.kind = DoorKind::BlazeOpen;
            line.special = 0;
            door.speed = VDOOR * 2.0;
            start_sector_sound(&line, SfxNum::Bdopn, &level.snd_command);
        }
        _ => {
            start_sector_sound(&line, SfxNum::Doropn, &level.snd_command);
        }
    }

    door.topheight = find_lowest_ceiling_surrounding(sec.clone());
    door.topheight -= 4.0;

    debug!("Activated door: {door:?}");
    let thinker = MapObject::create_thinker(ThinkerData::VerticalDoor(door), VerticalDoor::think);

    if let Some(ptr) = level.thinkers.push::<VerticalDoor>(thinker) {
        ptr.set_obj_thinker_ptr();
        sec.specialdata = Some(ptr);
    }
}
