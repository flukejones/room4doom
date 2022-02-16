use log::{debug, warn};

use crate::{
    flags::LineDefFlags,
    level_data::map_defs::LineDef,
    p_ceiling::ev_do_ceiling,
    p_doors::{ev_do_door, ev_vertical_door},
    p_floor::ev_do_floor,
    p_map_object::MapObject,
    p_plats::ev_do_platform,
    p_spec::{CeilingKind, DoorKind, FloorKind, PlatKind},
    DPtr,
};

// P_ChangeSwitchTexture(line, 0);, 0 = switch, 1 = button

/// P_UseSpecialLine
/// Called when a thing uses a special line.
/// Only the front sides of lines are usable.
pub fn p_use_special_line(side: i32, line: DPtr<LineDef>, thing: &MapObject) -> bool {
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

    if thing.level.is_null() {
        panic!("Thing had a bad level pointer");
    }
    let level = unsafe { &mut *thing.level };
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
        51 => {
            // TODO: P_ChangeSwitchTexture(line, 0);
            level.do_secret_exit_level();
        }
        29 => {
            debug!("line-switch: vld_normal door!");
            if ev_do_door(line, DoorKind::vld_normal, level) {
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        50 => {
            debug!("line-switch: vld_close door!");
            if ev_do_door(line, DoorKind::vld_close, level) {
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        103 => {
            debug!("line-switch: vld_open door!");
            if ev_do_door(line, DoorKind::vld_open, level) {
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        111 => {
            debug!("line-switch: vld_blazeRaise door!");
            if ev_do_door(line, DoorKind::vld_blazeRaise, level) {
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        112 => {
            debug!("line-switch: vld_blazeOpen door!");
            if ev_do_door(line, DoorKind::vld_blazeOpen, level) {
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        113 => {
            debug!("line-switch: vld_blazeClose door!");
            if ev_do_door(line, DoorKind::vld_blazeClose, level) {
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        42 => {
            debug!("line-switch: vld_close door!");
            if ev_do_door(line, DoorKind::vld_close, level) {
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        61 => {
            debug!("line-switch: vld_open door!");
            if ev_do_door(line, DoorKind::vld_open, level) {
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        63 => {
            debug!("line-switch: vld_normal door!");
            if ev_do_door(line, DoorKind::vld_normal, level) {
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        114 => {
            debug!("line-switch: vld_blazeRaise door!");
            if ev_do_door(line, DoorKind::vld_blazeRaise, level) {
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        115 => {
            debug!("line-switch: vld_blazeOpen door!");
            if ev_do_door(line, DoorKind::vld_blazeOpen, level) {
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        116 => {
            debug!("line-switch: vld_blazeClose door!");
            if ev_do_door(line, DoorKind::vld_blazeClose, level) {
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        14 => {
            debug!("line-switch: raiseAndChange platform!");
            if ev_do_platform(line, PlatKind::raiseAndChange,32, level){
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        15 => {
            debug!("line-switch: raiseAndChange platform!");
            if ev_do_platform(line, PlatKind::raiseAndChange,24, level){
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        20 => {
            debug!("line-switch: raiseToNearestAndChange platform!");
            if ev_do_platform(line, PlatKind::raiseToNearestAndChange,0, level){
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        21 => {
            debug!("line-switch: downWaitUpStay platform!");
            if ev_do_platform(line, PlatKind::downWaitUpStay,0, level){
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        62 => {
            debug!("line-switch: downWaitUpStay platform!");
            if ev_do_platform(line, PlatKind::downWaitUpStay, 1, level){
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        66 => {
            debug!("line-switch: raiseAndChange platform!");
            if ev_do_platform(line, PlatKind::raiseAndChange, 24, level){
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        67 => {
            debug!("line-switch: raiseAndChange platform!");
            if ev_do_platform(line, PlatKind::raiseAndChange, 32, level){
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        68 => {
            debug!("line-switch: raiseToNearestAndChange platform!");
            if ev_do_platform(line, PlatKind::raiseToNearestAndChange, 0, level){
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        122 => {
            debug!("line-switch: blazeDWUS platform!");
            if ev_do_platform(line, PlatKind::blazeDWUS, 0, level){
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        123 => {
            debug!("line-switch: blazeDWUS platform!");
            if ev_do_platform(line, PlatKind::blazeDWUS, 0, level){
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        18 => {
            debug!("line-switch: raiseFloorToNearest floor!");
            if ev_do_floor(line, FloorKind::raiseFloorToNearest, level){
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        23 => {
            debug!("line-switch: lowerFloorToLowest floor!");
            if ev_do_floor(line, FloorKind::lowerFloorToLowest, level){
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        71 => {
            debug!("line-switch: turboLower floor!");
            if ev_do_floor(line, FloorKind::turboLower, level){
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        55 => {
            debug!("line-switch: raiseFloorCrush floor!");
            if ev_do_floor(line, FloorKind::raiseFloorCrush, level){
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        101 => {
            debug!("line-switch: raiseFloor floor!");
            if ev_do_floor(line, FloorKind::raiseFloor, level){
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        102 => {
            debug!("line-switch: lowerFloor floor!");
            if ev_do_floor(line, FloorKind::lowerFloor, level){
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        131 => {
            debug!("line-switch: lowerFloor floor!");
            if ev_do_floor(line, FloorKind::raiseFloorTurbo, level){
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        140 => {
            debug!("line-switch: lowerFloor floor!");
            if ev_do_floor(line, FloorKind::raiseFloor512, level){
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        45 => {
            debug!("line-switch: lowerFloor floor!");
            if ev_do_floor(line, FloorKind::lowerFloor, level){
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        60 => {
            debug!("line-switch: lowerFloorToLowest floor!");
            if ev_do_floor(line, FloorKind::lowerFloorToLowest, level){
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        64 => {
            debug!("line-switch: raiseFloor floor!");
            if ev_do_floor(line, FloorKind::raiseFloor, level){
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        65 => {
            debug!("line-switch: raiseFloorCrush floor!");
            if ev_do_floor(line, FloorKind::raiseFloorCrush, level){
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        69 => {
            debug!("line-switch: raiseFloorToNearest floor!");
            if ev_do_floor(line, FloorKind::raiseFloorToNearest, level){
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        70 => {
            debug!("line-switch: turboLower floor!");
            if ev_do_floor(line, FloorKind::turboLower, level){
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        132 => {
            debug!("line-switch: raiseFloorTurbo floor!");
            if ev_do_floor(line, FloorKind::raiseFloorTurbo, level){
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        41 => {
            debug!("line-switch: lowerToFloor ceiling!");
            if ev_do_ceiling(line, CeilingKind::lowerToFloor, level){
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        49 => {
            debug!("line-switch: crushAndRaise ceiling!");
            if ev_do_ceiling(line, CeilingKind::crushAndRaise, level){
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        43 => {
            debug!("line-switch: lowerToFloor ceiling!");
            if ev_do_ceiling(line, CeilingKind::lowerToFloor, level){
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        _ => {
            warn!("Invalid or unimplemented line switch: {}", line.special);
        }
    }
    false
}
