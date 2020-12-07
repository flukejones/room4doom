use std::ptr::NonNull;

use wad::lumps::Segment;

pub const MAXDRAWSEGS: usize = 256;

pub(crate) struct DrawSeg {
    pub curline: NonNull<Segment>,
    pub x1:      i32,
    pub x2:      i32,

    pub scale1:    f32,
    pub scale2:    f32,
    pub scalestep: f32,

    /// 0=none, 1=bottom, 2=top, 3=both
    pub silhouette: i32,

    /// do not clip sprites above this
    pub bsilheight: f32,

    /// do not clip sprites below this
    pub tsilheight: f32,

    // TODO: Pointers to lists for sprite clipping,
    //  all three adjusted so [x1] is first value.
    pub sprtopclip:       i16,
    pub sprbottomclip:    i16,
    pub maskedtexturecol: i16,
}

impl DrawSeg {
    pub fn new(seg: NonNull<Segment>) -> Self {
        DrawSeg {
            curline:          seg,
            x1:               0,
            x2:               0,
            scale1:           0.0,
            scale2:           0.0,
            scalestep:        0.0,
            silhouette:       0,
            bsilheight:       0.0,
            tsilheight:       0.0,
            sprtopclip:       0,
            sprbottomclip:    0,
            maskedtexturecol: 0,
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub(crate) struct ClipRange {
    pub first: i32,
    pub last:  i32,
}
