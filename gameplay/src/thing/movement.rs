//! Movement, collision handling.
//!
//! Almost all of the methods here are on `MapObject`.

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};
use std::ptr;

use glam::Vec2;
use log::{debug, error};

use crate::angle::Angle;
use crate::doom_def::{FLOATSPEED, USERANGE, VIEWHEIGHT};
use crate::env::specials::cross_special_line;
use crate::env::switch::p_use_special_line;
use crate::info::StateNum;
use crate::level::flags::LineDefFlags;
use crate::level::map_data::BSPTrace;
use crate::level::map_defs::{BBox, LineDef, SlopeType};
use crate::utilities::{
    box_on_line_side, p_random, path_traverse, BestSlide, Intercept, PortalZ, FRACUNIT_DIV4
};
use crate::{MapObjKind, MapObject, MapPtr};

use super::MapObjFlag;

pub const MAXMOVE: f32 = 30.0;
pub const STOPSPEED: f32 = 0.0625; // 0x1000
pub const FRICTION: f32 = 0.90625; // 0xE800

//const MAXSPECIALCROSS: i32 = 8;
pub const PT_ADDLINES: i32 = 1;
pub const PT_ADDTHINGS: i32 = 2;
pub const PT_EARLYOUT: i32 = 4;

/// The pupose of this struct is to record the highest and lowest points in a
/// subsector. When a mob crosses a seg it may be between floor/ceiling heights.
#[derive(Default)]
pub struct SubSectorMinMax {
    /// If "floatok" true, move would be ok
    /// if within "tmfloorz - tmceilingz".
    floatok: bool,
    pub min_floor_z: f32,
    pub max_ceil_z: f32,
    max_dropoff: f32,
    sky_line: Option<MapPtr<LineDef>>,
    spec_hits: Vec<MapPtr<LineDef>>,
}

impl MapObject {
    /// P_ZMovement
    pub(crate) fn p_z_movement(&mut self) {
        if self.player.is_some() && self.z < self.floorz {
            unsafe {
                let player = &mut *(self.player.unwrap());
                player.viewheight -= self.floorz - self.z;
                player.deltaviewheight = (((VIEWHEIGHT - player.viewheight) as i32) >> 3) as f32;
            }
        }

        // adjust height
        self.z += self.momz;

        if self.flags & MapObjFlag::Float as u32 != 0 {
            if let Some(target) = self.target {
                let target = unsafe { (*target).mobj() };

                // float down towards target if too close
                if self.flags & MapObjFlag::Skullfly as u32 == 0
                    && self.flags & MapObjFlag::Infloat as u32 == 0
                {
                    let dist = self.xy.distance(target.xy);
                    let delta = target.z + self.height / 2.0 - self.z;

                    if delta < 0.0 && dist < -(delta * 3.0) {
                        self.z -= FLOATSPEED;
                    } else if delta > 0.0 && dist < delta * 3.0 {
                        self.z += FLOATSPEED;
                    }
                }
            }
        }

        // clip movement

        if self.z <= self.floorz {
            // hit the floor
            // TODO: The lost soul correction for old demos
            if self.flags & MapObjFlag::Skullfly as u32 != 0 {
                // the skull slammed into something
                self.momz = -self.momz;
            }

            if self.momz < 0.0 {
                if self.player.is_some() && self.momz < -1.0 * 8.0 {
                    // Squat down.
                    // Decrease viewheight for a moment
                    // after hitting the ground (hard),
                    // and utter appropriate sound.
                    unsafe {
                        let player = &mut *(self.player.unwrap());
                        player.viewheight = ((self.momz as i32) >> 3) as f32;
                    }
                }
                self.momz = 0.0;
            }

            self.z = self.floorz;

            if self.flags & MapObjFlag::Missile as u32 != 0
                && self.flags & MapObjFlag::Noclip as u32 == 0
            {
                self.p_explode_missile();
                return;
            }
        } else if self.flags & MapObjFlag::Nogravity as u32 == 0 {
            if self.momz == 0.0 {
                self.momz = -1.0 * 2.0;
            } else {
                self.momz -= 1.0;
            }
        }

        if self.z + self.height > self.ceilingz {
            // hit the ceiling
            if self.momz > 0.0 {
                self.momz = 0.0;
                self.z = self.ceilingz - self.height;
            }

            if self.flags & MapObjFlag::Skullfly as u32 != 0 {
                // the skull slammed into something
                self.momz = -self.momz;
            }

            if self.flags & MapObjFlag::Missile as u32 != 0
                && self.flags & MapObjFlag::Noclip as u32 == 0
            {
                self.p_explode_missile();
            }
        }
    }

