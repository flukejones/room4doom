use std::{fmt::Debug, ptr::NonNull};

use gameplay::{Angle, Segment};

pub const SIL_NONE: i32 = 0;
pub const SIL_BOTTOM: i32 = 1;
pub const SIL_TOP: i32 = 2;
pub const SIL_BOTH: i32 = 3;

pub const SCREENWIDTH: usize = 1024;

pub const MAXDRAWSEGS: usize = 1024;

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
    pub sprtopclip: Option<i32>,
    /// Index in to visplanes.floorclip
    pub sprbottomclip: Option<i32>,

    /// Keeps an index that is used to index in to `openings`
    pub maskedtexturecol: i32,
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
            maskedtexturecol: 0,
        }
    }
}

#[derive(Copy, Clone)]
pub struct ClipRange {
    pub first: f32,
    pub last: f32,
}

/// Now what is a visplane, anyway?
#[derive(Copy, Clone)]
pub struct Visplane {
    pub height: f32,
    pub picnum: usize,
    pub lightlevel: i32,
    pub minx: f32,
    pub maxx: f32,
    /// Here lies the rub for all
    ///  dynamic resize/change of resolution.
    pub top: [f32; SCREENWIDTH + 1],
    /// See above.
    pub bottom: [f32; SCREENWIDTH + 1],

    pub basexscale: f32,
    pub baseyscale: f32,
    pub view_angle: Angle,
}

impl Debug for Visplane {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Visplane")
            .field("height", &self.height)
            .field("picnum", &self.picnum)
            .field("lightlevel", &self.lightlevel)
            .field("minx", &self.minx)
            .field("maxx", &self.maxx)
            .field(
                "top",
                &self
                    .top
                    .into_iter()
                    .map(|d| {
                        let mut d = d.to_string();
                        d.push(',');
                        d
                    })
                    .collect::<String>(),
            )
            .field(
                "bottom",
                &self
                    .bottom
                    .into_iter()
                    .map(|d| {
                        let mut d = d.to_string();
                        d.push(',');
                        d
                    })
                    .collect::<String>(),
            )
            .finish()
    }
}

impl Default for Visplane {
    fn default() -> Self {
        Visplane {
            height: 0.0,
            picnum: 0,
            lightlevel: 0,
            minx: 0.0,
            maxx: 0.0,
            top: [f32::MAX; SCREENWIDTH + 1],
            bottom: [0.0; SCREENWIDTH + 1],

            basexscale: 0.0,
            baseyscale: 0.0,
            view_angle: Angle::default(),
        }
    }
}

impl Visplane {
    pub fn clear(&mut self) {
        self.height = 0.0;
        self.picnum = 0;
        self.lightlevel = 0;
        self.picnum = 0;
        self.minx = 0.0;
        self.maxx = 0.0;

        for x in self.top.iter_mut() {
            *x = f32::MAX;
        }

        for x in self.bottom.iter_mut() {
            *x = f32::MIN;
        }
    }
}
