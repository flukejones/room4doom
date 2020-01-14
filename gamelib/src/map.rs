use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};
use std::str;

use utils::*;
use wad::{lumps::*, DPtr, LumpIndex, Vertex, Wad};

/// The smallest vector and the largest vertex, combined make up a
/// rectangle enclosing the map area
#[derive(Debug, Default)]
pub struct MapExtents {
    pub min_vertex:    Vertex,
    pub max_vertex:    Vertex,
    pub width:         f32,
    pub height:        f32,
    pub automap_scale: f32,
}

/// A `Map` contains everything required for building the actual level the
/// player will see in-game, such as the data to build a map, the textures used,
/// `Things`, `Sounds` and others.
///
/// `nodes`, `subsectors`, and `segments` are what get used most to render the
/// basic map
///
/// Access to the `Vec` arrays within is limited to immutable only to
/// prevent unwanted removal of items, which *will* break references and
/// segfault
///
/// # Examples:
/// ### Testing nodes
///
/// Test if a node is an index to another node in the tree or is an index to a `SubSector`
/// ```
/// # use wad::{Wad, nodes::IS_SSECTOR_MASK};
/// # use gamelib::map::Map;
/// # let mut wad = Wad::new("../doom1.wad");
/// # wad.read_directories();
/// # let mut map = Map::new("E1M1".to_owned());
/// # map.load(&wad);
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
/// # use wad::{Wad, nodes::{Node, IS_SSECTOR_MASK}, Vertex};
/// # use gamelib::map::Map;
/// # let mut wad = Wad::new("../doom1.wad");
/// # wad.read_directories();
/// # let mut map = Map::new("E1M1".to_owned());
/// # map.load(&wad);
///
/// // These are the coordinates for Player 1 in the WAD
/// let player = Vertex::new(1056.0, -3616.0);
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
///     let dx = (v.x() - nodes[node_id as usize].split_start.x()) as i32;
///     let dy = (v.y() - nodes[node_id as usize].split_start.y()) as i32;
///     if (dx * nodes[node_id as usize].split_delta.y() as i32)
///         - (dy * nodes[node_id as usize].split_delta.x() as i32) <= 0 {
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
pub struct Map {
    name:       String,
    things:     Vec<Thing>,
    vertexes:   Vec<Vertex>,
    linedefs:   Vec<LineDef>,
    sectors:    Vec<Sector>,
    sidedefs:   Vec<SideDef>,
    subsectors: Vec<SubSector>,
    segments:   Vec<Segment>,
    extents:    MapExtents,
    nodes:      Vec<Node>,
    start_node: u16,
    fov:        f32,
    half_fov:   f32,
}

impl Map {
    pub fn new(name: String) -> Map {
        Map {
            name,
            things: Vec::new(),
            vertexes: Vec::new(),
            linedefs: Vec::new(),
            sectors: Vec::new(),
            sidedefs: Vec::new(),
            subsectors: Vec::new(),
            segments: Vec::new(),
            extents: MapExtents::default(),
            nodes: Vec::new(),
            start_node: 0,
            fov: FRAC_PI_2,
            half_fov: FRAC_PI_4,
        }
    }

    #[inline]
    pub fn get_name(&self) -> &str {
        &self.name
    }

    #[inline]
    pub fn get_things(&self) -> &[Thing] {
        &self.things
    }

    #[inline]
    pub fn set_extents(&mut self) {
        // set the min/max to first vertex so we have a baseline
        // that isn't 0 causing comparison issues, eg; if it's 0,
        // then a min vertex of -3542 won't be set since it's negative
        self.extents.min_vertex.set_x(self.vertexes[0].x());
        self.extents.min_vertex.set_y(self.vertexes[0].y());
        self.extents.max_vertex.set_x(self.vertexes[0].x());
        self.extents.max_vertex.set_y(self.vertexes[0].y());
        for v in &self.vertexes {
            if self.extents.min_vertex.x() > v.x() {
                self.extents.min_vertex.set_x(v.x());
            } else if self.extents.max_vertex.x() < v.x() {
                self.extents.max_vertex.set_x(v.x());
            }

            if self.extents.min_vertex.y() > v.y() {
                self.extents.min_vertex.set_y(v.y());
            } else if self.extents.max_vertex.y() < v.y() {
                self.extents.max_vertex.set_y(v.y());
            }
        }
        self.extents.width =
            self.extents.max_vertex.x() - self.extents.min_vertex.x();
        self.extents.height =
            self.extents.max_vertex.y() - self.extents.min_vertex.y();
    }

