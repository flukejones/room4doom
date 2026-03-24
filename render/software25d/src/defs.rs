use std::fmt::Debug;
use std::ptr::NonNull;

use level::Segment;
use math::FixedT;

pub const SIL_NONE: i32 = 0;
pub const SIL_BOTTOM: i32 = 1;
pub const SIL_TOP: i32 = 2;
pub const SIL_BOTH: i32 = 3;

pub const MAXDRAWSEGS: usize = 1024 * 2;

#[derive(Debug, Clone, Copy)]
pub struct DrawSeg {
    pub curline: NonNull<Segment>,
    pub x1: FixedT,
    pub x2: FixedT,

    pub scale1: FixedT,
    pub scale2: FixedT,
    pub scalestep: FixedT,

    /// 0=none, 1=bottom, 2=top, 3=both
    pub silhouette: i32,

    /// do not clip sprites above this
    pub bsilheight: FixedT,

    /// do not clip sprites below this
    pub tsilheight: FixedT,

    // TODO: Pointers to lists for sprite clipping,
    //  all three adjusted so [x1] is first value.
    pub sprtopclip: Option<FixedT>,
    pub sprbottomclip: Option<FixedT>,

    /// Keeps an index that is used to index in to `openings`
    pub maskedtexturecol: FixedT,
}

impl DrawSeg {
    pub fn new(seg: NonNull<Segment>) -> Self {
        DrawSeg {
            curline: seg,
            x1: FixedT::ZERO,
            x2: FixedT::ZERO,
            scale1: FixedT::ZERO,
            scale2: FixedT::ZERO,
            scalestep: FixedT::ZERO,
            silhouette: 0,
            bsilheight: FixedT::ZERO,
            tsilheight: FixedT::ZERO,
            sprtopclip: None,
            sprbottomclip: None,
            maskedtexturecol: FixedT::ZERO,
        }
    }
}

/// A contiguous range of screen columns fully occluded by solid walls.
#[derive(Copy, Clone)]
pub struct ClipRange {
    /// Leftmost starting pixel/column
    pub first: FixedT,
    /// Rightmost ending pixel/column
    pub last: FixedT,
}

/// Per-column floor/ceiling clip arrays for portal (two-sided) rendering.
pub struct PortalClip {
    /// Clip values are the solid pixel bounding the range.
    ///  floorclip starts out view_height (render area, not full buffer)
    ///  ceilingclip starts out -1
    pub floorclip: Vec<FixedT>,
    pub ceilingclip: Vec<FixedT>,
    view_height: usize,
}

impl PortalClip {
    pub fn new(screen_width: usize, view_height: usize) -> Self {
        PortalClip {
            floorclip: vec![FixedT::ZERO; screen_width + 1],
            ceilingclip: vec![FixedT::ZERO; screen_width + 1],
            view_height,
        }
    }

    pub(super) fn set_view_height(&mut self, vh: usize) {
        self.view_height = vh;
    }

    pub(super) fn clear(&mut self) {
        self.floorclip
            .fill(FixedT::from(self.view_height as i32 + 1));
        self.ceilingclip.fill(FixedT::from(-1));
    }
}

#[cfg(test)]
mod tests {
    use super::PortalClip;

    #[test]
    fn default_portal_clip() {
        let mut rd = PortalClip::new(640, 400);
        rd.clear();
    }
}
