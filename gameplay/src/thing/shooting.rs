//! Shooting and aiming.
#[cfg(feature = "hprof")]
use coarse_prof::profile;
use math::{
    ANG90, ANG270, Bam, DivLineFixed, FixedT, intercept_vector_fixed, p_aprox_distance, p_random, r_point_to_angle
};
use sound_common::SfxName;

use crate::bsp_trace::{BSPTrace, Intercept, PortalZ, p_divline_side_raw, path_traverse_blockmap};
use crate::doom_def::{MAXRADIUS, MELEERANGE};
use crate::env::specials::shoot_special_line;
use crate::info::{MOBJINFO, StateNum};
use crate::{MapObjKind, MapObject};
use level::map_defs::{LineDef, is_subsector, subsector_index};
use level::{LevelData, LineDefFlags, MapPtr};
use math::Angle;

use super::{MapObjFlag, PT_ADDLINES, PT_ADDTHINGS};

const MAPBLOCKSHIFT: i32 = 23;

impl MapObject {
    /// Transition a missile into its death/explosion state
    /// (`P_ExplodeMissile`).
    ///
    /// Zeroes momentum, sets death state with randomized tic offset,
    /// removes `Missile` flag, and plays the death sound.
    pub(crate) fn p_explode_missile(&mut self) {
        self.momx = FixedT::ZERO;
        self.momy = FixedT::ZERO;
        self.momz = FixedT::ZERO;
        self.set_state(MOBJINFO[self.kind as usize].deathstate);

        self.tics -= p_random() & 3;

        if self.tics < 1 {
            self.tics = 1;
        }

        self.flags.remove(MapObjFlag::Missile);

        if self.info.deathsound != SfxName::None {
            self.start_sound(self.info.deathsound);
        }
    }

    /// Build a `BSPTrace` along this object's facing angle out to `distance`.
    /// Used to pre-collect intersected subsectors for shooting/aiming.
    pub(crate) fn get_shoot_bsp_trace(&self, distance: FixedT) -> BSPTrace {
        let bam = self.angle.to_bam();
        let cos = math::fine_cos(bam);
        let sin = math::fine_sin(bam);
        // OG: x + (distance >> FRACBITS) * finecosine[angle]
        let ex = self.x + cos * distance.to_i32();
        let ey = self.y + sin * distance.to_i32();
        // Radius captures overlapping demons in adjacent subsectors
        let mut bsp_trace = BSPTrace::new_line(self.x, self.y, ex, ey, FixedT::from_f32(20.0));
        let mut count = 0;
        let level = unsafe { &mut *self.level };
        bsp_trace.find_intercepts(level.level_data.start_node(), &level.level_data, &mut count);
        bsp_trace
    }

    /// Auto-aim traversal along the shooter's facing angle (`P_AimLineAttack`).
    ///
    /// Walks the blockmap collecting line and thing intercepts, narrowing
    /// the vertical slope window at each two-sided line portal. Returns
    /// the first shootable thing hit (with its aim slope) or `None`.
    pub(crate) fn aim_line_attack(
        &mut self,
        distance: FixedT,
        _bsp_trace: &mut BSPTrace,
    ) -> Option<AimResult> {
        let shootz = self.z + self.height.shr(1) + 8;
        let mut aim_traverse = SubSectTraverse::new(
            // OG: topslope = 100*FRACUNIT/160, bottomslope = -100*FRACUNIT/160
            FixedT::from_fixed(100 * 0x10000 / 160),
            FixedT::from_fixed(-100 * 0x10000 / 160),
            distance,
            shootz,
        );

        // OG: x + (distance >> FRACBITS) * finecosine[angle]
        let bam = self.angle.to_bam();
        let cos = math::fine_cos(bam);
        let sin = math::fine_sin(bam);
        let xy2_x = self.x + cos * distance.to_i32();
        let xy2_y = self.y + sin * distance.to_i32();

        let level = unsafe { &mut *self.level };
        path_traverse_blockmap(
            self.x,
            self.y,
            xy2_x,
            xy2_y,
            PT_ADDLINES | PT_ADDTHINGS,
            level,
            |t| aim_traverse.check_aim(self, t),
        );

        aim_traverse.result()
    }

