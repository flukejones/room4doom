use glam::Vec2;

use crate::{
    info::{SfxEnum, MOBJINFO},
    level::{map_data::BSPTrace, map_defs::LineDef},
    play::{
        specials::shoot_special_line,
        utilities::{p_random, path_traverse, Intercept, PortalZ, MAXRADIUS},
    },
    Angle, DPtr, LineDefFlags, MapObject, MapObjectType,
};

use super::{MapObjectFlag, PT_ADDLINES, PT_ADDTHINGS};

impl MapObject {
    /// P_ExplodeMissile
    pub(super) fn p_explode_missile(&mut self) {
        self.momxy = Vec2::default();
        self.momz = 0.0;
        self.set_state(MOBJINFO[self.kind as usize].deathstate);

        self.tics -= p_random() & 3;

        if self.tics < 1 {
            self.tics = 1;
        }

        self.flags &= !(MapObjectFlag::Missile as u32);

        if self.info.deathsound != SfxEnum::None {
            // TODO: S_StartSound (mo, mo->info->deathsound);
        }
    }

    pub(crate) fn get_shoot_bsp_trace(&self, distance: f32) -> BSPTrace {
        let xy2 = self.xy + self.angle.unit() * distance;
        // Use a radius for shooting to enable a sort of swept volume to capture more subsectors as
        // demons might overlap from a subsector that isn't caught otherwise (for example demon
        // might be in one subsector but overlap with radius in to a subsector the bullet passes through).
        let mut bsp_trace = BSPTrace::new_line(self.xy, xy2, 20.0);
        let mut count = 0;
        let level = unsafe { &mut *self.level };
        bsp_trace.find_intercepts(level.map_data.start_node(), &level.map_data, &mut count);
        bsp_trace
    }

    pub(crate) fn aim_line_attack(
        &mut self,
        distance: f32,
        bsp_trace: &mut BSPTrace,
    ) -> Option<AimResult> {
        let xy2 = self.xy + self.angle.unit() * distance;

        // set up traverser
        let mut aim_traverse = AimTraverse::new(
            // can't shoot outside view angles
            100.0 / 160.0,
            -100.0 / 160.0,
            //
            distance,
            self.z + (self.height as i32 >> 1) as f32 - 8.0,
        );

        let level = unsafe { &mut *self.level };
        path_traverse(
            self.xy,
            xy2,
            PT_ADDLINES | PT_ADDTHINGS,
            level,
            |t| aim_traverse.check(self, t),
            bsp_trace,
        );

        aim_traverse.result()
    }

    /// `shoot_line_attack` is preceeded by `aim_line_attack` in many cases, so the `BSPTrace` can be
    /// shared between the two.
    pub(crate) fn shoot_line_attack(
        &mut self,
        attack_range: f32,
        angle: Angle,
        aim_slope: f32,
        damage: f32,
        bsp_trace: &mut BSPTrace,
    ) {
        let mut shoot_traverse = ShootTraverse::new(
            aim_slope,
            attack_range,
            damage,
            self.z + (self.height as i32 >> 1) as f32 + 8.0,
            bsp_trace.origin,
            angle.unit() * (bsp_trace.endpoint - bsp_trace.origin).length(),
        );

        let xy2 = Vec2::new(
            self.xy.x() + attack_range * angle.cos(),
            self.xy.y() + attack_range * angle.sin(),
        );

        let level = unsafe { &mut *self.level };
        path_traverse(
            self.xy,
            xy2,
            PT_ADDLINES | PT_ADDTHINGS,
            level,
            |intercept| shoot_traverse.resolve(self, intercept),
            bsp_trace,
        );
    }

