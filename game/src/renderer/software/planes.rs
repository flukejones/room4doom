use std::f32::consts::FRAC_PI_2;

use doom_lib::{Angle, TextureData};
use sdl2::{rect::Rect, render::Canvas, surface::Surface};

use crate::utilities::CLASSIC_SCREEN_X_TO_VIEW;

use super::defs::{Visplane, MAXOPENINGS, SCREENHEIGHT, SCREENWIDTH};

pub const MAXVISPLANES: usize = 512;

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
            floorclip: [SCREENHEIGHT as i32; SCREENWIDTH],
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
    pub fn clear_planes(&mut self, view_angle: Angle) {
        // opening / clipping determination
        for i in 0..SCREENWIDTH {
            self.floorclip[i] = SCREENHEIGHT as i32;
            self.ceilingclip[i] = -1;
        }

        for p in self.visplanes.iter_mut() {
            p.clear();
        }

        self.lastvisplane = 0;
        self.lastopening = 0;

        // texture calculation
        for i in self.cachedheight.iter_mut() {
            *i = 0.0;
        }

        // left to right mapping
        // TODO: angle = (viewangle - ANG90) >> ANGLETOFINESHIFT;
        self.basexscale = (view_angle.rad() - FRAC_PI_2).cos() / SCREENWIDTH as f32;
        self.baseyscale =
            -(SCREENWIDTH as f32 / (view_angle.rad() - FRAC_PI_2).sin() / SCREENWIDTH as f32);
    }

    /// Find a plane matching height, picnum, light level. Otherwise return a new plane.
    pub fn find_plane<'a>(
        &'a mut self,
        mut height: u32,
        picnum: usize,
        skynum: usize,
        mut light_level: u32,
    ) -> usize {
        if picnum == skynum {
            height = 0;
            light_level = 0;
        }

        let len = self.visplanes.len();

        for i in 0..self.lastvisplane {
            if height == self.visplanes[i].height
                && picnum == self.visplanes[i].picnum
                && light_level == self.visplanes[i].lightlevel
            {
                return i;
            }
        }

        if self.lastvisplane < len - 1 {
            //panic!("SHIT");
            self.lastvisplane += 1;
        }

        // Otherwise edit new
        let mut check = &mut self.visplanes[self.lastvisplane];
        check.height = height;
        check.picnum = picnum;
        check.lightlevel = light_level;
        check.minx = SCREENWIDTH as i32;
        check.maxx = -1;
        for t in &mut check.top {
            *t = 0xff;
        }

        self.lastvisplane
    }

    /// Check if this plane should be used, otherwise use a new plane.
    pub fn check_plane<'a>(&'a mut self, start: i32, stop: i32, plane_idx: usize) -> usize {
        let plane = &mut self.visplanes[plane_idx];

        let (intrl, unionl) = if start < plane.minx {
            (plane.minx, start)
        } else {
            (start, plane.minx)
        };

        let (intrh, unionh) = if stop > plane.maxx {
            (plane.maxx, stop)
        } else {
            (stop, plane.maxx)
        };

        let mut x = intrl;

        for i in intrl..=intrh + 1 {
            if i > intrh {
                x = i;
            }
            if plane.top[i as usize] != 0xff {
                break;
            }
        }

        if x > intrh {
            plane.minx = unionl;
            plane.maxx = unionh;
            // Use the same plane
            return plane_idx;
        }

        // Otherwise make a new plane
        let height = plane.height;
        let picnum = plane.picnum;
        let lightlevel = plane.lightlevel;

        self.lastvisplane += 1;
        let plane = &mut self.visplanes[self.lastvisplane];
        plane.height = height;
        plane.picnum = picnum;
        plane.lightlevel = lightlevel;

        if self.lastvisplane == self.visplanes.len() - 1 {
            panic!("No more visplanes");
        }

        self.lastvisplane += 1;
        let plane = &mut self.visplanes[self.lastvisplane];
        plane.minx = start;
        plane.maxx = stop;
        plane.height = height;
        plane.picnum = picnum;
        plane.lightlevel = lightlevel;

        for t in &mut plane.top {
            *t = 0xff;
        }

        self.lastvisplane
    }
}

