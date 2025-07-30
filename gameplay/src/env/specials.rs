//! Implements special effects:
//! Texture animation, height or lighting changes according to adjacent sectors,
//! respective utility functions. Line Tag handling. Line and Sector triggers.
//!
//! Doom source name `p_spec`

use crate::doom_def::{ONCEILINGZ, ONFLOORZ};
use crate::env::ceiling::{CeilKind, ev_do_ceiling};
use crate::env::doors::{DoorKind, ev_do_door};
use crate::env::floor::{FloorKind, StairKind, ev_build_stairs, ev_do_floor};
use crate::env::lights::{
    FASTDARK, FireFlicker, Glow, LightFlash, SLOWDARK, StrobeFlash, ev_start_light_strobing,
    ev_turn_light_on, ev_turn_tag_lights_off,
};
use crate::env::platforms::{PlatKind, ev_do_platform, ev_stop_platform};
use crate::env::switch::{change_switch_texture, start_sector_sound};
use crate::env::teleport::teleport;
use crate::info::{MOBJINFO, MapObjKind};
use crate::level::Level;
use crate::level::flags::LineDefFlags;
use crate::level::map_defs::{LineDef, Sector};
use crate::pic::ButtonWhere;
use crate::thing::MapObject;
use crate::{Angle, BSP3D, MapObjFlag, MapPtr, MovementType, PicData, TICRATE};
use glam::Vec2;
use log::{debug, error, trace};
use math::circle_line_collide;
use sound_traits::SfxName;
use std::ptr;

pub fn get_next_sector(line: MapPtr<LineDef>, sector: MapPtr<Sector>) -> Option<MapPtr<Sector>> {
    if line.flags & LineDefFlags::TwoSided as u32 == 0 {
        return None;
    }

    if ptr::eq(line.frontsector.as_ref(), sector.as_ref()) {
        return line.backsector.clone();
    }

    Some(line.frontsector.clone())
}

/// P_FindMinSurroundingLight
pub fn find_min_light_surrounding(sec: MapPtr<Sector>, max: usize) -> usize {
    let mut min = max;
    for line in &sec.lines {
        if let Some(other) = get_next_sector(line.clone(), sec.clone()) {
            if other.lightlevel < min {
                min = other.lightlevel;
            }
        }
    }
    trace!("find_min_light_surrounding: {min}");
    min
}

pub fn find_max_light_surrounding(sec: MapPtr<Sector>, mut max: usize) -> usize {
    for line in &sec.lines {
        if let Some(other) = get_next_sector(line.clone(), sec.clone()) {
            if other.lightlevel > max {
                max = other.lightlevel;
            }
        }
    }
    debug!("find_max_light_surrounding: {max}");
    max
}

/// P_FindLowestCeilingSurrounding
pub fn find_lowest_ceiling_surrounding(sec: MapPtr<Sector>) -> f32 {
    let mut height = f32::MAX;
    for line in &sec.lines {
        if let Some(other) = get_next_sector(line.clone(), sec.clone()) {
            if other.ceilingheight < height {
                height = other.ceilingheight;
            }
        }
    }
    debug!("find_lowest_ceiling_surrounding: {height}");
    height
}

/// P_FindHighestCeilingSurrounding
pub fn find_highest_ceiling_surrounding(sec: MapPtr<Sector>) -> f32 {
    let mut height = 0.0;
    for line in &sec.lines {
        if let Some(other) = get_next_sector(line.clone(), sec.clone()) {
            if other.ceilingheight > height {
                height = other.ceilingheight;
            }
        }
    }
    debug!("find_highest_ceiling_surrounding: {height}");
    height
}

/// P_FindLowestFloorSurrounding
pub fn find_lowest_floor_surrounding(sec: MapPtr<Sector>) -> f32 {
    let mut floor = sec.floorheight;
    for line in &sec.lines {
        if let Some(other) = get_next_sector(line.clone(), sec.clone()) {
            if other.floorheight < floor {
                floor = other.floorheight;
            }
        }
    }
    debug!("find_lowest_floor_surrounding: {floor}");
    floor
}

