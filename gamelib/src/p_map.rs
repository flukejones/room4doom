//!	Movement, collision handling.
//!	Shooting and aiming.
use glam::Vec2;

use crate::flags::LineDefFlags;
use crate::level_data::level::Level;
use crate::level_data::map_defs::{BBox, LineDef};
use crate::p_local::MAXRADIUS;
use crate::p_map_object::{MapObject, MapObjectFlag, FRICTION, MAXMOVE};
use crate::p_map_util::{
    circle_to_seg_intersect, unit_vec_from, LineContact, PortalZ,
};
use crate::DPtr;
use std::f32::consts::FRAC_PI_2;
use std::f32::EPSILON;

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
    pub fn p_try_move(&mut self, level: &mut Level) {
        // P_CrossSpecialLine
        level.mobj_ctrl.floatok = false;

        let ctrl = &mut level.mobj_ctrl;
        // TODO: ceilingline = NULL;

        let subs = level.map_data.point_in_subsector(&(self.xy + self.momxy));
        ctrl.min_floor_z = subs.sector.floorheight;
        ctrl.max_dropoff = subs.sector.floorheight;
        ctrl.max_ceil_z = subs.sector.ceilingheight;

        // TODO: validcount++;??? There's like, two places in the p_map.c file
        if ctrl.tmflags & MapObjectFlag::MF_NOCLIP as u32 != 0 {
            return;
        }

        // Check things first, possibly picking things up.
        // TODO: P_BlockThingsIterator, PIT_CheckThing

        // TODO: testing functions
        // This is effectively P_BlockLinesIterator, PIT_CheckLine
        let segs = level.map_data.get_segments();

        let mut contacts: Vec<LineContact> = Vec::new();
        let mut blocked = false;
        for seg in &segs[subs.start_seg as usize
            ..(subs.start_seg + subs.seg_count) as usize]
        {
            if let Some(contact) = self.pit_check_line(ctrl, &seg.linedef) {
                contacts.push(contact);
            }
        }

        for contact in contacts.iter_mut() {
            self.momxy -= contact.normal * (contact.penetration);
        }

        let mut ang_less = FRAC_PI_2;
        let mut ang_greater = 0.0;
        let mut slide = Vec2::default();
        for contact in contacts.iter_mut() {
            if contact.half_angle >= ang_greater {
                ang_greater = contact.half_angle;
            }
            if contact.half_angle <= FRAC_PI_2 {
                ang_less = contact.half_angle;
            }

            slide += contact.slide_dir;
            // self.momxy =
            //     contact.slide_dir * (self.momxy.length() * contact.half_angle);
        }

        if !contacts.is_empty() {
            blocked = true;
            if ang_less >= ang_greater {
                self.momxy = slide * (self.momxy.length() * ang_less);
            } else {
                self.momxy =
                    slide * (self.momxy.length() * ang_greater - ang_less);
            }

            contacts.clear();
            for seg in &segs[subs.start_seg as usize
                ..(subs.start_seg + subs.seg_count) as usize]
            {
                if let Some(contact) = self.pit_check_line(ctrl, &seg.linedef) {
                    contacts.push(contact);
                }
            }
            for contact in contacts.iter_mut() {
                self.momxy -= contact.normal * (contact.penetration);
            }
        }

        self.xy += self.momxy;
        if !blocked {
            self.floorz = level.mobj_ctrl.min_floor_z;
            self.ceilingz = level.mobj_ctrl.max_ceil_z;
        }

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
    }

    /// PIT_CheckLine
    /// Adjusts tmfloorz and tmceilingz as lines are contacted, if
    /// penetration with a line is detected then the pen distance is returned
    fn pit_check_line(
        &mut self,
        ctrl: &mut SubSectorMinMax,
        ld: &LineDef,
    ) -> Option<LineContact> {
        if let Some(contact) = circle_to_seg_intersect(
            self.xy,
            self.momxy,
            self.radius,
            *ld.v1,
            *ld.v2,
        ) {
            if ld.backsector.is_none() {
                // one-sided line
                return Some(contact);
            }

            // Flag checks
            // TODO: can we move these up a call?
            if self.flags & MapObjectFlag::MF_MISSILE as u32 == 0 {
                if ld.flags & LineDefFlags::Blocking as i16 != 0 {
                    return Some(contact); // explicitly blocking everything
                }

                if self.player.is_none()
                    && ld.flags & LineDefFlags::BlockMonsters as i16 != 0
                {
                    return Some(contact); // block monsters only
                }
            } else if self.flags & MapObjectFlag::MF_MISSILE as u32 != 0 {
                return Some(contact);
            }

            // Find the smallest/largest etc if group of line hits
            let portal = PortalZ::new(ld);
            if portal.top_z < ctrl.max_ceil_z {
                ctrl.max_ceil_z = portal.top_z;
                // TODO: ceilingline = ld;
            }
            if portal.bottom_z > ctrl.min_floor_z {
                ctrl.min_floor_z = portal.bottom_z;
            }
            if portal.low_point < ctrl.max_dropoff {
                ctrl.max_dropoff = portal.low_point;
            }

            if ld.special != 0 {
                ctrl.spec_hits.push(DPtr::new(ld));
            }

            if portal.bottom_z - self.z > 24.0 {
                return Some(contact);
            }
        }
        None
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
