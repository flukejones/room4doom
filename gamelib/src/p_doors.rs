use std::ptr::NonNull;

use crate::d_thinker::{ActionF, Think, Thinker, ThinkerType};
use crate::level_data::level::Level;
use crate::p_spec::DoorKind;
use crate::{
    doom_def::Card, level_data::map_defs::LineDef, p_map_object::MapObject, p_spec::VerticalDoor,
    DPtr,
};

pub fn ev_vertical_door(line: DPtr<LineDef>, thing: &MapObject, level: &Level) {
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
    let door = VerticalDoor {
        thinker: NonNull::dangling(),
        sector: line.front_sidedef.sector.clone(),
        kind: DoorKind::vld_normal,
        topheight: 0.0,
        speed: 1.0,
        direction: 1,
        topwait: 1,
        topcountdown: 3,
    };

    /*
    switch (line->special)
        {
        case 1:
        case 26:
        case 27:
        case 28:
            door->type = normal;
            break;

        case 31:
        case 32:
        case 33:
        case 34:
            door->type = open;
            line->special = 0;
            break;

        case 117: // blazing door raise
            door->type = blazeRaise;
            door->speed = VDOORSPEED * 4;
            break;
        case 118: // blazing door open
            door->type = blazeOpen;
            line->special = 0;
            door->speed = VDOORSPEED * 4;
            break;
        }
        // find the top and bottom of the movement range
        door->topheight = P_FindLowestCeilingSurrounding(sec);
        door->topheight -= 4 * FRACUNIT;
        */

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
    fn think(object: &mut ThinkerType, level: &mut Level) -> bool {
        let door = object.bad_mut::<VerticalDoor>();
        dbg!(door.kind);
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
