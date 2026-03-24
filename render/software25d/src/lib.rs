#![allow(clippy::too_many_arguments)]

use self::defs::DrawSeg;
use defs::{MAXDRAWSEGS, PortalClip};
use math::{Angle, Bam};

mod bsp;
mod defs;
mod segs;
mod things;
mod utilities;

pub use bsp::Software25D;

pub(crate) struct RenderData {
    pub rw_angle1: Angle<Bam>,
    pub drawsegs: Vec<DrawSeg>,
    pub portal_clip: PortalClip,
    pub ds_p: usize,
}

impl RenderData {
    pub(crate) fn new(screen_width: usize, screen_height: usize) -> Self {
        Self {
            rw_angle1: Angle::<Bam>::default(),
            drawsegs: Vec::with_capacity(MAXDRAWSEGS),
            ds_p: 0,
            portal_clip: PortalClip::new(screen_width, screen_height),
        }
    }

    pub(crate) fn set_view_height(&mut self, vh: usize) {
        self.portal_clip.set_view_height(vh);
    }

    pub(crate) fn clear_data(&mut self) {
        self.portal_clip.clear();
        self.drawsegs.clear();
        self.ds_p = 0;
        self.rw_angle1 = Angle::<Bam>::default();
    }
}

#[cfg(test)]
mod tests {

    use crate::RenderData;
    use crate::defs::PortalClip;

    #[test]
    fn default_portal_clip() {
        let mut rd = PortalClip::new(640, 400);
        rd.clear();
    }

    #[test]
    fn default_render_data() {
        let mut rd = RenderData::new(640, 400);
        rd.clear_data();
    }
}
