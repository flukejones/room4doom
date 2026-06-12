//! Higher-level editing operations over an [`EditorMap`]: weld vertices, move geometry, construct edges/shapes, copy/paste a self-contained fragment, flip lines, merge sectors — composing the primitives in [`crate::geom`] and [`crate::sector_build`]. Tolerances arrive in world units; pixel/grid concerns belong to the caller.

use std::collections::{HashMap, HashSet};
use std::f32::consts::{PI, TAU};
use std::mem;

use crate::flags::LineFlags;
use crate::geom::{
    dedup_coincident_lines, flip_line, merge_coincident_moved, nearest_point_on_segment,
    segment_points, split_lines_at_intersections, weld_moved_vertices, weld_vertices,
};
use crate::model::{
    EditorMap, LineDef, LineKey, Sector, SectorKey, SideDef, Thing, ThingKey, VertKey,
};
use crate::sector_build::{VoidRule, build_sectors};

/// Max fraction of a line's length a corner trim (fillet/chamfer) may consume.
const TRIM_MAX_FRAC: f32 = 0.95;
/// Minimum deviation from straight (radians) for a vertex to count as a corner.
const CORNER_STRAIGHT_EPS: f32 = 1e-3;
/// Minimum normalised cross product between extrude delta and line direction (parallel reject).
const EXTRUDE_PARALLEL_EPS: f32 = 1e-3;

/// Weld the candidates within `tol` of their centroid onto one point; collapsed/duplicate lines are removed, a sector-less loop the weld closes gains a sector from `default_sector` (open chains stay void), and the return says whether anything welded.
pub fn weld_cluster(
    map: &mut EditorMap,
    candidate_ids: &[VertKey],
    tol: f32,
    default_sector: Sector,
) -> bool {
    let pts: Vec<(VertKey, [f32; 2])> = candidate_ids
        .iter()
        .filter_map(|&k| map.vertices.get(k).map(|v| (k, [v.x, v.y])))
        .collect();
    if pts.len() < 2 {
        return false;
    }
    let centroid = mean(pts.iter().map(|(_, p)| *p));
    let tol_sq = tol * tol;
    let near: Vec<(VertKey, [f32; 2])> = pts
        .into_iter()
        .filter(|(_, p)| dist_sq(*p, centroid) <= tol_sq)
        .collect();
    if near.len() < 2 {
        return false;
    }
    let target = mean(near.iter().map(|(_, p)| *p));
    let ids: Vec<VertKey> = near.iter().map(|(k, _)| *k).collect();

    weld_vertices(map, &ids, target);
    let seeds = lines_at_positions(map, &[target]);
    let newly = flood_sectorless(map, &seeds);
    build_sectors(map, &newly, default_sector, VoidRule::KeepPockets);
    true
}

/// Commit a vertex (and thing) move to the map: `moves`/`thing_moves` carry the final, already-snapped positions per key. After applying, lines incident to a moved vertex are split where they now cross, coincident vertices/lines collapse, and — only on a real topology change — the lines around the move are re-sectored against `default_sector`. `tol` is in world units; the return is the re-sectored line set (empty when nothing re-sectored).
pub fn move_vertices(
    map: &mut EditorMap,
    moves: &[(VertKey, [f32; 2])],
    thing_moves: &[(ThingKey, [i32; 2])],
    tol: f32,
    default_sector: Sector,
) -> Vec<LineKey> {
    for &(k, p) in moves {
        if let Some(v) = map.vertices.get_mut(k) {
            v.x = p[0];
            v.y = p[1];
        }
    }
    for &(k, p) in thing_moves {
        if let Some(t) = map.things.get_mut(k) {
            t.x = p[0];
            t.y = p[1];
        }
    }
    let vert_ids: Vec<VertKey> = moves.iter().map(|(k, _)| *k).collect();
    let moved_set: HashSet<VertKey> = vert_ids.iter().copied().collect();
    let moved_lines: Vec<LineKey> = map
        .lines
        .iter()
        .filter(|(_, l)| moved_set.contains(&l.v1) || moved_set.contains(&l.v2))
        .map(|(k, _)| k)
        .collect();
    if moved_lines.is_empty() {
        return Vec::new();
    }
    // Re-sectoring is scoped by position: the merge/dedup below can remove the moved vertices themselves, but post-move positions are byte-stable.
    let moved_pos: Vec<[f32; 2]> = vert_ids
        .iter()
        .filter_map(|&k| map.vertices.get(k).map(|v| [v.x, v.y]))
        .collect();
    let verts_before: HashSet<VertKey> = map.vertices.keys().collect();
    let crossed = split_lines_at_intersections(map, &moved_lines, tol);
    let crossings: Vec<[f32; 2]> = map
        .vertices
        .iter()
        .filter(|(k, _)| !verts_before.contains(k))
        .map(|(_, v)| [v.x, v.y])
        .collect();
    let welded = weld_moved_vertices(map, &vert_ids, tol);
    merge_coincident_moved(map, &vert_ids);
    let deduped = dedup_coincident_lines(map).len();
    if !welded && crossed.is_empty() && deduped == 0 {
        return Vec::new();
    }
    let affected = lines_at_positions(map, &moved_pos);
    let mut newly: Vec<LineKey> = affected
        .iter()
        .copied()
        .filter(|&k| line_at_crossing(map, k, &crossings))
        .collect();
    // A drop-weld can close a sector-less loop; its whole wall network counts as new.
    for k in flood_sectorless(map, &affected) {
        if !newly.contains(&k) {
            newly.push(k);
        }
    }
    build_sectors(map, &newly, default_sector, VoidRule::KeepPockets);
    newly
}

/// Sector-less lines reachable from the sector-less members of `seeds` through shared vertices.
fn flood_sectorless(map: &EditorMap, seeds: &[LineKey]) -> Vec<LineKey> {
    let mut by_vert: HashMap<VertKey, Vec<LineKey>> = HashMap::new();
    for (k, l) in map.lines.iter() {
        if l.sides().all(|s| s.sector.is_none()) {
            by_vert.entry(l.v1).or_default().push(k);
            by_vert.entry(l.v2).or_default().push(k);
        }
    }
    let mut stack: Vec<LineKey> = seeds
        .iter()
        .copied()
        .filter(|&k| {
            map.lines
                .get(k)
                .is_some_and(|l| l.sides().all(|s| s.sector.is_none()))
        })
        .collect();
    let mut seen: HashSet<LineKey> = HashSet::new();
    let mut out: Vec<LineKey> = Vec::new();
    while let Some(k) = stack.pop() {
        if !seen.insert(k) {
            continue;
        }
        out.push(k);
        let l = &map.lines[k];
        for v in [l.v1, l.v2] {
            if let Some(near) = by_vert.get(&v) {
                stack.extend(near.iter().copied().filter(|n| !seen.contains(n)));
            }
        }
    }
    out.sort_unstable();
    out
}

/// Append a one-sided edge between two world points; returns its key. Reuse exact-match vertices, insert a line carrying `front`/`flags` (sector left void here — the caller's sector pass assigns it), then split anything it crosses. A degenerate (same-point) edge is skipped. `tol` is in world units.
pub fn add_edge(
    map: &mut EditorMap,
    a: [f32; 2],
    b: [f32; 2],
    front: SideDef,
    flags: LineFlags,
    tol: f32,
) -> Option<LineKey> {
    if a == b {
        return None;
    }
    let v1 = map.find_or_add_vertex(a);
    let v2 = map.find_or_add_vertex(b);
    let new_line = map.lines.insert(LineDef {
        v1,
        v2,
        flags,
        special: 0,
        tag: 0,
        front,
        back: None,
    });
    split_lines_at_intersections(map, &[new_line], tol);
    Some(new_line)
}

/// Re-sector a finished draw: `drawn` is every line the draw created (including split tails), expanded to every line sharing a vertex with one (a split wall's surviving head); the sector-less lines of that set are re-derived against `record`.
pub fn derive_sectors(map: &mut EditorMap, drawn: &[LineKey], record: Sector) {
    if drawn.is_empty() {
        return;
    }
    // Only sector-less lines are writable (a split tail copies its wall's sectors, so "no sector anywhere" = genuinely new); frozen walls are trace context only.
    let newly: Vec<LineKey> = expand_via_shared_vertices(map, drawn)
        .into_iter()
        .filter(|&k| {
            map.lines
                .get(k)
                .is_some_and(|l| l.sides().all(|s| s.sector.is_none()))
        })
        .collect();
    build_sectors(map, &newly, record, VoidRule::SectorDrawnLoops);
}

/// The `new` lines plus every line sharing a vertex with one.
fn expand_via_shared_vertices(map: &EditorMap, new: &[LineKey]) -> Vec<LineKey> {
    let mut touched: HashSet<VertKey> = HashSet::new();
    for &k in new {
        if let Some(l) = map.lines.get(k) {
            touched.insert(l.v1);
            touched.insert(l.v2);
        }
    }
    map.lines
        .iter()
        .filter(|(_, l)| touched.contains(&l.v1) || touched.contains(&l.v2))
        .map(|(k, _)| k)
        .collect()
}

