use std::collections::HashMap;
use std::time::Instant;

use crate::map_data::{is_subsector, subsector_index};
use crate::map_defs::{LineDef, Node, Segment, SubSector};
use glam::Vec2;
use log::{debug, info, warn};
use wad::WadData;
use wad::extended::WadExtendedMap;
use wad::types::WadSegment;

/// Maximum number of vertices allowed in polygon clipping.
const MAX_CLIP_VERTICES: usize = 128;

/// Epsilon for vertex classification against a half-space (map units).
/// Vertices within this distance of the splitting line are classified as `On`,
/// preventing misclassification due to floating-point rounding.
const ON_EPSILON: f64 = 0.01;

/// Minimum polygon area (map units²) below which a polygon is discarded as
/// degenerate.
const MIN_AREA: f64 = 0.5;

/// World-bounds half-extent for the initial clipping polygon (map units).
const WORLD_SIZE: f64 = 32768.0;

/// Near-duplicate distance² threshold for cleanup.
const DEDUP_DIST_SQ: f64 = 0.01 * 0.01;

/// Collinear cross-product threshold for cleanup.
const COLLINEAR_THRESHOLD: f64 = 0.1;

/// Maximum snap distance to a canonical intersection point (map units).
const CANONICAL_SNAP_DIST: f64 = 1.0;

/// Deduplication distance for canonical intersection points (map units).
const CANONICAL_DEDUP_DIST: f64 = 0.5;

/// Spatial grid cell size for canonical intersection point lookup.
const CANONICAL_CELL: f64 = 4.0;

/// Minimum cross-product magnitude for non-parallel divline intersection.
const PARALLEL_EPSILON: f64 = 1e-10;

/// Tolerance for point-on-edge distance check (map units).
const POINT_ON_EDGE_EPSILON: f64 = 0.5;

/// Vertex classification relative to a half-space.
#[derive(Clone, Copy, PartialEq)]
enum Side {
    /// Vertex is on the front (inside) side of the splitting line.
    Front,
    /// Vertex is on the back (outside) side of the splitting line.
    Back,
    /// Vertex is within [`ON_EPSILON`] of the splitting line.
    On,
}

/// BSP splitting line used as a half-space clipper.
///
/// All arithmetic is f64 to avoid precision loss at Doom's map scales
/// (up to 32768 units). The line is defined by an origin `(x, y)` and a
/// direction vector `(dx, dy)`. The "inside" half-space is to the right of
/// the direction vector (positive signed-distance side).
#[derive(Debug, Clone, Copy)]
pub struct DivLine {
    /// X coordinate of the splitting line origin.
    pub x: f64,
    /// Y coordinate of the splitting line origin.
    pub y: f64,
    /// X component of the splitting line direction.
    pub dx: f64,
    /// Y component of the splitting line direction.
    pub dy: f64,
}

impl DivLine {
    /// Create a dividing line from a BSP node (f32 → f64 promotion).
    pub fn from_node(node: &Node) -> Self {
        Self {
            x: node.xy.x as f64,
            y: node.xy.y as f64,
            dx: node.delta.x as f64,
            dy: node.delta.y as f64,
        }
    }

    /// Signed perpendicular distance from `(px, py)` to this line.
    /// Positive = front (right of direction vector), negative = back (left).
    fn signed_distance(&self, px: f64, py: f64) -> f64 {
        let len_sq = self.dx * self.dx + self.dy * self.dy;
        if len_sq < 1e-20 {
            return 0.0;
        }
        let inv_len = 1.0 / len_sq.sqrt();
        let nx = self.dy * inv_len;
        let ny = -self.dx * inv_len;
        (px - self.x) * nx + (py - self.y) * ny
    }

    /// Classify a point relative to this splitting line.
    fn classify(&self, px: f64, py: f64) -> Side {
        let d = self.signed_distance(px, py);
        if d > ON_EPSILON {
            Side::Front
        } else if d < -ON_EPSILON {
            Side::Back
        } else {
            Side::On
        }
    }

    /// Compute the intersection of two infinite lines. Returns `None` if
    /// the lines are parallel (cross-product magnitude below threshold).
    pub fn intersect_line(&self, other: &DivLine) -> Option<(f64, f64)> {
        let cross = self.dx * other.dy - self.dy * other.dx;
        if cross.abs() < PARALLEL_EPSILON {
            return None;
        }
        let ox = other.x - self.x;
        let oy = other.y - self.y;
        let t = (ox * other.dy - oy * other.dx) / cross;
        Some((self.x + t * self.dx, self.y + t * self.dy))
    }

