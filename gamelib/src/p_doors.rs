use crate::d_thinker::Think;
use crate::level_data::level::Level;
use crate::{
    doom_def::Card, level_data::map_defs::LineDef, p_map_object::MapObject, p_spec::VerticalDoor,
    DPtr,
};

pub fn ev_vertical_door(line: DPtr<LineDef>, thing: &MapObject) {
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
}

impl Think for VerticalDoor {
    fn think(object: &mut crate::d_thinker::ThinkerType, level: &mut Level) -> bool {
        todo!()
    }

    fn set_thinker_ptr(&mut self, ptr: std::ptr::NonNull<crate::d_thinker::Thinker>) {
        todo!()
    }

    fn thinker_ref(&self) -> &crate::d_thinker::Thinker {
        todo!()
    }

    fn thinker_mut(&mut self) -> &mut crate::d_thinker::Thinker {
        todo!()
    }
}
