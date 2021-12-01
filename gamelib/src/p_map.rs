//!	Movement, collision handling.
//!	Shooting and aiming.
use glam::Vec2;

use crate::angle::Angle;
use crate::flags::LineDefFlags;
use crate::level_data::level::Level;
use crate::level_data::map_data::BSPTrace;
use crate::level_data::map_defs::{BBox, LineDef, SlopeType};
use crate::p_local::{BestSlide, Intercept, MAXRADIUS};
use crate::p_map_object::{MapObject, MapObjectFlag};
use crate::p_map_util::{
    box_on_line_side, path_traverse,
    PortalZ,
};
use crate::DPtr;

const MAXSPECIALCROSS: i32 = 8;

/// The pupose of this struct is to record the highest and lowest points in a
/// subsector. When a mob crosses a seg it may be between floor/ceiling heights.
#[derive(Default)]
pub struct SubSectorMinMax {
    tmflags: u32,
    /// If "floatok" true, move would be ok
    /// if within "tmfloorz - tmceilingz".
    floatok: bool,
    min_floor_z: f32,
    max_ceil_z: f32,
    max_dropoff: f32,
    spec_hits: Vec<DPtr<LineDef>>,
}

impl MapObject {
    /// P_TryMove, merged with P_CheckPosition and using a more verbose/modern collision
    pub fn p_try_move(&mut self, ptryx: f32, ptryy: f32, level: &mut Level) -> bool {
        // P_CrossSpecialLine
        level.mobj_ctrl.floatok = false;

        let try_move = Vec2::new(ptryx, ptryy);

        level.mobj_ctrl.spec_hits.clear();
        level.mobj_ctrl.floatok = true;
        if !self.p_check_position(self.xy, try_move, level) {
            return false;
        }

        let ctrl = &mut level.mobj_ctrl;
        if self.flags & MapObjectFlag::MF_NOCLIP as u32 == 0 {
            if ctrl.max_ceil_z - ctrl.min_floor_z < self.height {
                return false; // doesn't fit
            }
            ctrl.floatok = true;

            if self.flags & MapObjectFlag::MF_TELEPORT as u32 == 0
                && ctrl.max_ceil_z - self.z < self.height
            {
                return false; // mobj must lower itself to fit
            }

            if self.flags & MapObjectFlag::MF_TELEPORT as u32 == 0
                && ctrl.min_floor_z - self.z > 24.0
            {
                return false; // too big a step up
            }

            if self.flags & (MapObjectFlag::MF_DROPOFF as u32 | MapObjectFlag::MF_FLOAT as u32) == 0
                && ctrl.min_floor_z - ctrl.max_dropoff > 24.0
            {
                return false; // too big a step up
            }
        }

        // the move is ok,
        // so link the thing into its new position
        // P_UnsetThingPosition (thing);

        let old_xy = self.xy;

        self.floorz = ctrl.min_floor_z;
        self.ceilingz = ctrl.max_ceil_z;
        self.xy = try_move;

        // P_SetThingPosition (thing);

        if self.flags & (MapObjectFlag::MF_TELEPORT as u32 | MapObjectFlag::MF_NOCLIP as u32) != 0 {
            for ld in &ctrl.spec_hits {
                // see if the line was crossed
                let side = ld.point_on_side(&self.xy);
                let old_side = ld.point_on_side(&old_xy);
                if side != old_side && ld.special != 0 {
                    // TODO: P_CrossSpecialLine(ld - lines, oldside, thing);
                }
            }
        }
        true
    }

