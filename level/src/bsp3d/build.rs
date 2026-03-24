#[cfg(feature = "hprof")]
use coarse_prof::profile;

use crate::flags::LineDefFlags;
use crate::map_defs::{LineDef, Node, Sector, Segment, SubSector, is_subsector, subsector_index};
use glam::{Vec2, Vec3};
use std::collections::HashMap;
use std::f32::consts::FRAC_PI_2;
/// Quantization grid for position-only vertex dedup.
/// Vertex deduplication grid cell size. rbsp already deduplicates at 1e-5,
/// so this only needs to catch floating-point drift from f64→f32 conversion.
pub(crate) const QUANT_PRECISION: f32 = 0.001;
pub(crate) const HEIGHT_EPSILON: f32 = 0.1;
/// Minimum cross-product magnitude for a non-degenerate triangle.
const MIN_TRI_CROSS: f32 = 1e-4;
/// Rotation applied to horizontal surface texture coordinates (90°).
const HORIZONTAL_TEX_DIRECTION: f32 = FRAC_PI_2;

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
        /// BOOM linedef special 260: translucent middle texture
        translucent: bool,
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
pub(super) struct QuantizedVec3 {
    x: i32,
    y: i32,
    z: i32,
}

impl QuantizedVec3 {
    pub(super) fn from_vec3(v: Vec3, precision: f32) -> Self {
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
    /// Build a complete 3D BSP from the 2D map data.
    ///
    /// - Converts 2D BSP nodes to 3D with vertical extents
    /// - Carves convex subsector polygons via Sutherland-Hodgman clipping
    /// - Creates wall quads, floor/ceiling N-gons, and sky filler geometry
    /// - Runs the mover vertex pass for zero-height boundary sectors
    /// - Builds linedef-to-wall-polygon index for runtime texture updates
    pub fn new(
        root_node: u32,
        nodes: &[Node],
        subsectors: &[SubSector],
        segments: &[Segment],
        sectors: &[Sector],
        linedefs: &[LineDef],
        pre_carved: Vec<Vec<Vec2>>,
        sky_num: Option<usize>,
        sky_pic: Option<usize>,
    ) -> Self {
        #[cfg(feature = "hprof")]
        profile!("BSP3D::new");

        let mut bsp3d = Self {
            nodes: Vec::new(),
            subsector_leaves: Vec::new(),
            root_node,
            vertices: Vec::new(),
            sector_subsectors: vec![Vec::new(); sectors.len()],
            carved_polygons: pre_carved,
            linedef_wall_polygons: HashMap::new(),
            zh_wall_records: Vec::new(),
        };

        let mut vertex_map: HashMap<QuantizedVec3, usize> =
            HashMap::with_capacity(segments.len() * 2);

        bsp3d.initialize_nodes(nodes, sectors);
        bsp3d.initialize_subsectors(subsectors);
        bsp3d.build_sector_subsector_mapping(subsectors, segments, sectors);
        bsp3d.subsector_leaves = vec![BSPLeaf3D::default(); subsectors.len()];

        // Create walls from segments (data already fully corrected).
        for (ss_id, subsector) in subsectors.iter().enumerate() {
            let start_seg = subsector.start_seg as usize;
            let end_seg = start_seg + subsector.seg_count as usize;

            bsp3d.subsector_leaves[ss_id].sector_id = subsector.sector.num as usize;

            if let Some(_) = segments.get(start_seg..end_seg) {
                for seg_idx in start_seg..end_seg {
                    let segment = &segments[seg_idx];
                    let front_sector = &segment.frontsector;
                    let sv1 = segment.v1.pos;
                    let sv2 = segment.v2.pos;
                    bsp3d.subsector_leaves[ss_id]
                        .occlusion_segs
                        .push(OcclusionSeg {
                            v1: sv1,
                            v2: sv2,
                            front_sector_id: front_sector.num as usize,
                            back_sector_id: segment.backsector.as_ref().map(|s| s.num as usize),
                            seg_angle_rad: (sv2.y - sv1.y).atan2(sv2.x - sv1.x),
                        });

                    if let Some(back_sector) = &segment.backsector {
                        bsp3d.create_two_sided_walls(
                            segment,
                            front_sector,
                            back_sector,
                            ss_id,
                            &mut vertex_map,
                            sky_num,
                        );
                    } else {
                        bsp3d.create_one_sided_wall(segment, front_sector, ss_id, &mut vertex_map);
                    }
                }
            }
        }

        // Precompute per-sector sky bounds from adjacent sky sectors.
        let sky_bounds = sky_num.map(|sn| Self::compute_sky_bounds(sectors, sn));
        let sky_max_ceil = sky_bounds.as_ref().map(|(c, _)| c.as_slice());
        let sky_min_floor = sky_bounds.as_ref().map(|(_, f)| f.as_slice());

        // Phase 1b: Create floor/ceiling N-gons from carved polygons.
        bsp3d.create_all_floor_ceiling_polygons(
            subsectors,
            &mut vertex_map,
            sky_num,
            sky_max_ceil,
            sky_min_floor,
        );

        // Phase 2: Mover vertex pass — separate shared vertices at
        // zero-height boundaries, connect wall vertices, set moves flags.
        bsp3d.mover_vertex_pass(sectors, segments, subsectors, linedefs);

        // Phase 3: Sky filler — extend perimeter walls of sky sectors
        // up to max adjacent sky ceiling / down to min adjacent sky floor.
        if let (Some(sn), Some(sp)) = (sky_num, sky_pic) {
            bsp3d.sky_filler_pass(
                sectors,
                subsectors,
                segments,
                sn,
                sp,
                &mut vertex_map,
                sky_max_ceil.unwrap(),
                sky_min_floor.unwrap(),
            );
        }

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

    /// Move all vertices of a sector's floor or ceiling polygons to
    /// `new_height` and update the horizontal surface texture.
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

    /// Apply interpolated sector heights to BSP3D vertices for smooth
    /// rendering. Called before each frame render with the sub-tic
    /// fraction. Saves true post-tic values into `interp_*` fields so they
    /// can be restored after rendering via `restore_sector_state()`.
    pub fn apply_interpolated_heights(&mut self, sectors: &mut [Sector], frac: f32) {
        for (sector_id, sector) in sectors.iter_mut().enumerate() {
            if sector_id >= self.sector_subsectors.len() {
                break;
            }
            // Save true post-tic values before overwriting
            sector.interp_floorheight = sector.floorheight;
            sector.interp_ceilingheight = sector.ceilingheight;
            sector.interp_lightlevel = sector.lightlevel;

            let prev_floor = sector.prev_floorheight.to_f32();
            let curr_floor = sector.floorheight.to_f32();
            if prev_floor != curr_floor {
                let h = prev_floor + (curr_floor - prev_floor) * frac;
                self.set_surface_height(sector_id, MovementType::Floor, h);
                sector.floorheight = math::FixedT::from_f32(h);
            }

            let prev_ceil = sector.prev_ceilingheight.to_f32();
            let curr_ceil = sector.ceilingheight.to_f32();
            if prev_ceil != curr_ceil {
                let h = prev_ceil + (curr_ceil - prev_ceil) * frac;
                self.set_surface_height(sector_id, MovementType::Ceiling, h);
                sector.ceilingheight = math::FixedT::from_f32(h);
            }

            if sector.prev_lightlevel != sector.lightlevel {
                let l = sector.prev_lightlevel as f32
                    + (sector.lightlevel as f32 - sector.prev_lightlevel as f32) * frac;
                sector.lightlevel = l.round() as usize;
            }
        }
    }

    /// Restore true post-tic sector values and vertex Z after rendering.
    pub fn restore_sector_state(&mut self, sectors: &mut [Sector]) {
        for (sector_id, sector) in sectors.iter_mut().enumerate() {
            if sector_id >= self.sector_subsectors.len() {
                break;
            }
            let floor_changed = sector.floorheight != sector.interp_floorheight;
            let ceil_changed = sector.ceilingheight != sector.interp_ceilingheight;

            sector.floorheight = sector.interp_floorheight;
            sector.ceilingheight = sector.interp_ceilingheight;
            sector.lightlevel = sector.interp_lightlevel;

            if floor_changed {
                self.set_surface_height(
                    sector_id,
                    MovementType::Floor,
                    sector.floorheight.to_f32(),
                );
            }
            if ceil_changed {
                self.set_surface_height(
                    sector_id,
                    MovementType::Ceiling,
                    sector.ceilingheight.to_f32(),
                );
            }
        }
    }

    /// Set vertex Z for all polygons of a surface type in a sector (no texture
    /// update).
    fn set_surface_height(&mut self, sector_id: usize, movement_type: MovementType, height: f32) {
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
                    self.vertices[vertex_idx].z = height;
                }
            }
        }
    }

    /// Replace the floor texture for all subsector polygons in a sector.
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

    /// Index all wall polygons by linedef ID for O(1) texture update lookups.
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

    /// Convert 2D BSP nodes to 3D, extending vertical bounds to the global
    /// floor/ceiling range.
    fn initialize_nodes(&mut self, nodes: &[Node], sectors: &[Sector]) {
        let min_z = sectors
            .iter()
            .map(|s| s.floorheight.to_f32())
            .fold(f32::INFINITY, f32::min);
        let max_z = sectors
            .iter()
            .map(|s| s.ceilingheight.to_f32())
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

    /// Build the sector-to-subsector reverse mapping from segment front
    /// sectors.
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

    /// Create upper, lower, and middle wall quads for a two-sided segment.
    /// Suppresses walls between matching sky surfaces (Doom sky hack).
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
        // below front ceiling (includes zero-height for movers). Suppressed
        // when both sectors have sky ceilings.
        if !both_sky_ceil {
            if let Some(texture) = segment.sidedef.toptexture {
                if back_sector.ceilingheight <= front_sector.ceilingheight {
                    self.add_wall_quad(
                        segment,
                        back_sector.ceilingheight.to_f32(),
                        front_sector.ceilingheight.to_f32(),
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
        // above front floor (includes zero-height for movers). Suppressed
        // when both sectors have sky floors.
        if !both_sky_floor {
            if let Some(texture) = segment.sidedef.bottomtexture {
                if back_sector.floorheight >= front_sector.floorheight {
                    self.add_wall_quad(
                        segment,
                        front_sector.floorheight.to_f32(),
                        back_sector.floorheight.to_f32(),
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
            let bottom = front_sector
                .floorheight
                .to_f32()
                .max(back_sector.floorheight.to_f32());
            let top = front_sector
                .ceilingheight
                .to_f32()
                .min(back_sector.ceilingheight.to_f32());
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

    /// Create a middle wall quad for a one-sided segment. Zero-height sectors
    /// (doors) get fresh vertices and a `ZhWallRecord` for the mover pass.
    fn create_one_sided_wall(
        &mut self,
        segment: &Segment,
        front_sector: &Sector,
        front_subsector_id: usize,
        vertex_map: &mut HashMap<QuantizedVec3, usize>,
    ) {
        if let Some(texture) = segment.sidedef.midtexture {
            let front_id = front_sector.num as usize;
            let is_zh = (front_sector.ceilingheight.to_f32() - front_sector.floorheight.to_f32())
                .abs()
                <= HEIGHT_EPSILON;
            // For zh sectors (doors): pass self as back_sector so add_wall_quad
            // creates fresh vertices and a ZhWallRecord. The mover pass
            // connects bottom → floor vertex, top → ceiling vertex.
            let back_sector_id = if is_zh { Some(front_id) } else { None };
            self.add_wall_quad(
                segment,
                front_sector.floorheight.to_f32(),
                front_sector.ceilingheight.to_f32(),
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
        let start_pos = segment.v1.pos;
        let end_pos = segment.v2.pos;
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
            tex_x_offset: (segment.sidedef.textureoffset + segment.offset).into(),
            tex_y_offset: segment.sidedef.rowoffset.into(),
            texture_direction,
            wall_type,
            wall_tex_pin: WallTexPin::from(segment.linedef.flags),
            front_ceiling_z: segment.frontsector.ceilingheight.to_f32(),
            two_sided,
            linedef_id: segment.linedef.num,
            translucent: segment.linedef.special == 260,
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

    /// Compute global sky bounds for the level.
    /// Returns (sky_max_ceil, sky_min_floor) indexed by sector id.
    /// All sky-ceiling sectors get the global max sky ceiling height.
    /// All sky-floor sectors get the global min sky floor height.
    fn compute_sky_bounds(sectors: &[Sector], sky_num: usize) -> (Vec<f32>, Vec<f32>) {
        let global_max_ceil = sectors
            .iter()
            .map(|s| s.ceilingheight.to_f32())
            .fold(f32::NEG_INFINITY, f32::max);
        let global_min_floor = sectors
            .iter()
            .map(|s| s.floorheight.to_f32())
            .fold(f32::INFINITY, f32::min);

        let max_ceil: Vec<f32> = sectors
            .iter()
            .map(|s| {
                if s.ceilingpic == sky_num {
                    global_max_ceil
                } else {
                    s.ceilingheight.to_f32()
                }
            })
            .collect();
        let min_floor: Vec<f32> = sectors
            .iter()
            .map(|s| {
                if s.floorpic == sky_num {
                    global_min_floor
                } else {
                    s.floorheight.to_f32()
                }
            })
            .collect();

        (max_ceil, min_floor)
    }

    /// Create sky-textured filler walls on perimeter walls of sky sectors.
    /// Upper filler extends above sky-ceiling perimeter walls to max_ceil.
    /// Lower filler extends below sky-floor perimeter walls to min_floor.
    fn sky_filler_pass(
        &mut self,
        sectors: &[Sector],
        subsectors: &[SubSector],
        segments: &[Segment],
        sky_num: usize,
        sky_pic: usize,
        vertex_map: &mut HashMap<QuantizedVec3, usize>,
        sky_max_ceil: &[f32],
        sky_min_floor: &[f32],
    ) {
        for sector in sectors.iter() {
            let sector_id = sector.num as usize;
            let is_sky_ceil = sector.ceilingpic == sky_num;
            let is_sky_floor = sector.floorpic == sky_num;
            if !is_sky_ceil && !is_sky_floor {
                continue;
            }

            let sky_ceil = sector.ceilingheight.to_f32();
            let sky_floor = sector.floorheight.to_f32();
            let max_h = sky_max_ceil[sector_id];
            let min_h = sky_min_floor[sector_id];
            let needs_ceil_filler = is_sky_ceil && max_h > sky_ceil;
            let needs_floor_filler = is_sky_floor && min_h < sky_floor;

            if !needs_ceil_filler && !needs_floor_filler {
                continue;
            }

            let ss_ids: Vec<usize> = self.sector_subsectors[sector_id].clone();

            for &ss_id in &ss_ids {
                let ss = &subsectors[ss_id];
                let start = ss.start_seg as usize;
                let end = start + ss.seg_count as usize;
                for seg in segments[start..end].iter() {
                    // Only perimeter segments: skip interior (same-sector)
                    // and sky-to-sky boundaries.
                    let is_perimeter_ceil = match seg.backsector {
                        Some(ref back) => {
                            back.num != seg.frontsector.num && back.ceilingpic != sky_num
                        }
                        None => true,
                    };
                    let is_perimeter_floor = match seg.backsector {
                        Some(ref back) => {
                            back.num != seg.frontsector.num && back.floorpic != sky_num
                        }
                        None => true,
                    };

                    // Skip if the existing wall already reaches the target.
                    if needs_ceil_filler && is_perimeter_ceil && sky_ceil < max_h {
                        self.add_wall_quad(
                            seg,
                            sky_ceil,
                            max_h,
                            WallType::Upper,
                            sky_pic,
                            sector_id,
                            seg.backsector.is_some(),
                            ss_id,
                            None,
                            vertex_map,
                        );
                    }
                    if needs_floor_filler && is_perimeter_floor && min_h < sky_floor {
                        self.add_wall_quad(
                            seg,
                            min_h,
                            sky_floor,
                            WallType::Lower,
                            sky_pic,
                            sector_id,
                            seg.backsector.is_some(),
                            ss_id,
                            None,
                            vertex_map,
                        );
                    }
                }
            }
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
        sky_max_ceil: Option<&[f32]>,
        sky_min_floor: Option<&[f32]>,
    ) {
        for ssid in 0..subsectors.len() {
            let polygon = self.carved_polygons[ssid].clone();
            self.create_floor_ceiling_polygons(
                ssid,
                &subsectors[ssid],
                &polygon,
                vertex_map,
                sky_num,
                sky_max_ceil,
                sky_min_floor,
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
        _sky_num: Option<usize>,
        sky_max_ceil: Option<&[f32]>,
        sky_min_floor: Option<&[f32]>,
    ) {
        if polygon.len() < 3 {
            return;
        }

        let sector_num = subsector.sector.num as usize;
        let base_floor_h = subsector.sector.floorheight.to_f32();
        let base_ceil_h = subsector.sector.ceilingheight.to_f32();
        // For sky surfaces, use precomputed bounds so the sky polygon matches
        // the filler walls.
        let is_sky_ceil = _sky_num.is_some_and(|sky| subsector.sector.ceilingpic == sky);
        let is_sky_floor = _sky_num.is_some_and(|sky| subsector.sector.floorpic == sky);
        let ceil_h = if is_sky_ceil {
            sky_max_ceil
                .map(|smc| smc[sector_num])
                .unwrap_or(base_ceil_h)
        } else {
            base_ceil_h
        };
        let floor_h = if is_sky_floor {
            sky_min_floor
                .map(|smf| smf[sector_num])
                .unwrap_or(base_floor_h)
        } else {
            base_floor_h
        };

        let skip_ceil = is_sky_ceil;
        let skip_floor = is_sky_floor;

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

        // Floor needs upward normal (0,0,1) → positive 3D shoelace.
        // Ceiling needs downward normal (0,0,-1) → negative 3D shoelace.
        // Input polygon winding determines whether we reverse or not:
        //   negative 2D shoelace (CW) → reverse for floor, keep for ceiling (old
        // convention)   positive 2D shoelace (CCW) → keep for floor, reverse
        // for ceiling (rbsp convention)
        let input_is_ccw = shoelace > 0.0;

        if !skip_floor {
            let ordered: Vec<&Vec2> = if input_is_ccw {
                polygon.iter().collect()
            } else {
                polygon.iter().rev().collect()
            };
            let fv: Vec<usize> = ordered
                .iter()
                .map(|v| self.vertex_add(Vec3::new(v.x, v.y, floor_h), vertex_map))
                .collect();
            if fv.len() >= 3 && vertex_shoelace(&fv, &self.vertices) > 0.0 {
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
            let ordered: Vec<&Vec2> = if input_is_ccw {
                // CCW input: reverse to get negative shoelace → downward normal
                polygon.iter().rev().collect()
            } else {
                // CW input: keep order → already negative shoelace → downward normal
                polygon.iter().collect()
            };
            let cv: Vec<usize> = ordered
                .iter()
                .map(|v| self.vertex_add(Vec3::new(v.x, v.y, ceil_h), vertex_map))
                .collect();
            if cv.len() < 3 || vertex_shoelace(&cv, &self.vertices) >= 0.0 {
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
