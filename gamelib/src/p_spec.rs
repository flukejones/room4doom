/// Implements special effects:
/// Texture animation, height or lighting changes according to adjacent sectors,
/// respective utility functions, etc.
use crate::angle::Angle;
use crate::d_thinker::Thinker;
use crate::flags::LineDefFlags;
use crate::info::MapObjectType;
use crate::level_data::map_defs::{LineDef, Sector};
use crate::p_ceiling::ev_do_ceiling;
use crate::p_doors::ev_do_door;
use crate::p_floor::ev_do_floor;
use crate::p_map_object::MapObject;
use crate::p_plats::ev_do_platform;
use crate::DPtr;
use log::{debug, warn};
use std::fmt;
use std::fmt::{Debug, Formatter};
use std::ptr::NonNull;

// P_LIGHTS
pub struct FireFlicker {
    pub thinker: NonNull<Thinker>,
    pub sector: DPtr<Sector>,
    pub count: i32,
    pub max_light: i32,
    pub min_light: i32,
}

pub struct LightFlash {
    pub thinker: NonNull<Thinker>,
    pub sector: DPtr<Sector>,
    pub count: i32,
    pub max_light: i32,
    pub min_light: i32,
    pub max_time: i32,
    pub min_time: i32,
}

pub struct Strobe {
    pub thinker: NonNull<Thinker>,
    pub sector: DPtr<Sector>,
    pub count: i32,
    pub min_light: i32,
    pub max_light: i32,
    pub dark_time: i32,
    pub bright_time: i32,
}

pub struct Glow {
    pub thinker: NonNull<Thinker>,
    pub sector: DPtr<Sector>,
    pub min_light: i32,
    pub max_light: i32,
    pub direction: Angle,
}

// P_PLATS
pub enum PlatStatus {
    Up,
    Down,
    Waiting,
    InStasis,
}

#[derive(Debug, Clone, Copy)]
pub enum PlatKind {
    PerpetualRaise,
    DownWaitUpStay,
    RaiseAndChange,
    RaiseToNearestAndChange,
    BlazeDWUS,
}

pub struct Platform {
    pub thinker: NonNull<Thinker>,
    pub sector: DPtr<Sector>,
    pub speed: f32,
    pub low: f32,
    pub high: f32,
    pub wait: i32,
    pub count: i32,
    pub status: PlatStatus,
    pub old_status: PlatStatus,
    pub crush: bool,
    pub tag: i16,
    pub kind: PlatKind,
}

// P_FLOOR
//
#[derive(Debug, Clone, Copy)]
pub enum FloorKind {
    /// lower floor to highest surrounding floor
    LowerFloor,
    /// lower floor to lowest surrounding floor
    LowerFloorToLowest,
    /// lower floor to highest surrounding floor VERY FAST
    TurboLower,
    /// raise floor to lowest surrounding CEILING
    RaiseFloor,
    /// raise floor to next highest surrounding floor
    RaiseFloorToNearest,
    /// raise floor to shortest height with same texture around it
    RaiseToTexture,
    /// lower floor to lowest surrounding floor and change floorpic
    LowerAndChange,
    /// Raise floor 24 units from start
    RaiseFloor24,
    /// Raise floor 24 units from start and change texture
    RaiseFloor24andChange,
    /// Raise floor and crush all entities on it
    RaiseFloorCrush,
    /// raise to next highest floor, turbo-speed
    RaiseFloorTurbo,
    /// Do donuts
    DonutRaise,
    /// Raise floor 512 units from start
    RaiseFloor512,
}

#[derive(Debug, Clone, Copy)]
pub enum StairEnum {
    /// slowly build by 8
    Build8,
    /// quickly build by 16
    Turbo16,
}

#[derive(Debug)]
pub enum ResultE {
    Ok,
    Crushed,
    PastDest,
}