    // P_CheckPosition
    // This is purely informative, nothing is modified
    // (except things picked up).
    //
    // in:
    //  a mobj_t (can be valid or invalid)
    //  a position to be checked
    //   (doesn't need to be related to the mobj_t->x,y)
    //
    // during:
    //  special things are touched if MF_PICKUP
    //  early out on solid lines?
    //
    // out:
    //  newsubsec
    //  floorz
    //  ceilingz
    //  tmdropoffz
    //   the lowest point contacted
    //   (monsters won't move to a dropoff)
    //  speciallines[]
    //  numspeciallines
    //
    /// Check for things and lines contacts.
    ///
    /// `PIT_CheckLine` is called by an iterator over the blockmap parts contacted
    /// and this function checks if the line is solid, if not then it also sets
    /// the portal ceil/floor coords and dropoffs
    fn p_check_position(&mut self, origin: Vec2, endpoint: Vec2, level: &mut Level) -> bool {
        let left = endpoint.x() - self.radius;
        let right = endpoint.x() + self.radius;
        let top = endpoint.y() + self.radius;
        let bottom = endpoint.y() - self.radius;
        let tmbbox = BBox {
            top,
            bottom,
            left,
            right,
        };

        let ctrl = &mut level.mobj_ctrl;
        let newsubsec = level.map_data.point_in_subsector(endpoint);
        // The base floor / ceiling is from the subsector
        // that contains the point.
        ctrl.min_floor_z = newsubsec.sector.floorheight;
        ctrl.max_dropoff = newsubsec.sector.floorheight;
        ctrl.max_ceil_z = newsubsec.sector.ceilingheight;

        if self.flags & MapObjectFlag::MF_NOCLIP as u32 != 0 {
            return true;
        }

        // TODO: use the blockmap for checking lines
        // TODO: use a P_BlockThingsIterator
        // TODO: use a P_BlockLinesIterator - used to build a list of lines to check
        //       it also calls PIT_CheckLine on each line
        //       P_BlockLinesIterator is called mobj->radius^2

        // BSP walk to find all subsectors between two points
        // Pretty much replaces the block iterators
        let sub_sectors = level.map_data.get_subsectors();

        // The p_try_move calls check collisions -> p_check_position -> pit_check_line
        // A single BSP trace varies from 5 to 15 recursions.
        // Regular Doom maps have 4 to 100 or so lines in a sector
        // SIGIL wad has 4000+ lines per map (approx),
        // 3 recursions = average 25 depth total
        // subsectors crossed = average 2
        // lines per subsector = average 4
        // Lines to check = 4~
        let mut bsp_trace = BSPTrace::new(
            Vec2::new(left, bottom),
            Vec2::new(right, top),
            level.map_data.start_node(),
        );
        let mut count = 0;
        bsp_trace.find_ssect_intercepts(&level.map_data, &mut count);

        bsp_trace.set_line(Vec2::new(left, top), Vec2::new(right, bottom));
        bsp_trace.find_ssect_intercepts(&level.map_data, &mut count);

        bsp_trace.set_line(Vec2::new(left, bottom), Vec2::new(left, top));
        bsp_trace.find_ssect_intercepts(&level.map_data, &mut count);

        bsp_trace.set_line(Vec2::new(right, bottom), Vec2::new(right, top));
        bsp_trace.find_ssect_intercepts(&level.map_data, &mut count);

        bsp_trace.set_line(Vec2::new(right, top), Vec2::new(left, top));
        bsp_trace.find_ssect_intercepts(&level.map_data, &mut count);

        bsp_trace.set_line(Vec2::new(right, bottom), Vec2::new(left, bottom));
        bsp_trace.find_ssect_intercepts(&level.map_data, &mut count);
        //dbg!(count);

        let segs = level.map_data.get_segments();
        for n in bsp_trace.intercepted_nodes() {
            let ssect = &sub_sectors[*n as usize];
            let start = ssect.start_seg as usize;
            let end = start + ssect.seg_count as usize;
            for seg in &segs[start..end] {
                if !self.pit_check_line(&tmbbox, ctrl, &seg.linedef) {
                    return false;
                }
            }
        }
        // for seg in level.map_data.get_linedefs().iter() {
        //         if !self.pit_check_line(&tmbbox, ctrl, &seg) {
        //             return false;
        //         }
        //     }

        true
    }

    /// PIT_CheckLine
    /// Adjusts tmfloorz and tmceilingz as lines are contacted
    fn pit_check_line(
        &mut self,
        tmbbox: &BBox,
        // point1: Vec2,
        // point2: Vec2,
        ctrl: &mut SubSectorMinMax,
        ld: &LineDef,
    ) -> bool {
        if tmbbox.right < ld.bbox.left
            || tmbbox.left > ld.bbox.right
            || tmbbox.top < ld.bbox.bottom
            || tmbbox.bottom > ld.bbox.top
        {
            return true;
        }

        // In OG Doom the function used to check if collided is P_BoxOnLineSide
        // this does very fast checks using the line slope, for example a
        // line that is horizontal or vertical checked against the top/bottom/left/right
        // of bbox.
        // If the line is a slope then if it's positive or negative determines which
        // box corners are used - Doom checks which side of the line each are on
        // using `P_PointOnLineSide`
        // If both are same side then there is no intersection.

        if box_on_line_side(&tmbbox, &ld) != -1 {
            return true;
        }

        if ld.backsector.is_none() {
            // one-sided line
            return false;
        }

        if self.flags & MapObjectFlag::MF_MISSILE as u32 == 0 {
            if ld.flags & LineDefFlags::Blocking as i16 != 0 {
                return false; // explicitly blocking everything
            }

            if self.player.is_none() && ld.flags & LineDefFlags::BlockMonsters as i16 != 0 {
                return false; // block monsters only
            }
        }

        // Find the smallest/largest etc if group of line hits
        let portal = PortalZ::new(&ld);
        if portal.top_z < ctrl.max_ceil_z {
            ctrl.max_ceil_z = portal.top_z;
            // TODO: ceilingline = ld;
        }
        // Find the highest floor point (for steps etc)
        if portal.bottom_z > ctrl.min_floor_z {
            ctrl.min_floor_z = portal.bottom_z;
        }
        // Find the lowest possible point in subsectors contacted
        if portal.lowest_z < ctrl.max_dropoff {
            ctrl.max_dropoff = portal.lowest_z;
        }

        if ld.special != 0 {
            ctrl.spec_hits.push(DPtr::new(&ld));
        }

        true
    }