    /// Compute the intersection point of this line with edge `a→b`.
    fn intersect_edge(&self, ax: f64, ay: f64, bx: f64, by: f64) -> (f64, f64) {
        let da = self.signed_distance(ax, ay);
        let db = self.signed_distance(bx, by);
        let denom = da - db;
        if denom.abs() < 1e-20 {
            return (ax, ay);
        }
        let t = da / denom;
        (ax + t * (bx - ax), ay + t * (by - ay))
    }
}

/// Clip a convex polygon against a sequence of half-space dividing lines
/// using Sutherland-Hodgman. Each clipper retains the "inside" (right-of-
/// direction) half-space. All arithmetic is f64.
fn clip_polygon(initial: &[(f64, f64)], clippers: &[DivLine]) -> Vec<(f64, f64)> {
    let mut poly: Vec<(f64, f64)> = initial.to_vec();

    for clipper in clippers {
        if poly.is_empty() || poly.len() >= MAX_CLIP_VERTICES {
            break;
        }

        let input = std::mem::take(&mut poly);
        let n = input.len();

        for i in 0..n {
            let (cx, cy) = input[i];
            let (nx, ny) = input[(i + 1) % n];
            let cs = clipper.classify(cx, cy);
            let ns = clipper.classify(nx, ny);

            match cs {
                Side::Front | Side::On => {
                    poly.push((cx, cy));
                    if ns == Side::Back {
                        poly.push(clipper.intersect_edge(cx, cy, nx, ny));
                    }
                }
                Side::Back => {
                    if ns == Side::Front || ns == Side::On {
                        poly.push(clipper.intersect_edge(cx, cy, nx, ny));
                    }
                }
            }
        }

        if poly.is_empty() {
            break;
        }
    }

    poly
}

/// Remove near-duplicate vertices, collinear vertices, and discard degenerate
/// polygons (area < [`MIN_AREA`] map units²).
fn cleanup_polygon(vertices: &mut Vec<(f64, f64)>) {
    if vertices.len() < 3 {
        vertices.clear();
        return;
    }

    // 1. Remove near-duplicate consecutive vertices.
    let mut i = 0;
    while i < vertices.len() && vertices.len() >= 3 {
        let prev = if i == 0 { vertices.len() - 1 } else { i - 1 };
        let dx = vertices[i].0 - vertices[prev].0;
        let dy = vertices[i].1 - vertices[prev].1;
        if dx * dx + dy * dy < DEDUP_DIST_SQ {
            vertices.remove(i);
        } else {
            i += 1;
        }
    }

    if vertices.len() < 3 {
        vertices.clear();
        return;
    }

    // 2. Remove collinear vertices.
    let mut i = 0;
    while i < vertices.len() && vertices.len() >= 3 {
        let n = vertices.len();
        let prev = if i == 0 { n - 1 } else { i - 1 };
        let next = (i + 1) % n;
        let (ax, ay) = vertices[prev];
        let (bx, by) = vertices[i];
        let (cx, cy) = vertices[next];
        let cross = (bx - ax) * (cy - by) - (by - ay) * (cx - bx);
        if cross.abs() < COLLINEAR_THRESHOLD {
            vertices.remove(i);
        } else {
            i += 1;
        }
    }

    if vertices.len() < 3 {
        vertices.clear();
        return;
    }

    // 3. Discard degenerate slivers by area.
    let n = vertices.len();
    let mut area = 0.0_f64;
    for i in 0..n {
        let j = (i + 1) % n;
        area += vertices[i].0 * vertices[j].1;
        area -= vertices[j].0 * vertices[i].1;
    }
    if area.abs() * 0.5 < MIN_AREA {
        vertices.clear();
    }
}

/// Precomputed canonical intersection points from all BSP divline pairs and
/// segment endpoints. Used to snap SH-clipped polygon vertices to ensure
/// adjacent subsectors sharing a geometric point get identical coordinates.
pub struct IntersectionCache {
    /// All canonical points.
    points: Vec<(f64, f64)>,
    /// Spatial grid mapping cell coords to indices in `points`.
    grid: HashMap<(i32, i32), Vec<usize>>,
}