/// Flip lines in place: swap each line's endpoints and front/back sides, so a two-sided line stays visually identical (winding fix) and a one-sided line reverses its facing.
pub fn flip_lines(map: &mut EditorMap, keys: &[LineKey]) {
    for &k in keys {
        flip_line(map, k);
    }
}

/// The shared vertex of lines `a` and `b` and each line's other (far) endpoint, if they share exactly one. `None` when they share no vertex, are the same line, or are coincident.
fn shared_and_far(map: &EditorMap, a: LineKey, b: LineKey) -> Option<(VertKey, VertKey, VertKey)> {
    if a == b {
        return None;
    }
    let la = map.lines.get(a)?;
    let lb = map.lines.get(b)?;
    let (a1, a2, b1, b2) = (la.v1, la.v2, lb.v1, lb.v2);
    let shared = if a1 == b1 || a1 == b2 {
        a1
    } else if a2 == b1 || a2 == b2 {
        a2
    } else {
        return None;
    };
    let far_a = if a1 == shared { a2 } else { a1 };
    let far_b = if b1 == shared { b2 } else { b1 };
    if far_a == far_b {
        return None;
    }
    Some((shared, far_a, far_b))
}

/// The deviation from straight (radians) of the chain far_a → shared → far_b: 0 when the two segments are perfectly collinear, π when fully doubled back.
fn chain_deviation(
    map: &EditorMap,
    shared: VertKey,
    far_a: VertKey,
    far_b: VertKey,
) -> Option<f32> {
    let s = map.vertices.get(shared)?;
    let pa = map.vertices.get(far_a)?;
    let pb = map.vertices.get(far_b)?;
    // Directions pointing away from the shared vertex along each line.
    let da = (pa.x - s.x, pa.y - s.y);
    let db = (pb.x - s.x, pb.y - s.y);
    let dot = da.0 * db.0 + da.1 * db.1;
    let (ma, mb) = ((da.0.hypot(da.1)), (db.0.hypot(db.1)));
    if ma == 0.0 || mb == 0.0 {
        return None;
    }
    let cos = (dot / (ma * mb)).clamp(-1.0, 1.0);
    // Straight = directions opposite (angle π); deviation = π − angle.
    Some(PI - cos.acos())
}

/// The chain `(shared, far_a, far_b)` when `a` and `b` can merge into one line: exactly one shared vertex with no third line attached (a merge would strand it as an unsplit T-junction), the same sector on each geometric side, and deviation from straight under `max_dev_rad`.
fn merge_candidate(
    map: &EditorMap,
    a: LineKey,
    b: LineKey,
    max_dev_rad: f32,
) -> Option<(VertKey, VertKey, VertKey)> {
    let candidate = chain_candidate(map, a, b, max_dev_rad)?;
    let degree = map
        .lines
        .values()
        .filter(|l| l.v1 == candidate.0 || l.v2 == candidate.0)
        .count();
    (degree == 2).then_some(candidate)
}

/// [`merge_candidate`] minus the shared-vertex degree check, for callers that already know it (adjacency-scanning gates).
fn chain_candidate(
    map: &EditorMap,
    a: LineKey,
    b: LineKey,
    max_dev_rad: f32,
) -> Option<(VertKey, VertKey, VertKey)> {
    let (shared, far_a, far_b) = shared_and_far(map, a, b)?;
    if !sides_compatible(&map.lines[a], &map.lines[b], shared) {
        return None;
    }
    if !chain_deviation(map, shared, far_a, far_b).is_some_and(|d| d < max_dev_rad) {
        return None;
    }
    Some((shared, far_a, far_b))
}

/// Whether two lines meeting at `shared` carry the same sector on each geometric side: chain-aligned windings match front↔front, opposed match front↔back.
fn sides_compatible(la: &LineDef, lb: &LineDef, shared: VertKey) -> bool {
    let aligned = (la.v2 == shared) == (lb.v1 == shared);
    let (a_front, a_back) = (Some(la.front.sector), la.back.map(|s| s.sector));
    let (b_front, b_back) = (Some(lb.front.sector), lb.back.map(|s| s.sector));
    if aligned {
        a_front == b_front && a_back == b_back
    } else {
        a_front == b_back && a_back == b_front
    }
}

/// Whether [`merge_collinear_lines`] would merge `a` and `b`.
pub fn can_merge_collinear(map: &EditorMap, a: LineKey, b: LineKey, max_dev_rad: f32) -> bool {
    merge_candidate(map, a, b, max_dev_rad).is_some()
}

/// Merge two near-collinear lines sharing a vertex into one: the chain far_a → shared → far_b becomes one line spanning the far vertices. Line `a` is reshaped to the span (keeping its sides/flags); line `b` and the now-orphan shared vertex are removed. Returns whether the merge happened.
pub fn merge_collinear_lines(
    map: &mut EditorMap,
    a: LineKey,
    b: LineKey,
    max_dev_rad: f32,
) -> bool {
    let Some((shared, _, far_b)) = merge_candidate(map, a, b, max_dev_rad) else {
        return false;
    };
    // Reshape `a` to span the far vertices, preserving its winding direction.
    let line = &mut map.lines[a];
    if line.v1 == shared {
        line.v1 = far_b;
    } else {
        line.v2 = far_b;
    }
    map.remove_lines(&[b]);
    true
}

/// Whether any vertex in `verts` would dissolve under [`dissolve_collinear_vertices`]; one line scan, the editor's menu gate.
pub fn any_dissolvable(map: &EditorMap, verts: &[VertKey], max_dev_rad: f32) -> bool {
    let wanted: HashSet<VertKey> = verts.iter().copied().collect();
    let mut incident: HashMap<VertKey, Vec<LineKey>> = HashMap::new();
    for (k, l) in map.lines.iter() {
        for v in [l.v1, l.v2] {
            if wanted.contains(&v) {
                incident.entry(v).or_default().push(k);
            }
        }
    }
    verts.iter().any(|v| {
        matches!(incident.get(v).map(Vec::as_slice),
            Some(&[a, b]) if chain_candidate(map, a, b, max_dev_rad).is_some())
    })
}

/// Dissolve each vertex in `verts` whose two incident lines pass [`merge_collinear_lines`]' guards, merging them; returns the count dissolved.
pub fn dissolve_collinear_vertices(
    map: &mut EditorMap,
    verts: &[VertKey],
    max_dev_rad: f32,
) -> usize {
    let mut dissolved = 0;
    for &v in verts {
        let incident: Vec<LineKey> = map
            .lines
            .iter()
            .filter(|(_, l)| l.v1 == v || l.v2 == v)
            .map(|(k, _)| k)
            .collect();
        if incident.len() == 2 && merge_collinear_lines(map, incident[0], incident[1], max_dev_rad)
        {
            dissolved += 1;
        }
    }
    dissolved
}

/// A trimmable corner: a degree-2 vertex whose side-compatible lines meet at a real angle.
struct Corner {
    line_a: LineKey,
    line_b: LineKey,
    v: VertKey,
    pos: [f32; 2],
    /// Unit direction from the corner toward `line_a`'s far endpoint.
    dir_a: [f32; 2],
    dir_b: [f32; 2],
    len_a: f32,
    len_b: f32,
}

/// The corner at `v`, or `None` when its lines cannot host a fillet/chamfer.
fn corner_at(map: &EditorMap, v: VertKey) -> Option<Corner> {
    let incident: Vec<LineKey> = map
        .lines
        .iter()
        .filter(|(_, l)| l.v1 == v || l.v2 == v)
        .map(|(k, _)| k)
        .collect();
    let [line_a, line_b] = incident[..] else {
        return None;
    };
    let (shared, far_a, far_b) = shared_and_far(map, line_a, line_b)?;
    if shared != v || !sides_compatible(&map.lines[line_a], &map.lines[line_b], v) {
        return None;
    }
    // A straight corner has nothing to round.
    if !chain_deviation(map, shared, far_a, far_b).is_some_and(|d| d >= CORNER_STRAIGHT_EPS) {
        return None;
    }
    let pos = map.vertices.get(v).map(|p| [p.x, p.y])?;
    let dir_len = |far: VertKey| {
        let f = map.vertices[far];
        let d = [f.x - pos[0], f.y - pos[1]];
        let len = d[0].hypot(d[1]);
        ([d[0] / len, d[1] / len], len)
    };
    let (dir_a, len_a) = dir_len(far_a);
    let (dir_b, len_b) = dir_len(far_b);
    Some(Corner {
        line_a,
        line_b,
        v,
        pos,
        dir_a,
        dir_b,
        len_a,
        len_b,
    })
}

/// Whether [`fillet_vertex`]/[`chamfer_vertex`] can act on `v`.
pub fn can_trim_corner(map: &EditorMap, v: VertKey) -> bool {
    corner_at(map, v).is_some()
}

/// Cut the corner at `v` with a straight line `dist` along each incident line (clamped to fit); returns the cut line.
pub fn chamfer_vertex(map: &mut EditorMap, v: VertKey, dist: f32) -> Option<LineKey> {
    let c = corner_at(map, v)?;
    let trim = dist
        .min(c.len_a * TRIM_MAX_FRAC)
        .min(c.len_b * TRIM_MAX_FRAC);
    if trim <= 0.0 {
        return None;
    }
    let pa = [c.pos[0] + c.dir_a[0] * trim, c.pos[1] + c.dir_a[1] * trim];
    let pb = [c.pos[0] + c.dir_b[0] * trim, c.pos[1] + c.dir_b[1] * trim];
    insert_corner_chain(map, &c, &[pa, pb]).first().copied()
}

