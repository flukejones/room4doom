//! Pure 2D geometry for the editor: point/segment tests, line intersection/splitting, ring orientation, loop detection, snap-candidate selection; Slint-free and app-state-free, unit-testable against a real [`EditorMap`], points are world-space `[f32; 2]`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::mem;

use crate::model::{EditorMap, LineDef, LineKey, SectorKey, VertKey, Vertex};

/// Parametric epsilon: a crossing counts as interior only when both segment parameters land strictly inside `(EPS, 1 - EPS)`, excluding endpoint touches.
const INTERSECT_EPS: f32 = 1e-4;
/// Below `moved × vertices` of this, a direct weld scan beats building the cell grid.
const WELD_GRID_THRESHOLD: usize = 4096;

/// A line's key, endpoint positions, and unpadded bbox, cached for the split pair scan.
type CachedSegment = (LineKey, [f32; 2], [f32; 2], ([f32; 2], [f32; 2]));

/// Inputs to [`choose_snap`]: grid settings, per-kind tolerances, and candidate geometry pre-filtered by the caller to the snap radius; an empty candidate slice disables that snap kind.
pub struct SnapOptions<'a> {
    /// Grid spacing in world units.
    pub grid: f32,
    /// Whether grid snapping applies (the fallback and the on-line bias).
    pub grid_on: bool,
    /// Max distance for a vertex candidate to win.
    pub vertex_tol: f32,
    /// Max distance for an on-line candidate to win.
    pub line_tol: f32,
    /// Vertex candidates within the snap radius.
    pub nearby_vertices: &'a [[f32; 2]],
    /// Line-segment candidates within the snap radius.
    pub nearby_lines: &'a [([f32; 2], [f32; 2])],
    /// Ray origin for angle snapping (e.g. the previous chain point); `None` disables.
    pub angle_from: Option<[f32; 2]>,
    /// Angle quantisation step (radians); `<= 0` disables.
    pub angle_step_rad: f32,
}

/// True when `world` lies on the front (right) side of `p1`->`p2` in world (Y-up) coordinates; the single facing rule shared by sector probing, the hover preview, and the scanline fill.
pub(crate) fn is_front_side(world: [f32; 2], p1: [f32; 2], p2: [f32; 2]) -> bool {
    (p2[0] - p1[0]) * (world[1] - p1[1]) - (p2[1] - p1[1]) * (world[0] - p1[0]) < 0.0
}

/// Distance from `p` to segment (`a`,`b`) (not squared).
pub fn distance_to_segment(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let n = nearest_point_on_segment(p, a, b);
    let d = [p[0] - n[0], p[1] - n[1]];
    (d[0] * d[0] + d[1] * d[1]).sqrt()
}

/// The point on segment (`a`,`b`) nearest `p` (the clamped projection).
pub fn nearest_point_on_segment(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let ap = [p[0] - a[0], p[1] - a[1]];
    let len_sq = ab[0] * ab[0] + ab[1] * ab[1];
    let t = if len_sq > 0.0 {
        ((ap[0] * ab[0] + ap[1] * ab[1]) / len_sq).clamp(0.0, 1.0)
    } else {
        0.0
    };
    [a[0] + ab[0] * t, a[1] + ab[1] * t]
}

/// The proper interior crossing of segments (`a1`,`a2`) and (`b1`,`b2`); returns the point and both parameters only when each is strictly inside its segment, `None` for parallel, collinear, or endpoint-only touches (handled by [`point_on_segment_interior`]).
pub(crate) fn segment_intersection(
    a1: [f32; 2],
    a2: [f32; 2],
    b1: [f32; 2],
    b2: [f32; 2],
) -> Option<([f32; 2], f32, f32)> {
    let r = [a2[0] - a1[0], a2[1] - a1[1]];
    let s = [b2[0] - b1[0], b2[1] - b1[1]];
    let denom = r[0] * s[1] - r[1] * s[0];
    if denom.abs() < INTERSECT_EPS {
        return None;
    }
    let qp = [b1[0] - a1[0], b1[1] - a1[1]];
    let t = (qp[0] * s[1] - qp[1] * s[0]) / denom;
    let u = (qp[0] * r[1] - qp[1] * r[0]) / denom;
    if t <= INTERSECT_EPS
        || t >= 1.0 - INTERSECT_EPS
        || u <= INTERSECT_EPS
        || u >= 1.0 - INTERSECT_EPS
    {
        return None;
    }
    Some(([a1[0] + r[0] * t, a1[1] + r[1] * t], t, u))
}

/// The parameter of `p` along segment (`a`,`b`) when `p` lies on it within `tol`, strictly between the endpoints; `None` at the endpoints or off-segment.
pub(crate) fn point_on_segment_interior(
    p: [f32; 2],
    a: [f32; 2],
    b: [f32; 2],
    tol: f32,
) -> Option<f32> {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let len_sq = ab[0] * ab[0] + ab[1] * ab[1];
    if len_sq <= 0.0 {
        return None;
    }
    let ap = [p[0] - a[0], p[1] - a[1]];
    let t = (ap[0] * ab[0] + ap[1] * ab[1]) / len_sq;
    if t <= INTERSECT_EPS || t >= 1.0 - INTERSECT_EPS {
        return None;
    }
    let closest = [a[0] + ab[0] * t, a[1] + ab[1] * t];
    let d = [p[0] - closest[0], p[1] - closest[1]];
    ((d[0] * d[0] + d[1] * d[1]).sqrt() <= tol).then_some(t)
}

