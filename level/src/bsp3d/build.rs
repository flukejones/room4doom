#[cfg(feature = "hprof")]
use coarse_prof::profile;

use crate::flags::LineDefFlags;
use crate::map_defs::{
    LineDef, Node, Sector, Segment, SideDef, SubSector, is_subsector, subsector_index,
};
use glam::{Vec2, Vec3};
use std::collections::{HashMap, HashSet};
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
    /// Global index into [`BSP3D::polygons`].
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

/// Max Doom light band.
pub const LIGHT_LEVELS: i32 = 15;

/// Fake-contrast brightness delta for axis-aligned walls (N/S −1 darker, E/W +1
/// lighter); 0 otherwise. Shared so the formula has one home.
pub fn contrast_adjust(normal: Vec3) -> i32 {
    let horizontal = normal.z.abs() >= 0.01; // floor/ceiling
    let north_south = normal.x.abs() < 0.001;
    let east_west = normal.y.abs() < 0.001;
    match (horizontal, north_south, east_west) {
        (false, true, _) => -1,
        (false, _, true) => 1,
        _ => 0,
    }
}

/// Final light band (0..15): `(sector_light>>4 + extralight)` capped at 15, then
/// the contrast delta re-clamped. Matches the software3d order exactly.
pub fn light_band(sector_light: usize, extralight: usize, normal: Vec3) -> i32 {
    let base = ((sector_light >> 4) + extralight).min(LIGHT_LEVELS as usize) as i32;
    (base + contrast_adjust(normal)).clamp(0, LIGHT_LEVELS)
}

/// Whether a polygon is a sky surface: a sky-filler wall (front texture ==
/// `sky_pic`) or a sky flat (texture == `sky_num`).
fn poly_is_sky(polygon: &SurfacePolygon, sky_pic: Option<usize>, sky_num: Option<usize>) -> bool {
    match &polygon.surface_kind {
        SurfaceKind::Vertical {
            front,
            ..
        } => front.texture == sky_pic && sky_pic.is_some(),
        SurfaceKind::Horizontal {
            texture,
            ..
        } => Some(*texture) == sky_num,
    }
}

/// A polygon's texture id and whether it is a flat. `u32::MAX` = untextured
/// (e.g. a wall face with no texture, or sky).
fn poly_texture(polygon: &SurfacePolygon) -> (u32, bool) {
    match &polygon.surface_kind {
        SurfaceKind::Vertical {
            front,
            ..
        } => (front.texture.map_or(u32::MAX, |t| t as u32), false),
        SurfaceKind::Horizontal {
            texture,
            ..
        } => (*texture as u32, true),
    }
}