impl Default for IntersectionCache {
    fn default() -> Self {
        Self {
            points: Vec::new(),
            grid: HashMap::new(),
        }
    }
}

impl IntersectionCache {
    /// Snap a vertex to the nearest canonical point within `max_dist`.
    /// Returns the snapped position, or the original if no point is near.
    pub fn snap_vertex(&self, x: f64, y: f64, max_dist: f64) -> (f64, f64) {
        let max_dist_sq = max_dist * max_dist;
        let cx = (x / CANONICAL_CELL).floor() as i32;
        let cy = (y / CANONICAL_CELL).floor() as i32;

        let mut best_dist_sq = max_dist_sq;
        let mut best = None;

        for gx in (cx - 1)..=(cx + 1) {
            for gy in (cy - 1)..=(cy + 1) {
                if let Some(indices) = self.grid.get(&(gx, gy)) {
                    for &idx in indices {
                        let (px, py) = self.points[idx];
                        let dx = x - px;
                        let dy = y - py;
                        let d_sq = dx * dx + dy * dy;
                        if d_sq < best_dist_sq {
                            best_dist_sq = d_sq;
                            best = Some((px, py));
                        }
                    }
                }
            }
        }

        best.unwrap_or((x, y))
    }
}

/// Build the canonical intersection cache by traversing the BSP tree,
/// computing all pairwise divline intersections per leaf, and collecting
/// all segment endpoints.
pub fn build_intersection_cache(
    root_node: u32,
    nodes: &[Node],
    corrected_divlines: &[DivLine],
    subsectors: &[SubSector],
    segments: &[Segment],
) -> IntersectionCache {
    let start = Instant::now();

    let mut raw_points: Vec<(f64, f64)> = Vec::new();

    // Collect all segment endpoints as canonical points.
    for seg in segments {
        raw_points.push((seg.v1.x as f64, seg.v1.y as f64));
        raw_points.push((seg.v2.x as f64, seg.v2.y as f64));
    }

    // Traverse BSP tree, collecting divline intersections at each leaf.
    collect_divline_intersections(
        root_node,
        nodes,
        corrected_divlines,
        subsectors,
        segments,
        &mut Vec::new(),
        &mut raw_points,
    );

    info!(
        "Intersection cache: {} raw points from {} segments + BSP traversal ({:#?})",
        raw_points.len(),
        segments.len(),
        start.elapsed()
    );

    // Deduplicate points within CANONICAL_DEDUP_DIST using spatial grid.
    let dedup_sq = CANONICAL_DEDUP_DIST * CANONICAL_DEDUP_DIST;
    let dedup_cell = CANONICAL_DEDUP_DIST * 2.0;
    let mut dedup_grid: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
    let mut points: Vec<(f64, f64)> = Vec::with_capacity(raw_points.len() / 2);

    for &(x, y) in &raw_points {
        let cx = (x / dedup_cell).floor() as i32;
        let cy = (y / dedup_cell).floor() as i32;

        let mut is_dup = false;
        'search: for gx in (cx - 1)..=(cx + 1) {
            for gy in (cy - 1)..=(cy + 1) {
                if let Some(indices) = dedup_grid.get(&(gx, gy)) {
                    for &idx in indices {
                        let (px, py) = points[idx];
                        let dx = x - px;
                        let dy = y - py;
                        if dx * dx + dy * dy < dedup_sq {
                            is_dup = true;
                            break 'search;
                        }
                    }
                }
            }
        }

        if !is_dup {
            let idx = points.len();
            dedup_grid.entry((cx, cy)).or_default().push(idx);
            points.push((x, y));
        }
    }

    // Build spatial grid for snap lookups.
    let mut grid: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
    for (idx, &(x, y)) in points.iter().enumerate() {
        let cx = (x / CANONICAL_CELL).floor() as i32;
        let cy = (y / CANONICAL_CELL).floor() as i32;
        grid.entry((cx, cy)).or_default().push(idx);
    }

    info!(
        "Intersection cache: {} canonical points, {} grid cells ({:#?})",
        points.len(),
        grid.len(),
        start.elapsed()
    );

    IntersectionCache {
        points,
        grid,
    }
}

