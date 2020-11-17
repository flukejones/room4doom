use glam::Vec2;
use sdl2::{render::Canvas, surface::Surface};
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};
use std::str;

use crate::{angle::Angle, player::Player};

use wad::{lumps::*, DPtr, LumpIndex, Vertex, Wad};

const MAX_SEGS: usize = 32;

#[derive(Debug, Copy, Clone)]
struct ClipRange {
    first: i32,
    last:  i32,
}

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
/// # use gamelib::bsp::Bsp;
/// # let mut wad = Wad::new("../doom1.wad");
/// # wad.read_directories();
/// # let mut map = Bsp::new("E1M1".to_owned());
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
/// # use gamelib::bsp::Bsp;
/// # let mut wad = Wad::new("../doom1.wad");
/// # wad.read_directories();
/// # let mut map = Bsp::new("E1M1".to_owned());
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
pub struct Bsp {
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
    // put below in new struct
    solidsegs:  Vec<ClipRange>,
    /// index in to self.solidsegs
    new_end:    usize,
    rw_angle1:  Angle,
}

impl Bsp {
    pub fn new(name: String) -> Bsp {
        Bsp {
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
            //
            solidsegs: Vec::with_capacity(MAX_SEGS),
            new_end: 0,
            rw_angle1: Angle::new(0.0),
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
    pub fn get_nodes(&self) -> &[Node] {
        &self.nodes
    }

    #[inline]
    pub fn start_node(&self) -> u16 {
        self.start_node
    }

    #[inline]
    pub fn get_rw_angle1(&self) -> Angle {
        self.rw_angle1
    }

    #[inline]
    pub fn get_map_extents(&self) -> &MapExtents {
        &self.extents
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
    pub fn find_subsector(&self, point: &Vertex) -> Option<&SubSector> {
        let mut node_id = self.start_node();
        let mut node;
        let mut side;

        while node_id & IS_SSECTOR_MASK != IS_SSECTOR_MASK {
            node = &self.get_nodes()[node_id as usize];
            side = node.point_on_side(&point);
            node_id = node.child_index[side];
        }

        return Some(
            &self.get_subsectors()[(node_id ^ IS_SSECTOR_MASK) as usize],
        );
    }

    /// R_AddLine - r_bsp
    fn add_line<'a>(
        &'a mut self,
        object: &Player,
        seg: &'a Segment,
        canvas: &mut Canvas<Surface>,
    ) {
        // reject orthogonal back sides
        if !seg.is_facing_point(&object.xy) {
            return;
        }

        let clipangle = Angle::new(FRAC_PI_4);
        // Reset to correct angles
        let mut angle1 = vertex_angle_to_object(&seg.start_vertex, object);
        let mut angle2 = vertex_angle_to_object(&seg.end_vertex, object);

        let span = angle1 - angle2;

        if span.rad() >= PI {
            return;
        }

        // Global angle needed by segcalc.
        self.rw_angle1 = angle1;

        angle1 -= object.rotation;
        angle2 -= object.rotation;

        let mut tspan = angle1 + clipangle;
        if tspan.rad() >= FRAC_PI_2 {
            tspan -= 2.0 * clipangle.rad();

            // Totally off the left edge?
            if tspan.rad() >= span.rad() {
                return;
            }
            angle1 = clipangle;
        }
        tspan = clipangle - angle2;
        if tspan.rad() > 2.0 * clipangle.rad() {
            tspan -= 2.0 * clipangle.rad();

            // Totally off the left edge?
            if tspan.rad() >= span.rad() {
                return;
            }
            angle2 = -clipangle;
        }

        angle1 += FRAC_PI_2;
        angle2 += FRAC_PI_2;
        let x1 = angle_to_screen(angle1.rad()); // this or vertex_angle_to_object incorrect
        let x2 = angle_to_screen(angle2.rad());

        // Does not cross a pixel?
        if x1 == x2 {
            return;
        }

        if let Some(back_sector) = &seg.linedef.back_sidedef {
            let front_sector = &seg.linedef.front_sidedef.sector;
            let back_sector = &back_sector.sector;

            // Doors. Block view
            if back_sector.ceil_height <= front_sector.floor_height
                || back_sector.floor_height >= front_sector.ceil_height
            {
                self.clip_solid_seg(x1, x2 - 1, object, seg, canvas);
                return;
            }

            // Windows usually, but also changes in heights from sectors eg: steps
            if back_sector.ceil_height != front_sector.ceil_height
                || back_sector.floor_height != front_sector.floor_height
            {
                // TODO: clip-pass
                //self.clip_portal_seg(x1, x2 - 1, object, seg, canvas);
                return;
            }

            // Reject empty lines used for triggers and special events.
            // Identical floor and ceiling on both sides, identical light levels
            // on both sides, and no middle texture.
            if back_sector.ceil_tex == front_sector.ceil_tex
                && back_sector.floor_tex == front_sector.floor_tex
                && back_sector.light_level == front_sector.light_level
                && seg.linedef.front_sidedef.middle_tex.is_empty()
            {
                return;
            }
        }
        self.clip_solid_seg(x1, x2 - 1, object, seg, canvas);
    }

    /// R_Subsector - r_bsp
    fn draw_subsector<'a>(
        &'a mut self,
        object: &Player,
        subsect: &SubSector,
        canvas: &mut Canvas<Surface>,
    ) {
        // TODO: planes for floor & ceiling
        for i in subsect.start_seg..subsect.start_seg + subsect.seg_count {
            let seg = self.get_segments()[i as usize].clone();
            self.add_line(object, &seg, canvas);
        }
    }

    /// R_ClearClipSegs - r_bsp
    pub fn clear_clip_segs(&mut self) {
        self.solidsegs.clear();
        self.solidsegs.push(ClipRange {
            first: -0x7fffffff,
            last:  -1,
        });
        for _ in 0..MAX_SEGS {
            self.solidsegs.push(ClipRange {
                first: 320,
                last:  0x7fffffff,
            });
        }
        self.new_end = 2;
    }

    /// R_ClipSolidWallSegment - r_bsp
    fn clip_solid_seg(
        &mut self,
        first: i32,
        last: i32,
        object: &Player,
        seg: &Segment,
        canvas: &mut Canvas<Surface>,
    ) {
        let mut next;

        // Find the first range that touches the range
        //  (adjacent pixels are touching).
        let mut start = 0; // first index
        while self.solidsegs[start].last < first - 1 {
            start += 1;
        }

        if first < self.solidsegs[start].first {
            if last < self.solidsegs[start].first - 1 {
                // Post is entirely visible (above start),
                // so insert a new clippost.
                self.store_wall_range(first, last, seg, object, canvas);

                next = self.new_end;
                self.new_end += 1;

                while next != 0 && next != start {
                    self.solidsegs[next] = self.solidsegs[next - 1];
                    next -= 1;
                }

                self.solidsegs[next].first = first;
                self.solidsegs[next].last = last;
                return;
            }

            // There is a fragment above *start.
            self.store_wall_range(
                first,
                self.solidsegs[start].first - 1,
                seg,
                object,
                canvas,
            );
            // Now adjust the clip size.
            self.solidsegs[start].first = first;
        }

        // Bottom contained in start?
        if last <= self.solidsegs[start].last {
            return;
        }

        next = start;
        while last >= self.solidsegs[next + 1].first - 1
            && next + 1 < self.solidsegs.len() - 1
        {
            self.store_wall_range(
                self.solidsegs[next].last + 1,
                self.solidsegs[next + 1].first - 1,
                seg,
                object,
                canvas,
            );

            next += 1;

            if last <= self.solidsegs[next].last {
                self.solidsegs[start].last = self.solidsegs[next].last;
                return self.crunch(start, next);
            }
        }

        // There is a fragment after *next.
        self.store_wall_range(
            self.solidsegs[next].last + 1,
            last,
            seg,
            object,
            canvas,
        );
        // Adjust the clip size.
        self.solidsegs[start].last = last;

        //crunch
        self.crunch(start, next);
    }

    /// R_ClipPassWallSegment - r_bsp
    fn clip_portal_seg(
        &mut self,
        first: i32,
        last: i32,
        object: &Player,
        seg: &Segment,
        canvas: &mut Canvas<Surface>,
    ) {
        let mut next;

        // Find the first range that touches the range
        //  (adjacent pixels are touching).
        let mut start = 0; // first index
        while self.solidsegs[start].last < first - 1 {
            start += 1;
        }

        if first < self.solidsegs[start].first {
            if last < self.solidsegs[start].first - 1 {
                // Post is entirely visible (above start),
                self.store_wall_range(first, last, seg, object, canvas);
                return;
            }

            // There is a fragment above *start.
            self.store_wall_range(
                first,
                self.solidsegs[start].first - 1,
                seg,
                object,
                canvas,
            );
        }

        // Bottom contained in start?
        if last <= self.solidsegs[start].last {
            return;
        }

        next = start;
        while last >= self.solidsegs[next + 1].first - 1
            && next + 1 < self.solidsegs.len() - 1
        {
            self.store_wall_range(
                self.solidsegs[next].last + 1,
                self.solidsegs[next + 1].first - 1,
                seg,
                object,
                canvas,
            );

            next += 1;

            if last <= self.solidsegs[next].last {
                return;
            }
        }

        // There is a fragment after *next.
        self.store_wall_range(
            self.solidsegs[next].last + 1,
            last,
            seg,
            object,
            canvas,
        );
    }

    fn crunch(&mut self, mut start: usize, mut next: usize) {
        {
            if next == start {
                return;
            }

            while (next + 1) != self.new_end && start < self.solidsegs.len() - 1
            {
                next += 1;
                start += 1;
                self.solidsegs[start] = self.solidsegs[next];
            }
            self.new_end = start + 1;
        }
    }

    /// R_RenderBSPNode - r_bsp
    pub fn draw_bsp<'a>(
        &'a mut self,
        object: &Player,
        node_id: u16,
        canvas: &mut Canvas<Surface>,
    ) {
        if node_id & IS_SSECTOR_MASK == IS_SSECTOR_MASK {
            // It's a leaf node and is the index to a subsector
            let subsect = &self.get_subsectors()
                [(node_id ^ IS_SSECTOR_MASK) as usize]
                .clone();
            // Check if it should be drawn, then draw
            self.draw_subsector(object, &subsect, canvas);
            return;
        }

        // otherwise get node
        let node = &self.nodes[node_id as usize].clone();
        // find which side the point is on
        let side = node.point_on_side(&object.xy);
        // Recursively divide front space.
        self.draw_bsp(object, node.child_index[side], canvas);

        // Possibly divide back space.
        // check if each corner of the BB is in the FOV
        //if node.point_in_bounds(&v, side ^ 1) {
        if node.bb_extents_in_fov(
            &object.xy,
            object.rotation.rad(),
            FRAC_PI_4,
            side ^ 1,
        ) {
            self.draw_bsp(object, node.child_index[side ^ 1], canvas);
        }
    }
}