    // P_SlideMove
    // Loop until get a good move or stopped
    pub fn p_slide_move(&mut self, level: &mut Level) {
        // let ctrl = &mut level.mobj_ctrl;
        let mut hitcount = 0;
        self.best_slide = BestSlide::new();

        let leadx;
        let leady;
        let trailx;
        let traily;

        if self.momxy.x() > 0.0 {
            leadx = self.xy.x() + self.radius;
            trailx = self.xy.x() - self.radius;
        } else {
            leadx = self.xy.x() - self.radius;
            trailx = self.xy.x() + self.radius;
        }

        if self.momxy.y() > 0.0 {
            leady = self.xy.y() + self.radius;
            traily = self.xy.y() - self.radius;
        } else {
            leady = self.xy.y() - self.radius;
            traily = self.xy.y() + self.radius;
        }

        loop {
            if hitcount == 3 {
                self.stair_step(level);
                return;
            }

            // tail to front, centered
            let mut bsp_trace = BSPTrace::new(
                Vec2::new(trailx, traily),
                Vec2::new(leadx, leady) + self.momxy,
                level.map_data.start_node(),
            );
            let mut count = 0;
            bsp_trace.find_ssect_intercepts(&level.map_data, &mut count);
            // outside edges
            bsp_trace.set_line(
                Vec2::new(trailx, leady),
                Vec2::new(trailx, leady) + self.momxy,
            );
            bsp_trace.find_ssect_intercepts(&level.map_data, &mut count);

            bsp_trace.set_line(
                Vec2::new(leadx, traily),
                Vec2::new(leadx, traily) + self.momxy,
            );
            bsp_trace.find_ssect_intercepts(&level.map_data, &mut count);
            // leading edges
            bsp_trace.set_line(
                Vec2::new(leadx, leady) + self.momxy,
                Vec2::new(trailx, leady) + self.momxy,
            );
            bsp_trace.find_ssect_intercepts(&level.map_data, &mut count);

            bsp_trace.set_line(
                Vec2::new(leadx, leady) + self.momxy,
                Vec2::new(leadx, traily) + self.momxy,
            );
            bsp_trace.find_ssect_intercepts(&level.map_data, &mut count);

            path_traverse(
                Vec2::new(leadx, leady),
                Vec2::new(leadx, leady) + self.momxy,
                level,
                |intercept| self.slide_traverse(intercept),
                &mut bsp_trace,
            );
            path_traverse(
                Vec2::new(trailx, leady),
                Vec2::new(trailx, leady) + self.momxy,
                level,
                |intercept| self.slide_traverse(intercept),
                &mut &mut bsp_trace,
            );
            path_traverse(
                Vec2::new(leadx, traily),
                Vec2::new(leadx, traily) + self.momxy,
                level,
                |intercept| self.slide_traverse(intercept),
                &mut &mut bsp_trace,
            );

            if self.best_slide.best_slide_frac == 2.0 {
                // The move most have hit the middle, so stairstep.
                self.stair_step(level);
                return;
            }

            self.best_slide.best_slide_frac -= 0.031250;
            if self.best_slide.best_slide_frac > 0.0 {
                let slide_move = self.momxy * self.best_slide.best_slide_frac; // bestfrac
                if !self.p_try_move(
                    self.xy.x() + slide_move.x(),
                    self.xy.y() + slide_move.y(),
                    level,
                ) {
                    self.stair_step(level);
                    return;
                }
            }

            // Now continue along the wall.
            // First calculate remainder.
            self.best_slide.best_slide_frac = 1.0 - (self.best_slide.best_slide_frac + 0.031250);
            if self.best_slide.best_slide_frac > 1.0 {
                self.best_slide.best_slide_frac = 1.0;
            }

            if self.best_slide.best_slide_frac <= 0.0 {
                return;
            }

            let mut slide_move = self.momxy * self.best_slide.best_slide_frac;
            // Clip the moves.
            if let Some(best_slide_line) = self.best_slide.best_slide_line.as_ref() {
                self.hit_slide_line(&mut slide_move, best_slide_line);
            }

            self.momxy = slide_move;

            let endpoint = self.xy + slide_move;
            if self.p_try_move(endpoint.x(), endpoint.y(), level) {
                return;
            }

            hitcount += 1;
        }
    }