/// Round the corner at `v` with an arc of `radius` approximated by `segments` chords; returns the new lines (empty when the tangent trim does not fit either line).
pub fn fillet_vertex(map: &mut EditorMap, v: VertKey, radius: f32, segments: u32) -> Vec<LineKey> {
    let Some(c) = corner_at(map, v) else {
        return Vec::new();
    };
    if radius <= 0.0 {
        return Vec::new();
    }
    let segments = segments.max(1);
    let cos = (c.dir_a[0] * c.dir_b[0] + c.dir_a[1] * c.dir_b[1]).clamp(-1.0, 1.0);
    let half = cos.acos() * 0.5;
    let trim = radius / half.tan();
    if trim > c.len_a * TRIM_MAX_FRAC || trim > c.len_b * TRIM_MAX_FRAC {
        return Vec::new();
    }
    let pa = [c.pos[0] + c.dir_a[0] * trim, c.pos[1] + c.dir_a[1] * trim];
    let pb = [c.pos[0] + c.dir_b[0] * trim, c.pos[1] + c.dir_b[1] * trim];
    let bis = [c.dir_a[0] + c.dir_b[0], c.dir_a[1] + c.dir_b[1]];
    let bis_len = bis[0].hypot(bis[1]);
    let to_centre = radius / half.sin();
    let centre = [
        c.pos[0] + bis[0] / bis_len * to_centre,
        c.pos[1] + bis[1] / bis_len * to_centre,
    ];
    let a0 = (pa[1] - centre[1]).atan2(pa[0] - centre[0]);
    let a1 = (pb[1] - centre[1]).atan2(pb[0] - centre[0]);
    // The arc subtends π − θ < π, so the short way round is always correct.
    let mut sweep = a1 - a0;
    if sweep > PI {
        sweep -= TAU;
    } else if sweep < -PI {
        sweep += TAU;
    }
    let mut pts: Vec<[f32; 2]> = (0..=segments)
        .map(|i| {
            let a = a0 + sweep * i as f32 / segments as f32;
            [centre[0] + radius * a.cos(), centre[1] + radius * a.sin()]
        })
        .collect();
    // Exact tangent points at the ends; interpolation only for the interior.
    pts[0] = pa;
    *pts.last_mut().expect("segments >= 1") = pb;
    insert_corner_chain(map, &c, &pts)
}

/// Replace the corner vertex with the polyline `pts` (ordered from `line_a`'s trim point to `line_b`'s): both lines reshape onto the end points, the interior chain copies the loop-order source line's sides/flags, and the orphaned corner vertex prunes.
fn insert_corner_chain(map: &mut EditorMap, c: &Corner, pts: &[[f32; 2]]) -> Vec<LineKey> {
    let verts: Vec<VertKey> = pts.iter().map(|&p| map.find_or_add_vertex(p)).collect();
    let (va, vb) = (verts[0], *verts.last().expect("two or more points"));
    let reshape = |map: &mut EditorMap, k: LineKey, to: VertKey| {
        let line = &mut map.lines[k];
        if line.v1 == c.v {
            line.v1 = to;
        } else {
            line.v2 = to;
        }
    };
    reshape(map, c.line_a, va);
    reshape(map, c.line_b, vb);
    // Chain direction follows loop order through the corner: the line flowing INTO `v` is the winding source.
    let (source, ordered): (LineKey, Vec<VertKey>) = if map.lines[c.line_a].v2 == va {
        (c.line_a, verts)
    } else if map.lines[c.line_b].v2 == vb {
        (c.line_b, verts.into_iter().rev().collect())
    } else {
        (c.line_a, verts.into_iter().rev().collect())
    };
    let template = map.lines[source];
    let mut new = Vec::with_capacity(ordered.len() - 1);
    for pair in ordered.windows(2) {
        new.push(map.lines.insert(LineDef {
            v1: pair[0],
            v2: pair[1],
            ..template
        }));
    }
    map.prune_orphan_vertices();
    dedup_coincident_lines(map);
    new
}

/// World axis selector for align/distribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
}

impl Axis {
    fn get(self, p: [f32; 2]) -> f32 {
        match self {
            Self::X => p[0],
            Self::Y => p[1],
        }
    }

    fn set(self, p: [f32; 2], v: f32) -> [f32; 2] {
        match self {
            Self::X => [v, p[1]],
            Self::Y => [p[0], v],
        }
    }
}

/// Moves setting every vertex's `axis` coordinate to the selection mean; commit via [`move_vertices`]. Empty when nothing would change.
pub fn align_vertices(map: &EditorMap, verts: &[VertKey], axis: Axis) -> Vec<(VertKey, [f32; 2])> {
    let pts: Vec<(VertKey, [f32; 2])> = vert_positions(map, verts);
    if pts.len() < 2 {
        return Vec::new();
    }
    let target = pts.iter().map(|(_, p)| axis.get(*p)).sum::<f32>() / pts.len() as f32;
    pts.into_iter()
        .filter(|(_, p)| axis.get(*p) != target)
        .map(|(k, p)| (k, axis.set(p, target)))
        .collect()
}

/// Moves spacing the vertices evenly along `axis` between the selection extremes (which stay put); commit via [`move_vertices`].
pub fn distribute_vertices(
    map: &EditorMap,
    verts: &[VertKey],
    axis: Axis,
) -> Vec<(VertKey, [f32; 2])> {
    let mut pts = vert_positions(map, verts);
    if pts.len() < 3 {
        return Vec::new();
    }
    pts.sort_by(|(_, a), (_, b)| axis.get(*a).total_cmp(&axis.get(*b)));
    let (lo, hi) = (
        axis.get(pts[0].1),
        axis.get(pts.last().expect("three or more").1),
    );
    let step = (hi - lo) / (pts.len() - 1) as f32;
    pts.iter()
        .enumerate()
        .filter_map(|(i, &(k, p))| {
            let target = lo + step * i as f32;
            (axis.get(p) != target).then_some((k, axis.set(p, target)))
        })
        .collect()
}

/// Moves projecting a connected degree-≤2 chain's interior vertices onto the segment between its two endpoints; empty when the set is not such a chain (branch, cycle, or disconnected).
pub fn straighten_chain(map: &EditorMap, verts: &[VertKey]) -> Vec<(VertKey, [f32; 2])> {
    let set: HashSet<VertKey> = verts.iter().copied().collect();
    if set.len() < 3 {
        return Vec::new();
    }
    // Adjacency restricted to lines whose both endpoints are selected.
    let mut adj: HashMap<VertKey, Vec<VertKey>> = HashMap::new();
    for l in map.lines.values() {
        if set.contains(&l.v1) && set.contains(&l.v2) {
            adj.entry(l.v1).or_default().push(l.v2);
            adj.entry(l.v2).or_default().push(l.v1);
        }
    }
    let mut ends: Vec<VertKey> = set
        .iter()
        .copied()
        .filter(|v| adj.get(v).map_or(0, Vec::len) == 1)
        .collect();
    if ends.len() != 2 || set.iter().any(|v| adj.get(v).map_or(0, Vec::len) > 2) {
        return Vec::new();
    }
    ends.sort_unstable();
    // Walk end to end; failing to visit every vertex means a disconnected set.
    let mut order = vec![ends[0]];
    let mut prev = ends[0];
    while let Some(&next) = adj[order.last().expect("seeded")]
        .iter()
        .find(|&&n| n != prev)
    {
        prev = *order.last().expect("seeded");
        order.push(next);
        if next == ends[1] {
            break;
        }
    }
    if order.len() != set.len() {
        return Vec::new();
    }
    let a = map.vertices[ends[0]];
    let b = map.vertices[ends[1]];
    order[1..order.len() - 1]
        .iter()
        .filter_map(|&k| {
            let p = map.vertices.get(k).map(|v| [v.x, v.y])?;
            let proj = nearest_point_on_segment(p, [a.x, a.y], [b.x, b.y]);
            (proj != p).then_some((k, proj))
        })
        .collect()
}

/// Moves scaling `verts` about `pivot` (per-axis; negative mirrors) then rotating by `rot_rad` (CCW, Y-up); commit via [`move_vertices`]. A negative-determinant scale mirrors — the caller must flip the fully-contained lines afterwards so front/back stay on the correct geometric sides.
pub fn transform_moves(
    map: &EditorMap,
    verts: &[VertKey],
    pivot: [f32; 2],
    rot_rad: f32,
    scale: [f32; 2],
) -> Vec<(VertKey, [f32; 2])> {
    let (s, c) = rot_rad.sin_cos();
    vert_positions(map, verts)
        .into_iter()
        .filter_map(|(k, p)| {
            let d = [(p[0] - pivot[0]) * scale[0], (p[1] - pivot[1]) * scale[1]];
            let q = [
                pivot[0] + d[0] * c - d[1] * s,
                pivot[1] + d[0] * s + d[1] * c,
            ];
            (q != p).then_some((k, q))
        })
        .collect()
}

