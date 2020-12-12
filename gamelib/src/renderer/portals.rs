//! Vertical clipping for windows/portals, used in Segs render part
//! which will have some of it's function split out to here.

use crate::renderer::defs::{SCREENHEIGHT, SCREENWIDTH};

pub(crate) struct PortalClip {
    /// Clip values are the solid pixel bounding the range.
    ///  floorclip starts out SCREENHEIGHT
    ///  ceilingclip starts out -1
    pub floorclip:   [i32; SCREENWIDTH],
    pub ceilingclip: [i32; SCREENWIDTH],
}

impl PortalClip {
    pub(crate) fn new() -> Self {
        PortalClip {
            floorclip:   [0; SCREENWIDTH],
            ceilingclip: [0; SCREENWIDTH],
        }
    }

    pub(crate) fn clear(&mut self) {
        for i in 0..SCREENWIDTH {
            self.floorclip[i] = SCREENHEIGHT as i32;
            self.ceilingclip[i] = -1;
        }
    }
}

impl Default for PortalClip {
    fn default() -> Self { PortalClip::new() }
}
