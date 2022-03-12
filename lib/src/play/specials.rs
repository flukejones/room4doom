//! Implements special effects:
//! Texture animation, height or lighting changes according to adjacent sectors,
//! respective utility functions. Line Tag handling. Line and Sector triggers.
//!
//! Doom source name `p_spec`

use super::{
    ceiling::{ev_do_ceiling, CeilingKind},
    doors::{ev_do_door, DoorKind},
    floor::{ev_build_stairs, ev_do_floor, FloorKind, StairKind},
    lights::{
        ev_start_light_strobing, ev_turn_light_on, ev_turn_tag_lights_off, FireFlicker, Glow,
        LightFlash, StrobeFlash, FASTDARK, SLOWDARK,
    },
    map_object::MapObject,
    platforms::{ev_do_platform, PlatKind},
};

use crate::{
    flags::LineDefFlags,
    info::MapObjectType,
    level_data::{
        map_defs::{LineDef, Sector},
        Level,
    },
    textures::ButtonWhere,
    DPtr, Game,
};
use log::{debug, error, trace, warn};

pub fn get_next_sector(line: DPtr<LineDef>, sector: DPtr<Sector>) -> Option<DPtr<Sector>> {
    if line.flags & LineDefFlags::TwoSided as u32 == 0 {
        return None;
    }

    if line.frontsector == sector {
        return line.backsector.clone();
    }

    Some(line.frontsector.clone())
}

/// P_FindMinSurroundingLight
pub fn find_min_light_surrounding(sec: DPtr<Sector>, max: i32) -> i32 {
    let mut min = max;
    for line in &sec.lines {
        if let Some(other) = get_next_sector(line.clone(), sec.clone()) {
            if other.lightlevel < min {
                min = other.lightlevel;
            }
        }
    }
    debug!("find_min_light_surrounding: {min}");
    min
}