    #[inline]
    pub fn get_vertexes(&self) -> &[Vertex] {
        &self.vertexes
    }

    #[inline]
    pub fn get_linedefs(&self) -> &[LineDef] {
        &self.linedefs
    }

    #[inline]
    pub fn get_sectors(&self) -> &[Sector] {
        &self.sectors
    }

    #[inline]
    pub fn get_sidedefs(&self) -> &[SideDef] {
        &self.sidedefs
    }

    #[inline]
    pub fn get_subsectors(&self) -> &[SubSector] {
        &self.subsectors
    }

    #[inline]
    pub fn get_segments(&self) -> &[Segment] {
        &self.segments
    }

    #[inline]
    pub fn get_extents(&self) -> &MapExtents {
        &self.extents
    }

    #[inline]
    pub fn set_scale(&mut self, scale: f32) {
        self.extents.automap_scale = scale
    }

    #[inline]
    pub fn get_nodes(&self) -> &[Node] {
        &self.nodes
    }

    #[inline]
    pub fn start_node(&self) -> u16 {
        self.start_node
    }

    pub fn load<'m>(&mut self, wad: &Wad) {
        let index = wad.find_lump_index(self.get_name());
        // THINGS
        self.things =
            wad.read_lump_to_vec(index, LumpIndex::Things, 10, |offset| {
                Thing::new(
                    Vertex::new(
                        wad.read_2_bytes(offset) as i16 as f32,
                        wad.read_2_bytes(offset + 2) as i16 as f32,
                    ),
                    wad.read_2_bytes(offset + 4) as u16 as f32,
                    wad.read_2_bytes(offset + 6),
                    wad.read_2_bytes(offset + 8),
                )
            });
        // Vertexes
        self.vertexes =
            wad.read_lump_to_vec(index, LumpIndex::Vertexes, 4, |offset| {
                Vertex::new(
                    wad.read_2_bytes(offset) as i16 as f32,
                    wad.read_2_bytes(offset + 2) as i16 as f32,
                )
            });
        // Sectors
        self.sectors =
            wad.read_lump_to_vec(index, LumpIndex::Sectors, 26, |offset| {
                Sector::new(
                    wad.read_2_bytes(offset) as i16,
                    wad.read_2_bytes(offset + 2) as i16,
                    &wad.wad_data[offset + 4..offset + 12],
                    &wad.wad_data[offset + 12..offset + 20],
                    wad.read_2_bytes(offset + 20),
                    wad.read_2_bytes(offset + 22),
                    wad.read_2_bytes(offset + 24),
                )
            });
        // Sidedefs
        self.sidedefs =
            wad.read_lump_to_vec(index, LumpIndex::SideDefs, 30, |offset| {
                let sector =
                    &self.get_sectors()[wad.read_2_bytes(offset + 28) as usize];
                SideDef::new(
                    wad.read_2_bytes(offset) as i16,
                    wad.read_2_bytes(offset + 2) as i16,
                    &wad.wad_data[offset + 4..offset + 12],
                    &wad.wad_data[offset + 12..offset + 20],
                    &wad.wad_data[offset + 20..offset + 28],
                    DPtr::new(sector),
                )
            });
        //LineDefs
        self.linedefs =
            wad.read_lump_to_vec(index, LumpIndex::LineDefs, 14, |offset| {
                let start_vertex =
                    &self.get_vertexes()[wad.read_2_bytes(offset) as usize];
                let end_vertex =
                    &self.get_vertexes()[wad.read_2_bytes(offset + 2) as usize];
                let front_sidedef = &self.get_sidedefs()
                    [wad.read_2_bytes(offset + 10) as usize];
                let back_sidedef = {
                    let index = wad.read_2_bytes(offset + 12) as usize;
                    if index < 65535 {
                        Some(DPtr::new(&self.get_sidedefs()[index]))
                    } else {
                        None
                    }
                };
                LineDef::new(
                    DPtr::new(start_vertex),
                    DPtr::new(end_vertex),
                    wad.read_2_bytes(offset + 4),
                    wad.read_2_bytes(offset + 6),
                    wad.read_2_bytes(offset + 8),
                    DPtr::new(front_sidedef),
                    back_sidedef,
                )
            });
        // Sector, Sidedef, Linedef, Seg all need to be preprocessed before
        // storing in map struct
        //
        // SEGS
        self.segments =
            wad.read_lump_to_vec(index, LumpIndex::Segs, 12, |offset| {
                let start_vertex =
                    &self.get_vertexes()[wad.read_2_bytes(offset) as usize];
                let end_vertex =
                    &self.get_vertexes()[wad.read_2_bytes(offset + 2) as usize];
                let linedef =
                    &self.get_linedefs()[wad.read_2_bytes(offset + 6) as usize];
                Segment::new(
                    DPtr::new(start_vertex),
                    DPtr::new(end_vertex),
                    wad.read_2_bytes(offset + 4) as f32,
                    DPtr::new(linedef),
                    wad.read_2_bytes(offset + 8),
                    wad.read_2_bytes(offset + 10),
                )
            });
        // SSECTORS
        self.subsectors =
            wad.read_lump_to_vec(index, LumpIndex::SubSectors, 4, |offset| {
                let start_seg = wad.read_2_bytes(offset + 2);
                let sector = self.get_segments()[start_seg as usize]
                    .linedef
                    .front_sidedef
                    .sector
                    .clone();
                SubSector::new(sector, wad.read_2_bytes(offset), start_seg)
            });