/// Signed area of the polygon formed by a ring of vertex keys (shoelace); positive is counter-clockwise in world (Y-up) coordinates.
pub(crate) fn ring_signed_area(map: &EditorMap, ring: &[VertKey]) -> f32 {
    let mut sum = 0.0;
    for i in 0..ring.len() {
        let a = map.vertices[ring[i]];
        let b = map.vertices[ring[(i + 1) % ring.len()]];
        sum += a.x * b.y - b.x * a.y;
    }
    sum * 0.5
}

/// The sector at world point `p`, by the same horizontal-scanline rule the sector fill uses: along `y = p.y`, the sector is the one entered at the last line crossing left of `p.x` (correct for concave sectors); `None` for the void.
pub fn sector_at(map: &EditorMap, p: [f32; 2]) -> Option<SectorKey> {
    let [px, py] = p;
    // The entered sector at the nearest crossing at or left of the point.
    let mut best: Option<(f32, Option<SectorKey>)> = None;
    for line in map.lines.values() {
        let v1 = map.vertices[line.v1];
        let v2 = map.vertices[line.v2];
        // Half-open y span so a vertex shared by two edges counts once.
        if (v1.y <= py) == (v2.y <= py) {
            continue;
        }
        let cx = v1.x + (v2.x - v1.x) * (py - v1.y) / (v2.y - v1.y);
        if cx > px {
            continue;
        }
        // Sector entered crossing this edge rightward (the fill's convention).
        let enter = if v2.y > v1.y {
            line.front.sector
        } else {
            line.back.and_then(|b| b.sector)
        };
        if best.is_none_or(|(x, _)| cx > x) {
            best = Some((cx, enter));
        }
    }
    let (_, s) = best?;
    s
}

/// The floor height a thing at world-XY `p` sits at: its containing sector's floor, or 0 in the void; a thing is fully 3D, this is the authoritative rule for its stored `z`.
pub fn thing_floor_z(map: &EditorMap, p: [f32; 2]) -> i32 {
    sector_at(map, p).map_or(0, |s| map.sectors[s].floor_height)
}

/// Re-derive every thing's stored `z` from its containing sector floor; called after load/import (WAD carries no thing Z) and after edits that move things or change floor heights.
pub fn derive_thing_heights(map: &mut EditorMap) {
    let targets: Vec<(crate::model::ThingKey, [f32; 2])> = map
        .things
        .iter()
        .map(|(k, t)| (k, [t.x as f32, t.y as f32]))
        .collect();
    for (k, p) in targets {
        let z = thing_floor_z(map, p);
        if let Some(t) = map.things.get_mut(k) {
            t.z = z;
        }
    }
}

/// Split `line` at `point` into two lines sharing a new vertex: the original becomes `v1`->`vnew`, a new line `vnew`->`v2` is inserted, both keep the original sides/flags; the `v1`->`v2` direction is preserved on both halves so front/back stay on the correct geometric sides.
pub fn split_line_at(map: &mut EditorMap, line: LineKey, point: [f32; 2]) -> (VertKey, LineKey) {
    let vnew = map.find_or_add_vertex(point);
    let original = map.lines[line];
    let tail = LineDef {
        v1: vnew,
        v2: original.v2,
        flags: original.flags,
        special: original.special,
        tag: original.tag,
        front: original.front,
        back: original.back,
    };
    map.lines[line].v2 = vnew;
    let tail_key = map.lines.insert(tail);
    (vnew, tail_key)
}

/// Split the given `lines` wherever they cross or touch any line in the map; returns the keys of every split line (the in-place "head" halves, the inserted "tail" halves are new keys). A proper crossing splits both lines at the point; an endpoint landing on another's interior splits that other line at the endpoint. All split points are collected first, then applied per line, so insertions never invalidate an in-flight key.
pub fn split_lines_at_intersections(
    map: &mut EditorMap,
    lines: &[LineKey],
    tol: f32,
) -> Vec<LineKey> {
    // Positions are stable while candidates accumulate; cache every line's points + bbox once.
    let all: Vec<CachedSegment> = map
        .lines
        .keys()
        .map(|k| {
            let (p1, p2) = segment_points(map, k);
            (k, p1, p2, segment_bbox(p1, p2, 0.0))
        })
        .collect();
    let mut points: Vec<(LineKey, [f32; 2])> = Vec::new();
    for &a in lines {
        let (a1, a2) = segment_points(map, a);
        let a_bbox = segment_bbox(a1, a2, tol);
        for &(b, b1, b2, b_bbox) in &all {
            if a == b {
                continue;
            }
            // Tol-inflated bbox reject before the exact segment tests.
            if !bbox_overlaps(a_bbox, b_bbox) {
                continue;
            }
            if let Some((p, _, _)) = segment_intersection(a1, a2, b1, b2) {
                points.push((a, p));
                points.push((b, p));
            }
            // Either line's endpoint touching the other's interior is a T-junction; split the touched line at that point.
            for end in [a1, a2] {
                if point_on_segment_interior(end, b1, b2, tol).is_some() {
                    points.push((b, end));
                }
            }
            for end in [b1, b2] {
                if point_on_segment_interior(end, a1, a2, tol).is_some() {
                    points.push((a, end));
                }
            }
        }
    }
    apply_splits(map, points)
}