    /// Fire a hitscan line attack along `angle` with fixed `aim_slope`
    /// (`P_LineAttack`).
    ///
    /// Preceded by `aim_line_attack` in many cases, so `BSPTrace` can be
    /// shared. Walks the blockmap, spawning puffs on walls or blood/damage
    /// on things. Nudges the trace origin to avoid blockmap cell-boundary
    /// edge cases.
    pub(crate) fn shoot_line_attack(
        &mut self,
        attack_range: FixedT,
        angle: Angle<Bam>,
        aim_slope: FixedT,
        damage: i32,
        _bsp_trace: &mut BSPTrace,
    ) {
        let shootz = self.z + self.height.shr(1) + 8;
        // OG: x + (distance >> FRACBITS) * finecosine[angle]
        let bam = angle.to_bam();
        let cos = math::fine_cos(bam);
        let sin = math::fine_sin(bam);
        let xy2_x = self.x + cos * attack_range.to_i32();
        let xy2_y = self.y + sin * attack_range.to_i32();
        let level = unsafe { &mut *self.level };
        // OG: trace origin is nudged inside P_PathTraverse before trace
        // global is set. Replicate the nudge for ShootTraverse.
        let bm = level.level_data.blockmap();
        let mut ox = self.x.to_fixed_raw();
        let mut oy = self.y.to_fixed_raw();
        if (ox - bm.x_origin) & ((128 << 16) - 1) == 0 {
            ox += 0x10000;
        }
        if (oy - bm.y_origin) & ((128 << 16) - 1) == 0 {
            oy += 0x10000;
        }
        let trace_x = FixedT::from_fixed(ox);
        let trace_y = FixedT::from_fixed(oy);
        let trace_dx = xy2_x - trace_x;
        let trace_dy = xy2_y - trace_y;
        let mut shoot_traverse = ShootTraverse::new(
            aim_slope,
            attack_range,
            damage,
            shootz,
            trace_x,
            trace_y,
            trace_dx,
            trace_dy,
            self.level().sky_num,
        );
        path_traverse_blockmap(
            self.x,
            self.y,
            xy2_x,
            xy2_y,
            PT_ADDLINES | PT_ADDTHINGS,
            level,
            |intercept| shoot_traverse.resolve(self, intercept),
        );
    }

