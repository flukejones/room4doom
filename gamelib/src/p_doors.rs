use log::{debug, error, warn};
use std::ptr::NonNull;

use crate::d_thinker::{ActionF, Think, Thinker, ThinkerType};
use crate::doom_def::TICRATE;
use crate::level_data::level::Level;
use crate::p_floor::move_plane;
use crate::p_spec::{find_lowest_ceiling_surrounding, DoorKind, ResultE};
use crate::{
    doom_def::Card, level_data::map_defs::LineDef, p_map_object::MapObject, p_spec::VerticalDoor,
    DPtr,
};

const VDOOR: f32 = 2.0;
const VDOORWAIT: i32 = 150;
const VDOORSPEED: f32 = 2.0;

impl Think for VerticalDoor {
    fn think(object: &mut ThinkerType, _level: &mut Level) -> bool {
        let door = object.bad_mut::<VerticalDoor>();

        match door.direction {
            0 => {
                door.topcountdown -= 1;
                if door.topcountdown == 0 {
                    debug!("Door for sector {:?} should go down", door.sector.as_ptr());
                    match door.kind {
                        DoorKind::vld_blazeRaise => {
                            door.direction = -1;
                        }
                        DoorKind::vld_normal => {
                            door.direction = -1;
                        }
                        DoorKind::vld_close30ThenOpen => {
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
                        DoorKind::vld_raiseIn5Mins => {
                            door.direction = 1;
                            door.kind = DoorKind::vld_normal;
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

                if matches!(res, ResultE::PastDest) {
                    match door.kind {
                        DoorKind::vld_blazeRaise | DoorKind::vld_blazeClose => {
                            door.sector.specialdata = None;
                            // TODO: sound
                            unsafe {
                                door.sector.specialdata = None;
                                door.thinker.as_mut().set_action(ActionF::Remove);
                            }
                        }
                        DoorKind::vld_normal | DoorKind::vld_close => {
                            door.sector.specialdata = None;
                            unsafe {
                                door.sector.specialdata = None;
                                door.thinker.as_mut().set_action(ActionF::Remove);
                            }
                        }
                        DoorKind::vld_close30ThenOpen => {
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

                if matches!(res, ResultE::PastDest) {
                    match door.kind {
                        DoorKind::vld_blazeRaise | DoorKind::vld_normal => {
                            door.direction = 0; // wait at top
                            door.topcountdown = door.topwait;
                        }
                        DoorKind::vld_close30ThenOpen
                        | DoorKind::vld_blazeOpen
                        | DoorKind::vld_open => {
                            door.sector.specialdata = None;
                            unsafe {
                                door.sector.specialdata = None;
                                door.thinker.as_mut().set_action(ActionF::Remove);
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => warn!("Invalid door direction of {}", door.direction),
        };

        //unsafe { door.thinker.as_mut().set_action(ActionF::Remove) };
        true
    }

    fn set_thinker_ptr(&mut self, ptr: std::ptr::NonNull<Thinker>) {
        self.thinker = ptr;
    }

    fn thinker(&self) -> NonNull<Thinker> {
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
            thinker: NonNull::dangling(),
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
            DoorKind::vld_normal | DoorKind::vld_open => {
                door.topheight = top;
                door.topheight -= 4.0;
                door.direction = 1;
                if door.topheight != sec.ceilingheight {
                    // TODO: S_StartSound(&door->sector->soundorg, sfx_doropn);
                }
            }
            DoorKind::vld_blazeRaise | DoorKind::vld_blazeOpen => {
                door.topheight = top;
                door.topheight -= 4.0;
                door.direction = 1;
                door.speed *= 4.0;
                if door.topheight != sec.ceilingheight {
                    // TODO: S_StartSound(&door->sector->soundorg, sfx_bdopn);
                }
            }
            DoorKind::vld_blazeClose => {
                door.topheight = top;
                door.topheight -= 4.0;
                door.direction = -1;
                door.speed *= 4.0;
                // TODO: S_StartSound(&door->sector->soundorg, sfx_bdcls);
            }
            DoorKind::vld_close30ThenOpen => {
                door.topheight = sec.ceilingheight;
                door.direction = -1;
                // TODO: S_StartSound(&door->sector->soundorg, sfx_dorcls);
            }
            DoorKind::vld_close => {
                door.topheight = top;
                door.topheight -= 4.0;
                door.direction = -1;
                // TODO: S_StartSound(&door->sector->soundorg, sfx_dorcls);
            }
            _ => {}
        }

        let thinker = MapObject::create_thinker(
            ThinkerType::VDoor(door),
            ActionF::Action1(VerticalDoor::think),
        );

        if let Some(mut ptr) = level.thinkers.push::<VerticalDoor>(thinker) {
            unsafe {
                ptr.as_mut()
                    .obj_mut()
                    .bad_mut::<VerticalDoor>()
                    .set_thinker_ptr(ptr);

                sec.specialdata = Some(ptr);
                dbg!("here");
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
    if let Some(mut data) = sec.specialdata {
        // TODO:
        let mut door = unsafe { data.as_mut().obj_mut().bad_mut::<VerticalDoor>() };
        match line.special {
            1 | 26 | 27 | 28 | 117 => {
                if door.direction == -1 {
                    door.direction = 1; // go back up
                } else {
                    if thing.player.is_none() {
                        return; // bad guys never close doors
                    }

                    if matches!(door.thinker_ref().obj_ref(), ThinkerType::VDoor(_)) {
                        door.direction = -1;
                    } else if matches!(door.thinker_ref().obj_ref(), ThinkerType::VDoor(_)) { // TODO: PLATFORM
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
        thinker: NonNull::dangling(),
        sector: sec.clone(),
        kind: DoorKind::vld_normal,
        topheight: 0.0,
        speed: VDOORSPEED,
        direction: 1,
        topwait: VDOORWAIT,
        topcountdown: 0,
    };

    match line.special {
        1 | 26 | 27 | 28 => door.kind = DoorKind::vld_normal,
        31 | 32 | 33 | 34 => {
            door.kind = DoorKind::vld_open;
            line.special = 0;
        }
        117 => {
            door.kind = DoorKind::vld_blazeRaise;
            door.speed = VDOOR * 2.0;
        }
        118 => {
            door.kind = DoorKind::vld_blazeOpen;
            line.special = 0;
            door.speed = VDOOR * 2.0;
        }
        _ => {}
    }

    door.topheight = find_lowest_ceiling_surrounding(sec.clone());
    door.topheight -= 4.0;

    debug!("Activated door: {door:?}");
    let thinker = MapObject::create_thinker(
        ThinkerType::VDoor(door),
        ActionF::Action1(VerticalDoor::think),
    );

    if let Some(mut ptr) = level.thinkers.push::<VerticalDoor>(thinker) {
        unsafe {
            ptr.as_mut()
                .obj_mut()
                .bad_mut::<VerticalDoor>()
                .set_thinker_ptr(ptr);

            sec.specialdata = Some(ptr);
        }
    }
}