        // NODES
        self.nodes =
            wad.read_lump_to_vec(index, LumpIndex::Nodes, 28, |offset| {
                Node::new(
                    Vertex::new(
                        wad.read_2_bytes(offset) as i16 as f32,
                        wad.read_2_bytes(offset + 2) as i16 as f32,
                    ),
                    Vertex::new(
                        wad.read_2_bytes(offset + 4) as i16 as f32,
                        wad.read_2_bytes(offset + 6) as i16 as f32,
                    ),
                    [
                        [
                            Vertex::new(
                                wad.read_2_bytes(offset + 12) as i16 as f32, // top
                                wad.read_2_bytes(offset + 8) as i16 as f32, // left
                            ),
                            Vertex::new(
                                wad.read_2_bytes(offset + 14) as i16 as f32, // bottom
                                wad.read_2_bytes(offset + 10) as i16 as f32, // right
                            ),
                        ],
                        [
                            Vertex::new(
                                wad.read_2_bytes(offset + 20) as i16 as f32,
                                wad.read_2_bytes(offset + 16) as i16 as f32,
                            ),
                            Vertex::new(
                                wad.read_2_bytes(offset + 22) as i16 as f32,
                                wad.read_2_bytes(offset + 18) as i16 as f32,
                            ),
                        ],
                    ],
                    wad.read_2_bytes(offset + 24),
                    wad.read_2_bytes(offset + 26),
                )
            });
        self.start_node = (self.nodes.len() - 1) as u16;
        self.set_extents();
    }

    pub fn find_subsector(
        &self,
        point: &Vertex,
        node_id: u16,
    ) -> Option<&SubSector> {
        // Test if it is a child node or a leaf node
        if node_id & IS_SSECTOR_MASK == IS_SSECTOR_MASK {
            // It's a leaf node and is the index to a subsector
            return Some(
                &self.get_subsectors()[(node_id ^ IS_SSECTOR_MASK) as usize],
            );
        }

        let node = &self.get_nodes()[node_id as usize];
        let side = node.point_on_side(&point);

        if let Some(res) = self.find_subsector(&point, node.child_index[side]) {
            return Some(res);
        }
        if node.point_in_bounds(&point, side ^ 1) {
            return self.find_subsector(&point, node.child_index[side ^ 1]);
        }
        None
    }

    pub fn add_line<'a>(
        &'a self,
        object: &Object,
        seg: &'a Segment,
        seg_list: &mut Vec<&'a Segment>,
    ) {
        if !seg.is_facing_point(&object.xy) {
            return;
        }

        // Is seg in front of the point?
        let unit = unit_vec_from(object.rotation) * 2.0;
        // Will usually be left of point
        let d1 = (*seg.start_vertex - object.xy).length_squared();
        let d2 = (*seg.start_vertex - (object.xy - unit)).length_squared();
        // also capture right of point
        let d3 = (*seg.end_vertex - object.xy).length_squared();
        let d4 = (*seg.end_vertex - (object.xy - unit)).length_squared();
        if d2 < d1 && d3 > d4 {
            return;
        }

        seg_list.push(seg);
    }

    pub fn draw_bsp<'a>(
        &'a self,
        object: &Object,
        node_id: u16,
        seg_list: &mut Vec<&'a Segment>,
    ) {
        if node_id & IS_SSECTOR_MASK == IS_SSECTOR_MASK {
            // It's a leaf node and is the index to a subsector
            let subsect =
                &self.get_subsectors()[(node_id ^ IS_SSECTOR_MASK) as usize];
            //let sector = subsect.sector;
            let segs = self.get_segments();

            for i in subsect.start_seg..subsect.start_seg + subsect.seg_count {
                let seg = &segs[i as usize];
                self.add_line(object, seg, seg_list);
            }
            return;
        }

        let node = &self.nodes[node_id as usize];

        let side = node.point_on_side(&object.xy);
        self.draw_bsp(object, node.child_index[side], seg_list);

        // check if each corner of the BB is in the FOV
        //if node.point_in_bounds(&v, side ^ 1) {
        if node.bb_extents_in_fov(object, self.half_fov, side ^ 1) {
            self.draw_bsp(object, node.child_index[side ^ 1], seg_list);
        }
    }
}