/// Recursive BSP traversal collecting pairwise divline intersections at leaves.
fn collect_divline_intersections(
    node_id: u32,
    nodes: &[Node],
    corrected_divlines: &[DivLine],
    subsectors: &[SubSector],
    segments: &[Segment],
    divline_stack: &mut Vec<DivLine>,
    out: &mut Vec<(f64, f64)>,
) {
    if node_id & 0x8000_0000 != 0 {
        if node_id == u32::MAX {
            return;
        }
        let ss_id = (node_id & !0x8000_0000) as usize;
        if ss_id >= subsectors.len() {
            return;
        }

        // Add segment-as-divline × BSP-divline intersections.
        let ss = &subsectors[ss_id];
        let start = ss.start_seg as usize;
        let end = start + ss.seg_count as usize;
        let mut seg_divlines: Vec<DivLine> = Vec::new();
        if let Some(ss_segs) = segments.get(start..end) {
            for seg in ss_segs {
                let dx = seg.v2.x as f64 - seg.v1.x as f64;
                let dy = seg.v2.y as f64 - seg.v1.y as f64;
                if dx.abs() + dy.abs() > 1e-6 {
                    seg_divlines.push(DivLine {
                        x: seg.v1.x as f64,
                        y: seg.v1.y as f64,
                        dx,
                        dy,
                    });
                }
            }
        }

        // Compute all pairwise intersections of BSP divlines in the stack.
        // Filter to within world bounds — near-parallel intersections can
        // produce extreme coordinates.
        let n = divline_stack.len();
        for i in 0..n {
            for j in (i + 1)..n {
                if let Some((px, py)) = divline_stack[i].intersect_line(&divline_stack[j]) {
                    if px.abs() <= WORLD_SIZE && py.abs() <= WORLD_SIZE {
                        out.push((px, py));
                    }
                }
            }
            for sdl in &seg_divlines {
                if let Some((px, py)) = divline_stack[i].intersect_line(sdl) {
                    if px.abs() <= WORLD_SIZE && py.abs() <= WORLD_SIZE {
                        out.push((px, py));
                    }
                }
            }
        }
        return;
    }

    let nid = node_id as usize;
    if nid >= nodes.len() || nid >= corrected_divlines.len() {
        return;
    }

    let node = &nodes[nid];
    let dl = corrected_divlines[nid];

    // Right child: forward divline.
    divline_stack.push(dl);
    collect_divline_intersections(
        node.children[0],
        nodes,
        corrected_divlines,
        subsectors,
        segments,
        divline_stack,
        out,
    );
    divline_stack.pop();

    // Left child: reversed divline.
    divline_stack.push(DivLine {
        dx: -dl.dx,
        dy: -dl.dy,
        ..dl
    });
    collect_divline_intersections(
        node.children[1],
        nodes,
        corrected_divlines,
        subsectors,
        segments,
        divline_stack,
        out,
    );
    divline_stack.pop();
}

