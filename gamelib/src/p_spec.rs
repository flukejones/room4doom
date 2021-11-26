///	Implements special effects:
///	Texture animation, height or lighting changes according to adjacent sectors,
/// respective utility functions, etc.
use crate::angle::Angle;
use crate::d_thinker::Thinker;
use std::ptr::NonNull;
use wad::lumps::WadSector;

// P_LIGHTS
pub struct FireFlicker {
    pub thinker: Option<NonNull<Thinker<FireFlicker>>>,
    pub sector: NonNull<WadSector>,
    pub count: i32,
    pub max_light: i32,
    pub min_light: i32,
}

pub struct LightFlash {
    pub thinker: Option<NonNull<Thinker<LightFlash>>>,
    pub sector: NonNull<WadSector>,
    pub count: i32,
    pub max_light: i32,
    pub min_light: i32,
    pub max_time: i32,
    pub min_time: i32,
}

pub struct Strobe {
    pub thinker: Option<NonNull<Thinker<Strobe>>>,
    pub sector: NonNull<WadSector>,
    pub count: i32,
    pub min_light: i32,
    pub max_light: i32,
    pub dark_time: i32,
    pub bright_time: i32,
}

pub struct Glow {
    pub thinker: Option<NonNull<Thinker<Glow>>>,
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
    pub thinker: Option<NonNull<Thinker<Platform>>>,
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
    pub thinker: Option<NonNull<Thinker<FloorMove>>>,
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
    pub thinker: Option<NonNull<Thinker<FloorMove>>>,
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