/// Re-seat sides after an improper (mirroring) transform of `lines`' endpoints: the mirrored front faces what the back faced, so two-sided lines swap sides in place and one-sided lines reverse direction instead (front must stay populated). A full flip would be a geometric no-op.
pub fn mirror_fixup(map: &mut EditorMap, lines: &[LineKey]) {
    for &k in lines {
        let Some(line) = map.lines.get_mut(k) else {
            continue;
        };
        if let Some(back) = line.back.take() {
            line.back = Some(mem::replace(&mut line.front, back));
        } else {
            mem::swap(&mut line.v1, &mut line.v2);
        }
    }
}

/// Live `(key, position)` pairs for `verts`.
fn vert_positions(map: &EditorMap, verts: &[VertKey]) -> Vec<(VertKey, [f32; 2])> {
    verts
        .iter()
        .filter_map(|&k| map.vertices.get(k).map(|v| (k, [v.x, v.y])))
        .collect()
}

/// Extrude `line` by `delta` into a quad: three new edges (copying the line's front textures/flags, sector void) close a loop with the original, then the draw re-sector pass assigns `default_sector` and promotes the original wall to two-sided. Returns the new lines (empty when `delta` is zero or parallel to the line). `tol` is in world units.
pub fn extrude_line(
    map: &mut EditorMap,
    line: LineKey,
    delta: [f32; 2],
    tol: f32,
    default_sector: Sector,
) -> Vec<LineKey> {
    let Some(l) = map.lines.get(line).copied() else {
        return Vec::new();
    };
    let (p1, p2) = segment_points(map, line);
    let along = [p2[0] - p1[0], p2[1] - p1[1]];
    let cross = delta[0] * along[1] - delta[1] * along[0];
    let scale = delta[0].hypot(delta[1]) * along[0].hypot(along[1]);
    if scale <= 0.0 || cross.abs() / scale < EXTRUDE_PARALLEL_EPS {
        return Vec::new();
    }
    let q1 = [p1[0] + delta[0], p1[1] + delta[1]];
    let q2 = [p2[0] + delta[0], p2[1] + delta[1]];
    let side = SideDef {
        sector: None,
        ..l.front
    };
    let mut new = Vec::new();
    for (a, b) in [(p1, q1), (q1, q2), (q2, p2)] {
        if let Some(k) = add_edge(map, a, b, side, l.flags, tol) {
            new.push(k);
        }
    }
    derive_sectors(map, &new, default_sector);
    new.retain(|&k| map.lines.contains(k));
    new
}

/// Whether sectors `a` and `b` are adjacent — joined by at least one two-sided line whose two sides face one and the other.
pub fn sectors_share_two_sided_wall(map: &EditorMap, a: SectorKey, b: SectorKey) -> bool {
    map.lines.values().any(|l| {
        let Some(back) = l.back else {
            return false;
        };
        let (f, bk) = (l.front.sector, back.sector);
        (f == Some(a) && bk == Some(b)) || (f == Some(b) && bk == Some(a))
    })
}

/// Delete a sector: its shared two-sided walls become single-sided facing the neighbour; its outer single-sided walls become void.
pub fn delete_sector(map: &mut EditorMap, sector: SectorKey) {
    for line in map.lines.values_mut() {
        let front_is = line.front.sector == Some(sector);
        let back_is = line.back.is_some_and(|b| b.sector == Some(sector));
        match (front_is, back_is, line.back) {
            (true, false, Some(back)) => {
                line.front = back;
                line.back = None;
                line.flags.remove(LineFlags::TWO_SIDED);
            }
            (false, true, Some(_)) => {
                line.back = None;
                line.flags.remove(LineFlags::TWO_SIDED);
            }
            (true, false, None) => line.front.sector = None,
            _ => {}
        }
    }
    map.prune_unused_sectors();
}

/// Merge each pair of sectors joined by a just-deleted two-sided wall: the lowest key (slot order) of each connected group survives, every other member's sides reassign to it, and emptied records prune. Union-find over the pairs so chained deletes (a|b and b|c) collapse to one survivor.
pub fn merge_sectors(map: &mut EditorMap, pairs: &[(SectorKey, SectorKey)]) {
    if pairs.is_empty() {
        return;
    }
    let mut parent: HashMap<SectorKey, SectorKey> = HashMap::new();
    for &(a, b) in pairs {
        let (ra, rb) = (find(&parent, a), find(&parent, b));
        if ra != rb {
            let (keep, drop) = (ra.min(rb), ra.max(rb));
            parent.insert(drop, keep);
        }
    }
    let reassign = |s: &mut Option<SectorKey>, parent: &mut HashMap<SectorKey, SectorKey>| {
        if let Some(k) = s {
            *s = Some(find(parent, *k));
        }
    };
    let keys: Vec<LineKey> = map.lines.keys().collect();
    for k in keys {
        let line = &mut map.lines[k];
        let mut front = line.front.sector;
        reassign(&mut front, &mut parent);
        line.front.sector = front;
        if let Some(back) = &mut line.back {
            let mut b = back.sector;
            reassign(&mut b, &mut parent);
            back.sector = b;
        }
    }
    map.prune_unused_sectors();
}

fn find(parent: &HashMap<SectorKey, SectorKey>, mut x: SectorKey) -> SectorKey {
    while let Some(&p) = parent.get(&x) {
        if p == x {
            break;
        }
        x = p;
    }
    x
}

/// Build a self-contained fragment (its own [`EditorMap`]) from a selection: referenced vertices are copied and deduplicated, line references remap to fragment-local keys, and each faced sector is inlined into the fragment. Paste with [`paste_fragment`].
pub fn extract_fragment(
    map: &EditorMap,
    line_ids: &[LineKey],
    thing_ids: &[ThingKey],
) -> EditorMap {
    let mut frag = EditorMap::default();
    let mut vmap: HashMap<VertKey, VertKey> = HashMap::new();
    let mut smap: HashMap<SectorKey, SectorKey> = HashMap::new();
    let mut local_vertex = |frag: &mut EditorMap, src: VertKey| -> VertKey {
        *vmap
            .entry(src)
            .or_insert_with(|| frag.vertices.insert(map.vertices[src]))
    };
    let mut local_side = |frag: &mut EditorMap, side: &SideDef| -> SideDef {
        let sector = side.sector.map(|s| {
            *smap
                .entry(s)
                .or_insert_with(|| frag.sectors.insert(map.sectors[s]))
        });
        SideDef {
            sector,
            ..*side
        }
    };
    for &k in line_ids {
        let Some(line) = map.lines.get(k).copied() else {
            continue;
        };
        let v1 = local_vertex(&mut frag, line.v1);
        let v2 = local_vertex(&mut frag, line.v2);
        let front = local_side(&mut frag, &line.front);
        let back = line.back.as_ref().map(|s| local_side(&mut frag, s));
        frag.lines.insert(LineDef {
            v1,
            v2,
            front,
            back,
            ..line
        });
    }
    for &k in thing_ids {
        if let Some(t) = map.things.get(k) {
            frag.things.insert(*t);
        }
    }
    frag
}

/// Append `fragment` to `map` offset by `delta`: its vertices reuse exact matches, its sectors insert once each, and its lines/things copy over with remapped references. Returns the new line and thing keys (in fragment order) for the caller to select.
pub fn paste_fragment(
    map: &mut EditorMap,
    fragment: &EditorMap,
    delta: [f32; 2],
) -> (Vec<LineKey>, Vec<ThingKey>) {
    let verts: HashMap<VertKey, VertKey> = fragment
        .vertices
        .iter()
        .map(|(k, v)| (k, map.find_or_add_vertex([v.x + delta[0], v.y + delta[1]])))
        .collect();
    let sectors: HashMap<SectorKey, SectorKey> = fragment
        .sectors
        .iter()
        .map(|(k, s)| (k, map.sectors.insert(*s)))
        .collect();
    let remap_side = |side: &SideDef| SideDef {
        sector: side.sector.map(|s| sectors[&s]),
        ..*side
    };
    let mut new_lines = Vec::with_capacity(fragment.lines.len());
    for line in fragment.lines.values() {
        new_lines.push(map.lines.insert(LineDef {
            v1: verts[&line.v1],
            v2: verts[&line.v2],
            front: remap_side(&line.front),
            back: line.back.as_ref().map(remap_side),
            ..*line
        }));
    }
    let mut new_things = Vec::with_capacity(fragment.things.len());
    for t in fragment.things.values() {
        new_things.push(map.things.insert(Thing {
            x: t.x + delta[0] as i32,
            y: t.y + delta[1] as i32,
            ..*t
        }));
    }
    (new_lines, new_things)
}

/// The min corner of a fragment's geometry; paste offsets it to the drop point.
pub fn fragment_min_corner(fragment: &EditorMap) -> [f32; 2] {
    let mut min = [f32::MAX, f32::MAX];
    for v in fragment.vertices.values() {
        min[0] = min[0].min(v.x);
        min[1] = min[1].min(v.y);
    }
    for t in fragment.things.values() {
        min[0] = min[0].min(t.x as f32);
        min[1] = min[1].min(t.y as f32);
    }
    if min[0] == f32::MAX { [0.0, 0.0] } else { min }
}

