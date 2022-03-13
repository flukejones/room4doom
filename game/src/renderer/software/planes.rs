use super::defs::{Visplane, MAXOPENINGS, MAXVISPLANES, SCREENHEIGHT, SCREENWIDTH};

pub struct VisPlaneRender {
    // Here comes the obnoxious "visplane".
    pub visplanes: [Visplane; MAXVISPLANES],
    pub lastvisplane: usize,
    /// Index of current visplane in `self.visplanes` for floor
    pub floorplane: usize,
    /// Index of current visplane in `self.visplanes` for ceiling
    pub ceilingplane: usize,

    /// Stores the column number of the texture required for this opening
    pub openings: [f32; MAXOPENINGS],
    pub lastopening: i32,

    pub floorclip: [i32; SCREENWIDTH],
    pub ceilingclip: [i32; SCREENWIDTH],
    /// spanstart holds the start of a plane span
    /// initialized to 0 at start
    pub spanstart: [i32; SCREENHEIGHT],
    pub spanstop: [i32; SCREENHEIGHT],

    //lighttable_t **planezlight;
    pub planeheight: f32,

    pub yslope: [f32; SCREENHEIGHT],
    pub distscale: [f32; SCREENWIDTH],
    pub basexscale: f32,
    pub baseyscale: f32,

    pub cachedheight: [f32; SCREENHEIGHT],
    pub cacheddistance: [f32; SCREENHEIGHT],
    pub cachedxstep: [f32; SCREENHEIGHT],
    pub cachedystep: [f32; SCREENHEIGHT],
}

impl Default for VisPlaneRender {
    fn default() -> Self {
        VisPlaneRender::new()
    }
}

impl VisPlaneRender {
    pub fn new() -> Self {
        VisPlaneRender {
            visplanes: [Visplane::default(); MAXVISPLANES],
            lastvisplane: 0,
            floorplane: 0,
            ceilingplane: 0,
            openings: [f32::MAX; MAXOPENINGS],
            lastopening: 0,
            floorclip: [-1; SCREENWIDTH],
            ceilingclip: [-1; SCREENWIDTH],
            spanstart: [0; SCREENHEIGHT],
            spanstop: [0; SCREENHEIGHT],
            planeheight: 0.0,
            yslope: [0.0; SCREENHEIGHT],
            distscale: [0.0; SCREENWIDTH],
            basexscale: 0.0,
            baseyscale: 0.0,
            cachedheight: [0.0; SCREENHEIGHT],
            cacheddistance: [0.0; SCREENHEIGHT],
            cachedxstep: [0.0; SCREENHEIGHT],
            cachedystep: [0.0; SCREENHEIGHT],
        }
    }

    /// R_ClearPlanes
    /// At begining of frame.
    pub fn clear_planes(&mut self) {
        // opening / clipping determination
        for i in 0..SCREENWIDTH {
            self.floorclip[i] = SCREENHEIGHT as i32;
            self.ceilingclip[i] = -1;
        }

        self.lastvisplane = 0;
        self.lastopening = 0;

        // texture calculation
        for i in self.cachedheight.iter_mut() {
            *i = 0.0;
        }

        // left to right mapping
        // TODO: angle = (viewangle - ANG90) >> ANGLETOFINESHIFT;

        // TODO: Don't hardcode this; centerxfrac
        // scale will be unit scale at SCREENWIDTH/2 distance
        self.basexscale = (160.0f32).cos();
        self.baseyscale = -(160.0f32).sin();
    }

    fn current_floor_plane(&self) -> &Visplane {
        &self.visplanes[self.floorplane]
    }

    fn current_ceiling_plane(&self) -> &Visplane {
        &self.visplanes[self.ceilingplane]
    }

    // R_CheckPlane
    //pub fn check_set_floor_plane
}
