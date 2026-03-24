//! Movement, collision handling.
//!
//! Almost all of the methods here are on `MapObject`.

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};
use std::ptr;

use log::{debug, error};

use crate::bsp_trace::{
    BestSlide, Intercept, PortalZ, box_on_line_side, path_traverse_blockmap, point_on_line_side
};
use crate::doom_def::{FLOATSPEED, USERANGE, VIEWHEIGHT};
use crate::env::specials::cross_special_line;
use crate::env::switch::p_use_special_line;
use crate::info::{STATES, StateData, StateNum};
use crate::{MapObjKind, MapObject};
use level::MapPtr;
use level::flags::LineDefFlags;
use level::map_defs::{LineDef, SlopeType};
use math::{ANG180, Angle, AngleInner, FixedT, p_random, r_point_to_angle};

use super::MapObjFlag;

const MAPBLOCKSHIFT: i32 = 23;
/// BBox index: max Y
const BOXTOP: usize = 0;
/// BBox index: min Y
const BOXBOTTOM: usize = 1;
/// BBox index: min X
const BOXLEFT: usize = 2;
/// BBox index: max X
const BOXRIGHT: usize = 3;
/// OG Doom MAXRADIUS in 16.16 fixed-point = 32 << 16
const MAXRADIUS_FIXED: i32 = 32 << 16;
pub const GRAVITY: i32 = 0x10000;
pub const MAXMOVE: i32 = 30 * 0x10000;
pub const STOPSPEED: i32 = 0x1000;
pub const FRICTION: i32 = 0xE800;
/// FRACUNIT + 1 sentinel for "no slide hit found"
const FRACUNIT_SENTINEL: i32 = 0x10001;
/// 0x800 fudge factor to avoid re-hitting the wall
const SLIDE_FUDGE: i32 = 0x800;

//const MAXSPECIALCROSS: i32 = 8;
pub const PT_ADDLINES: i32 = 1;
pub const PT_ADDTHINGS: i32 = 2;
pub const PT_EARLYOUT: i32 = 4;

/// Tracks the tightest floor/ceiling window and dropoff depth across all lines
/// contacted during a position check. `floatok` indicates a vertically
/// acceptable move.
pub struct SubSectorMinMax {
    floatok: bool,
    pub min_floor_z: FixedT,
    pub max_ceil_z: FixedT,
    max_dropoff: FixedT,
    sky_line: Option<MapPtr<LineDef>>,
    spec_hits: Vec<MapPtr<LineDef>>,
}

impl Default for SubSectorMinMax {
    fn default() -> Self {
        Self {
            floatok: false,
            min_floor_z: FixedT::ZERO,
            max_ceil_z: FixedT::ZERO,
            max_dropoff: FixedT::ZERO,
            sky_line: None,
            spec_hits: Vec::new(),
        }
    }
}

