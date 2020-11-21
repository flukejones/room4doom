///	Implements special effects:
///	Texture animation, height or lighting changes according to adjacent sectors,
/// respective utility functions, etc.
use crate::angle::Angle;
use crate::d_thinker::Thinker;
use std::ptr::NonNull;
use wad::lumps::Sector;

// P_LIGHTS
//
pub struct Fire_Flicker<'p> {
    pub thinker:   Option<NonNull<Thinker<'p, Fire_Flicker<'p>>>>,
    pub sector:    NonNull<Sector>,
    pub count:     i32,
    pub max_light: i32,
    pub min_light: i32,
}

pub struct Light_Flash<'p> {
    pub thinker:   Option<NonNull<Thinker<'p, Light_Flash<'p>>>>,
    pub sector:    NonNull<Sector>,
    pub count:     i32,
    pub max_light: i32,
    pub min_light: i32,
    pub max_time:  i32,
    pub min_time:  i32,
}

pub struct Strobe<'p> {
    pub thinker:     Option<NonNull<Thinker<'p, Strobe<'p>>>>,
    pub sector:      NonNull<Sector>,
    pub count:       i32,
    pub min_light:   i32,
    pub max_light:   i32,
    pub dark_time:   i32,
    pub bright_time: i32,
}

pub struct Glow<'p> {
    pub thinker:   Option<NonNull<Thinker<'p, Glow<'p>>>>,
    pub sector:    NonNull<Sector>,
    pub min_light: i32,
    pub max_light: i32,
    pub direction: Angle,
}

// P_PLATS
//
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

struct Platform<'p> {
    pub thinker:    Option<NonNull<Thinker<'p, Platform<'p>>>>,
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

pub struct FloorMove<'p> {
    pub thinker:         Option<NonNull<Thinker<'p, FloorMove<'p>>>>,
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
pub enum CeilingKind {
    lowerToFloor,
    raiseToHighest,
    lowerAndCrush,
    crushAndRaise,
    fastCrushAndRaise,
    silentCrushAndRaise,
}

pub struct CeilingMove<'p> {
    pub thinker:      Option<NonNull<Thinker<'p, FloorMove<'p>>>>,
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
