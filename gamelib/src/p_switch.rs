use log::{debug, warn};

use crate::{
    flags::LineDefFlags,
    level_data::{level::Level, map_defs::LineDef},
    p_doors::{ev_do_door, ev_vertical_door},
    p_map_object::MapObject,
    p_plats::ev_do_platform,
    DPtr,
};

/// P_UseSpecialLine
/// Called when a thing uses a special line.
/// Only the front sides of lines are usable.
pub fn p_use_special_line(
    side: i32,
    line: DPtr<LineDef>,
    thing: &MapObject,
    level: &mut Level,
) -> bool {
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
            // Nothing
        } else {
            return false;
        }
    }

    match line.special {
        1        // Vertical Door
        | 26      // Blue Door/Locked
        | 27      // Yellow Door /Locked
        | 28      // Red Door /Locked

        | 31      // Manual door open
        | 32      // Blue locked door open
        | 33      // Red locked door open
        | 34      // Yellow locked door open

        | 117     // Blazing door raise
        | 118     // Blazing door open
        => {
            ev_vertical_door(line, thing, level);
            println!("*hydralic sounds*");
        }
        7 => {
            // TODO: EV_BuildStairs
            todo!("if (EV_BuildStairs(line, build8))
			P_ChangeSwitchTexture(line, 0);");
        }
        9 => {
            // TODO: EV_DoDonut
            todo!("if (EV_DoDonut(line))
			P_ChangeSwitchTexture(line, 0);");
        }
        11 => {
            // TODO: P_ChangeSwitchTexture(line, 0);
            level.do_exit_level();
        }
        29 => {
            if ev_do_door(line, crate::p_spec::DoorKind::vld_normal, level) {
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        50 => {
            if ev_do_door(line, crate::p_spec::DoorKind::vld_close, level) {
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        103 => {
            if ev_do_door(line, crate::p_spec::DoorKind::vld_open, level) {
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        111 => {
            if ev_do_door(line, crate::p_spec::DoorKind::vld_blazeRaise, level) {
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        112 => {
            if ev_do_door(line, crate::p_spec::DoorKind::vld_blazeOpen, level) {
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        113 => {
            if ev_do_door(line, crate::p_spec::DoorKind::vld_blazeClose, level) {
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        42 => {
            if ev_do_door(line, crate::p_spec::DoorKind::vld_close, level) {
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        61 => {
            if ev_do_door(line, crate::p_spec::DoorKind::vld_open, level) {
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        63 => {
            if ev_do_door(line, crate::p_spec::DoorKind::vld_normal, level) {
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        114 => {
            if ev_do_door(line, crate::p_spec::DoorKind::vld_blazeRaise, level) {
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        115 => {
            if ev_do_door(line, crate::p_spec::DoorKind::vld_blazeOpen, level) {
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        116 => {
            if ev_do_door(line, crate::p_spec::DoorKind::vld_blazeClose, level) {
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        20 => {
            debug!("Raise platform!");
            if ev_do_platform(line, crate::p_spec::PlatKind::raiseToNearestAndChange, level){
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        _ => {
            warn!("Invalid or unimplemented line special: {}", line.special);
        }
    }
    false
}