    /// Source is the creature that caused the explosion at spot(self).
    ///
    /// Doom function name `P_RadiusAttack`
    pub fn radius_attack(&mut self, damage: i32) {
        let level = unsafe { &mut *self.level };
        level.valid_count = level.valid_count.wrapping_add(1);
        let valid = level.valid_count;

        // OG: dist = (damage + MAXRADIUS) << FRACBITS
        let dist_raw = (damage + MAXRADIUS as i32) << 16;
        let bm = level.level_data.blockmap();
        let orgx = bm.x_origin;
        let orgy = bm.y_origin;
        let bmw = bm.columns;
        let bmh = bm.rows;
        let sx = self.x.to_fixed_raw();
        let sy = self.y.to_fixed_raw();

        let xl = (sx - dist_raw - orgx) >> MAPBLOCKSHIFT;
        let xh = (sx + dist_raw - orgx) >> MAPBLOCKSHIFT;
        let yl = (sy - dist_raw - orgy) >> MAPBLOCKSHIFT;
        let yh = (sy + dist_raw - orgy) >> MAPBLOCKSHIFT;

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
                    if !self.radius_damage_other(thing, damage, valid) {
                        return;
                    }
                }
            }
        }
    }

    /// Cause damage to other thing if in radius of self
    fn radius_damage_other(&mut self, other: &mut MapObject, damage: i32, valid: usize) -> bool {
        if other.valid_count == valid {
            return true;
        }
        other.valid_count = valid;

        if !other.flags.contains(MapObjFlag::Shootable) {
            return true;
        }

        if matches!(other.kind, MapObjKind::MT_CYBORG | MapObjKind::MT_SPIDER) {
            return true;
        }

        let dx = (other.x - self.x).doom_abs();
        let dy = (other.y - self.y).doom_abs();
        // OG: dist = max(dx,dy); dist = (dist - thing->radius) >> FRACBITS
        let mut dist = if dx > dy { dx } else { dy };
        dist = dist - other.radius;
        let dist_i = if dist.to_i32() < 0 { 0 } else { dist.to_i32() };

        if dist_i >= damage {
            return true; // out of range
        }

        // OG: P_DamageMobj(thing, bombspot, bombsource, bombdamage - dist)
        if self.check_sight_target(other) {
            let actual = damage - dist_i;
            other.p_take_damage(
                Some((self.x, self.y, self.z)),
                self.target.map(|t| unsafe { (*t).mobj_mut() }),
                actual,
            );
        }
        true
    }

    /// Determine the aim slope for bullet weapons (`P_BulletSlope`).
    ///
    /// Tries auto-aim at the current angle, then +/- 5.625 degrees.
    /// Restores the original angle after probing.
    pub(crate) fn bullet_slope(
        &mut self,
        distance: FixedT,
        bsp_trace: &mut BSPTrace,
    ) -> Option<AimResult> {
        let mut bullet_slope = self.aim_line_attack(distance, bsp_trace);
        let old_angle = self.angle;
        if bullet_slope.is_none() {
            // OG: an += 1<<26 (ANG90/16 = 5.625 degrees)
            self.angle = Angle::from_bam(self.angle.to_bam().wrapping_add(1 << 26));
            bullet_slope = self.aim_line_attack(distance, bsp_trace);
            if bullet_slope.is_none() {
                // OG: an -= 2<<26
                self.angle = Angle::from_bam(self.angle.to_bam().wrapping_sub(2 << 26));
                bullet_slope = self.aim_line_attack(distance, bsp_trace);
            }
        }
        self.angle = old_angle;

        bullet_slope
    }

    /// Fire a single pistol/chaingun bullet (`P_GunShot`).
    ///
    /// Applies random spread when `accurate` is false (i.e. not first shot).
    pub(crate) fn gun_shot(
        &mut self,
        accurate: bool,
        distance: FixedT,
        bullet_slope: Option<AimResult>,
        bsp_trace: &mut BSPTrace,
    ) {
        let damage = 5 * (p_random() % 3 + 1);
        let mut angle = self.angle;

        if !accurate {
            // OG: angle += (P_Random()-P_Random())<<18
            let spread = ((p_random() - p_random()) << 18) as u32;
            angle = Angle::from_bam(angle.to_bam().wrapping_add(spread));
        }

        if let Some(res) = bullet_slope {
            self.shoot_line_attack(distance, angle, res.aimslope, damage, bsp_trace);
        } else {
            self.shoot_line_attack(distance, angle, FixedT::ZERO, damage, bsp_trace);
        }
    }

    /// Fire a line attack along the given angle using the previous `AimResult`
    /// and `BSPTrace`.
    pub(crate) fn line_attack(
        &mut self,
        damage: i32,
        distance: FixedT,
        angle: Angle<Bam>,
        bullet_slope: Option<AimResult>,
        bsp_trace: &mut BSPTrace,
    ) {
        if let Some(res) = bullet_slope {
            self.shoot_line_attack(distance, angle, res.aimslope, damage, bsp_trace);
        } else {
            self.shoot_line_attack(distance, angle, FixedT::ZERO, damage, bsp_trace);
        }
    }

    /// Check if there is a clear line of sight to the selected point.
    /// Matches OG `P_CheckSight` — point-to-point trace with no radius offset.
    pub(crate) fn check_sight(
        &mut self,
        to_x: FixedT,
        to_y: FixedT,
        to_z: FixedT,
        to_height: FixedT,
    ) -> bool {
        let z_start = self.z + self.height - self.height.shr(2);

        let level = unsafe { &mut *self.level };
        // OG: BSP-based sight check (P_CheckSight / P_CrossBSPNode)
        let strace = DivLineFixed {
            x: self.x,
            y: self.y,
            dx: to_x - self.x,
            dy: to_y - self.y,
        };
        let t2x = to_x;
        let t2y = to_y;
        let sightzstart = z_start;
        // OG: topslope = t2->z + t2->height - sightzstart
        let mut topslope = to_z + to_height - sightzstart;
        let mut bottomslope = to_z - sightzstart;

        level.valid_count = level.valid_count.wrapping_add(1);
        let valid = level.valid_count;

        cross_bsp_node(
            level.level_data.start_node(),
            &strace,
            t2x,
            t2y,
            sightzstart,
            &mut topslope,
            &mut bottomslope,
            &level.level_data,
            valid,
        )
    }

    /// Iterate through the available live players and check if there is a LOS
    /// to one.
    pub(crate) fn look_for_players(&mut self, all_around: bool) -> bool {
        let mut see = 0;
        let stop = ((self.lastlook as i32 - 1) & 3) as usize;

        self.lastlook = stop;
        for _ in 0..self.lastlook {
            self.lastlook = (self.lastlook - 1) & 3;
            if !self.level().players_in_game()[self.lastlook] {
                continue;
            }
            see += 1;
            if see == 2 || self.lastlook == stop {
                return false;
            }

            if self.level().players()[self.lastlook].status.health <= 0 {
                continue;
            }

            if let Some(target) = self.level().players()[self.lastlook].mobj() {
                let tx = target.x;
                let ty = target.y;
                let tz = target.z;
                let th = target.height;

                if !self.check_sight(tx, ty, tz, th) {
                    continue;
                }

                if !all_around {
                    let an = r_point_to_angle(tx - self.x, ty - self.y)
                        .wrapping_sub(self.angle.to_bam());
                    if an > ANG90 && an < ANG270 {
                        let dist = p_aprox_distance(tx - self.x, ty - self.y);
                        if dist > MELEERANGE {
                            continue;
                        }
                    }
                }

                let last_look = self.lastlook as usize;
                self.target = self.level_mut().players_mut()[last_look]
                    .mobj_mut()
                    .map(|m| m.thinker);
                return true;
            }
        }
        false
    }

    /// Check if there is a clear line of sight to the selected target object.
    /// This checks the '2D top-down' nature of Doom, followed by the Z
    /// (height) axis.
    pub(crate) fn check_sight_target(&mut self, target: &MapObject) -> bool {
        #[cfg(feature = "hprof")]
        profile!("check_sight_target");

        let s1 = self.subsector.sector.num;
        let s2 = target.subsector.sector.num;
        let sector_count = self.level().level_data.sectors().len() as i32;
        let pnum = s1 * sector_count + s2;
        let bytenum = pnum >> 3;
        let bitnum = 1 << (pnum & 7);

        if !self.level().level_data.get_devils_rejects().is_empty() {
            if self.level().level_data.get_devils_rejects()[bytenum as usize] & bitnum != 0 {
                return false;
            }
        }

        self.check_sight(target.x, target.y, target.z, target.height)
    }

    /// Return true if the current target is within melee range and visible.
    pub(crate) fn check_melee_range(&mut self) -> bool {
        #[cfg(feature = "hprof")]
        profile!("check_melee_range");
        if let Some(target) = self.target {
            let target = unsafe { (*target).mobj() };

            let dist = p_aprox_distance(target.x - self.x, target.y - self.y);
            if dist >= target.radius + (MELEERANGE - 20) {
                return false;
            }

            return self.check_sight_target(target);
        }
        false
    }

    /// Returns true if `self` should fire a missile this tic. Bypasses if
    /// `Justhit` is set; gated by `reactiontime`; probability increases as
    /// distance decreases. OG: P_CheckMissileRange.
    pub(crate) fn check_missile_range(&mut self) -> bool {
        #[cfg(feature = "hprof")]
        profile!("check_missile_range");
        if let Some(target) = self.target {
            let target = unsafe { (*target).mobj() };

            if !self.check_sight_target(target) {
                return false;
            }

            // Was just attacked, fight back!
            if self.flags.contains(MapObjFlag::Justhit) {
                self.flags.remove(MapObjFlag::Justhit);
                return true;
            }

            if self.reactiontime != 0 {
                return false; // do not attack yet
            }

            let raw_dist = p_aprox_distance(self.x - target.x, self.y - target.y);
            let mut dist = raw_dist - 64;

            if self.info.meleestate == StateNum::None {
                dist = dist - 128; // no melee attack, so fire more
            }

            // OG: dist >>= 16 to get integer units
            let mut dist_i = dist.to_i32();

            if self.kind == MapObjKind::MT_VILE && dist_i > 14 * 64 {
                return false; // too far away
            }

            if self.kind == MapObjKind::MT_UNDEAD {
                if dist_i < 196 {
                    return false; // Close in to punch
                }
                dist_i >>= 1;
            }

            if matches!(
                self.kind,
                MapObjKind::MT_CYBORG | MapObjKind::MT_SPIDER | MapObjKind::MT_SKULL
            ) {
                dist_i >>= 1;
            }

            if dist_i > 200 {
                dist_i = 200;
            }

            if self.kind == MapObjKind::MT_CYBORG && dist_i > 160 {
                dist_i = 160;
            }

            // All down to chance now
            if p_random() >= dist_i {
                return true;
            }
        }
        false
    }
}

