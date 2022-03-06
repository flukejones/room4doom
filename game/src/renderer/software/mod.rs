use doom_lib::{Angle, TextureData};
use wad::WadData;

use self::{defs::DrawSeg, portals::PortalClip};

pub mod bsp;
pub mod defs;
pub mod plane;
pub mod portals;
pub mod segs;
pub mod things;

/// We store most of what is needed for rendering in various functions here to avoid
/// having to pass too many things in args through multiple function calls. This
/// is due to the Doom C relying a fair bit on global state.
///
/// `RenderData` will be passed to the sprite drawer/clipper to use `drawsegs`
/// ----------------------------------------------------------------------------
/// - R_DrawSprite, r_things.c
/// - R_DrawMasked, r_things.c
/// - R_StoreWallRange, r_segs.c, checks only for overflow of drawsegs, and uses *one* entry through ds_p
///                               it then inserts/incs pointer to next drawseg in the array when finished
/// - R_DrawPlanes, r_plane.c, checks only for overflow of drawsegs
#[derive(Default)]
pub struct RenderData {
    pub rw_angle1: Angle,
    // DrawSeg used, which is inserted in drawsegs at end of r_segs
    pub drawsegs: Vec<DrawSeg>,
    pub portal_clip: PortalClip,
    /// index to drawsegs
    /// Used in r_segs and r_things
    pub ds_p: usize, // Or, depending on place in code this can be skipped and a new

    /// Texturre data including colourmaps and lightlevels
    pub texture_data: TextureData,
}

impl RenderData {
    pub fn new(wad: &WadData) -> Self {
        Self {
            rw_angle1: Angle::default(),
            drawsegs: Vec::new(),
            portal_clip: PortalClip::default(),
            ds_p: 0,
            texture_data: TextureData::new(wad),
        }
    }

    pub fn clear_data(&mut self) {
        self.portal_clip.clear();
        self.drawsegs.clear();
        self.ds_p = 0;
        self.rw_angle1 = Angle::default();
    }
}
