//! Mover vertex pass.
//!
//! Separates shared vertices at zero-height boundaries, connects wall vertices
//! to floor/ceiling polygons, and sets `moves` flags for sectors that
//! participate in lifts, doors, and platforms.

use super::build::{BSP3D, HEIGHT_EPSILON, QUANT_PRECISION, QuantizedVec3, SurfaceKind, WallType};
use crate::flags::LineDefFlags;
use crate::map_defs::{LineDef, Sector, Segment, SubSector};
use crate::special_encode::{self, Category as SpecialCategory};
use glam::{Vec2, Vec3};
use std::collections::{BTreeSet, HashMap, HashSet};

/// Deduplication tolerance for vertex proximity checks.
const DEDUP_EPSILON: f32 = 0.1;
/// Max perpendicular distance for point-on-edge detection during boundary
/// vertex insertion into N-gon floor/ceiling polygons.
const EDGE_INSERT_EPSILON: f32 = 1.0;

/// Type alias for the per-position, per-sector vertex index maps used to
/// connect wall vertices to floor/ceiling polygon vertices.
type VertexMap = HashMap<QuantizedVec2, HashMap<usize, usize>>;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MoverKind {
    Floor,
    Ceiling,
    Both,
}

impl MoverKind {
    fn combine(self, other: Self) -> Self {
        match (self, other) {
            (Self::Floor, Self::Ceiling)
            | (Self::Ceiling, Self::Floor)
            | (Self::Both, _)
            | (_, Self::Both) => Self::Both,
            (Self::Floor, Self::Floor) => Self::Floor,
            (Self::Ceiling, Self::Ceiling) => Self::Ceiling,
        }
    }
}

/// Classify a (generalized) linedef special as floor, ceiling, or both.
///
/// Specials are normalised to generalized form at load (see
/// [`crate::special_encode`]), so this decodes the generalized special and
/// maps its category to which surface(s) move: floors/lifts/stairs raise the
/// floor, ceilings/doors/crushers move the ceiling, and the composite
/// vanilla-40 moves both.
fn classify_special(special: u32) -> Option<MoverKind> {
    let spec = special_encode::decode(special)?;
    if spec.composite {
        return Some(MoverKind::Both);
    }
    Some(match spec.category {
        SpecialCategory::Floor | SpecialCategory::Lift | SpecialCategory::Stairs => {
            MoverKind::Floor
        }
        SpecialCategory::Ceiling | SpecialCategory::Door => MoverKind::Ceiling,
    })
}

/// Check if a sector participates in any line-special-triggered movement.
pub fn is_sector_mover(sector: &Sector, linedefs: &[LineDef]) -> bool {
    let tag_linedefs = build_tag_linedef_index(linedefs);
    classify_sector_mover(sector, linedefs, &tag_linedefs).is_some()
}

/// Build a tag→linedef-indices map for O(1) lookup per sector tag.
pub fn build_tag_linedef_index(linedefs: &[LineDef]) -> HashMap<i16, Vec<usize>> {
    let mut map: HashMap<i16, Vec<usize>> = HashMap::new();
    for (li, ld) in linedefs.iter().enumerate() {
        if ld.tag != 0 {
            map.entry(ld.tag).or_default().push(li);
        }
    }
    map
}

/// Classify a sector's movement type from the linedef specials targeting it.
pub fn classify_sector_mover(
    sector: &Sector,
    linedefs: &[LineDef],
    tag_linedefs: &HashMap<i16, Vec<usize>>,
) -> Option<MoverKind> {
    let mut result: Option<MoverKind> = None;

    if sector.tag != 0
        && let Some(indices) = tag_linedefs.get(&sector.tag)
    {
        for &li in indices {
            if let Some(kind) = classify_special(linedefs[li].special) {
                result = Some(match result {
                    Some(prev) => prev.combine(kind),
                    None => kind,
                });
            }
        }
    }
    for line in &sector.lines {
        if let Some(back) = &line.backsector
            && back.num == sector.num
            && let Some(kind) = classify_special(line.special)
        {
            result = Some(match result {
                Some(prev) => prev.combine(kind),
                None => kind,
            });
        }
    }
    result
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
            x: (v.x / precision).ceil() as i32,
            y: (v.y / precision).ceil() as i32,
        }
    }

    /// Look up the vertex index for a given sector in a vertex map.
    fn lookup(&self, map: &VertexMap, sector_id: usize) -> Option<usize> {
        let sector_map = map.get(self)?;
        sector_map.get(&sector_id).copied()
    }
}

/// Add a boundary point to `bounds` if no existing entry matches `pos` for
/// the same sector pair within `DEDUP_EPSILON`.
fn push_dedup_bound(
    bounds: &mut Vec<(Vec2, usize, usize)>,
    seen: &mut HashSet<(QuantizedVec2, usize, usize)>,
    pos: Vec2,
    a: usize,
    b: usize,
) {
    let key = (QuantizedVec2::from_vec2(pos, DEDUP_EPSILON), a, b);
    if seen.insert(key) {
        bounds.push((pos, a, b));
    }
}

impl BSP3D {
    // ------------------------------------------------------------------
    // Entry points (called from BSP3D::new in mod.rs)
    // ------------------------------------------------------------------