#[derive(Clone)]
pub(crate) struct AimResult {
    pub aimslope: FixedT,
    pub line_target: MapPtr<MapObject>,
}

struct SubSectTraverse {
    top_slope: FixedT,
    bot_slope: FixedT,
    attack_range: FixedT,
    shootz: FixedT,
    result: Option<AimResult>,
}

impl SubSectTraverse {
    fn new(top_slope: FixedT, bot_slope: FixedT, attack_range: FixedT, shootz: FixedT) -> Self {
        Self {
            top_slope,
            bot_slope,
            attack_range,
            shootz,
            result: None,
        }
    }

    /// Narrow the vertical slope window based on a two-sided line's portal
    /// opening.
    fn set_slope(&mut self, line: &LineDef, portal: &PortalZ, dist: FixedT) {
        if let Some(backsector) = line.backsector.as_ref() {
            if line.frontsector.floorheight != backsector.floorheight {
                let slope = (portal.bottom_z - self.shootz) / dist;
                if slope > self.bot_slope {
                    self.bot_slope = slope;
                }
            }

            if line.frontsector.ceilingheight != backsector.ceilingheight {
                let slope = (portal.top_z - self.shootz) / dist;
                if slope < self.top_slope {
                    self.top_slope = slope;
                }
            }
        }
    }

