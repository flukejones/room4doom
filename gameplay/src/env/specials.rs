//! Implements special effects:
//! Texture animation, height or lighting changes according to adjacent sectors,
//! respective utility functions. Line Tag handling. Line and Sector triggers.
//!
//! Doom source name `p_spec`

use crate::doom_def::{ONCEILINGZ, ONFLOORZ};
use crate::env::ceiling::{CeilKind, ev_do_ceiling};
use crate::env::doors::{DoorKind, ev_do_door};
use crate::env::floor::{FloorKind, StairKind, ev_build_stairs, ev_do_floor};
use crate::env::generalized;
use crate::env::lights::{
    FASTDARK, FireFlicker, Glow, LightFlash, SLOWDARK, StrobeFlash, ev_start_light_strobing, ev_turn_light_on, ev_turn_tag_lights_off
};
use crate::env::platforms::{PlatKind, ev_do_platform, ev_stop_platform};
use crate::env::switch::{change_switch_texture, start_sector_sound};
use crate::env::teleport::teleport;
use crate::info::{MOBJINFO, MapObjKind};
use crate::level::LevelState;
use crate::pic::ButtonWhere;
use crate::thing::MapObject;
use crate::{MapObjFlag, TICRATE};
use level::flags::LineDefFlags;
use level::map_defs::{LineDef, Sector, SectorHeight};
use level::{BSP3D, MapPtr, MovementType, WallType};
use log::{debug, error, trace};
use math::{Angle, FixedT};
use pic_data::PicData;
use sound_common::SfxName;

// BOOM generalized sector type bit masks
#[allow(dead_code)]
const BOOM_DAMAGE_MASK: i16 = 0x60;
#[allow(dead_code)]
const BOOM_DAMAGE_SHIFT: i16 = 5;
const BOOM_SECRET_MASK: i16 = 0x80;
use std::ptr;

