//! Vertical clipping for windows/portals, used in Segs render part
//! which will have some of it's function split out to here.

use super::defs::{SCREENHEIGHT, SCREENWIDTH};

pub struct PortalClip {
    /// Clip values are the solid pixel bounding the range.
    ///  floorclip starts out SCREENHEIGHT
    ///  ceilingclip starts out -1
    pub floorclip: [f32; SCREENWIDTH],
    pub ceilingclip: [f32; SCREENWIDTH],
}

impl PortalClip {
    pub fn new() -> Self {
        PortalClip {
            floorclip: [0.0; SCREENWIDTH],
            ceilingclip: [0.0; SCREENWIDTH],
        }
    }

    pub(super) fn clear(&mut self) {
        for i in 0..SCREENWIDTH {
            self.floorclip[i] = SCREENHEIGHT as f32;
            self.ceilingclip[i] = -1.0;
        }
    }
}

impl Default for PortalClip {
    fn default() -> Self {
        PortalClip::new()
    }
}