/// Tol-inflated axis-aligned bounds of segment (`a`,`b`), padded by `pad` on every side.
fn segment_bbox(a: [f32; 2], b: [f32; 2], pad: f32) -> ([f32; 2], [f32; 2]) {
    (
        [a[0].min(b[0]) - pad, a[1].min(b[1]) - pad],
        [a[0].max(b[0]) + pad, a[1].max(b[1]) + pad],
    )
}

/// Whether two axis-aligned `(min, max)` boxes overlap.
fn bbox_overlaps(a: ([f32; 2], [f32; 2]), b: ([f32; 2], [f32; 2])) -> bool {
    a.0[0] <= b.1[0] && b.0[0] <= a.1[0] && a.0[1] <= b.1[1] && b.0[1] <= a.1[1]
}

/// Weld each moved vertex onto the nearest stationary vertex within `tol` (run after a drag): a vertex within tolerance of an existing one remaps onto it (lines re-point), and lines collapsing to one vertex are deleted. Only `moved` keys weld, and only onto vertices outside that set, so a stationary target never shifts and two moved vertices never chain. Returns whether anything changed.
pub(crate) fn weld_moved_vertices(map: &mut EditorMap, moved: &[VertKey], tol: f32) -> bool {
    let tol_sq = tol * tol;
    let moved_set: HashSet<VertKey> = moved.iter().copied().collect();
    let nearest = |p: Vertex, best: &mut Option<(f32, VertKey)>, j: VertKey, v: Vertex| {
        let d = (v.x - p.x) * (v.x - p.x) + (v.y - p.y) * (v.y - p.y);
        if d <= tol_sq && best.is_none_or(|(bd, _)| d < bd) {
            *best = Some((d, j));
        }
    };
    let mut remap: HashMap<VertKey, VertKey> = HashMap::new();
    if moved.len() * map.vertices.len() <= WELD_GRID_THRESHOLD {
        // Small workload (the common single-vertex drag): direct scan beats building the grid.
        for &i in moved {
            let Some(&p) = map.vertices.get(i) else {
                continue;
            };
            let mut best: Option<(f32, VertKey)> = None;
            for (j, &v) in map.vertices.iter() {
                if !moved_set.contains(&j) {
                    nearest(p, &mut best, j, v);
                }
            }
            if let Some((_, target)) = best {
                remap.insert(i, target);
            }
        }
    } else {
        // Stationary vertices bucketed into tol-sized cells; a target within tol always lies in the moved vertex's 3x3 cell neighbourhood.
        let cell = |x: f32, y: f32| [(x / tol).floor() as i64, (y / tol).floor() as i64];
        let mut buckets: HashMap<[i64; 2], Vec<VertKey>> = HashMap::new();
        for (j, v) in map.vertices.iter() {
            if !moved_set.contains(&j) {
                buckets.entry(cell(v.x, v.y)).or_default().push(j);
            }
        }
        for &i in moved {
            let Some(&p) = map.vertices.get(i) else {
                continue;
            };
            let c = cell(p.x, p.y);
            let mut best: Option<(f32, VertKey)> = None;
            for dx in -1..=1 {
                for dy in -1..=1 {
                    let neighbour = [c[0].saturating_add(dx), c[1].saturating_add(dy)];
                    for &j in buckets.get(&neighbour).map(Vec::as_slice).unwrap_or(&[]) {
                        nearest(p, &mut best, j, map.vertices[j]);
                    }
                }
            }
            if let Some((_, target)) = best {
                remap.insert(i, target);
            }
        }
    }
    if remap.is_empty() {
        return false;
    }
    for l in map.lines.values_mut() {
        if let Some(&t) = remap.get(&l.v1) {
            l.v1 = t;
        }
        if let Some(&t) = remap.get(&l.v2) {
            l.v2 = t;
        }
    }
    let collapsed: Vec<LineKey> = map
        .lines
        .iter()
        .filter(|(_, l)| l.v1 == l.v2)
        .map(|(k, _)| k)
        .collect();
    map.remove_lines(&collapsed);
    map.prune_orphan_vertices();
    true
}

