use crate::{
    flags::LineDefFlags, level_data::{map_defs::LineDef, level::Level}, p_doors::ev_vertical_door,
    p_map_object::MapObject, DPtr,
};

/// P_UseSpecialLine
/// Called when a thing uses a special line.
/// Only the front sides of lines are usable.
pub fn p_use_special_line(side: i32, line: DPtr<LineDef>, thing: &MapObject, level: &Level) -> bool {
    //  Switches that other things can activate
    if thing.player.is_none() {
        // never open secret doors
        if (line.flags as u32) & LineDefFlags::Secret as u32 != 0 {
            return false;
        }

        if let 1    // MANUAL DOOR RAISE
            | 32    // MANUAL BLUE
            | 33    // MANUAL RED
            | 34    // MANUAL YELLOW
            = line.special {
            return false;
        }
    }

    if let 1        // Vertical Door
          | 26      // Blue Door/Locked
          | 27      // Yellow Door /Locked
          | 28      // Red Door /Locked

          | 31      // Manual door open
          | 32      // Blue locked door open
          | 33      // Red locked door open
          | 34      // Yellow locked door open

          | 117     // Blazing door raise
          | 118     // Blazing door open
          = line.special {
        ev_vertical_door(line, thing, level);
        println!("*hydralic sounds*");
    }
    false
}