/// P_FindHighestFloorSurrounding
pub fn find_highest_floor_surrounding(sec: MapPtr<Sector>) -> f32 {
    let mut floor = f32::MIN;
    for line in &sec.lines {
        if let Some(other) = get_next_sector(line.clone(), sec.clone()) {
            if other.floorheight > floor {
                floor = other.floorheight;
            }
        }
    }
    debug!("find_highest_floor_surrounding: {floor}");
    floor
}

/// P_FindNextHighestFloor
pub fn find_next_highest_floor(sec: MapPtr<Sector>, current: f32) -> f32 {
    let mut height = current;
    let mut height_list = Vec::new();

    for line in &sec.lines {
        if let Some(other) = get_next_sector(line.clone(), sec.clone()) {
            if other.floorheight > height {
                height = other.floorheight;
            }
            height_list.push(other.floorheight);
        }
    }

    if height_list.is_empty() {
        return current;
    }
    let mut min = height_list[0];

    for height in height_list {
        if height < min {
            min = height;
        }
    }

    min
}

/// P_ChangeSector
fn change_sector(mut sector: MapPtr<Sector>, crunch: bool) -> bool {
    let mut no_fit = false;
    let valid = sector.validcount + 1;
    // The call to pit_change_sector relies on the mobj doing height_clip() which
    // initiates the position check on itself
    sector.run_mut_func_on_thinglist(|thing| {
        trace!("Thing type {:?} is in affected sector", thing.kind);
        thing.pit_change_sector(&mut no_fit, crunch)
    });
    sector.validcount = valid;

    // Causes floating bloodsplat?
    for line in sector.lines.iter() {
        if let Some(mut next) = get_next_sector(line.clone(), sector.clone()) {
            if next.validcount == valid {
                continue;
            }
            next.run_mut_func_on_thinglist(|thing| {
                let mut hit = false;
                if circle_line_collide(thing.xy, thing.radius, line.v1, line.v2) {
                    trace!(
                        "Thing type {:?} is in affected neightbouring sector",
                        thing.kind
                    );
                    hit = thing.pit_change_sector(&mut no_fit, crunch);
                }
                if !hit {
                    thing.pit_change_sector(&mut no_fit, crunch);
                }
                true
            });
            next.validcount = valid;
        }
    }

    no_fit
}

/// The result of raising a plane. `PastDest` = stop, `Crushed` = should crush
/// all in sector
#[derive(Debug, Clone, Copy)]
pub enum PlaneResult {
    Ok,
    Crushed,
    PastDest,
}

