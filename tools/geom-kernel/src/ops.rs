//! Higher-level editing operations over an [`EditorMap`].
//!
//! Weld a cluster of vertices, move geometry, construct edges and shapes,
//! copy/paste a self-contained fragment, flip lines, and merge sectors. These
//! compose the primitives in [`crate::geom`] and [`crate::sector_build`].
//! Tolerances arrive in world units; pixel/grid concerns belong to the caller.

use std::collections::{HashMap, HashSet};
use std::f32::consts::TAU;

use crate::flags::LineFlags;
use crate::geom::{
    dedup_coincident_lines, merge_coincident_vertices, split_lines_at_intersections,
    weld_moved_vertices, weld_vertices,
};
use crate::model::{EditorMap, LineDef, Sector, SideDef, Thing};
use crate::sector_build::build_sectors;

/// Outcome of [`weld_cluster`].
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct WeldResult {
    /// Point the welded vertices collapsed onto; `None` when nothing welded.
    pub target: Option<[f32; 2]>,
    /// How many lines the weld removed (collapsed to a point or de-duplicated).
    pub removed_lines: usize,
}

impl WeldResult {
    /// Whether the weld changed geometry.
    pub fn changed(&self) -> bool {
        self.target.is_some()
    }
}

/// Weld the candidate vertices within `tol` of their centroid onto one point.
///
/// The target is the centroid of those participants; vertices farther than `tol`
/// from the candidate centroid are left in place. Lines that collapse or
/// duplicate as a result are removed, and lines around the weld point are
/// re-sectored against `default_sector`. Fewer than two participants is a no-op.
pub fn weld_cluster(
    map: &mut EditorMap,
    candidate_ids: &[u32],
    tol: f32,
    default_sector: Sector,
) -> WeldResult {
    let pts: Vec<(u32, [f32; 2])> = candidate_ids
        .iter()
        .filter_map(|&i| map.vertices.get(i as usize).map(|v| (i, [v.x, v.y])))
        .collect();
    if pts.len() < 2 {
        return WeldResult::default();
    }
    let centroid = mean(pts.iter().map(|(_, p)| *p));
    let tol_sq = tol * tol;
    let near: Vec<(u32, [f32; 2])> = pts
        .into_iter()
        .filter(|(_, p)| dist_sq(*p, centroid) <= tol_sq)
        .collect();
    if near.len() < 2 {
        return WeldResult::default();
    }
    let target = mean(near.iter().map(|(_, p)| *p));
    let ids: Vec<u32> = near.iter().map(|(i, _)| *i).collect();

    let before = map.lines.len();
    weld_vertices(map, &ids, target);
    let removed_lines = before - map.lines.len();
    let affected = lines_at_positions(map, &[target]);
    build_sectors(map, &affected, &[], default_sector);
    WeldResult {
        target: Some(target),
        removed_lines,
    }
}

/// Outcome of [`move_vertices`].
#[derive(Debug, Default, Clone, PartialEq)]
pub struct MoveResult {
    /// Whether the move changed topology (a crossing split, a weld, or a dedup),
    /// triggering a re-sector. A plain nudge leaves this `false`.
    pub resectored: bool,
    /// Lines removed by the de-duplication pass.
    pub deduped_lines: Vec<u32>,
}

