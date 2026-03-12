#[cfg(feature = "hprof")]
use coarse_prof::profile;

use crate::flags::LineDefFlags;
use crate::map_defs::{LineDef, Node, Sector, Segment, SubSector};
use crate::triangulation::{DivLine, carve_subsector_polygon};
use glam::{Vec2, Vec3};
use std::collections::{HashMap, HashSet};

const IS_SUBSECTOR_MASK: u32 = 0x8000_0000;
/// Quantization grid for position-only vertex dedup.
const QUANT_PRECISION: f32 = 2.0;
const HEIGHT_EPSILON: f32 = 0.1;
/// Minimum cross-product magnitude for a non-degenerate triangle.
const MIN_TRI_CROSS: f32 = 1e-4;
/// Maximum perpendicular distance (map units) for T-junction detection.
const TJUNC_DIST: f32 = 0.666;
/// Spatial grid cell size for the T-junction edge lookup.
const TJUNC_CELL: f32 = 4.0;
/// Point-on-edge parameter epsilon.
const EDGE_EPSILON: f32 = 0.001;
/// Deduplication tolerance for vertex proximity checks.
const DEDUP_EPSILON: f32 = 0.1;

/// Construction-only record tracking zero-height wall vertex roles.
/// Needed because zh walls have bottom and top at the same (x,y,z) — with
/// position-only dedup they'd share one index, producing degenerate triangles.
/// Fresh vertices are created instead, and this record tells the post-pass
/// which vertices are bottom (front sector) vs top (back sector).
#[derive(Clone)]
struct ZhWallRecord {
    /// Subsector leaf containing the wall polygons.
    subsector_id: usize,
    /// Polygon indices within the subsector leaf (two triangles).
    poly_indices: [usize; 2],
    /// Vertex indices for the bottom edge [start, end].
    bottom: [usize; 2],
    /// Vertex indices for the top edge [start, end].
    top: [usize; 2],
    /// Wall type (Upper/Lower/Middle).
    wall_type: WallType,
    /// Front sector of the segment.
    front_sector: usize,
    /// Back sector of the segment.
    back_sector: usize,
}

#[derive(Default, Debug, Clone, Copy, PartialEq)]
pub enum MovementType {
    Floor,
    Ceiling,
    #[default]
    None,
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
        /// For texture alignment.
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
/// only if their quantized coordinates match.
#[derive(Hash, PartialEq, Eq, Clone, Copy)]
struct QuantizedVec3 {
    x: i32,
    y: i32,
    z: i32,
}

impl QuantizedVec3 {
    fn from_vec3(v: Vec3, precision: f32) -> Self {
        Self {
            x: (v.x / precision).round() as i32,
            y: (v.y / precision).round() as i32,
            z: (v.z / precision).round() as i32,
        }
    }
}

/// 2D position key for per-sector vertex separation at zh boundaries.
#[derive(Hash, PartialEq, Eq, Clone, Copy)]
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
    /// Temporary: zh wall records used during construction only.
    zh_wall_records: Vec<ZhWallRecord>,
}

impl BSP3D {
    pub fn new(
        root_node: u32,
        nodes: &[Node],
        subsectors: &[SubSector],
        segments: &[Segment],
        sectors: &[Sector],
        linedefs: &[LineDef],
        corrected_divlines: &[DivLine],
        sky_num: Option<usize>,
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
            zh_wall_records: Vec::new(),
        };

        let mut vertex_map: HashMap<QuantizedVec3, usize> =
            HashMap::with_capacity(segments.len() * 2);

        bsp3d.initialize_nodes(nodes, sectors);
        bsp3d.initialize_subsectors(subsectors);
        bsp3d.build_sector_subsector_mapping(subsectors, segments, sectors);
        bsp3d.subsector_leaves = vec![BSPLeaf3D::default(); subsectors.len()];

        // Phase 1: Create all geometry with position-only vertex dedup.
        // Segment vertices are already at canonical positions from the
        // snap_vertices_to_canonical pass in map loading.
        bsp3d.carve_polygons_recursive(
            nodes,
            subsectors,
            segments,
            linedefs,
            corrected_divlines,
            bsp3d.root_node,
            Vec::new(),
            &mut vertex_map,
            sky_num,
        );

        // Phase 1b: Fix T-junctions between adjacent subsector polygons.
        // Commented out to assess intersection cache impact in isolation.
        bsp3d.tjunction_fix_pass(subsectors);

        // Phase 2: Post-creation pass — split triangles at boundary endpoints,
        // separate mover vertices at zero-height boundaries, set moves flags.
        bsp3d.mover_vertex_pass(sectors, segments, subsectors, linedefs);
        bsp3d.zh_wall_records = Vec::new();

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