    fn blocking_intercept(&mut self, intercept: &Intercept) {
        if intercept.frac < self.best_slide.best_slide_frac {
            self.best_slide.second_slide_frac = self.best_slide.best_slide_frac;
            self.best_slide.second_slide_line = self.best_slide.best_slide_line.clone();
            self.best_slide.best_slide_frac = intercept.frac;
            self.best_slide.best_slide_line = intercept.line.clone();
        }
    }

    pub fn slide_traverse(&mut self, intercept: &Intercept) -> bool {
        if let Some(line) = &intercept.line {
            if (line.flags as usize) & LineDefFlags::TwoSided as usize == 0 {
                if line.point_on_side(&self.xy) != 0 {
                    return true; // Don't hit backside
                }
                self.blocking_intercept(intercept);
            }

            // set openrange, opentop, openbottom
            let portal = PortalZ::new(line);
            if portal.range < self.height // doesn't fit
                || portal.top_z - self.z < self.height // mobj is too high
                || portal.bottom_z - self.z > 24.0
            // too big a step up
            {
                self.blocking_intercept(intercept);
                return false;
            }
            // this line doesn't block movement
            return true;
        }

        self.blocking_intercept(intercept);
        false
    }

    pub fn stair_step(&mut self, level: &mut Level) {
        // Line might have hit the middle, end-on?
        if !self.p_try_move(self.xy.x(), self.xy.y() + self.momxy.y(), level) {
            self.p_try_move(self.xy.x() + self.momxy.x(), self.xy.y(), level);
        }
    }

    /// P_HitSlideLine
    pub fn hit_slide_line(&self, slide_move: &mut Vec2, line: &LineDef) {
        if matches!(line.slopetype, SlopeType::Horizontal) {
            slide_move.set_y(0.0);
            return;
        }
        if matches!(line.slopetype, SlopeType::Vertical) {
            slide_move.set_x(0.0);
            return;
        }

        // let side = line.point_on_side(slide_move);
        let line_angle = Angle::from_vector(line.delta);
        // if side == 1 {
        //     //line_angle += FRAC_PI_2;
        //     line_angle = Angle::from_vector(Vec2::new(line.delta.x() * -1.0, line.delta.y() * -1.0));
        // }

        let move_angle = Angle::from_vector(*slide_move);
        // if move_angle.rad() > FRAC_PI_2 {
        //     move_angle -= FRAC_PI_2;
        // }

        let delta_angle = move_angle - line_angle;
        // if delta_angle.rad() > FRAC_PI_2 {
        //     delta_angle += FRAC_PI_2;
        // }

        let move_dist = slide_move.length();
        let new_dist = move_dist * delta_angle.cos();

        *slide_move = line_angle.unit() * new_dist;
    }
}

/// P_RadiusAttack
/// Source is the creature that caused the explosion at spot.
pub fn p_radius_attack(spot: &mut MapObject, source: &mut MapObject, damage: f32) {
    let dist = damage + MAXRADIUS;
    unimplemented!()
    // // origin of block level is bmaporgx and bmaporgy
    // let yh = (spot.xy.y() + dist - bmaporgy) >> MAPBLOCKSHIFT;
    // let yl = (spot.xy.y() - dist - bmaporgy) >> MAPBLOCKSHIFT;
    // let xh = (spot.xy.x() + dist - bmaporgx) >> MAPBLOCKSHIFT;
    // let xl = (spot.xy.x() - dist - bmaporgx) >> MAPBLOCKSHIFT;
    // bombspot = spot;
    // bombsource = source;
    // bombdamage = damage;

    // for (y = yl; y <= yh; y++)
    // for (x = xl; x <= xh; x++)
    // P_BlockThingsIterator(x, y, PIT_RadiusAttack);
}
