use crate::level::map_defs::Node;

// use crate::play::utilities::ray_to_line_intersect;
use glam::Vec2;

impl Node {
    /// R_PointOnSide
    ///
    /// Determine with cross-product which side of a splitting line the point is on
    pub fn point_on_side(&self, v: &Vec2) -> usize {
        let dx = v.x - self.xy.x;
        let dy = v.y - self.xy.y;

        if (self.delta.y * dx) > (dy * self.delta.x) {
            return 0;
        }
        1
    }

    pub fn point_in_bounds(&self, v: &Vec2, side: usize) -> bool {
        if v.x > self.bounding_boxes[side][0].x
            && v.x < self.bounding_boxes[side][1].x
            && v.y < self.bounding_boxes[side][0].y
            && v.y > self.bounding_boxes[side][1].y
        {
            return true;
        }
        false
    }

    // #[inline]
    // pub fn cross(lhs: &Vec2, rhs: &Vec2) -> f32 {
    //     lhs.x * rhs.y - lhs.y * rhs.x
    // }

    // #[inline]
    // pub fn ray_to_line_intersect(
    //     origin: &Vec2,
    //     direction: f32,
    //     point1: &Vec2,
    //     point2: &Vec2,
    // ) -> Option<f32> {
    //     let direction = unit_vec_from(direction);
    //     let v1 = *origin - *point1;
    //     let v2 = *point2 - *point1;
    //     let v3 = Vec2::new(-direction.y, direction.x);
    //     let dot = v2.dot(v3);
    //     if dot.abs() < 0.000001 {
    //         return None;
    //     }
    //     let t1 = dot / cross(&v2, &v1);
    //     let t2 = v1.dot(v3) / dot;
    //     if t1 >= 0.0 && t2 >= 0.0 && t2 <= 1.0 {
    //         return Some(t1);
    //     }
    //     None
    // }

    // Old fun ray-casting stuff
    // pub fn ray_from_point_intersect(&self, origin_v: &Vec2, origin_ang: f32, side: usize) -> bool {
    //     let steps = 90.0; //half_fov * (180.0 / PI); // convert fov to degrees
    //     let step_size = 5; //steps as usize / 1;
    //     let top_left = &self.bounding_boxes[side][0];
    //     let bottom_right = &self.bounding_boxes[side][1];
    //     // Fine phase, check if a ray intersects any box line made from diagonals from corner
    //     // to corner. This will often catch cases where we want to see what's in a BB, but the FOV
    //     // is passing through the box with extents on outside of FOV
    //     let top_right = Vec2::new(bottom_right.x, top_left.y);
    //     let bottom_left = Vec2::new(top_left.x, bottom_right.y);
    //     // Start from FOV edges to catch the FOV passing through a BB case early
    //     // In reality this hardly ever fires for BB
    //     for i in (0..=steps as u32).rev().step_by(step_size) {
    //         // From center outwards
    //         let left_fov = origin_ang + (i as f32).to_radians();
    //         let right_fov = origin_ang - (i as f32).to_radians();
    //         // We don't need the result from this, just need to know if it's "None"
    //         if ray_to_line_intersect(origin_v, left_fov, top_left, bottom_right).is_some() {
    //             return true;
    //         }

    //         if ray_to_line_intersect(origin_v, left_fov, &bottom_left, &top_right).is_some() {
    //             return true;
    //         }

    //         if ray_to_line_intersect(origin_v, right_fov, top_left, bottom_right).is_some() {
    //             return true;
    //         }

    //         if ray_to_line_intersect(origin_v, right_fov, &bottom_left, &top_right).is_some() {
    //             return true;
    //         }
    //     }
    //     false
    // }
}