/// The four corners of a corner-to-corner rectangle, wound CCW in Y-up; `a` and `b` are opposite corners.
pub fn rect_corners(a: [f32; 2], b: [f32; 2]) -> [[f32; 2]; 4] {
    let (x0, x1) = (a[0].min(b[0]), a[0].max(b[0]));
    let (y0, y1) = (a[1].min(b[1]), a[1].max(b[1]));
    [[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
}

/// Vertices of a regular `sides`-gon centred at `center`, radius/rotation taken from `pointer` (distance = radius, angle = rotation); the first vertex points at `pointer`.
pub fn ngon_points(center: [f32; 2], pointer: [f32; 2], sides: u32) -> Vec<[f32; 2]> {
    let (dx, dy) = (pointer[0] - center[0], pointer[1] - center[1]);
    let radius = (dx * dx + dy * dy).sqrt();
    let rot = dy.atan2(dx);
    let step = TAU / sides as f32;
    (0..sides)
        .map(|k| {
            let a = rot + step * k as f32;
            [center[0] + radius * a.cos(), center[1] + radius * a.sin()]
        })
        .collect()
}

/// Mean of a set of points; the iterator must be non-empty.
fn mean(pts: impl Iterator<Item = [f32; 2]>) -> [f32; 2] {
    let mut n = 0.0f32;
    let sum = pts.fold([0.0, 0.0], |a, p| {
        n += 1.0;
        [a[0] + p[0], a[1] + p[1]]
    });
    [sum[0] / n, sum[1] / n]
}

fn dist_sq(a: [f32; 2], b: [f32; 2]) -> f32 {
    let d = [a[0] - b[0], a[1] - b[1]];
    d[0] * d[0] + d[1] * d[1]
}

/// Lines to re-sector after a move: those touching a moved-vertex position, plus every line sharing a vertex with one (a split wall's surviving head). Scoped by position because the merge/dedup before this can remove the moved keys.
fn lines_at_positions(map: &EditorMap, positions: &[[f32; 2]]) -> Vec<LineKey> {
    let at_moved = |x: f32, y: f32| positions.iter().any(|p| p[0] == x && p[1] == y);
    let mut touched: HashSet<VertKey> = HashSet::new();
    for l in map.lines.values() {
        if let (Some(p1), Some(p2)) = (map.vertices.get(l.v1), map.vertices.get(l.v2))
            && (at_moved(p1.x, p1.y) || at_moved(p2.x, p2.y))
        {
            touched.insert(l.v1);
            touched.insert(l.v2);
        }
    }
    map.lines
        .iter()
        .filter(|(_, l)| touched.contains(&l.v1) || touched.contains(&l.v2))
        .map(|(k, _)| k)
        .collect()
}

/// True when an endpoint of `line` is a split crossing point — the move cut this line, so the re-sector may treat it as new geometry.
fn line_at_crossing(map: &EditorMap, line: LineKey, crossings: &[[f32; 2]]) -> bool {
    let Some(l) = map.lines.get(line) else {
        return false;
    };
    [l.v1, l.v2].iter().any(|&v| {
        map.vertices
            .get(v)
            .is_some_and(|p| crossings.iter().any(|c| c[0] == p.x && c[1] == p.y))
    })
}

#[cfg(test)]
mod tests {
    use std::f32::consts::FRAC_PI_4;

    use super::*;
    use crate::geom::sector_at;
    use crate::model::DenseLineDef;
    use crate::name8::Name8;
    use crate::test_fixtures::{
        def_sector, dline_with, dside, fixture, line_keys, sector_keys, vert_keys, vtx,
    };

    fn dline(v1: u32, v2: u32) -> DenseLineDef {
        dline_with(v1, v2, LineFlags::empty(), None)
    }

    #[test]
    fn weld_cluster_collapses_near_corners() {
        // Triangle; weld the two near base corners to their centroid.
        let mut map = fixture(
            vec![vtx(0.0, 0.0), vtx(4.0, 0.0), vtx(2.0, 40.0)],
            vec![dline(0, 1), dline(1, 2), dline(2, 0)],
            0,
        );
        let v = vert_keys(&map);
        assert!(weld_cluster(&mut map, &[v[0], v[1]], 8.0, def_sector()));
        assert_eq!(map.vertices.len(), 2);
        assert!(
            map.vertices.values().any(|p| (p.x, p.y) == (2.0, 0.0)),
            "welded onto the corner centroid"
        );
        assert!(map.lines.len() < 3, "base collapsed");
    }

    #[test]
    fn weld_closing_loop_creates_sector() {
        // Open square: the last edge stops 4 units short of the first corner.
        let mut map = fixture(
            vec![
                vtx(0.0, 0.0),
                vtx(100.0, 0.0),
                vtx(100.0, 100.0),
                vtx(0.0, 100.0),
                vtx(0.0, 4.0),
            ],
            vec![dline(0, 1), dline(1, 2), dline(2, 3), dline(3, 4)],
            0,
        );
        let v = vert_keys(&map);
        assert!(weld_cluster(&mut map, &[v[0], v[4]], 8.0, def_sector()));
        assert_eq!(map.sectors.len(), 1, "closed loop gains a sector");
        assert_eq!(map.lines.len(), 4);
        let s = sector_keys(&map)[0];
        for (_, l) in map.lines.iter() {
            assert_eq!(l.front.sector, Some(s), "every loop side faces the room");
        }
    }

    #[test]
    fn weld_joining_open_chain_stays_void() {
        // Two disjoint segments; welding their near endpoints leaves an open L.
        let mut map = fixture(
            vec![
                vtx(0.0, 0.0),
                vtx(100.0, 0.0),
                vtx(0.0, 4.0),
                vtx(0.0, 100.0),
            ],
            vec![dline(0, 1), dline(2, 3)],
            0,
        );
        let v = vert_keys(&map);
        assert!(weld_cluster(&mut map, &[v[0], v[2]], 8.0, def_sector()));
        assert!(map.sectors.is_empty(), "open chain stays void");
        assert!(
            map.lines.iter().all(|(_, l)| l.front.sector.is_none()),
            "no side gains a sector"
        );
    }

    #[test]
    fn move_weld_closing_loop_creates_sector() {
        // Same open square, closed by dragging the loose endpoint onto the corner.
        let mut map = fixture(
            vec![
                vtx(0.0, 0.0),
                vtx(100.0, 0.0),
                vtx(100.0, 100.0),
                vtx(0.0, 100.0),
                vtx(0.0, 4.0),
            ],
            vec![dline(0, 1), dline(1, 2), dline(2, 3), dline(3, 4)],
            0,
        );
        let v = vert_keys(&map);
        move_vertices(&mut map, &[(v[4], [0.0, 0.0])], &[], 2.0, def_sector());
        assert_eq!(map.sectors.len(), 1, "closed loop gains a sector");
        assert_eq!(map.lines.len(), 4);
    }

    #[test]
    fn weld_cluster_skips_far_apart() {
        let mut map = fixture(vec![vtx(0.0, 0.0), vtx(100.0, 0.0)], vec![dline(0, 1)], 0);
        let v = vert_keys(&map);
        assert!(
            !weld_cluster(&mut map, &[v[0], v[1]], 8.0, def_sector()),
            "both outside the weld radius"
        );
        assert_eq!(map.vertices.len(), 2);
    }

    #[test]
    fn move_vertices_plain_nudge_does_not_resector() {
        // A lone line, no crossing/weld/dedup: a nudge changes positions only.
        let mut map = fixture(vec![vtx(0.0, 0.0), vtx(64.0, 0.0)], vec![dline(0, 1)], 0);
        let v = vert_keys(&map);
        move_vertices(&mut map, &[(v[0], [8.0, 8.0])], &[], 2.0, def_sector());
        assert!(map.sectors.is_empty(), "plain nudge leaves sectoring alone");
        let moved = map.vertices[v[0]];
        assert_eq!((moved.x, moved.y), (8.0, 8.0));
    }

    #[test]
    fn move_vertices_weld_on_drop_collapses_line() {
        // Chain a-b-c; move b onto c (within tol). Line b-c collapses.
        let mut map = fixture(
            vec![vtx(0.0, 0.0), vtx(40.0, 1.0), vtx(40.0, 0.0)],
            vec![dline(0, 1), dline(1, 2)],
            0,
        );
        let v = vert_keys(&map);
        move_vertices(&mut map, &[(v[1], [40.0, 0.0])], &[], 2.0, def_sector());
        assert_eq!(map.lines.len(), 1, "collapsed line removed");
    }

    #[test]
    fn extract_then_paste_round_trips_geometry() {
        // Two lines sharing a vertex, one faced sector. Extract both, paste at a delta: the fragment self-contains its verts/sector, paste re-appends.
        let mut map = fixture(
            vec![vtx(0.0, 0.0), vtx(8.0, 0.0), vtx(8.0, 8.0)],
            vec![
                {
                    let mut l = dline(0, 1);
                    l.front = dside(Some(0));
                    l
                },
                dline(1, 2),
            ],
            1,
        );
        let keys = line_keys(&map);
        let frag = extract_fragment(&map, &keys, &[]);
        assert_eq!(frag.vertices.len(), 3, "shared vertex copied once");
        assert_eq!(frag.lines.len(), 2);
        assert_eq!(frag.sectors.len(), 1, "faced sector inlined");
        let frag_sector = frag.sectors.keys().next();
        assert_eq!(
            frag.lines.values().next().expect("line").front.sector,
            frag_sector,
            "remapped to fragment"
        );

        let (lines, _things) = paste_fragment(&mut map, &frag, [100.0, 0.0]);
        assert_eq!(lines.len(), 2);
        assert_eq!(map.lines.len(), 4, "two pasted lines appended");
        let pasted = map.lines[lines[0]];
        let v1 = map.vertices[pasted.v1];
        assert_eq!((v1.x, v1.y), (100.0, 0.0), "offset by delta");
        assert_eq!(map.sectors.len(), 2, "fragment sector appended once");
    }

    #[test]
    fn flip_lines_swaps_endpoints_and_sides() {
        let mut map = fixture(vec![vtx(0.0, 0.0), vtx(4.0, 0.0)], vec![dline(0, 1)], 0);
        let k = line_keys(&map)[0];
        let (v1, v2) = (map.lines[k].v1, map.lines[k].v2);
        map.lines[k].back = Some(map.lines[k].front);
        flip_lines(&mut map, &[k]);
        assert_eq!(
            (map.lines[k].v1, map.lines[k].v2),
            (v2, v1),
            "endpoints swapped"
        );
    }

    #[test]
    fn merge_sectors_unifies_to_lowest_key() {
        // Lines facing sectors 0,1,2; merge (0,1) and (1,2) -> all become the first.
        let mut map = fixture(
            vec![vtx(0.0, 0.0), vtx(4.0, 0.0)],
            vec![
                {
                    let mut l = dline(0, 1);
                    l.front = dside(Some(0));
                    l
                },
                {
                    let mut l = dline(0, 1);
                    l.front = dside(Some(1));
                    l
                },
                {
                    let mut l = dline(0, 1);
                    l.front = dside(Some(2));
                    l
                },
            ],
            3,
        );
        let s = sector_keys(&map);
        merge_sectors(&mut map, &[(s[0], s[1]), (s[1], s[2])]);
        assert_eq!(map.sectors.len(), 1, "three sectors merged to one");
        for l in map.lines.values() {
            assert_eq!(l.front.sector, Some(s[0]));
        }
    }

    #[test]
    fn add_edge_splits_crossing_line() {
        // A horizontal line; add a vertical edge crossing it -> a split vertex.
        let mut map = fixture(vec![vtx(0.0, 0.0), vtx(8.0, 0.0)], vec![dline(0, 1)], 0);
        let side = SideDef {
            x_offset: 0,
            y_offset: 0,
            top_tex: Name8::EMPTY,
            bottom_tex: Name8::EMPTY,
            middle_tex: Name8::EMPTY,
            sector: None,
        };
        let added = add_edge(
            &mut map,
            [4.0, -4.0],
            [4.0, 4.0],
            side,
            LineFlags::BLOCKING,
            0.1,
        );
        assert!(added.is_some());
        assert!(map.lines.len() > 2, "the crossed line was split");
    }

    #[test]
    fn merge_collinear_lines_spans_far_vertices() {
        // Straight chain a(0,0)-b(4,0)-c(8,0): merging the two lines spans a-c.
        let mut map = fixture(
            vec![vtx(0.0, 0.0), vtx(4.0, 0.0), vtx(8.0, 0.0)],
            vec![dline(0, 1), dline(1, 2)],
            0,
        );
        let k = line_keys(&map);
        let tol = FRAC_PI_4;
        assert!(can_merge_collinear(&map, k[0], k[1], tol));
        assert!(merge_collinear_lines(&mut map, k[0], k[1], tol));
        assert_eq!(map.lines.len(), 1, "two lines became one");
        assert_eq!(map.vertices.len(), 2, "shared vertex pruned");
        let l = map.lines[k[0]];
        let mut xs = [map.vertices[l.v1].x, map.vertices[l.v2].x];
        xs.sort_by(|a, b| a.total_cmp(b));
        assert_eq!(xs, [0.0, 8.0], "spans the far vertices");
    }

    #[test]
    fn merge_collinear_lines_rejects_sharp_angle() {
        // Right-angle chain (90° deviation from straight): not mergeable at 45°.
        let mut map = fixture(
            vec![vtx(0.0, 0.0), vtx(4.0, 0.0), vtx(4.0, 4.0)],
            vec![dline(0, 1), dline(1, 2)],
            0,
        );
        let k = line_keys(&map);
        let tol = FRAC_PI_4;
        assert!(!can_merge_collinear(&map, k[0], k[1], tol));
        assert!(!merge_collinear_lines(&mut map, k[0], k[1], tol));
        assert_eq!(map.lines.len(), 2, "unchanged");
    }

    #[test]
    fn merge_collinear_lines_rejects_t_junction() {
        // Straight chain with a stem at the shared vertex: merging would strand the stem endpoint on the merged line's interior.
        let mut map = fixture(
            vec![vtx(0.0, 0.0), vtx(4.0, 0.0), vtx(8.0, 0.0), vtx(4.0, 4.0)],
            vec![dline(0, 1), dline(1, 2), dline(1, 3)],
            0,
        );
        let k = line_keys(&map);
        assert!(!can_merge_collinear(&map, k[0], k[1], FRAC_PI_4));
        assert!(!merge_collinear_lines(&mut map, k[0], k[1], FRAC_PI_4));
        assert_eq!(map.lines.len(), 3, "unchanged");
    }

    #[test]
    fn merge_collinear_lines_rejects_sector_mismatch() {
        // Straight chain but the lines face different sectors: merging would misassign one span.
        let mut map = fixture(
            vec![vtx(0.0, 0.0), vtx(4.0, 0.0), vtx(8.0, 0.0)],
            vec![
                {
                    let mut l = dline(0, 1);
                    l.front = dside(Some(0));
                    l
                },
                {
                    let mut l = dline(1, 2);
                    l.front = dside(Some(1));
                    l
                },
            ],
            2,
        );
        let k = line_keys(&map);
        assert!(!can_merge_collinear(&map, k[0], k[1], FRAC_PI_4));
        assert!(!merge_collinear_lines(&mut map, k[0], k[1], FRAC_PI_4));
    }

    #[test]
    fn merge_collinear_lines_opposed_winding_compares_geometric_sides() {
        // `b` wound against the chain: its front faces the opposite geometric side of `a`'s front, so equal front sectors still mismatch.
        let mut map = fixture(
            vec![vtx(0.0, 0.0), vtx(4.0, 0.0), vtx(8.0, 0.0)],
            vec![
                {
                    let mut l = dline(0, 1);
                    l.front = dside(Some(0));
                    l
                },
                {
                    let mut l = dline(2, 1);
                    l.front = dside(Some(0));
                    l
                },
            ],
            1,
        );
        let k = line_keys(&map);
        assert!(!merge_collinear_lines(&mut map, k[0], k[1], FRAC_PI_4));
    }

    #[test]
    fn merge_collinear_lines_accepts_opposed_winding_mirrored_sides() {
        // Opposed winding with mirrored two-sided sectors is geometrically identical: mergeable.
        let mut map = fixture(
            vec![vtx(0.0, 0.0), vtx(4.0, 0.0), vtx(8.0, 0.0)],
            vec![
                {
                    let mut l = dline(0, 1);
                    l.front = dside(Some(0));
                    l.back = Some(dside(Some(1)));
                    l
                },
                {
                    let mut l = dline(2, 1);
                    l.front = dside(Some(1));
                    l.back = Some(dside(Some(0)));
                    l
                },
            ],
            2,
        );
        let k = line_keys(&map);
        assert!(merge_collinear_lines(&mut map, k[0], k[1], FRAC_PI_4));
        assert_eq!(map.lines.len(), 1);
    }

    #[test]
    fn any_dissolvable_matches_dissolve_outcome() {
        // Straight chain: interior vertex dissolvable; endpoint-only selection is not.
        let map = fixture(
            vec![vtx(0.0, 0.0), vtx(4.0, 0.0), vtx(8.0, 0.0)],
            vec![dline(0, 1), dline(1, 2)],
            0,
        );
        let v = vert_keys(&map);
        assert!(any_dissolvable(&map, &v, FRAC_PI_4));
        assert!(
            !any_dissolvable(&map, &[v[0], v[2]], FRAC_PI_4),
            "endpoints"
        );

        // A junction vertex never dissolves.
        let map = fixture(
            vec![
                vtx(0.0, 0.0),
                vtx(4.0, 0.0),
                vtx(8.0, 0.0),
                vtx(4.0, 4.0),
                vtx(4.0, -4.0),
            ],
            vec![dline(0, 1), dline(1, 2), dline(1, 3), dline(1, 4)],
            0,
        );
        let v = vert_keys(&map);
        assert!(!any_dissolvable(&map, &v, FRAC_PI_4), "junction only");

        // A right-angle corner never dissolves at 45° tolerance.
        let map = fixture(
            vec![vtx(0.0, 0.0), vtx(4.0, 0.0), vtx(4.0, 4.0)],
            vec![dline(0, 1), dline(1, 2)],
            0,
        );
        let v = vert_keys(&map);
        assert!(!any_dissolvable(&map, &v, FRAC_PI_4), "right-angle corner");
    }

    #[test]
    fn dissolve_collinear_vertices_collapses_chain() {
        // Four collinear segments: dissolving the three interior vertices leaves one line.
        let mut map = fixture(
            vec![
                vtx(0.0, 0.0),
                vtx(2.0, 0.0),
                vtx(4.0, 0.0),
                vtx(6.0, 0.0),
                vtx(8.0, 0.0),
            ],
            vec![dline(0, 1), dline(1, 2), dline(2, 3), dline(3, 4)],
            0,
        );
        let v = vert_keys(&map);
        let n = dissolve_collinear_vertices(&mut map, &[v[1], v[2], v[3]], FRAC_PI_4);
        assert_eq!(n, 3);
        assert_eq!(map.lines.len(), 1);
        assert_eq!(map.vertices.len(), 2);
    }

    #[test]
    fn dissolve_collinear_vertices_skips_corner_and_junction() {
        // An L corner and a 3-line junction both survive a dissolve pass.
        let mut map = fixture(
            vec![
                vtx(0.0, 0.0),
                vtx(4.0, 0.0),
                vtx(4.0, 4.0),
                vtx(8.0, 0.0),
                vtx(4.0, -4.0),
            ],
            vec![dline(0, 1), dline(1, 2), dline(1, 3), dline(1, 4)],
            0,
        );
        let v = vert_keys(&map);
        assert_eq!(dissolve_collinear_vertices(&mut map, &v, FRAC_PI_4), 0);
        assert_eq!(map.lines.len(), 4, "unchanged");
    }

    /// 64-unit square run through the real sector builder; returns the map and its sector.
    fn sectored_square() -> (EditorMap, SectorKey) {
        let mut map = fixture(
            vec![
                vtx(0.0, 0.0),
                vtx(64.0, 0.0),
                vtx(64.0, 64.0),
                vtx(0.0, 64.0),
            ],
            vec![dline(0, 1), dline(1, 2), dline(2, 3), dline(3, 0)],
            0,
        );
        let keys = line_keys(&map);
        build_sectors(&mut map, &keys, def_sector(), VoidRule::KeepPockets);
        let s = sector_keys(&map)[0];
        (map, s)
    }

    /// Every vertex is v1 of exactly one line and v2 of exactly one — the loop stayed a loop.
    fn assert_single_loop(map: &EditorMap) {
        for (v, _) in map.vertices.iter() {
            let outs = map.lines.values().filter(|l| l.v1 == v).count();
            let ins = map.lines.values().filter(|l| l.v2 == v).count();
            assert_eq!((outs, ins), (1, 1), "loop winding broken at a vertex");
        }
    }

    #[test]
    fn chamfer_right_angle_inserts_cut_line() {
        let (mut map, s) = sectored_square();
        let v = vert_keys(&map);
        assert!(can_trim_corner(&map, v[1]));
        let cut = chamfer_vertex(&mut map, v[1], 16.0).expect("chamfer applies");
        assert_eq!(map.lines.len(), 5);
        assert!(!map.vertices.contains(v[1]), "corner vertex pruned");
        let (p1, p2) = (
            map.vertices[map.lines[cut].v1],
            map.vertices[map.lines[cut].v2],
        );
        let mut ends = [(p1.x, p1.y), (p2.x, p2.y)];
        ends.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
        assert_eq!(ends, [(48.0, 0.0), (64.0, 16.0)]);
        assert_eq!(map.lines[cut].front.sector, Some(s), "sides copied");
        assert_single_loop(&map);
        assert_eq!(
            sector_at(&map, [32.0, 32.0]),
            Some(s),
            "sector still resolves"
        );
    }

    #[test]
    fn chamfer_clamps_oversized_distance() {
        let (mut map, _) = sectored_square();
        let v = vert_keys(&map);
        assert!(chamfer_vertex(&mut map, v[1], 1000.0).is_some());
        assert_eq!(map.lines.len(), 5);
        assert_single_loop(&map);
    }

    #[test]
    fn fillet_right_angle_chords_lie_on_arc() {
        let (mut map, s) = sectored_square();
        let v = vert_keys(&map);
        let new = fillet_vertex(&mut map, v[1], 16.0, 4);
        assert_eq!(new.len(), 4, "one line per segment");
        assert_eq!(map.lines.len(), 8);
        assert!(!map.vertices.contains(v[1]), "corner vertex pruned");
        // Right angle at (64,0), radius 16: tangents (48,0)/(64,16), centre (48,16).
        for &k in &new {
            let l = map.lines[k];
            assert_eq!(l.front.sector, Some(s), "sides copied");
            for p in [map.vertices[l.v1], map.vertices[l.v2]] {
                let r = (p.x - 48.0).hypot(p.y - 16.0);
                assert!((r - 16.0).abs() < 1e-3, "chord endpoint off arc: r={r}");
            }
        }
        assert_single_loop(&map);
        assert_eq!(sector_at(&map, [32.0, 32.0]), Some(s));
    }

    #[test]
    fn fillet_rejects_junction_collinear_and_oversized() {
        // Junction: a third line at the corner.
        let (mut map, _) = sectored_square();
        let v = vert_keys(&map);
        let template = map.lines.values().next().copied().expect("line");
        let stem_far = map.vertices.insert(vtx(128.0, -64.0));
        map.lines.insert(LineDef {
            v1: v[1],
            v2: stem_far,
            ..template
        });
        assert!(!can_trim_corner(&map, v[1]));
        assert!(fillet_vertex(&mut map, v[1], 8.0, 4).is_empty());

        // Collinear: a straight chain vertex is not a corner.
        let map = fixture(
            vec![vtx(0.0, 0.0), vtx(4.0, 0.0), vtx(8.0, 0.0)],
            vec![dline(0, 1), dline(1, 2)],
            0,
        );
        let v = vert_keys(&map);
        assert!(!can_trim_corner(&map, v[1]));

        // Oversized: tangent trim exceeds the wall length.
        let (mut map, _) = sectored_square();
        let v = vert_keys(&map);
        assert!(fillet_vertex(&mut map, v[1], 100.0, 4).is_empty());
        assert_eq!(map.lines.len(), 4, "unchanged");
    }

    #[test]
    fn align_vertices_sets_axis_to_mean() {
        let map = fixture(
            vec![vtx(0.0, 0.0), vtx(10.0, 4.0), vtx(20.0, 8.0)],
            Vec::new(),
            0,
        );
        let v = vert_keys(&map);
        let moves = align_vertices(&map, &v, Axis::Y);
        assert_eq!(moves.len(), 2, "middle vertex already at the mean");
        assert!(moves.iter().all(|(_, p)| p[1] == 4.0), "mean y = 4");
        assert!(
            moves.iter().all(|(k, p)| p[0] == map.vertices[*k].x),
            "x untouched"
        );
    }

    #[test]
    fn distribute_vertices_spaces_evenly() {
        let map = fixture(
            vec![vtx(0.0, 0.0), vtx(3.0, 1.0), vtx(5.0, 2.0), vtx(30.0, 3.0)],
            Vec::new(),
            0,
        );
        let v = vert_keys(&map);
        let moves = distribute_vertices(&map, &v, Axis::X);
        // Extremes stay; interiors land at 10 and 20.
        let mut xs: Vec<f32> = moves.iter().map(|(_, p)| p[0]).collect();
        xs.sort_by(|a, b| a.total_cmp(b));
        assert_eq!(xs, [10.0, 20.0]);
    }

    #[test]
    fn straighten_chain_projects_interior() {
        let mut map = fixture(
            vec![
                vtx(0.0, 0.0),
                vtx(30.0, 10.0),
                vtx(60.0, -8.0),
                vtx(90.0, 0.0),
            ],
            vec![dline(0, 1), dline(1, 2), dline(2, 3)],
            0,
        );
        let v = vert_keys(&map);
        let moves = straighten_chain(&map, &v);
        assert_eq!(moves.len(), 2, "two interior vertices project");
        assert!(
            moves.iter().all(|(_, p)| p[1] == 0.0),
            "projected onto the end-to-end segment"
        );
        // A branch at an interior vertex refuses.
        let far = map.vertices.insert(vtx(30.0, 50.0));
        let template = map.lines.values().next().copied().expect("line");
        map.lines.insert(LineDef {
            v1: v[1],
            v2: far,
            ..template
        });
        let mut with_branch = v.clone();
        with_branch.push(far);
        assert!(straighten_chain(&map, &with_branch).is_empty());
    }

    #[test]
    fn straighten_rejects_l_selection_of_disconnected_sets() {
        let map = fixture(
            vec![
                vtx(0.0, 0.0),
                vtx(10.0, 0.0),
                vtx(50.0, 50.0),
                vtx(60.0, 50.0),
            ],
            vec![dline(0, 1), dline(2, 3)],
            0,
        );
        let v = vert_keys(&map);
        assert!(straighten_chain(&map, &v).is_empty(), "two separate chains");
    }

    #[test]
    fn mirror_with_fixup_keeps_sector_resolvable() {
        let (mut map, s) = sectored_square();
        let verts = vert_keys(&map);
        let lines = line_keys(&map);
        let moves = transform_moves(&map, &verts, [32.0, 32.0], 0.0, [-1.0, 1.0]);
        move_vertices(&mut map, &moves, &[], 0.1, def_sector());
        mirror_fixup(&mut map, &lines);
        assert_eq!(
            sector_at(&map, [32.0, 32.0]),
            Some(s),
            "interior resolves after mirror"
        );
    }

    #[test]
    fn transform_moves_scales_and_mirrors_about_pivot() {
        let map = fixture(vec![vtx(10.0, 0.0), vtx(20.0, 4.0)], Vec::new(), 0);
        let v = vert_keys(&map);
        let moves = transform_moves(&map, &v, [10.0, 0.0], 0.0, [2.0, 2.0]);
        assert_eq!(moves.len(), 1, "pivot vertex unmoved");
        assert_eq!(moves[0].1, [30.0, 8.0]);
        let mirrored = transform_moves(&map, &v, [0.0, 0.0], 0.0, [-1.0, 1.0]);
        let xs: Vec<f32> = mirrored.iter().map(|(_, p)| p[0]).collect();
        assert_eq!(xs, [-10.0, -20.0], "x negated, y kept");
    }

    #[test]
    fn extrude_sector_wall_grows_room() {
        let (mut map, s) = sectored_square();
        // The wall along y=0.
        let wall = map
            .lines
            .iter()
            .find(|(_, l)| map.vertices[l.v1].y == 0.0 && map.vertices[l.v2].y == 0.0)
            .map(|(k, _)| k)
            .expect("bottom wall");
        let new = extrude_line(&mut map, wall, [0.0, -32.0], 0.1, def_sector());
        assert_eq!(new.len(), 3);
        assert_eq!(map.lines.len(), 7);
        assert_eq!(map.sectors.len(), 2, "quad gets its own sector");
        let shared = map.lines[wall];
        assert!(shared.back.is_some(), "source wall promoted to two-sided");
        let below = sector_at(&map, [32.0, -16.0]).expect("extruded space sectored");
        assert_ne!(below, s);
        assert_eq!(sector_at(&map, [32.0, 32.0]), Some(s), "room unchanged");
        let sides = [shared.front.sector, shared.back.and_then(|b| b.sector)];
        assert!(sides.contains(&Some(s)) && sides.contains(&Some(below)));
    }

    #[test]
    fn extrude_void_line_creates_sector() {
        let mut map = fixture(vec![vtx(0.0, 0.0), vtx(64.0, 0.0)], vec![dline(0, 1)], 0);
        let k = line_keys(&map)[0];
        let new = extrude_line(&mut map, k, [0.0, 32.0], 0.1, def_sector());
        assert_eq!(new.len(), 3);
        assert_eq!(map.lines.len(), 4);
        assert_eq!(map.sectors.len(), 1);
        assert!(sector_at(&map, [32.0, 16.0]).is_some());
    }

    #[test]
    fn extrude_rejects_parallel_and_zero_delta() {
        let mut map = fixture(vec![vtx(0.0, 0.0), vtx(64.0, 0.0)], vec![dline(0, 1)], 0);
        let k = line_keys(&map)[0];
        assert!(extrude_line(&mut map, k, [16.0, 0.0], 0.1, def_sector()).is_empty());
        assert!(extrude_line(&mut map, k, [0.0, 0.0], 0.1, def_sector()).is_empty());
        assert_eq!(map.lines.len(), 1, "unchanged");
    }

    #[test]
    fn extrude_splits_crossed_lines() {
        // A floating vertical line crossing the extruded quad's far edge.
        let mut map = fixture(
            vec![
                vtx(0.0, 0.0),
                vtx(64.0, 0.0),
                vtx(32.0, 16.0),
                vtx(32.0, 48.0),
            ],
            vec![dline(0, 1), dline(2, 3)],
            0,
        );
        let k = line_keys(&map)[0];
        let new = extrude_line(&mut map, k, [0.0, 32.0], 0.1, def_sector());
        assert_eq!(new.len(), 3);
        assert!(map.lines.len() > 5, "crossing split both lines");
    }

    #[test]
    fn move_onto_collinear_contained_span_dedups() {
        // Drag a floating line onto the interior of a longer wall: the wall splits at both landed endpoints and the duplicate middle collapses.
        let mut map = fixture(
            vec![vtx(0.0, 0.0), vtx(8.0, 0.0), vtx(2.0, 10.0), vtx(6.0, 10.0)],
            vec![dline(0, 1), dline(2, 3)],
            0,
        );
        let v = vert_keys(&map);
        move_vertices(
            &mut map,
            &[(v[2], [2.0, 0.0]), (v[3], [6.0, 0.0])],
            &[],
            0.5,
            def_sector(),
        );
        assert_eq!(map.lines.len(), 3, "wall split twice, duplicate removed");
        assert_eq!(map.vertices.len(), 4);
    }

    #[test]
    fn move_onto_collinear_partial_overlap_dedups() {
        // Drag a line so it half-overlaps a wall: both split at the shared span and the duplicate collapses.
        let mut map = fixture(
            vec![
                vtx(0.0, 0.0),
                vtx(8.0, 0.0),
                vtx(4.0, 10.0),
                vtx(12.0, 10.0),
            ],
            vec![dline(0, 1), dline(2, 3)],
            0,
        );
        let v = vert_keys(&map);
        move_vertices(
            &mut map,
            &[(v[2], [4.0, 0.0]), (v[3], [12.0, 0.0])],
            &[],
            0.5,
            def_sector(),
        );
        assert_eq!(map.lines.len(), 3, "shared span deduped");
        assert_eq!(map.vertices.len(), 4);
    }

    #[test]
    fn merge_collinear_lines_rejects_disjoint() {
        // Two lines that share no vertex cannot merge.
        let mut map = fixture(
            vec![vtx(0.0, 0.0), vtx(4.0, 0.0), vtx(8.0, 0.0), vtx(12.0, 0.0)],
            vec![dline(0, 1), dline(2, 3)],
            0,
        );
        let k = line_keys(&map);
        assert!(!merge_collinear_lines(&mut map, k[0], k[1], FRAC_PI_4));
    }

    #[test]
    fn sectors_share_two_sided_wall_detects_adjacency() {
        let mut map = fixture(
            vec![vtx(0.0, 0.0), vtx(4.0, 0.0)],
            vec![{
                let mut l = dline(0, 1);
                l.front = dside(Some(0));
                l.back = Some(dside(Some(1)));
                l
            }],
            3,
        );
        map.lines.values_mut().next().expect("line").flags |= LineFlags::TWO_SIDED;
        let s = sector_keys(&map);
        assert!(sectors_share_two_sided_wall(&map, s[0], s[1]));
        assert!(sectors_share_two_sided_wall(&map, s[1], s[0]));
        assert!(
            !sectors_share_two_sided_wall(&map, s[0], s[2]),
            "no wall to 2"
        );
    }

    #[test]
    fn delete_sector_makes_shared_wall_single_sided_facing_survivor() {
        let mut map = fixture(
            vec![vtx(2.0, 0.0), vtx(2.0, 4.0)],
            vec![
                {
                    let mut l = dline(0, 1);
                    l.front = dside(Some(1));
                    l.back = Some(dside(Some(0)));
                    l.flags = LineFlags::TWO_SIDED;
                    l
                },
                {
                    let mut l = dline(0, 1);
                    l.front = dside(Some(0));
                    l
                },
            ],
            2,
        );
        let s = sector_keys(&map);
        let k = line_keys(&map);
        delete_sector(&mut map, s[0]);

        assert_eq!(map.sectors.len(), 1, "deleted pruned, survivor remains");
        let divider = &map.lines[k[0]];
        assert!(divider.back.is_none(), "divider single-sided");
        assert!(
            !divider.flags.contains(LineFlags::TWO_SIDED),
            "two-sided flag cleared"
        );
        assert_eq!(divider.front.sector, Some(s[1]), "faces the survivor");
        assert_eq!(map.lines[k[1]].front.sector, None, "outer wall voided");
    }

    #[test]
    fn delete_sector_promotes_back_when_deleted_on_front() {
        let mut map = fixture(
            vec![vtx(2.0, 0.0), vtx(2.0, 4.0)],
            vec![{
                let mut l = dline(0, 1);
                l.front = dside(Some(0));
                l.back = Some(dside(Some(1)));
                l.flags = LineFlags::TWO_SIDED;
                l
            }],
            2,
        );
        let s = sector_keys(&map);
        let k = line_keys(&map)[0];
        delete_sector(&mut map, s[0]);

        assert_eq!(map.sectors.len(), 1, "deleted sector pruned");
        let divider = &map.lines[k];
        assert!(divider.back.is_none(), "now single-sided");
        assert!(
            !divider.flags.contains(LineFlags::TWO_SIDED),
            "two-sided flag cleared"
        );
        assert_eq!(
            divider.front.sector,
            Some(s[1]),
            "back sector promoted to front"
        );
    }
}