    /// Process a single aim intercept, updating slope bounds and recording
    /// hits.
    fn check_aim(&mut self, shooter: &mut MapObject, intercept: &mut Intercept) -> bool {
        if let Some(line) = intercept.line.as_mut() {
            if !line.flags.contains(LineDefFlags::TwoSided) {
                return false;
            }

            let portal = PortalZ::new(line);
            if portal.bottom_z >= portal.top_z {
                return false;
            }

            let dist = self.attack_range * intercept.frac;
            self.set_slope(line, &portal, dist);

            if self.top_slope <= self.bot_slope {
                return false;
            }

            return true;
        } else if let Some(thing) = intercept.thing.as_mut() {
            if shooter as *const _ as usize == thing.as_ref() as *const _ as usize {
                return true;
            }
            if !thing.flags.contains(MapObjFlag::Shootable) {
                return true;
            }

            let dist = self.attack_range * intercept.frac;
            let mut thing_top_slope = (thing.z + thing.height - self.shootz) / dist;
            if thing_top_slope < self.bot_slope {
                return true; // Shot over
            }

            let mut thing_bot_slope = (thing.z - self.shootz) / dist;
            if thing_bot_slope > self.top_slope {
                return true; // Shot below
            }

            if thing_top_slope > self.top_slope {
                thing_top_slope = self.top_slope;
            }
            if thing_bot_slope < self.bot_slope {
                thing_bot_slope = self.bot_slope;
            }

            self.result = Some(AimResult {
                aimslope: (thing_top_slope + thing_bot_slope) / 2,
                line_target: thing.clone(),
            });
        }

        false
    }

    fn result(&mut self) -> Option<AimResult> {
        self.result.take()
    }
}

struct ShootTraverse {
    aim_slope: FixedT,
    attack_range: FixedT,
    damage: i32,
    shootz: FixedT,
    trace_x: FixedT,
    trace_y: FixedT,
    trace_dx: FixedT,
    trace_dy: FixedT,
    sky_num: usize,
}

impl ShootTraverse {
    fn new(
        aim_slope: FixedT,
        attack_range: FixedT,
        damage: i32,
        shootz: FixedT,
        trace_x: FixedT,
        trace_y: FixedT,
        trace_dx: FixedT,
        trace_dy: FixedT,
        sky_num: usize,
    ) -> Self {
        Self {
            aim_slope,
            attack_range,
            damage,
            shootz,
            trace_x,
            trace_y,
            trace_dx,
            trace_dy,
            sky_num,
        }
    }