/// ANgle must be within 90d range
fn angle_to_screen(mut radian: f32) -> i32 {
    let mut x;

    // Left side
    let p = 160.0 / (FRAC_PI_4).tan();
    if radian > FRAC_PI_2 {
        radian -= FRAC_PI_2;
        let t = radian.tan();
        x = t * p;
        x = p - x;
    } else {
        // Right side
        radian = (FRAC_PI_2) - radian;
        let t = radian.tan();
        x = t * p;
        x += p;
    }
    x as i32
}

/// R_PointToAngle
// To get a global angle from cartesian coordinates,
//  the coordinates are flipped until they are in
//  the first octant of the coordinate system, then
//  the y (<=x) is scaled and divided by x to get a
//  tangent (slope) value which is looked up in the
//  tantoangle[] table.
///
/// The flipping isn't done here...
pub fn vertex_angle_to_object(vertex: &Vec2, object: &Player) -> Angle {
    let x = vertex.x() - object.xy.x();
    let y = vertex.y() - object.xy.y();
    Angle::new(y.atan2(x))

    // if x >= 0.0 {
    //     if y >= 0.0 {
    //         if x > y {
    //             // octant 0
    //             return (y / x).tan();
    //         } else {
    //             // octant 1
    //             return (FRAC_PI_2) - 1.0 - (x/y).tan();
    //         }
    //     } else {
    //         // y<0
    //         y = -y;
    //         if x > y {
    //             // octant 8
    //             return -(y/x).tan();
    //         } else {
    //             // octant 7
    //             return (PI + PI/2.0) + (x/y).tan();
    //         }
    //     }
    // } else {
    //     x = -x;
    //     if y >= 0.0 {
    //         if x > y {
    //             // octant 3
    //             return PI - 1.0 - (y/x).tan();
    //         } else {
    //             // octant 2
    //             return (FRAC_PI_2) + (x/y).tan();
    //         }
    //     } else {
    //         y = -y;
    //         if x > y {
    //             // octant 4
    //             return PI + (y/x).tan();
    //         } else {
    //             // octant 5
    //             return  (PI + PI/2.0) - 1.0 - (x/y).tan();
    //         }
    //     }
    // }
}