pub fn move_plane(
    mut sector: MapPtr<Sector>,
    speed: f32,
    dest: f32,
    crush: bool,
    floor_or_ceiling: i32,
    direction: i32,
    bsp3d: &mut BSP3D,
) -> PlaneResult {
    let sector_num = sector.num as usize;
    match floor_or_ceiling {
        0 => {
            // FLOOR
            match direction {
                -1 => {
                    // DOWN
                    trace!(
                        "move_plane: floor: down: {} to {} at speed {}",
                        sector.floorheight, dest, speed
                    );
                    if sector.floorheight - speed < dest {
                        let last_pos = sector.floorheight;
                        sector.floorheight = dest;
                        bsp3d.move_vertices(sector_num, MovementType::Floor, dest);

                        if change_sector(sector.clone(), crush) {
                            sector.floorheight = last_pos;
                            bsp3d.move_vertices(sector_num, MovementType::Floor, last_pos);
                            change_sector(sector, crush);
                        }
                        return PlaneResult::PastDest;
                    } else {
                        // COULD GET CRUSHED
                        let last_pos = sector.floorheight;
                        sector.floorheight -= speed;
                        bsp3d.move_vertices(sector_num, MovementType::Floor, sector.floorheight);
                        if change_sector(sector.clone(), crush) {
                            if crush {
                                return PlaneResult::Crushed;
                            }
                            sector.floorheight = last_pos;
                            bsp3d.move_vertices(sector_num, MovementType::Floor, last_pos);
                            change_sector(sector, crush);
                            return PlaneResult::Crushed;
                        }
                    }
                }
                1 => {
                    // UP
                    trace!(
                        "move_plane: floor: up: {} to {} at speed {}",
                        sector.floorheight, dest, speed
                    );
                    if sector.floorheight + speed > dest {
                        let last_pos = sector.floorheight;
                        sector.floorheight = dest;
                        bsp3d.move_vertices(sector_num, MovementType::Floor, dest);

                        if change_sector(sector.clone(), crush) {
                            sector.floorheight = last_pos;
                            bsp3d.move_vertices(sector_num, MovementType::Floor, last_pos);
                            change_sector(sector, crush);
                        }
                        return PlaneResult::PastDest;
                    } else {
                        let last_pos = sector.floorheight;
                        sector.floorheight += speed;
                        bsp3d.move_vertices(sector_num, MovementType::Floor, sector.floorheight);
                        if change_sector(sector.clone(), crush) {
                            if crush {
                                return PlaneResult::Crushed;
                            }
                            sector.floorheight = last_pos;
                            bsp3d.move_vertices(sector_num, MovementType::Floor, last_pos);
                            change_sector(sector, crush);
                            return PlaneResult::Crushed;
                        }
                    }
                }
                _ => error!("Invalid floor direction: {}", direction),
            }
        }
        1 => {
            // CEILING
            match direction {
                -1 => {
                    // DOWN
                    trace!(
                        "move_plane: ceiling: down: {} to {} at speed {}",
                        sector.ceilingheight, dest, speed
                    );
                    if sector.ceilingheight - speed < dest {
                        let last_pos = sector.ceilingheight;
                        sector.ceilingheight = dest;
                        bsp3d.move_vertices(sector_num, MovementType::Ceiling, dest);

                        if change_sector(sector.clone(), crush) {
                            sector.ceilingheight = last_pos;
                            bsp3d.move_vertices(sector_num, MovementType::Ceiling, last_pos);
                            change_sector(sector.clone(), crush);
                        }
                        return PlaneResult::PastDest;
                    } else {
                        // COULD GET CRUSHED
                        let last_pos = sector.ceilingheight;
                        sector.ceilingheight -= speed;
                        bsp3d.move_vertices(
                            sector_num,
                            MovementType::Ceiling,
                            sector.ceilingheight,
                        );

                        if change_sector(sector.clone(), crush) {
                            if crush {
                                return PlaneResult::Crushed;
                            }
                            sector.ceilingheight = last_pos;
                            bsp3d.move_vertices(sector_num, MovementType::Ceiling, last_pos);
                            change_sector(sector.clone(), crush);
                            return PlaneResult::Crushed;
                        }
                    }
                }
                1 => {
                    // UP
                    trace!(
                        "move_plane: ceiling: up: {} to {} at speed {}",
                        sector.ceilingheight, dest, speed
                    );
                    if sector.ceilingheight + speed >= dest {
                        let last_pos = sector.ceilingheight;
                        sector.ceilingheight = dest;
                        bsp3d.move_vertices(sector_num, MovementType::Ceiling, dest);

                        if change_sector(sector.clone(), crush) {
                            sector.ceilingheight = last_pos;
                            bsp3d.move_vertices(sector_num, MovementType::Ceiling, last_pos);
                            change_sector(sector, crush);
                        }
                        return PlaneResult::PastDest;
                    } else {
                        //let last_pos = sector.ceilingheight;
                        sector.ceilingheight += speed;
                        bsp3d.move_vertices(
                            sector_num,
                            MovementType::Ceiling,
                            sector.ceilingheight,
                        );
                        change_sector(sector, crush);
                    }
                }
                _ => error!("Invalid ceiling direction: {}", direction),
            }
        }
        _ => error!("Invalid floor_or_ceiling: {}", floor_or_ceiling),
    }

    PlaneResult::Ok
}