pub fn make_spans(
    x: i32,
    mut t1: i32,
    mut b1: i32,
    mut t2: i32,
    mut b2: i32,
    plane_height: i32,
    basexscale: f32,
    baseyscale: f32,
    view_angle: Angle,
    span_start: &mut [i32; SCREENWIDTH],
    canvas: &mut Canvas<Surface>,
    r: u8,
) {
    while t1 < t2 && t1 <= b1 {
        map_plane(
            t1,
            span_start[t1 as usize],
            x - 1,
            plane_height,
            basexscale,
            baseyscale,
            view_angle,
            canvas,
            r,
        );
        t1 += 1;
    }

    while b1 > b2 && b1 >= t1 {
        map_plane(
            b1,
            span_start[b1 as usize],
            x - 1,
            plane_height,
            basexscale,
            baseyscale,
            view_angle,
            canvas,
            r,
        );
        b1 -= 1;
    }

    while t2 < t1 && t2 <= b2 {
        span_start[t2 as usize] = x;
        t2 += 1;
    }

    while b2 > b1 && b2 >= t2 {
        span_start[b2 as usize] = x;
        b2 -= 1;
    }
}

fn map_plane(
    y: i32,
    x1: i32,
    x2: i32,
    plane_height: i32,
    basexscale: f32,
    baseyscale: f32,
    view_angle: Angle,
    canvas: &mut Canvas<Surface>,
    r: u8,
) {
    // TODO: maybe cache?
    let distance = plane_height * y / 1000; // TODO: yslope
    let ds_xstep = distance as f32 * basexscale;
    let ds_ystep = distance as f32 * baseyscale;

    let length = distance as f32 * 0.5; // TODO: distscale table
    let angle = view_angle + CLASSIC_SCREEN_X_TO_VIEW[x1 as usize];
    let ds_xfrac = view_angle.unit().x() + angle.cos() * length;
    let ds_yfrac = view_angle.unit().y() + angle.sin() * length;

    let ds_y = y;
    let ds_x1 = x1;
    let ds_x2 = x2;

    let mut ds = DrawSpan::new(
        // texture_column,
        // colourmap,
        ds_xstep, ds_ystep, ds_xfrac, ds_yfrac, ds_y, ds_x1, ds_x2,
    );
    ds.draw_(canvas, r);
}

pub struct DrawSpan {
    // texture_column: &'a [usize],
    // colourmap: &'a [usize],
    ds_xstep: f32,
    ds_ystep: f32,
    ds_xfrac: f32,
    ds_yfrac: f32,
    ds_y: i32,
    ds_x1: i32,
    ds_x2: i32,
}

impl DrawSpan {
    pub fn new(
        // texture_column: &'a [usize],
        // colourmap: &'a [usize],
        ds_xstep: f32,
        ds_ystep: f32,
        ds_xfrac: f32,
        ds_yfrac: f32,
        ds_y: i32,
        ds_x1: i32,
        ds_x2: i32,
    ) -> Self {
        Self {
            // texture_column,
            // colourmap,
            ds_xstep,
            ds_ystep,
            ds_xfrac,
            ds_yfrac,
            ds_y,
            ds_x1,
            ds_x2,
        }
    }

    //fn draw_(&mut self, textures: &TextureData, canvas: &mut Canvas<Surface>) {
    fn draw_(&mut self, canvas: &mut Canvas<Surface>, r: u8) {
        let colour = sdl2::pixels::Color::RGBA((50 as u32 + r as u32) as u8, 20, 20, 255);
        canvas.set_draw_color(colour);

        let mut count = self.ds_x2 - self.ds_x1;
        while count != -1 {
            canvas
                .fill_rect(Rect::new(self.ds_x1, self.ds_y, 1, 1))
                .unwrap();
            count -= 1;
            self.ds_x1 += 1;
        }
    }
}