    /// Post-construction pass that separates shared vertices at mover
    /// boundaries, connects wall vertices to floor/ceiling polygons, and
    /// marks affected polygons as moveable.
    ///
    /// Seven logical steps:
    /// 1. Identify mover sectors and zh boundaries
    /// 2. Insert missing boundary vertices into floor/ceiling N-gons
    /// 3. Internal zh sector separation (clone the moving surface)
    /// 4. Cross-sector boundary separation (+ residual + vertex map population)
    /// 5. Zh wall vertex connection via ZhWallRecords
    /// 6. Non-zh wall vertex connection via linedef lookup
    /// 7. Set `moves` flag on affected polygons
    pub(super) fn mover_vertex_pass(
        &mut self,
        sectors: &[Sector],
        segments: &[Segment],
        subsectors: &[SubSector],
        linedefs: &[LineDef],
    ) {
        // Step 1: identify mover sectors and zh boundaries.
        let mut mover_sectors: HashMap<usize, MoverKind> = HashMap::new();
        // BTreeSet: deterministic iteration in the vertex-separation steps below.
        let mut zh_sectors: BTreeSet<usize> = BTreeSet::new();
        let mut zh_lower_bounds: Vec<(Vec2, usize, usize)> = Vec::new();
        let mut zh_upper_bounds: Vec<(Vec2, usize, usize)> = Vec::new();
        let mut zh_lower_seen: HashSet<(QuantizedVec2, usize, usize)> = HashSet::new();
        let mut zh_upper_seen: HashSet<(QuantizedVec2, usize, usize)> = HashSet::new();
        let mut zh_lower_sectors: HashSet<usize> = HashSet::new();
        let mut zh_upper_sectors: HashSet<usize> = HashSet::new();

        let tag_linedefs = build_tag_linedef_index(linedefs);

        for (i, sector) in sectors.iter().enumerate() {
            if let Some(kind) = classify_sector_mover(sector, linedefs, &tag_linedefs) {
                mover_sectors.insert(i, kind);
            }
            if (sector.ceilingheight.to_f32() - sector.floorheight.to_f32()).abs() <= HEIGHT_EPSILON
            {
                zh_sectors.insert(i);
            }
        }

        // Build sector→segment indices for O(1) lookup per sector.
        let mut sector_segs: Vec<Vec<usize>> = vec![Vec::new(); sectors.len()];
        for (si, seg) in segments.iter().enumerate() {
            let front_id = seg.frontsector.num as usize;
            if front_id < sectors.len() {
                sector_segs[front_id].push(si);
            }
            if let Some(back) = &seg.backsector {
                let back_id = back.num as usize;
                if back_id < sectors.len() && back_id != front_id {
                    sector_segs[back_id].push(si);
                }
            }
        }

        // Texture-marked zh boundaries.
        for seg in segments {
            let Some(back) = &seg.backsector else {
                continue;
            };
            let front_id = seg.frontsector.num as usize;
            let back_id = back.num as usize;

            if seg.sidedef.bottomtexture.is_some()
                && (seg.frontsector.floorheight.to_f32() - back.floorheight.to_f32()).abs()
                    <= HEIGHT_EPSILON
            {
                zh_lower_sectors.insert(front_id);
                zh_lower_sectors.insert(back_id);
                mover_sectors.entry(front_id).or_insert(MoverKind::Floor);
                mover_sectors.entry(back_id).or_insert(MoverKind::Floor);
                for sv in [seg.v1.pos, seg.v2.pos] {
                    push_dedup_bound(
                        &mut zh_lower_bounds,
                        &mut zh_lower_seen,
                        sv,
                        front_id,
                        back_id,
                    );
                }
            }

            if seg.sidedef.toptexture.is_some()
                && (seg.frontsector.ceilingheight.to_f32() - back.ceilingheight.to_f32()).abs()
                    <= HEIGHT_EPSILON
            {
                zh_upper_sectors.insert(front_id);
                zh_upper_sectors.insert(back_id);
                mover_sectors.entry(front_id).or_insert(MoverKind::Ceiling);
                mover_sectors.entry(back_id).or_insert(MoverKind::Ceiling);
                for sv in [seg.v1.pos, seg.v2.pos] {
                    push_dedup_bound(
                        &mut zh_upper_bounds,
                        &mut zh_upper_seen,
                        sv,
                        front_id,
                        back_id,
                    );
                }
            }
        }

        // Mover-based boundary detection: same-height boundaries without
        // texture markers.
        for seg in segments {
            let Some(back) = &seg.backsector else {
                continue;
            };
            let front_id = seg.frontsector.num as usize;
            let back_id = back.num as usize;
            if !mover_sectors.contains_key(&front_id) && !mover_sectors.contains_key(&back_id) {
                continue;
            }
            if (seg.frontsector.floorheight.to_f32() - back.floorheight.to_f32()).abs()
                <= HEIGHT_EPSILON
            {
                for sv in [seg.v1.pos, seg.v2.pos] {
                    push_dedup_bound(
                        &mut zh_lower_bounds,
                        &mut zh_lower_seen,
                        sv,
                        front_id,
                        back_id,
                    );
                }
            }
            if (seg.frontsector.ceilingheight.to_f32() - back.ceilingheight.to_f32()).abs()
                <= HEIGHT_EPSILON
            {
                for sv in [seg.v1.pos, seg.v2.pos] {
                    push_dedup_bound(
                        &mut zh_upper_bounds,
                        &mut zh_upper_seen,
                        sv,
                        front_id,
                        back_id,
                    );
                }
            }
        }

        // Floor/ceiling crossings: mover floor at adjacent ceiling height.
        let mut floor_ceil_bounds: Vec<(Vec2, usize, usize)> = Vec::new();
        let mut floor_ceil_seen: HashSet<(QuantizedVec2, usize, usize)> = HashSet::new();
        let mut floor_ceil_sectors: HashSet<usize> = HashSet::new();
        for seg in segments {
            let Some(back) = &seg.backsector else {
                continue;
            };
            let front_id = seg.frontsector.num as usize;
            let back_id = back.num as usize;
            if !mover_sectors.contains_key(&front_id) && !mover_sectors.contains_key(&back_id) {
                continue;
            }
            if (seg.frontsector.floorheight.to_f32() - back.ceilingheight.to_f32()).abs()
                <= HEIGHT_EPSILON
            {
                floor_ceil_sectors.insert(front_id);
                floor_ceil_sectors.insert(back_id);
                for sv in [seg.v1.pos, seg.v2.pos] {
                    push_dedup_bound(
                        &mut floor_ceil_bounds,
                        &mut floor_ceil_seen,
                        sv,
                        front_id,
                        back_id,
                    );
                }
            }
            if (back.floorheight.to_f32() - seg.frontsector.ceilingheight.to_f32()).abs()
                <= HEIGHT_EPSILON
            {
                floor_ceil_sectors.insert(front_id);
                floor_ceil_sectors.insert(back_id);
                for sv in [seg.v1.pos, seg.v2.pos] {
                    push_dedup_bound(
                        &mut floor_ceil_bounds,
                        &mut floor_ceil_seen,
                        sv,
                        back_id,
                        front_id,
                    );
                }
            }
        }

        if mover_sectors.is_empty() && zh_sectors.is_empty() && floor_ceil_bounds.is_empty() {
            return;
        }

        // Step 2: insert missing boundary vertices into floor/ceiling N-gons.
        // Sorted for deterministic build order.
        let all_relevant: BTreeSet<usize> = zh_lower_sectors
            .union(&zh_upper_sectors)
            .copied()
            .chain(zh_sectors.iter().copied())
            .chain(mover_sectors.keys().copied())
            .chain(floor_ceil_sectors.iter().copied())
            .collect();

        // Build position map from existing vertices for O(1) reuse lookup.
        let mut pos_map: HashMap<QuantizedVec3, usize> =
            HashMap::with_capacity(self.vertices.len());
        for (vi, v) in self.vertices.iter().enumerate() {
            let key = QuantizedVec3::from_vec3(*v, QUANT_PRECISION);
            pos_map.entry(key).or_insert(vi);
        }

        for &sector_id in &all_relevant {
            let boundary_pts =
                self.collect_boundary_points(sector_id, segments, subsectors, &sector_segs);
            let floor_h = sectors[sector_id].floorheight.to_f32();
            let ceil_h = sectors[sector_id].ceilingheight.to_f32();
            let in_lower = zh_lower_sectors.contains(&sector_id);
            let in_upper = zh_upper_sectors.contains(&sector_id);
            let in_zh = zh_sectors.contains(&sector_id);
            let in_fc = floor_ceil_sectors.contains(&sector_id);
            let mover_kind = mover_sectors.get(&sector_id).copied();
            let do_floor = in_zh
                || in_fc
                || in_lower
                || matches!(mover_kind, Some(MoverKind::Floor | MoverKind::Both));
            let do_ceil = in_zh
                || in_fc
                || in_upper
                || matches!(mover_kind, Some(MoverKind::Ceiling | MoverKind::Both));
            for pt in &boundary_pts {
                for i in 0..self.sector_subsectors[sector_id].len() {
                    let ss_id = self.sector_subsectors[sector_id][i];
                    if do_floor {
                        self.insert_boundary_vertex(ss_id, *pt, floor_h, true, &mut pos_map);
                    }
                    if do_ceil {
                        self.insert_boundary_vertex(ss_id, *pt, ceil_h, false, &mut pos_map);
                    }
                }
            }
        }

        // Step 3: internal zh sector separation. Clone the moving surface's
        // vertices off the still surface so displacing it drags nothing (a door
        // moves its ceiling, a floor mover its floor; Both clones each).
        for &sector_id in &zh_sectors {
            let clone_floor = matches!(
                mover_sectors.get(&sector_id),
                Some(MoverKind::Floor | MoverKind::Both)
            );
            let clone_ceiling = !matches!(mover_sectors.get(&sector_id), Some(MoverKind::Floor));
            if clone_ceiling {
                self.separate_zh_surface(sector_id, false);
            }
            if clone_floor {
                self.separate_zh_surface(sector_id, true);
            }
        }

        // Step 4: cross-sector boundary separation.
        let mut lower_vertex_map: VertexMap = HashMap::new();
        let mut upper_vertex_map: VertexMap = HashMap::new();

        self.separate_boundary_vertices(
            &zh_lower_bounds,
            sectors,
            &mover_sectors,
            &mut lower_vertex_map,
            true,
        );
        self.separate_boundary_vertices(
            &zh_upper_bounds,
            sectors,
            &mover_sectors,
            &mut upper_vertex_map,
            false,
        );

        // Populate zh sector vertex maps for Step 5 connection.
        for &sector_id in &zh_sectors {
            let height = sectors[sector_id].floorheight.to_f32();
            self.populate_zh_vertex_maps(
                sector_id,
                height,
                &mut lower_vertex_map,
                &mut upper_vertex_map,
            );
        }

        // Step 4b: cross-height separation (floor at ceiling height).
        self.separate_cross_height_vertices(
            &floor_ceil_bounds,
            sectors,
            &mut lower_vertex_map,
            &mut upper_vertex_map,
        );

        // Step 4c: residual mover vertex separation.
        self.residual_mover_separation(
            &zh_upper_bounds,
            &mover_sectors,
            &mut upper_vertex_map,
            false,
        );
        self.residual_mover_separation(
            &zh_lower_bounds,
            &mover_sectors,
            &mut lower_vertex_map,
            true,
        );

        // Step 4d: populate vertex maps from all mover/zh polygon vertices.
        let floor_map_sectors: BTreeSet<usize> = mover_sectors
            .keys()
            .copied()
            .chain(zh_lower_sectors.iter().copied())
            .chain(zh_sectors.iter().copied())
            .collect();
        let ceil_map_sectors: BTreeSet<usize> = mover_sectors
            .keys()
            .copied()
            .chain(zh_upper_sectors.iter().copied())
            .chain(zh_sectors.iter().copied())
            .collect();
        self.populate_vertex_map_from_polys(
            &floor_map_sectors,
            sectors,
            &mut lower_vertex_map,
            true,
        );
        self.populate_vertex_map_from_polys(
            &ceil_map_sectors,
            sectors,
            &mut upper_vertex_map,
            false,
        );

        // Step 5: zh wall vertex connection via ZhWallRecords.
        self.connect_zh_wall_vertices(&lower_vertex_map, &upper_vertex_map);

        // Step 6: non-zh wall vertex connection via linedef lookup.
        self.connect_non_zh_wall_vertices(sectors, linedefs, &lower_vertex_map, &upper_vertex_map);

        // Step 7: set `moves` flag on affected polygons.
        self.set_mover_flags(
            &mover_sectors,
            &zh_lower_sectors,
            &zh_upper_sectors,
            &zh_sectors,
            linedefs,
        );
    }