/// Snap segment vertices to canonical divline intersection points in-place.
/// Since segments store `MapPtr<Vec2>` into `vertexes`, modifying `vertexes[i]`
/// is immediately visible through all segment and linedef references.
///
/// **Case A** — linedef endpoint: snap to nearest canonical intersection point.
/// **Case B** — BSP-split vertex: project onto parent linedef's v1→v2 line in
/// f64.
pub fn snap_vertices_to_canonical(
    vertexes: &mut [Vec2],
    linedefs: &[LineDef],
    cache: &IntersectionCache,
    wad: &WadData,
    map_name: &str,
    extended: Option<&WadExtendedMap>,
) {
    let start = Instant::now();
    let mut hit = vec![false; vertexes.len()];
    let mut snapped_endpoints = 0u32;
    let mut projected_splits = 0u32;

    let mut process_seg = |seg: WadSegment| {
        let v_indices = [seg.start_vertex as usize, seg.end_vertex as usize];
        let ld = &linedefs[seg.linedef as usize];
        let ld_v1_idx = ld.v1.as_ptr() as usize;
        let ld_v2_idx = ld.v2.as_ptr() as usize;
        let base = vertexes.as_ptr() as usize;
        let stride = std::mem::size_of::<Vec2>();
        let ld_v1i = (ld_v1_idx - base) / stride;
        let ld_v2i = (ld_v2_idx - base) / stride;

        for &v_idx in &v_indices {
            if v_idx >= vertexes.len() || hit[v_idx] {
                continue;
            }
            hit[v_idx] = true;

            if v_idx == ld_v1i || v_idx == ld_v2i {
                // Case A: linedef endpoint — snap to canonical
                let vx = vertexes[v_idx].x as f64;
                let vy = vertexes[v_idx].y as f64;
                let (sx, sy) = cache.snap_vertex(vx, vy, CANONICAL_SNAP_DIST);
                if sx != vx || sy != vy {
                    vertexes[v_idx].x = sx as f32;
                    vertexes[v_idx].y = sy as f32;
                    snapped_endpoints += 1;
                }
            } else {
                // Case B: BSP-split vertex — project onto linedef line in f64
                let ldx = *ld.v1;
                let ldy = *ld.v2;
                let ld_dx = (ldy.x - ldx.x) as f64;
                let ld_dy = (ldy.y - ldx.y) as f64;
                let len_sq = ld_dx * ld_dx + ld_dy * ld_dy;
                if len_sq > 1e-20 {
                    let vx = vertexes[v_idx].x as f64 - ldx.x as f64;
                    let vy = vertexes[v_idx].y as f64 - ldx.y as f64;
                    let t = (vx * ld_dx + vy * ld_dy) / len_sq;
                    vertexes[v_idx].x = (ldx.x as f64 + t * ld_dx) as f32;
                    vertexes[v_idx].y = (ldx.y as f64 + t * ld_dy) as f32;
                    projected_splits += 1;
                }
            }
        }
    };

    if let Some(ext) = extended {
        ext.segments.iter().for_each(|s| process_seg(s.clone()));
    } else {
        wad.segment_iter(map_name).for_each(process_seg);
    }

    info!(
        "snap_vertices_to_canonical: {} endpoints snapped, {} splits projected ({:#?})",
        snapped_endpoints,
        projected_splits,
        start.elapsed()
    );
}

/// Test if a point lies inside or on the boundary of a convex polygon.
///
/// Uses signed-area (cross-product) winding. A point on an edge (within
/// [`POINT_ON_EDGE_EPSILON`]) counts as inside.
fn point_in_or_on_polygon(p: (f64, f64), poly: &[(f64, f64)]) -> bool {
    let n = poly.len();
    if n < 3 {
        return false;
    }
    // Check if point is within POINT_ON_EDGE_EPSILON of any edge first.
    for i in 0..n {
        let a = poly[i];
        let b = poly[(i + 1) % n];
        let ex = b.0 - a.0;
        let ey = b.1 - a.1;
        let len_sq = ex * ex + ey * ey;
        if len_sq < 1e-12 {
            continue;
        }
        // Project p onto edge a→b.
        let t = ((p.0 - a.0) * ex + (p.1 - a.1) * ey) / len_sq;
        let t_clamped = t.clamp(0.0, 1.0);
        let cx = a.0 + t_clamped * ex;
        let cy = a.1 + t_clamped * ey;
        let dist_sq = (p.0 - cx) * (p.0 - cx) + (p.1 - cy) * (p.1 - cy);
        if dist_sq <= POINT_ON_EDGE_EPSILON * POINT_ON_EDGE_EPSILON {
            return true;
        }
    }
    // Winding number test — consistent sign of cross products.
    let mut positive = false;
    let mut negative = false;
    for i in 0..n {
        let a = poly[i];
        let b = poly[(i + 1) % n];
        let cross = (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0);
        if cross > 0.0 {
            positive = true;
        } else if cross < 0.0 {
            negative = true;
        }
        if positive && negative {
            return false;
        }
    }
    true
}

/// Maximum distance (map units) a segment endpoint can be from the polygon
/// boundary and still be inserted. Prevents runaway expansion from bad data.
const MAX_EXPAND_DIST: f64 = 4.0;

