

use super::defs::Visplane;

pub const MAXVISPLANES: usize = 1024;

pub struct VisPlaneRender {
    // Here comes the obnoxious "visplane".
    pub visplanes: Vec<Visplane>,
    pub lastvisplane: usize,
    /// Index of current visplane in `self.visplanes` for floor
    pub floorplane: usize,
    /// Index of current visplane in `self.visplanes` for ceiling
    pub ceilingplane: usize,

    /// Stores the column number of the texture required for this opening
    pub openings: Vec<i32>,
    pub lastopening: f32,

    pub floorclip: Vec<i32>,
    pub ceilingclip: Vec<i32>,

    screen_width: f32,
    half_screen_width: f32,
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
            openings: vec![i32::MAX; screen_width * 128], // TODO: find a good limit
            lastopening: 0.0,
            floorclip: vec![screen_height as i32; screen_width],
            ceilingclip: vec![-1; screen_width],
            screen_width: screen_width as f32,
            half_screen_width: screen_width as f32 / 2.0,
            screen_height: screen_height as f32,
        }
    }

    /// R_ClearPlanes
    /// At begining of frame.
    pub fn clear_planes(&mut self) {
        // opening / clipping determination
        self.floorclip.fill(self.screen_height as i32);
        self.ceilingclip.fill(-1);
        for p in self.visplanes.iter_mut() {
            p.clear();
        }

        self.lastvisplane = 0;
        self.lastopening = 0.;
        self.floorplane = 0;
        self.ceilingplane = 0;
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
            if height == plane.height && picnum == plane.picnum && light_level == plane.lightlevel {
                return index;
            }
        }

        if self.lastvisplane < len {
            self.lastvisplane += 1;
        } else {
            panic!("Out of visplanes");
        }

        // Otherwise edit new
        let check = &mut self.visplanes[self.lastvisplane];
        check.height = height;
        check.picnum = picnum;
        check.lightlevel = light_level;
        check.minx = self.screen_width;
        check.maxx = 0.0;
        for t in &mut check.top {
            *t = i32::MAX;
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
            if plane.top[i as usize] != i32::MAX {
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
            *t = 0;
        }

        self.lastvisplane
    }
}
