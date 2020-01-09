use crate::lumps::Vertex;
use std::f32::consts::PI;

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
///
/// # Examples:
/// ### Testing nodes
///
/// Test if a node is an index to another node in the tree or is an index to a `SubSector`
/// ```
/// # use wad::{Wad, map, nodes::IS_SSECTOR_MASK};
/// # let mut wad = Wad::new("../doom1.wad");
/// # wad.read_directories();
/// # let mut map = map::Map::new("E1M1".to_owned());
/// # wad.load_map(&mut map);
/// let nodes = map.get_nodes();
/// // Test if it is a child node or a leaf node
/// if nodes[2].child_index[0] & IS_SSECTOR_MASK == IS_SSECTOR_MASK {
///     // It's a leaf node, so it's a subsector index
///     let ssect_index = nodes[2].child_index[0] ^ IS_SSECTOR_MASK;
///     panic!("The right child of this node should be an index to another node")
/// } else {
///     // It's a child node and is the index to another node in the tree
///     let node_index = nodes[2].child_index[0];
///     assert_eq!(node_index, 1);
/// }
///
/// // Both sides function the same
/// // The left child of this node is an index to a SubSector
/// if nodes[2].child_index[1] & IS_SSECTOR_MASK == IS_SSECTOR_MASK {
///     // It's a leaf node
///     let ssect_index = nodes[2].child_index[1] ^ IS_SSECTOR_MASK;
///     assert_eq!(ssect_index, 4);
/// } else {
///     let node_index = nodes[2].child_index[1];
///     panic!("The left child of node 3 should be an index to a SubSector")
/// }
///
/// ```
///
/// ### Testing nodes
///
/// Find the subsector a player is in
/// ```
/// # use wad::{Wad, map, nodes::IS_SSECTOR_MASK};
/// # use wad::lumps::{Vertex, SubSector};
/// # use wad::nodes::Node;
/// # let mut wad = Wad::new("../doom1.wad");
/// # wad.read_directories();
/// # let mut map = map::Map::new("E1M1".to_owned());
/// # wad.load_map(&mut map);
///
/// // These are the coordinates for Player 1 in the WAD
/// let player = Vertex::new(1056, -3616);
/// let nodes = map.get_nodes();
///
/// fn find_subsector(v: &Vertex, node_id: u16, nodes: &[Node]) -> Option<u16> {
///     // Test if it is a child node or a leaf node
///     if node_id & IS_SSECTOR_MASK == IS_SSECTOR_MASK {
///         println!("{:#018b}", node_id & IS_SSECTOR_MASK);
///         // It's a leaf node and is the index to a subsector
///         //dbg!(&nodes[index as usize]);
///         return Some(node_id ^ IS_SSECTOR_MASK);
///     }
///
///     let dx = (v.x - nodes[node_id as usize].split_start.x) as i32;
///     let dy = (v.y - nodes[node_id as usize].split_start.y) as i32;
///     if (dx * nodes[node_id as usize].split_delta.y as i32)
///         - (dy * nodes[node_id as usize].split_delta.x as i32) <= 0 {
///         println!("BRANCH LEFT");
///         return find_subsector(&v, nodes[node_id as usize].child_index[1], nodes);
///     } else {
///         println!("BRANCH RIGHT");
///         return find_subsector(&v, nodes[node_id as usize].child_index[0], nodes);
///     }
///     None
/// }
///
/// let id = find_subsector(&player, (nodes.len() - 1) as u16, &nodes);
/// assert_eq!(id, Some(103));
/// assert_eq!(&map.get_subsectors()[id.unwrap() as usize].seg_count, &5);
/// assert_eq!(&map.get_subsectors()[id.unwrap() as usize].start_seg, &305);
/// ```
#[derive(Debug)]
pub struct Node {
    /// Where the line used for splitting the map starts
    pub split_start: Vertex,
    /// Where the line used for splitting the map ends
    pub split_delta: Vertex,
    /// Coordinates of the bounding boxes:
    /// - [0][0] == right box, top-left
    /// - [0][1] == right box, bottom-right
    /// - [1][0] == left box, top-left
    /// - [1][1] == left box, bottom-right
    pub bounding_boxes: [[Vertex; 2]; 2],
    /// The node children. Doom uses a cleaver trick where if one node is selected
    /// then the other can also be checked with the same/minimal code by inverting
    /// the last bit
    pub child_index: [u16; 2],
}

impl Node {
    pub fn new(
        split_start: Vertex,
        split_delta: Vertex,
        right_box_start: Vertex,
        right_box_end: Vertex,
        left_box_start: Vertex,
        left_box_end: Vertex,
        right_child_id: u16,
        left_child_id: u16,
    ) -> Node {
        Node {
            split_start,
            split_delta,
            bounding_boxes: [
                [right_box_start, right_box_end],
                [left_box_start, left_box_end],
            ],
            child_index: [right_child_id, left_child_id],
        }
    }

    /// Transliteration of R_PointOnSide from Chocolate Doom
    pub fn point_on_side(&self, v: &Vertex) -> usize {
        // if horizontal
        if self.split_delta.x == 0 && v.x == self.split_start.x {
            return (self.split_delta.y > 0) as usize;
        }
        // if vertical
        if self.split_delta.y == 0 && v.y == self.split_start.y {
            return (self.split_delta.x > 0) as usize;
        }

        let dx = (v.x - self.split_start.x) as i32;
        let dy = (v.y - self.split_start.y) as i32;

        if (dx * self.split_delta.y as i32) > (dy * self.split_delta.x as i32) {
            return 0;
        }
        1
    }

