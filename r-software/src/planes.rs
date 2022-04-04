use std::f32::consts::FRAC_PI_2;

use crate::utilities::CLASSIC_SCREEN_X_TO_VIEW;
use doom_lib::{Angle, FlatPic, PicData};
use glam::Vec2;
use sdl2::{rect::Rect, render::Canvas, surface::Surface};

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
    pub openings: [i32; MAXOPENINGS],
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

impl VisPlaneRender {
    pub fn new() -> Self {
        VisPlaneRender {
            visplanes: [Visplane::default(); MAXVISPLANES],
            lastvisplane: 0,
            floorplane: 0,
            ceilingplane: 0,
            openings: [i32::MAX; MAXOPENINGS],
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
        self.basexscale = (view_angle - FRAC_PI_2).cos() / (SCREENWIDTH / 2) as f32;
        self.baseyscale = -((view_angle - FRAC_PI_2).sin() / (SCREENWIDTH / 2) as f32);
    }

    /// Find a plane matching height, picnum, light level. Otherwise return a new plane.
    pub fn find_plane(
        &mut self,
        mut height: i32,
        picnum: usize,
        skynum: usize,
        mut light_level: i32,
    ) -> usize {
        if picnum == skynum {
            height = 0;
            light_level = 0;
        }

        let len = self.visplanes.len();

        for (index, plane) in self.visplanes[0..self.lastvisplane].iter().enumerate() {
            if height == plane.height && picnum == plane.picnum && light_level == plane.lightlevel {
                return index;
            }
        }

        if self.lastvisplane < len - 1 {
            self.lastvisplane += 1;
        } else {
            panic!("Out of visplanes");
        }

        // Otherwise edit new
        let mut check = &mut self.visplanes[self.lastvisplane];
        check.height = height;
        check.picnum = picnum;
        check.lightlevel = light_level;
        check.minx = SCREENWIDTH as i32;
        check.maxx = 0;
        for t in &mut check.top {
            *t = 0xff;
        }

        self.lastvisplane
    }

    /// Check if this plane should be used, otherwise use a new plane.
    pub fn check_plane(&mut self, start: i32, stop: i32, plane_idx: usize) -> usize {
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

        // if intrh <= intrl {
        //     plane.minx = unionl;
        //     plane.maxx = unionh;
        //     return plane_idx;
        // }

        for i in intrl..=320 {
            if i >= intrh {
                plane.minx = unionl;
                plane.maxx = unionh;
                // Use the same plane
                return plane_idx;
            }
            if plane.top[i as usize] != 0xff {
                break;
            }
        }

        // Otherwise make a new plane
        let height = plane.height;
        let picnum = plane.picnum;
        let lightlevel = plane.lightlevel;

        if self.lastvisplane == self.visplanes.len() - 1 {
            panic!("No more visplanes: used {}", self.lastvisplane);
        }

        self.lastvisplane += 1;
        let plane = &mut self.visplanes[self.lastvisplane];
        plane.height = height;
        plane.picnum = picnum;
        plane.lightlevel = lightlevel;
        plane.minx = start;
        plane.maxx = stop;

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
    viewxy: Vec2,
    viewz: f32,
    plane: &Visplane,
    span_start: &mut [i32; SCREENWIDTH],
    texture_data: &PicData,
    canvas: &mut Canvas<Surface>,
) {
    while t1 < t2 && t1 <= b1 {
        map_plane(
            t1,
            span_start[t1 as usize],
            x - 1,
            viewxy,
            viewz,
            plane,
            texture_data,
            canvas,
        );
        t1 += 1;
    }

    while b1 > b2 && b1 >= t1 {
        map_plane(
            b1,
            span_start[b1 as usize],
            x - 1,
            viewxy,
            viewz,
            plane,
            texture_data,
            canvas,
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
    viewxy: Vec2,
    viewz: f32,
    plane: &Visplane,
    texture_data: &PicData,
    canvas: &mut Canvas<Surface>,
) {
    let planeheight = (plane.height as f32 - viewz).floor().abs();
    // TODO: maybe cache?
    let dy = (y as f32 - SCREENHEIGHT as f32 / 2.0) + 0.5; // OK
    let yslope = (SCREENWIDTH as f32 / 2.0) / dy.abs(); // OK
    let distance = planeheight as f32 * yslope; // OK
    let ds_xstep = distance * plane.basexscale;
    let ds_ystep = distance * plane.baseyscale;

    // distance * distscale[i]
    let distscale = CLASSIC_SCREEN_X_TO_VIEW[x1 as usize]
        .to_radians()
        .cos()
        .abs();
    let length = distance * (1.0 / distscale);
    let angle = plane.view_angle + (CLASSIC_SCREEN_X_TO_VIEW[x1 as usize].to_radians());
    let ds_xfrac = viewxy.x() + angle.cos() * length;
    let ds_yfrac = -viewxy.y() - angle.sin() * length;

    // let flat = texture_data.texture_column(plane.picnum, ds_xfrac as i32);
    let flat = texture_data.get_flat(plane.picnum);
    let cm = texture_data.flat_light_colourmap(plane.lightlevel as i32, distance);

    let mut ds = DrawSpan::new(flat, cm, ds_xstep, ds_ystep, ds_xfrac, ds_yfrac, y, x1, x2);

    ds.draw(texture_data, canvas);
}

pub struct DrawSpan<'a> {
    texture: &'a FlatPic,
    colourmap: &'a [usize],
    ds_xstep: f32,
    ds_ystep: f32,
    ds_xfrac: f32,
    ds_yfrac: f32,
    ds_y: i32,
    ds_x1: i32,
    ds_x2: i32,
}

impl<'a> DrawSpan<'a> {
    pub fn new(
        texture: &'a FlatPic,
        colourmap: &'a [usize],
        ds_xstep: f32,
        ds_ystep: f32,
        ds_xfrac: f32,
        ds_yfrac: f32,
        ds_y: i32,
        ds_x1: i32,
        ds_x2: i32,
    ) -> Self {
        Self {
            texture,
            colourmap,
            ds_xstep,
            ds_ystep,
            ds_xfrac,
            ds_yfrac,
            ds_y,
            ds_x1,
            ds_x2,
        }
    }

    fn draw(&mut self, textures: &PicData, canvas: &mut Canvas<Surface>) {
        for s in self.ds_x1..=self.ds_x2 {
            let mut x = self.ds_xfrac.round().abs() as i32 & 127;
            let mut y = self.ds_yfrac.round().abs() as i32 & 127;

            if y >= self.texture.data[0].len() as i32 {
                y %= self.texture.data[0].len() as i32;
            }

            if x >= self.texture.data.len() as i32 {
                x %= self.texture.data.len() as i32;
            }

            let px = self.colourmap[self.texture.data[x as usize][y as usize] as usize];
            let colour = if px == usize::MAX {
                // ERROR COLOUR
                sdl2::pixels::Color::RGBA(255, 0, 0, 255)
            } else {
                let colour = textures.palette(0)[px];
                sdl2::pixels::Color::RGBA(colour.r, colour.g, colour.b, 255)
            };

            canvas.set_draw_color(colour);
            canvas
                .fill_rect(Rect::new(s as i32, self.ds_y as i32, 1, 1))
                .unwrap();

            self.ds_xfrac += self.ds_xstep;
            self.ds_yfrac += self.ds_ystep;
        }
    }
}