#[cfg(test)]
mod tests {
    use crate::bsp;
    use crate::bsp::IS_SSECTOR_MASK;
    use wad::{Vertex, Wad};

    #[test]
    fn check_e1m1_things() {
        let mut wad = Wad::new("../doom1.wad");
        wad.read_directories();

        let mut map = bsp::Bsp::new("E1M1".to_owned());
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

        let mut map = bsp::Bsp::new("E1M1".to_owned());
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

        let mut map = bsp::Bsp::new("E1M1".to_owned());
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

        let mut map = bsp::Bsp::new("E1M1".to_owned());
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

        let mut map = bsp::Bsp::new("E1M1".to_owned());
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

        let mut map = bsp::Bsp::new("E1M1".to_owned());
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

        let mut map = bsp::Bsp::new("E1M1".to_owned());
        map.load(&wad);

        let segments = map.get_segments();
        assert_eq!(segments[0].start_vertex.x() as i32, 1552);
        assert_eq!(segments[0].end_vertex.x() as i32, 1552);
        assert_eq!(segments[731].start_vertex.x() as i32, 3040);
        assert_eq!(segments[731].end_vertex.x() as i32, 2976);
        assert_eq!(segments[0].angle, 90.0);
        assert_eq!(segments[0].linedef.front_sidedef.upper_tex, "BIGDOOR2");
        assert_eq!(segments[0].direction, 0);
        assert_eq!(segments[0].offset, 0);

        assert_eq!(segments[731].angle, 180.0);
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

    #[test]
    fn check_nodes_of_e1m1() {
        let mut wad = Wad::new("../doom1.wad");
        wad.read_directories();

        let mut map = bsp::Bsp::new("E1M1".to_owned());
        map.load(&wad);

        let nodes = map.get_nodes();
        assert_eq!(nodes[0].split_start.x() as i32, 1552);
        assert_eq!(nodes[0].split_start.y() as i32, -2432);
        assert_eq!(nodes[0].split_delta.x() as i32, 112);
        assert_eq!(nodes[0].split_delta.y() as i32, 0);

        assert_eq!(nodes[0].bounding_boxes[0][0].x() as i32, 1552); //top
        assert_eq!(nodes[0].bounding_boxes[0][0].y() as i32, -2432); //bottom

        assert_eq!(nodes[0].bounding_boxes[1][0].x() as i32, 1600);
        assert_eq!(nodes[0].bounding_boxes[1][0].y() as i32, -2048);

        assert_eq!(nodes[0].child_index[0], 32768);
        assert_eq!(nodes[0].child_index[1], 32769);
        assert_eq!(IS_SSECTOR_MASK, 0x8000);

        assert_eq!(nodes[235].split_start.x() as i32, 2176);
        assert_eq!(nodes[235].split_start.y() as i32, -3776);
        assert_eq!(nodes[235].split_delta.x() as i32, 0);
        assert_eq!(nodes[235].split_delta.y() as i32, -32);
        assert_eq!(nodes[235].child_index[0], 128);
        assert_eq!(nodes[235].child_index[1], 234);

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

        let mut map = bsp::Bsp::new("E1M1".to_owned());
        map.load(&wad);

        // The actual location of THING0
        let player = Vertex::new(1056.0, -3616.0);
        let subsector = map.find_subsector(&player).unwrap();
        //assert_eq!(subsector_id, Some(103));
        assert_eq!(subsector.seg_count, 5);
        assert_eq!(subsector.start_seg, 305);
    }
}