    /// Doom function name `P_XYMovement`
    pub(crate) fn p_xy_movement(&mut self) {
        if self.momxy.x == 0.0 && self.momxy.y == 0.0 {
            if self.flags & MapObjFlag::Skullfly as u32 != 0 {
                self.flags &= !(MapObjFlag::Skullfly as u32);
                self.momz = 0.0;
                self.set_state(self.info.spawnstate);
            }
            return;
        }

        self.momxy.x = self.momxy.x.clamp(-MAXMOVE, MAXMOVE);
        self.momxy.y = self.momxy.y.clamp(-MAXMOVE, MAXMOVE);

        // This whole loop is a bit crusty. It consists of looping over progressively
        // smaller moves until we either hit 0, or get a move. Because the whole
        // game-exe is 2D we can use modern 2D collision detection where if
        // there is a seg/wall penetration then we move the player back by the
        // penetration amount. This would also make the "slide" stuff
        // a lot easier (but perhaps not as accurate to Doom classic?)
        // Oh yeah, this would also remove:
        //  - linedef BBox,
        //  - BBox checks (these are AABB)
        //  - the need to store line slopes
        // TODO: The above stuff, refactor the collisions and movement to use modern
        // techniques

        // P_XYMovement
        // `p_try_move` will apply the move if it is valid, and do specials, explodes
        // etc
        let mut xmove = self.momxy.x;
        let mut ymove = self.momxy.y;
        let mut ptryx;
        let mut ptryy;
        loop {
            if xmove > MAXMOVE / 2.0 || ymove > MAXMOVE / 2.0 {
                ptryx = self.xy.x + xmove / 2.0;
                ptryy = self.xy.y + ymove / 2.0;
                xmove /= 2.0;
                ymove /= 2.0;
            } else {
                ptryx = self.xy.x + xmove;
                ptryy = self.xy.y + ymove;
                xmove = 0.0;
                ymove = 0.0;
            }

            let mut ctrl = SubSectorMinMax::default();
            if !self.p_try_move(ptryx, ptryy, &mut ctrl) {
                if self.player.is_some() {
                    self.p_slide_move();
                } else if self.flags & MapObjFlag::Missile as u32 != 0 {
                    if let Some(line) = ctrl.sky_line {
                        if line.frontsector.ceilingpic == self.level().sky_num() {
                            self.remove();
                            return;
                        }
                        if let Some(back) = line.backsector.as_ref() {
                            if back.ceilingpic == self.level().sky_num() {
                                self.remove();
                                return;
                            }
                        }
                    }
                    self.p_explode_missile(); //
                } else {
                    self.momxy = Vec2::default();
                }
            }

            if xmove == 0.0 || ymove == 0.0 {
                break;
            }
        }

        // slow down
        if self.flags & (MapObjFlag::Missile as u32 | MapObjFlag::Skullfly as u32) != 0 {
            return; // no friction for missiles ever
        }

        if self.z > self.floorz {
            return; // no friction when airborne
        }

        let floorheight = unsafe { (*self.subsector).sector.floorheight };

        if self.flags & MapObjFlag::Corpse as u32 != 0 {
            // do not stop sliding
            //  if halfway off a step with some momentum
            if (self.momxy.x > FRACUNIT_DIV4
                || self.momxy.x < -FRACUNIT_DIV4
                || self.momxy.y > FRACUNIT_DIV4
                || self.momxy.y < -FRACUNIT_DIV4)
                && (self.floorz - floorheight).abs() > f32::EPSILON
            {
                return;
            }
        }

        if self.momxy.x > -STOPSPEED
            && self.momxy.x < STOPSPEED
            && self.momxy.y > -STOPSPEED
            && self.momxy.y < STOPSPEED
        {
            if self.player.is_none() {
                self.momxy = Vec2::default();
            } else if let Some(player) = self.player_mut() {
                if player.cmd.forwardmove == 0 && player.cmd.sidemove == 0 {
                    // if in a walking frame, stop moving
                    // TODO: What the everliving fuck is C doing here? You can't just subtract the
                    // states array if ((player.mo.state - states) - PLAY_RUN1)
                    // < 4 {
                    self.set_state(StateNum::PLAY);
                    // }
                    self.momxy = Vec2::default();
                }
            }
        } else {
            self.momxy *= FRICTION;
        }
    }

