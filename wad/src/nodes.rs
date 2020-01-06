use crate::lumps::Vertex;

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
#[derive(Debug)]
pub struct Node {
    /// Where the line used for splitting the map starts
    pub split_start: Vertex,
    /// Where the line used for splitting the map ends
    pub split_change: Vertex,
    /// Coordinates of the top-left vertex
    pub right_box_start: Vertex,
    /// Coordinates of the bottom-right vertex
    pub right_box_end: Vertex,
    /// Coordinates of the top-left vertex
    pub left_box_start: Vertex,
    /// Coordinates of the bottom-right vertex
    pub left_box_end: Vertex,
    /// Index number of the right child node (in order of WAD data)
    pub right_child_id: u16,
    /// Index number of the left child node (in order of WAD data)
    pub left_child_id: u16,
}

impl Node {
    pub fn new(
        split_start: Vertex,
        split_change: Vertex,
        right_box_start: Vertex,
        right_box_end: Vertex,
        left_box_start: Vertex,
        left_box_end: Vertex,
        right_child_id: u16,
        left_child_id: u16,
    ) -> Node {
        Node {
            split_start,
            split_change,
            right_box_start,
            right_box_end,
            left_box_start,
            left_box_end,
            right_child_id,
            left_child_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::lumps::*;
    use crate::map;
    use crate::wad::Wad;

    #[test]
    fn load_e1m1() {
        let mut wad = Wad::new("../doom1.wad");
        wad.read_directories();

        let mut map = map::Map::new("E1M1".to_owned());
        wad.load_map(&mut map);

        let nodes = map.get_nodes();
        assert_eq!(nodes[0].split_start.x, 1552);
        assert_eq!(nodes[0].split_start.y, -2432);
        assert_eq!(nodes[0].split_change.x, 112);
        assert_eq!(nodes[0].split_change.y, 0);

        assert_eq!(nodes[0].right_box_start.x, 1552); //top
        assert_eq!(nodes[0].right_box_start.y, -2432); //bottom

        assert_eq!(nodes[0].left_box_start.x, 1600);
        assert_eq!(nodes[0].left_box_start.y, -2048);

        assert_eq!(nodes[0].right_child_id, 32768);
        assert_eq!(nodes[0].left_child_id, 32769);
    }
}