/// Weld `ids` together at `target`: move each to the point, unify the now coincident vertices, then drop any line that collapsed to a point or duplicates another. Returns whether anything welded (fewer than two ids is a no-op).
pub(crate) fn weld_vertices(map: &mut EditorMap, ids: &[VertKey], target: [f32; 2]) -> bool {
    if ids.len() < 2 {
        return false;
    }
    for &i in ids {
        if let Some(v) = map.vertices.get_mut(i) {
            v.x = target[0];
            v.y = target[1];
        }
    }
    merge_coincident_vertices(map);
    let degenerate: Vec<LineKey> = map
        .lines
        .iter()
        .filter(|(_, l)| l.v1 == l.v2)
        .map(|(k, _)| k)
        .collect();
    map.remove_lines(&degenerate);
    dedup_coincident_lines(map);
    true
}

/// The key of the vertex at exactly `p` (bit-equal), if any.
pub fn vertex_at(map: &EditorMap, p: [f32; 2]) -> Option<VertKey> {
    map.vertices
        .iter()
        .find(|(_, v)| v.x.to_bits() == p[0].to_bits() && v.y.to_bits() == p[1].to_bits())
        .map(|(k, _)| k)
}

/// Delete a vertex, resolving the lines attached to it: two-sided lines at the vertex are deleted; single-sided lines are deleted too, except exactly two of them dissolve the vertex into one line spanning their far endpoints (keeping the first line's sides/flags). The vertex then prunes; any duplicate or degenerate line the merge produced is collapsed.
pub fn delete_vertex(map: &mut EditorMap, vertex: VertKey) {
    let far = |l: &LineDef| if l.v1 == vertex { l.v2 } else { l.v1 };
    let incident: Vec<LineKey> = map
        .lines
        .iter()
        .filter(|(_, l)| l.v1 == vertex || l.v2 == vertex)
        .map(|(k, _)| k)
        .collect();
    let singles: Vec<LineKey> = incident
        .iter()
        .copied()
        .filter(|&k| map.lines[k].back.is_none())
        .collect();

    let mut remove = incident.clone();
    if singles.len() == 2 {
        let (keep, drop) = (singles[0], singles[1]);
        let b = far(&map.lines[drop]);
        let line = &mut map.lines[keep];
        if line.v1 == vertex {
            line.v1 = b;
        } else {
            line.v2 = b;
        }
        remove.retain(|&k| k != keep);
    }
    map.remove_lines(&remove);
    merge_coincident_vertices(map);
    let degenerate: Vec<LineKey> = map
        .lines
        .iter()
        .filter(|(_, l)| l.v1 == l.v2)
        .map(|(k, _)| k)
        .collect();
    map.remove_lines(&degenerate);
    dedup_coincident_lines(map);
}

/// Unify vertices that share an exact position: every line endpoint is rewritten to the first-slot vertex at its position, then orphaned vertices prune away. Run where any vertex may coincide (weld, vertex delete); the move path uses [`merge_coincident_moved`].
pub(crate) fn merge_coincident_vertices(map: &mut EditorMap) {
    let mut canonical: BTreeMap<(u32, u32), VertKey> = BTreeMap::new();
    let mut remap: HashMap<VertKey, VertKey> = HashMap::new();
    for (k, v) in map.vertices.iter() {
        let key = (v.x.to_bits(), v.y.to_bits());
        let lead = *canonical.entry(key).or_insert(k);
        if lead != k {
            remap.insert(k, lead);
        }
    }
    apply_vertex_remap(map, &remap);
}

/// Move-path variant of [`merge_coincident_vertices`]: only positions occupied by a `moved` vertex can have newly become coincident, so canonicalisation is restricted to those positions.
pub(crate) fn merge_coincident_moved(map: &mut EditorMap, moved: &[VertKey]) {
    let moved_pos: HashSet<(u32, u32)> = moved
        .iter()
        .filter_map(|&k| map.vertices.get(k))
        .map(|v| (v.x.to_bits(), v.y.to_bits()))
        .collect();
    let mut canonical: BTreeMap<(u32, u32), VertKey> = BTreeMap::new();
    let mut remap: HashMap<VertKey, VertKey> = HashMap::new();
    for (k, v) in map.vertices.iter() {
        let key = (v.x.to_bits(), v.y.to_bits());
        if !moved_pos.contains(&key) {
            continue;
        }
        let lead = *canonical.entry(key).or_insert(k);
        if lead != k {
            remap.insert(k, lead);
        }
    }
    apply_vertex_remap(map, &remap);
}

/// Rewrite every line endpoint through `remap`, then prune orphaned vertices; a no-op for an empty remap.
fn apply_vertex_remap(map: &mut EditorMap, remap: &HashMap<VertKey, VertKey>) {
    if remap.is_empty() {
        return;
    }
    for l in map.lines.values_mut() {
        if let Some(&t) = remap.get(&l.v1) {
            l.v1 = t;
        }
        if let Some(&t) = remap.get(&l.v2) {
            l.v2 = t;
        }
    }
    map.prune_orphan_vertices();
}