/// Expand a carved polygon to include any segment endpoints that lie outside
/// it.
///
/// For each missing endpoint, finds the nearest polygon edge and inserts the
/// point between that edge's vertices. This corrects cases where divline-only
/// SH clipping produces a polygon that doesn't cover the full subsector.
fn expand_polygon_to_segment_endpoints(
    poly: &mut Vec<(f64, f64)>,
    segments: &[Segment],
    subsector_id: usize,
) {
    for segment in segments {
        for &p in &[
            (segment.v1.x as f64, segment.v1.y as f64),
            (segment.v2.x as f64, segment.v2.y as f64),
        ] {
            if point_in_or_on_polygon(p, poly) {
                continue;
            }

            // Find nearest polygon edge to insert at.
            let n = poly.len();
            let mut best_edge = 0;
            let mut best_dist_sq = f64::MAX;
            for i in 0..n {
                let a = poly[i];
                let b = poly[(i + 1) % n];
                let dist_sq = point_to_segment_dist_sq(p, a, b);
                if dist_sq < best_dist_sq {
                    best_dist_sq = dist_sq;
                    best_edge = i;
                }
            }

            let dist = best_dist_sq.sqrt();
            if dist > MAX_EXPAND_DIST {
                warn!(
                    "SS{} carve gap: ({:.2}, {:.2}) too far ({:.2} units), skipped",
                    subsector_id, p.0, p.1, dist
                );
                continue;
            }

            // Insert after the first vertex of the nearest edge. Fan
            // triangulation will produce a small triangle between the
            // inserted point and the old divline intersection — this fills
            // the gap. Minor overlap with neighbouring subsectors is
            // acceptable as rendering handles coplanar same-height geometry.
            poly.insert(best_edge + 1, p);
            debug!(
                "SS{} carve fix: inserted ({:.2}, {:.2}) at edge {} ({:.2} units)",
                subsector_id, p.0, p.1, best_edge, dist
            );
        }
    }
}

/// Squared distance from point `p` to the finite line segment `a`→`b`.
fn point_to_segment_dist_sq(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let ex = b.0 - a.0;
    let ey = b.1 - a.1;
    let len_sq = ex * ex + ey * ey;
    if len_sq < 1e-12 {
        let dx = p.0 - a.0;
        let dy = p.1 - a.1;
        return dx * dx + dy * dy;
    }
    let t = ((p.0 - a.0) * ex + (p.1 - a.1) * ey) / len_sq;
    let t = t.clamp(0.0, 1.0);
    let cx = a.0 + t * ex;
    let cy = a.1 + t * ey;
    (p.0 - cx) * (p.0 - cx) + (p.1 - cy) * (p.1 - cy)
}

/// Generate a convex polygon for a subsector by clipping against BSP divlines
/// and subsector segment boundaries.
///
/// Returns f64 polygon vertices. Caller converts to f32 at vertex storage.
pub fn carve_subsector_polygon(
    segments: &[Segment],
    divlines: &[DivLine],
    subsector_id: usize,
) -> Vec<(f64, f64)> {
    if divlines.is_empty() {
        return extract_sector_boundary_from_segments(segments);
    }

    let initial: [(f64, f64); 4] = [
        (-WORLD_SIZE, WORLD_SIZE),
        (WORLD_SIZE, WORLD_SIZE),
        (WORLD_SIZE, -WORLD_SIZE),
        (-WORLD_SIZE, -WORLD_SIZE),
    ];

    // BSP divlines (reversed order matches root-to-leaf accumulation).
    let mut clippers: Vec<DivLine> = divlines.iter().rev().copied().collect();

    // Subsector boundary segments tighten the polygon to actual wall edges.
    // Segment vertices are already at canonical positions after the
    // snap_vertices_to_canonical pass.
    for segment in segments {
        let dx = segment.v2.x as f64 - segment.v1.x as f64;
        let dy = segment.v2.y as f64 - segment.v1.y as f64;
        if dx.abs() + dy.abs() < 1e-6 {
            continue;
        }
        clippers.push(DivLine {
            x: segment.v1.x as f64,
            y: segment.v1.y as f64,
            dx,
            dy,
        });
    }

    let mut clipped = clip_polygon(&initial, &clippers);

    cleanup_polygon(&mut clipped);

    // Snap clipped polygon vertices to nearby segment endpoints.
    // SH clipping can produce vertices slightly offset from the true
    // endpoint even with f64 arithmetic. Segment endpoints are already
    // at canonical positions from the snap_vertices_to_canonical pass.
    // snap_to_segment_endpoints(&mut clipped, segments);

    // Expand polygon to include segment endpoints that fell outside due to
    // divline clipping not matching the actual subsector boundary.
    if clipped.len() >= 3 {
        expand_polygon_to_segment_endpoints(&mut clipped, segments, subsector_id);
    }

    clipped
}