/// Commit a vertex (and thing) move to the map.
///
/// `moves`/`thing_moves` carry the final, already-snapped positions per index.
/// After applying them, lines incident to a moved vertex are split where they
/// now cross, coincident vertices/lines collapse, and — only on a real topology
/// change — the lines around the move are re-sectored against `default_sector`.
/// `tol` is in world units.
pub fn move_vertices(
    map: &mut EditorMap,
    moves: &[(u32, [f32; 2])],
    thing_moves: &[(u32, [i32; 2])],
    tol: f32,
    default_sector: Sector,
) -> MoveResult {
    for &(i, p) in moves {
        if let Some(v) = map.vertices.get_mut(i as usize) {
            v.x = p[0];
            v.y = p[1];
        }
    }
    for &(i, p) in thing_moves {
        if let Some(t) = map.things.get_mut(i as usize) {
            t.x = p[0];
            t.y = p[1];
        }
    }
    let vert_ids: Vec<u32> = moves.iter().map(|(i, _)| *i).collect();
    let moved_set: HashSet<u32> = vert_ids.iter().copied().collect();
    let moved_lines: Vec<u32> = (0..map.lines.len() as u32)
        .filter(|&i| {
            let l = &map.lines[i as usize];
            moved_set.contains(&l.v1) || moved_set.contains(&l.v2)
        })
        .collect();
    if moved_lines.is_empty() {
        return MoveResult::default();
    }
    // Re-sectoring is scoped by position: the merge/dedup below renumber
    // vertices and lines, but post-move positions are byte-stable.
    let moved_pos: Vec<[f32; 2]> = vert_ids
        .iter()
        .filter_map(|&i| map.vertices.get(i as usize).map(|v| [v.x, v.y]))
        .collect();
    let old_verts = map.vertices.len();
    let crossed = split_lines_at_intersections(map, &moved_lines, tol);
    let crossings: Vec<[f32; 2]> = (old_verts..map.vertices.len())
        .map(|i| [map.vertices[i].x, map.vertices[i].y])
        .collect();
    let welded = weld_moved_vertices(map, &vert_ids, tol);
    merge_coincident_vertices(map);
    let deduped_lines = dedup_coincident_lines(map);
    if !welded && crossed.is_empty() && deduped_lines.is_empty() {
        return MoveResult::default();
    }
    let affected = lines_at_positions(map, &moved_pos);
    let newly: Vec<u32> = affected
        .iter()
        .copied()
        .filter(|&i| line_at_crossing(map, i, &crossings))
        .collect();
    build_sectors(map, &affected, &newly, default_sector);
    MoveResult {
        resectored: true,
        deduped_lines,
    }
}

/// Append a one-sided edge between two world points.
///
/// Reuse exact-match vertices, push a line carrying `front` and `flags` (its
/// sector left void here — the caller's sector pass assigns it), then split
/// anything it crosses. A degenerate (same-point) edge is skipped. `tol` is in
/// world units.
pub fn add_edge(
    map: &mut EditorMap,
    a: [f32; 2],
    b: [f32; 2],
    front: SideDef,
    flags: LineFlags,
    tol: f32,
) {
    if a == b {
        return;
    }
    let v1 = map.find_or_add_vertex(a);
    let v2 = map.find_or_add_vertex(b);
    map.lines.push(LineDef {
        v1,
        v2,
        flags,
        special: 0,
        tag: 0,
        front,
        back: None,
    });
    let new_line = (map.lines.len() - 1) as u32;
    split_lines_at_intersections(map, &[new_line], tol);
}

/// Re-sector a finished draw.
///
/// Every line added since `base`, plus every line sharing a vertex with one (a
/// split wall's surviving head), is re-derived against `record`. Unlike a move,
/// a draw reshapes sectors, so every affected line is treated as new geometry.
pub fn derive_sectors(map: &mut EditorMap, base: usize, record: Sector) {
    if map.lines.len() <= base {
        return;
    }
    let affected = lines_since(map, base);
    build_sectors(map, &affected, &affected, record);
}

/// Lines a finished draw must sector: the new lines (`base`..end) plus every
/// line sharing a vertex with one.
fn lines_since(map: &EditorMap, base: usize) -> Vec<u32> {
    let mut touched = vec![false; map.vertices.len()];
    for l in &map.lines[base..] {
        touched[l.v1 as usize] = true;
        touched[l.v2 as usize] = true;
    }
    (0..map.lines.len() as u32)
        .filter(|&i| {
            let l = &map.lines[i as usize];
            i as usize >= base || touched[l.v1 as usize] || touched[l.v2 as usize]
        })
        .collect()
}