    /// P_TryMove, merged with P_CheckPosition and using a more verbose/modern
    /// collision
    pub(crate) fn p_try_move(
        &mut self,
        ptryx: f32,
        ptryy: f32,
        ctrl: &mut SubSectorMinMax,
    ) -> bool {
        // P_CrossSpecialLine
        let try_move = Vec2::new(ptryx, ptryy);

        ctrl.floatok = false;
        if !self.p_check_position(try_move, ctrl) {
            return false;
        }

        if self.flags & MapObjFlag::Noclip as u32 == 0 {
            if ctrl.max_ceil_z - ctrl.min_floor_z < self.height {
                return false; // doesn't fit
            }
            ctrl.floatok = true;

            if self.flags & MapObjFlag::Teleport as u32 == 0
                && ctrl.max_ceil_z - self.z < self.height
            {
                return false; // thing must lower itself to fit
            }

            if self.flags & MapObjFlag::Teleport as u32 == 0 && ctrl.min_floor_z - self.z > 24.0 {
                return false; // too big a step up
            }

            if self.flags & (MapObjFlag::Dropoff as u32 | MapObjFlag::Float as u32) == 0
                && ctrl.min_floor_z - ctrl.max_dropoff > 24.0
            {
                return false; // too big a step up
            }
        }

        // the move is ok,
        // so link the thing into its new position
        unsafe {
            self.unset_thing_position();
        }

        let old_xy = self.xy;

        self.floorz = ctrl.min_floor_z;
        self.ceilingz = ctrl.max_ceil_z;
        self.xy = try_move;

        unsafe {
            self.set_thing_position();
        }

        if self.flags & (MapObjFlag::Teleport as u32 | MapObjFlag::Noclip as u32) == 0 {
            for ld in &ctrl.spec_hits {
                // see if the line was crossed
                let side = ld.point_on_side(self.xy);
                let old_side = ld.point_on_side(old_xy);
                if side != old_side && ld.special != 0 {
                    cross_special_line(old_side, ld.clone(), self)
                }
            }
        }
        true
    }

