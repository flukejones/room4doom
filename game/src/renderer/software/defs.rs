use std::ptr::NonNull;

use doom_lib::Segment;

pub const SIL_NONE: i32 = 0;
pub const SIL_BOTTOM: i32 = 1;
pub const SIL_TOP: i32 = 2;
pub const SIL_BOTH: i32 = 3;

pub const SCREENWIDTH: usize = 320;
pub const SCREENHEIGHT: usize = 200;
pub const SCREENHEIGHT_HALF: usize = SCREENHEIGHT / 2;

pub const MAXDRAWSEGS: usize = 1024;

pub const MAXVISPLANES: usize = 256;

pub const MAXOPENINGS: usize = SCREENWIDTH * 128;

#[derive(Debug, Clone, Copy)]
pub struct DrawSeg {
    pub curline: NonNull<Segment>,
    pub x1: i32,
    pub x2: i32,

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
            x1: 0,
            x2: 0,
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
    pub first: i32,
    pub last: i32,
}

/// Now what is a visplane, anyway?
#[derive(Copy, Clone)]
pub struct Visplane {
    pub height: f32,
    pub picnum: i32,
    pub lightlevel: i32,
    pub minx: i32,
    pub maxx: i32,
    /// leave pads for [minx-1]/[maxx+1]
    pub pad1: u8,
    // TODO: resolution stuff
    /// Here lies the rub for all
    ///  dynamic resize/change of resolution.
    pub top: [u8; SCREENWIDTH],
    pub pad2: u8,
    pub pad3: u8,
    /// See above.
    pub bottom: [u8; SCREENWIDTH],
    pub pad4: u8,
}

impl Default for Visplane {
    fn default() -> Self {
        Visplane {
            height: 0.0,
            picnum: 0,
            lightlevel: 0,
            minx: 0,
            maxx: 0,
            pad1: 0,
            top: [0; SCREENWIDTH],
            pad2: 0,
            pad3: 0,
            bottom: [0; SCREENWIDTH],
            pad4: 0,
        }
    }
}

impl Visplane {
    pub fn clear(&mut self) {
        self.height = 0.0;
        self.picnum = 0;
        self.lightlevel = 0;
        self.picnum = 0;
        self.minx = 0;
        self.maxx = 0;
        self.pad1 = 0;
        self.pad2 = 0;
        self.pad3 = 0;
        self.pad4 = 0;

        for x in self.top.iter_mut() {
            *x = 0;
        }

        for x in self.bottom.iter_mut() {
            *x = 0;
        }
    }
}
