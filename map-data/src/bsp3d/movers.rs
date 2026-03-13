//! Mover vertex pass: separates shared vertices at zero-height boundaries,
//! connects wall vertices to floor/ceiling polygons, and sets `moves` flags
//! for sectors that participate in lifts, doors, and platforms.

use super::build::{BSP3D, HEIGHT_EPSILON, QUANT_PRECISION, SurfaceKind, WallType};
use crate::flags::LineDefFlags;
use crate::map_defs::{LineDef, Sector, Segment, SubSector};
use glam::{Vec2, Vec3};
use std::collections::{HashMap, HashSet};

/// Deduplication tolerance for vertex proximity checks.
const DEDUP_EPSILON: f32 = 0.1;
/// Max perpendicular distance for point-on-edge detection during boundary
/// vertex insertion into N-gon floor/ceiling polygons.
const EDGE_INSERT_EPSILON: f32 = 1.0;

/// Type alias for the per-position, per-sector vertex index maps used to
/// connect wall vertices to floor/ceiling polygon vertices.
type VertexMap = HashMap<QuantizedVec2, HashMap<usize, usize>>;

/// Check if a sector participates in any line-special-triggered movement.
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
        map.get(self)
            .and_then(|sector_map| sector_map.get(&sector_id).copied())
    }
}