    /// Spawn a bullet puff at the wall hit point, respecting sky ceilings.
    fn hit_line(&self, shooter: &mut MapObject, frac: FixedT, line: &LineDef) {
        let frac_adj = frac - (4 / self.attack_range);
        let x = self.trace_x + self.trace_dx * frac_adj;
        let y = self.trace_y + self.trace_dy * frac_adj;
        // OG: z = shootz + FixedMul(aimslope, FixedMul(frac, attackrange))
        let z = self.shootz
            + self
                .aim_slope
                .fixed_mul(frac_adj.fixed_mul(self.attack_range));

        if line.frontsector.ceilingpic == self.sky_num {
            // OG: don't shoot the sky
            if z > FixedT::from_fixed(line.frontsector.ceilingheight.to_fixed_raw()) {
                return;
            }
            // OG: sky hack wall — backsector also has sky ceiling
            if let Some(back) = line.backsector.as_ref() {
                if back.ceilingpic == self.sky_num {
                    return;
                }
            }
        }

        MapObject::spawn_puff(x, y, z, self.attack_range, unsafe { &mut *shooter.level });
    }

    /// Process one intercept during a shoot traversal (`PTR_ShootTraverse`).
    ///
    /// - Lines: activates specials, checks portal slopes, spawns puffs on walls
    /// - Things: skips self and non-shootable, spawns blood/puff and applies
    ///   damage
    fn resolve(&mut self, shooter: &mut MapObject, intercept: &mut Intercept) -> bool {
        if let Some(line) = intercept.line.as_mut() {
            if line.special != 0 {
                shoot_special_line(line.clone(), shooter);
            }

            if !line.flags.contains(LineDefFlags::TwoSided) {
                self.hit_line(shooter, intercept.frac, line);
                return false;
            }

            let portal = PortalZ::new(line);
            let dist = self.attack_range * intercept.frac;

            if let Some(backsector) = line.backsector.as_ref() {
                if line.frontsector.floorheight != backsector.floorheight {
                    let slope = (portal.bottom_z - self.shootz) / dist;
                    if slope > self.aim_slope {
                        self.hit_line(shooter, intercept.frac, line);
                        return false;
                    }
                }

                if line.frontsector.ceilingheight != backsector.ceilingheight {
                    let slope = (portal.top_z - self.shootz) / dist;
                    if slope < self.aim_slope {
                        self.hit_line(shooter, intercept.frac, line);
                        return false;
                    }
                }
            }

            return true;
        } else if let Some(thing) = intercept.thing.as_mut() {
            if shooter as *const _ as usize == thing.as_ref() as *const _ as usize {
                return true;
            }
            if !thing.flags.contains(MapObjFlag::Shootable) {
                return true;
            }

            let dist = self.attack_range * intercept.frac;
            let thing_top_slope = (thing.z + thing.height - self.shootz) / dist;
            if thing_top_slope < self.aim_slope {
                return true; // Shot over
            }

            let thing_bot_slope = (thing.z - self.shootz) / dist;
            if thing_bot_slope > self.aim_slope {
                return true; // Shot below
            }

            // Spawn position
            let frac_adj = intercept.frac - (10 / self.attack_range);
            let x = self.trace_x + self.trace_dx * frac_adj;
            let y = self.trace_y + self.trace_dy * frac_adj;
            // OG: z = shootz + FixedMul(aimslope, FixedMul(frac, attackrange))
            let z = self.shootz
                + self
                    .aim_slope
                    .fixed_mul(frac_adj.fixed_mul(self.attack_range));

            if thing.flags.contains(MapObjFlag::Noblood) {
                MapObject::spawn_puff(x, y, z, self.attack_range, unsafe { &mut *thing.level })
            } else {
                MapObject::spawn_blood(x, y, z, self.damage, unsafe { &mut *thing.level });
            }

            if self.damage > 0 {
                let inflictor = Some((shooter.x, shooter.y, shooter.z));
                // OG: P_DamageMobj(th, shootthing, shootthing, la_damage)
                let source = unsafe { &mut *(shooter as *mut MapObject) };
                thing.p_take_damage(inflictor, Some(source), self.damage);
                return false;
            }
        }

        false
    }
}

