/// Implements special effects:
/// Texture animation, height or lighting changes according to adjacent sectors,
/// respective utility functions, etc.
use crate::angle::Angle;
use crate::d_thinker::Thinker;
use crate::info::MapObjectType;
use crate::level_data::map_defs::{LineDef, Sector};
use crate::p_map_object::MapObject;
use crate::DPtr;
use std::ptr::NonNull;
use wad::lumps::WadSector;

// P_LIGHTS
pub struct FireFlicker {
    pub thinker: Option<Thinker>,
    pub sector: NonNull<WadSector>,
    pub count: i32,
    pub max_light: i32,
    pub min_light: i32,
}

pub struct LightFlash {
    pub thinker: Option<Thinker>,
    pub sector: NonNull<WadSector>,
    pub count: i32,
    pub max_light: i32,
    pub min_light: i32,
    pub max_time: i32,
    pub min_time: i32,
}

pub struct Strobe {
    pub thinker: Option<Thinker>,
    pub sector: NonNull<WadSector>,
    pub count: i32,
    pub min_light: i32,
    pub max_light: i32,
    pub dark_time: i32,
    pub bright_time: i32,
}

pub struct Glow {
    pub thinker: Option<Thinker>,
    pub sector: NonNull<WadSector>,
    pub min_light: i32,
    pub max_light: i32,
    pub direction: Angle,
}

// P_PLATS
pub enum PlatEnum {
    up,
    down,
    waiting,
    in_stasis,
}

pub enum PlatType {
    perpetualRaise,
    downWaitUpStay,
    raiseAndChange,
    raiseToNearestAndChange,
    blazeDWUS,
}

pub struct Platform {
    pub thinker: Option<Thinker>,
    pub sector: NonNull<WadSector>,
    pub speed: f32,
    pub low: f32,
    pub high: f32,
    pub wait: i32,
    pub count: i32,
    pub status: PlatEnum,
    pub old_status: PlatEnum,
    pub crush: bool,
    pub tag: i32,
    pub plat_type: PlatType,
}

// P_FLOOR
//
pub enum FloorEnum {
    /// lower floor to highest surrounding floor
    lowerFloor,
    /// lower floor to lowest surrounding floor
    lowerFloorToLowest,
    /// lower floor to highest surrounding floor VERY FAST
    turboLower,
    /// raise floor to lowest surrounding CEILING
    raiseFloor,
    /// raise floor to next highest surrounding floor
    raiseFloorToNearest,
    /// raise floor to shortest height texture around it
    raiseToTexture,
    /// lower floor to lowest surrounding floor
    ///  and change floorpic
    lowerAndChange,
    raiseFloor24,
    raiseFloor24AndChange,
    raiseFloorCrush,
    /// raise to next highest floor, turbo-speed
    raiseFloorTurbo,
    donutRaise,
    raiseFloor512,
}

pub enum StairEnum {
    /// slowly build by 8
    build8,
    /// quickly build by 16
    turbo16,
}

pub struct FloorMove {
    pub thinker: Option<Thinker>,
    pub sector: NonNull<WadSector>,
    kind: FloorEnum,
    pub speed: f32,
    pub crush: bool,
    pub direction: i32,
    pub newspecial: i32,
    pub texture: u8,
    pub floordestheight: f32,
}

// P_CEILNG
//
pub enum CeilingKind {
    lowerToFloor,
    raiseToHighest,
    lowerAndCrush,
    crushAndRaise,
    fastCrushAndRaise,
    silentCrushAndRaise,
}

pub struct CeilingMove {
    pub thinker: Option<Thinker>,
    pub sector: NonNull<WadSector>,
    pub kind: CeilingKind,
    pub bottomheight: f32,
    pub topheight: f32,
    pub speed: f32,
    pub crush: bool,
    // 1 = up, 0 = waiting, -1 = down
    pub direction: i32,
    // ID
    pub tag: i32,
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

/// P_CrossSpecialLine, trigger various actions when a line is crossed which has
/// a non-zero special attached
pub fn cross_special_line(side: i32, line: DPtr<LineDef>, thing: &mut MapObject) {
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

    match line.special {
        2 => {
            //EV_DoDoor(line,open);
            //line.special = 0;
        }
        _ => {}
    }
}