#[cfg(test)]
mod tests {
    use wad::Wad;

    use crate::map;

    #[test]
    fn check_e1m1_things() {
        let mut wad = Wad::new("../doom1.wad");
        wad.read_directories();

        let mut map = map::Map::new("E1M1".to_owned());
        map.load(&wad);

        let things = map.get_things();
        assert_eq!(things[0].pos.x() as i32, 1056);
        assert_eq!(things[0].pos.y() as i32, -3616);
        assert_eq!(things[0].angle as i32, 90);
        assert_eq!(things[0].kind, 1);
        assert_eq!(things[0].flags, 7);
        assert_eq!(things[137].pos.x() as i32, 3648);
        assert_eq!(things[137].pos.y() as i32, -3840);
        assert_eq!(things[137].angle as i32, 0);
        assert_eq!(things[137].kind, 2015);
        assert_eq!(things[137].flags, 7);

        assert_eq!(things[0].angle as i32, 90);
        assert_eq!(things[9].angle as i32, 135);
        assert_eq!(things[14].angle as i32, 0);
        assert_eq!(things[16].angle as i32, 90);
        assert_eq!(things[17].angle as i32, 180);
        assert_eq!(things[83].angle as i32, 270);
    }

    #[test]
    fn check_e1m1_vertexes() {
        let mut wad = Wad::new("../doom1.wad");
        wad.read_directories();

        let mut map = map::Map::new("E1M1".to_owned());
        map.load(&wad);

        let vertexes = map.get_vertexes();
        assert_eq!(vertexes[0].x() as i32, 1088);
        assert_eq!(vertexes[0].y() as i32, -3680);
        assert_eq!(vertexes[466].x() as i32, 2912);
        assert_eq!(vertexes[466].y() as i32, -4848);
    }

    #[test]
    fn check_e1m1_lump_pointers() {
        let mut wad = Wad::new("../doom1.wad");
        wad.read_directories();

        let mut map = map::Map::new("E1M1".to_owned());
        map.load(&wad);
        let linedefs = map.get_linedefs();

        // Check links
        // LINEDEF->VERTEX
        assert_eq!(linedefs[2].start_vertex.x() as i32, 1088);
        assert_eq!(linedefs[2].end_vertex.x() as i32, 1088);
        // LINEDEF->SIDEDEF
        assert_eq!(linedefs[2].front_sidedef.middle_tex, "LITE3");
        // LINEDEF->SIDEDEF->SECTOR
        assert_eq!(linedefs[2].front_sidedef.sector.floor_tex, "FLOOR4_8");
        // LINEDEF->SIDEDEF->SECTOR
        assert_eq!(linedefs[2].front_sidedef.sector.ceil_height, 72);

        let segments = map.get_segments();
        // SEGMENT->VERTEX
        assert_eq!(segments[0].start_vertex.x() as i32, 1552);
        assert_eq!(segments[0].end_vertex.x() as i32, 1552);
        // SEGMENT->LINEDEF->SIDEDEF->SECTOR
        // seg:0 -> line:152 -> side:209 -> sector:0 -> ceiltex:CEIL3_5 lightlevel:160
        assert_eq!(
            segments[0].linedef.front_sidedef.sector.ceil_tex,
            "CEIL3_5"
        );
        // SEGMENT->LINEDEF->SIDEDEF
        assert_eq!(segments[0].linedef.front_sidedef.upper_tex, "BIGDOOR2");

        let sides = map.get_sidedefs();
        assert_eq!(sides[211].sector.ceil_tex, "TLITE6_4");
    }