pub struct FloorMove {
    pub thinker: NonNull<Thinker>,
    pub sector: DPtr<Sector>,
    pub kind: FloorKind,
    pub speed: f32,
    pub crush: bool,
    pub direction: i32,
    pub newspecial: i16,
    pub texture: u8,
    pub destheight: f32,
}

// P_CEILNG
#[derive(Debug, Clone, Copy)]
pub enum CeilingKind {
    lowerToFloor,
    raiseToHighest,
    lowerAndCrush,
    crushAndRaise,
    fastCrushAndRaise,
    silentCrushAndRaise,
}

pub struct CeilingMove {
    pub thinker: NonNull<Thinker>,
    pub sector: DPtr<Sector>,
    pub kind: CeilingKind,
    pub bottomheight: f32,
    pub topheight: f32,
    pub speed: f32,
    pub crush: bool,
    // 1 = up, 0 = waiting, -1 = down
    pub direction: i32,
    // ID
    pub tag: i16,
    pub olddirection: i32,
}

// P_DOORS
//
#[derive(Debug, Clone, Copy)]
pub enum DoorKind {
    vld_normal,
    vld_close30ThenOpen,
    vld_close,
    vld_open,
    vld_raiseIn5Mins,
    vld_blazeRaise,
    vld_blazeOpen,
    vld_blazeClose,
}

pub struct VerticalDoor {
    pub thinker: NonNull<Thinker>,
    pub sector: DPtr<Sector>,
    pub kind: DoorKind,
    pub topheight: f32,
    pub speed: f32,
    // 1 = up, 0 = waiting, -1 = down
    pub direction: i32,
    // tics to wait at the top
    pub topwait: i32,
    // (keep in case a door going down is reset)
    // when it reaches 0, start going down
    pub topcountdown: i32,
}

impl fmt::Debug for VerticalDoor {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VerticalDoor")
            .field("kind", &self.kind)
            .field("topheight", &self.topheight)
            .field("speed", &self.speed)
            .field("direction", &self.direction)
            .field("topwait", &self.topwait)
            .field("topcountdown", &self.topcountdown)
            .finish()
    }
}