    /// Add or reuse a vertex by position. Simple position-only dedup.
    fn vertex_add(
        &mut self,
        vertex: Vec3,
        vertex_map: &mut HashMap<QuantizedVec3, usize>,
    ) -> usize {
        let key = QuantizedVec3::from_vec3(vertex, QUANT_PRECISION);
        if let Some(&idx) = vertex_map.get(&key) {
            idx
        } else {
            let idx = self.vertices.len();
            self.vertices.push(vertex);
            vertex_map.insert(key, idx);
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

    /// BSP traversal to collect dividing lines and generate polygons.
    fn carve_polygons_recursive(
        &mut self,
        nodes: &[Node],
        subsectors: &[SubSector],
        segments: &[Segment],
        linedefs: &[LineDef],
        corrected_divlines: &[DivLine],
        node_id: u32,
        divlines: Vec<DivLine>,
        vertex_map: &mut HashMap<QuantizedVec3, usize>,
        sky_num: Option<usize>,
    ) {
        #[cfg(feature = "hprof")]
        profile!("carve_polygons_recursive");

        if node_id & IS_SUBSECTOR_MASK != 0 {
            self.process_subsector_node(
                subsectors, segments, node_id, &divlines, vertex_map, sky_num,
            );
        } else {
            self.process_internal_node(
                nodes,
                subsectors,
                segments,
                linedefs,
                corrected_divlines,
                node_id,
                divlines,
                vertex_map,
                sky_num,
            );
        }
    }

    fn process_subsector_node(
        &mut self,
        subsectors: &[SubSector],
        segments: &[Segment],
        node_id: u32,
        divlines: &[DivLine],
        vertex_map: &mut HashMap<QuantizedVec3, usize>,
        sky_num: Option<usize>,
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

            self.subsector_leaves[subsector_id].sector_id = subsector.sector.num as usize;

            if let Some(subsector_segments) = segments.get(start_seg..end_seg) {
                // Create walls
                for segment_idx in start_seg..end_seg {
                    let segment = &segments[segment_idx];
                    let front_sector = &segment.frontsector;
                    let sv1 = *segment.v1;
                    let sv2 = *segment.v2;
                    self.subsector_leaves[subsector_id]
                        .occlusion_segs
                        .push(OcclusionSeg {
                            v1: sv1,
                            v2: sv2,
                            front_sector_id: front_sector.num as usize,
                            back_sector_id: segment.backsector.as_ref().map(|s| s.num as usize),
                            seg_angle_rad: (sv2.y - sv1.y).atan2(sv2.x - sv1.x),
                        });

                    if let Some(back_sector) = &segment.backsector {
                        self.create_two_sided_walls(
                            segment,
                            front_sector,
                            back_sector,
                            subsector_id,
                            vertex_map,
                            sky_num,
                        );
                    } else {
                        self.create_one_sided_wall(segment, front_sector, subsector_id, vertex_map);
                    }
                }

                // Create floor/ceiling polygons from f64 carving result.
                let polygon_f64 =
                    carve_subsector_polygon(subsector_segments, divlines, subsector_id);
                let polygon: Vec<Vec2> = polygon_f64
                    .iter()
                    .map(|&(x, y)| Vec2::new(x as f32, y as f32))
                    .collect();
                self.carved_polygons[subsector_id] = polygon.clone();
                self.create_floor_ceiling_polygons(
                    subsector_id,
                    subsector,
                    &polygon,
                    vertex_map,
                    sky_num,
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
        vertex_map: &mut HashMap<QuantizedVec3, usize>,
        sky_num: Option<usize>,
    ) {
        let front_id = front_sector.num as usize;
        let back_id = back_sector.num as usize;

        // Sky hack: suppress upper wall between two sky-ceiling sectors and
        // lower wall between two sky-floor sectors (matches original Doom
        // r_segs.c behaviour).
        let both_sky_ceil = sky_num
            .is_some_and(|sky| front_sector.ceilingpic == sky && back_sector.ceilingpic == sky);
        let both_sky_floor =
            sky_num.is_some_and(|sky| front_sector.floorpic == sky && back_sector.floorpic == sky);

        // Upper wall: create if toptexture exists and back ceiling is at or
        // below front ceiling (includes zero-height). Suppressed when both
        // sectors have sky ceilings.
        if !both_sky_ceil {
            if let Some(texture) = segment.sidedef.toptexture {
                if back_sector.ceilingheight <= front_sector.ceilingheight {
                    self.add_wall_quad(
                        segment,
                        back_sector.ceilingheight,
                        front_sector.ceilingheight,
                        WallType::Upper,
                        texture,
                        front_id,
                        true,
                        front_subsector_id,
                        Some(back_id),
                        vertex_map,
                    );
                }
            }
        }

        // Lower wall: create if bottomtexture exists and back floor is at or
        // above front floor (includes zero-height). Suppressed when both
        // sectors have sky floors.
        if !both_sky_floor {
            if let Some(texture) = segment.sidedef.bottomtexture {
                if back_sector.floorheight >= front_sector.floorheight {
                    self.add_wall_quad(
                        segment,
                        front_sector.floorheight,
                        back_sector.floorheight,
                        WallType::Lower,
                        texture,
                        front_id,
                        true,
                        front_subsector_id,
                        Some(back_id),
                        vertex_map,
                    );
                }
            }
        }

        // Middle wall: create if midtexture exists.
        if let Some(texture) = segment.sidedef.midtexture {
            let bottom = front_sector.floorheight.max(back_sector.floorheight);
            let top = front_sector.ceilingheight.min(back_sector.ceilingheight);
            self.add_wall_quad(
                segment,
                bottom,
                top,
                WallType::Middle,
                texture,
                front_id,
                true,
                front_subsector_id,
                Some(back_id),
                vertex_map,
            );
        }
    }

    fn create_one_sided_wall(
        &mut self,
        segment: &Segment,
        front_sector: &Sector,
        front_subsector_id: usize,
        vertex_map: &mut HashMap<QuantizedVec3, usize>,
    ) {
        if let Some(texture) = segment.sidedef.midtexture {
            let front_id = front_sector.num as usize;
            let is_zh =
                (front_sector.ceilingheight - front_sector.floorheight).abs() <= HEIGHT_EPSILON;
            // For zh sectors (doors): pass self as back_sector so add_wall_quad
            // creates fresh vertices and a ZhWallRecord. The mover pass
            // connects bottom → floor vertex, top → ceiling vertex.
            let back_sector_id = if is_zh { Some(front_id) } else { None };
            self.add_wall_quad(
                segment,
                front_sector.floorheight,
                front_sector.ceilingheight,
                WallType::Middle,
                texture,
                front_id,
                false,
                front_subsector_id,
                back_sector_id,
                vertex_map,
            );
        }
    }

    /// Create wall quad triangles from a segment and push them to the
    /// subsector leaf. For zero-height walls with a back sector, creates
    /// fresh (non-dedup'd) vertices so bottom and top have distinct indices,
    /// and records a `ZhWallRecord` for the post-pass.
    fn add_wall_quad(
        &mut self,
        segment: &Segment,
        bottom_height: f32,
        top_height: f32,
        wall_type: WallType,
        texture: usize,
        sector_id: usize,
        two_sided: bool,
        subsector_id: usize,
        back_sector_id: Option<usize>,
        vertex_map: &mut HashMap<QuantizedVec3, usize>,
    ) {
        let start_pos = *segment.v1;
        let end_pos = *segment.v2;
        let is_zero_height = (top_height - bottom_height).abs() <= HEIGHT_EPSILON;

        let (bottom_start, bottom_end, top_start, top_end) = if is_zero_height
            && back_sector_id.is_some()
        {
            // Fresh vertices for zh walls — bypass dedup so bottom and top
            // get distinct indices even though they're at the same position.
            let bs = self.vertices.len();
            self.vertices
                .push(Vec3::new(start_pos.x, start_pos.y, bottom_height));
            let be = self.vertices.len();
            self.vertices
                .push(Vec3::new(end_pos.x, end_pos.y, bottom_height));
            let ts = self.vertices.len();
            self.vertices
                .push(Vec3::new(start_pos.x, start_pos.y, top_height));
            let te = self.vertices.len();
            self.vertices
                .push(Vec3::new(end_pos.x, end_pos.y, top_height));
            (bs, be, ts, te)
        } else {
            let bs = self.vertex_add(
                Vec3::new(start_pos.x, start_pos.y, bottom_height),
                vertex_map,
            );
            let be = self.vertex_add(Vec3::new(end_pos.x, end_pos.y, bottom_height), vertex_map);
            let ts = self.vertex_add(Vec3::new(start_pos.x, start_pos.y, top_height), vertex_map);
            let te = self.vertex_add(Vec3::new(end_pos.x, end_pos.y, top_height), vertex_map);
            (bs, be, ts, te)
        };

        let wall_direction = (end_pos - start_pos).normalize();
        let normal = if is_zero_height {
            Vec3::new(wall_direction.y, -wall_direction.x, 0.0)
        } else {
            let v0 = self.vertices[bottom_start];
            let v1 = self.vertices[bottom_end];
            let v2 = self.vertices[top_start];
            (v1 - v0).cross(v2 - v0).normalize()
        };

        let angle = wall_direction.y.atan2(wall_direction.x);
        let texture_direction = Vec3::new(angle.cos(), angle.sin(), 0.0);

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

        let tri1 = SurfacePolygon::new(
            sector_id,
            surface_kind.clone(),
            vec![bottom_start, bottom_end, top_start],
            normal,
            &self.vertices,
            false,
        );
        let pi0 = self.subsector_leaves[subsector_id].polygons.len();
        self.subsector_leaves[subsector_id].polygons.push(tri1);

        let tri2 = SurfacePolygon::new(
            sector_id,
            surface_kind,
            vec![top_start, bottom_end, top_end],
            normal,
            &self.vertices,
            false,
        );
        let pi1 = self.subsector_leaves[subsector_id].polygons.len();
        self.subsector_leaves[subsector_id].polygons.push(tri2);

        if is_zero_height {
            if let Some(back_id) = back_sector_id {
                self.zh_wall_records.push(ZhWallRecord {
                    subsector_id,
                    poly_indices: [pi0, pi1],
                    bottom: [bottom_start, bottom_end],
                    top: [top_start, top_end],
                    wall_type,
                    front_sector: sector_id,
                    back_sector: back_id,
                });
            }
        }
    }

    fn process_internal_node(
        &mut self,
        nodes: &[Node],
        subsectors: &[SubSector],
        segments: &[Segment],
        linedefs: &[LineDef],
        corrected_divlines: &[DivLine],
        node_id: u32,
        divlines: Vec<DivLine>,
        vertex_map: &mut HashMap<QuantizedVec3, usize>,
        sky_num: Option<usize>,
    ) {
        if let Some(node) = nodes.get(node_id as usize) {
            let nid = node_id as usize;
            let node_divline = if nid < corrected_divlines.len() {
                corrected_divlines[nid]
            } else {
                DivLine::from_node(node)
            };

            let mut right_divlines = divlines.clone();
            right_divlines.push(node_divline);
            self.carve_polygons_recursive(
                nodes,
                subsectors,
                segments,
                linedefs,
                corrected_divlines,
                node.children[0],
                right_divlines,
                vertex_map,
                sky_num,
            );

            let mut left_divlines = divlines;
            let mut reversed = node_divline;
            reversed.dx = -reversed.dx;
            reversed.dy = -reversed.dy;
            left_divlines.push(reversed);
            self.carve_polygons_recursive(
                nodes,
                subsectors,
                segments,
                linedefs,
                corrected_divlines,
                node.children[1],
                left_divlines,
                vertex_map,
                sky_num,
            );
        }
    }

    /// Create floor and ceiling triangles via fan triangulation. No mover
    /// awareness — all vertices use simple position dedup. The post-pass
    /// handles triangle splitting and vertex separation.
    fn create_floor_ceiling_polygons(
        &mut self,
        subsector_id: usize,
        subsector: &SubSector,
        polygon: &[Vec2],
        vertex_map: &mut HashMap<QuantizedVec3, usize>,
        sky_num: Option<usize>,
    ) {
        if polygon.len() < 3 {
            return;
        }

        let sector_num = subsector.sector.num as usize;
        let floor_h = subsector.sector.floorheight;
        let ceil_h = subsector.sector.ceilingheight;

        // Sky flats are rendered as a full-screen backdrop pass, not as
        // per-subsector polygons. Skip creation so they don't consume depth
        // buffer or rendering time.
        let skip_floor = sky_num.is_some_and(|sky| subsector.sector.floorpic == sky);
        let skip_ceil = sky_num.is_some_and(|sky| subsector.sector.ceilingpic == sky);

        // For polygons with > 4 vertices (typically from expand_polygon
        // inserting segment endpoints), use centroid fan to avoid long sliver
        // triangles that a corner fan would produce.
        let centroid_fan = polygon.len() > 4;
        let center = if centroid_fan {
            let inv = 1.0 / polygon.len() as f32;
            let cx = polygon.iter().map(|v| v.x).sum::<f32>() * inv;
            let cy = polygon.iter().map(|v| v.y).sum::<f32>() * inv;
            Some(Vec2::new(cx, cy))
        } else {
            None
        };

        if let Some(c) = center {
            // Centroid fan: one triangle per edge.
            for i in 0..polygon.len() {
                let next = (i + 1) % polygon.len();

                if !skip_floor {
                    // Floor: winding order [center, next, i] for upward normal.
                    let fv: Vec<usize> = [c, polygon[next], polygon[i]]
                        .iter()
                        .map(|v| self.vertex_add(Vec3::new(v.x, v.y, floor_h), vertex_map))
                        .collect();

                    if !Self::is_degenerate_triangle(&fv, &self.vertices) {
                        let fp = SurfacePolygon::new(
                            sector_num,
                            self.create_horizontal_surface_kind(subsector.sector.floorpic),
                            fv,
                            Vec3::new(0.0, 0.0, 1.0),
                            &self.vertices,
                            false,
                        );
                        let fi = self.subsector_leaves[subsector_id].polygons.len();
                        self.subsector_leaves[subsector_id].polygons.push(fp);
                        self.subsector_leaves[subsector_id].floor_polygons.push(fi);
                    }
                }

                if !skip_ceil {
                    // Ceiling: winding order [i, next, center] for downward normal.
                    let cv: Vec<usize> = [polygon[i], polygon[next], c]
                        .iter()
                        .map(|v| self.vertex_add(Vec3::new(v.x, v.y, ceil_h), vertex_map))
                        .collect();

                    if !Self::is_degenerate_triangle(&cv, &self.vertices) {
                        let cp = SurfacePolygon::new(
                            sector_num,
                            self.create_horizontal_surface_kind(subsector.sector.ceilingpic),
                            cv,
                            Vec3::new(0.0, 0.0, -1.0),
                            &self.vertices,
                            false,
                        );
                        let ci = self.subsector_leaves[subsector_id].polygons.len();
                        self.subsector_leaves[subsector_id].polygons.push(cp);
                        self.subsector_leaves[subsector_id]
                            .ceiling_polygons
                            .push(ci);
                    }
                }
            }
        } else {
            // Standard corner fan from polygon[0].
            for i in 1..polygon.len() - 1 {
                if !skip_floor {
                    // Floor: winding order [0, i+1, i] for upward normal.
                    let fv: Vec<usize> = [polygon[0], polygon[i + 1], polygon[i]]
                        .iter()
                        .map(|v| self.vertex_add(Vec3::new(v.x, v.y, floor_h), vertex_map))
                        .collect();

                    if !Self::is_degenerate_triangle(&fv, &self.vertices) {
                        let fp = SurfacePolygon::new(
                            sector_num,
                            self.create_horizontal_surface_kind(subsector.sector.floorpic),
                            fv,
                            Vec3::new(0.0, 0.0, 1.0),
                            &self.vertices,
                            false,
                        );
                        let fi = self.subsector_leaves[subsector_id].polygons.len();
                        self.subsector_leaves[subsector_id].polygons.push(fp);
                        self.subsector_leaves[subsector_id].floor_polygons.push(fi);
                    }
                }

                if !skip_ceil {
                    // Ceiling: winding order [i, i+1, 0] for downward normal.
                    let cv: Vec<usize> = [polygon[i], polygon[i + 1], polygon[0]]
                        .iter()
                        .map(|v| self.vertex_add(Vec3::new(v.x, v.y, ceil_h), vertex_map))
                        .collect();

                    if !Self::is_degenerate_triangle(&cv, &self.vertices) {
                        let cp = SurfacePolygon::new(
                            sector_num,
                            self.create_horizontal_surface_kind(subsector.sector.ceilingpic),
                            cv,
                            Vec3::new(0.0, 0.0, -1.0),
                            &self.vertices,
                            false,
                        );
                        let ci = self.subsector_leaves[subsector_id].polygons.len();
                        self.subsector_leaves[subsector_id].polygons.push(cp);
                        self.subsector_leaves[subsector_id]
                            .ceiling_polygons
                            .push(ci);
                    }
                }
            }
        }
    }

    /// Returns true if the triangle has duplicate vertex indices or near-zero
    /// area (collinear vertices from quantization merging).
    fn is_degenerate_triangle(indices: &[usize], vertices: &[Vec3]) -> bool {
        if indices.len() < 3 {
            return true;
        }
        let (a, b, c) = (indices[0], indices[1], indices[2]);
        if a == b || b == c || a == c {
            return true;
        }
        (vertices[b] - vertices[a])
            .cross(vertices[c] - vertices[a])
            .length()
            < MIN_TRI_CROSS
    }

    /// Fix T-junctions between adjacent subsector polygons.
    ///
    /// After Sutherland-Hodgman carving, a vertex from one subsector can land
    /// near an edge of a neighbouring subsector without a corresponding vertex
    /// on that edge. This creates a hairline gap. The fix: for each such
    /// vertex, split the neighbour's edge (and its floor/ceiling triangles)
    /// so both polygons share the exact same vertex.
    fn tjunction_fix_pass(&mut self, subsectors: &[SubSector]) {
        // 1. Build spatial grid of all carved polygon edges.
        let mut grid: HashMap<(i32, i32), Vec<(usize, usize, Vec2, Vec2)>> = HashMap::new();
        let inv_cell = 1.0 / TJUNC_CELL;

        for (ss_id, poly) in self.carved_polygons.iter().enumerate() {
            let n = poly.len();
            if n < 3 {
                continue;
            }
            for ei in 0..n {
                let a = poly[ei];
                let b = poly[(ei + 1) % n];
                let min_x = a.x.min(b.x) - TJUNC_DIST;
                let max_x = a.x.max(b.x) + TJUNC_DIST;
                let min_y = a.y.min(b.y) - TJUNC_DIST;
                let max_y = a.y.max(b.y) + TJUNC_DIST;
                let cx0 = (min_x * inv_cell).floor() as i32;
                let cx1 = (max_x * inv_cell).floor() as i32;
                let cy0 = (min_y * inv_cell).floor() as i32;
                let cy1 = (max_y * inv_cell).floor() as i32;
                for cx in cx0..=cx1 {
                    for cy in cy0..=cy1 {
                        grid.entry((cx, cy)).or_default().push((ss_id, ei, a, b));
                    }
                }
            }
        }

        // 2. Scan all vertices for T-junctions against edges from other subsectors. The
        //    mover pass handles vertex separation at sector boundaries.
        struct TJunc {
            pt: Vec2,
            target_ss: usize,
        }

        let mut fixes: Vec<TJunc> = Vec::new();

        for (src_ss, poly) in self.carved_polygons.iter().enumerate() {
            for &v in poly {
                let cx = (v.x * inv_cell).floor() as i32;
                let cy = (v.y * inv_cell).floor() as i32;
                if let Some(edges) = grid.get(&(cx, cy)) {
                    for &(tgt_ss, _ei, ea, eb) in edges {
                        if tgt_ss == src_ss {
                            continue;
                        }
                        if (v - ea).length() < TJUNC_DIST || (v - eb).length() < TJUNC_DIST {
                            continue;
                        }
                        let edge_vec = eb - ea;
                        let edge_len_sq = edge_vec.length_squared();
                        if edge_len_sq < 1e-6 {
                            continue;
                        }
                        let t = (v - ea).dot(edge_vec) / edge_len_sq;
                        if t <= EDGE_EPSILON || t >= 1.0 - EDGE_EPSILON {
                            continue;
                        }
                        let proj = ea + edge_vec * t;
                        let perp_dist = (v - proj).length();
                        if perp_dist < TJUNC_DIST {
                            let dup = fixes.iter().any(|f| {
                                f.target_ss == tgt_ss && (f.pt - v).length() < DEDUP_EPSILON
                            });
                            if !dup {
                                fixes.push(TJunc {
                                    pt: v,
                                    target_ss: tgt_ss,
                                });
                            }
                        }
                    }
                }
            }
        }

        // 3. Apply fixes: insert vertex into carved polygon + split triangles.
        for fix in &fixes {
            let poly = &self.carved_polygons[fix.target_ss];
            let n = poly.len();
            let mut insert_pos = None;
            for i in 0..n {
                let ea = poly[i];
                let eb = poly[(i + 1) % n];
                let edge_vec = eb - ea;
                let edge_len_sq = edge_vec.length_squared();
                if edge_len_sq < 1e-6 {
                    continue;
                }
                let t = (fix.pt - ea).dot(edge_vec) / edge_len_sq;
                if t <= EDGE_EPSILON || t >= 1.0 - EDGE_EPSILON {
                    continue;
                }
                let proj = ea + edge_vec * t;
                if (fix.pt - proj).length() < TJUNC_DIST {
                    insert_pos = Some(i + 1);
                    break;
                }
            }

            if let Some(pos) = insert_pos {
                self.carved_polygons[fix.target_ss].insert(pos, fix.pt);
            }

            let ss = &subsectors[fix.target_ss];
            let floor_h = ss.sector.floorheight;
            let ceil_h = ss.sector.ceilingheight;

            self.split_triangle_at_tjunc(fix.target_ss, true, floor_h, fix.pt);
            self.split_triangle_at_tjunc(fix.target_ss, false, ceil_h, fix.pt);
        }
    }

    /// Split a single floor or ceiling triangle at a T-junction point.
    /// Finds the triangle whose edge contains `pt` and splits it into two.
    fn split_triangle_at_tjunc(
        &mut self,
        subsector_id: usize,
        is_floor: bool,
        height: f32,
        pt: Vec2,
    ) {
        let current_indices = if is_floor {
            self.subsector_leaves[subsector_id].floor_polygons.clone()
        } else {
            self.subsector_leaves[subsector_id].ceiling_polygons.clone()
        };

        for &pi in &current_indices {
            let verts = self.subsector_leaves[subsector_id].polygons[pi]
                .vertices
                .clone();
            if verts.len() != 3 {
                continue;
            }

            let already_present = verts.iter().any(|&vi| {
                let v = self.vertices[vi];
                (v.x - pt.x).abs() < DEDUP_EPSILON && (v.y - pt.y).abs() < DEDUP_EPSILON
            });
            if already_present {
                continue;
            }

            let mut split_edge = None;
            for e in 0..3 {
                let ei0 = verts[e];
                let ei1 = verts[(e + 1) % 3];
                let e0 = Vec2::new(self.vertices[ei0].x, self.vertices[ei0].y);
                let e1 = Vec2::new(self.vertices[ei1].x, self.vertices[ei1].y);
                let edge_vec = e1 - e0;
                let edge_len_sq = edge_vec.length_squared();
                if edge_len_sq < EDGE_EPSILON * EDGE_EPSILON {
                    continue;
                }
                let to_v = pt - e0;
                let proj = to_v.dot(edge_vec) / edge_len_sq;
                if proj > EDGE_EPSILON && proj < 1.0 - EDGE_EPSILON {
                    let projected = e0 + edge_vec * proj;
                    if (pt - projected).length() < TJUNC_DIST {
                        split_edge = Some(e);
                        break;
                    }
                }
            }

            if let Some(e) = split_edge {
                let target = Vec3::new(pt.x, pt.y, height);
                let new_vi = self.vertices.len();
                self.vertices.push(target);

                let a = verts[e];
                let b = verts[(e + 1) % 3];
                let c = verts[(e + 2) % 3];

                // Stricter threshold (10x) to catch near-degenerate slivers.
                let tri1 = [a, new_vi, c];
                let tri2 = [new_vi, b, c];
                let tjunc_degen = |tri: &[usize; 3]| -> bool {
                    if tri[0] == tri[1] || tri[1] == tri[2] || tri[0] == tri[2] {
                        return true;
                    }
                    (self.vertices[tri[1]] - self.vertices[tri[0]])
                        .cross(self.vertices[tri[2]] - self.vertices[tri[0]])
                        .length()
                        < MIN_TRI_CROSS * 10.0
                };
                if tjunc_degen(&tri1) || tjunc_degen(&tri2) {
                    break;
                }

                // Replace original triangle: (a, new, c)
                let poly = &mut self.subsector_leaves[subsector_id].polygons[pi];
                poly.vertices = vec![a, new_vi, c];
                let mut aabb = AABB::new();
                for &vi in &poly.vertices {
                    aabb.expand_to_include_point(self.vertices[vi]);
                }
                poly.aabb = aabb;

                // Add new triangle: (new, b, c)
                let sector_id = self.subsector_leaves[subsector_id].polygons[pi].sector_id;
                let surface_kind = self.subsector_leaves[subsector_id].polygons[pi]
                    .surface_kind
                    .clone();
                let normal = self.subsector_leaves[subsector_id].polygons[pi].normal;
                let moves = self.subsector_leaves[subsector_id].polygons[pi].moves;

                let new_poly = SurfacePolygon::new(
                    sector_id,
                    surface_kind,
                    vec![new_vi, b, c],
                    normal,
                    &self.vertices,
                    moves,
                );
                let new_pi = self.subsector_leaves[subsector_id].polygons.len();
                self.subsector_leaves[subsector_id].polygons.push(new_poly);
                if is_floor {
                    self.subsector_leaves[subsector_id]
                        .floor_polygons
                        .push(new_pi);
                } else {
                    self.subsector_leaves[subsector_id]
                        .ceiling_polygons
                        .push(new_pi);
                }

                break;
            }
        }
    }

    /// Post-creation pass that handles mover vertex concerns:
    /// 1) Identify mover sectors (is_sector_mover + texture-based zh scan)
    /// 2) Split floor/ceiling triangles at boundary segment endpoints
    /// 3) Internal zh sector separation (floor vs ceiling at same z)
    /// 4) Cross-sector boundary separation (shared vertices at zh boundaries)
    /// 5) Zh wall vertex connection via ZhWallRecords
    /// 6) Non-zh wall vertex connection via linedef lookup
    /// 7) Set `moves` flag on affected polygons
    fn mover_vertex_pass(
        &mut self,
        sectors: &[Sector],
        segments: &[Segment],
        subsectors: &[SubSector],
        linedefs: &[LineDef],
    ) {
        // Step 1: Identify all mover sectors and zh boundaries.
        let mut mover_sectors: HashSet<usize> = HashSet::new();
        let mut zh_sectors: HashSet<usize> = HashSet::new();
        let mut zh_lower_bounds: Vec<(Vec2, usize, usize)> = Vec::new();
        let mut zh_upper_bounds: Vec<(Vec2, usize, usize)> = Vec::new();
        let mut zh_lower_sectors: HashSet<usize> = HashSet::new();
        let mut zh_upper_sectors: HashSet<usize> = HashSet::new();

        for (i, sector) in sectors.iter().enumerate() {
            if is_sector_mover(sector, linedefs) {
                mover_sectors.insert(i);
            }
            if (sector.ceilingheight - sector.floorheight).abs() <= HEIGHT_EPSILON {
                zh_sectors.insert(i);
            }
        }

        for seg in segments {
            let back = match &seg.backsector {
                Some(b) => b,
                None => continue,
            };
            let front_id = seg.frontsector.num as usize;
            let back_id = back.num as usize;

            if seg.sidedef.bottomtexture.is_some()
                && (seg.frontsector.floorheight - back.floorheight).abs() <= HEIGHT_EPSILON
            {
                zh_lower_sectors.insert(front_id);
                zh_lower_sectors.insert(back_id);
                mover_sectors.insert(front_id);
                mover_sectors.insert(back_id);
                for sv in [*seg.v1, *seg.v2] {
                    if !zh_lower_bounds.iter().any(|(p, a, b)| {
                        (*p - sv).length() < DEDUP_EPSILON && *a == front_id && *b == back_id
                    }) {
                        zh_lower_bounds.push((sv, front_id, back_id));
                    }
                }
            }

            if seg.sidedef.toptexture.is_some()
                && (seg.frontsector.ceilingheight - back.ceilingheight).abs() <= HEIGHT_EPSILON
            {
                zh_upper_sectors.insert(front_id);
                zh_upper_sectors.insert(back_id);
                mover_sectors.insert(front_id);
                mover_sectors.insert(back_id);
                for sv in [*seg.v1, *seg.v2] {
                    if !zh_upper_bounds.iter().any(|(p, a, b)| {
                        (*p - sv).length() < DEDUP_EPSILON && *a == front_id && *b == back_id
                    }) {
                        zh_upper_bounds.push((sv, front_id, back_id));
                    }
                }
            }
        }

        // Mover-based boundary detection: catch same-height boundaries
        // without texture markers.
        for seg in segments {
            let back = match &seg.backsector {
                Some(b) => b,
                None => continue,
            };
            let front_id = seg.frontsector.num as usize;
            let back_id = back.num as usize;
            let either_mover =
                mover_sectors.contains(&front_id) || mover_sectors.contains(&back_id);
            if !either_mover {
                continue;
            }

            if (seg.frontsector.floorheight - back.floorheight).abs() <= HEIGHT_EPSILON {
                for sv in [*seg.v1, *seg.v2] {
                    if !zh_lower_bounds.iter().any(|(p, a, b)| {
                        (*p - sv).length() < DEDUP_EPSILON && *a == front_id && *b == back_id
                    }) {
                        zh_lower_bounds.push((sv, front_id, back_id));
                    }
                }
            }

            if (seg.frontsector.ceilingheight - back.ceilingheight).abs() <= HEIGHT_EPSILON {
                for sv in [*seg.v1, *seg.v2] {
                    if !zh_upper_bounds.iter().any(|(p, a, b)| {
                        (*p - sv).length() < DEDUP_EPSILON && *a == front_id && *b == back_id
                    }) {
                        zh_upper_bounds.push((sv, front_id, back_id));
                    }
                }
            }
        }

        // Detect floor/ceiling crossings: mover floor at adjacent ceiling
        // height (or vice versa).
        let mut floor_ceil_bounds: Vec<(Vec2, usize, usize)> = Vec::new();
        let mut floor_ceil_sectors: HashSet<usize> = HashSet::new();
        for seg in segments {
            let back = match &seg.backsector {
                Some(b) => b,
                None => continue,
            };
            let front_id = seg.frontsector.num as usize;
            let back_id = back.num as usize;
            let either_mover =
                mover_sectors.contains(&front_id) || mover_sectors.contains(&back_id);
            if !either_mover {
                continue;
            }
            if (seg.frontsector.floorheight - back.ceilingheight).abs() <= HEIGHT_EPSILON {
                floor_ceil_sectors.insert(front_id);
                floor_ceil_sectors.insert(back_id);
                for sv in [*seg.v1, *seg.v2] {
                    if !floor_ceil_bounds.iter().any(|(p, a, b)| {
                        (*p - sv).length() < DEDUP_EPSILON && *a == front_id && *b == back_id
                    }) {
                        floor_ceil_bounds.push((sv, front_id, back_id));
                    }
                }
            }
            if (back.floorheight - seg.frontsector.ceilingheight).abs() <= HEIGHT_EPSILON {
                floor_ceil_sectors.insert(front_id);
                floor_ceil_sectors.insert(back_id);
                for sv in [*seg.v1, *seg.v2] {
                    if !floor_ceil_bounds.iter().any(|(p, a, b)| {
                        (*p - sv).length() < DEDUP_EPSILON && *a == back_id && *b == front_id
                    }) {
                        floor_ceil_bounds.push((sv, back_id, front_id));
                    }
                }
            }
        }

        if mover_sectors.is_empty() && zh_sectors.is_empty() && floor_ceil_bounds.is_empty() {
            return;
        }

        // Build set of zh wall fresh vertex indices.
        let zh_fresh: HashSet<usize> = self
            .zh_wall_records
            .iter()
            .flat_map(|r| r.bottom.iter().chain(r.top.iter()).copied())
            .collect();

        // Step 2: Split floor/ceiling triangles at boundary segment endpoints.
        let all_relevant: HashSet<usize> = zh_lower_sectors
            .union(&zh_upper_sectors)
            .copied()
            .chain(zh_sectors.iter().copied())
            .chain(mover_sectors.iter().copied())
            .chain(floor_ceil_sectors.iter().copied())
            .collect();

        for &sector_id in &all_relevant {
            let mut boundary_pts: Vec<Vec2> = Vec::new();
            for seg in segments {
                let front_id = seg.frontsector.num as usize;
                let back_id = seg.backsector.as_ref().map(|b| b.num as usize);
                if front_id == sector_id || back_id == Some(sector_id) {
                    for sv in [*seg.v1, *seg.v2] {
                        if !boundary_pts
                            .iter()
                            .any(|p| (*p - sv).length() < DEDUP_EPSILON)
                        {
                            boundary_pts.push(sv);
                        }
                    }
                }
                if front_id == sector_id {
                    if let Some(back) = &seg.backsector {
                        let back_num = back.num as usize;
                        for &ss_id in &self.sector_subsectors[back_num] {
                            let ss = &subsectors[ss_id];
                            let start = ss.start_seg as usize;
                            let end = start + ss.seg_count as usize;
                            for gi in start..end {
                                if let Some(gs) = segments.get(gi) {
                                    for sv in [*gs.v1, *gs.v2] {
                                        if !boundary_pts
                                            .iter()
                                            .any(|p| (*p - sv).length() < DEDUP_EPSILON)
                                        {
                                            boundary_pts.push(sv);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if zh_lower_sectors.contains(&sector_id)
                || zh_sectors.contains(&sector_id)
                || mover_sectors.contains(&sector_id)
            {
                let floor_h = sectors[sector_id].floorheight;
                for &ss_id in &self.sector_subsectors[sector_id].clone() {
                    self.split_triangles_at_points(ss_id, true, floor_h, &boundary_pts, &zh_fresh);
                }
            }
            if zh_upper_sectors.contains(&sector_id)
                || zh_sectors.contains(&sector_id)
                || mover_sectors.contains(&sector_id)
            {
                let ceil_h = sectors[sector_id].ceilingheight;
                for &ss_id in &self.sector_subsectors[sector_id].clone() {
                    self.split_triangles_at_points(ss_id, false, ceil_h, &boundary_pts, &zh_fresh);
                }
            }
        }

        // Step 3: Internal zh sector separation.
        for &sector_id in &zh_sectors {
            let floor_vis: HashSet<usize> = self.sector_subsectors[sector_id]
                .iter()
                .flat_map(|&ss_id| {
                    let leaf = &self.subsector_leaves[ss_id];
                    leaf.floor_polygons
                        .iter()
                        .flat_map(|&pi| leaf.polygons[pi].vertices.iter().copied())
                        .collect::<Vec<_>>()
                })
                .collect();

            let mut replaced: HashMap<usize, usize> = HashMap::new();
            for &ss_id in &self.sector_subsectors[sector_id].clone() {
                let ceil_indices = self.subsector_leaves[ss_id].ceiling_polygons.clone();
                for pi in ceil_indices {
                    for vi in &mut self.subsector_leaves[ss_id].polygons[pi].vertices {
                        if floor_vis.contains(vi) {
                            let new_vi = *replaced.entry(*vi).or_insert_with(|| {
                                let idx = self.vertices.len();
                                self.vertices.push(self.vertices[*vi]);
                                idx
                            });
                            *vi = new_vi;
                        }
                    }
                }
            }
        }

        // Step 4: Cross-sector boundary separation.
        let mut lower_vertex_map: HashMap<QuantizedVec2, HashMap<usize, usize>> = HashMap::new();
        let mut upper_vertex_map: HashMap<QuantizedVec2, HashMap<usize, usize>> = HashMap::new();

        for &(pos, sector_a, sector_b) in &zh_lower_bounds {
            let height = sectors[sector_a].floorheight;
            let qp = QuantizedVec2::from_vec2(pos, QUANT_PRECISION);

            let shared_vi = self.find_floor_ceil_vertex_for_sector(sector_a, pos, height, true);
            let Some(shared_vi) = shared_vi else {
                continue;
            };

            lower_vertex_map
                .entry(qp)
                .or_default()
                .entry(sector_a)
                .or_insert(shared_vi);

            let sector_b_uses = self.sector_uses_vertex(sector_b, shared_vi, true);
            if sector_b_uses {
                let (keeper, mover_out) = if mover_sectors.contains(&sector_a) {
                    (sector_a, sector_b)
                } else {
                    (sector_b, sector_a)
                };
                let new_vi = self.vertices.len();
                self.vertices.push(self.vertices[shared_vi]);
                lower_vertex_map
                    .entry(qp)
                    .or_default()
                    .insert(mover_out, new_vi);
                lower_vertex_map
                    .entry(qp)
                    .or_default()
                    .entry(keeper)
                    .or_insert(shared_vi);

                self.replace_vertex_in_sector_polys(mover_out, shared_vi, new_vi, pos, true);
            } else if let Some(vi) =
                self.find_floor_ceil_vertex_for_sector(sector_b, pos, height, true)
            {
                lower_vertex_map
                    .entry(qp)
                    .or_default()
                    .entry(sector_b)
                    .or_insert(vi);
            }
        }

        for &(pos, sector_a, sector_b) in &zh_upper_bounds {
            let height = sectors[sector_a].ceilingheight;
            let qp = QuantizedVec2::from_vec2(pos, QUANT_PRECISION);

            let shared_vi = self.find_floor_ceil_vertex_for_sector(sector_a, pos, height, false);
            let Some(shared_vi) = shared_vi else {
                continue;
            };

            upper_vertex_map
                .entry(qp)
                .or_default()
                .entry(sector_a)
                .or_insert(shared_vi);

            let sector_b_uses = self.sector_uses_vertex(sector_b, shared_vi, false);
            if sector_b_uses {
                let (keeper, mover_out) = if mover_sectors.contains(&sector_a) {
                    (sector_a, sector_b)
                } else {
                    (sector_b, sector_a)
                };
                let new_vi = self.vertices.len();
                self.vertices.push(self.vertices[shared_vi]);
                upper_vertex_map
                    .entry(qp)
                    .or_default()
                    .insert(mover_out, new_vi);
                upper_vertex_map
                    .entry(qp)
                    .or_default()
                    .entry(keeper)
                    .or_insert(shared_vi);

                self.replace_vertex_in_sector_polys(mover_out, shared_vi, new_vi, pos, false);
            } else if let Some(vi) =
                self.find_floor_ceil_vertex_for_sector(sector_b, pos, height, false)
            {
                upper_vertex_map
                    .entry(qp)
                    .or_default()
                    .entry(sector_b)
                    .or_insert(vi);
            }
        }

        // For zh sectors: populate vertex maps with separated floor/ceiling
        // vertex indices for zh wall connection (Step 5).
        for &sector_id in &zh_sectors {
            let height = sectors[sector_id].floorheight;
            for &ss_id in &self.sector_subsectors[sector_id] {
                let leaf = &self.subsector_leaves[ss_id];
                for &pi in &leaf.floor_polygons {
                    for &vi in &leaf.polygons[pi].vertices {
                        let v = self.vertices[vi];
                        if (v.z - height).abs() < HEIGHT_EPSILON {
                            let qp = QuantizedVec2::from_vec2(Vec2::new(v.x, v.y), QUANT_PRECISION);
                            lower_vertex_map
                                .entry(qp)
                                .or_default()
                                .entry(sector_id)
                                .or_insert(vi);
                        }
                    }
                }
                for &pi in &leaf.ceiling_polygons {
                    for &vi in &leaf.polygons[pi].vertices {
                        let v = self.vertices[vi];
                        if (v.z - height).abs() < HEIGHT_EPSILON {
                            let qp = QuantizedVec2::from_vec2(Vec2::new(v.x, v.y), QUANT_PRECISION);
                            upper_vertex_map
                                .entry(qp)
                                .or_default()
                                .entry(sector_id)
                                .or_insert(vi);
                        }
                    }
                }
            }
        }

        // Step 4b: Cross-height separation (floor at ceiling height).
        for &(pos, floor_sector, ceil_sector) in &floor_ceil_bounds {
            let height = sectors[floor_sector].floorheight;
            let qp = QuantizedVec2::from_vec2(pos, QUANT_PRECISION);

            let floor_vi = self.find_floor_ceil_vertex_for_sector(floor_sector, pos, height, true);
            let Some(floor_vi) = floor_vi else {
                continue;
            };

            let ceil_uses = self.sector_uses_vertex(ceil_sector, floor_vi, false);
            if ceil_uses {
                let new_vi = self.vertices.len();
                self.vertices.push(self.vertices[floor_vi]);

                self.replace_vertex_in_sector_polys(ceil_sector, floor_vi, new_vi, pos, false);

                lower_vertex_map
                    .entry(qp)
                    .or_default()
                    .entry(floor_sector)
                    .or_insert(floor_vi);
                upper_vertex_map
                    .entry(qp)
                    .or_default()
                    .insert(ceil_sector, new_vi);
            } else {
                lower_vertex_map
                    .entry(qp)
                    .or_default()
                    .entry(floor_sector)
                    .or_insert(floor_vi);
                if let Some(vi) =
                    self.find_floor_ceil_vertex_for_sector(ceil_sector, pos, height, false)
                {
                    upper_vertex_map
                        .entry(qp)
                        .or_default()
                        .entry(ceil_sector)
                        .or_insert(vi);
                }
            }
        }

        // Step 4c: Residual mover vertex separation.
        {
            let mut upper_pairs: HashSet<(usize, usize)> = HashSet::new();
            for &(_, a, b) in &zh_upper_bounds {
                if a != b {
                    upper_pairs.insert(if a < b { (a, b) } else { (b, a) });
                }
            }
            for (sector_a, sector_b) in &upper_pairs {
                let ceil_vis_a: HashSet<usize> = self.sector_subsectors[*sector_a]
                    .iter()
                    .flat_map(|&ssid| {
                        let leaf = &self.subsector_leaves[ssid];
                        leaf.ceiling_polygons
                            .iter()
                            .flat_map(|&cpi| leaf.polygons[cpi].vertices.iter().copied())
                            .collect::<Vec<_>>()
                    })
                    .collect();
                let shared: Vec<usize> = self.sector_subsectors[*sector_b]
                    .clone()
                    .iter()
                    .flat_map(|&ssid| {
                        let leaf = &self.subsector_leaves[ssid];
                        leaf.ceiling_polygons
                            .iter()
                            .flat_map(|&cpi| leaf.polygons[cpi].vertices.iter().copied())
                            .collect::<Vec<_>>()
                    })
                    .filter(|vi| ceil_vis_a.contains(vi))
                    .collect();
                for vi in shared {
                    let (keeper, other) = if mover_sectors.contains(sector_a) {
                        (*sector_a, *sector_b)
                    } else {
                        (*sector_b, *sector_a)
                    };
                    let pos = self.vertices[vi];
                    let pos2 = Vec2::new(pos.x, pos.y);
                    let new_vi = self.vertices.len();
                    self.vertices.push(pos);
                    self.replace_vertex_in_sector_polys(other, vi, new_vi, pos2, false);
                    let qp = QuantizedVec2::from_vec2(pos2, QUANT_PRECISION);
                    upper_vertex_map
                        .entry(qp)
                        .or_default()
                        .insert(other, new_vi);
                    upper_vertex_map
                        .entry(qp)
                        .or_default()
                        .entry(keeper)
                        .or_insert(vi);
                }
            }

            let mut lower_pairs: HashSet<(usize, usize)> = HashSet::new();
            for &(_, a, b) in &zh_lower_bounds {
                if a != b {
                    lower_pairs.insert(if a < b { (a, b) } else { (b, a) });
                }
            }
            for (sector_a, sector_b) in &lower_pairs {
                let floor_vis_a: HashSet<usize> = self.sector_subsectors[*sector_a]
                    .iter()
                    .flat_map(|&ssid| {
                        let leaf = &self.subsector_leaves[ssid];
                        leaf.floor_polygons
                            .iter()
                            .flat_map(|&fpi| leaf.polygons[fpi].vertices.iter().copied())
                            .collect::<Vec<_>>()
                    })
                    .collect();
                let shared: Vec<usize> = self.sector_subsectors[*sector_b]
                    .clone()
                    .iter()
                    .flat_map(|&ssid| {
                        let leaf = &self.subsector_leaves[ssid];
                        leaf.floor_polygons
                            .iter()
                            .flat_map(|&fpi| leaf.polygons[fpi].vertices.iter().copied())
                            .collect::<Vec<_>>()
                    })
                    .filter(|vi| floor_vis_a.contains(vi))
                    .collect();
                for vi in shared {
                    let (keeper, other) = if mover_sectors.contains(sector_a) {
                        (*sector_a, *sector_b)
                    } else {
                        (*sector_b, *sector_a)
                    };
                    let pos = self.vertices[vi];
                    let pos2 = Vec2::new(pos.x, pos.y);
                    let new_vi = self.vertices.len();
                    self.vertices.push(pos);
                    self.replace_vertex_in_sector_polys(other, vi, new_vi, pos2, true);
                    let qp = QuantizedVec2::from_vec2(pos2, QUANT_PRECISION);
                    lower_vertex_map
                        .entry(qp)
                        .or_default()
                        .insert(other, new_vi);
                    lower_vertex_map
                        .entry(qp)
                        .or_default()
                        .entry(keeper)
                        .or_insert(vi);
                }
            }
        }

        // Step 5: Zh wall vertex connection via ZhWallRecords.
        for rec in &self.zh_wall_records.clone() {
            let leaf = &mut self.subsector_leaves[rec.subsector_id];

            match rec.wall_type {
                WallType::Lower => {
                    for (wall_vi, sector_id, vertex_map) in [
                        (rec.bottom[0], rec.front_sector, &lower_vertex_map),
                        (rec.bottom[1], rec.front_sector, &lower_vertex_map),
                        (rec.top[0], rec.back_sector, &lower_vertex_map),
                        (rec.top[1], rec.back_sector, &lower_vertex_map),
                    ] {
                        let pos = self.vertices[wall_vi];
                        let qp = QuantizedVec2::from_vec2(Vec2::new(pos.x, pos.y), QUANT_PRECISION);
                        if let Some(sector_map) = vertex_map.get(&qp) {
                            if let Some(&target_vi) = sector_map.get(&sector_id) {
                                for &pi in &rec.poly_indices {
                                    for vi in &mut leaf.polygons[pi].vertices {
                                        if *vi == wall_vi {
                                            *vi = target_vi;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                WallType::Upper => {
                    for (wall_vi, sector_id, vertex_map) in [
                        (rec.bottom[0], rec.back_sector, &upper_vertex_map),
                        (rec.bottom[1], rec.back_sector, &upper_vertex_map),
                        (rec.top[0], rec.front_sector, &upper_vertex_map),
                        (rec.top[1], rec.front_sector, &upper_vertex_map),
                    ] {
                        let pos = self.vertices[wall_vi];
                        let qp = QuantizedVec2::from_vec2(Vec2::new(pos.x, pos.y), QUANT_PRECISION);
                        if let Some(sector_map) = vertex_map.get(&qp) {
                            if let Some(&target_vi) = sector_map.get(&sector_id) {
                                for &pi in &rec.poly_indices {
                                    for vi in &mut leaf.polygons[pi].vertices {
                                        if *vi == wall_vi {
                                            *vi = target_vi;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                WallType::Middle => {
                    if rec.front_sector == rec.back_sector {
                        for (wall_vi, vertex_map) in [
                            (rec.bottom[0], &lower_vertex_map),
                            (rec.bottom[1], &lower_vertex_map),
                            (rec.top[0], &upper_vertex_map),
                            (rec.top[1], &upper_vertex_map),
                        ] {
                            let pos = self.vertices[wall_vi];
                            let qp =
                                QuantizedVec2::from_vec2(Vec2::new(pos.x, pos.y), QUANT_PRECISION);
                            if let Some(sector_map) = vertex_map.get(&qp) {
                                if let Some(&target_vi) = sector_map.get(&rec.front_sector) {
                                    for &pi in &rec.poly_indices {
                                        for vi in &mut leaf.polygons[pi].vertices {
                                            if *vi == wall_vi {
                                                *vi = target_vi;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Step 6: Non-zh wall vertex connection.
        for ss_id in 0..self.subsector_leaves.len() {
            let polys_len = self.subsector_leaves[ss_id].polygons.len();
            for pi in 0..polys_len {
                let poly = &self.subsector_leaves[ss_id].polygons[pi];
                let (wall_type, linedef_id) = match &poly.surface_kind {
                    SurfaceKind::Vertical {
                        wall_type,
                        linedef_id,
                        ..
                    } => (*wall_type, *linedef_id),
                    _ => continue,
                };
                let verts = poly.vertices.clone();
                let all_same_z = verts.iter().all(|&vi| {
                    (self.vertices[vi].z - self.vertices[verts[0]].z).abs() <= HEIGHT_EPSILON
                });
                if all_same_z && matches!(wall_type, WallType::Lower | WallType::Upper) {
                    continue;
                }

                let ld = &linedefs[linedef_id];
                let ld_front = ld.frontsector.num as usize;
                let ld_back = match &ld.backsector {
                    Some(b) => b.num as usize,
                    None => continue,
                };
                let wall_front = poly.sector_id;
                let wall_back = if wall_front == ld_front {
                    ld_back
                } else {
                    ld_front
                };

                for vi_idx in 0..verts.len() {
                    let vi = verts[vi_idx];
                    let v = self.vertices[vi];
                    let qp = QuantizedVec2::from_vec2(Vec2::new(v.x, v.y), QUANT_PRECISION);

                    match wall_type {
                        WallType::Lower => {
                            if let Some(sector_map) = lower_vertex_map.get(&qp) {
                                if (v.z - sectors[wall_front].floorheight).abs() < HEIGHT_EPSILON {
                                    if let Some(&target_vi) = sector_map.get(&wall_front) {
                                        if vi != target_vi {
                                            self.subsector_leaves[ss_id].polygons[pi].vertices
                                                [vi_idx] = target_vi;
                                        }
                                    }
                                }
                                if (v.z - sectors[wall_back].floorheight).abs() < HEIGHT_EPSILON {
                                    if let Some(&target_vi) = sector_map.get(&wall_back) {
                                        if vi != target_vi {
                                            self.subsector_leaves[ss_id].polygons[pi].vertices
                                                [vi_idx] = target_vi;
                                        }
                                    }
                                }
                            }
                        }
                        WallType::Upper => {
                            if let Some(sector_map) = upper_vertex_map.get(&qp) {
                                if (v.z - sectors[wall_front].ceilingheight).abs() < HEIGHT_EPSILON
                                {
                                    if let Some(&target_vi) = sector_map.get(&wall_front) {
                                        if vi != target_vi {
                                            self.subsector_leaves[ss_id].polygons[pi].vertices
                                                [vi_idx] = target_vi;
                                        }
                                    }
                                }
                                if (v.z - sectors[wall_back].ceilingheight).abs() < HEIGHT_EPSILON {
                                    if let Some(&target_vi) = sector_map.get(&wall_back) {
                                        if vi != target_vi {
                                            self.subsector_leaves[ss_id].polygons[pi].vertices
                                                [vi_idx] = target_vi;
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // Step 7: Set `moves` flag.
        let floor_movers: HashSet<usize> = mover_sectors
            .union(&zh_lower_sectors)
            .copied()
            .chain(zh_sectors.iter().copied())
            .collect();
        let ceil_movers: HashSet<usize> = mover_sectors
            .union(&zh_upper_sectors)
            .copied()
            .chain(zh_sectors.iter().copied())
            .collect();

        for &sector_id in &floor_movers {
            for &ss_id in &self.sector_subsectors[sector_id].clone() {
                let leaf = &mut self.subsector_leaves[ss_id];
                for &fi in &leaf.floor_polygons.clone() {
                    leaf.polygons[fi].moves = true;
                }
            }
        }
        for &sector_id in &ceil_movers {
            for &ss_id in &self.sector_subsectors[sector_id].clone() {
                let leaf = &mut self.subsector_leaves[ss_id];
                for &ci in &leaf.ceiling_polygons.clone() {
                    leaf.polygons[ci].moves = true;
                }
            }
        }
        // Zh wall polygons.
        for rec in &self.zh_wall_records.clone() {
            let leaf = &mut self.subsector_leaves[rec.subsector_id];
            for &pi in &rec.poly_indices {
                leaf.polygons[pi].moves = true;
            }
        }
        // Non-zh wall polygons at mover boundaries.
        for ss_id in 0..self.subsector_leaves.len() {
            let polys_len = self.subsector_leaves[ss_id].polygons.len();
            for pi in 0..polys_len {
                let poly = &self.subsector_leaves[ss_id].polygons[pi];
                let linedef_id = match &poly.surface_kind {
                    SurfaceKind::Vertical {
                        linedef_id,
                        wall_type,
                        ..
                    } if matches!(wall_type, WallType::Lower | WallType::Upper) => *linedef_id,
                    _ => continue,
                };
                if poly.moves {
                    continue;
                }
                let ld = &linedefs[linedef_id];
                let ld_front = ld.frontsector.num as usize;
                let ld_back = match &ld.backsector {
                    Some(b) => b.num as usize,
                    None => continue,
                };
                if mover_sectors.contains(&ld_front) || mover_sectors.contains(&ld_back) {
                    self.subsector_leaves[ss_id].polygons[pi].moves = true;
                }
            }
        }
    }

    /// Find the vertex index used by a sector's floor or ceiling polygons at
    /// a given 2D position and height.
    fn find_floor_ceil_vertex_for_sector(
        &self,
        sector_id: usize,
        pos: Vec2,
        height: f32,
        is_floor: bool,
    ) -> Option<usize> {
        for &ss_id in &self.sector_subsectors[sector_id] {
            let leaf = &self.subsector_leaves[ss_id];
            let indices = if is_floor {
                &leaf.floor_polygons
            } else {
                &leaf.ceiling_polygons
            };
            for &pi in indices {
                for &vi in &leaf.polygons[pi].vertices {
                    let v = self.vertices[vi];
                    if (v.x - pos.x).abs() < DEDUP_EPSILON
                        && (v.y - pos.y).abs() < DEDUP_EPSILON
                        && (v.z - height).abs() < HEIGHT_EPSILON
                    {
                        return Some(vi);
                    }
                }
            }
        }
        None
    }

    /// Check if a sector's floor or ceiling polygons use a specific vertex
    /// index.
    fn sector_uses_vertex(&self, sector_id: usize, vi: usize, is_floor: bool) -> bool {
        for &ss_id in &self.sector_subsectors[sector_id] {
            let leaf = &self.subsector_leaves[ss_id];
            let indices = if is_floor {
                &leaf.floor_polygons
            } else {
                &leaf.ceiling_polygons
            };
            for &pi in indices {
                if leaf.polygons[pi].vertices.contains(&vi) {
                    return true;
                }
            }
        }
        false
    }

    /// Find an existing vertex at the target position, excluding zh wall
    /// fresh vertices.
    fn find_vertex_at_position(&self, target: Vec3, zh_fresh: &HashSet<usize>) -> Option<usize> {
        for (i, v) in self.vertices.iter().enumerate() {
            if zh_fresh.contains(&i) {
                continue;
            }
            if (v.x - target.x).abs() < DEDUP_EPSILON
                && (v.y - target.y).abs() < DEDUP_EPSILON
                && (v.z - target.z).abs() < HEIGHT_EPSILON
            {
                return Some(i);
            }
        }
        None
    }

    /// Replace a vertex index in a sector's floor/ceiling polygons and
    /// wall polygons at a specific 2D position.
    fn replace_vertex_in_sector_polys(
        &mut self,
        sector_id: usize,
        old_vi: usize,
        new_vi: usize,
        pos: Vec2,
        is_floor: bool,
    ) {
        let height = self.vertices[old_vi].z;
        for &ss_id in &self.sector_subsectors[sector_id].clone() {
            let leaf = &mut self.subsector_leaves[ss_id];
            let fc_indices = if is_floor {
                leaf.floor_polygons.clone()
            } else {
                leaf.ceiling_polygons.clone()
            };
            for pi in fc_indices {
                for vi in &mut leaf.polygons[pi].vertices {
                    if *vi == old_vi {
                        let v = self.vertices[*vi];
                        if (v.x - pos.x).abs() < DEDUP_EPSILON
                            && (v.y - pos.y).abs() < DEDUP_EPSILON
                        {
                            *vi = new_vi;
                        }
                    }
                }
            }
            let poly_count = leaf.polygons.len();
            for pi in 0..poly_count {
                if !matches!(leaf.polygons[pi].surface_kind, SurfaceKind::Vertical { .. }) {
                    continue;
                }
                if leaf.polygons[pi].sector_id != sector_id {
                    continue;
                }
                for vi in &mut leaf.polygons[pi].vertices {
                    if *vi == old_vi {
                        let v = self.vertices[*vi];
                        if (v.x - pos.x).abs() < DEDUP_EPSILON
                            && (v.y - pos.y).abs() < DEDUP_EPSILON
                            && (v.z - height).abs() < HEIGHT_EPSILON
                        {
                            *vi = new_vi;
                        }
                    }
                }
            }
        }
    }

    /// Split floor or ceiling triangles of a subsector at boundary points.
    fn split_triangles_at_points(
        &mut self,
        subsector_id: usize,
        is_floor: bool,
        height: f32,
        boundary_pts: &[Vec2],
        zh_fresh: &HashSet<usize>,
    ) {
        for pt in boundary_pts {
            let current_indices = if is_floor {
                self.subsector_leaves[subsector_id].floor_polygons.clone()
            } else {
                self.subsector_leaves[subsector_id].ceiling_polygons.clone()
            };

            for &pi in &current_indices {
                let verts = self.subsector_leaves[subsector_id].polygons[pi]
                    .vertices
                    .clone();
                if verts.len() != 3 {
                    continue;
                }

                let matching_idx = verts.iter().position(|&vi| {
                    let v = self.vertices[vi];
                    (v.x - pt.x).abs() < DEDUP_EPSILON && (v.y - pt.y).abs() < DEDUP_EPSILON
                });
                if let Some(idx) = matching_idx {
                    // Position already present — ensure index is shared with
                    // wall vertices (expand_polygon may have created a
                    // duplicate index at the same position).
                    let current_vi = verts[idx];
                    let target = Vec3::new(pt.x, pt.y, height);
                    if let Some(shared_vi) = self.find_vertex_at_position(target, zh_fresh) {
                        if shared_vi != current_vi {
                            self.subsector_leaves[subsector_id].polygons[pi].vertices[idx] =
                                shared_vi;
                        }
                    }
                    continue;
                }

                let mut split_edge = None;
                for e in 0..3 {
                    let ei0 = verts[e];
                    let ei1 = verts[(e + 1) % 3];
                    let e0 = Vec2::new(self.vertices[ei0].x, self.vertices[ei0].y);
                    let e1 = Vec2::new(self.vertices[ei1].x, self.vertices[ei1].y);
                    let edge_vec = e1 - e0;
                    let edge_len_sq = edge_vec.length_squared();
                    if edge_len_sq < EDGE_EPSILON * EDGE_EPSILON {
                        continue;
                    }
                    let to_v = *pt - e0;
                    let proj = to_v.dot(edge_vec) / edge_len_sq;
                    if proj > EDGE_EPSILON && proj < 1.0 - EDGE_EPSILON {
                        let projected = e0 + edge_vec * proj;
                        if (*pt - projected).length() < EDGE_EPSILON {
                            split_edge = Some(e);
                            break;
                        }
                    }
                }

                if let Some(e) = split_edge {
                    let target = Vec3::new(pt.x, pt.y, height);
                    let new_vi = self
                        .find_vertex_at_position(target, zh_fresh)
                        .unwrap_or_else(|| {
                            let idx = self.vertices.len();
                            self.vertices.push(target);
                            idx
                        });

                    let a = verts[e];
                    let b = verts[(e + 1) % 3];
                    let c = verts[(e + 2) % 3];

                    let poly = &mut self.subsector_leaves[subsector_id].polygons[pi];
                    poly.vertices = vec![a, new_vi, c];
                    let mut aabb = AABB::new();
                    for &vi in &poly.vertices {
                        aabb.expand_to_include_point(self.vertices[vi]);
                    }
                    poly.aabb = aabb;

                    let sector_id = self.subsector_leaves[subsector_id].polygons[pi].sector_id;
                    let surface_kind = self.subsector_leaves[subsector_id].polygons[pi]
                        .surface_kind
                        .clone();
                    let normal = self.subsector_leaves[subsector_id].polygons[pi].normal;
                    let moves = self.subsector_leaves[subsector_id].polygons[pi].moves;

                    let new_poly = SurfacePolygon::new(
                        sector_id,
                        surface_kind,
                        vec![new_vi, b, c],
                        normal,
                        &self.vertices,
                        moves,
                    );
                    let new_pi = self.subsector_leaves[subsector_id].polygons.len();
                    self.subsector_leaves[subsector_id].polygons.push(new_poly);
                    if is_floor {
                        self.subsector_leaves[subsector_id]
                            .floor_polygons
                            .push(new_pi);
                    } else {
                        self.subsector_leaves[subsector_id]
                            .ceiling_polygons
                            .push(new_pi);
                    }

                    break;
                }
            }
        }
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
        if has_valid_aabb {
            self.nodes[node_idx].aabb = combined_aabb;
        }
    }

    /// Expand leaf and node AABBs for mover sectors and zero-height sectors
    /// (doors) to cover the full movement range.
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
            zh_wall_records: Vec::new(),
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