/// Trigger various actions when a line is crossed which has a non-zero special
/// attached
///
/// Doom function name is `P_CrossSpecialLine`
pub fn cross_special_line(side: usize, mut line: MapPtr<LineDef>, thing: &mut MapObject) {
    let mut ok = false;

    //  Triggers that other things can activate
    if thing.player().is_none() {
        // Things that should NOT trigger specials...
        match thing.kind {
            MapObjKind::MT_ROCKET
            | MapObjKind::MT_PLASMA
            | MapObjKind::MT_BFG
            | MapObjKind::MT_TROOPSHOT
            | MapObjKind::MT_HEADSHOT
            | MapObjKind::MT_BRUISERSHOT => return,
            _ => {}
        }

        if matches!(
            line.special,
            39    // TELEPORT TRIGGER
            | 97  // TELEPORT RETRIGGER
            | 125 // TELEPORT MONSTERONLY TRIGGER
            | 126 // TELEPORT MONSTERONLY RETRIGGER
            | 4   // RAISE DOOR
            | 10  // PLAT DOWN-WAIT-UP-STAY TRIGGER
            | 88 // PLAT DOWN-WAIT-UP-STAY RETRIGGER
        ) {
            ok = true;
        }

        if !ok {
            return;
        }
    }

    if thing.level.is_null() {
        panic!("Thing had a bad level pointer");
    }
    let level = unsafe { &mut *thing.level };
    match line.special {
        2 => {
            debug!("line-special #{}: vld_open door!", line.special);
            ev_do_door(line.clone(), DoorKind::Open, level);
            line.special = 0;
        }
        3 => {
            debug!("line-special #{}: vld_close door!", line.special);
            ev_do_door(line.clone(), DoorKind::Close, level);
            line.special = 0;
        }
        4 => {
            debug!("line-special #{}: vld_normal door!", line.special);
            ev_do_door(line.clone(), DoorKind::Normal, level);
            line.special = 0;
        }
        16 => {
            debug!("line-special #{}: vld_close30ThenOpen door!", line.special);
            ev_do_door(line.clone(), DoorKind::Close30ThenOpen, level);
            line.special = 0;
        }
        108 => {
            debug!("line-special #{}: vld_blazeRaise door!", line.special);
            ev_do_door(line.clone(), DoorKind::BlazeRaise, level);
            line.special = 0;
        }
        109 => {
            debug!("line-special #{}: vld_blazeOpen door!", line.special);
            ev_do_door(line.clone(), DoorKind::BlazeOpen, level);
            line.special = 0;
        }
        110 => {
            debug!("line-special #{}: vld_blazeClose door!", line.special);
            ev_do_door(line.clone(), DoorKind::BlazeClose, level);
            line.special = 0;
        }
        75 => {
            debug!("line-special #{}: vld_close door!", line.special);
            ev_do_door(line.clone(), DoorKind::Close, level);
        }
        76 => {
            debug!("line-special #{}: vld_close30ThenOpen door!", line.special);
            ev_do_door(line.clone(), DoorKind::Close30ThenOpen, level);
        }
        86 => {
            debug!("line-special #{}: vld_open door!", line.special);
            ev_do_door(line.clone(), DoorKind::Open, level);
        }
        90 => {
            debug!("line-special #{}: vld_normal door!", line.special);
            ev_do_door(line.clone(), DoorKind::Normal, level);
        }
        105 => {
            debug!("line-special #{}: vld_blazeRaise door!", line.special);
            ev_do_door(line.clone(), DoorKind::BlazeRaise, level);
        }
        106 => {
            debug!("line-special #{}: vld_blazeOpen door!", line.special);
            ev_do_door(line.clone(), DoorKind::BlazeOpen, level);
        }
        107 => {
            debug!("line-special #{}: vld_blazeClose door!", line.special);
            ev_do_door(line.clone(), DoorKind::BlazeClose, level);
        }

        10 => {
            debug!("line-special #{}: downWaitUpStay platform!", line.special);
            ev_do_platform(line.clone(), PlatKind::DownWaitUpStay, 0, level);
            line.special = 0;
        }
        22 => {
            debug!(
                "line-special #{}: raiseToNearestAndChange platform!",
                line.special
            );
            ev_do_platform(line.clone(), PlatKind::RaiseToNearestAndChange, 0, level);
            line.special = 0;
        }
        53 => {
            debug!("line-special #{}: perpetualRaise platform!", line.special);
            ev_do_platform(line.clone(), PlatKind::PerpetualRaise, 0, level);
            line.special = 0;
        }
        121 => {
            debug!("line-special #{}: blazeDWUS platform!", line.special);
            ev_do_platform(line.clone(), PlatKind::BlazeDWUS, 0, level);
            line.special = 0;
        }
        87 => {
            debug!("line-special #{}: perpetualRaise platform!", line.special);
            ev_do_platform(line.clone(), PlatKind::PerpetualRaise, 0, level);
        }
        88 => {
            debug!("line-special #{}: downWaitUpStay platform!", line.special);
            ev_do_platform(line.clone(), PlatKind::DownWaitUpStay, 0, level);
        }
        95 => {
            debug!(
                "line-special #{}: raiseToNearestAndChange platform!",
                line.special
            );
            ev_do_platform(line.clone(), PlatKind::RaiseToNearestAndChange, 0, level);
        }
        120 => {
            debug!("line-special #{}: blazeDWUS platform!", line.special);
            ev_do_platform(line.clone(), PlatKind::BlazeDWUS, 0, level);
        }
        5 => {
            debug!("line-special #{}: raiseFloor floor!", line.special);
            ev_do_floor(line.clone(), FloorKind::RaiseFloor, level);
            line.special = 0;
        }
        19 => {
            debug!("line-special #{}: lowerFloor floor!", line.special);
            ev_do_floor(line.clone(), FloorKind::LowerFloor, level);
            line.special = 0;
        }
        30 => {
            debug!("line-special #{}: raiseToTexture floor!", line.special);
            ev_do_floor(line.clone(), FloorKind::RaiseToTexture, level);
            line.special = 0;
        }
        36 => {
            debug!("line-special #{}: TurboLower floor!", line.special);
            ev_do_floor(line.clone(), FloorKind::TurboLower, level);
            line.special = 0;
        }
        37 => {
            debug!("line-special #{}: lowerAndChange floor!", line.special);
            ev_do_floor(line.clone(), FloorKind::LowerAndChange, level);
            line.special = 0;
        }
        38 => {
            debug!("line-special #{}: lowerFloorToLowest floor!", line.special);
            ev_do_floor(line.clone(), FloorKind::LowerFloorToLowest, level);
            line.special = 0;
        }
        56 => {
            debug!("line-special #{}: raiseFloorCrush floor!", line.special);
            ev_do_floor(line.clone(), FloorKind::RaiseFloorCrush, level);
            line.special = 0;
        }
        59 => {
            debug!(
                "line-special #{}: raiseFloor24AndChange floor!",
                line.special
            );
            ev_do_floor(line.clone(), FloorKind::RaiseFloor24andChange, level);
            line.special = 0;
        }
        119 => {
            debug!("line-special #{}: raiseFloorToNearest floor!", line.special);
            ev_do_floor(line.clone(), FloorKind::RaiseFloorToNearest, level);
            line.special = 0;
        }
        130 => {
            debug!("line-special #{}: raiseFloorTurbo floor!", line.special);
            ev_do_floor(line.clone(), FloorKind::RaiseFloorTurbo, level);
            line.special = 0;
        }
        82 => {
            debug!("line-special #{}: raiseFloorTurbo floor!", line.special);
            ev_do_floor(line, FloorKind::LowerFloorToLowest, level);
        }
        83 => {
            debug!("line-special #{}: lowerFloor floor!", line.special);
            ev_do_floor(line, FloorKind::LowerFloor, level);
        }
        84 => {
            debug!("line-special #{}: lowerAndChange floor!", line.special);
            ev_do_floor(line, FloorKind::LowerAndChange, level);
        }
        91 => {
            debug!("line-special #{}: raiseFloor floor!", line.special);
            ev_do_floor(line, FloorKind::RaiseFloor, level);
        }
        92 => {
            debug!("line-special #{}: raiseFloor24 floor!", line.special);
            ev_do_floor(line, FloorKind::RaiseFloor24, level);
        }
        93 => {
            debug!(
                "line-special #{}: raiseFloor24AndChange floor!",
                line.special
            );
            ev_do_floor(line, FloorKind::RaiseFloor24andChange, level);
        }
        94 => {
            debug!("line-special #{}: raiseFloorCrush floor!", line.special);
            ev_do_floor(line, FloorKind::RaiseFloorCrush, level);
        }
        96 => {
            debug!("line-special #{}: raiseToTexture floor!", line.special);
            ev_do_floor(line, FloorKind::RaiseToTexture, level);
        }
        98 => {
            debug!("line-special #{}: turboLower floor!", line.special);
            ev_do_floor(line, FloorKind::TurboLower, level);
        }
        128 => {
            debug!("line-special #{}: raiseFloorToNearest floor!", line.special);
            ev_do_floor(line, FloorKind::RaiseFloorToNearest, level);
        }
        129 => {
            debug!("line-special #{}: raiseFloorTurbo floor!", line.special);
            ev_do_floor(line, FloorKind::RaiseFloorTurbo, level);
        }
        6 => {
            debug!("line-special #{}: fastCrushAndRaise ceiling!", line.special);
            ev_do_ceiling(line.clone(), CeilKind::FastCrushAndRaise, level);
            line.special = 0;
        }
        25 => {
            debug!("line-special #{}: crushAndRaise ceiling!", line.special);
            ev_do_ceiling(line.clone(), CeilKind::CrushAndRaise, level);
            line.special = 0;
        }
        40 => {
            debug!(
                "line-special #{}: raiseToHighest ceiling, floor!",
                line.special
            );
            ev_do_ceiling(line.clone(), CeilKind::RaiseToHighest, level);
            ev_do_floor(line.clone(), FloorKind::LowerFloorToLowest, level);
            line.special = 0;
        }
        44 => {
            debug!("line-special #{}: lowerAndCrush ceiling!", line.special);
            ev_do_ceiling(line.clone(), CeilKind::LowerAndCrush, level);
            line.special = 0;
        }
        141 => {
            debug!(
                "line-special #{}: silentCrushAndRaise ceiling!",
                line.special
            );
            ev_do_ceiling(line.clone(), CeilKind::SilentCrushAndRaise, level);
            line.special = 0;
        }
        72 => {
            debug!("line-special #{}: LowerAndCrush ceiling!", line.special);
            ev_do_ceiling(line.clone(), CeilKind::LowerAndCrush, level);
        }
        73 => {
            debug!("line-special #{}: crushAndRaise ceiling!", line.special);
            ev_do_ceiling(line.clone(), CeilKind::CrushAndRaise, level);
        }
        77 => {
            debug!("line-special #{}: fastCrushAndRaise ceiling!", line.special);
            ev_do_ceiling(line.clone(), CeilKind::FastCrushAndRaise, level);
        }
        52 => {
            level.do_exit_level();
        }
        124 => {
            level.do_secret_exit_level();
        }
        12 => {
            debug!(
                "line-special #{}: turn light on nearest bright!",
                line.special
            );
            ev_turn_light_on(line.clone(), 0, level);
            line.special = 0;
        }
        13 => {
            debug!("line-special #{}: turn light on 255!", line.special);
            ev_turn_light_on(line.clone(), 255, level);
            line.special = 0;
        }
        35 => {
            debug!("line-special #{}: turn light off!", line.special);
            ev_turn_light_on(line.clone(), 35, level);
            line.special = 0;
        }
        79 => {
            debug!("line-special #{}: turn light off!", line.special);
            ev_turn_light_on(line.clone(), 35, level);
        }
        80 => {
            debug!(
                "line-special #{}: turn light on nearest bright!",
                line.special
            );
            ev_turn_light_on(line, 0, level);
        }
        81 => {
            debug!("line-special #{}: turn light on 255!", line.special);
            ev_turn_light_on(line, 255, level);
        }
        17 => {
            debug!("line-special #{}: start light strobe!", line.special);
            ev_start_light_strobing(line.clone(), level);
            line.special = 0;
        }
        104 => {
            debug!(
                "line-special #{}: turn lights off in sector tag!",
                line.special
            );
            ev_turn_tag_lights_off(line.clone(), level);
            line.special = 0;
        }
        8 => {
            debug!("line-special #{}: build 8 stair steps", line.special);
            ev_build_stairs(line.clone(), StairKind::Build8, level);
            line.special = 0;
        }
        100 => {
            debug!("line-special #{}: build 16 stair steps turbo", line.special);
            ev_build_stairs(line.clone(), StairKind::Turbo16, level);
            line.special = 0;
        }
        125 => {
            // TELEPORT MonsterONLY
            if thing.player().is_none() {
                teleport(line.clone(), side, thing, level);
                line.special = 0;
            }
        }
        39 => {
            teleport(line.clone(), side, thing, level);
            line.special = 0;
        }
        54 => {
            ev_stop_platform(line.clone(), level);
            line.special = 0;
        }
        89 => {
            ev_stop_platform(line.clone(), level);
        }
        97 => {
            teleport(line, side, thing, level);
        }
        126 => {
            // TELEPORT MonsterONLY
            if thing.player().is_none() {
                teleport(line.clone(), side, thing, level);
            }
        }
        114 | 103 => {
            // Ignore. It's a switch
        }
        _ => {
            //warn!("Invalid or unimplemented line special: {}", line.special);
        }
    }
}

