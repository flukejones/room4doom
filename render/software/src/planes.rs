use std::f32::consts::FRAC_PI_2;

use crate::utilities::screen_to_x_view;
use gameplay::{Angle, FlatPic, PicData};
use glam::Vec2;
use render_traits::PixelBuf;

use super::defs::Visplane;

pub const MAXVISPLANES: usize = 512;

pub struct VisPlaneRender {
    // Here comes the obnoxious "visplane".
    pub visplanes: Vec<Visplane>,
    pub lastvisplane: usize,
    /// Index of current visplane in `self.visplanes` for floor
    pub floorplane: usize,
    /// Index of current visplane in `self.visplanes` for ceiling
    pub ceilingplane: usize,

    /// Stores the column number of the texture required for this opening
    pub openings: Vec<f32>,
    pub lastopening: f32,

    pub floorclip: Vec<f32>,
    pub ceilingclip: Vec<f32>,
    /// spanstart holds the start of a plane span
    /// initialized to 0 at start
    pub spanstart: Vec<f32>,
    pub spanstop: Vec<f32>,

    //lighttable_t **planezlight;
    pub planeheight: f32,

    pub yslope: Vec<f32>,
    pub distscale: Vec<f32>,
    pub basexscale: f32,
    pub baseyscale: f32,

    screen_width: f32,
    screen_height: f32,
}

impl VisPlaneRender {
    pub fn new(screen_width: usize, screen_height: usize) -> Self {
        VisPlaneRender {
            // TODO: this uses a huge amount of memory and is inefficient
            //  MAXVISPLANES * screen_width * 2
            visplanes: vec![Visplane::new(screen_width); MAXVISPLANES],
            lastvisplane: 0,
            floorplane: 0,
            ceilingplane: 0,
            openings: vec![f32::MAX; screen_width * 64],
            lastopening: 0.0,
            floorclip: vec![screen_height as f32; screen_width],
            ceilingclip: vec![-1.0; screen_width],
            spanstart: vec![0.0; screen_height],
            spanstop: vec![0.0; screen_height],
            planeheight: 0.0,
            yslope: vec![0.0; screen_height],
            distscale: vec![0.0; screen_width],
            basexscale: 0.0,
            baseyscale: 0.0,
            screen_width: screen_width as f32,
            screen_height: screen_height as f32,
        }
    }

    /// R_ClearPlanes
    /// At begining of frame.
    pub fn clear_planes(&mut self, view_angle: Angle) {
        // opening / clipping determination
        for i in 0..self.floorclip.len() {
            self.floorclip[i] = self.screen_height;
            self.ceilingclip[i] = -1.0;
        }

        for p in self.visplanes.iter_mut() {
            p.clear();
        }

        self.lastvisplane = 0;
        self.lastopening = 0.;
        self.floorplane = 0;
        self.ceilingplane = 0;

        // left to right mapping
        // TODO: angle = (viewangle - ANG90) >> ANGLETOFINESHIFT;
        self.basexscale = (view_angle - FRAC_PI_2).cos() / (self.screen_width / 2.0);
        self.baseyscale = -((view_angle - FRAC_PI_2).sin() / (self.screen_width / 2.0));
    }

    /// Find a plane matching height, picnum, light level. Otherwise return a new plane.
    pub fn find_plane(
        &mut self,
        mut height: f32,
        picnum: usize,
        skynum: usize,
        mut light_level: i32,
    ) -> usize {
        if picnum == skynum {
            height = 0.0;
            light_level = 0;
        }

        let len = self.visplanes.len();

        for (index, plane) in self.visplanes[0..self.lastvisplane].iter().enumerate() {
            if height.floor() == plane.height.floor()
                && picnum == plane.picnum
                && light_level == plane.lightlevel
            {
                return index;
            }
        }

        if self.lastvisplane < len {
            self.lastvisplane += 1;
        } else {
            panic!("Out of visplanes");
        }

        // Otherwise edit new
        let mut check = &mut self.visplanes[self.lastvisplane];
        check.height = height;
        check.picnum = picnum;
        check.lightlevel = light_level;
        check.minx = self.screen_width;
        check.maxx = 0.0;
        for t in &mut check.top {
            *t = f32::MAX;
        }

        self.lastvisplane
    }