/// OG `P_CrossBSPNode` — recursive BSP sight traversal.
fn cross_bsp_node(
    node_id: u32,
    strace: &DivLineFixed,
    t2x: FixedT,
    t2y: FixedT,
    sightzstart: FixedT,
    topslope: &mut FixedT,
    bottomslope: &mut FixedT,
    level_data: &LevelData,
    valid: usize,
) -> bool {
    if is_subsector(node_id) {
        let ss_idx = subsector_index(node_id);
        return cross_subsector(
            ss_idx,
            strace,
            t2x,
            t2y,
            sightzstart,
            topslope,
            bottomslope,
            level_data,
            valid,
        );
    }

    let node = &level_data.get_nodes()[node_id as usize];
    let nx = FixedT::from_f32(node.xy.x);
    let ny = FixedT::from_f32(node.xy.y);
    let ndx = FixedT::from_f32(node.delta.x);
    let ndy = FixedT::from_f32(node.delta.y);

    let mut side = p_divline_side_raw(strace.x, strace.y, nx, ny, ndx, ndy);
    if side == 2 {
        side = 0;
    }

    if !cross_bsp_node(
        node.children[side],
        strace,
        t2x,
        t2y,
        sightzstart,
        topslope,
        bottomslope,
        level_data,
        valid,
    ) {
        return false;
    }

    let side2 = p_divline_side_raw(t2x, t2y, nx, ny, ndx, ndy);
    if side2 == side || side2 == 2 {
        return true;
    }

    cross_bsp_node(
        node.children[side ^ 1],
        strace,
        t2x,
        t2y,
        sightzstart,
        topslope,
        bottomslope,
        level_data,
        valid,
    )
}

/// OG `P_CrossSubsector` — check all segs in a subsector for sight blocking.
fn cross_subsector(
    ss_idx: usize,
    strace: &DivLineFixed,
    t2x: FixedT,
    t2y: FixedT,
    sightzstart: FixedT,
    topslope: &mut FixedT,
    bottomslope: &mut FixedT,
    level_data: &LevelData,
    valid: usize,
) -> bool {
    let ss = &level_data.subsectors()[ss_idx];
    let segments = level_data.segments();

    for i in 0..ss.seg_count {
        let seg = &segments[(ss.start_seg + i) as usize];
        let mut line = seg.linedef.clone();

        if line.valid_count == valid {
            continue;
        }
        line.valid_count = valid;

        let v1x = FixedT::from_fixed(line.v1.x_fp.to_fixed_raw());
        let v1y = FixedT::from_fixed(line.v1.y_fp.to_fixed_raw());
        let v2x = FixedT::from_fixed(line.v2.x_fp.to_fixed_raw());
        let v2y = FixedT::from_fixed(line.v2.y_fp.to_fixed_raw());

        let s1 = p_divline_side_raw(v1x, v1y, strace.x, strace.y, strace.dx, strace.dy);
        let s2 = p_divline_side_raw(v2x, v2y, strace.x, strace.y, strace.dx, strace.dy);
        if s1 == s2 {
            continue;
        }

        let ldx = v2x - v1x;
        let ldy = v2y - v1y;
        let s1 = p_divline_side_raw(strace.x, strace.y, v1x, v1y, ldx, ldy);
        let s2 = p_divline_side_raw(t2x, t2y, v1x, v1y, ldx, ldy);
        if s1 == s2 {
            continue;
        }

        if !line.flags.contains(LineDefFlags::TwoSided) {
            return false;
        }

        let front = &seg.frontsector;
        let back = match seg.backsector.as_ref() {
            Some(b) => b,
            None => return false,
        };

        let front_floor = front.floorheight;
        let front_ceil = front.ceilingheight;
        let back_floor = back.floorheight;
        let back_ceil = back.ceilingheight;

        if front_floor == back_floor && front_ceil == back_ceil {
            continue;
        }

        let opentop = if front_ceil < back_ceil {
            front_ceil
        } else {
            back_ceil
        };
        let openbottom = if front_floor > back_floor {
            front_floor
        } else {
            back_floor
        };

        if openbottom >= opentop {
            return false;
        }

        let divl = DivLineFixed {
            x: v1x,
            y: v1y,
            dx: ldx,
            dy: ldy,
        };
        let frac = intercept_vector_fixed(strace, &divl);

        if front_floor != back_floor {
            let slope = (openbottom - sightzstart).fixed_div(frac);
            if slope > *bottomslope {
                *bottomslope = slope;
            }
        }

        if front_ceil != back_ceil {
            let slope = (opentop - sightzstart).fixed_div(frac);
            if slope < *topslope {
                *topslope = slope;
            }
        }

        if *topslope <= *bottomslope {
            return false;
        }
    }

    true
}