/// Flip lines in place: swap each line's endpoints and its front/back sides, so
/// a two-sided line stays visually identical (winding fix) and a one-sided line
/// reverses its facing.
pub fn flip_lines(map: &mut EditorMap, indices: &[u32]) {
    for &i in indices {
        if let Some(line) = map.lines.get_mut(i as usize) {
            std::mem::swap(&mut line.v1, &mut line.v2);
            if let Some(back) = line.back.take() {
                line.back = Some(std::mem::replace(&mut line.front, back));
            }
        }
    }
}

/// The shared vertex of lines `a` and `b` and each line's other (far) endpoint,
/// if they share exactly one. `None` when they share no vertex, are the same
/// line, or are coincident.
fn shared_and_far(map: &EditorMap, a: u32, b: u32) -> Option<(u32, u32, u32)> {
    if a == b {
        return None;
    }
    let la = map.lines.get(a as usize)?;
    let lb = map.lines.get(b as usize)?;
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

/// The deviation from straight (radians) of the chain far_a → shared → far_b: 0
/// when the two segments are perfectly collinear, π when fully doubled back.
fn chain_deviation(map: &EditorMap, shared: u32, far_a: u32, far_b: u32) -> Option<f32> {
    let s = map.vertices.get(shared as usize)?;
    let pa = map.vertices.get(far_a as usize)?;
    let pb = map.vertices.get(far_b as usize)?;
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
    Some(std::f32::consts::PI - cos.acos())
}

/// Whether lines `a` and `b` share a vertex and the chain through it deviates
/// from straight by less than `max_dev_rad` — i.e. they are mergeable.
pub fn lines_share_vertex_within_angle(map: &EditorMap, a: u32, b: u32, max_dev_rad: f32) -> bool {
    let Some((shared, far_a, far_b)) = shared_and_far(map, a, b) else {
        return false;
    };
    chain_deviation(map, shared, far_a, far_b).is_some_and(|d| d < max_dev_rad)
}

/// Merge two near-collinear lines sharing a vertex into one.
///
/// The chain far_a → shared → far_b becomes one line spanning the far vertices,
/// when it deviates from straight by less than `max_dev_rad`. Line `a` is
/// reshaped to the span (keeping its sides/flags); line `b` and the now-orphan
/// shared vertex are removed. Returns whether the merge happened.
pub fn merge_collinear_lines(map: &mut EditorMap, a: u32, b: u32, max_dev_rad: f32) -> bool {
    let Some((shared, far_a, far_b)) = shared_and_far(map, a, b) else {
        return false;
    };
    if !chain_deviation(map, shared, far_a, far_b).is_some_and(|d| d < max_dev_rad) {
        return false;
    }
    // Reshape `a` to span the far vertices, preserving its winding direction.
    let line = &mut map.lines[a as usize];
    if line.v1 == shared {
        line.v1 = far_b;
    } else {
        line.v2 = far_b;
    }
    map.remove_lines(&[b]);
    true
}

/// Whether sectors `a` and `b` are adjacent — joined by at least one two-sided
/// line whose two sides face one and the other.
pub fn sectors_share_two_sided_wall(map: &EditorMap, a: u32, b: u32) -> bool {
    map.lines.iter().any(|l| {
        let Some(back) = l.back else {
            return false;
        };
        let (f, bk) = (l.front.sector, back.sector);
        (f == Some(a) && bk == Some(b)) || (f == Some(b) && bk == Some(a))
    })
}

/// Delete sector `index`: its shared two-sided walls become single-sided facing
/// the neighbour; its outer single-sided walls become void. Then renumber.
pub fn delete_sector(map: &mut EditorMap, index: u32) {
    for line in &mut map.lines {
        let front_is = line.front.sector == Some(index);
        let back_is = line.back.is_some_and(|b| b.sector == Some(index));
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

/// Merge each pair of sectors joined by a just-deleted two-sided wall.
///
/// The lower index of each connected group survives, every other member's sides
/// reassign to it, and emptied records prune. Union-find over the pairs so
/// chained deletes (a|b and b|c) collapse to one survivor.
pub fn merge_sectors(map: &mut EditorMap, pairs: &[(u32, u32)]) {
    if pairs.is_empty() {
        return;
    }
    let mut parent: Vec<u32> = (0..map.sectors.len() as u32).collect();
    for &(a, b) in pairs {
        let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
        if ra != rb {
            let (keep, drop) = (ra.min(rb), ra.max(rb));
            parent[drop as usize] = keep;
        }
    }
    let reassign = |s: &mut Option<u32>, parent: &mut [u32]| {
        if let Some(idx) = s {
            *s = Some(find(parent, *idx));
        }
    };
    for line in &mut map.lines {
        reassign(&mut line.front.sector, &mut parent);
        if let Some(back) = &mut line.back {
            reassign(&mut back.sector, &mut parent);
        }
    }
    map.prune_unused_sectors();
}

fn find(parent: &mut [u32], mut x: u32) -> u32 {
    while parent[x as usize] != x {
        parent[x as usize] = parent[parent[x as usize] as usize];
        x = parent[x as usize];
    }
    x
}

/// Build a self-contained fragment (its own [`EditorMap`]) from a selection.
///
/// From the selected lines and things: referenced vertices are copied and
/// deduplicated, line endpoints remap to fragment-local indices, and each faced
/// sector is inlined into the fragment. Paste with [`paste_fragment`].
pub fn extract_fragment(map: &EditorMap, line_ids: &[u32], thing_ids: &[u32]) -> EditorMap {
    let mut frag = EditorMap::default();
    let mut vmap: HashMap<u32, u32> = HashMap::new();
    let mut smap: HashMap<u32, u32> = HashMap::new();
    let mut local_vertex = |frag: &mut EditorMap, src: u32| -> u32 {
        *vmap.entry(src).or_insert_with(|| {
            frag.vertices.push(map.vertices[src as usize]);
            (frag.vertices.len() - 1) as u32
        })
    };
    let mut local_side = |frag: &mut EditorMap, side: &SideDef| -> SideDef {
        let sector = side.sector.map(|s| {
            *smap.entry(s).or_insert_with(|| {
                frag.sectors.push(map.sectors[s as usize]);
                (frag.sectors.len() - 1) as u32
            })
        });
        SideDef {
            sector,
            ..*side
        }
    };
    for &i in line_ids {
        let Some(line) = map.lines.get(i as usize) else {
            continue;
        };
        let v1 = local_vertex(&mut frag, line.v1);
        let v2 = local_vertex(&mut frag, line.v2);
        let front = local_side(&mut frag, &line.front);
        let back = line.back.as_ref().map(|s| local_side(&mut frag, s));
        frag.lines.push(LineDef {
            v1,
            v2,
            front,
            back,
            ..*line
        });
    }
    for &i in thing_ids {
        if let Some(t) = map.things.get(i as usize) {
            frag.things.push(*t);
        }
    }
    frag
}

/// Append `fragment` to `map` offset by `delta`.
///
/// Its vertices reuse exact matches, its sectors append once each, and its
/// lines/things copy over with remapped indices. Returns the new line and thing
/// indices (in fragment order) for the caller to select.
pub fn paste_fragment(
    map: &mut EditorMap,
    fragment: &EditorMap,
    delta: [f32; 2],
) -> (Vec<u32>, Vec<u32>) {
    let verts: Vec<u32> = fragment
        .vertices
        .iter()
        .map(|v| map.find_or_add_vertex([v.x + delta[0], v.y + delta[1]]))
        .collect();
    let sectors: Vec<u32> = fragment
        .sectors
        .iter()
        .map(|s| {
            map.sectors.push(*s);
            (map.sectors.len() - 1) as u32
        })
        .collect();
    let remap_side = |side: &SideDef| SideDef {
        sector: side.sector.map(|s| sectors[s as usize]),
        ..*side
    };
    let mut new_lines = Vec::with_capacity(fragment.lines.len());
    for line in &fragment.lines {
        map.lines.push(LineDef {
            v1: verts[line.v1 as usize],
            v2: verts[line.v2 as usize],
            front: remap_side(&line.front),
            back: line.back.as_ref().map(remap_side),
            ..*line
        });
        new_lines.push((map.lines.len() - 1) as u32);
    }
    let mut new_things = Vec::with_capacity(fragment.things.len());
    for t in &fragment.things {
        map.things.push(Thing {
            x: t.x + delta[0] as i32,
            y: t.y + delta[1] as i32,
            ..*t
        });
        new_things.push((map.things.len() - 1) as u32);
    }
    (new_lines, new_things)
}

/// The min corner of a fragment's geometry; paste offsets it to the drop point.
pub fn fragment_min_corner(fragment: &EditorMap) -> [f32; 2] {
    let mut min = [f32::MAX, f32::MAX];
    for v in &fragment.vertices {
        min[0] = min[0].min(v.x);
        min[1] = min[1].min(v.y);
    }
    for t in &fragment.things {
        min[0] = min[0].min(t.x as f32);
        min[1] = min[1].min(t.y as f32);
    }
    if min[0] == f32::MAX { [0.0, 0.0] } else { min }
}

/// The four corners of a corner-to-corner rectangle, wound CCW in Y-up. `a` and
/// `b` are opposite corners.
pub fn rect_corners(a: [f32; 2], b: [f32; 2]) -> [[f32; 2]; 4] {
    let (x0, x1) = (a[0].min(b[0]), a[0].max(b[0]));
    let (y0, y1) = (a[1].min(b[1]), a[1].max(b[1]));
    [[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
}

/// Vertices of a regular `sides`-gon centred at `center`, radius and rotation
/// taken from `pointer` (distance = radius, angle = rotation); the first vertex
/// points at `pointer`.
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

/// Lines to re-sector after a move: those touching a moved-vertex position, plus
/// every line sharing a vertex with one (a split wall's surviving head). Scoped
/// by position because the merge/dedup before this renumber vertices and lines.
fn lines_at_positions(map: &EditorMap, positions: &[[f32; 2]]) -> Vec<u32> {
    let at_moved = |x: f32, y: f32| positions.iter().any(|p| p[0] == x && p[1] == y);
    let mut touched = vec![false; map.vertices.len()];
    for l in &map.lines {
        if let (Some(p1), Some(p2)) = (
            map.vertices.get(l.v1 as usize),
            map.vertices.get(l.v2 as usize),
        ) && (at_moved(p1.x, p1.y) || at_moved(p2.x, p2.y))
        {
            touched[l.v1 as usize] = true;
            touched[l.v2 as usize] = true;
        }
    }
    (0..map.lines.len() as u32)
        .filter(|&i| {
            let l = &map.lines[i as usize];
            touched[l.v1 as usize] || touched[l.v2 as usize]
        })
        .collect()
}

/// True when an endpoint of line `i` is a split crossing point — the move cut
/// this line, so the re-sector may treat it as new geometry.
fn line_at_crossing(map: &EditorMap, i: u32, crossings: &[[f32; 2]]) -> bool {
    let Some(l) = map.lines.get(i as usize) else {
        return false;
    };
    [l.v1, l.v2].iter().any(|&v| {
        map.vertices
            .get(v as usize)
            .is_some_and(|p| crossings.iter().any(|c| c[0] == p.x && c[1] == p.y))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{LineDef, SideDef, Vertex};
    use crate::name8::Name8;

    fn vtx(x: f32, y: f32) -> Vertex {
        Vertex {
            x,
            y,
        }
    }

    fn side() -> SideDef {
        SideDef {
            x_offset: 0,
            y_offset: 0,
            top_tex: Name8::EMPTY,
            bottom_tex: Name8::EMPTY,
            middle_tex: Name8::EMPTY,
            sector: None,
        }
    }

    fn line(v1: u32, v2: u32) -> LineDef {
        LineDef {
            v1,
            v2,
            flags: LineFlags::empty(),
            special: 0,
            tag: 0,
            front: side(),
            back: None,
        }
    }

    fn def_sector() -> Sector {
        Sector {
            floor_height: 0,
            floor_flat: Name8::EMPTY,
            ceil_height: 128,
            ceil_flat: Name8::EMPTY,
            light_level: 160,
            special: 0,
            tag: 0,
        }
    }

    #[test]
    fn weld_cluster_collapses_near_corners() {
        // Triangle; weld the two near base corners to their centroid.
        let mut map = EditorMap {
            vertices: vec![vtx(0.0, 0.0), vtx(4.0, 0.0), vtx(2.0, 40.0)],
            lines: vec![line(0, 1), line(1, 2), line(2, 0)],
            ..Default::default()
        };
        let r = weld_cluster(&mut map, &[0, 1], 8.0, def_sector());
        assert_eq!(r.target, Some([2.0, 0.0]), "centroid of the two corners");
        assert_eq!(map.vertices.len(), 2);
        assert!(r.removed_lines >= 1, "base collapsed");
    }

    #[test]
    fn weld_cluster_skips_far_apart() {
        let mut map = EditorMap {
            vertices: vec![vtx(0.0, 0.0), vtx(100.0, 0.0)],
            lines: vec![line(0, 1)],
            ..Default::default()
        };
        let r = weld_cluster(&mut map, &[0, 1], 8.0, def_sector());
        assert!(!r.changed(), "both outside the weld radius");
        assert_eq!(map.vertices.len(), 2);
    }

    #[test]
    fn move_vertices_plain_nudge_does_not_resector() {
        // A lone line, no crossing/weld/dedup: a nudge changes positions only.
        let mut map = EditorMap {
            vertices: vec![vtx(0.0, 0.0), vtx(64.0, 0.0)],
            lines: vec![line(0, 1)],
            ..Default::default()
        };
        let r = move_vertices(&mut map, &[(0, [8.0, 8.0])], &[], 2.0, def_sector());
        assert!(!r.resectored, "plain nudge leaves sectoring alone");
        assert_eq!((map.vertices[0].x, map.vertices[0].y), (8.0, 8.0));
    }

    #[test]
    fn move_vertices_weld_on_drop_collapses_line() {
        // Chain a-b-c; move b onto c (within tol). Line b-c collapses.
        let mut map = EditorMap {
            vertices: vec![vtx(0.0, 0.0), vtx(40.0, 1.0), vtx(40.0, 0.0)],
            lines: vec![line(0, 1), line(1, 2)],
            ..Default::default()
        };
        let r = move_vertices(&mut map, &[(1, [40.0, 0.0])], &[], 2.0, def_sector());
        assert!(r.resectored, "weld is a topology change");
        assert_eq!(map.lines.len(), 1, "collapsed line removed");
    }

    #[test]
    fn extract_then_paste_round_trips_geometry() {
        // Two lines sharing a vertex, one faced sector. Extract both, paste at a
        // delta: the fragment self-contains its verts/sector, paste re-appends.
        let mut map = EditorMap {
            vertices: vec![vtx(0.0, 0.0), vtx(8.0, 0.0), vtx(8.0, 8.0)],
            lines: vec![line(0, 1), line(1, 2)],
            sectors: vec![def_sector()],
            ..Default::default()
        };
        map.lines[0].front.sector = Some(0);
        let frag = extract_fragment(&map, &[0, 1], &[]);
        assert_eq!(frag.vertices.len(), 3, "shared vertex copied once");
        assert_eq!(frag.lines.len(), 2);
        assert_eq!(frag.sectors.len(), 1, "faced sector inlined");
        assert_eq!(frag.lines[0].front.sector, Some(0), "remapped to fragment");

        let (lines, _things) = paste_fragment(&mut map, &frag, [100.0, 0.0]);
        assert_eq!(lines.len(), 2);
        assert_eq!(map.lines.len(), 4, "two pasted lines appended");
        let pasted = &map.lines[lines[0] as usize];
        let v1 = map.vertices[pasted.v1 as usize];
        assert_eq!((v1.x, v1.y), (100.0, 0.0), "offset by delta");
        assert_eq!(map.sectors.len(), 2, "fragment sector appended once");
    }

    #[test]
    fn flip_lines_swaps_endpoints_and_sides() {
        let mut map = EditorMap {
            vertices: vec![vtx(0.0, 0.0), vtx(4.0, 0.0)],
            lines: vec![line(0, 1)],
            ..Default::default()
        };
        map.lines[0].back = Some(side());
        flip_lines(&mut map, &[0]);
        assert_eq!(
            (map.lines[0].v1, map.lines[0].v2),
            (1, 0),
            "endpoints swapped"
        );
    }

    #[test]
    fn merge_sectors_unifies_to_lowest_index() {
        // Lines facing sectors 0,1,2; merge (0,1) and (1,2) -> all become 0.
        let mut map = EditorMap {
            vertices: vec![vtx(0.0, 0.0), vtx(4.0, 0.0)],
            sectors: vec![def_sector(), def_sector(), def_sector()],
            ..Default::default()
        };
        for s in 0..3u32 {
            let mut l = line(0, 1);
            l.front.sector = Some(s);
            map.lines.push(l);
        }
        merge_sectors(&mut map, &[(0, 1), (1, 2)]);
        assert_eq!(map.sectors.len(), 1, "three sectors merged to one");
        for l in &map.lines {
            assert_eq!(l.front.sector, Some(0));
        }
    }

    #[test]
    fn add_edge_splits_crossing_line() {
        // A horizontal line; add a vertical edge crossing it -> a split vertex.
        let mut map = EditorMap {
            vertices: vec![vtx(0.0, 0.0), vtx(8.0, 0.0)],
            lines: vec![line(0, 1)],
            ..Default::default()
        };
        add_edge(
            &mut map,
            [4.0, -4.0],
            [4.0, 4.0],
            side(),
            LineFlags::BLOCKING,
            0.1,
        );
        assert!(map.lines.len() > 2, "the crossed line was split");
    }

    #[test]
    fn merge_collinear_lines_spans_far_vertices() {
        // Straight chain a(0,0)-b(4,0)-c(8,0): merging the two lines spans a-c.
        let mut map = EditorMap {
            vertices: vec![vtx(0.0, 0.0), vtx(4.0, 0.0), vtx(8.0, 0.0)],
            lines: vec![line(0, 1), line(1, 2)],
            ..Default::default()
        };
        let tol = std::f32::consts::FRAC_PI_4;
        assert!(lines_share_vertex_within_angle(&map, 0, 1, tol));
        assert!(merge_collinear_lines(&mut map, 0, 1, tol));
        assert_eq!(map.lines.len(), 1, "two lines became one");
        assert_eq!(map.vertices.len(), 2, "shared vertex pruned");
        let l = &map.lines[0];
        let mut xs = [map.vertices[l.v1 as usize].x, map.vertices[l.v2 as usize].x];
        xs.sort_by(|a, b| a.total_cmp(b));
        assert_eq!(xs, [0.0, 8.0], "spans the far vertices");
    }

    #[test]
    fn merge_collinear_lines_rejects_sharp_angle() {
        // Right-angle chain (90° deviation from straight): not mergeable at 45°.
        let mut map = EditorMap {
            vertices: vec![vtx(0.0, 0.0), vtx(4.0, 0.0), vtx(4.0, 4.0)],
            lines: vec![line(0, 1), line(1, 2)],
            ..Default::default()
        };
        let tol = std::f32::consts::FRAC_PI_4;
        assert!(!lines_share_vertex_within_angle(&map, 0, 1, tol));
        assert!(!merge_collinear_lines(&mut map, 0, 1, tol));
        assert_eq!(map.lines.len(), 2, "unchanged");
    }

    #[test]
    fn merge_collinear_lines_rejects_disjoint() {
        // Two lines that share no vertex cannot merge.
        let mut map = EditorMap {
            vertices: vec![vtx(0.0, 0.0), vtx(4.0, 0.0), vtx(8.0, 0.0), vtx(12.0, 0.0)],
            lines: vec![line(0, 1), line(2, 3)],
            ..Default::default()
        };
        assert!(!merge_collinear_lines(
            &mut map,
            0,
            1,
            std::f32::consts::FRAC_PI_4
        ));
    }

    #[test]
    fn sectors_share_two_sided_wall_detects_adjacency() {
        let mut map = EditorMap {
            vertices: vec![vtx(0.0, 0.0), vtx(4.0, 0.0)],
            sectors: vec![def_sector(), def_sector(), def_sector()],
            ..Default::default()
        };
        // A two-sided wall between sectors 0 and 1.
        let mut shared = line(0, 1);
        shared.front.sector = Some(0);
        shared.back = Some(SideDef {
            sector: Some(1),
            ..side()
        });
        map.lines.push(shared);
        assert!(sectors_share_two_sided_wall(&map, 0, 1));
        assert!(sectors_share_two_sided_wall(&map, 1, 0));
        assert!(!sectors_share_two_sided_wall(&map, 0, 2), "no wall to 2");
    }

    #[test]
    fn delete_sector_makes_shared_wall_single_sided_facing_survivor() {
        let mut map = EditorMap {
            vertices: vec![vtx(2.0, 0.0), vtx(2.0, 4.0)],
            sectors: vec![def_sector(), def_sector()],
            ..Default::default()
        };
        let mut divider = line(0, 1);
        divider.front.sector = Some(1);
        let mut back = divider.front;
        back.sector = Some(0);
        divider.back = Some(back);
        divider.flags.insert(LineFlags::TWO_SIDED);
        map.lines.push(divider);
        let mut outer = line(0, 1);
        outer.front.sector = Some(0);
        map.lines.push(outer);

        delete_sector(&mut map, 0);

        assert_eq!(map.sectors.len(), 1, "sector 0 pruned, sector 1 remains");
        let divider = &map.lines[0];
        assert!(divider.back.is_none(), "divider single-sided");
        assert!(
            !divider.flags.contains(LineFlags::TWO_SIDED),
            "two-sided flag cleared"
        );
        assert_eq!(divider.front.sector, Some(0), "faces the survivor");
        assert_eq!(map.lines[1].front.sector, None, "outer wall voided");
    }

    #[test]
    fn delete_sector_promotes_back_when_deleted_on_front() {
        let mut map = EditorMap {
            vertices: vec![vtx(2.0, 0.0), vtx(2.0, 4.0)],
            sectors: vec![def_sector(), def_sector()],
            ..Default::default()
        };
        let mut divider = line(0, 1);
        divider.front.sector = Some(0);
        let mut back = divider.front;
        back.sector = Some(1);
        divider.back = Some(back);
        divider.flags.insert(LineFlags::TWO_SIDED);
        map.lines.push(divider);

        delete_sector(&mut map, 0);

        assert_eq!(map.sectors.len(), 1, "deleted sector pruned");
        let divider = &map.lines[0];
        assert!(divider.back.is_none(), "now single-sided");
        assert!(
            !divider.flags.contains(LineFlags::TWO_SIDED),
            "two-sided flag cleared"
        );
        assert_eq!(
            divider.front.sector,
            Some(0),
            "back sector 1 promoted to front, renumbered to 0"
        );
    }
}
