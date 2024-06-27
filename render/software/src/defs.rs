use std::fmt::Debug;
use std::ptr::NonNull;

use gameplay::Segment;

pub const SIL_NONE: i32 = 0;
pub const SIL_BOTTOM: i32 = 1;
pub const SIL_TOP: i32 = 2;
pub const SIL_BOTH: i32 = 3;

pub const MAXDRAWSEGS: usize = 1024 * 2;

#[derive(Debug, Clone, Copy)]
pub struct DrawSeg {
    pub curline: NonNull<Segment>,
    pub x1: f32,
    pub x2: f32,

    pub scale1: f32,
    pub scale2: f32,
    pub scalestep: f32,

    /// 0=none, 1=bottom, 2=top, 3=both
    pub silhouette: i32,

    /// do not clip sprites above this
    pub bsilheight: f32,

    /// do not clip sprites below this
    pub tsilheight: f32,

    // TODO: Pointers to lists for sprite clipping,
    //  all three adjusted so [x1] is first value.
    /// Index in to visplanes.ceilingclip
    pub sprtopclip: Option<f32>,
    /// Index in to visplanes.floorclip
    pub sprbottomclip: Option<f32>,

    /// Keeps an index that is used to index in to `openings`
    pub maskedtexturecol: f32,
}

impl DrawSeg {
    pub fn new(seg: NonNull<Segment>) -> Self {
        DrawSeg {
            curline: seg,
            x1: 0.0,
            x2: 0.0,
            scale1: 0.0,
            scale2: 0.0,
            scalestep: 0.0,
            silhouette: 0,
            bsilheight: 0.0,
            tsilheight: 0.0,
            sprtopclip: None,
            sprbottomclip: None,
            maskedtexturecol: 0.0,
        }
    }
}

/// The range of columns on the screen clipped against
#[derive(Copy, Clone)]
pub struct ClipRange {
    /// Leftmost starting pixel/column
    pub first: f32,
    /// Rightmost ending pixel/column
    pub last: f32,
}