/// Add a boundary point to `bounds` if no existing entry matches `pos` for
/// the same sector pair within `DEDUP_EPSILON`.
fn push_dedup_bound(bounds: &mut Vec<(Vec2, usize, usize)>, pos: Vec2, a: usize, b: usize) {
    if !bounds
        .iter()
        .any(|(p, ea, eb)| (*p - pos).length() < DEDUP_EPSILON && *ea == a && *eb == b)
    {
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
    /// 3. Internal zh sector separation (floor vs ceiling)
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

        // Texture-marked zh boundaries.
        for seg in segments {
            let Some(back) = &seg.backsector else {
                continue;
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
                    push_dedup_bound(&mut zh_lower_bounds, sv, front_id, back_id);
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
                    push_dedup_bound(&mut zh_upper_bounds, sv, front_id, back_id);
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
            if !mover_sectors.contains(&front_id) && !mover_sectors.contains(&back_id) {
                continue;
            }
            if (seg.frontsector.floorheight - back.floorheight).abs() <= HEIGHT_EPSILON {
                for sv in [*seg.v1, *seg.v2] {
                    push_dedup_bound(&mut zh_lower_bounds, sv, front_id, back_id);
                }
            }
            if (seg.frontsector.ceilingheight - back.ceilingheight).abs() <= HEIGHT_EPSILON {
                for sv in [*seg.v1, *seg.v2] {
                    push_dedup_bound(&mut zh_upper_bounds, sv, front_id, back_id);
                }
            }
        }

        // Floor/ceiling crossings: mover floor at adjacent ceiling height.
        let mut floor_ceil_bounds: Vec<(Vec2, usize, usize)> = Vec::new();
        let mut floor_ceil_sectors: HashSet<usize> = HashSet::new();
        for seg in segments {
            let Some(back) = &seg.backsector else {
                continue;
            };
            let front_id = seg.frontsector.num as usize;
            let back_id = back.num as usize;
            if !mover_sectors.contains(&front_id) && !mover_sectors.contains(&back_id) {
                continue;
            }
            if (seg.frontsector.floorheight - back.ceilingheight).abs() <= HEIGHT_EPSILON {
                floor_ceil_sectors.insert(front_id);
                floor_ceil_sectors.insert(back_id);
                for sv in [*seg.v1, *seg.v2] {
                    push_dedup_bound(&mut floor_ceil_bounds, sv, front_id, back_id);
                }
            }
            if (back.floorheight - seg.frontsector.ceilingheight).abs() <= HEIGHT_EPSILON {
                floor_ceil_sectors.insert(front_id);
                floor_ceil_sectors.insert(back_id);
                for sv in [*seg.v1, *seg.v2] {
                    push_dedup_bound(&mut floor_ceil_bounds, sv, back_id, front_id);
                }
            }
        }

        if mover_sectors.is_empty() && zh_sectors.is_empty() && floor_ceil_bounds.is_empty() {
            return;
        }

        // Step 2: insert missing boundary vertices into floor/ceiling N-gons.
        let all_relevant: HashSet<usize> = zh_lower_sectors
            .union(&zh_upper_sectors)
            .copied()
            .chain(zh_sectors.iter().copied())
            .chain(mover_sectors.iter().copied())
            .chain(floor_ceil_sectors.iter().copied())
            .collect();

        for &sector_id in &all_relevant {
            let boundary_pts = self.collect_boundary_points(sector_id, segments, subsectors);
            let floor_h = sectors[sector_id].floorheight;
            let ceil_h = sectors[sector_id].ceilingheight;
            for pt in &boundary_pts {
                for ss_id in self.sector_subsectors[sector_id].clone() {
                    self.insert_boundary_vertex(ss_id, *pt, floor_h, true);
                    self.insert_boundary_vertex(ss_id, *pt, ceil_h, false);
                }
            }
        }

        // Step 3: internal zh sector separation (floor vs ceiling).
        for &sector_id in &zh_sectors {
            let floor_vis = self.collect_sector_poly_vertices(sector_id, true);
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
            let height = sectors[sector_id].floorheight;
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
        let floor_map_sectors: HashSet<usize> = mover_sectors
            .union(&zh_lower_sectors)
            .copied()
            .chain(zh_sectors.iter().copied())
            .collect();
        let ceil_map_sectors: HashSet<usize> = mover_sectors
            .union(&zh_upper_sectors)
            .copied()
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
    pub(super) fn expand_node_aabbs_for_movers(
        &mut self,
        sectors: &[Sector],
        linedefs: &[LineDef],
    ) {
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
    ) -> Vec<Vec2> {
        let mut pts: Vec<Vec2> = Vec::new();
        let mut add = |sv: Vec2| {
            if !pts.iter().any(|p| (*p - sv).length() < DEDUP_EPSILON) {
                pts.push(sv);
            }
        };
        for seg in segments {
            let front_id = seg.frontsector.num as usize;
            let back_id = seg.backsector.as_ref().map(|b| b.num as usize);
            if front_id == sector_id || back_id == Some(sector_id) {
                add(*seg.v1);
                add(*seg.v2);
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
                                add(*gs.v1);
                                add(*gs.v2);
                            }
                        }
                    }
                }
            }
        }
        pts
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
                    .flat_map(|&pi| leaf.polygons[pi].vertices.iter().copied())
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
        mover_sectors: &HashSet<usize>,
        vertex_map: &mut VertexMap,
        is_floor: bool,
    ) {
        for &(pos, sector_a, sector_b) in bounds {
            let height = if is_floor {
                sectors[sector_a].floorheight
            } else {
                sectors[sector_a].ceilingheight
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
                let (keeper, mover_out) = if mover_sectors.contains(&sector_a) {
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
            let height = sectors[floor_sector].floorheight;
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
        mover_sectors: &HashSet<usize>,
        vertex_map: &mut VertexMap,
        is_floor: bool,
    ) {
        let mut pairs: HashSet<(usize, usize)> = HashSet::new();
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
                let (keeper, other) = if mover_sectors.contains(sector_a) {
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
        sector_ids: &HashSet<usize>,
        sectors: &[Sector],
        vertex_map: &mut VertexMap,
        is_floor: bool,
    ) {
        for &sector_id in sector_ids {
            let height = if is_floor {
                sectors[sector_id].floorheight
            } else {
                sectors[sector_id].ceilingheight
            };
            for ss_id in self.sector_subsectors[sector_id].clone() {
                let leaf = &self.subsector_leaves[ss_id];
                let indices = if is_floor {
                    &leaf.floor_polygons
                } else {
                    &leaf.ceiling_polygons
                };
                for &pi in indices {
                    for &vi in &leaf.polygons[pi].vertices {
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
        for rec in &self.zh_wall_records.clone() {
            let leaf = &mut self.subsector_leaves[rec.subsector_id];

            let pairs: Vec<(usize, usize, &VertexMap)> = match rec.wall_type {
                WallType::Lower => vec![
                    (rec.bottom[0], rec.front_sector, lower_vertex_map),
                    (rec.bottom[1], rec.front_sector, lower_vertex_map),
                    (rec.top[0], rec.back_sector, lower_vertex_map),
                    (rec.top[1], rec.back_sector, lower_vertex_map),
                ],
                WallType::Upper => vec![
                    (rec.bottom[0], rec.back_sector, upper_vertex_map),
                    (rec.bottom[1], rec.back_sector, upper_vertex_map),
                    (rec.top[0], rec.front_sector, upper_vertex_map),
                    (rec.top[1], rec.front_sector, upper_vertex_map),
                ],
                WallType::Middle => {
                    if rec.front_sector == rec.back_sector {
                        vec![
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
                    for vi in &mut leaf.polygons[rec.poly_index].vertices {
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

                for vi_idx in 0..verts.len() {
                    let vi = verts[vi_idx];
                    let v = self.vertices[vi];
                    let qp = QuantizedVec2::from_vec2(Vec2::new(v.x, v.y), QUANT_PRECISION);

                    match wall_type {
                        WallType::Lower => {
                            self.try_link_wall_vertex(
                                ss_id,
                                pi,
                                vi_idx,
                                vi,
                                &qp,
                                v.z,
                                sectors[wall_front].floorheight,
                                wall_front,
                                lower_vertex_map,
                            );
                            self.try_link_wall_vertex(
                                ss_id,
                                pi,
                                vi_idx,
                                vi,
                                &qp,
                                v.z,
                                sectors[wall_back].floorheight,
                                wall_back,
                                lower_vertex_map,
                            );
                        }
                        WallType::Upper => {
                            self.try_link_wall_vertex(
                                ss_id,
                                pi,
                                vi_idx,
                                vi,
                                &qp,
                                v.z,
                                sectors[wall_front].ceilingheight,
                                wall_front,
                                upper_vertex_map,
                            );
                            self.try_link_wall_vertex(
                                ss_id,
                                pi,
                                vi_idx,
                                vi,
                                &qp,
                                v.z,
                                sectors[wall_back].ceilingheight,
                                wall_back,
                                upper_vertex_map,
                            );
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    /// Try to replace wall vertex `vi` at `vi_idx` with the floor/ceiling
    /// polygon vertex from `vertex_map` if the vertex height matches.
    fn try_link_wall_vertex(
        &mut self,
        ss_id: usize,
        pi: usize,
        vi_idx: usize,
        vi: usize,
        qp: &QuantizedVec2,
        vertex_z: f32,
        sector_height: f32,
        sector_id: usize,
        vertex_map: &VertexMap,
    ) {
        if (vertex_z - sector_height).abs() < HEIGHT_EPSILON {
            if let Some(target_vi) = qp.lookup(vertex_map, sector_id) {
                if vi != target_vi {
                    self.subsector_leaves[ss_id].polygons[pi].vertices[vi_idx] = target_vi;
                }
            }
        }
    }

    /// Step 7: set `moves` flag on all polygons in mover sectors.
    fn set_mover_flags(
        &mut self,
        mover_sectors: &HashSet<usize>,
        zh_lower_sectors: &HashSet<usize>,
        zh_upper_sectors: &HashSet<usize>,
        zh_sectors: &HashSet<usize>,
        linedefs: &[LineDef],
    ) {
        let floor_movers: HashSet<usize> = mover_sectors
            .union(zh_lower_sectors)
            .copied()
            .chain(zh_sectors.iter().copied())
            .collect();
        let ceil_movers: HashSet<usize> = mover_sectors
            .union(zh_upper_sectors)
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
            leaf.polygons[rec.poly_index].moves = true;
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
                let Some(ld_back_sector) = &ld.backsector else {
                    continue;
                };
                let ld_back = ld_back_sector.num as usize;
                if mover_sectors.contains(&ld_front) || mover_sectors.contains(&ld_back) {
                    self.subsector_leaves[ss_id].polygons[pi].moves = true;
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

    /// Insert a boundary vertex into a subsector's floor or ceiling N-gon
    /// if the point lies on a polygon edge but is not already a vertex.
    /// The inserted vertex position is the projection onto the edge (not the
    /// raw boundary point) to preserve polygon planarity and winding.
    fn insert_boundary_vertex(&mut self, ss_id: usize, pt: Vec2, height: f32, is_floor: bool) {
        let leaf = &self.subsector_leaves[ss_id];
        let poly_indices = if is_floor {
            &leaf.floor_polygons
        } else {
            &leaf.ceiling_polygons
        };
        if poly_indices.is_empty() {
            return;
        }
        let pi = poly_indices[0];
        let verts = &self.subsector_leaves[ss_id].polygons[pi].vertices;
        let n = verts.len();

        // Already a vertex at this position?
        for &vi in verts {
            let v = self.vertices[vi];
            if (v.x - pt.x).abs() < QUANT_PRECISION && (v.y - pt.y).abs() < QUANT_PRECISION {
                return;
            }
        }

        // Find the edge this point lies on via projection.
        let verts = self.subsector_leaves[ss_id].polygons[pi].vertices.clone();
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
            if t < -0.01 || t > 1.01 {
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
            let reuse_vi = self.vertices.iter().enumerate().find_map(|(vi, v)| {
                if (v.x - target_pos.x).abs() < QUANT_PRECISION
                    && (v.y - target_pos.y).abs() < QUANT_PRECISION
                    && (v.z - target_pos.z).abs() < HEIGHT_EPSILON
                {
                    Some(vi)
                } else {
                    None
                }
            });
            let ins_vi = reuse_vi.unwrap_or_else(|| {
                let vi = self.vertices.len();
                self.vertices.push(target_pos);
                vi
            });
            // Guard against inserting a vertex index already in the polygon.
            if self.subsector_leaves[ss_id].polygons[pi]
                .vertices
                .contains(&ins_vi)
            {
                return;
            }
            self.subsector_leaves[ss_id].polygons[pi]
                .vertices
                .insert(j, ins_vi);
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
}