    /// Check if this plane should be used, otherwise use a new plane.
    pub fn check_plane(&mut self, start: f32, stop: f32, plane_idx: usize) -> usize {
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

        for i in intrl.floor() as i32..=self.screen_width as i32 {
            if i >= intrh.floor() as i32 {
                plane.minx = unionl;
                plane.maxx = unionh;
                // Use the same plane
                return plane_idx;
            }
            if plane.top[i as usize] != f32::MAX {
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
            *t = 0.0;
        }

        self.lastvisplane
    }
}

pub fn make_spans(
    x: f32,
    mut t1: f32,
    mut b1: f32,
    mut t2: f32,
    mut b2: f32,
    viewxy: Vec2,
    viewz: f32,
    extra_light: i32,
    plane: &Visplane,
    span_start: &mut [f32],
    texture_data: &PicData,
    pixels: &mut PixelBuf,
) {
    // TODO: t1, y, is causing a glitch
    while t1 < t2 && t1 <= b1 {
        map_plane(
            t1,
            span_start[t1 as usize], // TODO: check if need floor
            x - 1.0,
            viewxy,
            viewz,
            extra_light,
            plane,
            texture_data,
            pixels,
        );
        t1 += 1.0;
    }

    while b1 > b2 && b1 >= t1 {
        map_plane(
            b1,
            span_start[b1 as usize],
            x - 1.0,
            viewxy,
            viewz,
            extra_light,
            plane,
            texture_data,
            pixels,
        );
        b1 -= 1.0;
    }

    while t2 < t1 && t2 <= b2 {
        span_start[t2 as usize] = x;
        t2 += 1.0;
    }

    while b2 > b1 && b2 >= t2 {
        span_start[b2 as usize] = x;
        b2 -= 1.0;
    }
}

fn map_plane(
    y: f32,
    x1: f32,
    x2: f32,
    viewxy: Vec2,
    viewz: f32,
    extra_light: i32,
    plane: &Visplane,
    texture_data: &PicData,
    pixels: &mut PixelBuf,
) {
    let planeheight = (plane.height - viewz).abs();
    // TODO: maybe cache?
    let dy = y - (pixels.height() as f32 / 2.0); // OK
    let yslope = (pixels.width() as f32 / 2.0) / dy.abs(); // OK
    let distance = planeheight * yslope; // OK
    let ds_xstep = distance * plane.basexscale;
    let ds_ystep = distance * plane.baseyscale;

    // distance * distscale[i]
    let distscale = screen_to_x_view(x1, pixels.width() as f32).cos().abs();
    let length = distance * (1.0 / distscale);
    let angle = plane.view_angle + screen_to_x_view(x1, pixels.width() as f32);
    let ds_xfrac = viewxy.x + angle.cos() * length;
    let ds_yfrac = -viewxy.y - angle.sin() * length;

    // let flat = texture_data.texture_column(plane.picnum, ds_xfrac as i32);
    let flat = texture_data.get_flat(plane.picnum);
    let light = (plane.lightlevel >> 4) + extra_light;
    let colourmap = texture_data.flat_light_colourmap(light, distance);

    let mut ds = DrawSpan::new(
        flat, colourmap, ds_xstep, ds_ystep, ds_xfrac, ds_yfrac, y, x1, x2,
    );

    ds.draw(texture_data, pixels);
}

pub struct DrawSpan<'a> {
    texture: &'a FlatPic,
    colourmap: &'a [usize],
    ds_xstep: f32,
    ds_ystep: f32,
    ds_xfrac: f32,
    ds_yfrac: f32,
    ds_y: f32,
    ds_x1: f32,
    ds_x2: f32,
}

impl<'a> DrawSpan<'a> {
    pub fn new(
        texture: &'a FlatPic,
        colourmap: &'a [usize],
        ds_xstep: f32,
        ds_ystep: f32,
        ds_xfrac: f32,
        ds_yfrac: f32,
        ds_y: f32,
        ds_x1: f32,
        ds_x2: f32,
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

    fn draw(&mut self, textures: &PicData, pixels: &mut PixelBuf) {
        let pal = textures.palette();
        for s in self.ds_x1.round() as i32..=self.ds_x2.round() as i32 {
            let mut x = (self.ds_xfrac.round() as i32 & 127) + 64;
            let mut y = (self.ds_yfrac.round() as i32 & 127) + 64;

            if y >= self.texture.data[0].len() as i32 {
                y %= self.texture.data[0].len() as i32;
            }

            if x >= self.texture.data.len() as i32 {
                x %= self.texture.data.len() as i32;
            }

            let px = self.colourmap[self.texture.data[x as usize][y as usize] as usize];
            let c = pal[px];
            pixels.set_pixel(s as usize, self.ds_y.round() as usize, c.r, c.g, c.b, 255);

            self.ds_xfrac += self.ds_xstep;
            self.ds_yfrac += self.ds_ystep;
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::defs::Visplane;

    #[test]
    fn default_vis_plane_render() {
        let mut rd = Visplane::new(320);
        rd.clear();
    }
}