pub fn get_next_sector(line: MapPtr<LineDef>, sector: MapPtr<Sector>) -> Option<MapPtr<Sector>> {
    if !line.flags.contains(LineDefFlags::TwoSided) {
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
pub fn find_lowest_ceiling_surrounding(sec: MapPtr<Sector>) -> SectorHeight {
    let mut height = SectorHeight::MAX;
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
pub fn find_highest_ceiling_surrounding(sec: MapPtr<Sector>) -> SectorHeight {
    let mut height = SectorHeight::ZERO;
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
pub fn find_lowest_floor_surrounding(sec: MapPtr<Sector>) -> SectorHeight {
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
pub fn find_highest_floor_surrounding(sec: MapPtr<Sector>) -> SectorHeight {
    let mut floor = -SectorHeight::MAX;
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

/// OG `P_FindNextHighestFloor` — find lowest floor ABOVE current among
/// neighbours.
pub fn find_next_highest_floor(sec: MapPtr<Sector>, current: SectorHeight) -> SectorHeight {
    let mut height_list = Vec::new();

    for line in &sec.lines {
        if let Some(other) = get_next_sector(line.clone(), sec.clone()) {
            // OG: only include floors ABOVE current
            if other.floorheight > current {
                height_list.push(other.floorheight);
            }
        }
    }

    if height_list.is_empty() {
        return current;
    }

    // Find lowest in the filtered list (the next step up)
    let mut min = height_list[0];
    for h in &height_list[1..] {
        if *h < min {
            min = *h;
        }
    }

    min
}

/// OG `P_ChangeSector` -- iterate blockmap cells in sector's bounding box.
fn change_sector(sector: &Sector, crunch: bool, level: &mut LevelState) -> bool {
    let mut no_fit = false;

    let bm = level.level_data.blockmap();
    let bmw = bm.columns;
    let bmh = bm.rows;

    // OG: sector->blockbox is [top, bottom, left, right]
    for bx in sector.blockbox[2]..=sector.blockbox[3] {
        for by in sector.blockbox[1]..=sector.blockbox[0] {
            if bx < 0 || by < 0 || bx >= bmw || by >= bmh {
                continue;
            }
            let idx = (by * bmw + bx) as usize;
            let mut mobj_ptr = level.blocklinks[idx];
            while let Some(ptr) = mobj_ptr {
                let thing = unsafe { &mut *ptr };
                mobj_ptr = thing.b_next;
                thing.pit_change_sector(&mut no_fit, crunch);
            }
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
    speed: SectorHeight,
    dest: SectorHeight,
    crush: bool,
    floor_or_ceiling: i32,
    direction: i32,
    level: &mut LevelState,
) -> PlaneResult {
    // Split borrow: bsp3d from level_data, blocklinks from level
    let bsp3d = unsafe { &mut *(&mut level.level_data.bsp_3d as *mut BSP3D) };
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
                        bsp3d.move_surface(
                            sector_num,
                            MovementType::Floor,
                            dest.to_f32(),
                            sector.floorpic,
                        );

                        if change_sector(&sector, crush, level) {
                            sector.floorheight = last_pos;
                            bsp3d.move_surface(
                                sector_num,
                                MovementType::Floor,
                                last_pos.to_f32(),
                                sector.floorpic,
                            );
                            change_sector(&sector, crush, level);
                        }
                        return PlaneResult::PastDest;
                    } else {
                        // COULD GET CRUSHED
                        let last_pos = sector.floorheight;
                        sector.floorheight = sector.floorheight - speed;
                        bsp3d.move_surface(
                            sector_num,
                            MovementType::Floor,
                            sector.floorheight.to_f32(),
                            sector.floorpic,
                        );
                        // OG: floor-down always reverts on collision (no crush check)
                        if change_sector(&sector, crush, level) {
                            sector.floorheight = last_pos;
                            bsp3d.move_surface(
                                sector_num,
                                MovementType::Floor,
                                last_pos.to_f32(),
                                sector.floorpic,
                            );
                            change_sector(&sector, crush, level);
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
                        bsp3d.move_surface(
                            sector_num,
                            MovementType::Floor,
                            dest.to_f32(),
                            sector.floorpic,
                        );

                        if change_sector(&sector, crush, level) {
                            sector.floorheight = last_pos;
                            bsp3d.move_surface(
                                sector_num,
                                MovementType::Floor,
                                last_pos.to_f32(),
                                sector.floorpic,
                            );
                            change_sector(&sector, crush, level);
                        }
                        return PlaneResult::PastDest;
                    } else {
                        let last_pos = sector.floorheight;
                        sector.floorheight = sector.floorheight + speed;
                        bsp3d.move_surface(
                            sector_num,
                            MovementType::Floor,
                            sector.floorheight.to_f32(),
                            sector.floorpic,
                        );
                        if change_sector(&sector, crush, level) {
                            if crush {
                                return PlaneResult::Crushed;
                            }
                            sector.floorheight = last_pos;
                            bsp3d.move_surface(
                                sector_num,
                                MovementType::Floor,
                                last_pos.to_f32(),
                                sector.floorpic,
                            );
                            change_sector(&sector, crush, level);
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
                        bsp3d.move_surface(
                            sector_num,
                            MovementType::Ceiling,
                            dest.to_f32(),
                            sector.ceilingpic,
                        );

                        if change_sector(&sector, crush, level) {
                            sector.ceilingheight = last_pos;
                            bsp3d.move_surface(
                                sector_num,
                                MovementType::Ceiling,
                                last_pos.to_f32(),
                                sector.ceilingpic,
                            );
                            change_sector(&sector, crush, level);
                        }
                        return PlaneResult::PastDest;
                    } else {
                        // COULD GET CRUSHED
                        let last_pos = sector.ceilingheight;
                        sector.ceilingheight = sector.ceilingheight - speed;
                        bsp3d.move_surface(
                            sector_num,
                            MovementType::Ceiling,
                            sector.ceilingheight.to_f32(),
                            sector.ceilingpic,
                        );

                        if change_sector(&sector, crush, level) {
                            if crush {
                                return PlaneResult::Crushed;
                            }
                            sector.ceilingheight = last_pos;
                            bsp3d.move_surface(
                                sector_num,
                                MovementType::Ceiling,
                                last_pos.to_f32(),
                                sector.ceilingpic,
                            );
                            change_sector(&sector, crush, level);
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
                    // OG: strictly greater (not >=)
                    if sector.ceilingheight + speed > dest {
                        let last_pos = sector.ceilingheight;
                        sector.ceilingheight = dest;
                        bsp3d.move_surface(
                            sector_num,
                            MovementType::Ceiling,
                            dest.to_f32(),
                            sector.ceilingpic,
                        );

                        if change_sector(&sector, crush, level) {
                            sector.ceilingheight = last_pos;
                            bsp3d.move_surface(
                                sector_num,
                                MovementType::Ceiling,
                                last_pos.to_f32(),
                                sector.ceilingpic,
                            );
                            change_sector(&sector, crush, level);
                        }
                        return PlaneResult::PastDest;
                    } else {
                        sector.ceilingheight = sector.ceilingheight + speed;
                        bsp3d.move_surface(
                            sector_num,
                            MovementType::Ceiling,
                            sector.ceilingheight.to_f32(),
                            sector.ceilingpic,
                        );
                        change_sector(&sector, crush, level);
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
    let level: &mut LevelState = unsafe { &mut *thing.level };

    // BOOM generalized linedef types
    if generalized::is_generalized(line.special) {
        let is_monster = thing.player().is_none();
        generalized::handle_generalized_cross(line, thing, level, is_monster);
        return;
    }

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
    let level: &mut LevelState = unsafe { &mut *thing.level };

    if thing.player().is_none() {
        if line.special == 46 {
            ok = true;
        }
        if !ok {
            return;
        }
    }

    // BOOM generalized linedef types
    if generalized::is_generalized(line.special) {
        generalized::handle_generalized_shoot(line, thing, level);
        return;
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
                &mut level.level_data.bsp_3d,
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
                &mut level.level_data.bsp_3d,
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
                &mut level.level_data.bsp_3d,
            );
        }
        _ => {}
    }
}

pub fn spawn_specials(level: &mut LevelState) {
    // TODO: level timer

    let level_iter = unsafe { &mut *(level as *mut LevelState) };
    for sector in level_iter
        .level_data
        .sectors_mut()
        .iter_mut()
        .filter(|s| s.special != 0)
    {
        // BOOM: bits 5-11 encode generalized sector properties.
        // Vanilla type is in bits 0-4.
        let vanilla_type = sector.special & 0x1F;

        // BOOM secret (bit 7)
        if sector.special & BOOM_SECRET_MASK != 0 {
            level.total_level_secrets += 1;
        }

        match vanilla_type {
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
                // Only count vanilla secret if BOOM secret bit is not set
                // (BOOM secret already counted above)
                if sector.special & BOOM_SECRET_MASK == 0 {
                    level.total_level_secrets += 1;
                }
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
            _ => {}
        }
    }

    for line in level_iter.level_data.linedefs.iter_mut() {
        // Scrolling wall
        if line.special == 48 {
            level.line_special_list.push(MapPtr::new(line));
        }
    }
}

/// Doom function name `P_UpdateSpecials`
pub fn update_specials(level: &mut LevelState, pic_data: &mut PicData) {
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
                        level.level_data.bsp_3d.update_wall_texture(
                            b.line.num,
                            WallType::Upper,
                            b.texture,
                        );
                    }
                    ButtonWhere::Middle => {
                        if let Some(t) = b.line.front_sidedef.midtexture.as_mut() {
                            *t = b.texture;
                        }
                        level.level_data.bsp_3d.update_wall_texture(
                            b.line.num,
                            WallType::Middle,
                            b.texture,
                        );
                    }
                    ButtonWhere::Bottom => {
                        if let Some(t) = b.line.front_sidedef.bottomtexture.as_mut() {
                            *t = b.texture;
                        }
                        level.level_data.bsp_3d.update_wall_texture(
                            b.line.num,
                            WallType::Lower,
                            b.texture,
                        );
                    }
                }
                start_sector_sound(&b.line, SfxName::Swtchn, &level.snd_command);
            }
        }
    }
    for line in level.line_special_list.iter_mut() {
        line.front_sidedef.textureoffset += 1;
        if line.front_sidedef.textureoffset == FixedT::MAX {
            line.front_sidedef.textureoffset = FixedT::ZERO;
        }
    }
}

/// P_RespawnSpecials
pub fn respawn_specials(level: &mut LevelState) {
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
        // spawn a teleport fog at the new spot
        // OG: mthing->x/y are i16 map units, << 16 for fixed-point
        let ss = level.level_data.point_in_subsector(
            FixedT::from(mthing.1.x as i32),
            FixedT::from(mthing.1.y as i32),
        );
        let floor = ss.sector.floorheight.to_i32();
        let fog = unsafe {
            &mut *MapObject::spawn_map_object(
                FixedT::from(mthing.1.x as i32),
                FixedT::from(mthing.1.y as i32),
                floor.into(),
                MapObjKind::MT_TFOG,
                level,
            )
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

        let z = if MOBJINFO[i as usize]
            .flags
            .contains(MapObjFlag::Spawnceiling)
        {
            ONCEILINGZ
        } else {
            ONFLOORZ
        };

        // spawn it
        let thing = unsafe {
            &mut *MapObject::spawn_map_object(
                FixedT::from(mthing.1.x as i32),
                FixedT::from(mthing.1.y as i32),
                z.into(),
                kind,
                level,
            )
        };
        // OG: mobj->angle = ANG45 * (mthing->angle/45)
        thing.angle = Angle::from_bam(math::ANG45.wrapping_mul((mthing.1.angle as u32) / 45));
        thing.spawnpoint = mthing.1;
    }
}
