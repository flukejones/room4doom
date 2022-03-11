//! Doom source name `p_switch`

use log::{debug, error, warn};

use super::{
    ceiling::{ev_do_ceiling, CeilingKind},
    doors::{ev_do_door, ev_vertical_door, DoorKind},
    floor::{ev_build_stairs, ev_do_floor, FloorKind, StairKind},
    lights::ev_turn_light_on,
    map_object::MapObject,
    platforms::{ev_do_platform, PlatKind},
};

use crate::{flags::LineDefFlags, level_data::map_defs::LineDef, DPtr, textures::Button};

// P_ChangeSwitchTexture(line, 0);, 0 = switch, 1 = button

/// Doom function name `P_StartButton`
pub fn start_button(line: DPtr<LineDef>, button_list: &mut [Button]) {

}

/// Doom function name `P_ChangeSwitchTexture`
pub fn change_switch_texture(line: DPtr<LineDef>, use_again: bool, switch_list: &[usize], button_list: &mut [Button]) {

}

/// P_UseSpecialLine
/// Called when a thing uses a special line.
/// Only the front sides of lines are usable.
pub fn p_use_special_line(_side: i32, line: DPtr<LineDef>, thing: &MapObject) -> bool {
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
            if ev_do_door(line, DoorKind::Normal, level) {
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        50 => {
            debug!("line-switch: vld_close door!");
            if ev_do_door(line, DoorKind::Close, level) {
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        103 => {
            debug!("line-switch: vld_open door!");
            if ev_do_door(line, DoorKind::Open, level) {
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        111 => {
            debug!("line-switch: vld_blazeRaise door!");
            if ev_do_door(line, DoorKind::BlazeRaise, level) {
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        112 => {
            debug!("line-switch: vld_blazeOpen door!");
            if ev_do_door(line, DoorKind::BlazeOpen, level) {
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        113 => {
            debug!("line-switch: vld_blazeClose door!");
            if ev_do_door(line, DoorKind::BlazeClose, level) {
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        42 => {
            debug!("line-switch: vld_close door!");
            if ev_do_door(line, DoorKind::Close, level) {
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        61 => {
            debug!("line-switch: vld_open door!");
            if ev_do_door(line, DoorKind::Open, level) {
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        63 => {
            debug!("line-switch: vld_normal door!");
            if ev_do_door(line, DoorKind::Normal, level) {
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        114 => {
            debug!("line-switch: vld_blazeRaise door!");
            if ev_do_door(line, DoorKind::BlazeRaise, level) {
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        115 => {
            debug!("line-switch: vld_blazeOpen door!");
            if ev_do_door(line, DoorKind::BlazeOpen, level) {
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        116 => {
            debug!("line-switch: vld_blazeClose door!");
            if ev_do_door(line, DoorKind::BlazeClose, level) {
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        14 => {
            debug!("line-switch: raiseAndChange platform!");
            if ev_do_platform(line, PlatKind::RaiseAndChange,32, level){
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        15 => {
            debug!("line-switch: raiseAndChange platform!");
            if ev_do_platform(line, PlatKind::RaiseAndChange,24, level){
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        20 => {
            debug!("line-switch: raiseToNearestAndChange platform!");
            if ev_do_platform(line, PlatKind::RaiseToNearestAndChange,0, level){
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        21 => {
            debug!("line-switch: downWaitUpStay platform!");
            if ev_do_platform(line, PlatKind::DownWaitUpStay,0, level){
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        62 => {
            debug!("line-switch: downWaitUpStay platform!");
            if ev_do_platform(line, PlatKind::DownWaitUpStay, 1, level){
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        66 => {
            debug!("line-switch: raiseAndChange platform!");
            if ev_do_platform(line, PlatKind::RaiseAndChange, 24, level){
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        67 => {
            debug!("line-switch: raiseAndChange platform!");
            if ev_do_platform(line, PlatKind::RaiseAndChange, 32, level){
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        68 => {
            debug!("line-switch: raiseToNearestAndChange platform!");
            if ev_do_platform(line, PlatKind::RaiseToNearestAndChange, 0, level){
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        122 => {
            debug!("line-switch: blazeDWUS platform!");
            if ev_do_platform(line, PlatKind::BlazeDWUS, 0, level){
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        123 => {
            debug!("line-switch: blazeDWUS platform!");
            if ev_do_platform(line, PlatKind::BlazeDWUS, 0, level){
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        18 => {
            debug!("line-switch: raiseFloorToNearest floor!");
            if ev_do_floor(line, FloorKind::RaiseFloorToNearest, level){
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        23 => {
            debug!("line-switch: lowerFloorToLowest floor!");
            if ev_do_floor(line, FloorKind::LowerFloorToLowest, level){
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        71 => {
            debug!("line-switch: turboLower floor!");
            if ev_do_floor(line, FloorKind::TurboLower, level){
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        55 => {
            debug!("line-switch: raiseFloorCrush floor!");
            if ev_do_floor(line, FloorKind::RaiseFloorCrush, level){
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        101 => {
            debug!("line-switch: raiseFloor floor!");
            if ev_do_floor(line, FloorKind::RaiseFloor, level){
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        102 => {
            debug!("line-switch: lowerFloor floor!");
            if ev_do_floor(line, FloorKind::LowerFloor, level){
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        131 => {
            debug!("line-switch: lowerFloor floor!");
            if ev_do_floor(line, FloorKind::RaiseFloorTurbo, level){
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        140 => {
            debug!("line-switch: lowerFloor floor!");
            if ev_do_floor(line, FloorKind::RaiseFloor512, level){
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        45 => {
            debug!("line-switch: lowerFloor floor!");
            if ev_do_floor(line, FloorKind::LowerFloor, level){
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        60 => {
            debug!("line-switch: lowerFloorToLowest floor!");
            if ev_do_floor(line, FloorKind::LowerFloorToLowest, level){
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        64 => {
            debug!("line-switch: raiseFloor floor!");
            if ev_do_floor(line, FloorKind::RaiseFloor, level){
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        65 => {
            debug!("line-switch: raiseFloorCrush floor!");
            if ev_do_floor(line, FloorKind::RaiseFloorCrush, level){
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        69 => {
            debug!("line-switch: raiseFloorToNearest floor!");
            if ev_do_floor(line, FloorKind::RaiseFloorToNearest, level){
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        70 => {
            debug!("line-switch: turboLower floor!");
            if ev_do_floor(line, FloorKind::TurboLower, level){
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        132 => {
            debug!("line-switch: raiseFloorTurbo floor!");
            if ev_do_floor(line, FloorKind::RaiseFloorTurbo, level){
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        41 => {
            debug!("line-switch: lowerToFloor ceiling!");
            if ev_do_ceiling(line, CeilingKind::LowerToFloor, level){
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        49 => {
            debug!("line-switch: crushAndRaise ceiling!");
            if ev_do_ceiling(line, CeilingKind::CrushAndRaise, level){
                // TODO: P_ChangeSwitchTexture(line, 0);
            }
        }
        43 => {
            debug!("line-switch: lowerToFloor ceiling!");
            if ev_do_ceiling(line, CeilingKind::LowerToFloor, level){
                // TODO: P_ChangeSwitchTexture(line, 1);
            }
        }
        138 => {
            debug!("line-switch: turn light on!");
            ev_turn_light_on(line, 255, level);
            // TODO: P_ChangeSwitchTexture(line, 1);
        }
        139 => {
            debug!("line-switch: turn light off!");
            ev_turn_light_on(line, 35, level);
            // TODO: P_ChangeSwitchTexture(line, 1);
        }
        7 => {
            debug!(
                "line-switch #{}: build 8 stair steps",
                line.special
            );
            ev_build_stairs(line.clone(), StairKind::Build8, level);
            // TODO: P_ChangeSwitchTexture(line, 0);
        }
        127 => {
            debug!(
                "line-switch #{}: build 16 stair steps turbo",
                line.special
            );
            ev_build_stairs(line.clone(), StairKind::Turbo16, level);
            // TODO: P_ChangeSwitchTexture(line, 0);
        }
        9 => {
            error!("line-special #{}: EV_DoDonut not implemented", line.special);
            // TODO: P_ChangeSwitchTexture(line, 0);
        }
        133 | 135 | 137 => {
            error!("line-special #{}: EV_DoLockedDoor not implemented", line.special);
            // TODO: P_ChangeSwitchTexture(line, 0);
        }
        99 | 134 | 136 => {
            error!("line-special #{}: EV_DoLockedDoor not implemented", line.special);
            // TODO: P_ChangeSwitchTexture(line, 1);
        }
        _ => {
            warn!("Invalid or unimplemented line switch: {}", line.special);
        }
    }
    false
}
