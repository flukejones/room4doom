use glam::Vec2;

use crate::{
    info::{SfxEnum, MOBJINFO},
    level::map_data::BSPTrace,
    play::utilities::{p_random, path_traverse, Intercept},
    DPtr, LineDefFlags, MapObject,
};

use super::{MapObjectFlag, PT_ADDLINES, PT_ADDTHINGS};

impl MapObject {
    /// P_ExplodeMissile
    fn p_explode_missile(&mut self) {
        self.momxy = Vec2::default();
        self.z = 0.0;
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

    pub(crate) fn aim_line_attack(&mut self, distance: f32) -> Option<AimResult> {
        let xy2 = self.xy + self.angle.unit() * distance;

        // These a globals in Doom, used in the traverse functions
        let shootz = self.z + (self.height as i32 >> 1) as f32 + 8.0;
        // can't shoot outside view angles
        let top_slope = 100.0 / 160.0;
        let bot_slope = -100.0 / 160.0;
        let attack_range = distance;
        // Path traverse neds to set line_target
        // let line_target = null;

        let mut bsp_trace = BSPTrace::new(self.xy, xy2, 10.0);
        let mut count = 0;
        let level = unsafe { &mut *self.level };
        bsp_trace.find_ssect_intercepts(level.map_data.start_node(), &level.map_data, &mut count);
        //bsp_trace.nodes = level.map_data.get_nodes().iter().enumerate().map(|(i,_)| i as u16).collect();

        // set up traverser
        let mut aim_traverse = AimTraverse::new(top_slope, bot_slope, attack_range, shootz);

        path_traverse(
            self.xy,
            xy2,
            PT_ADDLINES | PT_ADDTHINGS,
            level,
            |t| aim_traverse.check(self, t),
            &mut bsp_trace,
        );

        aim_traverse.result()
    }
}

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
    fn check(&mut self, shooter: &MapObject, intercept: &mut Intercept) -> bool {
        if let Some(line) = intercept.line.as_mut() {
            // Check if solid line and stop
            if line.flags & LineDefFlags::TwoSided as u32 == 0 {
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