/// Whether the wall texture is pegged so the texture origin is at the top
/// (matches `software3d::calculate_tex_coords`).
fn wall_unpeg(wall_type: WallType, pin: WallTexPin) -> bool {
    match wall_type {
        WallType::Upper => matches!(pin, WallTexPin::UnpegTop | WallTexPin::UnpegBoth),
        WallType::Middle => !matches!(pin, WallTexPin::UnpegBottom | WallTexPin::UnpegBoth),
        WallType::Lower => matches!(pin, WallTexPin::UnpegBottom | WallTexPin::UnpegBoth),
    }
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
            Self::UnpegBoth
        } else if flags.contains(LineDefFlags::UnpegBottom) {
            Self::UnpegBottom
        } else if flags.contains(LineDefFlags::UnpegTop) {
            Self::UnpegTop
        } else {
            Self::None
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

    fn expand_to_include_aabb(&mut self, other: &Self) {
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
    /// Indices into [`BSP3D::polygons`]; a two-sided wall index is in both
    /// leaves.
    pub polygon_indices: Vec<usize>,
    pub aabb: AABB,
    pub floor_polygons: Vec<usize>,
    pub ceiling_polygons: Vec<usize>,
    pub sector_id: usize,
    pub occlusion_segs: Vec<OcclusionSeg>,
}

/// A leaf's floor or ceiling polygon indices for a movement type. Free fn so it
/// composes inside a disjoint-field borrow of `BSP3D`.
fn surface_polygons_of(
    leaves: &[BSPLeaf3D],
    subsector_id: usize,
    movement: MovementType,
) -> &[usize] {
    match movement {
        MovementType::Floor => &leaves[subsector_id].floor_polygons,
        MovementType::Ceiling => &leaves[subsector_id].ceiling_polygons,
        MovementType::None => &[],
    }
}

/// Per-side texturing for a vertical wall. `texture` is `None` when that
/// sidedef is untextured.
#[derive(Debug, Clone)]
pub struct WallFace {
    pub texture: Option<usize>,
    pub tex_x_offset: f32,
    pub tex_y_offset: f32,
    pub texture_direction: Vec3,
    pub ceiling_z: f32,
}

#[derive(Debug, Clone)]
pub enum SurfaceKind {
    Vertical {
        /// Side the build-time normal faces.
        front: WallFace,
        /// Opposite side. `Some` only for two-sided Upper/Lower walls.
        back: Option<WallFace>,
        wall_type: WallType,
        wall_tex_pin: WallTexPin,
        two_sided: bool,
        linedef_id: usize,
        /// BOOM linedef special 260: translucent middle texture
        translucent: bool,
    },
    Horizontal {
        texture: usize,
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

    /// Sign test only — the offset is left unnormalised (no sqrt).
    #[inline]
    pub fn is_facing_point(&self, point: Vec3, vertex_positions: &[Vec3]) -> bool {
        let normal = if self.is_flipped(vertex_positions) {
            -self.normal
        } else {
            self.normal
        };
        let first_vertex = vertex_positions[unsafe { *self.vertices.get_unchecked(0) }];
        let dot = normal.dot(point - first_vertex);
        dot.is_sign_positive() || dot.is_nan()
    }

    /// A moving wall inverts when its floor crosses its ceiling, flipping the
    /// geometric normal against the build-time default. The decision is a dot,
    /// not a winding, so it survives the mover pass replacing vertex indices.
    #[inline]
    fn is_flipped(&self, vertex_positions: &[Vec3]) -> bool {
        if !(self.moves && matches!(self.surface_kind, SurfaceKind::Vertical { .. })) {
            return false;
        }
        unsafe {
            let p0 = vertex_positions.get_unchecked(self.vertices[0]);
            let p1 = vertex_positions.get_unchecked(self.vertices[1]);
            let p2 = vertex_positions.get_unchecked(self.vertices[2]);
            (*p1 - *p0)
                .cross(*p2 - *p0)
                .dot(self.normal)
                .is_sign_negative()
        }
    }

    /// The textured wall face facing the viewer, or `None` if that side is
    /// untextured. A flipped (inverted) wall shows its `back` sidedef.
    pub fn visible_face(&self, vertex_positions: &[Vec3]) -> Option<&WallFace> {
        let SurfaceKind::Vertical {
            front,
            back,
            ..
        } = &self.surface_kind
        else {
            return None;
        };
        let face = if self.is_flipped(vertex_positions) {
            back.as_ref().unwrap_or(front)
        } else {
            front
        };
        face.texture.map(|_| face)
    }

    /// A two-sided middle (masked) wall: drawn once, not tiled vertically.
    pub fn is_masked_middle(&self) -> bool {
        matches!(
            self.surface_kind,
            SurfaceKind::Vertical {
                two_sided: true,
                wall_type: WallType::Middle,
                ..
            }
        )
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

#[derive(Default)]
pub struct BSP3D {
    nodes: Vec<Node3D>,
    pub subsector_leaves: Vec<BSPLeaf3D>,
    /// All surface polygons; leaves reference these by index.
    pub polygons: Vec<SurfacePolygon>,
    pub(crate) root_node: u32,
    pub vertices: Vec<Vec3>,
    pub sector_subsectors: Vec<Vec<usize>>,
    /// Carved 2D convex polygon for each subsector, indexed by subsector ID.
    /// Empty vec for degenerate subsectors that produce no valid polygon.
    pub carved_polygons: Vec<Vec<Vec2>>,
    /// Maps linedef_id → global polygon indices, for wall texture updates.
    linedef_wall_polygons: HashMap<usize, Vec<usize>>,
    /// Temporary: zh wall records used during construction only.
    pub(crate) zh_wall_records: Vec<ZhWallRecord>,
    /// Fan triangulation: triples of indices into [`Self::vertices`]. Topology is
    /// stable across moves (movers only change vertex `z`).
    pub triangles: Vec<[u32; 3]>,
    /// Texel-space UV per polygon vertex (n-gon renderers: software3d), indexed
    /// via [`Self::poly_vertex_range`]. Texel, not normalised; renderers divide.
    pub poly_vertex_uv: Vec<[f32; 2]>,
    /// `[start, end)` into [`Self::poly_vertex_uv`] per polygon. Triangulating
    /// renderers (wgpu3d) fan this into per-corner UV at mesh upload.
    pub poly_vertex_range: Vec<(u32, u32)>,
    /// Per-polygon texture id; `u32::MAX` = untextured (sky).
    pub poly_tex: Vec<u32>,
    /// Per-polygon: flat (`true`) vs wall (`false`) texture.
    pub poly_is_flat: Vec<bool>,
    /// Per-polygon: a sky surface (sky-filler wall or sky flat). Renderers draw
    /// these with the sky, not the atlas.
    pub poly_is_sky: Vec<bool>,
    /// Wall polygon indices per sector (deduped), for mover UV recompute.
    pub sector_wall_polygons: Vec<Vec<usize>>,
    /// Per-polygon horizontal texture scroll in texels (special-48 scrollers),
    /// added to U at sample time. Delta beyond the build-baked offset.
    pub poly_scroll: Vec<f32>,
    /// Set when a surface moves; renderers re-upload only when set.
    geometry_dirty: bool,
    /// Set when poly_tex or poly_scroll changed (switch/scroll). Separate from
    /// geometry_dirty: scroll dirties every tic, movement only on move.
    texture_dirty: bool,
    /// Polygons behind `texture_dirty`; spills to `texture_dirty_full` at
    /// [`TEXTURE_DIRTY_POLY_CAP`] so a non-draining renderer can't grow it.
    texture_dirty_polys: Vec<usize>,
    texture_dirty_full: bool,
}

/// Dirty-poly count where scoped re-fan stops paying off (also the list bound).
const TEXTURE_DIRTY_POLY_CAP: usize = 4096;

/// Record a dirty polygon, spilling to the full-map flag at the cap.
fn mark_texture_dirty_poly(dirty_polys: &mut Vec<usize>, dirty_full: &mut bool, gi: usize) {
    if *dirty_full {
        return;
    }
    if dirty_polys.len() >= TEXTURE_DIRTY_POLY_CAP {
        *dirty_full = true;
        dirty_polys.clear();
        return;
    }
    dirty_polys.push(gi);
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
            polygons: Vec::new(),
            root_node,
            vertices: Vec::new(),
            sector_subsectors: vec![Vec::new(); sectors.len()],
            carved_polygons: pre_carved,
            linedef_wall_polygons: HashMap::new(),
            zh_wall_records: Vec::new(),
            triangles: Vec::new(),
            poly_vertex_uv: Vec::new(),
            poly_vertex_range: Vec::new(),
            poly_tex: Vec::new(),
            poly_is_flat: Vec::new(),
            poly_is_sky: Vec::new(),
            sector_wall_polygons: Vec::new(),
            poly_scroll: Vec::new(),
            // First frame must upload the initial geometry + textures.
            geometry_dirty: true,
            texture_dirty: true,
            texture_dirty_polys: Vec::new(),
            texture_dirty_full: true,
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

            if segments.get(start_seg..end_seg).is_some() {
                for segment in &segments[start_seg..end_seg] {
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

        // Phase 4: fan-triangulate every polygon now that all geometry exists
        // (walls, floors/ceilings, movers, sky filler) and bake per-vertex light.
        bsp3d.triangulate(sky_pic, sky_num);
        bsp3d.build_sector_wall_polygons();

        bsp3d.update_all_aabbs();
        bsp3d.expand_node_aabbs_for_movers(sectors, linedefs);
        bsp3d.build_linedef_wall_map();

        bsp3d
    }

    /// Collect each sector's wall (vertical) polygon indices, deduped. A
    /// two-sided wall appears in two subsectors; the `seen` set keeps one entry
    /// per sector.
    fn build_sector_wall_polygons(&mut self) {
        self.sector_wall_polygons = vec![Vec::new(); self.sector_subsectors.len()];
        for sector_id in 0..self.sector_subsectors.len() {
            let mut seen = HashSet::new();
            for si in 0..self.sector_subsectors[sector_id].len() {
                let ss = self.sector_subsectors[sector_id][si];
                for &gi in &self.subsector_leaves[ss].polygon_indices {
                    if matches!(self.polygons[gi].surface_kind, SurfaceKind::Vertical { .. })
                        && seen.insert(gi)
                    {
                        self.sector_wall_polygons[sector_id].push(gi);
                    }
                }
            }
        }
    }

    /// Build the fan triangulation + per-polygon-vertex UV/texture. Convex polys
    /// (rbsp carve / wall quads) fan as `(v0, vi, vi+1)`, no ear-clipping.
    fn triangulate(&mut self, sky_pic: Option<usize>, sky_num: Option<usize>) {
        let tri_count: usize = self
            .polygons
            .iter()
            .map(|p| p.vertices.len().saturating_sub(2))
            .sum();
        let vert_total: usize = self.polygons.iter().map(|p| p.vertices.len()).sum();
        self.triangles = Vec::with_capacity(tri_count);
        self.poly_vertex_uv = Vec::with_capacity(vert_total);
        self.poly_vertex_range = Vec::with_capacity(self.polygons.len());
        self.poly_tex = Vec::with_capacity(self.polygons.len());
        self.poly_is_flat = Vec::with_capacity(self.polygons.len());
        self.poly_is_sky = Vec::with_capacity(self.polygons.len());
        for poly_idx in 0..self.polygons.len() {
            let (tex, is_flat) = poly_texture(&self.polygons[poly_idx]);
            self.poly_tex.push(tex);
            self.poly_is_flat.push(is_flat);
            self.poly_is_sky
                .push(poly_is_sky(&self.polygons[poly_idx], sky_pic, sky_num));

            let pv_start = self.poly_vertex_uv.len() as u32;
            let n = self.polygons[poly_idx].vertices.len();
            if n >= 3 {
                for j in 0..n {
                    // Read the index per-iteration so `polygons` is not borrowed
                    // across the push (no vertex-list clone needed).
                    let vi = self.polygons[poly_idx].vertices[j];
                    self.poly_vertex_uv
                        .push(self.corner_uv_texels(poly_idx, vi));
                }
                let v0 = self.polygons[poly_idx].vertices[0] as u32;
                for i in 1..n - 1 {
                    let vi = self.polygons[poly_idx].vertices[i] as u32;
                    let vi1 = self.polygons[poly_idx].vertices[i + 1] as u32;
                    self.triangles.push([v0, vi, vi1]);
                }
            }
            self.poly_vertex_range
                .push((pv_start, self.poly_vertex_uv.len() as u32));
        }
        self.poly_scroll = vec![0.0; self.polygons.len()];
    }

    /// Recompute texture UV for one wall polygon after its vertex z changed
    /// (its sector's floor/ceiling moved). Reuses [`Self::corner_uv_texels`].
    /// No-op for non-vertical or degenerate polygons.
    fn recompute_wall_uv(&mut self, poly_idx: usize) {
        if !matches!(
            self.polygons[poly_idx].surface_kind,
            SurfaceKind::Vertical { .. }
        ) {
            return;
        }
        let n = self.polygons[poly_idx].vertices.len();
        if n < 3 {
            return;
        }
        let (pv_start, _) = self.poly_vertex_range[poly_idx];
        let base = pv_start as usize;
        for j in 0..n {
            // Read the index per-iteration so the borrow of `polygons` ends
            // before the `&mut self` write (no vertex-list clone needed).
            let vi = self.polygons[poly_idx].vertices[j];
            self.poly_vertex_uv[base + j] = self.corner_uv_texels(poly_idx, vi);
        }
    }

    /// Recompute UV for all walls of a sector whose surface just moved.
    fn recompute_sector_wall_uv(&mut self, sector_id: usize) {
        if sector_id >= self.sector_wall_polygons.len() {
            return;
        }
        for i in 0..self.sector_wall_polygons[sector_id].len() {
            let gi = self.sector_wall_polygons[sector_id][i];
            self.recompute_wall_uv(gi);
        }
    }

    /// Fan a per-polygon attribute into per-corner entries (static, order matches
    /// [`Self::triangles`]). `attr(poly_idx)` builds the value. `out` is cleared.
    pub fn fan_corner_attr<T: Copy>(&self, out: &mut Vec<T>, attr: impl Fn(usize) -> T) {
        out.clear();
        for poly_idx in 0..self.polygons.len() {
            let (start, end) = self.poly_vertex_range[poly_idx];
            let n = (end - start) as usize;
            if n < 3 {
                continue;
            }
            let v = attr(poly_idx);
            for _ in 0..(n - 2) * 3 {
                out.push(v);
            }
        }
    }

    /// Fan per-polygon-vertex UV ([`Self::poly_vertex_uv`]) into per-corner UV
    /// (fan `(v0, vi, vi+1)`, order matches [`Self::triangles`]). `out` is
    /// cleared. Triangulating renderers (wgpu3d) call this at mesh upload and on
    /// geometry/texture re-upload instead of storing a second UV array.
    pub fn fan_corner_uv(&self, out: &mut Vec<[f32; 2]>) {
        out.clear();
        for &(start, end) in &self.poly_vertex_range {
            let base = start as usize;
            let n = (end - start) as usize;
            if n < 3 {
                continue;
            }
            for i in 1..n - 1 {
                out.push(self.poly_vertex_uv[base]);
                out.push(self.poly_vertex_uv[base + i]);
                out.push(self.poly_vertex_uv[base + i + 1]);
            }
        }
    }

    /// Texel-space `(u, v)` for one polygon corner, replicating
    /// `software3d::calculate_tex_coords` (minus the `+tex_height` pegging shift,
    /// a whole-texture offset that wraps to a no-op). `level` has no texture
    /// dimensions; the renderer divides by them.
    fn corner_uv_texels(&self, poly_idx: usize, vertex_idx: usize) -> [f32; 2] {
        let polygon = &self.polygons[poly_idx];
        let world_pos = self.vertices[vertex_idx];
        match &polygon.surface_kind {
            SurfaceKind::Vertical {
                front,
                wall_type,
                wall_tex_pin,
                ..
            } => {
                let v1 = self.vertices[polygon.vertices[0]];
                let from_start = world_pos - v1;
                let u = from_start.x * front.texture_direction.x
                    + from_start.y * front.texture_direction.y
                    + front.tex_x_offset;

                let (wall_bottom_z, wall_top_z) = self.wall_z_range(polygon);
                let unpeg = wall_unpeg(*wall_type, *wall_tex_pin);
                // `+tex_height` omitted from the non-unpeg Upper/Middle case: it
                // is a whole-texture shift that wraps to a no-op under `fract`.
                let anchor_z = match (unpeg, wall_type) {
                    (true, WallType::Middle) | (false, WallType::Lower) => wall_top_z,
                    (true, _) => front.ceiling_z,
                    (false, WallType::Upper | WallType::Middle) => wall_bottom_z,
                };
                let v = -world_pos.z + anchor_z + front.tex_y_offset;
                [u, v]
            }
            SurfaceKind::Horizontal {
                ..
            } => {
                let tex_cos = HORIZONTAL_TEX_DIRECTION.cos();
                let tex_sin = HORIZONTAL_TEX_DIRECTION.sin();
                let u = world_pos.x * tex_cos - world_pos.y * tex_sin;
                let v = world_pos.x * tex_sin + world_pos.y * tex_cos;
                [u, v]
            }
        }
    }

    /// Min/max Z over a polygon's vertices (wall pegging anchors).
    fn wall_z_range(&self, polygon: &SurfacePolygon) -> (f32, f32) {
        let mut lo = f32::MAX;
        let mut hi = f32::MIN;
        for &vi in &polygon.vertices {
            let z = self.vertices[vi].z;
            lo = lo.min(z);
            hi = hi.max(z);
        }
        (lo, hi)
    }

    pub fn nodes(&self) -> &[Node3D] {
        &self.nodes
    }

    pub fn get_subsector_leaf(&self, subsector_id: usize) -> Option<&BSPLeaf3D> {
        self.subsector_leaves.get(subsector_id)
    }

    /// Polygons referenced by a leaf, resolved through [`Self::polygons`].
    pub fn leaf_polygons(&self, subsector_id: usize) -> impl Iterator<Item = &SurfacePolygon> {
        self.subsector_leaves[subsector_id]
            .polygon_indices
            .iter()
            .map(move |&i| &self.polygons[i])
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

    /// Move a sector's floor or ceiling polygons to `new_height` (vertex z only).
    /// Flat texture changes go through [`Self::update_flat_texture`], not here.
    pub fn move_surface(&mut self, sector_id: usize, movement_type: MovementType, new_height: f32) {
        if movement_type == MovementType::None {
            return;
        }
        for si in 0..self.sector_subsectors[sector_id].len() {
            let ss = self.sector_subsectors[sector_id][si];
            for pi in 0..self.surface_polygons(ss, movement_type).len() {
                let gi = self.surface_polygons(ss, movement_type)[pi];
                for vi in 0..self.polygons[gi].vertices.len() {
                    let vertex_idx = self.polygons[gi].vertices[vi];
                    self.vertices[vertex_idx].z = new_height;
                }
            }
        }

        self.recompute_sector_wall_uv(sector_id);
        self.geometry_dirty = true;
        self.update_affected_aabbs(sector_id);
    }

    /// A leaf's floor or ceiling polygon indices for a movement type.
    fn surface_polygons(&self, subsector_id: usize, movement: MovementType) -> &[usize] {
        surface_polygons_of(&self.subsector_leaves, subsector_id, movement)
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
    /// update). The sector's walls share these vertices, so re-derive their UV so
    /// textures stay anchored (tile) instead of stretching.
    fn set_surface_height(&mut self, sector_id: usize, movement: MovementType, height: f32) {
        for si in 0..self.sector_subsectors[sector_id].len() {
            let ss = self.sector_subsectors[sector_id][si];
            for pi in 0..self.surface_polygons(ss, movement).len() {
                let gi = self.surface_polygons(ss, movement)[pi];
                for vi in 0..self.polygons[gi].vertices.len() {
                    let vertex_idx = self.polygons[gi].vertices[vi];
                    self.vertices[vertex_idx].z = height;
                }
            }
        }
        self.recompute_sector_wall_uv(sector_id);
        self.geometry_dirty = true;
    }

    /// True if vertex positions / UV changed since the last
    /// [`Self::clear_geometry_dirty`]. A renderer uploads dynamic buffers only
    /// when set.
    pub fn geometry_dirty(&self) -> bool {
        self.geometry_dirty
    }

    /// Clear the dirty flag after a renderer has uploaded the dynamic buffers.
    pub fn clear_geometry_dirty(&mut self) {
        self.geometry_dirty = false;
    }

    /// True if poly_tex or poly_scroll changed since [`Self::clear_texture_dirty`]
    /// (switch swap or texture scroll). Renderers re-upload texture buffers only
    /// when set.
    pub fn texture_dirty(&self) -> bool {
        self.texture_dirty
    }

    /// Dirty polygons for scoped re-fan; `None` = re-fan the whole map.
    /// Valid only while [`Self::texture_dirty`] is set.
    pub fn texture_dirty_polys(&self) -> Option<&[usize]> {
        if self.texture_dirty_full {
            None
        } else {
            Some(&self.texture_dirty_polys)
        }
    }

    pub fn clear_texture_dirty(&mut self) {
        self.texture_dirty = false;
        self.texture_dirty_full = false;
        self.texture_dirty_polys.clear();
    }

    /// Set the floor or ceiling flat texture for a sector's polygons, syncing
    /// both render stores (`surface_kind` for software3d, `poly_tex` for wgpu3d)
    /// + `texture_dirty`. Called from env when a sector flat pic changes.
    pub fn update_flat_texture(
        &mut self,
        sector_id: usize,
        movement: MovementType,
        new_texture: usize,
    ) {
        let Self {
            sector_subsectors,
            subsector_leaves,
            polygons,
            poly_tex,
            texture_dirty,
            texture_dirty_polys,
            texture_dirty_full,
            ..
        } = self;
        for &ss in &sector_subsectors[sector_id] {
            for &gi in surface_polygons_of(subsector_leaves, ss, movement) {
                if let SurfaceKind::Horizontal {
                    texture,
                    ..
                } = &mut polygons[gi].surface_kind
                {
                    *texture = new_texture;
                    poly_tex[gi] = new_texture as u32;
                    *texture_dirty = true;
                    mark_texture_dirty_poly(texture_dirty_polys, texture_dirty_full, gi);
                }
            }
        }
    }

    /// Index all wall polygons by linedef ID for O(1) texture update lookups.
    fn build_linedef_wall_map(&mut self) {
        for (gi, polygon) in self.polygons.iter().enumerate() {
            if let SurfaceKind::Vertical {
                linedef_id,
                ..
            } = polygon.surface_kind
            {
                self.linedef_wall_polygons
                    .entry(linedef_id)
                    .or_default()
                    .push(gi);
            }
        }
    }

    /// Update the texture of all wall polygons belonging to `linedef_id` that
    /// match `wall_type`.  Called by the switch system after a sidedef texture
    /// change so the 3D scene stays in sync. Switches always fire on the front
    /// sidedef, so the front face is updated.
    pub fn update_wall_texture(
        &mut self,
        linedef_id: usize,
        wall_type: WallType,
        new_texture: usize,
    ) {
        let Self {
            linedef_wall_polygons,
            polygons,
            poly_tex,
            texture_dirty,
            texture_dirty_polys,
            texture_dirty_full,
            ..
        } = self;
        let Some(indices) = linedef_wall_polygons.get(&linedef_id) else {
            return;
        };
        for &gi in indices {
            if let SurfaceKind::Vertical {
                front,
                wall_type: wt,
                ..
            } = &mut polygons[gi].surface_kind
                && *wt == wall_type
            {
                front.texture = Some(new_texture);
                poly_tex[gi] = new_texture as u32;
                *texture_dirty = true;
                mark_texture_dirty_poly(texture_dirty_polys, texture_dirty_full, gi);
            }
        }
    }

    /// Set horizontal texture scroll (texels) for all wall polygons of a
    /// scrolling linedef (special 48). `delta` is the live offset minus the
    /// build-baked one, added to U at sample time so it does not double-count.
    pub fn set_wall_scroll(&mut self, linedef_id: usize, delta: f32) {
        let Self {
            linedef_wall_polygons,
            poly_scroll,
            texture_dirty,
            texture_dirty_polys,
            texture_dirty_full,
            ..
        } = self;
        let Some(indices) = linedef_wall_polygons.get(&linedef_id) else {
            return;
        };
        for &gi in indices {
            poly_scroll[gi] = delta;
            mark_texture_dirty_poly(texture_dirty_polys, texture_dirty_full, gi);
        }
        *texture_dirty = true;
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
    /// Upper/Lower are a single quad carrying both sidedefs' [`WallFace`]s.
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

        // Build from the segment whose side shows the wall (its sector is the
        // taller/lower one). At equal heights (a mover at rest) both segments
        // qualify, so the linedef-front segment builds it.
        let is_linedef_front = segment.frontsector.num == segment.linedef.frontsector.num;
        let other_sidedef = if is_linedef_front {
            segment.linedef.back_sidedef.as_deref()
        } else {
            Some(&*segment.linedef.front_sidedef)
        };

        let build_upper = if back_sector.ceilingheight == front_sector.ceilingheight {
            is_linedef_front
        } else {
            back_sector.ceilingheight < front_sector.ceilingheight
        };
        if build_upper && !both_sky_ceil {
            self.add_two_sided_wall(
                segment,
                WallType::Upper,
                back_sector.ceilingheight.to_f32(),
                front_sector.ceilingheight.to_f32(),
                front_sector,
                back_sector,
                other_sidedef,
                front_subsector_id,
                vertex_map,
            );
        }

        let build_lower = if back_sector.floorheight == front_sector.floorheight {
            is_linedef_front
        } else {
            back_sector.floorheight > front_sector.floorheight
        };
        if build_lower && !both_sky_floor {
            self.add_two_sided_wall(
                segment,
                WallType::Lower,
                front_sector.floorheight.to_f32(),
                back_sector.floorheight.to_f32(),
                front_sector,
                back_sector,
                other_sidedef,
                front_subsector_id,
                vertex_map,
            );
        }

        if segment.sidedef.midtexture.is_some() {
            let bottom = front_sector
                .floorheight
                .to_f32()
                .max(back_sector.floorheight.to_f32());
            let top = front_sector
                .ceilingheight
                .to_f32()
                .min(back_sector.ceilingheight.to_f32());
            let front_face = Self::wall_face_for_side(
                segment,
                false,
                segment.sidedef.midtexture,
                &segment.sidedef,
                front_sector.ceilingheight.to_f32(),
            );
            self.add_wall_quad(
                segment,
                bottom,
                top,
                WallType::Middle,
                front_face,
                None,
                front_id,
                true,
                front_subsector_id,
                Some(back_id),
                vertex_map,
            );
        }
    }

    /// Build a two-sided Upper/Lower wall quad spanning `bottom_h`..`top_h`,
    /// carrying this seg's `front` face and the opposite side's `back` face.
    /// Skips construction when neither side has the relevant texture.
    #[allow(clippy::too_many_arguments)]
    fn add_two_sided_wall(
        &mut self,
        segment: &Segment,
        wall_type: WallType,
        bottom_h: f32,
        top_h: f32,
        front_sector: &Sector,
        back_sector: &Sector,
        other_sidedef: Option<&SideDef>,
        subsector_id: usize,
        vertex_map: &mut HashMap<QuantizedVec3, usize>,
    ) {
        let tex = |sd: &SideDef| match wall_type {
            WallType::Upper => sd.toptexture,
            _ => sd.bottomtexture,
        };
        let front_tex = tex(&segment.sidedef);
        let back_tex = other_sidedef.and_then(tex);
        if front_tex.is_none() && back_tex.is_none() {
            return;
        }
        let front = Self::wall_face_for_side(
            segment,
            false,
            front_tex,
            &segment.sidedef,
            front_sector.ceilingheight.to_f32(),
        );
        let back = other_sidedef.map(|sd| {
            Self::wall_face_for_side(
                segment,
                true,
                back_tex,
                sd,
                back_sector.ceilingheight.to_f32(),
            )
        });
        self.add_wall_quad(
            segment,
            bottom_h,
            top_h,
            wall_type,
            front,
            back,
            front_sector.num as usize,
            true,
            subsector_id,
            Some(back_sector.num as usize),
            vertex_map,
        );
        // Share into the subsectors across this segment so a mover that inverts
        // the wall can render it from the other side.
        let gi = self.polygons.len() - 1;
        for &back in &segment.back_subsectors {
            self.subsector_leaves[back].polygon_indices.push(gi);
        }
    }

    /// Build a [`WallFace`] for one side of `segment`. The back side traverses
    /// the linedef reversed, so its direction and seg offset come from the
    /// opposite endpoints.
    fn wall_face_for_side(
        segment: &Segment,
        is_back: bool,
        texture: Option<usize>,
        sidedef: &SideDef,
        ceiling_z: f32,
    ) -> WallFace {
        let start = segment.v1.pos;
        let end = segment.v2.pos;
        let (dir, seg_offset) = if is_back {
            let ld_v2 = Vec2::new(segment.linedef.v2.x, segment.linedef.v2.y);
            (start - end, (ld_v2 - end).length())
        } else {
            (end - start, segment.offset.into())
        };
        let dir = dir.normalize();
        let angle = dir.y.atan2(dir.x);
        WallFace {
            texture,
            tex_x_offset: f32::from(sidedef.textureoffset) + seg_offset,
            tex_y_offset: sidedef.rowoffset.into(),
            texture_direction: Vec3::new(angle.cos(), angle.sin(), 0.0),
            ceiling_z,
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
        if segment.sidedef.midtexture.is_some() {
            let front_id = front_sector.num as usize;
            let is_zh = (front_sector.ceilingheight.to_f32() - front_sector.floorheight.to_f32())
                .abs()
                <= HEIGHT_EPSILON;
            // For zh sectors (doors): pass self as back_sector so add_wall_quad
            // creates fresh vertices and a ZhWallRecord. The mover pass
            // connects bottom → floor vertex, top → ceiling vertex.
            let back_sector_id = if is_zh { Some(front_id) } else { None };
            let front_face = Self::wall_face_for_side(
                segment,
                false,
                segment.sidedef.midtexture,
                &segment.sidedef,
                front_sector.ceilingheight.to_f32(),
            );
            self.add_wall_quad(
                segment,
                front_sector.floorheight.to_f32(),
                front_sector.ceilingheight.to_f32(),
                WallType::Middle,
                front_face,
                None,
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
    #[allow(clippy::too_many_arguments)]
    fn add_wall_quad(
        &mut self,
        segment: &Segment,
        bottom_height: f32,
        top_height: f32,
        wall_type: WallType,
        front: WallFace,
        back: Option<WallFace>,
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

        let surface_kind = SurfaceKind::Vertical {
            front,
            back,
            wall_type,
            wall_tex_pin: WallTexPin::from(segment.linedef.flags),
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
        let gi = self.polygons.len();
        self.polygons.push(quad);
        self.subsector_leaves[subsector_id].polygon_indices.push(gi);

        if is_zero_height && let Some(back_id) = back_sector_id {
            self.zh_wall_records.push(ZhWallRecord {
                poly_index: gi,
                bottom: [bottom_start, bottom_end],
                top: [top_start, top_end],
                wall_type,
                front_sector: sector_id,
                back_sector: back_id,
            });
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
        for sector in sectors {
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
                for seg in &segments[start..end] {
                    // Only perimeter segments: skip interior (same-sector)
                    // and sky-to-sky boundaries.
                    let is_perimeter_ceil = match &seg.backsector {
                        Some(back) => back.num != seg.frontsector.num && back.ceilingpic != sky_num,
                        None => true,
                    };
                    let is_perimeter_floor = match &seg.backsector {
                        Some(back) => back.num != seg.frontsector.num && back.floorpic != sky_num,
                        None => true,
                    };

                    // Skip if the existing wall already reaches the target.
                    if needs_ceil_filler && is_perimeter_ceil && sky_ceil < max_h {
                        let face = Self::wall_face_for_side(
                            seg,
                            false,
                            Some(sky_pic),
                            &seg.sidedef,
                            seg.frontsector.ceilingheight.to_f32(),
                        );
                        self.add_wall_quad(
                            seg,
                            sky_ceil,
                            max_h,
                            WallType::Upper,
                            face,
                            None,
                            sector_id,
                            seg.backsector.is_some(),
                            ss_id,
                            None,
                            vertex_map,
                        );
                    }
                    if needs_floor_filler && is_perimeter_floor && min_h < sky_floor {
                        let face = Self::wall_face_for_side(
                            seg,
                            false,
                            Some(sky_pic),
                            &seg.sidedef,
                            seg.frontsector.ceilingheight.to_f32(),
                        );
                        self.add_wall_quad(
                            seg,
                            min_h,
                            sky_floor,
                            WallType::Lower,
                            face,
                            None,
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
        for (ssid, subsector) in subsectors.iter().enumerate() {
            let polygon = self.carved_polygons[ssid].clone();
            self.create_floor_ceiling_polygons(
                ssid,
                subsector,
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
                    Self::create_horizontal_surface_kind(subsector.sector.floorpic),
                    fv,
                    Vec3::new(0.0, 0.0, 1.0),
                    &self.vertices,
                    false,
                );
                let fi = self.polygons.len();
                self.polygons.push(fp);
                self.subsector_leaves[subsector_id].polygon_indices.push(fi);
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
                Self::create_horizontal_surface_kind(subsector.sector.ceilingpic),
                cv,
                Vec3::new(0.0, 0.0, -1.0),
                &self.vertices,
                false,
            );
            let ci = self.polygons.len();
            self.polygons.push(cp);
            self.subsector_leaves[subsector_id].polygon_indices.push(ci);
            self.subsector_leaves[subsector_id]
                .ceiling_polygons
                .push(ci);
        }
    }

    /// Compute the AABB for a single subsector leaf from its polygon vertices.
    fn compute_leaf_aabb(&self, subsector_id: usize) -> AABB {
        let mut aabb = AABB::new();
        for &gi in &self.subsector_leaves[subsector_id].polygon_indices {
            for &vertex_idx in &self.polygons[gi].vertices {
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

    fn create_horizontal_surface_kind(texture: usize) -> SurfaceKind {
        SurfaceKind::Horizontal {
            texture,
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

impl Default for BSPLeaf3D {
    fn default() -> Self {
        Self {
            polygon_indices: Vec::new(),
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
