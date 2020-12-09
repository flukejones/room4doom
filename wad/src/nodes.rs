use std::f32::consts::PI;

use crate::lumps::{Node, WadVertex};
use utils::*;

pub const IS_SSECTOR_MASK: u16 = 0x8000;

impl Node {
    /// R_PointOnSide
    ///
    /// Determine with cross-product which side of a splitting line the point is on
    pub fn point_on_side(&self, v: &WadVertex) -> usize {
        let dx = v.x - self.split_start.x;
        let dy = v.y - self.split_start.y;

        if (self.split_delta.y * dx) > (dy * self.split_delta.x) {
            return 0;
        }
        1
    }

    /// Useful for finding the subsector that a Point is located in
    ///
    /// 0 == right, 1 == left
    pub fn point_in_bounds(&self, v: &WadVertex, side: usize) -> bool {
        if v.x > self.bounding_boxes[side][0].x
            && v.x < self.bounding_boxes[side][1].x
            && v.y < self.bounding_boxes[side][0].y
            && v.y > self.bounding_boxes[side][1].y
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
        if self.point_in_bounds(&vec, side) {
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
        origin_ang = origin_ang + shift;

        // Secondary broad phase check if each corner is in fov angle
        for x in [top_left.x, bottom_right.x].iter() {
            for y in [top_left.y, bottom_right.y].iter() {
                // generate angle from object position to bb corner
                let mut v_angle = (y - vec.y).atan2(x - vec.x);
                v_angle = (origin_ang - radian_range(v_angle + shift)).abs();
                if v_angle <= half_fov {
                    return true;
                }
            }
        }

        // Fine phase, raycasting
        self.ray_from_point_intersect(&vec, angle_rads, side)
    }

    pub fn ray_from_point_intersect(
        &self,
        origin_v: &WadVertex,
        origin_ang: f32,
        side: usize,
    ) -> bool {
        let steps = 90.0; //half_fov * (180.0 / PI); // convert fov to degrees
        let step_size = 5; //steps as usize / 1;
        let top_left = &self.bounding_boxes[side][0];
        let bottom_right = &self.bounding_boxes[side][1];
        // Fine phase, check if a ray intersects any box line made from diagonals from corner
        // to corner. This will often catch cases where we want to see what's in a BB, but the FOV
        // is passing through the box with extents on outside of FOV
        let top_right = WadVertex::new(bottom_right.x, top_left.y);
        let bottom_left = WadVertex::new(top_left.x, bottom_right.y);
        // Start from FOV edges to catch the FOV passing through a BB case early
        // In reality this hardly ever fires for BB
        for i in (0..=steps as u32).rev().step_by(step_size) {
            // From center outwards
            let left_fov = origin_ang + (i as f32 * PI / 180.0); // convert the step to rads
            let right_fov = origin_ang - (i as f32 * PI / 180.0);
            // We don't need the result from this, just need to know if it's "None"
            if ray_to_line_intersect(origin_v, left_fov, top_left, bottom_right)
                .is_some()
            {
                return true;
            }

            if ray_to_line_intersect(
                origin_v,
                left_fov,
                &bottom_left,
                &top_right,
            )
            .is_some()
            {
                return true;
            }

            if ray_to_line_intersect(
                origin_v,
                right_fov,
                top_left,
                bottom_right,
            )
            .is_some()
            {
                return true;
            }

            if ray_to_line_intersect(
                origin_v,
                right_fov,
                &bottom_left,
                &top_right,
            )
            .is_some()
            {
                return true;
            }
        }
        false
    }
}

// #[cfg(test)]
// mod tests {
//     use crate::nodes::IS_SSECTOR_MASK;
//     use gamelib::map::Map;
//     use crate::wad::Wad;
//     use crate::Vertex;

//     #[test]
//     fn check_nodes_of_e1m1() {
//         let mut wad = Wad::new("../doom1.wad");
//         wad.read_directories();

//         let mut map = Map::new("E1M1".to_owned());
//         map.load(&wad);

//         let nodes = map.get_nodes();
//         assert_eq!(nodes[0].split_start.x as i32, 1552);
//         assert_eq!(nodes[0].split_start.y as i32, -2432);
//         assert_eq!(nodes[0].split_delta.x as i32, 112);
//         assert_eq!(nodes[0].split_delta.y as i32, 0);

//         assert_eq!(nodes[0].bounding_boxes[0][0].x as i32, 1552); //top
//         assert_eq!(nodes[0].bounding_boxes[0][0].y as i32, -2432); //bottom

//         assert_eq!(nodes[0].bounding_boxes[1][0].x as i32, 1600);
//         assert_eq!(nodes[0].bounding_boxes[1][0].y as i32, -2048);

//         assert_eq!(nodes[0].child_index[0], 32768);
//         assert_eq!(nodes[0].child_index[1], 32769);
//         assert_eq!(IS_SSECTOR_MASK, 0x8000);

//         println!("{:#018b}", IS_SSECTOR_MASK);

//         println!("00: {:#018b}", nodes[0].child_index[0]);
//         println!("00: {:#018b}", nodes[0].child_index[1]);

//         println!("01: {:#018b}", nodes[1].child_index[0]);
//         println!("01: {:#018b}", nodes[1].child_index[1]);

//         println!("02: {:#018b}", nodes[2].child_index[0]);
//         println!("02: {:#018b}", nodes[2].child_index[1]);

//         println!("03: {:#018b}", nodes[3].child_index[0]);
//         println!("03: {:#018b}", nodes[3].child_index[1]);
//     }

//     #[test]
//     fn find_vertex_using_bsptree() {
//         let mut wad = Wad::new("../doom1.wad");
//         wad.read_directories();

//         let mut map = Map::new("E1M1".to_owned());
//         map.load(&wad);

//         let player = Vertex::new(1056.0, -3616.0);
//         let nodes = map.get_nodes();
//         let subsector = map
//             .find_subsector(&player, (nodes.len() - 1) as u16)
//             .unwrap();
//         //assert_eq!(subsector_id, Some(103));
//         assert_eq!(subsector.seg_count, 5);
//         assert_eq!(subsector.start_seg, 305);
//     }
// }
