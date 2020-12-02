///	Implements special effects:
///	Texture animation, height or lighting changes according to adjacent sectors,
/// respective utility functions, etc.
use crate::angle::Angle;
use crate::d_thinker::Thinker;
use std::ptr::NonNull;
use wad::lumps::Sector;

// P_LIGHTS
pub(crate) struct FireFlicker {
    pub thinker:   Option<NonNull<Thinker<FireFlicker>>>,
    pub sector:    NonNull<Sector>,
    pub count:     i32,
    pub max_light: i32,
    pub min_light: i32,
}

pub(crate) struct LightFlash {
    pub thinker:   Option<NonNull<Thinker<LightFlash>>>,
    pub sector:    NonNull<Sector>,
    pub count:     i32,
    pub max_light: i32,
    pub min_light: i32,
    pub max_time:  i32,
    pub min_time:  i32,
}

pub(crate) struct Strobe {
    pub thinker:     Option<NonNull<Thinker<Strobe>>>,
    pub sector:      NonNull<Sector>,
    pub count:       i32,
    pub min_light:   i32,
    pub max_light:   i32,
    pub dark_time:   i32,
    pub bright_time: i32,
}

pub(crate) struct Glow {
    pub thinker:   Option<NonNull<Thinker<Glow>>>,
    pub sector:    NonNull<Sector>,
    pub min_light: i32,
    pub max_light: i32,
    pub direction: Angle,
}

// P_PLATS
pub(crate) enum PlatEnum {
    up,
    down,
    waiting,
    in_stasis,
}

pub(crate) enum PlatType {
    perpetualRaise,
    downWaitUpStay,
    raiseAndChange,
    raiseToNearestAndChange,
    blazeDWUS,
}

pub(crate) struct Platform {
    pub thinker:    Option<NonNull<Thinker<Platform>>>,
    pub sector:     NonNull<Sector>,
    pub speed:      f32,
    pub low:        f32,
    pub high:       f32,
    pub wait:       i32,
    pub count:      i32,
    pub status:     PlatEnum,
    pub old_status: PlatEnum,
    pub crush:      bool,
    pub tag:        i32,
    pub plat_type:  PlatType,
}

// P_FLOOR
//
pub(crate) enum FloorEnum {
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

pub(crate) enum StairEnum {
    /// slowly build by 8
    build8,
    /// quickly build by 16
    turbo16,
}

pub(crate) struct FloorMove {
    pub thinker:         Option<NonNull<Thinker<FloorMove>>>,
    pub sector:          NonNull<Sector>,
    kind:                FloorEnum,
    pub speed:           f32,
    pub crush:           bool,
    pub direction:       i32,
    pub newspecial:      i32,
    pub texture:         u8,
    pub floordestheight: f32,
}

// P_CEILNG
//
pub(crate) enum CeilingKind {
    lowerToFloor,
    raiseToHighest,
    lowerAndCrush,
    crushAndRaise,
    fastCrushAndRaise,
    silentCrushAndRaise,
}

pub(crate) struct CeilingMove {
    pub thinker:      Option<NonNull<Thinker<FloorMove>>>,
    pub sector:       NonNull<Sector>,
    pub kind:         CeilingKind,
    pub bottomheight: f32,
    pub topheight:    f32,
    pub speed:        f32,
    pub crush:        bool,
    // 1 = up, 0 = waiting, -1 = down
    pub direction:    i32,
    // ID
    pub tag:          i32,
    pub olddirection: i32,
}