/// Fallback for subsectors with zero BSP divlines: extract boundary from
/// segment endpoints sorted by angle.
fn extract_sector_boundary_from_segments(segments: &[Segment]) -> Vec<(f64, f64)> {
    if segments.is_empty() {
        return Vec::new();
    }

    let mut all_vertices: Vec<(f64, f64)> = Vec::new();
    const EPSILON: f64 = 0.001;

    for segment in segments {
        let v1 = (segment.v1.x as f64, segment.v1.y as f64);
        let v2 = (segment.v2.x as f64, segment.v2.y as f64);

        if !all_vertices
            .iter()
            .any(|v| (v.0 - v1.0).abs() + (v.1 - v1.1).abs() < EPSILON)
        {
            all_vertices.push(v1);
        }
        if !all_vertices
            .iter()
            .any(|v| (v.0 - v2.0).abs() + (v.1 - v2.1).abs() < EPSILON)
        {
            all_vertices.push(v2);
        }
    }

    if all_vertices.len() < 3 {
        return Vec::new();
    }

    let n = all_vertices.len() as f64;
    let cx = all_vertices.iter().map(|v| v.0).sum::<f64>() / n;
    let cy = all_vertices.iter().map(|v| v.1).sum::<f64>() / n;

    all_vertices.sort_by(|a, b| {
        let angle_a = (a.1 - cy).atan2(a.0 - cx);
        let angle_b = (b.1 - cy).atan2(b.0 - cx);
        angle_a
            .partial_cmp(&angle_b)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    all_vertices
}

/// Traverse the BSP tree and return the carved 2D convex polygon for each
/// subsector. Produces f64 polygons indexed by subsector ID.
pub fn carve_subsector_polygons_2d(
    root_node: u32,
    nodes: &[Node],
    corrected_divlines: &[DivLine],
    subsectors: &[SubSector],
    segments: &[Segment],
) -> Vec<Vec<(f64, f64)>> {
    let mut result = vec![Vec::new(); subsectors.len()];
    carve_2d_recursive(
        root_node,
        nodes,
        corrected_divlines,
        subsectors,
        segments,
        Vec::new(),
        &mut result,
    );
    result
}

/// Recursive BSP traversal for 2D polygon carving.
fn carve_2d_recursive(
    node_id: u32,
    nodes: &[Node],
    corrected_divlines: &[DivLine],
    subsectors: &[SubSector],
    segments: &[Segment],
    divlines: Vec<DivLine>,
    result: &mut [Vec<(f64, f64)>],
) {
    if is_subsector(node_id) {
        if node_id == u32::MAX {
            return;
        }
        let subsector_id = subsector_index(node_id);
        if subsector_id < subsectors.len() {
            let ss = &subsectors[subsector_id];
            let start = ss.start_seg as usize;
            let end = start + ss.seg_count as usize;
            if let Some(ss_segments) = segments.get(start..end) {
                result[subsector_id] =
                    carve_subsector_polygon(ss_segments, &divlines, subsector_id);
            }
        }
    } else if let Some(node) = nodes.get(node_id as usize) {
        let nid = node_id as usize;
        let node_divline = if nid < corrected_divlines.len() {
            corrected_divlines[nid]
        } else {
            DivLine::from_node(node)
        };

        let mut right_divlines = divlines.clone();
        right_divlines.push(node_divline);
        carve_2d_recursive(
            node.children[0],
            nodes,
            corrected_divlines,
            subsectors,
            segments,
            right_divlines,
            result,
        );

        let mut left_divlines = divlines;
        left_divlines.push(DivLine {
            dx: -node_divline.dx,
            dy: -node_divline.dy,
            ..node_divline
        });
        carve_2d_recursive(
            node.children[1],
            nodes,
            corrected_divlines,
            subsectors,
            segments,
            left_divlines,
            result,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{DivLine, clip_polygon};

    #[test]
    fn test_polygon_clipping() {
        let square: [(f64, f64); 4] = [(-5.0, -5.0), (5.0, -5.0), (5.0, 5.0), (-5.0, 5.0)];

        let clipper = DivLine {
            x: 0.0,
            y: -10.0,
            dx: 0.0,
            dy: 20.0,
        };

        let clipped = clip_polygon(&square, &[clipper]);

        assert!(
            clipped.len() >= 3,
            "Should have at least 3 vertices, got {}",
            clipped.len()
        );

        for (i, vertex) in clipped.iter().enumerate() {
            assert!(
                vertex.0 >= -1e-5,
                "Vertex {}: x={} should be >= 0",
                i,
                vertex.0
            );
        }
    }
}