    /// Expand node AABBs to cover the full vertical range of mover sectors.
    ///
    /// A mover's vertical travel is propagated to its own subsector leaves
    /// **and** to the leaves of two-sided neighbours: the opposite-facing
    /// upper/lower wall of a shared linedef lives in the neighbour's
    /// subsector and tracks this sector's floor/ceiling as it moves.
    /// Without the neighbour expansion that wall's leaf AABB stays at the
    /// static opening and gets frustum-culled once the mover travels past
    /// it (e.g. E1M5 ld808: s48 floor drops, lower wall in s50's leaf
    /// vanishes when looking into the pit).
    pub(super) fn expand_node_aabbs_for_movers(
        &mut self,
        sectors: &[Sector],
        linedefs: &[LineDef],
    ) {
        let tag_linedefs = build_tag_linedef_index(linedefs);
        // Accumulated (min_z, max_z) expansion per subsector leaf.
        let mut expand: Vec<(f32, f32)> = vec![(f32::MAX, f32::MIN); self.subsector_leaves.len()];

        for (sector_id, sector) in sectors.iter().enumerate() {
            let is_mover = classify_sector_mover(sector, linedefs, &tag_linedefs).is_some();
            let is_zero_height = (sector.ceilingheight.to_f32() - sector.floorheight.to_f32())
                .abs()
                <= HEIGHT_EPSILON;

            if !is_mover && !is_zero_height {
                continue;
            }

            let mut min_floor = sector.floorheight.to_f32();
            let mut max_ceil = sector.ceilingheight.to_f32();
            let mut neighbours: Vec<usize> = Vec::new();

            for line in &sector.lines {
                if !line.flags.contains(LineDefFlags::TwoSided) {
                    continue;
                }
                let neighbor = if line.frontsector.num == sector.num {
                    line.backsector.as_ref()
                } else {
                    Some(&line.frontsector)
                };
                if let Some(other) = neighbor {
                    min_floor = min_floor.min(other.floorheight.to_f32());
                    max_ceil = max_ceil.max(other.ceilingheight.to_f32());
                    neighbours.push(other.num as usize);
                }
            }

            // Own leaves plus every two-sided neighbour's leaves: the shared
            // wall driven by this mover lives in the neighbour's subsector.
            let targets = std::iter::once(sector_id).chain(neighbours);
            for tid in targets {
                if tid >= self.sector_subsectors.len() {
                    continue;
                }
                for &subsector_id in &self.sector_subsectors[tid] {
                    let e = &mut expand[subsector_id];
                    e.0 = e.0.min(min_floor);
                    e.1 = e.1.max(max_ceil);
                }
            }
        }

        for (subsector_id, &(min_z, max_z)) in expand.iter().enumerate() {
            let leaf = &mut self.subsector_leaves[subsector_id];
            if min_z < leaf.aabb.min.z {
                leaf.aabb.min.z = min_z;
            }
            if max_z > leaf.aabb.max.z {
                leaf.aabb.max.z = max_z;
            }
        }

        self.update_node_aabbs_recursive(self.root_node);
    }