    /// 0 == right, 1 == left
    pub fn point_in_bounds(&self, v: &Vertex, side: usize) -> bool {
        if v.x > self.bounding_boxes[side][0].x
            && v.x < self.bounding_boxes[side][1].x
            && v.y > self.bounding_boxes[side][0].y
            && v.y < self.bounding_boxes[side][1].y
        {
            return true;
        }
        false
    }

    pub fn bb_extents_in_fov(&self, point: &Vertex, point_angle: f32, side: usize) -> bool {
        let ang40 = 45.0 * PI / 180.0;
        let ang90 = 90.0 * PI / 180.0;

        // Make sure range is in 2*PI
        let radian_range = |rad: f32| -> f32 {
            if rad < 0.0 {
                return rad + 2.0 * PI;
            } else if rad >= 2.0 * PI {
                return rad - 2.0 * PI;
            }
            rad
        };
        let point_angle = radian_range(point_angle + ang40);

        let top_left_angle = radian_range(
            ((self.bounding_boxes[side][0].y - point.y) as f32)
                .atan2((self.bounding_boxes[side][0].x - point.x) as f32)
                + ang40,
        ) - point_angle;
        if top_left_angle.abs() <= ang40 {
            return true;
        }

        let top_right_angle = radian_range(
            ((self.bounding_boxes[side][0].y - point.y) as f32)
                .atan2((self.bounding_boxes[side][1].x - point.x) as f32)
                + ang40,
        ) - point_angle;
        if top_right_angle.abs() <= ang40 {
            return true;
        }

        let bottom_right_angle = radian_range(
            ((self.bounding_boxes[side][1].y - point.y) as f32)
                .atan2((self.bounding_boxes[side][1].x - point.x) as f32)
                + ang40,
        ) - point_angle;
        if bottom_right_angle.abs() <= ang40 {
            return true;
        }

        let bottom_left_angle = radian_range(
            ((self.bounding_boxes[side][1].y - point.y) as f32)
                .atan2((self.bounding_boxes[side][0].x - point.x) as f32)
                + ang40,
        ) - point_angle;
        if bottom_left_angle.abs() <= ang40 {
            return true;
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use crate::lumps::*;
    use crate::map;
    use crate::nodes::IS_SSECTOR_MASK;
    use crate::wad::Wad;

    #[test]
    fn check_nodes_of_e1m1() {
        let mut wad = Wad::new("../doom1.wad");
        wad.read_directories();

        let mut map = map::Map::new("E1M1".to_owned());
        wad.load_map(&mut map);

        let nodes = map.get_nodes();
        assert_eq!(nodes[0].split_start.x, 1552);
        assert_eq!(nodes[0].split_start.y, -2432);
        assert_eq!(nodes[0].split_delta.x, 112);
        assert_eq!(nodes[0].split_delta.y, 0);

        assert_eq!(nodes[0].bounding_boxes[0][0].x, 1552); //top
        assert_eq!(nodes[0].bounding_boxes[0][0].y, -2432); //bottom

        assert_eq!(nodes[0].bounding_boxes[1][0].x, 1600);
        assert_eq!(nodes[0].bounding_boxes[1][0].y, -2048);

        assert_eq!(nodes[0].child_index[0], 32768);
        assert_eq!(nodes[0].child_index[1], 32769);
        assert_eq!(IS_SSECTOR_MASK, 0x8000);

        println!("{:#018b}", IS_SSECTOR_MASK);

        println!("00: {:#018b}", nodes[0].child_index[0]);
        dbg!(nodes[0].child_index[0] & IS_SSECTOR_MASK);
        println!("00: {:#018b}", nodes[0].child_index[1]);
        dbg!(nodes[0].child_index[1] & IS_SSECTOR_MASK);

        println!("01: {:#018b}", nodes[1].child_index[0]);
        dbg!(nodes[1].child_index[0] & IS_SSECTOR_MASK);
        println!("01: {:#018b}", nodes[1].child_index[1]);
        dbg!(nodes[1].child_index[1] & IS_SSECTOR_MASK);

        println!("02: {:#018b}", nodes[2].child_index[0]);
        dbg!(nodes[2].child_index[0]);
        println!("02: {:#018b}", nodes[2].child_index[1]);
        dbg!(nodes[2].child_index[1] & IS_SSECTOR_MASK);
        dbg!(nodes[2].child_index[1] ^ IS_SSECTOR_MASK);

        println!("03: {:#018b}", nodes[3].child_index[0]);
        dbg!(nodes[3].child_index[0]);
        println!("03: {:#018b}", nodes[3].child_index[1]);
        dbg!(nodes[3].child_index[1]);
    }

    #[test]
    fn find_vertex_using_bsptree() {
        let mut wad = Wad::new("../doom1.wad");
        wad.read_directories();

        let mut map = map::Map::new("E1M1".to_owned());
        wad.load_map(&mut map);

        let player = Vertex::new(1056, -3616);
        let nodes = map.get_nodes();
        let subsector_id = map.find_subsector(&player, (nodes.len() - 1) as u16, nodes);
        assert_eq!(subsector_id, Some(103));
        assert_eq!(
            &map.get_subsectors()[subsector_id.unwrap() as usize].seg_count,
            &5
        );
        assert_eq!(
            &map.get_subsectors()[subsector_id.unwrap() as usize].start_seg,
            &305
        );
    }
}
