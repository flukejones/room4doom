use std::f32::consts::PI;

use glam::Vec2;
use utils::*;

use crate::Vertex;

pub const IS_SSECTOR_MASK: u16 = 0x8000;

/// The base node structure as parsed from the WAD records. What is stored in the WAD
/// is the splitting line used for splitting the map/node (starts with the map then
/// consecutive nodes, aiming for an even split if possible), a box which encapsulates
/// the left and right regions of the split, and the index numbers for left and right
/// children of the node; the index is in to the array built from this lump.
///
/// **The last node is the root node**
///
/// The data in the WAD lump is structured as follows:
///
/// | Field Size | Data Type                            | Content                                          |
/// |------------|--------------------------------------|--------------------------------------------------|
/// | 0x00-0x03  | Partition line x coordinate          | X coordinate of the splitter                     |
/// | 0x02-0x03  | Partition line y coordinate          | Y coordinate of the splitter                     |
/// | 0x04-0x05  | Change in x to end of partition line | The amount to move in X to reach end of splitter |
/// | 0x06-0x07  | Change in y to end of partition line | The amount to move in Y to reach end of splitter |
/// | 0x08-0x09  | Right (Front) box top                | First corner of front box (Y coordinate)         |
/// | 0x0A-0x0B  | Right (Front)  box bottom            | Second corner of front box (Y coordinate)        |
/// | 0x0C-0x0D  | Right (Front)  box left              | First corner of front box (X coordinate)         |
/// | 0x0E-0x0F  | Right (Front)  box right             | Second corner of front box (X coordinate)        |
/// | 0x10-0x11  | Left (Back) box top                  | First corner of back box (Y coordinate)          |
/// | 0x12-0x13  | Left (Back)  box bottom              | Second corner of back box (Y coordinate)         |
/// | 0x14-0x15  | Left (Back)  box left                | First corner of back box (X coordinate)          |
/// | 0x16-0x17  | Left (Back)  box right               | Second corner of back box (X coordinate)         |
/// | 0x18-0x19  | Right (Front) child index            | Index of the front child + sub-sector indicator  |
/// | 0x1A-0x1B  | Left (Back)  child index             | Index of the back child + sub-sector indicator   |
#[derive(Debug, Clone)]
pub struct Node {
    /// Where the line used for splitting the map starts
    pub split_start:    Vertex,
    /// Where the line used for splitting the map ends
    pub split_delta:    Vertex,
    /// Coordinates of the bounding boxes:
    /// - [0][0] == right box, top-left
    /// - [0][1] == right box, bottom-right
    /// - [1][0] == left box, top-left
    /// - [1][1] == left box, bottom-right
    pub bounding_boxes: [[Vertex; 2]; 2],
    /// The node children. Doom uses a clever trick where if one node is selected
    /// then the other can also be checked with the same/minimal code by inverting
    /// the last bit
    pub child_index:    [u16; 2],
}

impl Node {
    pub fn new(
        split_start: Vertex,
        split_delta: Vertex,
        bounding_boxes: [[Vertex; 2]; 2],
        right_child_id: u16,
        left_child_id: u16,
    ) -> Node {
        Node {
            split_start,
            split_delta,
            bounding_boxes,
            child_index: [right_child_id, left_child_id],
        }
    }

    /// R_PointOnSide
    ///
    /// Determine with cross-product which side of a splitting line the point is on
    pub fn point_on_side(&self, v: &Vertex) -> usize {
        let dx = v.x() - self.split_start.x();
        let dy = v.y() - self.split_start.y();

        if (self.split_delta.y() * dx) > (dy * self.split_delta.x()) {
            return 0;
        }
        1
    }

    /// Useful for finding the subsector that a Point is located in
    ///
    /// 0 == right, 1 == left
    pub fn point_in_bounds(&self, v: &Vertex, side: usize) -> bool {
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
    /// R_CheckBBox
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
        self.ray_from_point_intersect(&vec, angle_rads, side)
    }

    pub fn ray_from_point_intersect(
        &self,
        origin_v: &Vertex,
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
        let top_right = Vertex::new(bottom_right.x(), top_left.y());
        let bottom_left = Vertex::new(top_left.x(), bottom_right.y());
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
//         assert_eq!(nodes[0].split_start.x() as i32, 1552);
//         assert_eq!(nodes[0].split_start.y() as i32, -2432);
//         assert_eq!(nodes[0].split_delta.x() as i32, 112);
//         assert_eq!(nodes[0].split_delta.y() as i32, 0);

//         assert_eq!(nodes[0].bounding_boxes[0][0].x() as i32, 1552); //top
//         assert_eq!(nodes[0].bounding_boxes[0][0].y() as i32, -2432); //bottom

//         assert_eq!(nodes[0].bounding_boxes[1][0].x() as i32, 1600);
//         assert_eq!(nodes[0].bounding_boxes[1][0].y() as i32, -2048);

//         assert_eq!(nodes[0].child_index[0], 32768);
//         assert_eq!(nodes[0].child_index[1], 32769);
//         assert_eq!(IS_SSECTOR_MASK, 0x8000);

//         println!("{:#018b}", IS_SSECTOR_MASK);

//         println!("00: {:#018b}", nodes[0].child_index[0]);
//         dbg!(nodes[0].child_index[0] & IS_SSECTOR_MASK);
//         println!("00: {:#018b}", nodes[0].child_index[1]);
//         dbg!(nodes[0].child_index[1] & IS_SSECTOR_MASK);

//         println!("01: {:#018b}", nodes[1].child_index[0]);
//         dbg!(nodes[1].child_index[0] & IS_SSECTOR_MASK);
//         println!("01: {:#018b}", nodes[1].child_index[1]);
//         dbg!(nodes[1].child_index[1] & IS_SSECTOR_MASK);

//         println!("02: {:#018b}", nodes[2].child_index[0]);
//         dbg!(nodes[2].child_index[0]);
//         println!("02: {:#018b}", nodes[2].child_index[1]);
//         dbg!(nodes[2].child_index[1] & IS_SSECTOR_MASK);
//         dbg!(nodes[2].child_index[1] ^ IS_SSECTOR_MASK);

//         println!("03: {:#018b}", nodes[3].child_index[0]);
//         dbg!(nodes[3].child_index[0]);
//         println!("03: {:#018b}", nodes[3].child_index[1]);
//         dbg!(nodes[3].child_index[1]);
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