    // ------------------------------------------------------------------
    // Step helpers
    // ------------------------------------------------------------------

    /// Collect all segment endpoint positions touching `sector_id`,
    /// deduplicated within `DEDUP_EPSILON`.
    fn collect_boundary_points(
        &self,
        sector_id: usize,
        segments: &[Segment],
        subsectors: &[SubSector],
        sector_segs: &[Vec<usize>],
    ) -> Vec<Vec2> {
        let mut seen = HashSet::new();
        let mut pts: Vec<Vec2> = Vec::new();
        let add = |sv: Vec2, seen: &mut HashSet<QuantizedVec2>, pts: &mut Vec<Vec2>| {
            let key = QuantizedVec2::from_vec2(sv, DEDUP_EPSILON);
            if seen.insert(key) {
                pts.push(sv);
            }
        };
        for &si in &sector_segs[sector_id] {
            let seg = &segments[si];
            add(seg.v1.pos, &mut seen, &mut pts);
            add(seg.v2.pos, &mut seen, &mut pts);
            // Also collect vertices from adjacent back-sector subsectors.
            let front_id = seg.frontsector.num as usize;
            if front_id == sector_id
                && let Some(back) = &seg.backsector
            {
                let back_num = back.num as usize;
                for &ss_id in &self.sector_subsectors[back_num] {
                    let ss = &subsectors[ss_id];
                    let start = ss.start_seg as usize;
                    let end = start + ss.seg_count as usize;
                    for gi in start..end {
                        if let Some(gs) = segments.get(gi) {
                            add(gs.v1.pos, &mut seen, &mut pts);
                            add(gs.v2.pos, &mut seen, &mut pts);
                        }
                    }
                }
            }
        }
        pts
    }

