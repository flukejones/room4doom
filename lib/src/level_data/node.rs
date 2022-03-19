use std::f32::consts::PI;

use crate::level_data::map_defs::Node;

use crate::{play::utilities::ray_to_line_intersect, radian_range};
use glam::Vec2;

impl Node {
    /// R_PointOnSide
    ///
    /// Determine with cross-product which side of a splitting line the point is on
    pub fn point_on_side(&self, v: &Vec2) -> usize {
        let dx = v.x() - self.xy.x();
        let dy = v.y() - self.xy.y();

        if (self.delta.y() * dx) > (dy * self.delta.x()) {
            return 0;
        }
        1
    }

    /// Useful for finding the subsector that a Point is located in
    ///
    /// 0 == right, 1 == left
    pub fn point_in_bounds(&self, v: &Vec2, side: usize) -> bool {
        if v.x() > self.bounding_boxes[side][0].x()
            && v.x() < self.bounding_boxes[side][1].x()
            && v.y() < self.bounding_boxes[side][0].y()
            && v.y() > self.bounding_boxes[side][1].y()
        {
            return true;
        }
        false
    }

    /// half_fov must be in radians
    /// R_CheckBBox - r_bsp
    ///
    /// TODO: solidsegs list
    pub fn bb_extents_in_fov(
        &self,
        vec: &Vec2,
        angle_rads: f32,
        half_fov: f32,
        side: usize,
    ) -> bool {
        let mut origin_ang = angle_rads;

        let top_left = &self.bounding_boxes[side][0];
        let bottom_right = &self.bounding_boxes[side][1];

        // Super broadphase: check if we are in a BB, this will be true for each
        // progressively smaller BB (as the BSP splits down)
        if self.point_in_bounds(vec, side) {
            return true;
        }

        // Make sure we never compare across the 360->0 range
        let shift = if (angle_rads - half_fov).is_sign_negative() {
            half_fov
        } else if angle_rads + half_fov > PI * 2.0 {
            -half_fov
        } else {
            0.0
        };
        //origin_ang = radian_range(origin_ang + shift);
        origin_ang += shift;

        // Secondary broad phase check if each corner is in fov angle
        for x in [top_left.x(), bottom_right.x()].iter() {
            for y in [top_left.y(), bottom_right.y()].iter() {
                // generate angle from object position to bb corner
                let mut v_angle = (y - vec.y()).atan2(x - vec.x());
                v_angle = (origin_ang - radian_range(v_angle + shift)).abs();
                if v_angle <= half_fov {
                    return true;
                }
            }
        }

        // Fine phase, raycasting
        self.ray_from_point_intersect(vec, angle_rads, side)
    }

    pub fn ray_from_point_intersect(&self, origin_v: &Vec2, origin_ang: f32, side: usize) -> bool {
        let steps = 90.0; //half_fov * (180.0 / PI); // convert fov to degrees
        let step_size = 5; //steps as usize / 1;
        let top_left = &self.bounding_boxes[side][0];
        let bottom_right = &self.bounding_boxes[side][1];
        // Fine phase, check if a ray intersects any box line made from diagonals from corner
        // to corner. This will often catch cases where we want to see what's in a BB, but the FOV
        // is passing through the box with extents on outside of FOV
        let top_right = Vec2::new(bottom_right.x(), top_left.y());
        let bottom_left = Vec2::new(top_left.x(), bottom_right.y());
        // Start from FOV edges to catch the FOV passing through a BB case early
        // In reality this hardly ever fires for BB
        for i in (0..=steps as u32).rev().step_by(step_size) {
            // From center outwards
            let left_fov = origin_ang + (i as f32).to_radians();
            let right_fov = origin_ang - (i as f32).to_radians();
            // We don't need the result from this, just need to know if it's "None"
            if ray_to_line_intersect(origin_v, left_fov, top_left, bottom_right).is_some() {
                return true;
            }

            if ray_to_line_intersect(origin_v, left_fov, &bottom_left, &top_right).is_some() {
                return true;
            }

            if ray_to_line_intersect(origin_v, right_fov, top_left, bottom_right).is_some() {
                return true;
            }

            if ray_to_line_intersect(origin_v, right_fov, &bottom_left, &top_right).is_some() {
                return true;
            }
        }
        false
    }
}