impl MapObject {
    /// Vertical movement and gravity for a map object (`P_ZMovement`).
    ///
    /// - Adjusts player viewheight when below floor
    /// - Applies vertical momentum, float-toward-target for flying monsters
    /// - Clips to floor (explodes missiles, applies gravity bounce for skulls)
    /// - Clips to ceiling (reverses skull momentum, explodes missiles)
    /// - Applies gravity when airborne and not `Nogravity`
    pub(crate) fn p_z_movement(&mut self) {
        if self.player.is_some() && self.z < self.floorz {
            unsafe {
                let player = &mut *(self.player.unwrap());
                player.viewheight -= self.floorz - self.z;
                player.deltaviewheight = (VIEWHEIGHT - player.viewheight).shr(3);
            }
        }

        // adjust height
        self.z += self.momz;

        if self.flags.contains(MapObjFlag::Float) {
            if let Some(target) = self.target {
                let target = unsafe { (*target).mobj() };

                // float down towards target if too close
                if !self.flags.contains(MapObjFlag::Skullfly)
                    && !self.flags.contains(MapObjFlag::Infloat)
                {
                    let dx = (self.x - target.x).doom_abs();
                    let dy = (self.y - target.y).doom_abs();
                    let dist = if dx < dy {
                        dx + dy - dx.shr(1)
                    } else {
                        dx + dy - dy.shr(1)
                    };
                    let delta = target.z + self.height.shr(1) - self.z;

                    if delta.is_negative() && dist < -(delta * 3) {
                        self.z -= FLOATSPEED;
                    } else if !delta.is_negative() && !delta.is_zero() && dist < delta * 3 {
                        self.z += FLOATSPEED;
                    }
                }
            }
        }

        // clip movement

        if self.z <= self.floorz {
            // hit the floor
            // TODO: The lost soul correction for old demos
            if self.flags.contains(MapObjFlag::Skullfly) {
                // the skull slammed into something
                self.momz = -self.momz;
            }

            if self.momz.is_negative() {
                if self.player.is_some() && self.momz < -8 {
                    // Squat down.
                    // Decrease viewheight for a moment
                    // after hitting the ground (hard),
                    // and utter appropriate sound.
                    unsafe {
                        let player = &mut *(self.player.unwrap());
                        player.deltaviewheight = self.momz.shr(3);
                    }
                }
                self.momz = FixedT::ZERO;
            }

            self.z = self.floorz;

            if self.flags.contains(MapObjFlag::Missile) && !self.flags.contains(MapObjFlag::Noclip)
            {
                self.p_explode_missile();
                return;
            }
        } else if !self.flags.contains(MapObjFlag::Nogravity) {
            if self.momz.is_zero() {
                self.momz = FixedT::from_fixed(-GRAVITY * 2);
            } else {
                self.momz = self.momz - FixedT::from_fixed(GRAVITY);
            }
        }

        if self.z + self.height > self.ceilingz {
            // hit the ceiling
            if !self.momz.is_negative() && !self.momz.is_zero() {
                self.momz = FixedT::ZERO;
            }
            self.z = self.ceilingz - self.height;

            if self.flags.contains(MapObjFlag::Skullfly) {
                // the skull slammed into something
                self.momz = -self.momz;
            }

            if self.flags.contains(MapObjFlag::Missile) && !self.flags.contains(MapObjFlag::Noclip)
            {
                self.p_explode_missile();
            }
        }
    }

