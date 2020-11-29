///	Movement, collision handling.
///	Shooting and aiming.
use glam::Vec2;

use crate::p_map_object::MapObject;
use crate::{level::Level, p_local::MAXRADIUS};

const MAXSPECIALCROSS: i32 = 8;

#[derive(Default)]
pub struct MobjCtrl {
    tmbbox:     [f32; 4],
    tmflags:    u32,
    tmx:        f32,
    tmy:        f32,
    /// If "floatok" true, move would be ok
    /// if within "tmfloorz - tmceilingz".
    floatok:    bool,
    tmfloorz:   f32,
    tmceilingz: f32,
    tmdropoffz: f32,
    numspechit: i32,
}

// TODO: these funcitons need to live in Level. Conflicting borrows are happening. We can keep the MobjCtrl struct

impl MapObject {
    /// P_TryMove // map function
    // TODO: P_TryMove
    pub fn p_try_move(
        &mut self,
        level: &mut Level,
        ptryx: f32,
        ptryy: f32,
    ) -> bool {
        // P_CheckPosition // map function
        // P_UnsetThingPosition // map function
        // P_SetThingPosition // map function
        // P_CrossSpecialLine
        //unimplemented!();
        level.mobj_ctrl.floatok = false;
        if !self.p_check_position(level, &Vec2::new(ptryx, ptryy)) {
            return false; // solid wall or thing
        }
        
        self.floorz = level.mobj_ctrl.tmfloorz;
        self.ceilingz = level.mobj_ctrl.tmceilingz;

        self.xy.set_x(ptryx);
        self.xy.set_y(ptryy);

        true
    }

    fn p_check_position(&mut self, level: &mut Level, xy: &Vec2) -> bool {
        // TODO: R_PointInSubsector
        if let Some(newsubsect) = level.map_data.point_in_subsector(xy) {
            level.mobj_ctrl.tmfloorz = newsubsect.sector.floor_height as f32;
            level.mobj_ctrl.tmceilingz = newsubsect.sector.ceil_height as f32;
        }
        true
    }

    /// P_SlideMove, // map function
    // TODO: P_SlideMove
    pub fn p_slide_move(&mut self) {
        //unimplemented!();
    }
}

/// P_RadiusAttack
/// Source is the creature that caused the explosion at spot.
pub fn p_radius_attack(
    spot: &mut MapObject,
    source: &mut MapObject,
    damage: f32,
) {
    let dist = damage + MAXRADIUS;
    unimplemented!()
    // // origin of block map is bmaporgx and bmaporgy
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
