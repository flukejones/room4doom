#[cfg(feature = "hprof")]
use coarse_prof::profile;

use crate::flags::LineDefFlags;
use crate::map_defs::{LineDef, Node, Sector, Segment, SubSector};
use crate::triangulation::{DivLine, carve_subsector_polygon};
use glam::{Vec2, Vec3};
use std::collections::HashMap;

const IS_SUBSECTOR_MASK: u32 = 0x8000_0000;
const QUANT_EPSILON: f32 = 0.1;
const HEIGHT_EPSILON: f32 = 0.1;

#[derive(Default, Debug, Clone, Copy, PartialEq)]
pub enum MovementType {
    Floor,
    Ceiling,
    #[default]
    None,
}

/// Per-sector sets of 2D positions that need separated vertex mappings.
/// Only vertices on zero-height wall boundaries where the sector is the
/// backsector need LowerSeparated/UpperSeparated. All other vertices use
/// Lower/Upper as normal.
#[derive(Debug, Clone, Default)]
struct ZeroHeightSectorVerts {
    lower: Vec<QuantizedVec2>,
    upper: Vec<QuantizedVec2>,
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
struct QuantizedVec2 {
    x: i32,
    y: i32,
}

impl QuantizedVec2 {
    fn from_vec2(v: Vec2, precision: f32) -> Self {
        Self {
            x: (v.x / precision).round() as i32,
            y: (v.y / precision).round() as i32,
        }
    }
}

/// Pre-pass over segments to find the specific vertex positions where each
/// sector participates in a zero-height wall. Both frontsector and backsector
/// are recorded, since both sides need LowerSeparated(sector_id) /
/// UpperSeparated(sector_id) to get unique vertex indices.
fn build_zero_height_wall_map(segments: &[Segment]) -> HashMap<usize, ZeroHeightSectorVerts> {
    let mut map: HashMap<usize, ZeroHeightSectorVerts> = HashMap::new();

    for seg in segments {
        let back = match &seg.backsector {
            Some(b) => b,
            None => continue,
        };

        let front_num = seg.frontsector.num as usize;
        let back_num = back.num as usize;
        let v1 = QuantizedVec2::from_vec2(*seg.v1, QUANT_EPSILON);
        let v2 = QuantizedVec2::from_vec2(*seg.v2, QUANT_EPSILON);

        // Lower wall: both sectors' floor vertices at segment endpoints
        // need LowerSeparated(sector_id) to share with their respective
        // wall verts (bottom for front, top for back).
        if seg.sidedef.bottomtexture.is_some()
            && (seg.frontsector.floorheight - back.floorheight).abs() <= HEIGHT_EPSILON
        {
            for &sector_num in &[front_num, back_num] {
                let entry = map.entry(sector_num).or_default();
                if !entry.lower.contains(&v1) {
                    entry.lower.push(v1);
                }
                if !entry.lower.contains(&v2) {
                    entry.lower.push(v2);
                }
            }
        }

        // Upper wall: both sectors' ceiling vertices at segment endpoints
        // need UpperSeparated(sector_id).
        if seg.sidedef.toptexture.is_some()
            && (seg.frontsector.ceilingheight - back.ceilingheight).abs() <= HEIGHT_EPSILON
        {
            for &sector_num in &[front_num, back_num] {
                let entry = map.entry(sector_num).or_default();
                if !entry.upper.contains(&v1) {
                    entry.upper.push(v1);
                }
                if !entry.upper.contains(&v2) {
                    entry.upper.push(v2);
                }
            }
        }
    }

    map
}

/// Check if a sector will have floor/ceiling movement at runtime.
/// A sector is a mover if:
/// - It has a non-zero tag and any linedef in the map targets that tag with a
///   special
/// - It is the backsector of a linedef that has a non-zero special (e.g. manual
///   doors)
pub fn is_sector_mover(sector: &Sector, linedefs: &[LineDef]) -> bool {
    if sector.tag != 0 {
        for ld in linedefs {
            if ld.tag == sector.tag && ld.special != 0 {
                return true;
            }
        }
    }
    for line in &sector.lines {
        if line.special != 0 {
            if let Some(ref back) = line.backsector {
                if back.num == sector.num {
                    return true;
                }
            }
        }
    }
    false
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WallType {
    Upper,
    Lower,
    Middle,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WallTexPin {
    UnpegTop = 0b0001,
    UnpegBottom = 0b0010,
    UnpegBoth = 0b0011,
    None = 0b1000,
}

impl From<LineDefFlags> for WallTexPin {
    fn from(flags: LineDefFlags) -> Self {
        if flags.contains(LineDefFlags::UnpegBottom) && flags.contains(LineDefFlags::UnpegTop) {
            WallTexPin::UnpegBoth
        } else if flags.contains(LineDefFlags::UnpegBottom) {
            WallTexPin::UnpegBottom
        } else if flags.contains(LineDefFlags::UnpegTop) {
            WallTexPin::UnpegTop
        } else {
            WallTexPin::None
        }
    }
}

#[derive(Debug, Clone)]
pub struct Node3D {
    pub xy: Vec2,
    pub delta: Vec2,
    pub bboxes: [AABB; 2],
    pub children: [u32; 2],
    pub aabb: AABB,
}

impl Node3D {
    pub const fn point_on_side(&self, point: Vec2) -> usize {
        let dx = point.x - self.xy.x;
        let dy = point.y - self.xy.y;
        if (self.delta.y * dx) > (dy * self.delta.x) {
            0
        } else {
            1
        }
    }

    /// Returns (front_child_id, back_child_id) for the given point.
    /// Front is the child on the same side as the point (closer).
    pub fn front_back_children(&self, point: Vec2) -> (u32, u32) {
        let side = self.point_on_side(point);
        (self.children[side], self.children[side ^ 1])
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AABB {
    pub min: Vec3,
    pub max: Vec3,
}

impl AABB {
    fn new() -> Self {
        Self {
            min: Vec3::new(f32::MAX, f32::MAX, f32::MAX),
            max: Vec3::new(f32::MIN, f32::MIN, f32::MIN),
        }
    }

    fn expand_to_include_point(&mut self, point: Vec3) {
        self.min = self.min.min(point);
        self.max = self.max.max(point);
    }

    fn expand_to_include_aabb(&mut self, other: &AABB) {
        self.min = self.min.min(other.min);
        self.max = self.max.max(other.max);
    }
}

impl From<&[Vec3; 2]> for AABB {
    fn from(bbox: &[Vec3; 2]) -> Self {
        Self {
            min: bbox[0],
            max: bbox[1],
        }
    }
}

/// Original 2D segment data retained for screen-space occlusion testing.
/// Stores sector IDs rather than baked heights so that live sector heights
/// (doors/lifts) are used at render time.
#[derive(Debug, Clone)]
pub struct OcclusionSeg {
    pub v1: Vec2,
    pub v2: Vec2,
    pub front_sector_id: usize,
    /// None = one-sided (fully solid). Some = two-sided portal whose
    /// opening is between max(front_floor, back_floor) and
    /// min(front_ceil, back_ceil).
    pub back_sector_id: Option<usize>,
    /// Precomputed atan2(v2.y - v1.y, v2.x - v1.x) — seg direction angle.
    pub seg_angle_rad: f32,
}

#[derive(Clone)]
pub struct BSPLeaf3D {
    pub polygons: Vec<SurfacePolygon>,
    pub aabb: AABB,
    pub floor_polygons: Vec<usize>,
    pub ceiling_polygons: Vec<usize>,
    pub sector_id: usize,
    pub occlusion_segs: Vec<OcclusionSeg>,
}

#[derive(Debug, Clone)]
pub enum SurfaceKind {
    Vertical {
        texture: Option<usize>,
        tex_x_offset: f32,
        tex_y_offset: f32,
        texture_direction: Vec3,
        wall_type: WallType,
        wall_tex_pin: WallTexPin,
        /// For texture alignment
        front_ceiling_z: f32,
        two_sided: bool,
        /// Index of the linedef this wall was created from; used to update the
        /// texture when a switch fires.
        linedef_id: usize,
    },
    Horizontal {
        texture: usize,
        tex_cos: f32,
        tex_sin: f32,
    },
}

#[derive(Debug, Clone)]
pub struct SurfacePolygon {
    pub sector_id: usize,
    pub surface_kind: SurfaceKind,
    pub vertices: Vec<usize>,
    pub normal: Vec3,
    pub aabb: AABB,
    pub moves: bool,
}

impl SurfacePolygon {
    fn new(
        sector_id: usize,
        surface_kind: SurfaceKind,
        vertices: Vec<usize>,
        normal: Vec3,
        vertex_positions: &[Vec3],
        moves: bool,
    ) -> Self {
        let mut aabb = AABB::new();
        for &vertex_idx in &vertices {
            aabb.expand_to_include_point(vertex_positions[vertex_idx]);
        }

        Self {
            sector_id,
            surface_kind,
            vertices,
            normal,
            aabb,
            moves,
        }
    }

    pub fn is_facing_point(&self, point: Vec3, vertex_positions: &[Vec3]) -> bool {
        let computed_normal = if self.moves {
            unsafe {
                let p0 = vertex_positions.get_unchecked(self.vertices[0]);
                let p1 = vertex_positions.get_unchecked(self.vertices[1]);
                let p2 = vertex_positions.get_unchecked(self.vertices[2]);
                let edge1 = *p1 - *p0;
                let edge2 = *p2 - *p0;
                let cross = edge1.cross(edge2);
                if cross.length_squared() > f32::EPSILON {
                    cross.normalize()
                } else {
                    self.normal
                }
            }
        } else {
            self.normal
        };

        let first_vertex_idx = unsafe { *self.vertices.get_unchecked(0) };
        let first_vertex = vertex_positions[first_vertex_idx];
        let view_vector = (point - first_vertex).normalize_or_zero();
        let dot_product = computed_normal.dot(view_vector);
        dot_product.is_sign_positive() || dot_product.is_nan()
    }
}

/// Bit-exact Vec3 key for vertex deduplication. Two vertices share an index
/// only if their float coordinates are identical, avoiding false merges.
#[derive(Hash, PartialEq, Eq, Clone, Copy)]
struct QuantizedVec3 {
    x: i32,
    y: i32,
    z: i32,
}

impl QuantizedVec3 {
    fn from_vec3(v: Vec3, precision: f32) -> Self {
        Self {
            y: (v.y / precision).round() as i32,
            z: (v.z / precision).round() as i32,
            x: (v.x / precision).round() as i32,
        }
    }
}

/// Which wall type is the vertex for. When adding walls we have to check
/// if the vertex is allowed to be used for the required position if
/// the wall height is zero. This is because it's impossible to know which
/// vertex in the same position is for what.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
enum VertexMappedTo {
    /// Floor polygons (default), lower wall bottom verts, mid wall bottom verts
    Lower,
    /// Ceiling polygons (default), upper wall top verts, mid wall top verts
    Upper,
    /// Floor vertex at a zero-height lower wall boundary, tagged by sector ID.
    /// Multiple sectors meeting at the same (x, y, z) each get a unique vertex
    /// index because the sector ID differentiates them in the hash key.
    LowerSeparated(usize),
    /// Ceiling vertex at a zero-height upper wall boundary, tagged by sector
    /// ID.
    UpperSeparated(usize),
    /// The vertex hasn't been assigned to anything
    Unused,
}

/// Track what vertexes are for, and where to find them
struct VertexTracking {
    /// Track what the vertex was used for, this determines what other walls are
    /// allowed to use which matters if there are more than one in a 3D
    /// position such as for zero-height walls. The index used to address
    /// this array is the same as that in global vertex array, which
    /// means both insert operations must be synced
    vertex_type: Vec<VertexMappedTo>,
    /// Tracking to enable faster vertex lookups instead of always iterating.
    vertex_map: HashMap<(QuantizedVec3, VertexMappedTo), usize>,
    precision: f32,
}

impl VertexTracking {
    fn new(segment_count: usize) -> Self {
        Self {
            vertex_type: Vec::with_capacity(segment_count * 2),
            vertex_map: HashMap::with_capacity(segment_count * 2),
            precision: 2.0,
        }
    }

    /// Get the index number in global array of the vertex the mapping is
    /// allowed to use. Returns None if no vertex exists.
    /// Function can be used to check if contains vertex.
    fn get_vertex_index(&mut self, vertex: Vec3, mapping: VertexMappedTo) -> Option<usize> {
        let quantized = QuantizedVec3::from_vec3(vertex, self.precision);
        self.vertex_map.get(&(quantized, mapping)).copied()
    }

    /// Insert the vertex mapping. Creates a quick lookup.
    fn insert_vertex(&mut self, vertex: Vec3, mapping: VertexMappedTo, index: usize) {
        let quantized = QuantizedVec3::from_vec3(vertex, self.precision);
        self.vertex_map.insert((quantized, mapping), index);
        if index >= self.vertex_type.len() {
            self.vertex_type.resize(index + 1, VertexMappedTo::Unused);
        }
        self.vertex_type[index] = mapping;
    }
}

pub struct BSP3D {
    nodes: Vec<Node3D>,
    pub subsector_leaves: Vec<BSPLeaf3D>,
    root_node: u32,
    pub vertices: Vec<Vec3>,
    pub sector_subsectors: Vec<Vec<usize>>,
    /// Carved 2D convex polygon for each subsector, indexed by subsector ID.
    /// Empty vec for degenerate subsectors that produce no valid polygon.
    pub carved_polygons: Vec<Vec<Vec2>>,
    /// Maps linedef_id → [(subsector_id, polygon_idx)] for wall texture
    /// updates.
    linedef_wall_polygons: HashMap<usize, Vec<(usize, usize)>>,
}

impl BSP3D {
    pub fn new(
        root_node: u32,
        nodes: &[Node],
        subsectors: &[SubSector],
        segments: &[Segment],
        sectors: &[Sector],
        linedefs: &[LineDef],
    ) -> Self {
        #[cfg(feature = "hprof")]
        profile!("BSP3D::new");

        let mut bsp3d = Self {
            nodes: Vec::new(),
            subsector_leaves: Vec::new(),
            root_node,
            vertices: Vec::new(),
            sector_subsectors: vec![Vec::new(); sectors.len()],
            carved_polygons: vec![Vec::new(); subsectors.len()],
            linedef_wall_polygons: HashMap::new(),
        };

        let zero_height_wall_map = build_zero_height_wall_map(segments);
        let mut vertex_tracking = VertexTracking::new(segments.len());

        bsp3d.initialize_nodes(nodes, sectors);
        bsp3d.initialize_subsectors(subsectors);
        bsp3d.build_sector_subsector_mapping(subsectors, segments, sectors);
        // Initialize subsector leaves for wall and floor/ceiling storage
        bsp3d.subsector_leaves = vec![BSPLeaf3D::default(); subsectors.len()];

        // Use BSP traversal to generate walls, floors, and ceiling polygons.
        bsp3d.carve_polygons_recursive(
            nodes,
            subsectors,
            segments,
            linedefs,
            bsp3d.root_node,
            Vec::new(),
            &zero_height_wall_map,
            &mut vertex_tracking,
        );

        bsp3d.update_all_aabbs();
        bsp3d.expand_node_aabbs_for_movers(sectors, linedefs);
        bsp3d.build_linedef_wall_map();

        bsp3d
    }

    pub fn nodes(&self) -> &[Node3D] {
        &self.nodes
    }

    pub fn get_subsector_leaf(&self, subsector_id: usize) -> Option<&BSPLeaf3D> {
        self.subsector_leaves.get(subsector_id)
    }

    pub fn get_subsector_leaf_count(&self) -> usize {
        self.subsector_leaves.len()
    }

    pub fn root_node(&self) -> u32 {
        self.root_node
    }

    fn vertex_add(
        &mut self,
        vertex: Vec3,
        mapping: VertexMappedTo,
        vertex_tracking: &mut VertexTracking,
    ) -> usize {
        if let Some(existing_idx) = vertex_tracking.get_vertex_index(vertex, mapping) {
            existing_idx
        } else {
            let idx = self.vertices.len();
            self.vertices.push(vertex);
            vertex_tracking.insert_vertex(vertex, mapping, idx);
            idx
        }
    }

    #[inline(always)]
    pub fn vertex_get(&self, idx: usize) -> Vec3 {
        unsafe { *self.vertices.get_unchecked(idx) }
    }

    pub fn move_surface(
        &mut self,
        sector_id: usize,
        movement_type: MovementType,
        new_height: f32,
        texture: usize,
    ) {
        let subsector_ids = self.sector_subsectors[sector_id].clone();

        for subsector_id in subsector_ids {
            let leaf = &self.subsector_leaves[subsector_id];
            let polygon_indices = match movement_type {
                MovementType::Floor => &leaf.floor_polygons,
                MovementType::Ceiling => &leaf.ceiling_polygons,
                MovementType::None => return,
            };

            for &polygon_idx in polygon_indices {
                let polygon = &leaf.polygons[polygon_idx];
                for &vertex_idx in &polygon.vertices {
                    self.vertices[vertex_idx].z = new_height;
                }
            }
        }

        for &subsector_id in &self.sector_subsectors[sector_id] {
            let leaf = &mut self.subsector_leaves[subsector_id];
            let indices = match movement_type {
                MovementType::Floor => &leaf.floor_polygons,
                MovementType::Ceiling => &leaf.ceiling_polygons,
                MovementType::None => return,
            };
            for i in 0..indices.len() {
                let polygon_idx = indices[i];
                if let SurfaceKind::Horizontal {
                    texture: ref mut tex,
                    ..
                } = leaf.polygons[polygon_idx].surface_kind
                {
                    *tex = texture;
                }
            }
        }

        self.update_affected_aabbs(sector_id);
    }

    pub fn update_floor_texture(&mut self, sector_id: usize, new_texture: usize) {
        for &subsector_id in &self.sector_subsectors[sector_id] {
            let leaf = &mut self.subsector_leaves[subsector_id];
            for i in 0..leaf.floor_polygons.len() {
                let polygon_idx = leaf.floor_polygons[i];
                if let SurfaceKind::Horizontal {
                    texture,
                    ..
                } = &mut leaf.polygons[polygon_idx].surface_kind
                {
                    *texture = new_texture;
                }
            }
        }
    }

    fn build_linedef_wall_map(&mut self) {
        for (subsector_id, leaf) in self.subsector_leaves.iter().enumerate() {
            for (poly_idx, polygon) in leaf.polygons.iter().enumerate() {
                if let SurfaceKind::Vertical {
                    linedef_id,
                    ..
                } = polygon.surface_kind
                {
                    self.linedef_wall_polygons
                        .entry(linedef_id)
                        .or_default()
                        .push((subsector_id, poly_idx));
                }
            }
        }
    }

    /// Update the texture of all wall polygons belonging to `linedef_id` that
    /// match `wall_type`.  Called by the switch system after a sidedef texture
    /// change so the 3D scene stays in sync.
    pub fn update_wall_texture(
        &mut self,
        linedef_id: usize,
        wall_type: WallType,
        new_texture: usize,
    ) {
        let Some(entries) = self.linedef_wall_polygons.get(&linedef_id) else {
            return;
        };
        let entries = entries.clone();
        for (subsector_id, poly_idx) in entries {
            let polygon = &mut self.subsector_leaves[subsector_id].polygons[poly_idx];
            if let SurfaceKind::Vertical {
                texture,
                wall_type: wt,
                ..
            } = &mut polygon.surface_kind
            {
                if *wt == wall_type {
                    *texture = Some(new_texture);
                }
            }
        }
    }

    fn initialize_nodes(&mut self, nodes: &[Node], sectors: &[Sector]) {
        self.nodes = nodes
            .iter()
            .map(|node| {
                let min_z = sectors
                    .iter()
                    .map(|s| s.floorheight)
                    .fold(f32::INFINITY, f32::min);
                let max_z = sectors
                    .iter()
                    .map(|s| s.ceilingheight)
                    .fold(f32::NEG_INFINITY, f32::max);

                Node3D {
                    xy: node.xy,
                    delta: node.delta,
                    bboxes: [
                        AABB {
                            min: Vec3::new(node.bboxes[0][0].x, node.bboxes[0][0].y, min_z),
                            max: Vec3::new(node.bboxes[0][1].x, node.bboxes[0][1].y, max_z),
                        },
                        AABB {
                            min: Vec3::new(node.bboxes[1][0].x, node.bboxes[1][0].y, min_z),
                            max: Vec3::new(node.bboxes[1][1].x, node.bboxes[1][1].y, max_z),
                        },
                    ],
                    children: node.children,
                    aabb: AABB::new(),
                }
            })
            .collect();
    }

    fn initialize_subsectors(&mut self, subsectors: &[SubSector]) {
        self.subsector_leaves = vec![BSPLeaf3D::default(); subsectors.len()];
    }

    fn build_sector_subsector_mapping(
        &mut self,
        subsectors: &[SubSector],
        segments: &[Segment],
        sectors: &[Sector],
    ) {
        for (subsector_id, subsector) in subsectors.iter().enumerate() {
            let segment = &segments[subsector.start_seg as usize];
            let sector_id = segment.frontsector.num as usize;
            if sector_id < sectors.len() {
                self.sector_subsectors[sector_id].push(subsector_id);
            }
        }
    }

    /// BSP traversal to collect dividing lines and generate polygons
    fn carve_polygons_recursive(
        &mut self,
        nodes: &[Node],
        subsectors: &[SubSector],
        segments: &[Segment],
        linedefs: &[LineDef],
        node_id: u32,
        divlines: Vec<DivLine>,
        zh_wall_map: &HashMap<usize, ZeroHeightSectorVerts>,
        vertex_tracking: &mut VertexTracking,
    ) {
        #[cfg(feature = "hprof")]
        profile!("carve_polygons_recursive");

        if node_id & IS_SUBSECTOR_MASK != 0 {
            self.process_subsector_node(
                subsectors,
                segments,
                node_id,
                &divlines,
                vertex_tracking,
                zh_wall_map,
            );
        } else {
            self.process_internal_node(
                nodes,
                subsectors,
                segments,
                linedefs,
                node_id,
                divlines,
                zh_wall_map,
                vertex_tracking,
            );
        }
    }

    fn process_subsector_node(
        &mut self,
        subsectors: &[SubSector],
        segments: &[Segment],
        node_id: u32,
        divlines: &[DivLine],
        vertex_tracking: &mut VertexTracking,
        zh_wall_map: &HashMap<usize, ZeroHeightSectorVerts>,
    ) {
        let subsector_id = if node_id == u32::MAX {
            return;
        } else {
            (node_id & !IS_SUBSECTOR_MASK) as usize
        };

        if subsector_id < subsectors.len() {
            let subsector = &subsectors[subsector_id];
            let start_seg = subsector.start_seg as usize;
            let end_seg = start_seg + subsector.seg_count as usize;

            // Store sector ID on the leaf
            self.subsector_leaves[subsector_id].sector_id = subsector.sector.num as usize;

            if let Some(subsector_segments) = segments.get(start_seg..end_seg) {
                // Process segments for walls
                for segment_idx in start_seg..end_seg {
                    let segment = &segments[segment_idx];
                    {
                        let this = &mut *self;
                        let front_sector = &segment.frontsector;

                        let sv1 = *segment.v1;
                        let sv2 = *segment.v2;
                        this.subsector_leaves[subsector_id]
                            .occlusion_segs
                            .push(OcclusionSeg {
                                v1: sv1,
                                v2: sv2,
                                front_sector_id: front_sector.num as usize,
                                back_sector_id: segment.backsector.as_ref().map(|s| s.num as usize),
                                seg_angle_rad: (sv2.y - sv1.y).atan2(sv2.x - sv1.x),
                            });

                        // Check for back sector
                        if let Some(back_sector) = &segment.backsector {
                            this.create_two_sided_walls(
                                segment,
                                front_sector,
                                back_sector,
                                subsector_id,
                                zh_wall_map,
                                vertex_tracking,
                            );
                        } else {
                            this.create_one_sided_wall(
                                segment,
                                front_sector,
                                subsector_id,
                                zh_wall_map,
                                vertex_tracking,
                            );
                        }
                    };
                }

                // Process floors and ceilings
                let polygon = carve_subsector_polygon(
                    subsector_segments,
                    divlines,
                    &self.sector_subsectors,
                    segments,
                    subsectors,
                );
                self.carved_polygons[subsector_id] = polygon.clone();
                self.create_floor_ceiling_polygons(
                    subsector_id,
                    subsector,
                    &polygon,
                    vertex_tracking,
                    zh_wall_map,
                );
            }
        }
    }

    fn create_two_sided_walls(
        &mut self,
        segment: &Segment,
        front_sector: &Sector,
        back_sector: &Sector,
        front_subsector_id: usize,
        zh_wall_map: &HashMap<usize, ZeroHeightSectorVerts>,
        vertex_tracking: &mut VertexTracking,
    ) {
        let front_id = front_sector.num as usize;
        let back_id = back_sector.num as usize;

        // Upper wall: create if toptexture exists and back ceiling is at or
        // below front ceiling (includes zero-height). Skip when back ceiling
        // is above front — that's the back side segment and the front side
        // segment creates the wall.
        if let Some(texture) = segment.sidedef.toptexture {
            if back_sector.ceilingheight <= front_sector.ceilingheight {
                let bottom_height = back_sector.ceilingheight;
                let top_height = front_sector.ceilingheight;

                let wall_polygons = self.create_wall_quad_from_segment(
                    segment,
                    bottom_height,
                    top_height,
                    WallType::Upper,
                    texture,
                    front_id,
                    Some(back_id),
                    true,
                    zh_wall_map,
                    vertex_tracking,
                );
                for wall_polygon in wall_polygons {
                    self.subsector_leaves[front_subsector_id]
                        .polygons
                        .push(wall_polygon);
                }
            }
        }

        // Lower wall: create if bottomtexture exists and back floor is at or
        // above front floor (includes zero-height). Skip when back floor is
        // below front — that's the back side segment.
        if let Some(texture) = segment.sidedef.bottomtexture {
            if back_sector.floorheight >= front_sector.floorheight {
                let bottom_height = front_sector.floorheight;
                let top_height = back_sector.floorheight;

                let wall_polygons = self.create_wall_quad_from_segment(
                    segment,
                    bottom_height,
                    top_height,
                    WallType::Lower,
                    texture,
                    front_id,
                    Some(back_id),
                    true,
                    zh_wall_map,
                    vertex_tracking,
                );
                for wall_polygon in wall_polygons {
                    self.subsector_leaves[front_subsector_id]
                        .polygons
                        .push(wall_polygon);
                }
            }
        }

        // Middle wall: create if midtexture exists
        if let Some(texture) = segment.sidedef.midtexture {
            let bottom = front_sector.floorheight.max(back_sector.floorheight);
            let top = front_sector.ceilingheight.min(back_sector.ceilingheight);

            let wall_polygons = self.create_wall_quad_from_segment(
                segment,
                bottom,
                top,
                WallType::Middle,
                texture,
                front_id,
                Some(back_id),
                true,
                zh_wall_map,
                vertex_tracking,
            );
            for wall_polygon in wall_polygons {
                self.subsector_leaves[front_subsector_id]
                    .polygons
                    .push(wall_polygon);
            }
        }
    }

    fn create_one_sided_wall(
        &mut self,
        segment: &Segment,
        front_sector: &Sector,
        front_subsector_id: usize,
        zh_wall_map: &HashMap<usize, ZeroHeightSectorVerts>,
        vertex_tracking: &mut VertexTracking,
    ) {
        if let Some(texture) = segment.sidedef.midtexture {
            let wall_polygons = self.create_wall_quad_from_segment(
                segment,
                front_sector.floorheight,
                front_sector.ceilingheight,
                WallType::Middle,
                texture,
                front_sector.num as usize,
                None,
                false,
                zh_wall_map,
                vertex_tracking,
            );
            for wall_polygon in wall_polygons {
                self.subsector_leaves[front_subsector_id]
                    .polygons
                    .push(wall_polygon);
            }
        }
    }

    fn create_wall_quad_from_segment(
        &mut self,
        segment: &Segment,
        bottom_height: f32,
        top_height: f32,
        wall_type: WallType,
        texture: usize,
        sector_id: usize,
        back_sector_id: Option<usize>,
        two_sided: bool,
        zh_wall_map: &HashMap<usize, ZeroHeightSectorVerts>,
        vertex_tracking: &mut VertexTracking,
    ) -> Vec<SurfacePolygon> {
        let start_pos = *segment.v1;
        let end_pos = *segment.v2;
        let is_zero_height = (top_height - bottom_height).abs() <= HEIGHT_EPSILON;
        let q_start = QuantizedVec2::from_vec2(start_pos, QUANT_EPSILON);
        let q_end = QuantizedVec2::from_vec2(end_pos, QUANT_EPSILON);

        // Determine per-vertex mappings. For zero-height walls, use
        // LowerSeparated(sector_id) / UpperSeparated(sector_id) directly.
        // For non-zero-height walls, check zh_wall_map per endpoint so that
        // wall vertices match the floor/ceiling mapping at zh boundaries.
        let (bottom_start_map, bottom_end_map, top_start_map, top_end_map, moves) = match wall_type
        {
            WallType::Lower => {
                if is_zero_height {
                    let back_id = back_sector_id.unwrap_or(sector_id);
                    (
                        VertexMappedTo::LowerSeparated(sector_id),
                        VertexMappedTo::LowerSeparated(sector_id),
                        VertexMappedTo::LowerSeparated(back_id),
                        VertexMappedTo::LowerSeparated(back_id),
                        true,
                    )
                } else {
                    // Bottom = frontsector floor, top = backsector floor
                    let back_id = back_sector_id.unwrap_or(sector_id);
                    (
                        Self::zh_lower_mapping(zh_wall_map, sector_id, q_start),
                        Self::zh_lower_mapping(zh_wall_map, sector_id, q_end),
                        Self::zh_lower_mapping(zh_wall_map, back_id, q_start),
                        Self::zh_lower_mapping(zh_wall_map, back_id, q_end),
                        false,
                    )
                }
            }
            WallType::Upper => {
                if is_zero_height {
                    let back_id = back_sector_id.unwrap_or(sector_id);
                    (
                        VertexMappedTo::UpperSeparated(back_id),
                        VertexMappedTo::UpperSeparated(back_id),
                        VertexMappedTo::UpperSeparated(sector_id),
                        VertexMappedTo::UpperSeparated(sector_id),
                        true,
                    )
                } else {
                    // Bottom = backsector ceiling, top = frontsector ceiling
                    let back_id = back_sector_id.unwrap_or(sector_id);
                    (
                        Self::zh_upper_mapping(zh_wall_map, back_id, q_start),
                        Self::zh_upper_mapping(zh_wall_map, back_id, q_end),
                        Self::zh_upper_mapping(zh_wall_map, sector_id, q_start),
                        Self::zh_upper_mapping(zh_wall_map, sector_id, q_end),
                        false,
                    )
                }
            }
            WallType::Middle => {
                // Bottom sits on floor, top sits on ceiling
                (
                    Self::zh_lower_mapping(zh_wall_map, sector_id, q_start),
                    Self::zh_lower_mapping(zh_wall_map, sector_id, q_end),
                    Self::zh_upper_mapping(zh_wall_map, sector_id, q_start),
                    Self::zh_upper_mapping(zh_wall_map, sector_id, q_end),
                    false,
                )
            }
        };

        let bottom_start = self.vertex_add(
            Vec3::new(start_pos.x, start_pos.y, bottom_height),
            bottom_start_map,
            vertex_tracking,
        );
        let bottom_end = self.vertex_add(
            Vec3::new(end_pos.x, end_pos.y, bottom_height),
            bottom_end_map,
            vertex_tracking,
        );
        let top_start = self.vertex_add(
            Vec3::new(start_pos.x, start_pos.y, top_height),
            top_start_map,
            vertex_tracking,
        );
        let top_end = self.vertex_add(
            Vec3::new(end_pos.x, end_pos.y, top_height),
            top_end_map,
            vertex_tracking,
        );

        let wall_direction = (end_pos - start_pos).normalize();
        let normal = if is_zero_height {
            Vec3::new(wall_direction.y, -wall_direction.x, 0.0)
        } else {
            let v0 = self.vertices[bottom_start];
            let v1 = self.vertices[bottom_end];
            let v2 = self.vertices[top_start];
            let edge1 = v1 - v0;
            let edge2 = v2 - v0;
            edge1.cross(edge2).normalize()
        };

        let texture_direction = wall_direction.y.atan2(wall_direction.x);
        let texture_direction = Vec3::new(texture_direction.cos(), texture_direction.sin(), 0.0);

        let surface_kind = SurfaceKind::Vertical {
            texture: Some(texture),
            tex_x_offset: segment.sidedef.textureoffset + segment.offset,
            tex_y_offset: segment.sidedef.rowoffset,
            texture_direction,
            wall_type,
            wall_tex_pin: WallTexPin::from(segment.linedef.flags),
            front_ceiling_z: segment.frontsector.ceilingheight,
            two_sided,
            linedef_id: segment.linedef.num,
        };

        let triangle1 = SurfacePolygon::new(
            sector_id,
            surface_kind.clone(),
            vec![bottom_start, bottom_end, top_start],
            normal,
            &self.vertices,
            moves,
        );

        let triangle2 = SurfacePolygon::new(
            sector_id,
            surface_kind,
            vec![top_start, bottom_end, top_end],
            normal,
            &self.vertices,
            moves,
        );

        vec![triangle1, triangle2]
    }

    /// Check if a vertex position is on a zh lower wall boundary for a sector.
    /// Returns `LowerSeparated(sector_id)` if yes, `Lower` otherwise.
    fn zh_lower_mapping(
        zh_wall_map: &HashMap<usize, ZeroHeightSectorVerts>,
        sector_id: usize,
        pos: QuantizedVec2,
    ) -> VertexMappedTo {
        if let Some(verts) = zh_wall_map.get(&sector_id) {
            if verts.lower.contains(&pos) {
                return VertexMappedTo::LowerSeparated(sector_id);
            }
        }
        VertexMappedTo::Lower
    }

    /// Check if a vertex position is on a zh upper wall boundary for a sector.
    /// Returns `UpperSeparated(sector_id)` if yes, `Upper` otherwise.
    fn zh_upper_mapping(
        zh_wall_map: &HashMap<usize, ZeroHeightSectorVerts>,
        sector_id: usize,
        pos: QuantizedVec2,
    ) -> VertexMappedTo {
        if let Some(verts) = zh_wall_map.get(&sector_id) {
            if verts.upper.contains(&pos) {
                return VertexMappedTo::UpperSeparated(sector_id);
            }
        }
        VertexMappedTo::Upper
    }

    fn process_internal_node(
        &mut self,
        nodes: &[Node],
        subsectors: &[SubSector],
        segments: &[Segment],
        linedefs: &[LineDef],
        node_id: u32,
        divlines: Vec<DivLine>,
        zh_wall_map: &HashMap<usize, ZeroHeightSectorVerts>,
        vertex_tracking: &mut VertexTracking,
    ) {
        if let Some(node) = nodes.get(node_id as usize) {
            let node_divline = DivLine::from_node(node);

            // Process right child with original divline
            let mut right_divlines = divlines.clone();
            right_divlines.push(node_divline);
            self.carve_polygons_recursive(
                nodes,
                subsectors,
                segments,
                linedefs,
                node.children[0],
                right_divlines,
                zh_wall_map,
                vertex_tracking,
            );

            // Process left child with reversed divline
            let mut left_divlines = divlines;
            let mut reversed_divline = node_divline;
            reversed_divline.dx = -reversed_divline.dx;
            reversed_divline.dy = -reversed_divline.dy;
            left_divlines.push(reversed_divline);
            self.carve_polygons_recursive(
                nodes,
                subsectors,
                segments,
                linedefs,
                node.children[1],
                left_divlines,
                zh_wall_map,
                vertex_tracking,
            );
        }
    }

    fn create_floor_ceiling_polygons(
        &mut self,
        subsector_id: usize,
        subsector: &SubSector,
        polygon: &[Vec2],
        vertex_tracking: &mut VertexTracking,
        zh_wall_map: &HashMap<usize, ZeroHeightSectorVerts>,
    ) {
        if polygon.len() < 3 {
            return;
        }

        let sector_num = subsector.sector.num as usize;
        let zh_verts = zh_wall_map.get(&sector_num);
        let has_any_zh_lower = zh_verts.map_or(false, |v| !v.lower.is_empty());
        let has_any_zh_upper = zh_verts.map_or(false, |v| !v.upper.is_empty());

        // Generate floor and ceiling polygons using triangulation
        for i in 1..polygon.len() - 1 {
            // Floor polygon: per-vertex mapping. Vertices on a zero-height
            // lower wall boundary use LowerSeparated(sector_num); others use Lower.
            let floor_vertices_2d = [polygon[0], polygon[i + 1], polygon[i]];
            let floor_vertices = self.get_polygon_vertices_index_per_vertex(
                &floor_vertices_2d,
                subsector.sector.floorheight,
                VertexMappedTo::Lower,
                VertexMappedTo::LowerSeparated(sector_num),
                zh_verts.map(|v| &v.lower),
                vertex_tracking,
            );

            let floor_polygon = SurfacePolygon::new(
                sector_num,
                self.create_horizontal_surface_kind(subsector.sector.floorpic),
                floor_vertices,
                Vec3::new(0.0, 0.0, 1.0),
                &self.vertices,
                has_any_zh_lower,
            );

            let floor_polygon_index = self.subsector_leaves[subsector_id].polygons.len();
            self.subsector_leaves[subsector_id]
                .polygons
                .push(floor_polygon);
            self.subsector_leaves[subsector_id]
                .floor_polygons
                .push(floor_polygon_index);

            // Ceiling polygon: per-vertex mapping. Vertices on a zero-height
            // upper wall boundary use UpperSeparated(sector_num); others use Upper.
            let ceiling_vertices_2d = [polygon[i], polygon[i + 1], polygon[0]];
            let ceiling_vertices = self.get_polygon_vertices_index_per_vertex(
                &ceiling_vertices_2d,
                subsector.sector.ceilingheight,
                VertexMappedTo::Upper,
                VertexMappedTo::UpperSeparated(sector_num),
                zh_verts.map(|v| &v.upper),
                vertex_tracking,
            );

            let ceiling_polygon = SurfacePolygon::new(
                sector_num,
                self.create_horizontal_surface_kind(subsector.sector.ceilingpic),
                ceiling_vertices,
                Vec3::new(0.0, 0.0, -1.0),
                &self.vertices,
                has_any_zh_upper,
            );

            let ceiling_polygon_index = self.subsector_leaves[subsector_id].polygons.len();
            self.subsector_leaves[subsector_id]
                .polygons
                .push(ceiling_polygon);
            self.subsector_leaves[subsector_id]
                .ceiling_polygons
                .push(ceiling_polygon_index);
        }
    }

    /// Add floor/ceiling polygon vertices with per-vertex mapping.
    /// Vertices whose 2D position matches a zero-height wall endpoint use
    /// `separated_mapping`; all others use `default_mapping`.
    fn get_polygon_vertices_index_per_vertex(
        &mut self,
        vertices_2d: &[Vec2],
        height: f32,
        default_mapping: VertexMappedTo,
        separated_mapping: VertexMappedTo,
        separated_positions: Option<&Vec<QuantizedVec2>>,
        vertex_tracking: &mut VertexTracking,
    ) -> Vec<usize> {
        let mut vertex_indices = Vec::new();

        for &vertex_2d in vertices_2d {
            let mapping = if let Some(positions) = separated_positions {
                let q = QuantizedVec2::from_vec2(vertex_2d, QUANT_EPSILON);
                if positions.contains(&q) {
                    separated_mapping
                } else {
                    default_mapping
                }
            } else {
                default_mapping
            };

            let vertex_idx = self.vertex_add(
                Vec3::new(vertex_2d.x, vertex_2d.y, height),
                mapping,
                vertex_tracking,
            );
            vertex_indices.push(vertex_idx);
        }

        vertex_indices
    }

    fn update_affected_aabbs(&mut self, sector_id: usize) {
        let subsector_ids = self.sector_subsectors[sector_id].clone();

        for subsector_id in subsector_ids {
            let mut aabb = AABB::new();
            let leaf = &self.subsector_leaves[subsector_id];

            for polygon in &leaf.polygons {
                for &vertex_idx in &polygon.vertices {
                    aabb.expand_to_include_point(self.vertices[vertex_idx]);
                }
            }

            for &polygon_idx in &leaf.floor_polygons {
                let polygon = &leaf.polygons[polygon_idx];
                for &vertex_idx in &polygon.vertices {
                    aabb.expand_to_include_point(self.vertices[vertex_idx]);
                }
            }

            for &polygon_idx in &leaf.ceiling_polygons {
                let polygon = &leaf.polygons[polygon_idx];
                for &vertex_idx in &polygon.vertices {
                    aabb.expand_to_include_point(self.vertices[vertex_idx]);
                }
            }

            self.subsector_leaves[subsector_id].aabb = aabb;
        }
    }

    fn update_all_aabbs(&mut self) {
        for subsector_id in 0..self.subsector_leaves.len() {
            let mut aabb = AABB::new();
            let leaf = &self.subsector_leaves[subsector_id];

            for polygon in &leaf.polygons {
                for &vertex_idx in &polygon.vertices {
                    aabb.expand_to_include_point(self.vertices[vertex_idx]);
                }
            }

            for &polygon_idx in &leaf.floor_polygons {
                let polygon = &leaf.polygons[polygon_idx];
                for &vertex_idx in &polygon.vertices {
                    aabb.expand_to_include_point(self.vertices[vertex_idx]);
                }
            }

            for &polygon_idx in &leaf.ceiling_polygons {
                let polygon = &leaf.polygons[polygon_idx];
                for &vertex_idx in &polygon.vertices {
                    aabb.expand_to_include_point(self.vertices[vertex_idx]);
                }
            }

            self.subsector_leaves[subsector_id].aabb = aabb;
        }

        self.update_node_aabbs_recursive(self.root_node);
    }

    fn create_horizontal_surface_kind(&self, texture: usize) -> SurfaceKind {
        const TEXTURE_DIRECTION: f32 = std::f32::consts::PI / 2.0;
        SurfaceKind::Horizontal {
            texture,
            tex_cos: TEXTURE_DIRECTION.cos(),
            tex_sin: TEXTURE_DIRECTION.sin(),
        }
    }

    fn update_node_aabbs_recursive(&mut self, node_id: u32) {
        if node_id & IS_SUBSECTOR_MASK != 0 {
            return;
        }

        let node_idx = node_id as usize;
        if node_idx >= self.nodes.len() {
            return;
        }

        let children = self.nodes[node_idx].children;
        let mut combined_aabb = AABB::new();
        let mut has_valid_aabb = false;

        for &child_id in &children {
            if child_id & IS_SUBSECTOR_MASK != 0 {
                let subsector_id = (child_id & !IS_SUBSECTOR_MASK) as usize;
                let leaf = &self.subsector_leaves[subsector_id];
                combined_aabb.expand_to_include_aabb(&leaf.aabb);
                has_valid_aabb = true;
            } else {
                self.update_node_aabbs_recursive(child_id);
                if let Some(child_aabb) = self.nodes.get(child_id as usize).map(|n| &n.aabb) {
                    combined_aabb.expand_to_include_aabb(child_aabb);
                    has_valid_aabb = true;
                }
            }
        }
        //Use direct addressing on arrays, not `.get()`
        if has_valid_aabb {
            self.nodes[node_idx].aabb = combined_aabb;
        }
    }

    /// Expand leaf and node AABBs for mover sectors and zero-height sectors
    /// (doors) to cover the full movement range. The expanded AABBs are kept
    /// permanently on leaves for PVS use.
    fn expand_node_aabbs_for_movers(&mut self, sectors: &[Sector], linedefs: &[LineDef]) {
        for (sector_id, sector) in sectors.iter().enumerate() {
            let is_mover = is_sector_mover(sector, linedefs);
            let is_zero_height =
                (sector.ceilingheight - sector.floorheight).abs() <= HEIGHT_EPSILON;

            if !is_mover && !is_zero_height {
                continue;
            }

            let mut min_floor = sector.floorheight;
            let mut max_ceil = sector.ceilingheight;

            for line in &sector.lines {
                if !line.flags.contains(LineDefFlags::TwoSided) {
                    continue;
                }
                let neighbor = if line.frontsector.num == sector.num as i32 {
                    line.backsector.as_ref()
                } else {
                    Some(&line.frontsector)
                };
                if let Some(other) = neighbor {
                    if other.floorheight < min_floor {
                        min_floor = other.floorheight;
                    }
                    if other.ceilingheight > max_ceil {
                        max_ceil = other.ceilingheight;
                    }
                }
            }

            for &subsector_id in &self.sector_subsectors[sector_id] {
                let leaf = &mut self.subsector_leaves[subsector_id];
                if min_floor < leaf.aabb.min.z {
                    leaf.aabb.min.z = min_floor;
                }
                if max_ceil > leaf.aabb.max.z {
                    leaf.aabb.max.z = max_ceil;
                }
            }
        }

        self.update_node_aabbs_recursive(self.root_node);
    }

    pub fn get_node_aabb(&self, node_id: u32) -> Option<&AABB> {
        if node_id & IS_SUBSECTOR_MASK != 0 {
            let subsector_id = (node_id & !IS_SUBSECTOR_MASK) as usize;
            self.subsector_leaves
                .get(subsector_id)
                .map(|leaf| &leaf.aabb)
        } else {
            self.nodes.get(node_id as usize).map(|node| &node.aabb)
        }
    }
}

impl Default for BSP3D {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            subsector_leaves: Vec::new(),
            root_node: 0,
            vertices: Vec::new(),
            sector_subsectors: Vec::new(),
            carved_polygons: Vec::new(),
            linedef_wall_polygons: HashMap::new(),
        }
    }
}

impl Default for BSPLeaf3D {
    fn default() -> Self {
        Self {
            polygons: Vec::new(),
            aabb: AABB::new(),
            floor_polygons: Vec::new(),
            ceiling_polygons: Vec::new(),
            sector_id: 0,
            occlusion_segs: Vec::new(),
        }
    }
}

// NOTE: WAD-loading tests (test_zero_height_walls) remain in gameplay crate
// as integration tests since they need PicData.
