///	Movement, collision handling.
///	Shooting and aiming.
use crate::p_local::MAXRADIUS;
use crate::p_map_object::MapObject;

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

impl MobjCtrl {
     /// P_TryMove // map function
    // TODO: P_TryMove
    pub fn p_try_move(&mut self, ptryx: f32, ptryy: f32) -> bool {
        // P_CheckPosition // map function
        // P_UnsetThingPosition // map function
        // P_SetThingPosition // map function
        // P_CrossSpecialLine
        //unimplemented!();
        false
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