    /// Horizontal movement with collision and wall sliding (`P_XYMovement`).
    ///
    /// - Clamps momentum to `MAXMOVE`, splits large moves into halves
    /// - Calls `p_try_move` for each sub-step; on failure:
    ///   - Players get wall sliding via `p_slide_move`
    ///   - Missiles explode (or silently remove against sky)
    ///   - Other objects stop
    /// - Applies friction when on the ground, stops at `STOPSPEED`
    /// - Resets player to standing sprite when stopped
    pub(crate) fn p_xy_movement(&mut self) {
        if self.momx.is_zero() && self.momy.is_zero() {
            if self.flags.contains(MapObjFlag::Skullfly) {
                self.flags.remove(MapObjFlag::Skullfly);
                self.momz = FixedT::ZERO;
                self.set_state(self.info.spawnstate);
            }
            return;
        }

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

        // P_XYMovement
        // `p_try_move` will apply the move if it is valid, and do specials, explodes
        // etc
        let maxmove = FixedT::from_fixed(MAXMOVE);
        self.momx = self.momx.clamp(-maxmove, maxmove);
        self.momy = self.momy.clamp(-maxmove, maxmove);
        let mut xmove = self.momx;
        let mut ymove = self.momy;
        let half_maxmove = FixedT::from_fixed(MAXMOVE / 2);
        let mut ptryx;
        let mut ptryy;

        while !xmove.is_zero() || !ymove.is_zero() {
            // OG only checks positive overflow (known vanilla Doom quirk)
            if xmove > half_maxmove || ymove > half_maxmove {
                // OG: ptryx = mo->x + xmove/2 (C int div, toward zero)
                ptryx = self.x + xmove.half_toward_zero();
                ptryy = self.y + ymove.half_toward_zero();
                // OG: xmove >>= 1 (arithmetic shift, toward -inf)
                xmove = xmove.shr(1);
                ymove = ymove.shr(1);
            } else {
                ptryx = self.x + xmove;
                ptryy = self.y + ymove;
                xmove = FixedT::ZERO;
                ymove = FixedT::ZERO;
            }

            let mut ctrl = SubSectorMinMax::default();
            if !self.p_try_move(ptryx, ptryy, &mut ctrl) {
                if self.player.is_some() {
                    self.p_slide_move();
                } else if self.flags.contains(MapObjFlag::Missile) {
                    if let Some(line) = ctrl.sky_line {
                        if let Some(back) = line.backsector.as_ref() {
                            if back.ceilingpic == self.level().sky_num {
                                self.remove();
                                return;
                            }
                        }
                    }
                    self.p_explode_missile(); //
                } else {
                    self.momx = FixedT::ZERO;
                    self.momy = FixedT::ZERO;
                }
            }
        }

        // slow down
        if self
            .flags
            .intersects(MapObjFlag::Missile | MapObjFlag::Skullfly)
        {
            return; // no friction for missiles ever
        }

        if self.z > self.floorz {
            return; // no friction when airborne
        }

        let floorheight = FixedT::from_fixed(self.subsector.sector.floorheight.to_fixed_raw());

        if self.flags.contains(MapObjFlag::Corpse) {
            // do not stop sliding
            //  if halfway off a step with some momentum
            let frac4 = FixedT::from_fixed(0x10000 / 4);
            if (self.momx > frac4 || self.momx < -frac4 || self.momy > frac4 || self.momy < -frac4)
                && self.floorz != floorheight
            {
                return;
            }
        }

        let mut pfwd = -1;
        let mut pside = -1;
        if let Some(player) = self.player() {
            pfwd = player.cmd.forwardmove;
            pside = player.cmd.sidemove;
        }

        let stopspeed = FixedT::from_fixed(STOPSPEED);
        if self.momx > -stopspeed
            && self.momx < stopspeed
            && self.momy > -stopspeed
            && self.momy < stopspeed
            && (self.player.is_none() || pfwd == 0 && pside == 0)
        {
            if self.player().is_some() {
                let state_idx = (self.state as *const _ as usize - STATES.as_ptr() as usize)
                    / std::mem::size_of::<StateData>();
                let run1 = StateNum::PLAY_RUN1 as usize;
                if state_idx >= run1 && state_idx < run1 + 4 {
                    self.set_state(StateNum::PLAY);
                }
            }
            self.momx = FixedT::ZERO;
            self.momy = FixedT::ZERO;
        } else {
            let friction = FixedT::from_fixed(FRICTION);
            self.momx = self.momx.fixed_mul(friction);
            self.momy = self.momy.fixed_mul(friction);
        }
    }

