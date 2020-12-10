//!	Movement, collision handling.
//!	Shooting and aiming.
use glam::Vec2;

use crate::level_data::level::Level;
use crate::p_local::MAXRADIUS;
use crate::p_map_object::{MapObject, MapObjectFlag};

const MAXSPECIALCROSS: i32 = 8;
const BOXTOP: usize = 0;
const BOXBOTTOM: usize = 1;
const BOXRIGHT: usize = 3;
const BOXLEFT: usize = 2;

#[derive(Default)]
pub(crate) struct MobjCtrl {
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

impl MapObject {
    /// P_TryMove
    pub fn p_try_move(
        &mut self,
        level: &mut Level,
        ptryx: f32,
        ptryy: f32,
    ) -> bool {
        // P_UnsetThingPosition // level function, sets subsector pointer and blockmap pointer
        // P_SetThingPosition // level function
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

        // TODO: if any special lines were hit, do the effect
        // if (!(thing->flags & (MF_TELEPORT | MF_NOCLIP)))
        // {
        //     while (numspechit--)
        //     {
        //         // see if the line was crossed
        //         ld = spechit[numspechit];
        //         side = P_PointOnLineSide(thing->x, thing->y, ld);
        //         oldside = P_PointOnLineSide(oldx, oldy, ld);
        //         if (side != oldside)
        //         {
        //             if (ld->special)
        //                 P_CrossSpecialLine(ld - lines, oldside, thing);
        //         }
        //     }
        // }

        true
    }

    /// P_CheckPosition
    /// This is purely informative, nothing is modified
    /// (except things picked up).
    ///
    /// in:
    ///  a mobj_t (can be valid or invalid)
    ///  a position to be checked
    ///   (doesn't need to be related to the mobj_t->x,y)
    ///
    /// during:
    ///  special things are touched if MF_PICKUP
    ///  early out on solid lines?
    ///
    /// out:
    ///  newsubsec
    ///  floorz
    ///  ceilingz
    ///  tmdropoffz
    ///   the lowest point contacted
    ///   (monsters won't move to a dropoff)
    ///  speciallines[]
    ///  numspeciallines
    fn p_check_position(&mut self, level: &mut Level, xy: &Vec2) -> bool {
        let ctrl = &mut level.mobj_ctrl;
        ctrl.tmbbox[BOXTOP] = xy.y() + self.radius;
        ctrl.tmbbox[BOXBOTTOM] = xy.y() - self.radius;
        ctrl.tmbbox[BOXRIGHT] = xy.x() + self.radius;
        ctrl.tmbbox[BOXLEFT] = xy.x() - self.radius;

        // TODO: ceilingline = NULL;

        let newsubsect = level.map_data.point_in_subsector(xy);
        ctrl.tmfloorz = newsubsect.sector.floorheight;
        ctrl.tmceilingz = newsubsect.sector.ceilingheight;

        // TODO: validcount++;??? There's like, two places in the p_map.c file
        ctrl.numspechit = 0;
        if ctrl.tmflags & MapObjectFlag::MF_NOCLIP as u32 != 0 {
            return true;
        }

        // Check things first, possibly picking things up.
        // The bounding box is extended by MAXRADIUS
        // because mobj_ts are grouped into mapblocks
        // based on their origin point, and can overlap
        // into adjacent blocks by up to MAXRADIUS units.

        // TODO: P_BlockThingsIterator, PIT_CheckThing
        // TODO: P_BlockLinesIterator, PIT_CheckLine

        level.mobj_ctrl.tmfloorz = newsubsect.sector.floorheight;
        level.mobj_ctrl.tmceilingz = newsubsect.sector.ceilingheight;

        true
    }

    /// P_SlideMove, // level function
    // TODO: P_SlideMove
    pub fn p_slide_move(&mut self) {
        //unimplemented!();
    }
}

/// P_RadiusAttack
/// Source is the creature that caused the explosion at spot.
pub(crate) fn p_radius_attack(
    spot: &mut MapObject,
    source: &mut MapObject,
    damage: f32,
) {
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