    /// Clone the `is_floor` surface's vertices wherever they are shared with
    /// the opposite surface or with another sector's geometry, giving the
    /// moving surface its own indices so displacing it cannot drag the
    /// still surface or a neighbour wall deduped to the same point.
    /// Vertices shared only within `sector_id` (e.g. a wall that travels
    /// with this surface) are left alone.
    fn separate_zh_surface(&mut self, sector_id: usize, is_floor: bool) {
        let mut other_vis = self.collect_sector_poly_vertices(sector_id, !is_floor);
        for ss_id in 0..self.subsector_leaves.len() {
            for gi in self.subsector_leaves[ss_id].polygon_indices.clone() {
                if self.polygons[gi].sector_id != sector_id {
                    other_vis.extend(self.polygons[gi].vertices.iter().copied());
                }
            }
        }
        let mut replaced: HashMap<usize, usize> = HashMap::new();
        for si in 0..self.sector_subsectors[sector_id].len() {
            let ss_id = self.sector_subsectors[sector_id][si];
            let polys = if is_floor {
                self.subsector_leaves[ss_id].floor_polygons.clone()
            } else {
                self.subsector_leaves[ss_id].ceiling_polygons.clone()
            };
            for gi in polys {
                for vi in &mut self.polygons[gi].vertices {
                    if other_vis.contains(vi) {
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

    /// Collect all vertex indices from a sector's floor or ceiling polygons.
    fn collect_sector_poly_vertices(&self, sector_id: usize, is_floor: bool) -> HashSet<usize> {
        self.sector_subsectors[sector_id]
            .iter()
            .flat_map(|&ss_id| {
                let leaf = &self.subsector_leaves[ss_id];
                let indices = if is_floor {
                    &leaf.floor_polygons
                } else {
                    &leaf.ceiling_polygons
                };
                indices
                    .iter()
                    .flat_map(|&gi| self.polygons[gi].vertices.iter().copied())
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// Step 4: separate shared vertices at zh boundaries between sector
    /// pairs. Operates on either floor or ceiling depending on `is_floor`.
    fn separate_boundary_vertices(
        &mut self,
        bounds: &[(Vec2, usize, usize)],
        sectors: &[Sector],
        mover_sectors: &HashMap<usize, MoverKind>,
        vertex_map: &mut VertexMap,
        is_floor: bool,
    ) {
        for &(pos, sector_a, sector_b) in bounds {
            let height = if is_floor {
                sectors[sector_a].floorheight.to_f32()
            } else {
                sectors[sector_a].ceilingheight.to_f32()
            };
            let qp = QuantizedVec2::from_vec2(pos, QUANT_PRECISION);

            let Some(shared_vi) =
                self.find_floor_ceil_vertex_for_sector(sector_a, pos, height, is_floor)
            else {
                continue;
            };

            vertex_map
                .entry(qp)
                .or_default()
                .entry(sector_a)
                .or_insert(shared_vi);

            if self.sector_uses_vertex(sector_b, shared_vi, is_floor) {
                let (keeper, mover_out) = if mover_sectors.contains_key(&sector_a) {
                    (sector_a, sector_b)
                } else {
                    (sector_b, sector_a)
                };
                let new_vi = self.vertices.len();
                self.vertices.push(self.vertices[shared_vi]);
                vertex_map.entry(qp).or_default().insert(mover_out, new_vi);
                vertex_map
                    .entry(qp)
                    .or_default()
                    .entry(keeper)
                    .or_insert(shared_vi);
                self.replace_vertex_in_sector_polys(mover_out, shared_vi, new_vi, pos, is_floor);
            } else if let Some(vi) =
                self.find_floor_ceil_vertex_for_sector(sector_b, pos, height, is_floor)
            {
                vertex_map
                    .entry(qp)
                    .or_default()
                    .entry(sector_b)
                    .or_insert(vi);
            }
        }
    }

    /// Populate zh sector vertex maps with separated floor/ceiling vertex
    /// indices for zh wall connection (Step 5).
    fn populate_zh_vertex_maps(
        &self,
        sector_id: usize,
        height: f32,
        lower_vertex_map: &mut VertexMap,
        upper_vertex_map: &mut VertexMap,
    ) {
        for &ss_id in &self.sector_subsectors[sector_id] {
            let leaf = &self.subsector_leaves[ss_id];
            for &gi in &leaf.floor_polygons {
                for &vi in &self.polygons[gi].vertices {
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
            for &gi in &leaf.ceiling_polygons {
                for &vi in &self.polygons[gi].vertices {
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

    /// Step 4b: separate vertices where a mover's floor meets an adjacent
    /// sector's ceiling at the same height.
    fn separate_cross_height_vertices(
        &mut self,
        bounds: &[(Vec2, usize, usize)],
        sectors: &[Sector],
        lower_vertex_map: &mut VertexMap,
        upper_vertex_map: &mut VertexMap,
    ) {
        for &(pos, floor_sector, ceil_sector) in bounds {
            let height = sectors[floor_sector].floorheight.to_f32();
            let qp = QuantizedVec2::from_vec2(pos, QUANT_PRECISION);

            let Some(floor_vi) =
                self.find_floor_ceil_vertex_for_sector(floor_sector, pos, height, true)
            else {
                continue;
            };

            if self.sector_uses_vertex(ceil_sector, floor_vi, false) {
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
    }

    /// Step 4c: find and separate remaining vertices shared between sector
    /// pairs in `bounds` that were not caught by `separate_boundary_vertices`.
    fn residual_mover_separation(
        &mut self,
        bounds: &[(Vec2, usize, usize)],
        mover_sectors: &HashMap<usize, MoverKind>,
        vertex_map: &mut VertexMap,
        is_floor: bool,
    ) {
        let mut pairs: BTreeSet<(usize, usize)> = BTreeSet::new();
        for &(_, a, b) in bounds {
            if a != b {
                pairs.insert(if a < b { (a, b) } else { (b, a) });
            }
        }
        for (sector_a, sector_b) in &pairs {
            let vis_a = self.collect_sector_poly_vertices(*sector_a, is_floor);
            let shared: Vec<usize> = self
                .collect_sector_poly_vertices(*sector_b, is_floor)
                .into_iter()
                .filter(|vi| vis_a.contains(vi))
                .collect();
            for vi in shared {
                let (keeper, other) = if mover_sectors.contains_key(sector_a) {
                    (*sector_a, *sector_b)
                } else {
                    (*sector_b, *sector_a)
                };
                let pos = self.vertices[vi];
                let pos2 = Vec2::new(pos.x, pos.y);
                let new_vi = self.vertices.len();
                self.vertices.push(pos);
                self.replace_vertex_in_sector_polys(other, vi, new_vi, pos2, is_floor);
                let qp = QuantizedVec2::from_vec2(pos2, QUANT_PRECISION);
                vertex_map.entry(qp).or_default().insert(other, new_vi);
                vertex_map
                    .entry(qp)
                    .or_default()
                    .entry(keeper)
                    .or_insert(vi);
            }
        }
    }

    /// Step 4d: ensure every vertex in mover/zh sector polygons has an entry
    /// in the vertex map so Steps 5-6 can link wall vertices.
    fn populate_vertex_map_from_polys(
        &self,
        sector_ids: &BTreeSet<usize>,
        sectors: &[Sector],
        vertex_map: &mut VertexMap,
        is_floor: bool,
    ) {
        for &sector_id in sector_ids {
            let height = if is_floor {
                sectors[sector_id].floorheight.to_f32()
            } else {
                sectors[sector_id].ceilingheight.to_f32()
            };
            for i in 0..self.sector_subsectors[sector_id].len() {
                let ss_id = self.sector_subsectors[sector_id][i];
                let leaf = &self.subsector_leaves[ss_id];
                let indices = if is_floor {
                    &leaf.floor_polygons
                } else {
                    &leaf.ceiling_polygons
                };
                for &gi in indices {
                    for &vi in &self.polygons[gi].vertices {
                        let v = self.vertices[vi];
                        if (v.z - height).abs() < HEIGHT_EPSILON {
                            let qp = QuantizedVec2::from_vec2(Vec2::new(v.x, v.y), QUANT_PRECISION);
                            vertex_map
                                .entry(qp)
                                .or_default()
                                .entry(sector_id)
                                .or_insert(vi);
                        }
                    }
                }
            }
        }
    }

    /// Step 5: connect zh wall polygon vertices to the separated
    /// floor/ceiling polygon vertices via ZhWallRecords.
    fn connect_zh_wall_vertices(
        &mut self,
        lower_vertex_map: &VertexMap,
        upper_vertex_map: &VertexMap,
    ) {
        for ri in 0..self.zh_wall_records.len() {
            let rec = &self.zh_wall_records[ri];
            let poly_index = rec.poly_index;

            let pairs: [(usize, usize, &VertexMap); 4] = match rec.wall_type {
                WallType::Lower => [
                    (rec.bottom[0], rec.front_sector, lower_vertex_map),
                    (rec.bottom[1], rec.front_sector, lower_vertex_map),
                    (rec.top[0], rec.back_sector, lower_vertex_map),
                    (rec.top[1], rec.back_sector, lower_vertex_map),
                ],
                WallType::Upper => [
                    (rec.bottom[0], rec.back_sector, upper_vertex_map),
                    (rec.bottom[1], rec.back_sector, upper_vertex_map),
                    (rec.top[0], rec.front_sector, upper_vertex_map),
                    (rec.top[1], rec.front_sector, upper_vertex_map),
                ],
                WallType::Middle => {
                    if rec.front_sector == rec.back_sector {
                        [
                            (rec.bottom[0], rec.front_sector, lower_vertex_map),
                            (rec.bottom[1], rec.front_sector, lower_vertex_map),
                            (rec.top[0], rec.front_sector, upper_vertex_map),
                            (rec.top[1], rec.front_sector, upper_vertex_map),
                        ]
                    } else {
                        continue;
                    }
                }
            };

            for (wall_vi, sector_id, vmap) in pairs {
                let pos = self.vertices[wall_vi];
                let qp = QuantizedVec2::from_vec2(Vec2::new(pos.x, pos.y), QUANT_PRECISION);
                if let Some(target_vi) = qp.lookup(vmap, sector_id) {
                    for vi in &mut self.polygons[poly_index].vertices {
                        if *vi == wall_vi {
                            *vi = target_vi;
                        }
                    }
                }
            }
        }
    }

    /// Step 6: connect non-zh wall polygon vertices to floor/ceiling polygon
    /// vertices using linedef sector information.
    fn connect_non_zh_wall_vertices(
        &mut self,
        sectors: &[Sector],
        linedefs: &[LineDef],
        lower_vertex_map: &VertexMap,
        upper_vertex_map: &VertexMap,
    ) {
        for ss_id in 0..self.subsector_leaves.len() {
            for gi in self.subsector_leaves[ss_id].polygon_indices.clone() {
                let poly = &self.polygons[gi];
                let (wall_type, linedef_id) = match &poly.surface_kind {
                    SurfaceKind::Vertical {
                        wall_type,
                        linedef_id,
                        ..
                    } => (*wall_type, *linedef_id),
                    SurfaceKind::Horizontal {
                        ..
                    } => continue,
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
                let Some(ld_back_sector) = &ld.backsector else {
                    continue;
                };
                let ld_back = ld_back_sector.num as usize;
                let wall_front = poly.sector_id;
                let wall_back = if wall_front == ld_front {
                    ld_back
                } else {
                    ld_front
                };

                for (vi_idx, &vi) in verts.iter().enumerate() {
                    let v = self.vertices[vi];
                    let qp = QuantizedVec2::from_vec2(Vec2::new(v.x, v.y), QUANT_PRECISION);

                    match wall_type {
                        WallType::Lower => {
                            self.try_link_wall_vertex(
                                gi,
                                vi_idx,
                                vi,
                                &qp,
                                v.z,
                                sectors[wall_front].floorheight.to_f32(),
                                wall_front,
                                lower_vertex_map,
                            );
                            self.try_link_wall_vertex(
                                gi,
                                vi_idx,
                                vi,
                                &qp,
                                v.z,
                                sectors[wall_back].floorheight.to_f32(),
                                wall_back,
                                lower_vertex_map,
                            );
                        }
                        WallType::Upper => {
                            self.try_link_wall_vertex(
                                gi,
                                vi_idx,
                                vi,
                                &qp,
                                v.z,
                                sectors[wall_front].ceilingheight.to_f32(),
                                wall_front,
                                upper_vertex_map,
                            );
                            self.try_link_wall_vertex(
                                gi,
                                vi_idx,
                                vi,
                                &qp,
                                v.z,
                                sectors[wall_back].ceilingheight.to_f32(),
                                wall_back,
                                upper_vertex_map,
                            );
                        }
                        WallType::Middle => {}
                    }
                }
            }
        }
    }

    /// Try to replace wall vertex `vi` at `vi_idx` with the floor/ceiling
    /// polygon vertex from `vertex_map` if the vertex height matches.
    fn try_link_wall_vertex(
        &mut self,
        gi: usize,
        vi_idx: usize,
        vi: usize,
        qp: &QuantizedVec2,
        vertex_z: f32,
        sector_height: f32,
        sector_id: usize,
        vertex_map: &VertexMap,
    ) {
        if (vertex_z - sector_height).abs() < HEIGHT_EPSILON
            && let Some(target_vi) = qp.lookup(vertex_map, sector_id)
            && vi != target_vi
        {
            self.polygons[gi].vertices[vi_idx] = target_vi;
        }
    }

    /// Step 7: set `moves` flag on all polygons in mover sectors.
    fn set_mover_flags(
        &mut self,
        mover_sectors: &HashMap<usize, MoverKind>,
        zh_lower_sectors: &HashSet<usize>,
        zh_upper_sectors: &HashSet<usize>,
        zh_sectors: &BTreeSet<usize>,
        linedefs: &[LineDef],
    ) {
        let floor_movers: BTreeSet<usize> = mover_sectors
            .keys()
            .copied()
            .chain(zh_lower_sectors.iter().copied())
            .chain(zh_sectors.iter().copied())
            .collect();
        let ceil_movers: BTreeSet<usize> = mover_sectors
            .keys()
            .copied()
            .chain(zh_upper_sectors.iter().copied())
            .chain(zh_sectors.iter().copied())
            .collect();

        for &sector_id in &floor_movers {
            for i in 0..self.sector_subsectors[sector_id].len() {
                let ss_id = self.sector_subsectors[sector_id][i];
                for gi in self.subsector_leaves[ss_id].floor_polygons.clone() {
                    self.polygons[gi].moves = true;
                }
            }
        }
        for &sector_id in &ceil_movers {
            for i in 0..self.sector_subsectors[sector_id].len() {
                let ss_id = self.sector_subsectors[sector_id][i];
                for gi in self.subsector_leaves[ss_id].ceiling_polygons.clone() {
                    self.polygons[gi].moves = true;
                }
            }
        }
        // Zh wall polygons.
        for ri in 0..self.zh_wall_records.len() {
            let poly_index = self.zh_wall_records[ri].poly_index;
            self.polygons[poly_index].moves = true;
        }
        // Non-zh wall polygons at mover boundaries.
        for ss_id in 0..self.subsector_leaves.len() {
            for gi in self.subsector_leaves[ss_id].polygon_indices.clone() {
                let poly = &self.polygons[gi];
                let linedef_id = match &poly.surface_kind {
                    SurfaceKind::Vertical {
                        linedef_id,
                        wall_type: WallType::Lower | WallType::Upper,
                        ..
                    } => *linedef_id,
                    _ => continue,
                };
                if poly.moves {
                    continue;
                }
                let ld = &linedefs[linedef_id];
                let ld_front = ld.frontsector.num as usize;
                let Some(ld_back_sector) = &ld.backsector else {
                    continue;
                };
                let ld_back = ld_back_sector.num as usize;
                if mover_sectors.contains_key(&ld_front) || mover_sectors.contains_key(&ld_back) {
                    self.polygons[gi].moves = true;
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Low-level helpers
    // ------------------------------------------------------------------

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
            for &gi in indices {
                for &vi in &self.polygons[gi].vertices {
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
            for &gi in indices {
                if self.polygons[gi].vertices.contains(&vi) {
                    return true;
                }
            }
        }
        false
    }

    /// Insert a boundary vertex into a subsector's floor or ceiling N-gon
    /// if the point lies on a polygon edge but is not already a vertex.
    /// The inserted vertex position is the projection onto the edge (not the
    /// raw boundary point) to preserve polygon planarity and winding.
    fn insert_boundary_vertex(
        &mut self,
        ss_id: usize,
        pt: Vec2,
        height: f32,
        is_floor: bool,
        pos_map: &mut HashMap<QuantizedVec3, usize>,
    ) {
        let leaf = &self.subsector_leaves[ss_id];
        let poly_indices = if is_floor {
            &leaf.floor_polygons
        } else {
            &leaf.ceiling_polygons
        };
        if poly_indices.is_empty() {
            return;
        }
        let gi = poly_indices[0];
        let verts = &self.polygons[gi].vertices;
        let n = verts.len();

        // Already a vertex at this position?
        for &vi in verts {
            let v = self.vertices[vi];
            if (v.x - pt.x).abs() < QUANT_PRECISION && (v.y - pt.y).abs() < QUANT_PRECISION {
                return;
            }
        }

        // Find the edge this point lies on via projection.
        let verts = self.polygons[gi].vertices.clone();
        for i in 0..n {
            let j = (i + 1) % n;
            let a = self.vertices[verts[i]];
            let b = self.vertices[verts[j]];
            let ab = Vec2::new(b.x - a.x, b.y - a.y);
            let ab_len_sq = ab.length_squared();
            if ab_len_sq < 1e-6 {
                continue;
            }
            let ap = Vec2::new(pt.x - a.x, pt.y - a.y);
            let t = ap.dot(ab) / ab_len_sq;
            if !(-0.01..=1.01).contains(&t) {
                continue;
            }
            let proj = Vec2::new(a.x + t * ab.x, a.y + t * ab.y);
            let dist = (proj - pt).length();
            if dist > EDGE_INSERT_EPSILON {
                continue;
            }
            // Reuse existing vertex at this position if one exists
            // (e.g. wall quad vertex). Otherwise create fresh.
            let target_pos = Vec3::new(proj.x, proj.y, height);
            let key = QuantizedVec3::from_vec3(target_pos, QUANT_PRECISION);
            let ins_vi = if let Some(&vi) = pos_map.get(&key) {
                vi
            } else {
                let vi = self.vertices.len();
                self.vertices.push(target_pos);
                pos_map.insert(key, vi);
                vi
            };
            // Guard against inserting a vertex index already in the polygon.
            if self.polygons[gi].vertices.contains(&ins_vi) {
                return;
            }
            // When the edge wraps (last→first), append rather than
            // prepend so the vertex sits between the last and first.
            let cur_len = self.polygons[gi].vertices.len();
            let insert_pos = if j == 0 { cur_len } else { j };
            self.polygons[gi].vertices.insert(insert_pos, ins_vi);
            return;
        }
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
        for i in 0..self.sector_subsectors[sector_id].len() {
            let ss_id = self.sector_subsectors[sector_id][i];
            let fc_indices = if is_floor {
                self.subsector_leaves[ss_id].floor_polygons.clone()
            } else {
                self.subsector_leaves[ss_id].ceiling_polygons.clone()
            };
            for gi in fc_indices {
                for vi in &mut self.polygons[gi].vertices {
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
            for gi in self.subsector_leaves[ss_id].polygon_indices.clone() {
                let poly = &self.polygons[gi];
                if !matches!(poly.surface_kind, SurfaceKind::Vertical { .. }) {
                    continue;
                }
                if poly.sector_id != sector_id {
                    continue;
                }
                for vi in &mut self.polygons[gi].vertices {
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
}