    /// Check for things and lines contacts.
    ///
    /// Doom function name `P_CheckPosition`
    pub(crate) fn p_check_position(&mut self, endpoint: Vec2, ctrl: &mut SubSectorMinMax) -> bool {
        let left = endpoint.x - self.radius;
        let right = endpoint.x + self.radius;
        let top = endpoint.y + self.radius;
        let bottom = endpoint.y - self.radius;
        let tmbbox = BBox {
            top,
            bottom,
            left,
            right,
        };

        let level = unsafe { &mut *self.level };
        let newsubsec = level.map_data.point_in_subsector_raw(endpoint);

        // The base floor / ceiling is from the subsector
        // that contains the point.
        unsafe {
            ctrl.min_floor_z = (*newsubsec).sector.floorheight;
            ctrl.max_dropoff = (*newsubsec).sector.floorheight;
            ctrl.max_ceil_z = (*newsubsec).sector.ceilingheight;
        }

        if self.flags & MapObjFlag::Noclip as u32 != 0 {
            return true;
        }

        // BSP walk to find all subsectors between two points
        // Pretty much replaces the block iterators
        //
        // The p_try_move calls check collisions -> p_check_position -> pit_check_line
        // A single BSP trace varies from 5 to 15 recursions.
        // Regular Doom maps have 4 to 100 or so lines in a sector, with average
        // recursion of 10-15 deep SIGIL wad has 4000+ lines per map (approx),
        // with average recursion of 15-40 deep
        //
        // subsectors crossed = average 2
        // lines per subsector = average 4
        // Lines to check = 4~
        let mut bsp_trace =
            BSPTrace::new_line(Vec2::new(left, bottom), Vec2::new(right, top), self.radius);
        let mut count = 0;
        bsp_trace.find_intercepts(level.map_data.start_node(), &level.map_data, &mut count);

        for n in bsp_trace.intercepted_subsectors() {
            let ssect = &mut level.map_data.subsectors_mut()[*n as usize];

            // Check things in subsectors
            if !ssect
                .sector
                .run_mut_func_on_thinglist(|thing| self.pit_check_thing(thing, endpoint, ctrl))
            {
                return false;
            }

            // Check subsector segments
            let start = ssect.start_seg as usize;
            let end = start + ssect.seg_count as usize;
            for seg in &mut level.map_data.segments_mut()[start..end] {
                if !self.pit_check_line(&tmbbox, ctrl, seg.linedef.as_mut()) {
                    return false;
                }
            }
        }
        true
    }

    /// Thing is generally the target.
    ///
    /// Function is intended to function similar to `PIT_CheckThing`
    fn pit_check_thing(
        &mut self,
        thing: &mut MapObject,
        endpoint: Vec2,
        ctrl: &mut SubSectorMinMax,
    ) -> bool {
        if thing.flags
            & (MapObjFlag::Solid as u32 | MapObjFlag::Special as u32 | MapObjFlag::Shootable as u32)
            == 0
        {
            return true;
        }

        let dist = thing.radius + self.radius;
        if (thing.xy.x - endpoint.x).abs() >= dist || (thing.xy.y - endpoint.y).abs() >= dist {
            // No hit
            return true;
        }

        if ptr::eq(self, thing) {
            // Ignore self
            return true;
        }

        if self.flags & MapObjFlag::Skullfly as u32 != 0 {
            let damage = ((p_random() % 8) + 1) * self.info.damage;
            thing.p_take_damage(Some(self), None, true, damage);

            self.momxy = Vec2::default();
            self.momz = 0.0;

            self.flags &= !(MapObjFlag::Skullfly as u32);
            self.set_state(self.info.spawnstate);
            return false;
        }

        // Special mssile handling
        if self.flags & MapObjFlag::Missile as u32 != 0 {
            if self.z > thing.z + thing.height {
                return true; // over
            }
            if self.z + thing.height < thing.z {
                return true; // under
            }

            if let Some(target) = self.target {
                let target = unsafe { (*target).mobj_mut() };

                if target.kind == thing.kind
                    || (target.kind == MapObjKind::MT_KNIGHT && thing.kind == MapObjKind::MT_KNIGHT)
                    || (target.kind == MapObjKind::MT_BRUISER
                        && thing.kind == MapObjKind::MT_BRUISER)
                {
                    // Don't hit same species as originator.
                    if ptr::eq(thing, target) {
                        return true;
                    }

                    if thing.kind != MapObjKind::MT_PLAYER {
                        // Explode, but do no damage.
                        // Let players missile other players.
                        return false;
                    }
                }

                if thing.flags & MapObjFlag::Shootable as u32 == 0 {
                    return thing.flags & MapObjFlag::Solid as u32 != MapObjFlag::Solid as u32;
                }

                let damage = ((p_random() % 8) + 1) * self.info.damage;
                thing.p_take_damage(Some(self), Some(target), false, damage);
            }
        }

        // Check special items
        if thing.flags & MapObjFlag::Special as u32 != 0 {
            let solid = thing.flags & MapObjFlag::Solid as u32 != MapObjFlag::Solid as u32;
            if self.flags & MapObjFlag::Pickup as u32 != 0 {
                // TODO: Fix getting skill level
                self.touch_special(thing);
            }
            return solid;
        }

        if thing.flags & MapObjFlag::Solid as u32 == MapObjFlag::Solid as u32 {
            // Already over it?
            if self.z >= thing.z + thing.height {
                // Step over it?
                if thing.z + thing.height - self.z > 24.0 {
                    return false;
                }
                ctrl.min_floor_z = thing.z + thing.height;
                return true; // over
            }
            if self.z + thing.height <= thing.z {
                return true; // under
            }
        }
        // final failsafe
        thing.flags & MapObjFlag::Solid as u32 != MapObjFlag::Solid as u32
    }

