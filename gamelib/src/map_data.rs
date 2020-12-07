use std::str;

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
/// # use gamelib::r_bsp::RenderData;
/// # let mut wad = Wad::new("../doom1.wad");
/// # wad.read_directories();
/// # let mut map = RenderData::new("E1M1".to_owned());
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
/// # use gamelib::r_bsp::RenderData;
/// # let mut wad = Wad::new("../doom1.wad");
/// # wad.read_directories();
/// # let mut map = RenderData::new("E1M1".to_owned());
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
pub struct MapData {
    name:       String,
    /// Things will be linked to/from each other in many ways, which means this array may
    /// never be resized or it will invalidate references and pointers
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
}

impl MapData {
    pub fn new(name: String) -> MapData {
        MapData {
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
        }
    }

    #[inline]
    pub fn get_name(&self) -> &str { &self.name }

    #[inline]
    pub fn get_things(&self) -> &[Thing] { &self.things }

    #[inline]
    pub(crate) fn set_extents(&mut self) {
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
    pub fn get_vertexes(&self) -> &[Vertex] { &self.vertexes }

    #[inline]
    pub fn get_linedefs(&self) -> &[LineDef] { &self.linedefs }

    #[inline]
    pub fn get_sectors(&self) -> &[Sector] { &self.sectors }

    #[inline]
    pub fn get_sidedefs(&self) -> &[SideDef] { &self.sidedefs }

    #[inline]
    pub fn get_subsectors(&self) -> &[SubSector] { &self.subsectors }

    #[inline]
    pub fn get_segments(&self) -> &[Segment] { &self.segments }

    fn set_scale(&mut self) {
        let map_width = self.extents.width as f32;
        let map_height = self.extents.height as f32;

        if map_height > map_width {
            self.extents.automap_scale = map_height / 200.0 * 1.1;
        } else {
            self.extents.automap_scale = map_width / 320.0 * 1.4;
        }
    }

    #[inline]
    pub fn get_nodes(&self) -> &[Node] { &self.nodes }

    #[inline]
    pub fn start_node(&self) -> u16 { self.start_node }

    #[inline]
    pub fn get_map_extents(&self) -> &MapExtents { &self.extents }

    pub fn load<'m>(&mut self, wad: &Wad) {
        let index = wad
            .find_lump_index(self.get_name())
            .expect(&format!("Could not find {}", self.get_name()));
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
                // SHOULD ALSO HAVE A SIDEDEF LINK
                let direction = wad.read_2_bytes(offset + 8);
                let sidedef = if direction == 0 {
                    linedef.front_sidedef.clone()
                } else {
                    // Safe as this is not possible. If there is no back sidedef
                    // then it defaults to the front
                    linedef.back_sidedef.as_ref().unwrap().clone()
                };
                Segment::new(
                    DPtr::new(start_vertex),
                    DPtr::new(end_vertex),
                    ((wad.read_2_bytes(offset + 4) as u32) << 16) as f32
                        * 8.38190317e-8,
                    DPtr::new(linedef),
                    sidedef,
                    direction, // 0 front or 1 back
                    wad.read_2_bytes(offset + 10),
                )
            });
        // SSECTORS
        self.subsectors =
            wad.read_lump_to_vec(index, LumpIndex::SubSectors, 4, |offset| {
                let start_seg = wad.read_2_bytes(offset + 2);
                let sector = self.get_segments()[start_seg as usize]
                    .sidedef
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
        self.set_scale();
    }

    /// R_PointInSubsector - r_main
    pub(crate) fn point_in_subsector(
        &self,
        point: &Vertex,
    ) -> Option<DPtr<SubSector>> {
        let mut node_id = self.start_node();
        let mut node;
        let mut side;

        while node_id & IS_SSECTOR_MASK == 0 {
            node = &self.get_nodes()[node_id as usize];
            side = node.point_on_side(&point);
            node_id = node.child_index[side];
        }

        return Some(DPtr::new(
            &self.get_subsectors()[(node_id ^ IS_SSECTOR_MASK) as usize],
        ));
    }
}
