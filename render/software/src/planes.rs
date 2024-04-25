use gameplay::{log::warn, Angle, FlatPic, PicData};
use glam::Vec2;
use render_target::PixelBuffer;

use crate::utilities::screen_to_x_view;

use super::defs::Visplane;

const MAXVISPLANES: usize = 128;
const VISPLANE_INCREASE: usize = 32;

pub struct VisPlaneRender {
    // Here comes the obnoxious "visplane".
    pub visplanes: Vec<Visplane>,
    pub lastvisplane: usize,
    /// Index of current visplane in `self.visplanes` for floor
    pub floorplane: usize,
    /// Index of current visplane in `self.visplanes` for ceiling
    pub ceilingplane: usize,
    screen_width: f32,
}

impl VisPlaneRender {
    pub fn new(screen_width: usize) -> Self {
        VisPlaneRender {
            // TODO: this uses a huge amount of memory and is inefficient
            //  MAXVISPLANES * screen_width * 2
            visplanes: vec![Visplane::new(screen_width); MAXVISPLANES],
            lastvisplane: 0,
            floorplane: 0,
            ceilingplane: 0,
            screen_width: screen_width as f32,
        }
    }

    /// R_ClearPlanes
    /// At begining of frame.
    pub fn clear_planes(&mut self) {
        // opening / clipping determination
        for p in self.visplanes.iter_mut() {
            p.clear();
        }

        self.lastvisplane = 0;
        self.floorplane = 0;
        self.ceilingplane = 0;
    }

    fn increase_store(&mut self) {
        let mut tmp = Vec::with_capacity(self.visplanes.len() + VISPLANE_INCREASE);
        tmp.clone_from(&self.visplanes);
        let mut start = self.visplanes.len();
        while start < self.visplanes.len() + VISPLANE_INCREASE {
            start += 1;
            tmp.push(Visplane::new(self.screen_width as usize));
        }
        warn!("New visplane array len {}", tmp.len());
        self.visplanes = tmp;
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

        let len = self.visplanes.len() - 1;

        for (index, plane) in self.visplanes[0..self.lastvisplane].iter().enumerate() {
            if height == plane.height && picnum == plane.picnum && light_level == plane.lightlevel {
                return index;
            }
        }

        if self.lastvisplane < len {
            self.lastvisplane += 1;
        } else {
            self.increase_store();
            // panic!("Out of visplanes");
        }

        // Otherwise edit new
        let check = &mut self.visplanes[self.lastvisplane];
        check.height = height;
        check.picnum = picnum;
        check.lightlevel = light_level;
        check.minx = self.screen_width;
        check.maxx = 0.0;
        // for t in &mut check.top {
        //     *t = i32::MAX;
        // }
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

        if intrh <= intrl {
            plane.minx = unionl;
            plane.maxx = unionh;
            return plane_idx;
        }

        for i in intrl as i32..=self.screen_width as i32 {
            if i >= intrh as i32 {
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

        if self.lastvisplane + 1 >= self.visplanes.len() - 1 {
            self.increase_store();
            // panic!("No more visplanes: used {}", self.lastvisplane);
        }

        self.lastvisplane += 1;
        let plane = &mut self.visplanes[self.lastvisplane];
        plane.height = height;
        plane.picnum = picnum;
        plane.lightlevel = lightlevel;
        plane.minx = start;
        plane.maxx = stop;

        // for t in &mut plane.top {
        //     *t = 0;
        // }
        self.lastvisplane
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_doom_style_flats(
    x: f32,
    mut t1: f32,
    mut b1: f32,
    mut t2: f32,
    mut b2: f32,
    basexscale: f32,
    baseyscale: f32,
    viewxy: Vec2,
    viewz: f32,
    view_angle: Angle,
    extra_light: i32,
    plane: &Visplane,
    span_start: &mut [f32],
    pic_data: &PicData,
    pixels: &mut dyn PixelBuffer,
) {
    while t1 < t2 && t1 <= b1 {
        map_plane(
            t1,
            span_start[t1 as usize], // TODO: check if need floor
            x - 1.0,
            basexscale,
            baseyscale,
            viewxy,
            viewz,
            view_angle,
            extra_light,
            plane,
            pic_data,
            pixels,
        );
        t1 += 1.0;
    }

    while b1 > b2 && b1 >= t1 {
        map_plane(
            b1,
            span_start[b1 as usize],
            x - 1.0,
            basexscale,
            baseyscale,
            viewxy,
            viewz,
            view_angle,
            extra_light,
            plane,
            pic_data,
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

#[allow(clippy::too_many_arguments)]
fn map_plane(
    y: f32,
    x1: f32,
    x2: f32,
    basexscale: f32,
    baseyscale: f32,
    viewxy: Vec2,
    viewz: f32,
    view_angle: Angle,
    extra_light: i32,
    plane: &Visplane,
    pic_data: &PicData,
    pixels: &mut dyn PixelBuffer,
) {
    let planeheight = (plane.height - viewz).abs();
    // TODO: maybe cache?
    let dy = y - pixels.size().half_height_f32(); // OK
    let yslope = pixels.size().half_width_f32() / dy.abs(); // OK
    let distance = planeheight * yslope; // OK
    let ds_xstep = distance * basexscale;
    let ds_ystep = distance * baseyscale;

    // distance * distscale[i]
    let distscale = screen_to_x_view(x1, pixels.size().width_f32()).cos();
    let length = distance * (1.0 / distscale);
    let angle = view_angle + screen_to_x_view(x1, pixels.size().width_f32());
    let ds_xfrac = viewxy.x + angle.cos() * length;
    let ds_yfrac = -viewxy.y - angle.sin() * length;

    // let flat = texture_data.texture_column(plane.picnum, ds_xfrac as i32);
    let flat = pic_data.get_flat(plane.picnum);
    let light = (plane.lightlevel >> 4) + extra_light;
    let colourmap = pic_data.flat_light_colourmap(light, distance as u32);

    draw(
        flat, colourmap, ds_xstep, ds_ystep, ds_xfrac, ds_yfrac, y as usize, x1, x2, pic_data,
        pixels,
    );
}

fn draw(
    texture: &FlatPic,
    colourmap: &[usize],
    ds_xstep: f32,
    ds_ystep: f32,
    mut ds_xfrac: f32,
    mut ds_yfrac: f32,
    ds_y: usize,
    ds_x1: f32,
    ds_x2: f32,
    pic_data: &PicData,
    pixels: &mut dyn PixelBuffer,
) {
    let pal = pic_data.palette();
    for s in ds_x1 as i32..=ds_x2 as i32 {
        let mut x = ds_xfrac.abs() as usize;
        let mut y = ds_yfrac.abs() as usize;

        y %= texture.data[0].len();
        x %= texture.data.len();

        let px = colourmap[texture.data[x][y]];
        let c = pal[px];
        pixels.set_pixel(s as usize, ds_y, (c.r, c.g, c.b, 255));

        ds_xfrac += ds_xstep;
        ds_yfrac += ds_ystep;
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