/// Actions for when a thing shoots a special line
///
/// Doom function name `P_ShootSpecialLine`
pub fn shoot_special_line(line: MapPtr<LineDef>, thing: &mut MapObject) {
    let mut ok = false;

    if thing.level.is_null() {
        panic!("Thing had a bad level pointer");
    }
    let level = unsafe { &mut *thing.level };

    if thing.player().is_none() {
        if line.special == 46 {
            ok = true;
        }
        if !ok {
            return;
        }
    }

    match line.special {
        24 => {
            debug!("shoot line-special #{}: raise floor!", line.special);
            ev_do_floor(line.clone(), FloorKind::RaiseFloor, level);
            change_switch_texture(
                line,
                false,
                &level.switch_list,
                &mut level.button_list,
                &level.snd_command,
            );
        }
        46 => {
            debug!("shoot line-special #{}: open door!", line.special);
            ev_do_door(line.clone(), DoorKind::Open, level);
            change_switch_texture(
                line,
                true,
                &level.switch_list,
                &mut level.button_list,
                &level.snd_command,
            );
        }
        47 => {
            debug!(
                "shoot line-special #{}: raise platform and change!",
                line.special
            );
            ev_do_platform(line.clone(), PlatKind::RaiseToNearestAndChange, 0, level);
            change_switch_texture(
                line,
                false,
                &level.switch_list,
                &mut level.button_list,
                &level.snd_command,
            );
        }
        _ => {}
    }
}