fn get_next_sector(line: DPtr<LineDef>, sector: DPtr<Sector>) -> Option<DPtr<Sector>> {
    if line.flags & LineDefFlags::TwoSided as i16 == 0 {
        return None;
    }

    if line.frontsector == sector {
        return line.backsector.clone();
    }

    Some(line.frontsector.clone())
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

/// P_CrossSpecialLine, trigger various actions when a line is crossed which has
/// a non-zero special attached
pub fn cross_special_line(side: usize, mut line: DPtr<LineDef>, thing: &MapObject) {
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
            debug!("line-special: vld_open door!");
            ev_do_door(line.clone(), DoorKind::vld_open, level);
            line.special = 0;
        }
        3 => {
            debug!("line-special: vld_close door!");
            ev_do_door(line.clone(), DoorKind::vld_close, level);
            line.special = 0;
        }
        4 => {
            debug!("line-special: vld_normal door!");
            ev_do_door(line.clone(), DoorKind::vld_normal, level);
            line.special = 0;
        }
        16 => {
            debug!("line-special: vld_close30ThenOpen door!");
            ev_do_door(line.clone(), DoorKind::vld_close30ThenOpen, level);
            line.special = 0;
        }
        108 => {
            debug!("line-special: vld_blazeRaise door!");
            ev_do_door(line.clone(), DoorKind::vld_blazeRaise, level);
            line.special = 0;
        }
        109 => {
            debug!("line-special: vld_blazeOpen door!");
            ev_do_door(line.clone(), DoorKind::vld_blazeOpen, level);
            line.special = 0;
        }
        110 => {
            debug!("line-special: vld_blazeClose door!");
            ev_do_door(line.clone(), DoorKind::vld_blazeClose, level);
            line.special = 0;
        }
        75 => {
            debug!("line-special: vld_close door!");
            ev_do_door(line.clone(), DoorKind::vld_close, level);
        }
        76 => {
            debug!("line-special: vld_close30ThenOpen door!");
            ev_do_door(line.clone(), DoorKind::vld_close30ThenOpen, level);
        }
        86 => {
            debug!("line-special: vld_open door!");
            ev_do_door(line.clone(), DoorKind::vld_open, level);
        }
        90 => {
            debug!("line-special: vld_normal door!");
            ev_do_door(line.clone(), DoorKind::vld_normal, level);
        }
        105 => {
            debug!("line-special: vld_blazeRaise door!");
            ev_do_door(line.clone(), DoorKind::vld_blazeRaise, level);
        }
        106 => {
            debug!("line-special: vld_blazeOpen door!");
            ev_do_door(line.clone(), DoorKind::vld_blazeOpen, level);
        }
        107 => {
            debug!("line-special: vld_blazeClose door!");
            ev_do_door(line.clone(), DoorKind::vld_blazeClose, level);
        }

        10 => {
            debug!("line-special: downWaitUpStay platform!");
            ev_do_platform(line.clone(), PlatKind::DownWaitUpStay, 0, level);
            line.special = 0;
        }
        22 => {
            debug!("line-special: raiseToNearestAndChange platform!");
            ev_do_platform(line.clone(), PlatKind::RaiseToNearestAndChange, 0, level);
            line.special = 0;
        }
        53 => {
            debug!("line-special: perpetualRaise platform!");
            ev_do_platform(line.clone(), PlatKind::PerpetualRaise, 0, level);
            line.special = 0;
        }
        121 => {
            debug!("line-special: blazeDWUS platform!");
            ev_do_platform(line.clone(), PlatKind::BlazeDWUS, 0, level);
            line.special = 0;
        }
        87 => {
            debug!("line-special: perpetualRaise platform!");
            ev_do_platform(line.clone(), PlatKind::PerpetualRaise, 0, level);
        }
        88 => {
            debug!("line-special: downWaitUpStay platform!");
            ev_do_platform(line.clone(), PlatKind::DownWaitUpStay, 0, level);
        }
        95 => {
            debug!("line-special: raiseToNearestAndChange platform!");
            ev_do_platform(line.clone(), PlatKind::RaiseToNearestAndChange, 0, level);
        }
        120 => {
            debug!("line-special: blazeDWUS platform!");
            ev_do_platform(line.clone(), PlatKind::BlazeDWUS, 0, level);
        }
        5 => {
            debug!("line-special: raiseFloor floor!");
            ev_do_floor(line.clone(), FloorKind::RaiseFloor, level);
            line.special = 0;
        }
        19 => {
            debug!("line-special: lowerFloor floor!");
            ev_do_floor(line.clone(), FloorKind::LowerFloor, level);
            line.special = 0;
        }
        30 => {
            debug!("line-special: raiseToTexture floor!");
            ev_do_floor(line.clone(), FloorKind::RaiseToTexture, level);
            line.special = 0;
        }
        36 => {
            debug!("line-special: downWaitUpStay floor!");
            ev_do_floor(line.clone(), FloorKind::TurboLower, level);
            line.special = 0;
        }
        37 => {
            debug!("line-special: lowerAndChange floor!");
            ev_do_floor(line.clone(), FloorKind::LowerAndChange, level);
            line.special = 0;
        }
        38 => {
            debug!("line-special: lowerFloorToLowest floor!");
            ev_do_floor(line.clone(), FloorKind::LowerFloorToLowest, level);
            line.special = 0;
        }
        56 => {
            debug!("line-special: raiseFloorCrush floor!");
            ev_do_floor(line.clone(), FloorKind::RaiseFloorCrush, level);
            line.special = 0;
        }
        59 => {
            debug!("line-special: raiseFloor24AndChange floor!");
            ev_do_floor(line.clone(), FloorKind::RaiseFloor24andChange, level);
            line.special = 0;
        }
        119 => {
            debug!("line-special: raiseFloorToNearest floor!");
            ev_do_floor(line.clone(), FloorKind::RaiseFloorToNearest, level);
            line.special = 0;
        }
        130 => {
            debug!("line-special: raiseFloorTurbo floor!");
            ev_do_floor(line.clone(), FloorKind::RaiseFloorTurbo, level);
            line.special = 0;
        }
        82 => {
            debug!("line-special: raiseFloorTurbo floor!");
            ev_do_floor(line, FloorKind::LowerFloorToLowest, level);
        }
        83 => {
            debug!("line-special: lowerFloor floor!");
            ev_do_floor(line, FloorKind::LowerFloor, level);
        }
        84 => {
            debug!("line-special: lowerAndChange floor!");
            ev_do_floor(line, FloorKind::LowerAndChange, level);
        }
        91 => {
            debug!("line-special: raiseFloor floor!");
            ev_do_floor(line, FloorKind::RaiseFloor, level);
        }
        92 => {
            debug!("line-special: raiseFloor24 floor!");
            ev_do_floor(line, FloorKind::RaiseFloor24, level);
        }
        93 => {
            debug!("line-special: raiseFloor24AndChange floor!");
            ev_do_floor(line, FloorKind::RaiseFloor24andChange, level);
        }
        94 => {
            debug!("line-special: raiseFloorCrush floor!");
            ev_do_floor(line, FloorKind::RaiseFloorCrush, level);
        }
        96 => {
            debug!("line-special: raiseToTexture floor!");
            ev_do_floor(line, FloorKind::RaiseToTexture, level);
        }
        98 => {
            debug!("line-special: turboLower floor!");
            ev_do_floor(line, FloorKind::TurboLower, level);
        }
        128 => {
            debug!("line-special: raiseFloorToNearest floor!");
            ev_do_floor(line, FloorKind::RaiseFloorToNearest, level);
        }
        129 => {
            debug!("line-special: raiseFloorTurbo floor!");
            ev_do_floor(line, FloorKind::RaiseFloorTurbo, level);
        }
        6 => {
            debug!("line-special: fastCrushAndRaise ceiling!");
            ev_do_ceiling(line.clone(), CeilingKind::fastCrushAndRaise, level);
            line.special = 0;
        }
        25 => {
            debug!("line-special: crushAndRaise ceiling!");
            ev_do_ceiling(line.clone(), CeilingKind::crushAndRaise, level);
            line.special = 0;
        }
        40 => {
            debug!("line-special: raiseToHighest ceiling, floor!");
            ev_do_ceiling(line.clone(), CeilingKind::raiseToHighest, level);
            ev_do_floor(line.clone(), FloorKind::LowerFloorToLowest, level);
            line.special = 0;
        }
        44 => {
            debug!("line-special: lowerAndCrush ceiling!");
            ev_do_ceiling(line.clone(), CeilingKind::lowerAndCrush, level);
            line.special = 0;
        }
        141 => {
            debug!("line-special: silentCrushAndRaise ceiling!");
            ev_do_ceiling(line.clone(), CeilingKind::silentCrushAndRaise, level);
            line.special = 0;
        }
        72 => {
            debug!("line-special: silentCrushAndRaise ceiling!");
            ev_do_ceiling(line.clone(), CeilingKind::lowerAndCrush, level);
        }
        73 => {
            debug!("line-special: crushAndRaise ceiling!");
            ev_do_ceiling(line.clone(), CeilingKind::crushAndRaise, level);
        }
        77 => {
            debug!("line-special: fastCrushAndRaise ceiling!");
            ev_do_ceiling(line.clone(), CeilingKind::fastCrushAndRaise, level);
        }
        52 => {
            level.do_exit_level();
        }
        124 => {
            level.do_secret_exit_level();
        }
        _ => {
            warn!("Invalid or unimplemented line special: {}", line.special);
        }
    }
}
