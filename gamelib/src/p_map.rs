//!	Movement, collision handling.
//!	Shooting and aiming.
use glam::Vec2;

use crate::flags::LineDefFlags;
use crate::level_data::level::Level;
use crate::level_data::map_defs::{LineDef};
use crate::p_local::MAXRADIUS;
use crate::p_map_object::{MapObject, MapObjectFlag, MAXMOVE};
use crate::p_map_util::{PortalZ, circle_to_line_intercept, slide_line_intercept};
use crate::DPtr;

const MAXSPECIALCROSS: i32 = 8;

/// The pupose of this struct is to record the highest and lowest points in a
/// subsector. When a mob crosses a seg it may be between floor/ceiling heights.
#[derive(Default)]
pub(crate) struct SubSectorMinMax {
    tmflags:     u32,
    /// If "floatok" true, move would be ok
    /// if within "tmfloorz - tmceilingz".
    floatok:     bool,
    min_floor_z: f32,
    max_ceil_z:  f32,
    max_dropoff: f32,
    spec_hits:   Vec<DPtr<LineDef>>,
}

impl MapObject {
    /// P_TryMove, merged with P_CheckPosition and using a more verbose/modern collision
    pub fn p_try_move(&mut self, ptryx: f32, ptryy: f32, level: &mut Level) -> bool {
        // P_CrossSpecialLine
        level.mobj_ctrl.floatok = false;

        let try_move = Vec2::new(ptryx, ptryy);

        if !self.p_check_position(&try_move, level) {
            // up to callee to do something like slide check
            return false;
        }

        // TODO: ceilingline = NULL;
        // First sector is always the one we are in
        let ctrl = &mut level.mobj_ctrl;
        let curr_ssect = level.map_data.point_in_subsector(&try_move);

        ctrl.min_floor_z = curr_ssect.sector.floorheight;
        ctrl.max_dropoff = curr_ssect.sector.floorheight;
        ctrl.max_ceil_z = curr_ssect.sector.ceilingheight;

        // TODO: validcount++;??? There's like, two places in the p_map.c file
        // TODO: P_BlockThingsIterator, PIT_CheckThing/Line

        // the move is ok,
        // so link the thing into its new position
        // P_UnsetThingPosition (thing);

        let old_xy = self.xy;

        if ctrl.min_floor_z - self.z <= 24.0 || ctrl.min_floor_z <= self.z {
            self.floorz = ctrl.min_floor_z;
            self.ceilingz = ctrl.max_ceil_z;
        }
        self.xy = try_move;

        // P_SetThingPosition (thing);

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

    // P_SlideMove
    // Loop until get a good move or stopped
    pub fn p_slide_move(&mut self, level: &mut Level) {
        // let ctrl = &mut level.mobj_ctrl;

        let mut hitcount = 0;
        let mut new_momxy;
        let mut try_move;

        loop {
            if hitcount == 3 {
                try_move = self.xy + self.momxy;
                self.p_try_move(try_move.x(), try_move.y(), level);
                break;
            }
            new_momxy = self.momxy;
            try_move = self.xy;

            let curr_ssect = level.map_data.point_in_subsector(&(self.xy));

            for ld in &curr_ssect.sector.lines {
                if let Some(contact) = slide_line_intercept(
                    self.xy + new_momxy,
                    new_momxy,
                    self.radius + 5.0,
                    *ld.v1,
                    *ld.v2,
                ) {
                    //try_move -= contact.penetration * contact.normal;
                    let new_len = contact.angle_delta * new_momxy.length();
                    new_momxy = contact.slide_dir * new_len;
                    break;
                }
            }

            // TODO: move up to the wall / stairstep

            try_move += new_momxy;
            self.momxy = new_momxy;

            if self.p_try_move(try_move.x(), try_move.y(), level) {
                break;
            }

            hitcount += 1;
        }
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
    fn p_check_position(
        &mut self, try_move: &Vec2, level: &mut Level
    ) -> bool {
        let ctrl = &mut level.mobj_ctrl;
        let curr_ssect = level.map_data.point_in_subsector(try_move);

        for line in &curr_ssect.sector.lines {
            if !self.pit_check_line(try_move, ctrl, line) {
                return false;
            }
        }
        true
    }

    /// PIT_CheckLine
    /// Adjusts tmfloorz and tmceilingz as lines are contacted
    fn pit_check_line(
        &mut self,
        try_move: &Vec2,
        ctrl: &mut SubSectorMinMax,
        ld: &LineDef,
    ) -> bool {
        // TODO: Line bounding box check here
        if try_move.x() + self.radius <= ld.bbox.left
            || try_move.x() - self.radius >= ld.bbox.right
            || try_move.y() + self.radius <= ld.bbox.bottom
            || try_move.y() - self.radius >= ld.bbox.top {
                return true;
        }

        if circle_to_line_intercept(
            *try_move,
            self.radius,
            *ld.v1,
            *ld.v2,
        ) {
            // Moved from before circle_to_seg_intersect call
            if ld.point_on_side(&self.xy) != 0 {
                return true;
            }
            // Moved from before circle_to_seg_intersect call
            if self.flags & MapObjectFlag::MF_MISSILE as u32 != 0 {
                if ld.flags & LineDefFlags::Blocking as i16 == 0 {
                    return false; // explicitly blocking everything
                }

                if self.player.is_none()
                    && ld.flags & LineDefFlags::BlockMonsters as i16 != 0
                {
                    return false; // block monsters only
                }
            }

            if ld.backsector.is_none() {
                // one-sided line
                return false;
            }

            // Find the smallest/largest etc if group of line hits
            let portal = PortalZ::new(ld);
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
                ctrl.spec_hits.push(DPtr::new(ld));
            }

            // Next two ifs imported from?
            // These are the very specific portal collisions
            if self.flags & MapObjectFlag::MF_TELEPORT as u32 != 0
                && portal.top_z - self.z < self.height
            {
                return false;
            }

            // Line is higher
            if portal.bottom_z - self.z > 24.0 {
                return false;
            }

            // if self.flags
            //     & (MapObjectFlag::MF_DROPOFF as u32
            //         | MapObjectFlag::MF_FLOAT as u32)
            //     != 0
            //     && portal.bottom_z - portal.lowest_z > 24.0
            // {
            //     contacts.push(contact);
            //     return;
            // }
        }
        true
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