    /// Source is the creature that caused the explosion at spot(self).
    ///
    /// Doom functrion name `P_RadiusAttack`
    pub fn radius_attack(&mut self, damage: f32) {
        // source is self.target
        // bsp_count is just for debugging BSP descent depth/width
        let mut bsp_count = 0;
        let dist = damage + MAXRADIUS;
        let mut bsp_trace = BSPTrace::new_radius(self.xy, dist);

        let level = unsafe { &mut *self.level };
        bsp_trace.find_intercepts(level.map_data.start_node(), &level.map_data, &mut bsp_count);

        let sub_sectors = &mut level.map_data.subsectors;
        level.valid_count = level.valid_count.wrapping_add(1);
        for n in bsp_trace.intercepted_subsectors() {
            let ssect = &mut sub_sectors[*n as usize];

            // Check things in subsectors
            if !ssect.sector.run_func_on_thinglist(|thing| {
                if thing.valid_count == level.valid_count {
                    return true;
                }
                thing.valid_count = level.valid_count;

                if thing.flags & MapObjectFlag::Shootable as u32 == 0 {
                    return true;
                }

                if matches!(
                    thing.kind,
                    MapObjectType::MT_CYBORG | MapObjectType::MT_SPIDER
                ) {
                    return true;
                }

                // Could just use vector lengths but it changes Doom behaviour...
                let dx = (thing.xy.x() - self.xy.x()).abs();
                let dy = (thing.xy.y() - self.xy.y()).abs();
                let mut dist = if dx > dy {
                    dx - thing.radius - self.radius
                } else {
                    dy - thing.radius - self.radius
                };

                if dist < 0.0 {
                    dist = 0.0;
                }

                if dist >= damage {
                    return true; // out of range of blowy
                }

                // TODO: P_CheckSight, use the existing BSPTrace.
                thing.p_take_damage(None, None, false, (damage - dist) as i32);
                true
            }) {
                return;
            }
        }
    }

    pub(crate) fn bullet_slope(
        &mut self,
        distance: f32,
        bsp_trace: &mut BSPTrace,
    ) -> Option<AimResult> {
        let mut bullet_slope = self.aim_line_attack(distance, bsp_trace);
        let old_angle = self.angle;
        if bullet_slope.is_none() {
            self.angle += 5.625f32.to_radians();
            bullet_slope = self.aim_line_attack(distance, bsp_trace);
            if bullet_slope.is_none() {
                self.angle -= 11.25f32.to_radians();
                bullet_slope = self.aim_line_attack(distance, bsp_trace);
            }
        }
        self.angle = old_angle;

        bullet_slope
    }

    pub(crate) fn gun_shot(
        &mut self,
        accurate: bool,
        distance: f32,
        bullet_slope: Option<AimResult>,
        bsp_trace: &mut BSPTrace,
    ) {
        let damage = 5.0 * (p_random() % 3 + 1) as f32;
        let mut angle = self.angle;

        if !accurate {
            angle += (((p_random() - p_random()) >> 5) as f32).to_radians();
        }

        if let Some(res) = bullet_slope {
            self.shoot_line_attack(distance, angle, res.aimslope, damage, bsp_trace);
        } else {
            self.shoot_line_attack(distance, angle, 0.0, damage, bsp_trace);
        }
    }

    pub(crate) fn line_attack(
        &mut self,
        damage: f32,
        distance: f32,
        angle: Angle,
        bullet_slope: Option<AimResult>,
        bsp_trace: &mut BSPTrace,
    ) {
        if let Some(res) = bullet_slope {
            self.shoot_line_attack(distance, angle, res.aimslope, damage, bsp_trace);
        } else {
            self.shoot_line_attack(distance, angle, 0.0, damage, bsp_trace);
        }
    }
}

#[derive(Clone)]
pub(crate) struct AimResult {
    pub aimslope: f32,
    pub line_target: DPtr<MapObject>,
}

struct AimTraverse {
    top_slope: f32,
    bot_slope: f32,
    attack_range: f32,
    shootz: f32,
    result: Option<AimResult>,
}

impl AimTraverse {
    fn new(top_slope: f32, bot_slope: f32, attack_range: f32, shootz: f32) -> Self {
        Self {
            top_slope,
            bot_slope,
            attack_range,
            shootz,
            result: None,
        }
    }

