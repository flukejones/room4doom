//! Pure 2D geometry for the editor: point/segment tests, line intersection and
//! splitting, ring orientation, loop detection, and snap-candidate selection.
//!
//! Slint-free and app-state-free so the editor's drawing logic (faced-sector
//! inheritance, sector carving, intersection cuts, snapping) is unit-testable
//! here against a real [`EditorMap`]. Points are world-space `[f32; 2]`.

use std::collections::BTreeMap;

use crate::model::{EditorMap, LineDef};

/// Parametric epsilon: a crossing counts as interior only when both segment
/// parameters land strictly inside `(EPS, 1 - EPS)`, excluding endpoint touches.
const INTERSECT_EPS: f32 = 1e-4;

/// True when `world` lies on the front (right) side of `p1`->`p2` in world
/// (Y-up) coordinates. The single facing rule shared by sector probing, the
/// hover preview, and the scanline fill.
pub fn is_front_side(world: [f32; 2], p1: [f32; 2], p2: [f32; 2]) -> bool {
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

/// The proper interior crossing of segments (`a1`,`a2`) and (`b1`,`b2`).
///
/// Returns the point and both parameters only when each is strictly inside its
/// segment. `None` for parallel, collinear, or endpoint-only touches (those are
/// handled by [`point_on_segment_interior`]).
pub fn segment_intersection(
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

/// The parameter of `p` along segment (`a`,`b`) when `p` lies on it within
/// `tol`, strictly between the endpoints. `None` at the endpoints or off-segment.
pub fn point_on_segment_interior(p: [f32; 2], a: [f32; 2], b: [f32; 2], tol: f32) -> Option<f32> {
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

/// Signed area of the polygon formed by a ring of vertex indices (shoelace).
/// Positive is counter-clockwise in world (Y-up) coordinates.
pub fn ring_signed_area(map: &EditorMap, ring: &[u32]) -> f32 {
    let mut sum = 0.0;
    for i in 0..ring.len() {
        let a = map.vertices[ring[i] as usize];
        let b = map.vertices[ring[(i + 1) % ring.len()] as usize];
        sum += a.x * b.y - b.x * a.y;
    }
    sum * 0.5
}

/// True when `p` is inside the polygon ring (even-odd ray cast).
pub fn point_in_ring(map: &EditorMap, ring: &[u32], p: [f32; 2]) -> bool {
    let mut inside = false;
    let n = ring.len();
    let mut j = n - 1;
    for i in 0..n {
        let vi = map.vertices[ring[i] as usize];
        let vj = map.vertices[ring[j] as usize];
        if (vi.y > p[1]) != (vj.y > p[1]) {
            let x = (vj.x - vi.x) * (p[1] - vi.y) / (vj.y - vi.y) + vi.x;
            if p[0] < x {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

/// The sector at world point `p`, by the same horizontal-scanline rule the
/// sector fill uses (`render/scene.rs::walk_sector_spans`).
///
/// Along `y = p.y`, the sector is the one entered at the last line crossing left
/// of `p.x`; correct for concave sectors. `None` for the void.
pub fn sector_at(map: &EditorMap, p: [f32; 2]) -> Option<u32> {
    let [px, py] = p;
    // The entered sector at the nearest crossing at or left of the point.
    let mut best: Option<(f32, Option<u32>)> = None;
    for line in &map.lines {
        let v1 = map.vertices[line.v1 as usize];
        let v2 = map.vertices[line.v2 as usize];
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

/// The floor height a thing at world-XY `p` sits at: its containing sector's
/// floor, or 0 in the void. A thing is fully 3D; this is the authoritative rule
/// for its stored `z`.
pub fn thing_floor_z(map: &EditorMap, p: [f32; 2]) -> i32 {
    sector_at(map, p).map_or(0, |s| map.sectors[s as usize].floor_height)
}

/// Re-derive every thing's stored `z` from its containing sector floor. Called
/// once after a load/import (the WAD format carries no thing Z) and after edits
/// that move things or change floor heights.
pub fn derive_thing_heights(map: &mut EditorMap) {
    for i in 0..map.things.len() {
        let p = [map.things[i].x as f32, map.things[i].y as f32];
        map.things[i].z = thing_floor_z(map, p);
    }
}

/// Split line `line_idx` at `point` into two lines sharing a new vertex.
///
/// The original becomes `v1`->`vnew`; a new line `vnew`->`v2` is appended; both
/// keep the original sides and flags. Returns the new vertex index and the new
/// line index. The `v1`->`v2` direction is preserved on both halves so front/back
/// stay on the correct geometric sides.
pub fn split_line_at(map: &mut EditorMap, line_idx: u32, point: [f32; 2]) -> (u32, u32) {
    let vnew = map.find_or_add_vertex(point);
    let original = &map.lines[line_idx as usize];
    let tail = LineDef {
        v1: vnew,
        v2: original.v2,
        flags: original.flags,
        special: original.special,
        tag: original.tag,
        front: original.front,
        back: original.back,
    };
    map.lines[line_idx as usize].v2 = vnew;
    map.lines.push(tail);
    (vnew, (map.lines.len() - 1) as u32)
}

/// Split the given `lines` wherever they cross or touch any line in the map.
///
/// Returns the indices of every line that was split (the in-place "head" halves;
/// the appended "tail" halves are the new lines past the old length).
///
/// A proper crossing splits both lines at the point; an endpoint of one line
/// landing on the interior of another splits that other line at the endpoint.
/// All split points are collected first, then applied per line, so growing the
/// line list never invalidates an in-flight index.
pub fn split_lines_at_intersections(map: &mut EditorMap, lines: &[u32], tol: f32) -> Vec<u32> {
    let mut points: Vec<(u32, [f32; 2])> = Vec::new();
    for &a in lines {
        let (a1, a2) = segment_points(map, a);
        for b in 0..map.lines.len() as u32 {
            if a == b {
                continue;
            }
            let (b1, b2) = segment_points(map, b);
            if let Some((p, _, _)) = segment_intersection(a1, a2, b1, b2) {
                points.push((a, p));
                points.push((b, p));
            }
            // Either line's endpoint touching the other's interior is a
            // T-junction; split the line that is touched at that point.
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

/// Weld each moved vertex onto the nearest stationary vertex within `tol`.
///
/// Run after a drag: a vertex dropped within tolerance of another existing
/// vertex remaps onto it (lines re-point to the target), and any line whose
/// endpoints collapse to one vertex is deleted. Only `moved_ids` weld, and only
/// onto vertices outside that set, so a stationary target never shifts and two
/// moved vertices never chain. Returns whether anything changed.
pub fn weld_moved_vertices(map: &mut EditorMap, moved_ids: &[u32], tol: f32) -> bool {
    let tol_sq = tol * tol;
    let moved: Vec<bool> = {
        let mut m = vec![false; map.vertices.len()];
        for &i in moved_ids {
            if let Some(slot) = m.get_mut(i as usize) {
                *slot = true;
            }
        }
        m
    };
    let mut remap: Vec<u32> = (0..map.vertices.len() as u32).collect();
    let mut welded = false;
    for &i in moved_ids {
        let Some(&p) = map.vertices.get(i as usize) else {
            continue;
        };
        let mut best: Option<(f32, u32)> = None;
        for (j, v) in map.vertices.iter().enumerate() {
            if moved[j] {
                continue;
            }
            let d = (v.x - p.x) * (v.x - p.x) + (v.y - p.y) * (v.y - p.y);
            if d <= tol_sq && best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, j as u32));
            }
        }
        if let Some((_, target)) = best {
            remap[i as usize] = target;
            welded = true;
        }
    }
    if !welded {
        return false;
    }
    for l in &mut map.lines {
        l.v1 = remap[l.v1 as usize];
        l.v2 = remap[l.v2 as usize];
    }
    let collapsed: Vec<u32> = (0..map.lines.len() as u32)
        .filter(|&i| map.lines[i as usize].v1 == map.lines[i as usize].v2)
        .collect();
    map.remove_lines(&collapsed);
    map.prune_orphan_vertices();
    true
}

/// Weld `ids` together at `target`.
///
/// Move each to the point, unify the now coincident vertices, then drop any line
/// that collapsed to a point or duplicates another. Returns whether anything
/// welded (fewer than two ids is a no-op).
pub fn weld_vertices(map: &mut EditorMap, ids: &[u32], target: [f32; 2]) -> bool {
    if ids.len() < 2 {
        return false;
    }
    for &i in ids {
        if let Some(v) = map.vertices.get_mut(i as usize) {
            v.x = target[0];
            v.y = target[1];
        }
    }
    merge_coincident_vertices(map);
    let degenerate: Vec<u32> = (0..map.lines.len() as u32)
        .filter(|&i| map.lines[i as usize].v1 == map.lines[i as usize].v2)
        .collect();
    map.remove_lines(&degenerate);
    dedup_coincident_lines(map);
    true
}

/// The index of the vertex at exactly `p` (bit-equal), if any. Resolves a
/// vertex by position when prior edits have renumbered the array.
pub fn vertex_at(map: &EditorMap, p: [f32; 2]) -> Option<u32> {
    map.vertices
        .iter()
        .position(|v| v.x.to_bits() == p[0].to_bits() && v.y.to_bits() == p[1].to_bits())
        .map(|i| i as u32)
}

/// Delete a vertex, resolving the lines attached to it.
///
/// Two-sided lines at the vertex are deleted. Single-sided lines are deleted
/// too, except the one case of exactly two of them: those dissolve the vertex
/// into a single line spanning their far endpoints (keeping the first line's
/// sides and flags). The vertex then prunes; any duplicate or degenerate line
/// the merge produced is collapsed.
pub fn delete_vertex(map: &mut EditorMap, vertex: u32) {
    let far = |l: &LineDef| if l.v1 == vertex { l.v2 } else { l.v1 };
    let incident: Vec<u32> = (0..map.lines.len() as u32)
        .filter(|&i| {
            let l = &map.lines[i as usize];
            l.v1 == vertex || l.v2 == vertex
        })
        .collect();
    let singles: Vec<u32> = incident
        .iter()
        .copied()
        .filter(|&i| map.lines[i as usize].back.is_none())
        .collect();

    let mut remove = incident.clone();
    if singles.len() == 2 {
        let (keep, drop) = (singles[0], singles[1]);
        let b = far(&map.lines[drop as usize]);
        let line = &mut map.lines[keep as usize];
        if line.v1 == vertex {
            line.v1 = b;
        } else {
            line.v2 = b;
        }
        remove.retain(|&i| i != keep);
    }
    map.remove_lines(&remove);
    merge_coincident_vertices(map);
    let degenerate: Vec<u32> = (0..map.lines.len() as u32)
        .filter(|&i| map.lines[i as usize].v1 == map.lines[i as usize].v2)
        .collect();
    map.remove_lines(&degenerate);
    dedup_coincident_lines(map);
}

/// Unify vertices that share an exact position.
///
/// Every line endpoint is rewritten to the lowest-indexed vertex at its
/// position, then orphaned vertices prune away. Run after a drag (which moves
/// vertices in place) so two walls dragged to the same spot share endpoints,
/// letting [`dedup_coincident_lines`] match.
pub fn merge_coincident_vertices(map: &mut EditorMap) {
    let mut canonical: BTreeMap<(u64, u64), u32> = BTreeMap::new();
    let mut remap: Vec<u32> = (0..map.vertices.len() as u32).collect();
    for (i, v) in map.vertices.iter().enumerate() {
        let key = (v.x.to_bits() as u64, v.y.to_bits() as u64);
        let lead = *canonical.entry(key).or_insert(i as u32);
        remap[i] = lead;
    }
    for l in &mut map.lines {
        l.v1 = remap[l.v1 as usize];
        l.v2 = remap[l.v2 as usize];
    }
    map.prune_orphan_vertices();
}

/// Collapse lines that share both endpoints (in either direction) to a single
/// line.
///
/// So dragging a wall to rest on a collinear one does not leave two coincident
/// lines. The first occurrence survives; if any duplicate carried a back side,
/// the survivor adopts it (two one-sided walls meeting become a two-sided
/// line). Returns the indices removed.
pub fn dedup_coincident_lines(map: &mut EditorMap) -> Vec<u32> {
    let mut seen: BTreeMap<(u32, u32), usize> = BTreeMap::new();
    let mut remove: Vec<u32> = Vec::new();
    for i in 0..map.lines.len() {
        let l = &map.lines[i];
        let key = (l.v1.min(l.v2), l.v1.max(l.v2));
        if let Some(&keep) = seen.get(&key) {
            // A duplicate: fold a back side onto the survivor, then drop it.
            let dup_back = map.lines[i].back;
            if map.lines[keep].back.is_none()
                && let Some(back) = dup_back
            {
                map.lines[keep].back = Some(back);
            }
            remove.push(i as u32);
        } else {
            seen.insert(key, i);
        }
    }
    map.remove_lines(&remove);
    remove
}

/// The world endpoints of line `idx`.
pub(crate) fn segment_points(map: &EditorMap, idx: u32) -> ([f32; 2], [f32; 2]) {
    let l = &map.lines[idx as usize];
    let p1 = map.vertices[l.v1 as usize];
    let p2 = map.vertices[l.v2 as usize];
    ([p1.x, p1.y], [p2.x, p2.y])
}

/// World length of `line`, or `None` if an endpoint vertex is out of range.
pub fn line_length(map: &EditorMap, line: &LineDef) -> Option<f32> {
    let p1 = map.vertices.get(line.v1 as usize)?;
    let p2 = map.vertices.get(line.v2 as usize)?;
    Some(((p2.x - p1.x).powi(2) + (p2.y - p1.y).powi(2)).sqrt())
}

/// Apply collected `(line, point)` splits, returning each line that was split.
/// Points on the same line are sorted along it and split in sequence; the latest
/// tail half holds the remaining points, so each split lands on the correct
/// piece.
fn apply_splits(map: &mut EditorMap, points: Vec<(u32, [f32; 2])>) -> Vec<u32> {
    // BTreeMap keeps a deterministic processing order across lines.
    let mut by_line: BTreeMap<u32, Vec<[f32; 2]>> = BTreeMap::new();
    for (line, p) in points {
        by_line.entry(line).or_default().push(p);
    }
    let mut split = Vec::new();
    for (line, mut pts) in by_line {
        let (a, _) = segment_points(map, line);
        // Sort by distance from v1 so splits proceed along the segment, then
        // drop near-duplicate points so no zero-length piece is created.
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

/// Snap a coordinate to the nearest multiple of `grid` (no-op for `grid <= 0`).
pub fn snap_coord(v: f32, grid: f32) -> f32 {
    if grid > 0.0 {
        (v / grid).round() * grid
    } else {
        v
    }
}

/// Choose the snap target for `raw`, preferring (in order) the nearest vertex,
/// then a grid-aligned point on the nearest line, then the plain grid point.
///
/// `grid_on` toggles grid snapping; `vertex`/`line` toggle geometry snapping.
/// `nearby_vertices` and `nearby_lines` (as endpoint pairs) are pre-filtered by
/// the caller to those within the snap radius; the nearest within `vertex_tol` /
/// `line_tol` wins. On-line points bias to the nearest grid intersection that is
/// still on the line, so results stay grid-aligned where possible.
pub fn choose_snap(
    raw: [f32; 2],
    grid: f32,
    grid_on: bool,
    vertex: bool,
    line: bool,
    vertex_tol: f32,
    line_tol: f32,
    nearby_vertices: &[[f32; 2]],
    nearby_lines: &[([f32; 2], [f32; 2])],
) -> [f32; 2] {
    let dist2 = |p: [f32; 2]| (p[0] - raw[0]).powi(2) + (p[1] - raw[1]).powi(2);

    if vertex
        && let Some(v) = nearby_vertices
            .iter()
            .copied()
            .filter(|v| dist2(*v) <= vertex_tol * vertex_tol)
            .min_by(|a, b| dist2(*a).total_cmp(&dist2(*b)))
    {
        return v;
    }

    if line {
        let on_line = nearby_lines
            .iter()
            .map(|(a, b)| {
                let proj = nearest_point_on_segment(raw, *a, *b);
                // Prefer the grid intersection nearest the projection when it
                // stays on the line; else the exact projection.
                let gp = [snap_coord(proj[0], grid), snap_coord(proj[1], grid)];
                let snapped = nearest_point_on_segment(gp, *a, *b);
                if grid_on
                    && (snapped[0] - gp[0]).abs() < line_tol
                    && (snapped[1] - gp[1]).abs() < line_tol
                {
                    snapped
                } else {
                    proj
                }
            })
            .filter(|p| dist2(*p) <= line_tol * line_tol)
            .min_by(|a, b| dist2(*a).total_cmp(&dist2(*b)));
        if let Some(p) = on_line {
            return p;
        }
    }

    if grid_on {
        [snap_coord(raw[0], grid), snap_coord(raw[1], grid)]
    } else {
        raw
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flags::LineFlags;
    use crate::model::{LineDef, SideDef, Vertex};
    use crate::name8::Name8;

    fn vtx(x: f32, y: f32) -> Vertex {
        Vertex {
            x,
            y,
        }
    }

    fn side(sector: u32) -> SideDef {
        SideDef {
            x_offset: 0,
            y_offset: 0,
            top_tex: Name8::EMPTY,
            bottom_tex: Name8::EMPTY,
            middle_tex: Name8::EMPTY,
            sector: Some(sector),
        }
    }

    fn line(v1: u32, v2: u32) -> LineDef {
        LineDef {
            v1,
            v2,
            flags: LineFlags::BLOCKING,
            special: 0,
            tag: 0,
            front: side(0),
            back: None,
        }
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
        let mut map = EditorMap {
            vertices: vec![vtx(0.0, 0.0), vtx(4.0, 0.0)],
            lines: vec![{
                let mut l = line(0, 1);
                l.front = side(7);
                l
            }],
            ..Default::default()
        };
        let (vnew, new_idx) = split_line_at(&mut map, 0, [2.0, 0.0]);
        assert_eq!(map.vertices[vnew as usize], vtx(2.0, 0.0));
        // Original is v1 -> vnew, new is vnew -> v2; both keep the front sector.
        assert_eq!((map.lines[0].v1, map.lines[0].v2), (0, vnew));
        let new_line = &map.lines[new_idx as usize];
        assert_eq!((new_line.v1, new_line.v2), (vnew, 1));
        assert_eq!(new_line.front.sector, Some(7));
    }

    #[test]
    fn crossing_lines_split_into_four_sharing_a_vertex() {
        // A horizontal and a vertical line crossing at the origin.
        let mut map = EditorMap {
            vertices: vec![vtx(-2.0, 0.0), vtx(2.0, 0.0), vtx(0.0, -2.0), vtx(0.0, 2.0)],
            lines: vec![line(0, 1), line(2, 3)],
            ..Default::default()
        };
        let mut split = split_lines_at_intersections(&mut map, &[0, 1], 0.01);
        split.sort_unstable();
        assert_eq!(split, vec![0, 1], "both crossing lines reported split");
        assert_eq!(map.lines.len(), 4, "2 crossing lines -> 4");
        let centre = map
            .vertices
            .iter()
            .position(|v| v.x.abs() < 1e-3 && v.y.abs() < 1e-3)
            .expect("shared centre vertex") as u32;
        let touching = map
            .lines
            .iter()
            .filter(|l| l.v1 == centre || l.v2 == centre)
            .count();
        assert_eq!(touching, 4, "all four halves meet at the centre");
    }

    #[test]
    fn endpoint_on_line_splits_only_that_line() {
        // A T-junction: line b ends on the interior of line a.
        let mut map = EditorMap {
            vertices: vec![vtx(0.0, 0.0), vtx(4.0, 0.0), vtx(2.0, 0.0), vtx(2.0, 3.0)],
            lines: vec![line(0, 1), line(2, 3)],
            ..Default::default()
        };
        let split = split_lines_at_intersections(&mut map, &[1], 0.01);
        assert_eq!(split, vec![0], "only the crossbar (line 0) was split");
        assert_eq!(map.lines.len(), 3, "the stem splits the crossbar");
    }

    #[test]
    fn endpoint_split_works_in_either_direction() {
        // Same T-junction, but the active line is the crossbar: the stem's
        // endpoint still splits it (reverse endpoint-on-line check).
        let mut map = EditorMap {
            vertices: vec![vtx(0.0, 0.0), vtx(4.0, 0.0), vtx(2.0, 0.0), vtx(2.0, 3.0)],
            lines: vec![line(0, 1), line(2, 3)],
            ..Default::default()
        };
        split_lines_at_intersections(&mut map, &[0], 0.01);
        assert_eq!(map.lines.len(), 3, "the crossbar is split at the stem foot");
    }

    #[test]
    fn snap_prefers_vertex_then_line_then_grid() {
        let verts = [[10.0, 10.0]];
        let lines = [([0.0, 5.0], [20.0, 5.0])];
        // Near the vertex: it wins.
        let p = choose_snap(
            [11.0, 11.0],
            8.0,
            true,
            true,
            true,
            4.0,
            4.0,
            &verts,
            &lines,
        );
        assert_eq!(p, [10.0, 10.0]);
        // Near the line, far from the vertex: snaps onto the line, grid-aligned x.
        let p = choose_snap([8.0, 6.0], 8.0, true, true, true, 4.0, 4.0, &verts, &lines);
        assert_eq!(p[1], 5.0, "lands on the line");
        assert_eq!(p[0], 8.0, "x biased to the grid");
        // Far from both: plain grid snap.
        let p = choose_snap(
            [100.0, 100.0],
            8.0,
            true,
            true,
            true,
            4.0,
            4.0,
            &verts,
            &lines,
        );
        assert_eq!(p, [104.0, 104.0]);
        // Snap off: raw passes through.
        let p = choose_snap(
            [3.0, 3.0],
            8.0,
            false,
            false,
            false,
            4.0,
            4.0,
            &verts,
            &lines,
        );
        assert_eq!(p, [3.0, 3.0]);
    }

    #[test]
    fn ring_area_sign_tracks_winding() {
        let map = EditorMap {
            vertices: vec![vtx(0.0, 0.0), vtx(2.0, 0.0), vtx(2.0, 2.0), vtx(0.0, 2.0)],
            ..Default::default()
        };
        assert!(ring_signed_area(&map, &[0, 1, 2, 3]) > 0.0); // CCW
        assert!(ring_signed_area(&map, &[0, 3, 2, 1]) < 0.0); // CW
        assert!(point_in_ring(&map, &[0, 1, 2, 3], [1.0, 1.0]));
        assert!(!point_in_ring(&map, &[0, 1, 2, 3], [3.0, 3.0]));
    }

    #[test]
    fn merge_coincident_vertices_unifies_and_prunes() {
        // Two lines sharing positions but via distinct vertices (as a drag
        // leaves them): v0/v1 and v2/v3 both at (0,0)/(4,0).
        let mut map = EditorMap {
            vertices: vec![vtx(0.0, 0.0), vtx(4.0, 0.0), vtx(0.0, 0.0), vtx(4.0, 0.0)],
            lines: vec![line(0, 1), line(2, 3)],
            ..Default::default()
        };
        merge_coincident_vertices(&mut map);
        assert_eq!(map.vertices.len(), 2, "coincident vertices unified");
        // Both lines now reference the same two vertices.
        assert_eq!(
            (
                map.lines[0].v1.min(map.lines[0].v2),
                map.lines[0].v1.max(map.lines[0].v2)
            ),
            (
                map.lines[1].v1.min(map.lines[1].v2),
                map.lines[1].v1.max(map.lines[1].v2)
            ),
        );
    }

    #[test]
    fn dedup_coincident_lines_collapses_and_folds_back_side() {
        let mut map = EditorMap {
            vertices: vec![vtx(0.0, 0.0), vtx(4.0, 0.0)],
            lines: vec![line(0, 1), line(1, 0)], // same span, opposite winding
            ..Default::default()
        };
        // Give the duplicate a back side to verify it folds onto the survivor.
        map.lines[1].back = Some(side(7));
        let removed = dedup_coincident_lines(&mut map);
        assert_eq!(removed, vec![1]);
        assert_eq!(map.lines.len(), 1, "coincident lines collapsed to one");
        assert!(
            map.lines[0].back.is_some(),
            "back side folded onto survivor"
        );
    }

    #[test]
    fn weld_moved_vertex_onto_neighbour_drops_collapsed_line() {
        // Chain a(0)-b(1)-c(2); drag b's vertex (id 1) onto c (within tol).
        let mut map = EditorMap {
            vertices: vec![vtx(0.0, 0.0), vtx(4.0, 0.1), vtx(4.0, 0.0)],
            lines: vec![line(0, 1), line(1, 2)],
            ..Default::default()
        };
        assert!(weld_moved_vertices(&mut map, &[1], 1.0));
        // Line b-c collapsed (zero-length) and was removed; a-c survives.
        assert_eq!(map.lines.len(), 1);
        let l = &map.lines[0];
        assert_eq!(
            (l.v1.min(l.v2), l.v1.max(l.v2)),
            (0, 1),
            "a-c remains after b welds onto c"
        );
    }

    #[test]
    fn delete_vertex_merges_two_single_sided() {
        // a(0)-v(1)-b(2): deleting v joins the two single-sided lines to a-b.
        let mut map = EditorMap {
            vertices: vec![vtx(0.0, 0.0), vtx(2.0, 0.0), vtx(4.0, 0.0)],
            lines: vec![line(0, 1), line(1, 2)],
            ..Default::default()
        };
        delete_vertex(&mut map, 1);
        assert_eq!(map.lines.len(), 1, "two singles merged to one");
        assert_eq!(map.vertices.len(), 2, "middle vertex pruned");
        let l = &map.lines[0];
        let mut xs = [map.vertices[l.v1 as usize].x, map.vertices[l.v2 as usize].x];
        xs.sort_by(|a, b| a.total_cmp(b));
        assert_eq!(xs, [0.0, 4.0], "spans the far endpoints");
    }

    #[test]
    fn delete_vertex_deletes_two_sided_lines() {
        // Two-sided line v(0)-b(1); a single-sided spur v-c(2). Deleting v drops
        // the two-sided line and the lone single-sided one (only one single).
        let mut map = EditorMap {
            vertices: vec![vtx(0.0, 0.0), vtx(2.0, 0.0), vtx(0.0, 2.0)],
            lines: vec![
                {
                    let mut l = line(0, 1);
                    l.back = Some(side(1));
                    l
                },
                line(0, 2),
            ],
            ..Default::default()
        };
        delete_vertex(&mut map, 0);
        assert!(map.lines.is_empty(), "two-sided + lone single both deleted");
    }

    #[test]
    fn delete_vertex_three_singles_deletes_all() {
        // A junction of three single-sided lines: no clean merge, delete all.
        let mut map = EditorMap {
            vertices: vec![vtx(0.0, 0.0), vtx(2.0, 0.0), vtx(-2.0, 0.0), vtx(0.0, 2.0)],
            lines: vec![line(0, 1), line(0, 2), line(0, 3)],
            ..Default::default()
        };
        delete_vertex(&mut map, 0);
        assert!(map.lines.is_empty(), "3-way junction lines all deleted");
    }
}
