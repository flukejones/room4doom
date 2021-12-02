use crate::{DPtr, flags::LineDefFlags, level_data::map_defs::LineDef, p_map_object::MapObject};

pub fn use_special_line(side: i32, line: DPtr<LineDef>, thing: &mut MapObject) -> bool {
    //  Switches that other things can activate
    if thing.player.is_none() {
        // never open secret doors
        if (line.flags as u32) & LineDefFlags::Secret as u32 != 0 {
            return false;
        }

        if !matches!(
            line.special,
            1     // MANUAL DOOR RAISE
            | 32 // MANUAL BLUE
            | 33  // MANUAL RED
            | 34 // MANUAL YELLOW
        ) {
            return false;
        }
    }

    false
}