    /// After `check()` is called, a result should be checked for
    fn check(&mut self, shooter: &mut MapObject, intercept: &mut Intercept) -> bool {
        if let Some(line) = intercept.line.as_mut() {
            // Check if solid line and stop
            if line.flags & LineDefFlags::TwoSided as u32 == 0 {
                return false;
            }

            let portal = PortalZ::new(line);
            if portal.bottom_z >= portal.top_z {
                return false;
            }

            let dist = self.attack_range * intercept.frac;

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

            if self.top_slope <= self.bot_slope {
                return false;
            }

            return true;
        } else if let Some(thing) = intercept.thing.as_mut() {
            // Don't shoot self
            if std::ptr::eq(shooter, thing.as_ref()) {
                return true;
            }
            // Corpse?
            if thing.flags & MapObjectFlag::Shootable as u32 == 0 {
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
                aimslope: (thing_top_slope + thing_bot_slope) / 2.0,
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
    aim_slope: f32,
    attack_range: f32,
    damage: f32,
    shootz: f32,
    trace_xy: Vec2,
    trace_dxy: Vec2,
}

impl ShootTraverse {
    fn new(
        aim_slope: f32,
        attack_range: f32,
        damage: f32,
        shootz: f32,
        trace_xy: Vec2,
        trace_dxy: Vec2,
    ) -> Self {
        Self {
            aim_slope,
            attack_range,
            damage,
            shootz,
            trace_xy,
            trace_dxy,
        }
    }

    fn hit_line(&self, shooter: &mut MapObject, frac: f32, line: &LineDef) {
        let frac = frac - (4.0 / self.attack_range);
        let x = self.trace_xy.x() + self.trace_dxy.x() * frac;
        let y = self.trace_xy.y() + self.trace_dxy.y() * frac;
        let z = self.shootz + self.aim_slope * frac * self.attack_range;

        MapObject::spawn_puff(x, y, z as i32, self.attack_range, unsafe {
            &mut *shooter.level
        });
    }

    fn resolve(&mut self, shooter: &mut MapObject, intercept: &mut Intercept) -> bool {
        if let Some(line) = intercept.line.as_mut() {
            // TODO: temporary, move this line to shoot traverse
            if line.special != 0 {
                shoot_special_line(line.clone(), shooter);
            }

            // Check if solid line and stop
            if line.flags & LineDefFlags::TwoSided as u32 == 0 {
                self.hit_line(shooter, intercept.frac, &line);
                return false;
            }

            let portal = PortalZ::new(line);
            let dist = self.attack_range * intercept.frac;

            if let Some(backsector) = line.backsector.as_ref() {
                if line.frontsector.floorheight != backsector.floorheight {
                    let slope = (portal.bottom_z - self.shootz) / dist;
                    if slope > self.aim_slope {
                        self.hit_line(shooter, intercept.frac, &line);
                        return false;
                    }
                }

                if line.frontsector.ceilingheight != backsector.ceilingheight {
                    let slope = (portal.top_z - self.shootz) / dist;
                    if slope < self.aim_slope {
                        self.hit_line(shooter, intercept.frac, &line);
                        return false;
                    }
                }
            }

            return true;
        } else if let Some(thing) = intercept.thing.as_mut() {
            // Don't shoot self
            if std::ptr::eq(shooter, thing.as_ref()) {
                return true;
            }
            // Corpse?
            if thing.flags & MapObjectFlag::Shootable as u32 == 0 {
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

            let frac = intercept.frac - (10.0 / self.attack_range);
            let x = self.trace_xy.x() + self.trace_dxy.x() * frac;
            let y = self.trace_xy.y() + self.trace_dxy.y() * frac;
            let z = self.shootz + self.aim_slope * frac * self.attack_range;

            if thing.flags & MapObjectFlag::NoBlood as u32 != 0 {
                MapObject::spawn_puff(x, y, z as i32, self.attack_range, unsafe {
                    &mut *thing.level
                })
            } else {
                MapObject::spawn_blood(x, y, z as i32, self.damage, unsafe { &mut *thing.level });
            }

            if self.damage > 0.0 {
                thing.p_take_damage(None, Some(shooter), false, self.damage as i32);
                return false;
            }
        }

        false
    }
}
