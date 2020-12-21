//!	Movement, collision handling.
//!	Shooting and aiming.
use glam::Vec2;

use crate::flags::LineDefFlags;
use crate::level_data::level::Level;
use crate::level_data::map_data::MapData;
use crate::level_data::map_defs::{BBox, LineDef, SubSector};
use crate::p_local::MAXRADIUS;
use crate::p_map_object::{MapObject, MapObjectFlag, MAXMOVE};
use crate::p_map_util::{
    circle_to_seg_intersect, unit_vec_from, LineContact, PortalZ,
};
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
    // TODO: Okay so, first, broadphase get all segs in radius+momentum length,
    //  then get first collision only for wall-slide. Need to manage portal collisions better
    //  Alternative:
    //  - find subsector we're in
    //  - check each line, if contact portal then get back sector if front checked
    //  - record each checked line to compare if added
    fn get_contacting_ssects(
        &self,
        map_data: &MapData,
    ) -> Vec<DPtr<SubSector>> {
        let mut subsects =
            vec![map_data.point_in_subsector(&(self.xy + self.momxy))];
        let mov = self.xy + self.momxy;
        let r = self.radius;
        // TODO: need to check if subsector already added
        subsects.push(
            map_data.point_in_subsector(&Vec2::new(mov.x() + r, mov.y() + r)),
        );
        subsects.push(
            map_data.point_in_subsector(&Vec2::new(mov.x() + r, mov.y())),
        );
        subsects.push(
            map_data.point_in_subsector(&Vec2::new(mov.x() - r, mov.y() - r)),
        );
        subsects.push(
            map_data.point_in_subsector(&Vec2::new(mov.x(), mov.y() + r)),
        );
        subsects.push(
            map_data.point_in_subsector(&Vec2::new(mov.x() - r, mov.y() + r)),
        );
        subsects.push(
            map_data.point_in_subsector(&Vec2::new(mov.x() - r, mov.y())),
        );
        subsects.push(
            map_data.point_in_subsector(&Vec2::new(mov.x() + r, mov.y() - r)),
        );
        subsects.push(
            map_data.point_in_subsector(&Vec2::new(mov.x(), mov.y() - r)),
        );

        subsects
    }

    fn get_contacts_map(
        &mut self,
        ctrl: &mut SubSectorMinMax,
        map_data: &MapData,
    ) -> Vec<LineContact> {
        let mut points = Vec::new();
        let mut contacts: Vec<LineContact> = Vec::new();
        // TODO: figure out a better way to get all segs in vicinity;

        for line in map_data.get_linedefs() {
            //for seg in segs.iter() {
            if let Some(contact) = self.pit_check_line(ctrl, &line) {
                if let Some(point) = contact.point_contacted {
                    if !points.contains(&point) {
                        points.push(point);
                        contacts.push(contact);
                    }
                } else {
                    contacts.push(contact);
                }
            }
        }
        contacts
    }

    fn get_contacts_in_ssects(
        &mut self,
        subsects: &[DPtr<SubSector>],
        ctrl: &mut SubSectorMinMax,
        map_data: &MapData,
    ) -> Vec<LineContact> {
        let mut points = Vec::new();
        let mut contacts: Vec<LineContact> = Vec::new();
        // TODO: record checked lines
        let segs = map_data.get_segments();
        for subsect in subsects.iter() {
            //let sector = &subsect.sector;
            //for line in sector.lines.iter() {}
            for seg in &segs[subsect.start_seg as usize
                ..(subsect.start_seg + subsect.seg_count) as usize]
            {
                //for seg in segs.iter() {
                if let Some(contact) = self.pit_check_line(ctrl, &seg.linedef) {
                    if let Some(point) = contact.point_contacted {
                        if !points.contains(&point) {
                            points.push(point);
                            contacts.push(contact);
                        }
                    } else {
                        contacts.push(contact);
                    }
                }
            }
        }
        contacts
    }

    fn resolve_contacts(&mut self, contacts: &[LineContact]) {
        for contact in contacts.iter() {
            let relative = contact.normal * contact.penetration;
            self.momxy -= relative;
        }
    }

    /// P_TryMove, merged with P_CheckPosition and using a more verbose/modern collision
    pub fn p_try_move(&mut self, level: &mut Level) {
        // P_CrossSpecialLine
        level.mobj_ctrl.floatok = false;

        let ctrl = &mut level.mobj_ctrl;
        // TODO: ceilingline = NULL;

        // First sector is always the one we are in
        let curr_ssect = level.map_data.point_in_subsector(&self.xy);
        ctrl.min_floor_z = curr_ssect.sector.floorheight;
        ctrl.max_dropoff = curr_ssect.sector.floorheight;
        ctrl.max_ceil_z = curr_ssect.sector.ceilingheight;

        // TODO: validcount++;??? There's like, two places in the p_map.c file
        if ctrl.tmflags & MapObjectFlag::MF_NOCLIP as u32 != 0 {
            return;
        }

        // Check things first, possibly picking things up.
        // TODO: P_BlockThingsIterator, PIT_CheckThing

        // This is effectively P_BlockLinesIterator, PIT_CheckLine
        let contacts = self.get_contacts_map(ctrl, &level.map_data);
        //self.get_contacts_in_ssects(&subsects, ctrl, &level.map_data);

        // TODO: find the most suitable contact to move with (wall sliding)
        if !contacts.is_empty() {
            if contacts[0].point_contacted.is_some() {
                // Have to pad the penetration by 1.0 to prevent a bad clip
                // on some occasions, like going full speed in to a corner
                self.momxy -=
                    contacts[0].normal * (contacts[0].penetration + 1.0);
            } else {
                self.momxy = contacts[0].slide_dir
                    * contacts[0].angle_delta
                    * self.momxy.length();
            }
            let contacts = self.get_contacts_map(ctrl, &level.map_data);
            //self.get_contacts_in_ssects(&subsects, ctrl, &level.map_data);
            self.resolve_contacts(&contacts);
        }

        let old_pos = self.xy;

        self.xy += self.momxy;
        if ctrl.min_floor_z - self.z <= 24.0 || ctrl.min_floor_z <= self.z {
            self.floorz = ctrl.min_floor_z;
            self.ceilingz = ctrl.max_ceil_z;
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
        if ld.point_on_side(&self.xy) == 1 {
            return None;
        }

        if let Some(contact) = circle_to_seg_intersect(
            self.xy,
            self.momxy,
            self.radius,
            *ld.v1,
            *ld.v2,
        ) {
            // TODO: really need to check the lines of the subsector on the
            //  on the other side of the contact too

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

            // These are the very specific portal collisions
            if self.flags & MapObjectFlag::MF_TELEPORT as u32 != 0
                && portal.top_z - self.z < self.height
            {
                return Some(contact);
            }

            if portal.bottom_z - self.z > 24.0 {
                return Some(contact);
            }

            if self.flags
                & (MapObjectFlag::MF_DROPOFF as u32
                    | MapObjectFlag::MF_FLOAT as u32)
                != 0
                && portal.bottom_z - portal.lowest_z > 24.0
            {
                return Some(contact);
            }

            // // Line crossed, we might be colliding a nearby line
            // if let Some(back) = &ld.backsector {
            //     for line in back.lines.iter() {
            //         if *line.v1 == *ld.v1 && *line.v2 == *ld.v2
            //             || *line.v1 == *ld.v2 && *line.v2 == *ld.v1
            //         {
            //             continue;
            //         }
            //         if let Some(contact) = self.pit_check_line(ctrl, line) {
            //             return Some(contact);
            //         }
            //     }
            // }
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