/// Collapse lines that share both endpoints (in either direction) to a single line, so dragging a wall onto a collinear one does not leave duplicates. The first-slot occurrence survives; if a duplicate carried a back side, the survivor adopts it (two one-sided walls meeting become two-sided). Returns the keys removed.
pub(crate) fn dedup_coincident_lines(map: &mut EditorMap) -> Vec<LineKey> {
    let mut seen: BTreeMap<(VertKey, VertKey), LineKey> = BTreeMap::new();
    let mut remove: Vec<LineKey> = Vec::new();
    let keys: Vec<LineKey> = map.lines.keys().collect();
    for k in keys {
        let l = map.lines[k];
        let span = (l.v1.min(l.v2), l.v1.max(l.v2));
        if let Some(&keep) = seen.get(&span) {
            // A duplicate: fold a back side onto the survivor, then drop it.
            if map.lines[keep].back.is_none()
                && let Some(back) = l.back
            {
                map.lines[keep].back = Some(back);
            }
            remove.push(k);
        } else {
            seen.insert(span, k);
        }
    }
    map.remove_lines(&remove);
    remove
}

/// Reverse `line`'s direction (and swap its sides if two-sided), so what faced the back now faces the front; a stale key is a no-op.
pub(crate) fn flip_line(map: &mut EditorMap, line: LineKey) {
    let Some(line) = map.lines.get_mut(line) else {
        return;
    };
    mem::swap(&mut line.v1, &mut line.v2);
    if let Some(back) = line.back.take() {
        line.back = Some(mem::replace(&mut line.front, back));
    }
}

/// The world endpoints of `line`.
pub(crate) fn segment_points(map: &EditorMap, line: LineKey) -> ([f32; 2], [f32; 2]) {
    let l = &map.lines[line];
    let p1 = map.vertices[l.v1];
    let p2 = map.vertices[l.v2];
    ([p1.x, p1.y], [p2.x, p2.y])
}

/// World length of `line`, or `None` if an endpoint reference is stale.
pub fn line_length(map: &EditorMap, line: &LineDef) -> Option<f32> {
    let p1 = map.vertices.get(line.v1)?;
    let p2 = map.vertices.get(line.v2)?;
    Some(((p2.x - p1.x).powi(2) + (p2.y - p1.y).powi(2)).sqrt())
}

/// Apply collected `(line, point)` splits, returning each line that was split. Points on the same line are sorted along it and split in sequence; the latest tail half holds the remaining points, so each split lands on the correct piece.
fn apply_splits(map: &mut EditorMap, points: Vec<(LineKey, [f32; 2])>) -> Vec<LineKey> {
    // BTreeMap keeps a deterministic processing order across lines.
    let mut by_line: BTreeMap<LineKey, Vec<[f32; 2]>> = BTreeMap::new();
    for (line, p) in points {
        by_line.entry(line).or_default().push(p);
    }
    let mut split = Vec::new();
    for (line, mut pts) in by_line {
        let (a, _) = segment_points(map, line);
        // Sort by distance from v1 so splits proceed along the segment, then drop near-duplicate points so no zero-length piece is created.
        pts.sort_by(|p, q| {
            let dp = (p[0] - a[0]).powi(2) + (p[1] - a[1]).powi(2);
            let dq = (q[0] - a[0]).powi(2) + (q[1] - a[1]).powi(2);
            dp.total_cmp(&dq)
        });
        pts.dedup_by(|p, q| {
            (p[0] - q[0]).abs() < INTERSECT_EPS && (p[1] - q[1]).abs() < INTERSECT_EPS
        });
        let mut current = line;
        let mut did_split = false;
        for p in pts {
            // Skip a point already at the current piece's start vertex.
            let (s, _) = segment_points(map, current);
            if (s[0] - p[0]).abs() < INTERSECT_EPS && (s[1] - p[1]).abs() < INTERSECT_EPS {
                continue;
            }
            let (_, tail) = split_line_at(map, current, p);
            current = tail;
            did_split = true;
        }
        if did_split {
            split.push(line);
        }
    }
    split
}

/// Snap a coordinate to the nearest multiple of `grid` (no-op for `grid <= 0`); plain nearest-multiple rounding, the editor UI's `snap` uses DoomEd half-away-from-zero.
pub fn snap_coord(v: f32, grid: f32) -> f32 {
    if grid > 0.0 {
        (v / grid).round() * grid
    } else {
        v
    }
}