    /// Attempt to move to `(ptryx, ptryy)`. Runs position check, validates step
    /// height and ceiling clearance, repositions the object, and triggers
    /// crossed special lines. Returns false if blocked.
    pub(crate) fn p_try_move(
        &mut self,
        ptryx: FixedT,
        ptryy: FixedT,
        ctrl: &mut SubSectorMinMax,
    ) -> bool {
        ctrl.floatok = false;
        if !self.p_check_position(ptryx, ptryy, ctrl) {
            return false;
        }

        if !self.flags.contains(MapObjFlag::Noclip) {
            if ctrl.max_ceil_z - ctrl.min_floor_z < self.height {
                return false;
            }
            ctrl.floatok = true;

            if !self.flags.contains(MapObjFlag::Teleport) && ctrl.max_ceil_z - self.z < self.height
            {
                return false;
            }

            if !self.flags.contains(MapObjFlag::Teleport) && ctrl.min_floor_z - self.z > 24 {
                return false;
            }

            if !self
                .flags
                .intersects(MapObjFlag::Dropoff | MapObjFlag::Float)
                && ctrl.min_floor_z - ctrl.max_dropoff > 24
            {
                return false;
            }
        }

        // the move is ok,
        // so link the thing into its new position
        unsafe {
            self.unset_thing_position();
        }

        let old_x = self.x;
        let old_y = self.y;

        self.floorz = ctrl.min_floor_z;
        self.ceilingz = ctrl.max_ceil_z;
        self.x = ptryx;
        self.y = ptryy;

        unsafe {
            self.set_thing_position();
        }

        if !self
            .flags
            .intersects(MapObjFlag::Teleport | MapObjFlag::Noclip)
        {
            for ld in &ctrl.spec_hits {
                // see if the line was crossed
                let side = point_on_line_side(self.x, self.y, ld);
                let old_side = point_on_line_side(old_x, old_y, ld);
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
    pub(crate) fn p_check_position(
        &mut self,
        endpoint_x: FixedT,
        endpoint_y: FixedT,
        ctrl: &mut SubSectorMinMax,
    ) -> bool {
        let ep_x_raw = endpoint_x.to_fixed_raw();
        let ep_y_raw = endpoint_y.to_fixed_raw();
        let rad_raw = self.radius.to_fixed_raw();
        // OG: tmbbox in fixed-point
        let tmbbox_int: [i32; 4] = [
            ep_y_raw + rad_raw, // BOXTOP
            ep_y_raw - rad_raw, // BOXBOTTOM
            ep_x_raw - rad_raw, // BOXLEFT
            ep_x_raw + rad_raw, // BOXRIGHT
        ];

        let level = unsafe { &mut *self.level };
        let newsubsec = level.level_data.point_in_subsector(endpoint_x, endpoint_y);

        // The base floor / ceiling is from the subsector
        // that contains the point.
        let floor_z = FixedT::from_fixed(newsubsec.sector.floorheight.to_fixed_raw());
        ctrl.min_floor_z = floor_z;
        ctrl.max_dropoff = floor_z;
        ctrl.max_ceil_z = FixedT::from_fixed(newsubsec.sector.ceilingheight.to_fixed_raw());

        if self.flags.contains(MapObjFlag::Noclip) {
            return true;
        }

        let bm = level.level_data.blockmap();
        let orgx = bm.x_origin;
        let orgy = bm.y_origin;
        let bmw = bm.columns;
        let bmh = bm.rows;

        // Things: extend by MAXRADIUS (OG P_CheckPosition)
        let xl = (ep_x_raw - rad_raw - orgx - MAXRADIUS_FIXED) >> MAPBLOCKSHIFT;
        let xh = (ep_x_raw + rad_raw - orgx + MAXRADIUS_FIXED) >> MAPBLOCKSHIFT;
        let yl = (ep_y_raw - rad_raw - orgy - MAXRADIUS_FIXED) >> MAPBLOCKSHIFT;
        let yh = (ep_y_raw + rad_raw - orgy + MAXRADIUS_FIXED) >> MAPBLOCKSHIFT;

        for by in yl..=yh {
            for bx in xl..=xh {
                if bx < 0 || by < 0 || bx >= bmw || by >= bmh {
                    continue;
                }
                let idx = (by * bmw + bx) as usize;
                let mut mobj_ptr = level.blocklinks[idx];
                while let Some(ptr) = mobj_ptr {
                    let thing = unsafe { &mut *ptr };
                    mobj_ptr = thing.b_next;
                    if !self.pit_check_thing(thing, endpoint_x, endpoint_y, ctrl) {
                        return false;
                    }
                }
            }
        }

        // Lines: no MAXRADIUS extension (OG P_CheckPosition)
        let xl = (ep_x_raw - rad_raw - orgx) >> MAPBLOCKSHIFT;
        let xh = (ep_x_raw + rad_raw - orgx) >> MAPBLOCKSHIFT;
        let yl = (ep_y_raw - rad_raw - orgy) >> MAPBLOCKSHIFT;
        let yh = (ep_y_raw + rad_raw - orgy) >> MAPBLOCKSHIFT;

        level.valid_count = level.valid_count.wrapping_add(1);
        let valid = level.valid_count;

        for by in yl..=yh {
            for bx in xl..=xh {
                if bx < 0 || by < 0 || bx >= bmw || by >= bmh {
                    continue;
                }
                let bidx = (by * bmw + bx) as usize;
                let bm = level.level_data.blockmap();
                let start = bm.block_offsets[bidx];
                let end = bm.block_offsets[bidx + 1];
                for i in start..end {
                    let mut line = bm.block_lines[i].clone();
                    if line.valid_count == valid {
                        continue;
                    }
                    line.valid_count = valid;
                    if !self.pit_check_line(&tmbbox_int, ctrl, line.as_mut()) {
                        return false;
                    }
                }
            }
        }

        true
    }

    /// OG `PIT_CheckThing` — collision response between `self` and `thing`.
    /// Returns false to block movement.
    fn pit_check_thing(
        &mut self,
        thing: &mut MapObject,
        endpoint_x: FixedT,
        endpoint_y: FixedT,
        _ctrl: &mut SubSectorMinMax,
    ) -> bool {
        if !thing
            .flags
            .intersects(MapObjFlag::Solid | MapObjFlag::Special | MapObjFlag::Shootable)
        {
            return true;
        }

        let dist = thing.radius + self.radius;
        let dx_abs = (thing.x - endpoint_x).doom_abs();
        let dy_abs = (thing.y - endpoint_y).doom_abs();

        if dx_abs >= dist || dy_abs >= dist {
            // No hit
            return true;
        }

        if ptr::eq(
            self as *const _ as *const u8,
            thing as *const _ as *const u8,
        ) {
            // Ignore self
            return true;
        }

        if self.flags.contains(MapObjFlag::Skullfly) {
            let damage = ((p_random() % 8) + 1) * self.info.damage;
            // OG: P_DamageMobj(thing, tmthing, tmthing, damage)
            let self_ptr = unsafe { &mut *(self as *mut MapObject) };
            thing.p_take_damage(Some((self.x, self.y, self.z)), Some(self_ptr), damage);

            self.momx = FixedT::ZERO;
            self.momy = FixedT::ZERO;
            self.momz = FixedT::ZERO;

            self.flags.remove(MapObjFlag::Skullfly);
            self.set_state(self.info.spawnstate);
            return false;
        }

        // Special missile handling
        if self.flags.contains(MapObjFlag::Missile) {
            if self.z > thing.z + thing.height {
                return true; // over
            }
            if self.z + self.height < thing.z {
                return true; // under
            }

            if let Some(target) = self.target {
                let target = unsafe { (*target).mobj_mut() };

                // OG: same species OR knight↔bruiser cross-species
                if target.kind == thing.kind
                    || (target.kind == MapObjKind::MT_KNIGHT
                        && thing.kind == MapObjKind::MT_BRUISER)
                    || (target.kind == MapObjKind::MT_BRUISER
                        && thing.kind == MapObjKind::MT_KNIGHT)
                {
                    // Don't hit same species as originator.
                    if ptr::eq(
                        thing as *const _ as *const u8,
                        target as *const _ as *const u8,
                    ) {
                        return true;
                    }

                    if thing.kind != MapObjKind::MT_PLAYER {
                        // Explode, but do no damage.
                        // Let players missile other players.
                        return false;
                    }
                }

                if !thing.flags.contains(MapObjFlag::Shootable) {
                    return !thing.flags.contains(MapObjFlag::Solid);
                }

                let damage = ((p_random() % 8) + 1) * self.info.damage;
                thing.p_take_damage(Some((self.x, self.y, self.z)), Some(target), damage);
                return false;
            }
        }

        // Check special items
        if thing.flags.contains(MapObjFlag::Special) {
            let solid = !thing.flags.contains(MapObjFlag::Solid);
            if self.flags.contains(MapObjFlag::Pickup) {
                self.touch_special(thing);
            }
            return solid;
        }

        if thing.flags.contains(MapObjFlag::Solid) {
            return false;
        }
        true
    }

    /// OG `PIT_CheckLine` -- test a linedef against the move bbox. Narrows
    /// `ctrl.max_ceil_z`/`ctrl.min_floor_z`/`ctrl.max_dropoff` for portal
    /// lines, records special lines. Returns false for blocking one-sided
    /// lines.
    fn pit_check_line(
        &mut self,
        tmbbox: &[i32; 4],
        ctrl: &mut SubSectorMinMax,
        ld: &mut LineDef,
    ) -> bool {
        if tmbbox[BOXRIGHT] <= ld.bbox_int[BOXLEFT]
            || tmbbox[BOXLEFT] >= ld.bbox_int[BOXRIGHT]
            || tmbbox[BOXTOP] <= ld.bbox_int[BOXBOTTOM]
            || tmbbox[BOXBOTTOM] >= ld.bbox_int[BOXTOP]
        {
            return true;
        }

        let bols = box_on_line_side(tmbbox, ld);
        if bols != -1 {
            return true;
        }

        if ld.backsector.is_none() {
            // one-sided line
            return false;
        }

        if !self.flags.contains(MapObjFlag::Missile) {
            if ld.flags.contains(LineDefFlags::Blocking) {
                return false; // explicitly blocking everything
            }

            if self.player.is_none() && ld.flags.contains(LineDefFlags::BlockMonsters) {
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
            ctrl.spec_hits.push(MapPtr::new(ld));
        }

        true
    }

    /// Loop until get a good move or stopped
    ///
    /// Doom function name `P_SlideMove`
    fn p_slide_move(&mut self) {
        let sentinel: FixedT = FixedT::from_fixed(FRACUNIT_SENTINEL);
        let fudge: FixedT = FixedT::from_fixed(SLIDE_FUDGE);
        let mut hitcount = 0;

        let level = unsafe { &mut *self.level };
        loop {
            hitcount += 1;
            if hitcount == 3 {
                self.stair_step();
                return;
            }

            // Recompute lead/trail from current position each retry
            let leadx;
            let leady;
            let trailx;
            let traily;
            if self.momx > FixedT::ZERO {
                leadx = self.x + self.radius;
                trailx = self.x - self.radius;
            } else {
                leadx = self.x - self.radius;
                trailx = self.x + self.radius;
            }
            if self.momy > FixedT::ZERO {
                leady = self.y + self.radius;
                traily = self.y - self.radius;
            } else {
                leady = self.y - self.radius;
                traily = self.y + self.radius;
            }

            // Reset best slide each retry
            self.best_slide = BestSlide::default();
            self.best_slide.best_slide_frac = sentinel;

            path_traverse_blockmap(
                leadx,
                leady,
                leadx + self.momx,
                leady + self.momy,
                PT_ADDLINES,
                level,
                |intercept| self.slide_traverse(intercept),
            );
            path_traverse_blockmap(
                trailx,
                leady,
                trailx + self.momx,
                leady + self.momy,
                PT_ADDLINES,
                level,
                |intercept| self.slide_traverse(intercept),
            );
            path_traverse_blockmap(
                leadx,
                traily,
                leadx + self.momx,
                traily + self.momy,
                PT_ADDLINES,
                level,
                |intercept| self.slide_traverse(intercept),
            );

            // No wall hit — stairstep
            if self.best_slide.best_slide_frac == sentinel {
                self.stair_step();
                return;
            }

            // Fudge to avoid re-hitting the wall
            self.best_slide.best_slide_frac = self.best_slide.best_slide_frac - fudge;
            if self.best_slide.best_slide_frac > FixedT::ZERO {
                let frac = self.best_slide.best_slide_frac;
                let newx = self.momx.fixed_mul(frac);
                let newy = self.momy.fixed_mul(frac);
                if !self.p_try_move(
                    self.x + newx,
                    self.y + newy,
                    &mut SubSectorMinMax::default(),
                ) {
                    self.stair_step();
                    return;
                }
            }

            // Now continue along the wall — calculate remainder
            self.best_slide.best_slide_frac =
                FixedT::ONE - (self.best_slide.best_slide_frac + fudge);
            if self.best_slide.best_slide_frac > FixedT::ONE {
                self.best_slide.best_slide_frac = FixedT::ONE;
            }
            if self.best_slide.best_slide_frac <= FixedT::ZERO {
                return;
            }

            let frac = self.best_slide.best_slide_frac;
            let mut tmxmove = self.momx.fixed_mul(frac);
            let mut tmymove = self.momy.fixed_mul(frac);

            // Clip the moves
            if let Some(best_slide_line) = self.best_slide.best_slide_line.as_ref() {
                self.hit_slide_line(&mut tmxmove, &mut tmymove, best_slide_line);
            }

            self.momx = tmxmove;
            self.momy = tmymove;

            if self.p_try_move(
                self.x + tmxmove,
                self.y + tmymove,
                &mut SubSectorMinMax::default(),
            ) {
                return;
            }
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
            if !line.flags.contains(LineDefFlags::TwoSided) {
                if point_on_line_side(self.x, self.y, line) != 0 {
                    return true;
                }
                self.blocking_intercept(intercept);
                return false;
            }

            let portal = PortalZ::new(line);
            let doesnt_fit = portal.range < self.height;
            let too_high = portal.top_z - self.z < self.height;
            let step_too_high = portal.bottom_z - self.z > 24;

            if doesnt_fit || too_high || step_too_high {
                self.blocking_intercept(intercept);
                return false;
            }
            return true;
        }

        self.blocking_intercept(intercept);
        false
    }

    fn stair_step(&mut self) {
        // Line might have hit the middle, end-on?
        if !self.p_try_move(self.x, self.y + self.momy, &mut SubSectorMinMax::default()) {
            self.p_try_move(self.x + self.momx, self.y, &mut SubSectorMinMax::default());
        }
    }

    /// P_HitSlideLine — clip slide movement along a wall
    fn hit_slide_line(&self, tmxmove: &mut FixedT, tmymove: &mut FixedT, line: &LineDef) {
        if matches!(line.slopetype, SlopeType::Horizontal) {
            *tmymove = FixedT::ZERO;
            return;
        }
        if matches!(line.slopetype, SlopeType::Vertical) {
            *tmxmove = FixedT::ZERO;
            return;
        }

        let side = point_on_line_side(self.x, self.y, line);

        let mut line_bam = r_point_to_angle(
            FixedT::from_fixed(line.delta_fp[0]),
            FixedT::from_fixed(line.delta_fp[1]),
        );
        if side == 1 {
            line_bam = line_bam.wrapping_add(ANG180);
        }

        let move_bam = r_point_to_angle(*tmxmove, *tmymove);
        let mut delta_bam = move_bam.wrapping_sub(line_bam);
        if delta_bam > ANG180 {
            delta_bam = delta_bam.wrapping_add(ANG180);
        }

        let movelen_dx = tmxmove.doom_abs();
        let movelen_dy = tmymove.doom_abs();
        let movelen = if movelen_dx < movelen_dy {
            movelen_dx + movelen_dy - movelen_dx.shr(1)
        } else {
            movelen_dx + movelen_dy - movelen_dy.shr(1)
        };

        let newlen = movelen.fixed_mul(FixedT::cos_bam(delta_bam));

        *tmxmove = newlen.fixed_mul(FixedT::cos_bam(line_bam));
        *tmymove = newlen.fixed_mul(FixedT::sin_bam(line_bam));
    }

    /// P_UseLines
    /// Looks for special lines in front of the player to activate.
    pub(crate) fn use_lines(&mut self) {
        let bam = self.angle.to_bam();
        let cos = math::fine_cos(bam);
        let sin = math::fine_sin(bam);
        // OG: x + (USERANGE >> FRACBITS) * finecosine[angle]
        let ep_x = self.x + cos * USERANGE;
        let ep_y = self.y + sin * USERANGE;

        let level = unsafe { &mut *self.level };
        path_traverse_blockmap(
            self.x,
            self.y,
            ep_x,
            ep_y,
            PT_ADDLINES,
            level,
            |intercept| self.use_traverse(intercept),
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
                self.x.to_i32(),
                self.y.to_i32(),
                intercept.frac,
            );

            if line.special == 0 {
                // TODO: ordering is not great
                let portal = PortalZ::new(line);
                if portal.range <= 0 {
                    self.start_sound(sound_common::SfxName::Noway);
                    // can't use through a wall
                    debug!("*UNNGFF!* Can't reach from this side");
                    return false;
                }
                // not a special line, but keep checking
                return true;
            }

            let side = point_on_line_side(self.x, self.y, line);
            p_use_special_line(side as i32, line.clone(), self);
            // BOOM PassUse: allow activating multiple lines in one press
            if line.flags.contains(LineDefFlags::PassUse) {
                return true;
            }
        }
        false
    }

    /// Pick a new movement direction toward the current target
    /// (`P_NewChaseDir`).
    ///
    /// - Tries the diagonal toward the target first
    /// - Falls back to cardinal axes, then random sweep
    /// - Avoids 180-degree turnarounds unless no other option
    pub(crate) fn new_chase_dir(&mut self) {
        if self.target.is_none() {
            error!("new_chase_dir called with no target");
            return;
        }

        let old_dir = self.movedir;
        let mut dirs = [MoveDir::None, MoveDir::None, MoveDir::None];
        let turnaround = DIR_OPPOSITE[old_dir as usize];

        let target = unsafe { (**self.target.as_mut().unwrap()).mobj() };

        let dx = target.x - self.x;
        let dy = target.y - self.y;
        let ten: FixedT = 10.into();
        // Select a cardinal angle based on delta
        if dx > ten {
            dirs[1] = MoveDir::East;
        } else if dx < -ten {
            dirs[1] = MoveDir::West;
        } else {
            dirs[1] = MoveDir::None;
        }

        if dy < -ten {
            dirs[2] = MoveDir::South;
        } else if dy > ten {
            dirs[2] = MoveDir::North;
        } else {
            dirs[2] = MoveDir::None;
        }

        // try direct route
        if dirs[1] != MoveDir::None && dirs[2] != MoveDir::None {
            self.movedir =
                DIR_DIAGONALS[(((dy < FixedT::ZERO) as usize) << 1) + (dx > FixedT::ZERO) as usize];
            if self.movedir != turnaround && self.try_walk() {
                return;
            }
        }

        // try other directions
        if p_random() > 200 || dy.doom_abs() > dx.doom_abs() {
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

    /// Attempt a move in the current `movedir`. Calls `do_move` and resets
    /// `movecount` on success.
    pub(crate) fn try_walk(&mut self) -> bool {
        if !self.do_move() {
            return false;
        }
        self.movecount = p_random() & 15;
        true
    }

    /// Execute one movement step: adjusts z for floaters, attempts
    /// `p_try_move`, tries to open blocking special lines. Returns false if
    /// blocked.
    pub(crate) fn do_move(&mut self) -> bool {
        if self.movedir == MoveDir::None {
            return false;
        }

        let tryx = self.x + FixedT::from_fixed(self.info.speed * DIR_XSPEED[self.movedir as usize]);
        let tryy = self.y + FixedT::from_fixed(self.info.speed * DIR_YSPEED[self.movedir as usize]);

        let mut specs = SubSectorMinMax::default();
        if !self.p_try_move(tryx, tryy, &mut specs) {
            if self.flags.contains(MapObjFlag::Float) && specs.floatok {
                // must adjust height
                if self.z < specs.min_floor_z {
                    self.z += FLOATSPEED;
                } else {
                    self.z -= FLOATSPEED;
                }
                self.flags.insert(MapObjFlag::Infloat);
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
            self.flags.remove(MapObjFlag::Infloat);
        }

        if !self.flags.contains(MapObjFlag::Float) {
            self.z = self.floorz;
        }

        true
    }
}

#[repr(usize)]
#[derive(Clone, Copy, PartialEq, PartialOrd, Debug)]
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

impl<A: AngleInner> From<MoveDir> for Angle<A> {
    fn from(d: MoveDir) -> Angle<A> {
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

const DIR_XSPEED: [i32; 8] = [65536, 47000, 0, -47000, -65536, -47000, 0, 47000];
const DIR_YSPEED: [i32; 8] = [0, 47000, 65536, 47000, 0, -47000, -65536, -47000];