pub fn spawn_specials(level: &mut Level) {
    // TODO: level timer

    let level_iter = unsafe { &mut *(level as *mut Level) };
    for sector in level_iter
        .map_data
        .sectors_mut()
        .iter_mut()
        .filter(|s| s.special != 0)
    {
        match sector.special {
            1 => {
                debug!("sector-special #{}: light flicker!", sector.special);
                LightFlash::spawn(sector, level);
            }
            2 => {
                debug!("sector-special #{}: strobe fast!", sector.special);
                StrobeFlash::spawn(sector, FASTDARK, false, level);
            }
            3 => {
                debug!("sector-special #{}: strobe slow!", sector.special);
                StrobeFlash::spawn(sector, SLOWDARK, false, level);
            }
            4 => {
                debug!(
                    "sector-special #{}: strobe fast death/slime!",
                    sector.special
                );
                StrobeFlash::spawn(sector, FASTDARK, false, level);
                sector.special = 4;
            }
            8 => {
                debug!("sector-special #{}: glowing light!", sector.special);
                Glow::spawn(sector, level);
            }
            9 => {
                debug!("sector-special #{}: secret", sector.special);
                level.total_level_secrets += 1;
            }
            12 => {
                debug!("sector-special #{}: strobe slow!", sector.special);
                StrobeFlash::spawn(sector, SLOWDARK, true, level);
            }
            13 => {
                debug!("sector-special #{}: strobe fast!", sector.special);
                StrobeFlash::spawn(sector, FASTDARK, true, level);
            }
            14 => {
                error!(
                    "sector-special #{}: P_SpawnDoorRaiseIn5Mins not implemented",
                    sector.special
                );
            }
            17 => {
                debug!("sector-special #{}: fire flicker!", sector.special);
                FireFlicker::spawn(sector, level);
            }
            _ => {
                // warn!(
                //     "Invalid or unimplemented sector special spawner: {}",
                //     sector.special
                // );
            }
        }
    }

    for line in level_iter.map_data.linedefs.iter_mut() {
        // Scrolling wall
        if line.special == 48 {
            level.line_special_list.push(MapPtr::new(line));
        }
    }
}

