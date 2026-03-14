#[cfg(feature = "hprof")]
use coarse_prof::profile;

use super::carve::{DivLine, carve_subsector_polygon};
use crate::flags::LineDefFlags;
use crate::map_defs::{LineDef, Node, Sector, Segment, SubSector, is_subsector, subsector_index};
use glam::{Vec2, Vec3};
use std::collections::HashMap;
/// Quantization grid for position-only vertex dedup.
pub(crate) const QUANT_PRECISION: f32 = 1.0;
pub(crate) const HEIGHT_EPSILON: f32 = 0.1;
/// Minimum cross-product magnitude for a non-degenerate triangle.
const MIN_TRI_CROSS: f32 = 1e-4;
/// Rotation applied to horizontal surface texture coordinates (90°).
const HORIZONTAL_TEX_DIRECTION: f32 = std::f32::consts::FRAC_PI_2;

/// Construction-only record tracking zero-height wall vertex roles.
/// Needed because zh walls have bottom and top at the same (x,y,z) — with
/// position-only dedup they'd share one index, producing degenerate triangles.
/// Fresh vertices are created instead, and this record tells the post-pass
/// which vertices are bottom (front sector) vs top (back sector).
#[derive(Clone)]
pub(crate) struct ZhWallRecord {
    /// Subsector leaf containing the wall polygons.
    pub(crate) subsector_id: usize,
    /// Polygon index within the subsector leaf (single quad).
    pub(crate) poly_index: usize,
    /// Vertex indices for the bottom edge [start, end].
    pub(crate) bottom: [usize; 2],
    /// Vertex indices for the top edge [start, end].
    pub(crate) top: [usize; 2],
    /// Wall type (Upper/Lower/Middle).
    pub(crate) wall_type: WallType,
    /// Front sector of the segment.
    pub(crate) front_sector: usize,
    /// Back sector of the segment.
    pub(crate) back_sector: usize,
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
/// Remove near-duplicate consecutive vertices from a carved 2D polygon.
///
/// BSP divline intersections can land within QUANT_PRECISION of segment
/// endpoints. After 3D vertex dedup these map to the same index, producing
/// zero-length edges or collinear fold-back slivers. Removing the duplicate
/// at the 2D stage avoids the problem without misaligning wall edges.
fn dedup_carved_polygon(polygon: &[Vec2]) -> Vec<Vec2> {
    if polygon.len() < 3 {
        return polygon.to_vec();
    }
    let mut out = Vec::with_capacity(polygon.len());
    for i in 0..polygon.len() {
        let next = &polygon[(i + 1) % polygon.len()];
        if (polygon[i] - *next).length_squared() >= QUANT_PRECISION * QUANT_PRECISION {
            out.push(polygon[i]);
        }
    }
    out
}

/// Check whether a vertex index list contains any duplicates.
fn has_duplicate_indices(indices: &[usize]) -> bool {
    for i in 0..indices.len() {
        for j in (i + 1)..indices.len() {
            if indices[i] == indices[j] {
                return true;
            }
        }
    }
    false
}

/// Compute shoelace signed area from vertex indices in XY.
fn vertex_shoelace(indices: &[usize], vertices: &[Vec3]) -> f32 {
    let n = indices.len();
    (0..n)
        .map(|i| {
            let a = vertices[indices[i]];
            let b = vertices[indices[(i + 1) % n]];
            a.x * b.y - b.x * a.y
        })
        .sum()
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
        // Only recompute normal for moving walls — their shape changes as
        // vertices shift vertically. Floor/ceiling normals are always ±Z and
        // don't change with movement, and the cross product of the first two
        // edges can disagree with the polygon winding for non-convex vertex
        // orderings.
        let computed_normal =
            if self.moves && matches!(self.surface_kind, SurfaceKind::Vertical { .. }) {
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

pub struct BSP3D {
    nodes: Vec<Node3D>,
    pub subsector_leaves: Vec<BSPLeaf3D>,
    pub(crate) root_node: u32,
    pub vertices: Vec<Vec3>,
    pub sector_subsectors: Vec<Vec<usize>>,
    /// Carved 2D convex polygon for each subsector, indexed by subsector ID.
    /// Empty vec for degenerate subsectors that produce no valid polygon.
    pub carved_polygons: Vec<Vec<Vec2>>,
    /// Maps linedef_id → [(subsector_id, polygon_idx)] for wall texture
    /// updates.
    linedef_wall_polygons: HashMap<usize, Vec<(usize, usize)>>,
    /// Temporary: zh wall records used during construction only.
    pub(crate) zh_wall_records: Vec<ZhWallRecord>,
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
        let mut divlines: Vec<DivLine> = Vec::new();
        bsp3d.carve_polygons_recursive(
            nodes,
            subsectors,
            segments,
            linedefs,
            corrected_divlines,
            bsp3d.root_node,
            &mut divlines,
            &mut vertex_map,
            sky_num,
        );

        // Phase 1b: Create floor/ceiling N-gons from carved polygons.
        bsp3d.create_all_floor_ceiling_polygons(subsectors, &mut vertex_map, sky_num);

        // Phase 2: Mover vertex pass — separate shared vertices at
        // zero-height boundaries, connect wall vertices, set moves flags.
        bsp3d.mover_vertex_pass(sectors, segments, subsectors, linedefs);

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
        for i in 0..self.sector_subsectors[sector_id].len() {
            let subsector_id = self.sector_subsectors[sector_id][i];
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
        let Some(n) = self.linedef_wall_polygons.get(&linedef_id).map(|e| e.len()) else {
            return;
        };
        for i in 0..n {
            let (subsector_id, poly_idx) = self.linedef_wall_polygons[&linedef_id][i];
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
        let min_z = sectors
            .iter()
            .map(|s| s.floorheight)
            .fold(f32::INFINITY, f32::min);
        let max_z = sectors
            .iter()
            .map(|s| s.ceilingheight)
            .fold(f32::NEG_INFINITY, f32::max);
        self.nodes = nodes
            .iter()
            .map(|node| Node3D {
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
        divlines: &mut Vec<DivLine>,
        vertex_map: &mut HashMap<QuantizedVec3, usize>,
        sky_num: Option<usize>,
    ) {
        #[cfg(feature = "hprof")]
        profile!("carve_polygons_recursive");

        if is_subsector(node_id) {
            self.process_subsector_node(
                subsectors, segments, node_id, divlines, vertex_map, sky_num,
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
            subsector_index(node_id)
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
                self.carved_polygons[subsector_id] = polygon;
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
        let front_sky_ceil = sky_num.is_some_and(|sky| front_sector.ceilingpic == sky);
        let both_sky_ceil =
            sky_num.is_some_and(|sky| front_sky_ceil && back_sector.ceilingpic == sky);
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

        let quad = SurfacePolygon::new(
            sector_id,
            surface_kind,
            vec![bottom_start, bottom_end, top_end, top_start],
            normal,
            &self.vertices,
            false,
        );
        let pi = self.subsector_leaves[subsector_id].polygons.len();
        self.subsector_leaves[subsector_id].polygons.push(quad);

        if is_zero_height {
            if let Some(back_id) = back_sector_id {
                self.zh_wall_records.push(ZhWallRecord {
                    subsector_id,
                    poly_index: pi,
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
        divlines: &mut Vec<DivLine>,
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

            divlines.push(node_divline);
            self.carve_polygons_recursive(
                nodes,
                subsectors,
                segments,
                linedefs,
                corrected_divlines,
                node.children[0],
                divlines,
                vertex_map,
                sky_num,
            );
            divlines.pop();

            let mut reversed = node_divline;
            reversed.dx = -reversed.dx;
            reversed.dy = -reversed.dy;
            divlines.push(reversed);
            self.carve_polygons_recursive(
                nodes,
                subsectors,
                segments,
                linedefs,
                corrected_divlines,
                node.children[1],
                divlines,
                vertex_map,
                sky_num,
            );
            divlines.pop();
        }
    }

    /// Create floor/ceiling N-gons for all subsectors from carved polygons.
    ///
    /// Removes near-duplicate consecutive vertices (within QUANT_PRECISION)
    /// before creating 3D polygons. BSP divline intersections can land within
    /// quantization distance of segment endpoints, producing zero-length
    /// edges or collinear slivers after vertex dedup.
    fn create_all_floor_ceiling_polygons(
        &mut self,
        subsectors: &[SubSector],
        vertex_map: &mut HashMap<QuantizedVec3, usize>,
        sky_num: Option<usize>,
    ) {
        for ssid in 0..subsectors.len() {
            let polygon = dedup_carved_polygon(&self.carved_polygons[ssid]);
            self.create_floor_ceiling_polygons(
                ssid,
                &subsectors[ssid],
                &polygon,
                vertex_map,
                sky_num,
            );
        }
    }

    /// Create a single floor and ceiling N-gon polygon for one subsector.
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

        let skip_floor = false; // sky_num.is_some_and(|sky| subsector.sector.floorpic == sky);
        let skip_ceil = false; // sky_num.is_some_and(|sky| subsector.sector.ceilingpic == sky);

        // Degenerate check via shoelace area.
        let shoelace: f32 = polygon
            .windows(2)
            .map(|w| w[0].x * w[1].y - w[1].x * w[0].y)
            .sum::<f32>()
            + polygon.last().unwrap().x * polygon[0].y
            - polygon[0].x * polygon.last().unwrap().y;
        if shoelace.abs() < MIN_TRI_CROSS {
            return;
        }

        // Carved polygons have negative shoelace (CCW in Doom's Y-down space).
        // Floor needs upward normal (0,0,1): reverse vertex order.
        // Ceiling needs downward normal (0,0,-1): keep original order.

        if !skip_floor {
            let fv: Vec<usize> = polygon
                .iter()
                .rev()
                .map(|v| self.vertex_add(Vec3::new(v.x, v.y, floor_h), vertex_map))
                .collect();
            if fv.len() >= 3
                && !has_duplicate_indices(&fv)
                && vertex_shoelace(&fv, &self.vertices) > 0.0
            {
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
            let cv: Vec<usize> = polygon
                .iter()
                .map(|v| self.vertex_add(Vec3::new(v.x, v.y, ceil_h), vertex_map))
                .collect();
            if cv.len() < 3
                || has_duplicate_indices(&cv)
                || vertex_shoelace(&cv, &self.vertices) >= 0.0
            {
                return;
            }
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

    /// Compute the AABB for a single subsector leaf from its polygon vertices.
    /// Floor/ceiling polygon indices point into the same `polygons` vec, so
    /// iterating `polygons` once covers all surfaces.
    fn compute_leaf_aabb(&self, subsector_id: usize) -> AABB {
        let mut aabb = AABB::new();
        for polygon in &self.subsector_leaves[subsector_id].polygons {
            for &vertex_idx in &polygon.vertices {
                aabb.expand_to_include_point(self.vertices[vertex_idx]);
            }
        }
        aabb
    }

    /// Recompute AABBs for all subsectors in a sector.
    fn update_affected_aabbs(&mut self, sector_id: usize) {
        for i in 0..self.sector_subsectors[sector_id].len() {
            let subsector_id = self.sector_subsectors[sector_id][i];
            let aabb = self.compute_leaf_aabb(subsector_id);
            self.subsector_leaves[subsector_id].aabb = aabb;
        }
    }

    fn update_all_aabbs(&mut self) {
        for subsector_id in 0..self.subsector_leaves.len() {
            let aabb = self.compute_leaf_aabb(subsector_id);
            self.subsector_leaves[subsector_id].aabb = aabb;
        }
        self.update_node_aabbs_recursive(self.root_node);
    }

    fn create_horizontal_surface_kind(&self, texture: usize) -> SurfaceKind {
        SurfaceKind::Horizontal {
            texture,
            tex_cos: HORIZONTAL_TEX_DIRECTION.cos(),
            tex_sin: HORIZONTAL_TEX_DIRECTION.sin(),
        }
    }

    pub(crate) fn update_node_aabbs_recursive(&mut self, node_id: u32) {
        if is_subsector(node_id) {
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
            if is_subsector(child_id) {
                let subsector_id = subsector_index(child_id);
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

    pub fn get_node_aabb(&self, node_id: u32) -> Option<&AABB> {
        if is_subsector(node_id) {
            let subsector_id = subsector_index(node_id);
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
