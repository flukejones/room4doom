//! Door movement thinker, controls open/close, locked.

use log::{debug, error, warn};
use std::fmt::{self, Formatter};
use std::ptr::null_mut;

use crate::d_thinker::{ActionF, Think, Thinker, ObjectType};
use crate::doom_def::TICRATE;
use crate::level_data::map_defs::Sector;
use crate::level_data::Level;
use crate::p_specials::{find_lowest_ceiling_surrounding, move_plane, PlaneResult};
use crate::{doom_def::Card, level_data::map_defs::LineDef, p_map_object::MapObject, DPtr};

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
    fn think(object: &mut ObjectType, _level: &mut Level) -> bool {
        let door = object.bad_mut::<VerticalDoor>();

        match door.direction {
            0 => {
                door.topcountdown -= 1;
                if door.topcountdown == 0 {
                    debug!("Door for sector {:?} should go down", door.sector.as_ptr());
                    match door.kind {
                        DoorKind::BlazeRaise => {
                            door.direction = -1;
                        }
                        DoorKind::Normal => {
                            door.direction = -1;
                        }
                        DoorKind::Close30ThenOpen => {
                            door.direction = -1;
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
                    debug!("Door for sector {:?} should go up", door.sector.as_ptr());
                    match door.kind {
                        DoorKind::RaiseIn5Mins => {
                            door.direction = 1;
                            door.kind = DoorKind::Normal;
                        }
                        _ => {
                            warn!("Invalid door kind: {:?}", door.kind);
                        }
                    }
                }
            }
            -1 => {
                debug!("Lower door for sector {:?}", door.sector.as_ptr());
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
                            unsafe {
                                door.sector.specialdata = None;
                                (*door.thinker).set_action(ActionF::Remove);
                                // TODO: sound
                            }
                        }
                        DoorKind::Normal | DoorKind::Close => unsafe {
                            door.sector.specialdata = None;
                            (*door.thinker).set_action(ActionF::Remove);
                        },
                        DoorKind::Close30ThenOpen => {
                            door.direction = 0;
                            door.topcountdown = TICRATE * 30;
                        }
                        _ => {}
                    }
                }
            }
            1 => {
                debug!("Raise door for sector {:?}", door.sector.as_ptr());
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
                            (*door.thinker).set_action(ActionF::Remove);
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

    fn thinker(&self) -> *mut Thinker {
        self.thinker
    }
}

/// EV_DoDoor
/// Can affect multiple sectors via the sector tag
pub fn ev_do_door(line: DPtr<LineDef>, kind: DoorKind, level: &mut Level) -> bool {
    let mut ret = false;
    for sector in level
        .map_data
        .sectors()
        .iter()
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
                    // TODO: S_StartSound(&door->sector->soundorg, sfx_doropn);
                }
            }
            DoorKind::BlazeRaise | DoorKind::BlazeOpen => {
                door.topheight = top;
                door.topheight -= 4.0;
                door.direction = 1;
                door.speed *= 4.0;
                if door.topheight != sec.ceilingheight {
                    // TODO: S_StartSound(&door->sector->soundorg, sfx_bdopn);
                }
            }
            DoorKind::BlazeClose => {
                door.topheight = top;
                door.topheight -= 4.0;
                door.direction = -1;
                door.speed *= 4.0;
                // TODO: S_StartSound(&door->sector->soundorg, sfx_bdcls);
            }
            DoorKind::Close30ThenOpen => {
                door.topheight = sec.ceilingheight;
                door.direction = -1;
                // TODO: S_StartSound(&door->sector->soundorg, sfx_dorcls);
            }
            DoorKind::Close => {
                door.topheight = top;
                door.topheight -= 4.0;
                door.direction = -1;
                // TODO: S_StartSound(&door->sector->soundorg, sfx_dorcls);
            }
            _ => {}
        }

        let thinker = MapObject::create_thinker(
            ObjectType::VDoor(door),
            ActionF::Think(VerticalDoor::think),
        );

        if let Some(ptr) = level.thinkers.push::<VerticalDoor>(thinker) {
            unsafe {
                (*ptr).set_obj_thinker_ptr::<VerticalDoor>(ptr);

                sec.specialdata = Some(ptr);
            }
        }
    }

    ret
}

pub fn ev_vertical_door(mut line: DPtr<LineDef>, thing: &MapObject, level: &mut Level) {
    if let Some(player) = thing.player {
        let player = unsafe { player.as_ref() };
        match line.special {
            26 | 32 => {
                if !player.cards[Card::it_bluecard as usize]
                    && !player.cards[Card::it_blueskull as usize]
                {
                    // TODO: player->message = DEH_String(PD_BLUEK);
                    // TODO: S_StartSound(NULL,sfx_oof);
                    println!("Ooof! I need the blue card");
                    return;
                }
            }
            27 | 34 => {
                if !player.cards[Card::it_yellowcard as usize]
                    && !player.cards[Card::it_yellowskull as usize]
                {
                    // TODO: player->message = DEH_String(PD_YELLOWK);
                    // TODO: S_StartSound(NULL,sfx_oof);
                    println!("Ooof! I need the yellow card");
                    return;
                }
            }
            28 | 33 => {
                if !player.cards[Card::it_redcard as usize]
                    && !player.cards[Card::it_redskull as usize]
                {
                    // TODO: player->message = DEH_String(PD_REDK);
                    // TODO: S_StartSound(NULL,sfx_oof);
                    println!("Ooof! I need the red card");
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
    if line.backsector.is_none() {
        error!("ev_vertical_door: tried to operate on a line that is not two-sided");
        return;
    }

    // new door thinker
    let mut sec = line.backsector.clone().unwrap();

    // if the sector has an active thinker, use it
    if let Some(data) = sec.specialdata {
        // TODO:
        let mut door = unsafe { (*data).obj_mut::<VerticalDoor>() };
        match line.special {
            1 | 26 | 27 | 28 | 117 => {
                if door.direction == -1 {
                    door.direction = 1; // go back up
                } else {
                    if thing.player.is_none() {
                        return; // bad guys never close doors
                    }

                    if matches!(door.thinker_ref().obj_type(), ObjectType::VDoor(_)) {
                        door.direction = -1;
                    } else if matches!(door.thinker_ref().obj_type(), ObjectType::VDoor(_)) { // TODO: PLATFORM
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
        1 | 26 | 27 | 28 => door.kind = DoorKind::Normal,
        31 | 32 | 33 | 34 => {
            door.kind = DoorKind::Open;
            line.special = 0;
        }
        117 => {
            door.kind = DoorKind::BlazeRaise;
            door.speed = VDOOR * 2.0;
        }
        118 => {
            door.kind = DoorKind::BlazeOpen;
            line.special = 0;
            door.speed = VDOOR * 2.0;
        }
        _ => {}
    }

    door.topheight = find_lowest_ceiling_surrounding(sec.clone());
    door.topheight -= 4.0;

    debug!("Activated door: {door:?}");
    let thinker = MapObject::create_thinker(
        ObjectType::VDoor(door),
        ActionF::Think(VerticalDoor::think),
    );

    if let Some(ptr) = level.thinkers.push::<VerticalDoor>(thinker) {
        unsafe {
            (*ptr).set_obj_thinker_ptr::<VerticalDoor>(ptr);

            sec.specialdata = Some(ptr);
        }
    }
}