    /// PIT_CheckLine
    /// Adjusts tmfloorz and tmceilingz as lines are contacted
    fn pit_check_line(
        &mut self,
        tmbbox: &BBox,
        // point1: Vec2,
        // point2: Vec2,
        ctrl: &mut SubSectorMinMax,
        ld: &mut LineDef,
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

        if box_on_line_side(tmbbox, ld) != -1 {
            return true;
        }

        if ld.backsector.is_none() {
            // one-sided line
            return false;
        }

        if self.flags & MapObjFlag::Missile as u32 == 0 {
            if ld.flags & LineDefFlags::Blocking as u32 != 0 {
                return false; // explicitly blocking everything
            }

            if self.player.is_none() && ld.flags & LineDefFlags::BlockMonsters as u32 != 0 {
                return false; // block monsters only
            }
        }

        // Find the smallest/largest etc if group of line hits
        let portal = PortalZ::new(ld);
        if portal.top_z < ctrl.max_ceil_z {
            ctrl.max_ceil_z = portal.top_z;
            ctrl.sky_line = Some(MapPtr::new(ld));
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
            for l in ctrl.spec_hits.iter() {
                let ptr = MapPtr::new(ld);
                if l.inner as usize != ptr.inner as usize {
                    ctrl.spec_hits.push(ptr);
                    break;
                }
            }
            if ctrl.spec_hits.is_empty() {
                ctrl.spec_hits.push(MapPtr::new(ld));
            }
        }

        true
    }

