use log::{debug, warn};
use std::ptr::NonNull;

use crate::d_thinker::{ActionF, Think, Thinker, ThinkerType};
use crate::level_data::level::Level;
use crate::p_spec::{find_lowest_ceiling_surrounding, DoorKind};
use crate::{
    doom_def::Card, level_data::map_defs::LineDef, p_map_object::MapObject, p_spec::VerticalDoor,
    DPtr,
};

const VDOOR: f32 = 2.0;

pub fn ev_vertical_door(mut line: DPtr<LineDef>, thing: &MapObject, level: &Level) {
    if let Some(player) = thing.player {
        let player = unsafe { player.as_ref() };
        match line.special {
            26 | 32 => {
                if !player.cards[Card::it_bluecard as usize]
                    && !player.cards[Card::it_blueskull as usize]
                {
                    // TODO: player->message = DEH_String(PD_BLUEK);
                    // TODO: S_StartSound(NULL,sfx_oof);
                    println!("Ooof!");
                    return;
                }
            }
            27 | 34 => {
                if !player.cards[Card::it_yellowcard as usize]
                    && !player.cards[Card::it_yellowskull as usize]
                {
                    // TODO: player->message = DEH_String(PD_YELLOWK);
                    // TODO: S_StartSound(NULL,sfx_oof);
                    println!("Ooof!");
                    return;
                }
            }
            28 | 33 => {
                if !player.cards[Card::it_redcard as usize]
                    && !player.cards[Card::it_redskull as usize]
                {
                    // TODO: player->message = DEH_String(PD_REDK);
                    // TODO: S_StartSound(NULL,sfx_oof);
                    println!("Ooof!");
                    return;
                }
            }
            _ => {
                // Ignore
            }
        }
    }

    // TODO: if the sector has an active thinker, use it

    // new door thinker
    let mut door = VerticalDoor {
        thinker: NonNull::dangling(),
        sector: line.front_sidedef.sector.clone(),
        kind: DoorKind::vld_normal,
        topheight: 0.0,
        speed: 1.0,
        direction: 1,
        topwait: 1,
        topcountdown: 3,
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

    door.topheight = find_lowest_ceiling_surrounding(line.frontsector.clone());
    door.topheight -= 4.0;

    debug!("Activated door: {door:?}");
    let thinker = MapObject::create_thinker(
        ThinkerType::VDoor(door),
        ActionF::Action1(VerticalDoor::think),
    );

    if let Some(mut ptr) = level.add_thinker::<VerticalDoor>(thinker) {
        unsafe {
            ptr.as_mut()
                .object()
                .bad_mut::<VerticalDoor>()
                .set_thinker_ptr(ptr);
        }
    }
}

impl Think for VerticalDoor {
    fn think(object: &mut ThinkerType, _level: &mut Level) -> bool {
        let door = object.bad_mut::<VerticalDoor>();
        dbg!(&door);

        match door.direction {
            0 => {
                door.topcountdown -= 1;
                if door.topcountdown == 0 {
                    match door.kind {
                        DoorKind::vld_normal => {
                            debug!(
                                "Door for sector {:?} should go down",
                                door.sector.p.as_ptr()
                            );
                            door.direction = -1;
                        }
                        _ => {
                            warn!("Invalid door kind: {:?}", door.kind);
                        }
                    }
                }
            }
            1 => {
                // TODO: actually raise with T_MovePlane
                debug!("Raise door for sector {:?}", door.sector.p.as_ptr());
            }
            _ => warn!("Invalid door direction of {}", door.direction),
        };

        unsafe { door.thinker.as_mut().set_action(ActionF::None) };
        true
    }

    fn set_thinker_ptr(&mut self, ptr: std::ptr::NonNull<Thinker>) {
        self.thinker = ptr;
    }

    fn thinker_ref(&self) -> &Thinker {
        unsafe { self.thinker.as_ref() }
    }

    fn thinker_mut(&mut self) -> &mut Thinker {
        unsafe { self.thinker.as_mut() }
    }
}
