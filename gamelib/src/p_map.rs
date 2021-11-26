//!	Movement, collision handling.
//!	Shooting and aiming.
use glam::Vec2;

use crate::flags::LineDefFlags;
use crate::level_data::level::Level;
use crate::level_data::map_defs::{BBox, LineDef};
use crate::p_local::MAXRADIUS;
use crate::p_map_object::{MapObject, MapObjectFlag};
use crate::p_map_util::{box_on_line_side, line_slide_direction, PortalZ};
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

        if !self.p_check_position(&try_move, level) {
            // up to callee to do something like slide check
            return false;
        }

        let ctrl = &mut level.mobj_ctrl;
        ctrl.spec_hits.clear();

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
    fn p_check_position(&mut self, try_move: &Vec2, level: &mut Level) -> bool {
        let left = try_move.x() - self.radius;
        let right = try_move.x() + self.radius;
        let top = try_move.y() + self.radius;
        let bottom = try_move.y() - self.radius;
        let tmbbox = BBox {
            top,
            bottom,
            left,
            right,
        };

        let ctrl = &mut level.mobj_ctrl;
        let curr_ssect = level.map_data.point_in_subsector(try_move);
        // The base floor / ceiling is from the subsector
        // that contains the point.
        // Any contacted lines the step closer together
        // will adjust them.
        ctrl.min_floor_z = curr_ssect.sector.floorheight;
        ctrl.max_dropoff = curr_ssect.sector.floorheight;
        ctrl.max_ceil_z = curr_ssect.sector.ceilingheight;

        if self.flags & MapObjectFlag::MF_NOCLIP as u32 != 0 {
            return true;
        }

        // TODO: use the blockmap for checking lines
        // TODO: use a P_BlockThingsIterator
        // TODO: use a P_BlockLinesIterator - used to build a list of lines to check
        //       it also calls PIT_CheckLine on each line
        //       P_BlockLinesIterator is called mobj->radius^2
        for line in level.map_data.get_linedefs() {
            if !self.pit_check_line(&tmbbox, ctrl, line) {
                return false;
            }
        }
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
        if tmbbox.right <= ld.bbox.left
            || tmbbox.left >= ld.bbox.right
            || tmbbox.top <= ld.bbox.bottom
            || tmbbox.bottom >= ld.bbox.top
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

        if box_on_line_side(&tmbbox, ld) != -1 {
            return true;
        }

        if ld.backsector.is_none() {
            // one-sided line
            return false;
        }

        if self.flags & MapObjectFlag::MF_MISSILE as u32 != 0 {
            if ld.flags & LineDefFlags::Blocking as i16 == 0 {
                return false; // explicitly blocking everything
            }

            if self.player.is_none() && ld.flags & LineDefFlags::BlockMonsters as i16 != 0 {
                return false; // block monsters only
            }
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
        true
    }

    // P_SlideMove
    // Loop until get a good move or stopped
    pub fn p_slide_move(&mut self, level: &mut Level) {
        // let ctrl = &mut level.mobj_ctrl;

        let mut hitcount = 0;
        let mut new_momxy;
        let mut try_move;

        // The p_try_move calls check collisions -> p_check_position -> pit_check_line
        loop {
            if hitcount == 3 {
                // try_move = self.xy + self.momxy;
                // self.p_try_move(try_move.x(), try_move.y(), level);
                break;
            }
            new_momxy = self.momxy;
            try_move = self.xy;

            let ssect = level.map_data.point_in_subsector(&(self.xy));
            // let segs = &level.map_data.get_segments()[ssect.start_seg as usize..(ssect.start_seg+ssect.seg_count) as usize];
            // TODO: Use the blockmap, find closest best line
            for ld in ssect.sector.lines.iter() {
                if try_move.x() + self.radius >= ld.bbox.left
                    || try_move.x() - self.radius <= ld.bbox.right
                    || try_move.y() + self.radius >= ld.bbox.bottom
                    || try_move.y() - self.radius <= ld.bbox.top
                {
                    //if ld.point_on_side(&self.xy) == 0 {
                    // TODO: Check lines in radius around mobj, find the best/closest line to use for slide
                    if let Some(m) =
                        line_slide_direction(self.xy, new_momxy, self.radius, *ld.v1, *ld.v2)
                    {
                        new_momxy = m;
                        break;
                    }
                    //}
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