/// Choose the snap target for `raw`, preferring the nearest vertex, then a grid-aligned point on the nearest line, then the plain grid point; nearest candidate within its tolerance wins, and on-line points bias to the nearest grid intersection still on the line.
pub fn choose_snap(raw: [f32; 2], opts: &SnapOptions) -> [f32; 2] {
    let dist2 = |p: [f32; 2]| (p[0] - raw[0]).powi(2) + (p[1] - raw[1]).powi(2);

    if let Some(v) = opts
        .nearby_vertices
        .iter()
        .copied()
        .filter(|v| dist2(*v) <= opts.vertex_tol * opts.vertex_tol)
        .min_by(|a, b| dist2(*a).total_cmp(&dist2(*b)))
    {
        return v;
    }

    let on_line = opts
        .nearby_lines
        .iter()
        .map(|(a, b)| {
            let proj = nearest_point_on_segment(raw, *a, *b);
            // Prefer the grid intersection nearest the projection when it stays on the line; else the exact projection.
            let gp = [
                snap_coord(proj[0], opts.grid),
                snap_coord(proj[1], opts.grid),
            ];
            let snapped = nearest_point_on_segment(gp, *a, *b);
            if opts.grid_on
                && (snapped[0] - gp[0]).abs() < opts.line_tol
                && (snapped[1] - gp[1]).abs() < opts.line_tol
            {
                snapped
            } else {
                proj
            }
        })
        .filter(|p| dist2(*p) <= opts.line_tol * opts.line_tol)
        .min_by(|a, b| dist2(*a).total_cmp(&dist2(*b)));
    if let Some(p) = on_line {
        return p;
    }

    if let Some(from) = opts.angle_from
        && opts.angle_step_rad > 0.0
    {
        let d = [raw[0] - from[0], raw[1] - from[1]];
        if d[0].hypot(d[1]) > 0.0 {
            let ang = (d[1].atan2(d[0]) / opts.angle_step_rad).round() * opts.angle_step_rad;
            let dir = [ang.cos(), ang.sin()];
            let on_ray = |p: [f32; 2]| {
                let t = (p[0] - from[0]) * dir[0] + (p[1] - from[1]) * dir[1];
                [from[0] + dir[0] * t, from[1] + dir[1] * t]
            };
            let proj = on_ray(raw);
            // Prefer the grid intersection nearest the projection when it stays on the ray.
            if opts.grid_on {
                let gp = [
                    snap_coord(proj[0], opts.grid),
                    snap_coord(proj[1], opts.grid),
                ];
                let back = on_ray(gp);
                if (back[0] - gp[0]).abs() < opts.line_tol
                    && (back[1] - gp[1]).abs() < opts.line_tol
                {
                    return back;
                }
            }
            return proj;
        }
    }

    if opts.grid_on {
        [snap_coord(raw[0], opts.grid), snap_coord(raw[1], opts.grid)]
    } else {
        raw
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flags::LineFlags;
    use crate::model::{DenseLineDef, Vertex};
    use crate::test_fixtures::{dline_with, dside, line_keys, vert_keys, vtx};

    fn dline(v1: u32, v2: u32) -> DenseLineDef {
        dline_with(v1, v2, LineFlags::BLOCKING, Some(0))
    }

    /// Keyed map with 8 default sectors backing the sides.
    fn fixture(vertices: Vec<Vertex>, lines: Vec<DenseLineDef>) -> EditorMap {
        crate::test_fixtures::fixture(vertices, lines, 8)
    }

    #[test]
    fn intersection_crossing_parallel_collinear_endpoint() {
        // Proper crossing at the origin.
        let hit = segment_intersection([-1.0, 0.0], [1.0, 0.0], [0.0, -1.0], [0.0, 1.0]);
        let (p, _, _) = hit.expect("crossing");
        assert!(p[0].abs() < 1e-3 && p[1].abs() < 1e-3);
        // Parallel.
        assert!(segment_intersection([0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]).is_none());
        // Collinear.
        assert!(segment_intersection([0.0, 0.0], [2.0, 0.0], [1.0, 0.0], [3.0, 0.0]).is_none());
        // Endpoint touch (T-junction) is not an interior crossing.
        assert!(segment_intersection([-1.0, 0.0], [1.0, 0.0], [0.0, 0.0], [0.0, 1.0]).is_none());
    }

    #[test]
    fn on_segment_interior_detects_t_junction() {
        assert!(point_on_segment_interior([1.0, 0.0], [0.0, 0.0], [2.0, 0.0], 0.01).is_some());
        assert!(point_on_segment_interior([0.0, 0.0], [0.0, 0.0], [2.0, 0.0], 0.01).is_none());
        assert!(point_on_segment_interior([1.0, 1.0], [0.0, 0.0], [2.0, 0.0], 0.01).is_none());
    }

    #[test]
    fn split_preserves_sides_and_winding() {
        let mut map = fixture(
            vec![vtx(0.0, 0.0), vtx(4.0, 0.0)],
            vec![{
                let mut l = dline(0, 1);
                l.front = dside(Some(7));
                l
            }],
        );
        let head = line_keys(&map)[0];
        let (v_start, v_end) = (map.lines[head].v1, map.lines[head].v2);
        let front_sector = map.lines[head].front.sector;
        let (vnew, tail) = split_line_at(&mut map, head, [2.0, 0.0]);
        assert_eq!(map.vertices[vnew], vtx(2.0, 0.0));
        // Original is v1 -> vnew, new is vnew -> v2; both keep the front sector.
        assert_eq!((map.lines[head].v1, map.lines[head].v2), (v_start, vnew));
        let new_line = map.lines[tail];
        assert_eq!((new_line.v1, new_line.v2), (vnew, v_end));
        assert_eq!(new_line.front.sector, front_sector);
    }

    #[test]
    fn crossing_lines_split_into_four_sharing_a_vertex() {
        // A horizontal and a vertical line crossing at the origin.
        let mut map = fixture(
            vec![vtx(-2.0, 0.0), vtx(2.0, 0.0), vtx(0.0, -2.0), vtx(0.0, 2.0)],
            vec![dline(0, 1), dline(2, 3)],
        );
        let keys = line_keys(&map);
        let mut split = split_lines_at_intersections(&mut map, &keys, 0.01);
        split.sort_unstable();
        let mut expect = keys.clone();
        expect.sort_unstable();
        assert_eq!(split, expect, "both crossing lines reported split");
        assert_eq!(map.lines.len(), 4, "2 crossing lines -> 4");
        let centre = map
            .vertices
            .iter()
            .find(|(_, v)| v.x.abs() < 1e-3 && v.y.abs() < 1e-3)
            .map(|(k, _)| k)
            .expect("shared centre vertex");
        let touching = map
            .lines
            .values()
            .filter(|l| l.v1 == centre || l.v2 == centre)
            .count();
        assert_eq!(touching, 4, "all four halves meet at the centre");
    }

    #[test]
    fn endpoint_on_line_splits_only_that_line() {
        // A T-junction: line b ends on the interior of line a.
        let mut map = fixture(
            vec![vtx(0.0, 0.0), vtx(4.0, 0.0), vtx(2.0, 0.0), vtx(2.0, 3.0)],
            vec![dline(0, 1), dline(2, 3)],
        );
        let keys = line_keys(&map);
        let split = split_lines_at_intersections(&mut map, &[keys[1]], 0.01);
        assert_eq!(split, vec![keys[0]], "only the crossbar was split");
        assert_eq!(map.lines.len(), 3, "the stem splits the crossbar");
    }

    #[test]
    fn endpoint_split_works_in_either_direction() {
        // Same T-junction, but the active line is the crossbar: the stem's endpoint still splits it (reverse endpoint-on-line check).
        let mut map = fixture(
            vec![vtx(0.0, 0.0), vtx(4.0, 0.0), vtx(2.0, 0.0), vtx(2.0, 3.0)],
            vec![dline(0, 1), dline(2, 3)],
        );
        let keys = line_keys(&map);
        split_lines_at_intersections(&mut map, &[keys[0]], 0.01);
        assert_eq!(map.lines.len(), 3, "the crossbar is split at the stem foot");
    }

    #[test]
    fn snap_prefers_vertex_then_line_then_grid() {
        let verts = [[10.0, 10.0]];
        let lines = [([0.0, 5.0], [20.0, 5.0])];
        let opts = SnapOptions {
            grid: 8.0,
            grid_on: true,
            vertex_tol: 4.0,
            line_tol: 4.0,
            nearby_vertices: &verts,
            nearby_lines: &lines,
            angle_from: None,
            angle_step_rad: 0.0,
        };
        // Near the vertex: it wins.
        assert_eq!(choose_snap([11.0, 11.0], &opts), [10.0, 10.0]);
        // Near the line, far from the vertex: snaps onto the line, grid-aligned x.
        let p = choose_snap([8.0, 6.0], &opts);
        assert_eq!(p[1], 5.0, "lands on the line");
        assert_eq!(p[0], 8.0, "x biased to the grid");
        // Far from both: plain grid snap.
        assert_eq!(choose_snap([100.0, 100.0], &opts), [104.0, 104.0]);
        // Snap off (no candidates, no grid): raw passes through.
        let off = SnapOptions {
            grid_on: false,
            nearby_vertices: &[],
            nearby_lines: &[],
            ..opts
        };
        assert_eq!(choose_snap([3.0, 3.0], &off), [3.0, 3.0]);
    }

    #[test]
    fn angle_snap_projects_onto_nearest_ray() {
        use std::f32::consts::FRAC_PI_4;
        let opts = SnapOptions {
            grid: 8.0,
            grid_on: false,
            vertex_tol: 4.0,
            line_tol: 4.0,
            nearby_vertices: &[],
            nearby_lines: &[],
            angle_from: Some([0.0, 0.0]),
            angle_step_rad: FRAC_PI_4,
        };
        // ~22° from the origin lands on the 0° ray.
        let p = choose_snap([10.0, 4.0], &opts);
        assert_eq!(p[1], 0.0);
        assert!((p[0] - 10.0).abs() < 1e-4);
        // ~39° lands on the 45° ray.
        let q = choose_snap([10.0, 8.0], &opts);
        assert!((q[0] - q[1]).abs() < 1e-4, "x == y on the diagonal: {q:?}");
        // Grid bias holds the point on the ray.
        let gridded = SnapOptions {
            grid_on: true,
            ..opts
        };
        let g = choose_snap([10.0, 4.0], &gridded);
        assert_eq!(g, [8.0, 0.0], "grid point on the 0-degree ray");
    }

    #[test]
    fn ring_area_sign_tracks_winding() {
        let map = fixture(
            vec![vtx(0.0, 0.0), vtx(2.0, 0.0), vtx(2.0, 2.0), vtx(0.0, 2.0)],
            Vec::new(),
        );
        let v = vert_keys(&map);
        let ring = [v[0], v[1], v[2], v[3]];
        let reversed = [v[0], v[3], v[2], v[1]];
        assert!(ring_signed_area(&map, &ring) > 0.0); // CCW
        assert!(ring_signed_area(&map, &reversed) < 0.0); // CW
    }

    #[test]
    fn merge_coincident_vertices_unifies_and_prunes() {
        // Two lines sharing positions but via distinct vertices (as a drag leaves them): v0/v1 and v2/v3 both at (0,0)/(4,0).
        let mut map = fixture(
            vec![vtx(0.0, 0.0), vtx(4.0, 0.0), vtx(0.0, 0.0), vtx(4.0, 0.0)],
            vec![dline(0, 1), dline(2, 3)],
        );
        merge_coincident_vertices(&mut map);
        assert_eq!(map.vertices.len(), 2, "coincident vertices unified");
        // Both lines now reference the same two vertices.
        let lines: Vec<&LineDef> = map.lines.values().collect();
        assert_eq!(
            (lines[0].v1.min(lines[0].v2), lines[0].v1.max(lines[0].v2)),
            (lines[1].v1.min(lines[1].v2), lines[1].v1.max(lines[1].v2)),
        );
    }

    #[test]
    fn dedup_coincident_lines_collapses_and_folds_back_side() {
        let mut map = fixture(
            vec![vtx(0.0, 0.0), vtx(4.0, 0.0)],
            vec![dline(0, 1), dline(1, 0)], // same span, opposite winding
        );
        let keys = line_keys(&map);
        // Give the duplicate a back side to verify it folds onto the survivor.
        map.lines[keys[1]].back = Some(map.lines[keys[1]].front);
        let removed = dedup_coincident_lines(&mut map);
        assert_eq!(removed, vec![keys[1]]);
        assert_eq!(map.lines.len(), 1, "coincident lines collapsed to one");
        assert!(
            map.lines[keys[0]].back.is_some(),
            "back side folded onto survivor"
        );
    }

    #[test]
    fn weld_moved_vertex_onto_neighbour_drops_collapsed_line() {
        // Chain a(0)-b(1)-c(2); drag b's vertex onto c (within tol).
        let mut map = fixture(
            vec![vtx(0.0, 0.0), vtx(4.0, 0.1), vtx(4.0, 0.0)],
            vec![dline(0, 1), dline(1, 2)],
        );
        let v = vert_keys(&map);
        assert!(weld_moved_vertices(&mut map, &[v[1]], 1.0));
        // Line b-c collapsed (zero-length) and was removed; a-c survives.
        assert_eq!(map.lines.len(), 1);
        let l = map.lines.values().next().expect("one line");
        let mut xs = [map.vertices[l.v1].x, map.vertices[l.v2].x];
        xs.sort_by(|a, b| a.total_cmp(b));
        assert_eq!(xs, [0.0, 4.0], "a-c remains after b welds onto c");
    }

    #[test]
    fn delete_vertex_merges_two_single_sided() {
        // a(0)-v(1)-b(2): deleting v joins the two single-sided lines to a-b.
        let mut map = fixture(
            vec![vtx(0.0, 0.0), vtx(2.0, 0.0), vtx(4.0, 0.0)],
            vec![dline(0, 1), dline(1, 2)],
        );
        let v = vert_keys(&map);
        delete_vertex(&mut map, v[1]);
        assert_eq!(map.lines.len(), 1, "two singles merged to one");
        assert_eq!(map.vertices.len(), 2, "middle vertex pruned");
        let l = map.lines.values().next().expect("one line");
        let mut xs = [map.vertices[l.v1].x, map.vertices[l.v2].x];
        xs.sort_by(|a, b| a.total_cmp(b));
        assert_eq!(xs, [0.0, 4.0], "spans the far endpoints");
    }

    #[test]
    fn delete_vertex_deletes_two_sided_lines() {
        // Two-sided line v(0)-b(1); a single-sided spur v-c(2). Deleting v drops the two-sided line and the lone single-sided one (only one single).
        let mut map = fixture(
            vec![vtx(0.0, 0.0), vtx(2.0, 0.0), vtx(0.0, 2.0)],
            vec![
                {
                    let mut l = dline(0, 1);
                    l.back = Some(dside(Some(1)));
                    l
                },
                dline(0, 2),
            ],
        );
        let v = vert_keys(&map);
        delete_vertex(&mut map, v[0]);
        assert!(map.lines.is_empty(), "two-sided + lone single both deleted");
    }

    #[test]
    fn delete_vertex_three_singles_deletes_all() {
        // A junction of three single-sided lines: no clean merge, delete all.
        let mut map = fixture(
            vec![vtx(0.0, 0.0), vtx(2.0, 0.0), vtx(-2.0, 0.0), vtx(0.0, 2.0)],
            vec![dline(0, 1), dline(0, 2), dline(0, 3)],
        );
        let v = vert_keys(&map);
        delete_vertex(&mut map, v[0]);
        assert!(map.lines.is_empty(), "3-way junction lines all deleted");
    }
}