/// Doom function name `P_UpdateSpecials`
pub fn update_specials(level: &mut Level, pic_data: &mut PicData) {
    // Used mostly for deathmatch as far as I know
    if level.level_timer && level.level_time == 0 {
        // exit
        level.do_exit_level();
    }

    // Flats and wall texture animations (switching between series)
    for anim in level.animations.iter_mut() {
        anim.update(pic_data, level.level_time as usize);
    }

    // Animate switches
    for b in level.button_list.iter_mut() {
        if b.timer != 0 {
            b.timer -= 1;
            if b.timer == 0 {
                debug!("Button {:?} is switching after countdown", b.line.as_ref());
                match b.bwhere {
                    ButtonWhere::Top => {
                        if let Some(t) = b.line.front_sidedef.toptexture.as_mut() {
                            *t = b.texture;
                        }
                    }
                    ButtonWhere::Middle => {
                        if let Some(t) = b.line.front_sidedef.midtexture.as_mut() {
                            *t = b.texture;
                        }
                    }
                    ButtonWhere::Bottom => {
                        if let Some(t) = b.line.front_sidedef.bottomtexture.as_mut() {
                            *t = b.texture;
                        }
                    }
                }
                start_sector_sound(&b.line, SfxName::Swtchn, &level.snd_command);
            }
        }
    }
    for line in level.line_special_list.iter_mut() {
        line.front_sidedef.textureoffset += 1.0;
        if line.front_sidedef.textureoffset == f32::MAX {
            line.front_sidedef.textureoffset = 0.0;
        }
    }
}

