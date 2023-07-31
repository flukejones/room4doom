//! Vertical clipping for windows/portals, used in Segs render part
//! which will have some of it's function split out to here.

pub struct PortalClip {
    /// Clip values are the solid pixel bounding the range.
    ///  floorclip starts out SCREENHEIGHT
    ///  ceilingclip starts out -1
    pub floorclip: Vec<f32>,
    pub ceilingclip: Vec<f32>,
    screen_width: usize,
    screen_height: usize,
}

impl PortalClip {
    pub fn new(screen_width: usize, screen_height: usize) -> Self {
        PortalClip {
            floorclip: vec![0.0; screen_width],
            ceilingclip: vec![0.0; screen_width],
            screen_width,
            screen_height,
        }
    }

    pub(super) fn clear(&mut self) {
        self.floorclip.fill(self.screen_height as f32);
        self.ceilingclip.fill(-1.0);
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