pub fn find_max_light_surrounding(sec: DPtr<Sector>, mut max: i32) -> i32 {
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
pub fn find_lowest_ceiling_surrounding(sec: DPtr<Sector>) -> f32 {
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
pub fn find_highest_ceiling_surrounding(sec: DPtr<Sector>) -> f32 {
    let mut height = f32::MAX;
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
pub fn find_lowest_floor_surrounding(sec: DPtr<Sector>) -> f32 {
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
pub fn find_highest_floor_surrounding(sec: DPtr<Sector>) -> f32 {
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
pub fn find_next_highest_floor(sec: DPtr<Sector>, current: f32) -> f32 {
    let mut min;
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
    min = height_list[0];

    for height in height_list {
        if height < min {
            min = height;
        }
    }

    min
}

/// P_ChangeSector
fn change_sector(sector: DPtr<Sector>, crunch: bool) -> bool {
    let mut no_fit = false;

    if !sector.thinglist.is_null() {
        let mut thing = sector.thinglist;
        while !thing.is_null() {
            unsafe {
                trace!("Thing type {:?} is in affected sector", (*thing).kind);
                (*thing).pit_change_sector(&mut no_fit, crunch);

                if (*thing).s_next.is_null() || (*thing).s_next == thing {
                    break;
                }
                thing = (*thing).s_next;
            }
        }
    }

    no_fit
}

/// The result of raising a plane. `PastDest` = stop, `Crushed` = should crush all in sector
#[derive(Debug, Clone, Copy)]
pub enum PlaneResult {
    Ok,
    Crushed,
    PastDest,
}

pub fn move_plane(
    mut sector: DPtr<Sector>,
    speed: f32,
    dest: f32,
    crush: bool,
    floor_or_ceiling: i32,
    direction: i32,
) -> PlaneResult {
    match floor_or_ceiling {
        0 => {
            // FLOOR
            match direction {
                -1 => {
                    // DOWN
                    trace!(
                        "move_plane: floor: down: {} to {} at speed {}",
                        sector.floorheight,
                        dest,
                        speed
                    );
                    if sector.floorheight - speed < dest {
                        let last_pos = sector.floorheight;
                        sector.floorheight = dest;

                        if change_sector(sector.clone(), crush) {
                            sector.floorheight = last_pos;
                            change_sector(sector, crush);
                        }
                        return PlaneResult::PastDest;
                    } else {
                        // COULD GET CRUSHED
                        let last_pos = sector.floorheight;
                        sector.floorheight -= speed;

                        if change_sector(sector.clone(), crush) {
                            if crush {
                                return PlaneResult::Crushed;
                            }
                            sector.floorheight = last_pos;
                            change_sector(sector, crush);
                            return PlaneResult::Crushed;
                        }
                    }
                }
                1 => {
                    // UP
                    trace!(
                        "move_plane: floor: up: {} to {} at speed {}",
                        sector.floorheight,
                        dest,
                        speed
                    );
                    if sector.floorheight + speed > dest {
                        let last_pos = sector.floorheight;
                        sector.floorheight = dest;

                        if change_sector(sector.clone(), crush) {
                            sector.floorheight = last_pos;
                            change_sector(sector, crush);
                        }
                        return PlaneResult::PastDest;
                    } else {
                        let last_pos = sector.floorheight;
                        sector.floorheight += speed;
                        if change_sector(sector.clone(), crush) {
                            if crush {
                                return PlaneResult::Crushed;
                            }
                            sector.floorheight = last_pos;
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
                        sector.ceilingheight,
                        dest,
                        speed
                    );
                    if sector.ceilingheight - speed < dest {
                        let last_pos = sector.ceilingheight;
                        sector.ceilingheight = dest;

                        if change_sector(sector.clone(), crush) {
                            sector.ceilingheight = last_pos;
                            change_sector(sector, crush);
                        }
                        return PlaneResult::PastDest;
                    } else {
                        // COULD GET CRUSHED
                        let last_pos = sector.ceilingheight;
                        sector.ceilingheight -= speed;

                        if change_sector(sector.clone(), crush) {
                            if crush {
                                return PlaneResult::Crushed;
                            }
                            sector.ceilingheight = last_pos;
                            change_sector(sector, crush);
                            return PlaneResult::Crushed;
                        }
                    }
                }
                1 => {
                    // UP
                    trace!(
                        "move_plane: ceiling: up: {} to {} at speed {}",
                        sector.ceilingheight,
                        dest,
                        speed
                    );
                    if sector.ceilingheight + speed > dest {
                        let last_pos = sector.ceilingheight;
                        sector.ceilingheight = dest;

                        if change_sector(sector.clone(), crush) {
                            sector.ceilingheight = last_pos;
                            change_sector(sector, crush);
                        }
                        return PlaneResult::PastDest;
                    } else {
                        //let last_pos = sector.ceilingheight;
                        sector.ceilingheight += speed;
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

/// Trigger various actions when a line is crossed which has a non-zero special attached
///
/// Doom function name is `P_CrossSpecialLine`
pub fn cross_special_line(_side: usize, mut line: DPtr<LineDef>, thing: &MapObject) {
    let mut ok = false;

    //  Triggers that other things can activate
    if thing.player.is_none() {
        // Things that should NOT trigger specials...
        match thing.kind {
            MapObjectType::MT_ROCKET
            | MapObjectType::MT_PLASMA
            | MapObjectType::MT_BFG
            | MapObjectType::MT_TROOPSHOT
            | MapObjectType::MT_HEADSHOT
            | MapObjectType::MT_BRUISERSHOT => return,
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
            ev_do_ceiling(line.clone(), CeilingKind::FastCrushAndRaise, level);
            line.special = 0;
        }
        25 => {
            debug!("line-special #{}: crushAndRaise ceiling!", line.special);
            ev_do_ceiling(line.clone(), CeilingKind::CrushAndRaise, level);
            line.special = 0;
        }
        40 => {
            debug!(
                "line-special #{}: raiseToHighest ceiling, floor!",
                line.special
            );
            ev_do_ceiling(line.clone(), CeilingKind::RaiseToHighest, level);
            ev_do_floor(line.clone(), FloorKind::LowerFloorToLowest, level);
            line.special = 0;
        }
        44 => {
            debug!("line-special #{}: lowerAndCrush ceiling!", line.special);
            ev_do_ceiling(line.clone(), CeilingKind::LowerAndCrush, level);
            line.special = 0;
        }
        141 => {
            debug!(
                "line-special #{}: silentCrushAndRaise ceiling!",
                line.special
            );
            ev_do_ceiling(line.clone(), CeilingKind::SilentCrushAndRaise, level);
            line.special = 0;
        }
        72 => {
            debug!("line-special #{}: LowerAndCrush ceiling!", line.special);
            ev_do_ceiling(line.clone(), CeilingKind::LowerAndCrush, level);
        }
        73 => {
            debug!("line-special #{}: crushAndRaise ceiling!", line.special);
            ev_do_ceiling(line.clone(), CeilingKind::CrushAndRaise, level);
        }
        77 => {
            debug!("line-special #{}: fastCrushAndRaise ceiling!", line.special);
            ev_do_ceiling(line.clone(), CeilingKind::FastCrushAndRaise, level);
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
            if thing.player.is_none() {
                // EV_Teleport
                line.special = 0;
            }
            error!(
                "line-special #{}: EV_Teleport not implemented",
                line.special
            );
        }
        39 => {
            error!(
                "line-special #{}: EV_Teleport not implemented",
                line.special
            );
            line.special = 0;
        }
        54 => {
            error!(
                "line-special #{}: EV_StopPlat not implemented",
                line.special
            );
            line.special = 0;
        }
        89 => {
            error!(
                "line-special #{}: EV_StopPlat not implemented",
                line.special
            );
        }
        97 => {
            error!(
                "line-special #{}: EV_Teleport not implemented",
                line.special
            );
        }
        126 => {
            // TELEPORT MonsterONLY
            if thing.player.is_none() {
                // EV_Teleport
            }
            error!(
                "line-special #{}: EV_Teleport not implemented",
                line.special
            );
        }
        114 | 103 => {
            // Ignore. It's a switch
        }
        _ => {
            warn!("Invalid or unimplemented line special: {}", line.special);
        }
    }
}

pub fn spawn_specials(level: &mut Level) {
    let level_iter = unsafe { &mut *(level as *mut Level) };
    for sector in level_iter
        .map_data
        .sectors
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
            9 => {
                level.totalsecret += 1;
            }
            12 => {
                debug!("sector-special #{}: strobe slow!", sector.special);
                StrobeFlash::spawn(sector, SLOWDARK, true, level);
            }
            13 => {
                debug!("sector-special #{}: strobe fast!", sector.special);
                StrobeFlash::spawn(sector, FASTDARK, true, level);
            }
            17 => {
                debug!("sector-special #{}: fire flicker!", sector.special);
                FireFlicker::spawn(sector, level);
            }
            8 => {
                debug!("sector-special #{}: glowing light!", sector.special);
                Glow::spawn(sector, level);
            }
            14 => {
                error!(
                    "sector-special #{}: P_SpawnDoorRaiseIn5Mins not implemented",
                    sector.special
                );
            }
            _ => {
                warn!(
                    "Invalid or unimplemented sector special spawner: {}",
                    sector.special
                );
            }
        }
    }
}

/// Doom function name `P_UpdateSpecials`
pub fn update_specials(game: &mut Game) {
    // TODO: level timer
    //if level.level_time

    for anim in &mut game.animations {
        anim.update(
            &mut game.textures,
            game.level.as_ref().unwrap().level_time as usize,
        );
    }

    if let Some(level) = game.level.as_mut() {
        for b in level.button_list.iter_mut() {
            if b.timer != 0 {
                b.timer -= 1;
                if b.timer == 0 {
                    debug!("Button {:?} is switching after countdown", b.line.as_ptr());
                    match b.bwhere {
                        ButtonWhere::Top => b.line.front_sidedef.toptexture = b.texture,
                        ButtonWhere::Middle => b.line.front_sidedef.midtexture = b.texture,
                        ButtonWhere::Bottom => b.line.front_sidedef.bottomtexture = b.texture,
                    }
                    // TODO: S_StartSound(&buttonlist[i].soundorg, sfx_swtchn);
                }
            }
        }
    }
}