/// P_RespawnSpecials
pub fn respawn_specials(level: &mut Level) {
    // only respawn items in deathmatch
    if level.options.deathmatch == 0 && !level.options.respawn_monsters {
        return;
    }

    if let Some(mthing) = level.respawn_queue.back() {
        // wait at least 30 seconds
        if (level.level_time - mthing.0) / (TICRATE as u32) < 30 {
            return;
        }
    } else {
        return;
    }

    if let Some(mthing) = level.respawn_queue.pop_back() {
        let xy = Vec2::new(mthing.1.x as f32, mthing.1.y as f32);

        // spawn a teleport fog at the new spot
        let ss = level.map_data.point_in_subsector(xy);
        let floor = ss.sector.floorheight as i32;
        let fog = unsafe {
            &mut *MapObject::spawn_map_object(xy.x, xy.y, floor, MapObjKind::MT_TFOG, level)
        };
        fog.start_sound(SfxName::Itmbk);

        let mut i = 0;
        for n in 0..MapObjKind::Count as u16 {
            if mthing.1.kind == MOBJINFO[n as usize].doomednum as i16 {
                i = n;
                break;
            }
        }

        if i == MapObjKind::Count as u16 {
            error!(
                "P_SpawnMapThing: Unknown type {} at ({}, {})",
                mthing.1.kind, mthing.1.x, mthing.1.y
            );
        }

        let kind = MapObjKind::from(i);

        let z = if MOBJINFO[i as usize].flags & MapObjFlag::Spawnceiling as u32 != 0 {
            ONCEILINGZ
        } else {
            ONFLOORZ
        };

        // spawn it
        let thing = unsafe { &mut *MapObject::spawn_map_object(xy.x, xy.y, z, kind, level) };
        thing.angle = Angle::new((mthing.1.angle as f32).to_radians());
        thing.spawnpoint = mthing.1;
    }
}