    #[test]
    fn check_e1m1_linedefs() {
        let mut wad = Wad::new("../doom1.wad");
        wad.read_directories();

        let mut map = map::Map::new("E1M1".to_owned());
        map.load(&wad);
        let linedefs = map.get_linedefs();
        assert_eq!(linedefs[0].start_vertex.x() as i32, 1088);
        assert_eq!(linedefs[0].end_vertex.x() as i32, 1024);
        assert_eq!(linedefs[2].start_vertex.x() as i32, 1088);
        assert_eq!(linedefs[2].end_vertex.x() as i32, 1088);

        assert_eq!(linedefs[474].start_vertex.x() as i32, 3536);
        assert_eq!(linedefs[474].end_vertex.x() as i32, 3520);
        assert!(linedefs[2].back_sidedef.is_none());
        assert_eq!(linedefs[474].flags, 1);
        assert!(linedefs[474].back_sidedef.is_none());
        assert!(linedefs[466].back_sidedef.is_some());

        // Flag check
        assert_eq!(linedefs[26].flags, 29);
    }

    #[test]
    fn check_e1m1_sectors() {
        let mut wad = Wad::new("../doom1.wad");
        wad.read_directories();

        let mut map = map::Map::new("E1M1".to_owned());
        map.load(&wad);

        let sectors = map.get_sectors();
        assert_eq!(sectors[0].floor_height, 0);
        assert_eq!(sectors[0].ceil_height, 72);
        assert_eq!(sectors[0].floor_tex, "FLOOR4_8");
        assert_eq!(sectors[0].ceil_tex, "CEIL3_5");
        assert_eq!(sectors[0].light_level, 160);
        assert_eq!(sectors[0].kind, 0);
        assert_eq!(sectors[0].tag, 0);
        assert_eq!(sectors[84].floor_height, -24);
        assert_eq!(sectors[84].ceil_height, 48);
        assert_eq!(sectors[84].floor_tex, "FLOOR5_2");
        assert_eq!(sectors[84].ceil_tex, "CEIL3_5");
        assert_eq!(sectors[84].light_level, 255);
        assert_eq!(sectors[84].kind, 0);
        assert_eq!(sectors[84].tag, 0);
    }

    #[test]
    fn check_e1m1_sidedefs() {
        let mut wad = Wad::new("../doom1.wad");
        wad.read_directories();

        let mut map = map::Map::new("E1M1".to_owned());
        map.load(&wad);

        let sidedefs = map.get_sidedefs();
        assert_eq!(sidedefs[0].x_offset, 0);
        assert_eq!(sidedefs[0].y_offset, 0);
        assert_eq!(sidedefs[0].middle_tex, "DOOR3");
        assert_eq!(sidedefs[0].sector.floor_tex, "FLOOR4_8");
        assert_eq!(sidedefs[9].x_offset, 0);
        assert_eq!(sidedefs[9].y_offset, 48);
        assert_eq!(sidedefs[9].middle_tex, "BROWN1");
        assert_eq!(sidedefs[9].sector.floor_tex, "FLOOR4_8");
        assert_eq!(sidedefs[647].x_offset, 4);
        assert_eq!(sidedefs[647].y_offset, 0);
        assert_eq!(sidedefs[647].middle_tex, "SUPPORT2");
        assert_eq!(sidedefs[647].sector.floor_tex, "FLOOR4_8");
    }

    #[test]
    fn check_e1m1_segments() {
        let mut wad = Wad::new("../doom1.wad");
        wad.read_directories();

        let mut map = map::Map::new("E1M1".to_owned());
        map.load(&wad);

        let segments = map.get_segments();
        assert_eq!(segments[0].start_vertex.x() as i32, 1552);
        assert_eq!(segments[0].end_vertex.x() as i32, 1552);
        assert_eq!(segments[731].start_vertex.x() as i32, 3040);
        assert_eq!(segments[731].end_vertex.x() as i32, 2976);
        assert_eq!(segments[0].angle, 16384.0);
        assert_eq!(segments[0].linedef.front_sidedef.upper_tex, "BIGDOOR2");
        assert_eq!(segments[0].direction, 0);
        assert_eq!(segments[0].offset, 0);

        assert_eq!(segments[731].angle, 32768.0);
        assert_eq!(segments[731].linedef.front_sidedef.upper_tex, "STARTAN1");
        assert_eq!(segments[731].direction, 1);
        assert_eq!(segments[731].offset, 0);

        let subsectors = map.get_subsectors();
        assert_eq!(subsectors[0].seg_count, 4);
        assert_eq!(subsectors[124].seg_count, 3);
        assert_eq!(subsectors[236].seg_count, 4);
        //assert_eq!(subsectors[0].start_seg.start_vertex.x() as i32, 1552);
        //assert_eq!(subsectors[124].start_seg.start_vertex.x() as i32, 472);
        //assert_eq!(subsectors[236].start_seg.start_vertex.x() as i32, 3040);
    }
}
