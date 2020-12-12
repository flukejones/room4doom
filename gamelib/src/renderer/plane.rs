use crate::renderer::defs::{
    Visplane, MAXOPENINGS, MAXVISPLANES, SCREENHEIGHT, SCREENWIDTH,
};

pub(crate) struct VisPlaneCtrl {
    // Here comes the obnoxious "visplane".
    pub visplanes:    [Visplane; MAXVISPLANES],
    pub lastvisplane: usize,
    pub floorplane:   usize,
    pub ceilingplane: usize,

    // ?
    pub openings:    [i16; MAXOPENINGS],
    pub lastopening: usize,

    /// spanstart holds the start of a plane span
    /// initialized to 0 at start
    pub spanstart: [i32; SCREENHEIGHT],
    pub spanstop:  [i32; SCREENHEIGHT],

    //lighttable_t **planezlight;
    pub planeheight: f32,

    pub yslope:     [f32; SCREENHEIGHT],
    pub distscale:  [f32; SCREENWIDTH],
    pub basexscale: f32,
    pub baseyscale: f32,

    pub cachedheight:   [f32; SCREENHEIGHT],
    pub cacheddistance: [f32; SCREENHEIGHT],
    pub cachedxstep:    [f32; SCREENHEIGHT],
    pub cachedystep:    [f32; SCREENHEIGHT],
}

impl VisPlaneCtrl {
    pub(crate) fn new() -> Self {
        VisPlaneCtrl {
            visplanes:      [Visplane::default(); MAXVISPLANES],
            lastvisplane:   0,
            floorplane:     0,
            ceilingplane:   0,
            openings:       [0; MAXOPENINGS],
            lastopening:    0,
            spanstart:      [0; SCREENHEIGHT],
            spanstop:       [0; SCREENHEIGHT],
            planeheight:    0.0,
            yslope:         [0.0; SCREENHEIGHT],
            distscale:      [0.0; SCREENWIDTH],
            basexscale:     0.0,
            baseyscale:     0.0,
            cachedheight:   [0.0; SCREENHEIGHT],
            cacheddistance: [0.0; SCREENHEIGHT],
            cachedxstep:    [0.0; SCREENHEIGHT],
            cachedystep:    [0.0; SCREENHEIGHT],
        }
    }
}