    /// Loop until get a good move or stopped
    ///
    /// Doom function name `P_SlideMove`
    fn p_slide_move(&mut self) {
        // let ctrl = &mut level.mobj_ctrl;
        let mut hitcount = 0;
        self.best_slide = BestSlide::new();

        let leadx;
        let leady;
        let trailx;
        let traily;

        if self.momxy.x > 0.0 {
            leadx = self.xy.x + self.radius;
            trailx = self.xy.x - self.radius;
        } else {
            leadx = self.xy.x - self.radius;
            trailx = self.xy.x + self.radius;
        }

        if self.momxy.y > 0.0 {
            leady = self.xy.y + self.radius;
            traily = self.xy.y - self.radius;
        } else {
            leady = self.xy.y - self.radius;
            traily = self.xy.y + self.radius;
        }

        let level = unsafe { &mut *self.level };
        loop {
            if hitcount == 3 {
                self.stair_step();
                return;
            }

            // tail to front, centered
            let mut bsp_trace = BSPTrace::new_line(self.xy, self.xy + self.momxy, self.radius);
            let mut count = 0;
            bsp_trace.find_intercepts(level.map_data.start_node(), &level.map_data, &mut count);

            path_traverse(
                Vec2::new(leadx, leady),
                Vec2::new(leadx, leady) + self.momxy,
                PT_ADDLINES,
                level,
                |intercept| self.slide_traverse(intercept),
                &mut bsp_trace,
            );
            path_traverse(
                Vec2::new(trailx, leady),
                Vec2::new(trailx, leady) + self.momxy,
                PT_ADDLINES,
                level,
                |intercept| self.slide_traverse(intercept),
                &mut bsp_trace,
            );
            path_traverse(
                Vec2::new(leadx, traily),
                Vec2::new(leadx, traily) + self.momxy,
                PT_ADDLINES,
                level,
                |intercept| self.slide_traverse(intercept),
                &mut bsp_trace,
            );

            if self.best_slide.best_slide_frac == 2.0 {
                // The move most have hit the middle, so stairstep.
                self.stair_step();
                return;
            }

            self.best_slide.best_slide_frac -= 0.031250;
            if self.best_slide.best_slide_frac > 0.0 {
                let slide_move = self.momxy * self.best_slide.best_slide_frac; // bestfrac
                if !self.p_try_move(
                    self.xy.x + slide_move.x,
                    self.xy.y + slide_move.y,
                    &mut SubSectorMinMax::default(),
                ) {
                    self.stair_step();
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
            if self.p_try_move(endpoint.x, endpoint.y, &mut SubSectorMinMax::default()) {
                return;
            }

            hitcount += 1;
        }
    }

    fn blocking_intercept(&mut self, intercept: &Intercept) {
        if intercept.frac < self.best_slide.best_slide_frac {
            self.best_slide.second_slide_frac = self.best_slide.best_slide_frac;
            self.best_slide
                .second_slide_line
                .clone_from(&self.best_slide.best_slide_line);
            self.best_slide.best_slide_frac = intercept.frac;
            self.best_slide.best_slide_line.clone_from(&intercept.line);
        }
    }

    fn slide_traverse(&mut self, intercept: &Intercept) -> bool {
        if let Some(line) = &intercept.line {
            if (line.flags as usize) & LineDefFlags::TwoSided as usize == 0 {
                if line.point_on_side(self.xy) != 0 {
                    return true; // Don't hit backside
                }
                self.blocking_intercept(intercept);
            }

            // set openrange, opentop, openbottom
            let portal = PortalZ::new(line);
            if portal.range < self.height // doesn't fit
                || portal.top_z - self.z < self.height // thing is too high
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

    fn stair_step(&mut self) {
        // Line might have hit the middle, end-on?
        if !self.p_try_move(
            self.xy.x,
            self.xy.y + self.momxy.y,
            &mut SubSectorMinMax::default(),
        ) {
            self.p_try_move(
                self.xy.x + self.momxy.x,
                self.xy.y,
                &mut SubSectorMinMax::default(),
            );
        }
    }

    /// P_HitSlideLine
    fn hit_slide_line(&self, slide_move: &mut Vec2, line: &LineDef) {
        if matches!(line.slopetype, SlopeType::Horizontal) {
            slide_move.y = 0.0;
            return;
        }
        if matches!(line.slopetype, SlopeType::Vertical) {
            slide_move.x = 0.0;
            return;
        }

        // let side = line.point_on_side(slide_move);
        let line_angle = Angle::from_vector(line.delta);
        // if side == 1 {
        //     //line_angle += FRAC_PI_2;
        //     line_angle = Angle::from_vector(Vec2::new(line.delta.x * -1.0,
        // line.delta.y * -1.0)); }

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

    /// P_UseLines
    /// Looks for special lines in front of the player to activate.
    pub(crate) fn use_lines(&mut self) {
        let angle = self.angle.unit();

        let origin = self.xy;
        let endpoint = origin + (angle * USERANGE);

        let level = unsafe { &mut *self.level };

        let mut bsp_trace = BSPTrace::new_line(origin, endpoint, self.radius);
        let mut count = 0;
        bsp_trace.find_intercepts(level.map_data.start_node(), &level.map_data, &mut count);
        debug!("BSP: traversal count for use line: {count}");

        path_traverse(
            origin,
            endpoint,
            PT_ADDLINES,
            level,
            |intercept| self.use_traverse(intercept),
            &mut bsp_trace,
        );
    }

    /// PTR_UseTraverse
    fn use_traverse(&mut self, intercept: &Intercept) -> bool {
        if let Some(line) = &intercept.line {
            debug!(
                "Line v1 x:{},y:{}, v2 x:{},y:{}, special: {:?} - self.x:{},y:{} - frac {}",
                line.v1.x,
                line.v1.y,
                line.v2.x,
                line.v2.y,
                line.special,
                self.xy.x as i32,
                self.xy.y as i32,
                intercept.frac,
            );

            if line.special == 0 {
                // TODO: ordering is not great
                let portal = PortalZ::new(line);
                if portal.range <= 0.0 {
                    self.start_sound(sound_traits::SfxName::Noway);
                    // can't use through a wall
                    debug!("*UNNGFF!* Can't reach from this side");
                    return false;
                }
                // not a special line, but keep checking
                return true;
            }

            let side = line.point_on_side(self.xy);
            p_use_special_line(side as i32, line.clone(), self);
        }
        // can't use for than one special line in a row
        false
    }

    pub(crate) fn new_chase_dir(&mut self) {
        if self.target.is_none() {
            error!("new_chase_dir called with no target");
            return;
        }

        let old_dir = self.movedir;
        let mut dirs = [MoveDir::None, MoveDir::None, MoveDir::None];
        let turnaround = DIR_OPPOSITE[old_dir as usize];

        let target = unsafe { (**self.target.as_mut().unwrap()).mobj() };

        let dx = target.xy.x - self.xy.x;
        let dy = target.xy.y - self.xy.y;
        // Select a cardinal angle based on delta
        if dx > 10.0 {
            dirs[1] = MoveDir::East;
        } else if dx < -10.0 {
            dirs[1] = MoveDir::West;
        } else {
            dirs[1] = MoveDir::None;
        }

        if dy < -10.0 {
            dirs[2] = MoveDir::South;
        } else if dy > 10.0 {
            dirs[2] = MoveDir::North;
        } else {
            dirs[2] = MoveDir::None;
        }

        // try direct route
        if dirs[1] != MoveDir::None && dirs[2] != MoveDir::None {
            self.movedir = DIR_DIAGONALS[(((dy < 0.0) as usize) << 1) + (dx > 0.0) as usize];
            if self.movedir != turnaround && self.try_walk() {
                return;
            }
        }

        // try other directions
        if p_random() > 200 || dy.abs() > dx.abs() {
            dirs.swap(1, 2);
        }
        if dirs[1] == turnaround {
            dirs[1] = MoveDir::None;
        }
        if dirs[2] == turnaround {
            dirs[2] = MoveDir::None;
        }

        if dirs[1] != MoveDir::None {
            self.movedir = dirs[1];
            if self.try_walk() {
                // either moved forward or attacked
                return;
            }
        }

        if dirs[2] != MoveDir::None {
            self.movedir = dirs[2];
            if self.try_walk() {
                // either moved forward or attacked
                return;
            }
        }

        // there is no direct path to the player, so pick another direction.
        if old_dir != MoveDir::None {
            self.movedir = old_dir;
            if self.try_walk() {
                return;
            }
        }

        // randomly determine direction of search
        if p_random() & 1 != 0 {
            for t in MoveDir::East as usize..=MoveDir::SouthEast as usize {
                let tdir = MoveDir::from(t);
                if tdir != turnaround {
                    self.movedir = tdir;
                    if self.try_walk() {
                        return;
                    }
                }
            }
        } else {
            for t in (MoveDir::East as usize..=MoveDir::SouthEast as usize).rev() {
                let tdir = MoveDir::from(t);
                if tdir != turnaround {
                    self.movedir = tdir;
                    if self.try_walk() {
                        return;
                    }
                }
            }
        }

        if turnaround != MoveDir::None {
            self.movedir = turnaround;
            if self.try_walk() {
                return;
            }
        }

        // Can't move
        self.movedir = MoveDir::None;
    }

    /// Try to move in current direction. If blocked by a wall or other actor it
    /// returns false, otherwise tries to open a door if the block is one, and
    /// continue.
    pub(crate) fn try_walk(&mut self) -> bool {
        if !self.do_move() {
            return false;
        }
        self.movecount = p_random() & 15;
        true
    }

    pub(crate) fn do_move(&mut self) -> bool {
        if self.movedir == MoveDir::None {
            return false;
        }

        let tryx = self.xy.x + self.info.speed * DIR_XSPEED[self.movedir as usize];
        let tryy = self.xy.y + self.info.speed * DIR_YSPEED[self.movedir as usize];

        let mut specs = SubSectorMinMax::default();
        if !self.p_try_move(tryx, tryy, &mut specs) {
            // open any specials
            // TODO: if (actor->flags & MF_FLOAT && floatok)
            if self.flags & MapObjFlag::Float as u32 != 0 && specs.floatok {
                // must adjust height
                if self.z < specs.min_floor_z {
                    self.z += FLOATSPEED;
                } else {
                    self.z -= FLOATSPEED;
                }
                self.flags |= MapObjFlag::Infloat as u32;
                return true;
            }

            if specs.spec_hits.is_empty() {
                return false;
            }

            self.movedir = MoveDir::None;
            let mut good = false;
            for ld in &specs.spec_hits {
                if p_use_special_line(0, ld.clone(), self) || ld.special == 0 {
                    good = true;
                }
            }
            return good;
        } else {
            self.flags &= !(MapObjFlag::Infloat as u32);
        }

        if self.flags & MapObjFlag::Float as u32 == 0 {
            self.z = self.floorz;
        }

        true
    }
}

#[repr(usize)]
#[derive(Clone, Copy, PartialEq, PartialOrd)]
pub(crate) enum MoveDir {
    East,
    NorthEast,
    North,
    NorthWest,
    West,
    SouthWest,
    South,
    SouthEast,
    None,
    NumDirs,
}

impl From<usize> for MoveDir {
    fn from(w: usize) -> Self {
        if w >= MoveDir::NumDirs as usize {
            panic!("{} is not a variant of DirType", w);
        }
        unsafe { std::mem::transmute(w) }
    }
}

impl From<MoveDir> for Angle {
    fn from(d: MoveDir) -> Angle {
        match d {
            MoveDir::East => Angle::default(),
            MoveDir::NorthEast => Angle::new(FRAC_PI_4),
            MoveDir::North => Angle::new(FRAC_PI_2),
            MoveDir::NorthWest => Angle::new(FRAC_PI_2 + FRAC_PI_4),
            MoveDir::West => Angle::new(PI),
            MoveDir::SouthWest => Angle::new(PI + FRAC_PI_4),
            MoveDir::South => Angle::new(PI + FRAC_PI_2),
            MoveDir::SouthEast => Angle::new(PI + FRAC_PI_2 + FRAC_PI_4),
            _ => Angle::default(),
        }
    }
}

const DIR_OPPOSITE: [MoveDir; 9] = [
    MoveDir::West,
    MoveDir::SouthWest,
    MoveDir::South,
    MoveDir::SouthEast,
    MoveDir::East,
    MoveDir::NorthEast,
    MoveDir::North,
    MoveDir::NorthWest,
    MoveDir::None,
];

const DIR_DIAGONALS: [MoveDir; 4] = [
    MoveDir::NorthWest,
    MoveDir::NorthEast,
    MoveDir::SouthWest,
    MoveDir::SouthEast,
];

const DIR_XSPEED: [f32; 8] = [1.0, 0.47, 0.0, -0.47, -1.0, -0.47, 0.0, 0.47];
const DIR_YSPEED: [f32; 8] = [0.0, 0.47, 1.0, 0.47, 0.0, -0.47, -1.0, -0.47];
