//! Geometric audit and heal over an [`EditorMap`]: read-only scans for defects the structural validator cannot see (near-coincident vertices, unsplit T-junctions, collinear overlaps, orphans), plus a repair pass composing the same split/weld/dedup machinery the editing ops use.

use std::collections::{HashMap, HashSet};

use crate::flags::LineFlags;
use crate::geom::{
    dedup_coincident_lines, point_on_segment_interior, segment_points,
    split_lines_at_intersections, weld_vertices,
};
use crate::model::{EditorMap, LineKey, SectorKey, VertKey};

/// Max heal passes; a fix can expose a new defect (a weld creating a T-junction).
const MAX_HEAL_PASSES: usize = 4;
/// Minimum normalised cross product between two line directions to count as non-parallel.
const PARALLEL_EPS: f32 = 1e-3;

/// A geometric defect found by [`audit_geometry`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeomIssue {
    /// Two distinct vertices within tolerance of each other.
    NearCoincidentVertices { a: VertKey, b: VertKey },
    /// A vertex lies on a line's interior without splitting it.
    UnsplitTJunction { line: LineKey, vertex: VertKey },
    /// Two collinear lines overlap along a span.
    OverlappingLines { a: LineKey, b: LineKey },
    /// No line references this vertex.
    OrphanVertex(VertKey),
    /// No line side references this sector.
    UnusedSector(SectorKey),
}

/// Scan for geometric defects; `tol` is the coincidence/on-line tolerance in world units.
pub fn audit_geometry(map: &EditorMap, tol: f32) -> Vec<GeomIssue> {
    let mut issues = Vec::new();
    near_coincident(map, tol, &mut issues);
    junctions_and_overlaps(map, tol, &mut issues);
    orphans(map, &mut issues);
    issues
}

/// Distinct vertex pairs within `tol`, each unordered pair reported once.
fn near_coincident(map: &EditorMap, tol: f32, out: &mut Vec<GeomIssue>) {
    let cell = |x: f32, y: f32| [(x / tol).floor() as i64, (y / tol).floor() as i64];
    let mut buckets: HashMap<[i64; 2], Vec<VertKey>> = HashMap::new();
    for (k, v) in map.vertices.iter() {
        buckets.entry(cell(v.x, v.y)).or_default().push(k);
    }
    let tol_sq = tol * tol;
    for (k, v) in map.vertices.iter() {
        let c = cell(v.x, v.y);
        for dx in -1..=1 {
            for dy in -1..=1 {
                let cell = [c[0].saturating_add(dx), c[1].saturating_add(dy)];
                for &j in buckets.get(&cell).map(Vec::as_slice).unwrap_or(&[]) {
                    if j <= k {
                        continue;
                    }
                    let w = map.vertices[j];
                    let d = (w.x - v.x).powi(2) + (w.y - v.y).powi(2);
                    if d <= tol_sq {
                        out.push(GeomIssue::NearCoincidentVertices {
                            a: k,
                            b: j,
                        });
                    }
                }
            }
        }
    }
}

/// Vertices on a foreign line's interior, and parallel line pairs overlapping by more than `tol`.
fn junctions_and_overlaps(map: &EditorMap, tol: f32, out: &mut Vec<GeomIssue>) {
    let lines: Vec<(LineKey, [f32; 2], [f32; 2])> = map
        .lines
        .keys()
        .map(|k| {
            let (p1, p2) = segment_points(map, k);
            (k, p1, p2)
        })
        .collect();
    for (vk, v) in map.vertices.iter() {
        for &(lk, p1, p2) in &lines {
            let (lo_x, hi_x) = (p1[0].min(p2[0]) - tol, p1[0].max(p2[0]) + tol);
            let (lo_y, hi_y) = (p1[1].min(p2[1]) - tol, p1[1].max(p2[1]) + tol);
            if v.x < lo_x || v.x > hi_x || v.y < lo_y || v.y > hi_y {
                continue;
            }
            let l = &map.lines[lk];
            if l.v1 == vk || l.v2 == vk {
                continue;
            }
            if point_on_segment_interior([v.x, v.y], p1, p2, tol).is_some() {
                out.push(GeomIssue::UnsplitTJunction {
                    line: lk,
                    vertex: vk,
                });
            }
        }
    }
    for (i, &(ka, a1, a2)) in lines.iter().enumerate() {
        let da = [a2[0] - a1[0], a2[1] - a1[1]];
        let len_a = da[0].hypot(da[1]);
        if len_a <= 0.0 {
            continue;
        }
        for &(kb, b1, b2) in &lines[i + 1..] {
            let db = [b2[0] - b1[0], b2[1] - b1[1]];
            let len_b = db[0].hypot(db[1]);
            if len_b <= 0.0 {
                continue;
            }
            let cross = da[0] * db[1] - da[1] * db[0];
            if cross.abs() / (len_a * len_b) >= PARALLEL_EPS {
                continue;
            }
            // Collinear when both of b's endpoints sit within tol of a's infinite line.
            let perp =
                |p: [f32; 2]| ((p[0] - a1[0]) * da[1] - (p[1] - a1[1]) * da[0]).abs() / len_a;
            if perp(b1) > tol || perp(b2) > tol {
                continue;
            }
            let param =
                |p: [f32; 2]| ((p[0] - a1[0]) * da[0] + (p[1] - a1[1]) * da[1]) / (len_a * len_a);
            let (t1, t2) = (param(b1), param(b2));
            let (lo, hi) = (t1.min(t2), t1.max(t2));
            let overlap = (hi.min(1.0) - lo.max(0.0)) * len_a;
            if overlap > tol {
                out.push(GeomIssue::OverlappingLines {
                    a: ka,
                    b: kb,
                });
            }
        }
    }
}

/// Vertices no line references and sectors no side references.
fn orphans(map: &EditorMap, out: &mut Vec<GeomIssue>) {
    let used_verts: HashSet<VertKey> = map.lines.values().flat_map(|l| [l.v1, l.v2]).collect();
    for (k, _) in map.vertices.iter() {
        if !used_verts.contains(&k) {
            out.push(GeomIssue::OrphanVertex(k));
        }
    }
    let used_sectors: HashSet<SectorKey> = map
        .lines
        .values()
        .flat_map(|l| l.sides().filter_map(|s| s.sector))
        .collect();
    for (k, _) in map.sectors.iter() {
        if !used_sectors.contains(&k) {
            out.push(GeomIssue::UnusedSector(k));
        }
    }
}

/// Repair geometric defects in place: weld near-coincident clusters, split T-junctions, fold overlapping collinear spans, drop degenerate lines, sync the TWO_SIDED flag to the back side, and prune orphans. Bounded fixpoint; returns the number of fixes applied.
pub fn heal_map(map: &mut EditorMap, tol: f32) -> usize {
    let mut fixes = 0;
    for _ in 0..MAX_HEAL_PASSES {
        let pass = heal_pass(map, tol);
        fixes += pass;
        if pass == 0 {
            break;
        }
    }
    fixes
}

fn heal_pass(map: &mut EditorMap, tol: f32) -> usize {
    let issues = audit_geometry(map, tol);
    let mut fixes = 0;

    // Weld each near-coincident cluster (union-find over the pairs) onto its centroid.
    let mut parent: HashMap<VertKey, VertKey> = HashMap::new();
    for issue in &issues {
        if let GeomIssue::NearCoincidentVertices {
            a,
            b,
        } = *issue
        {
            let (ra, rb) = (find(&parent, a), find(&parent, b));
            if ra != rb {
                parent.insert(ra.max(rb), ra.min(rb));
            }
        }
    }
    let mut clusters: HashMap<VertKey, Vec<VertKey>> = HashMap::new();
    for &v in parent.keys().chain(parent.values()) {
        clusters.entry(find(&parent, v)).or_default().push(v);
    }
    // Root-ordered so welds land deterministically.
    let mut clusters: Vec<(VertKey, Vec<VertKey>)> = clusters.into_iter().collect();
    clusters.sort_unstable_by_key(|(root, _)| *root);
    for (root, mut members) in clusters {
        if !members.contains(&root) {
            members.push(root);
        }
        members.sort_unstable();
        members.dedup();
        let pts: Vec<[f32; 2]> = members
            .iter()
            .filter_map(|&k| map.vertices.get(k).map(|v| [v.x, v.y]))
            .collect();
        if pts.len() < 2 {
            continue;
        }
        let n = pts.len() as f32;
        let target = pts
            .iter()
            .fold([0.0, 0.0], |a, p| [a[0] + p[0], a[1] + p[1]]);
        weld_vertices(map, &members, [target[0] / n, target[1] / n]);
        fixes += 1;
    }

    // Split T-junctions and overlapping spans, then fold the coincident twins.
    let mut targets: Vec<LineKey> = issues
        .iter()
        .flat_map(|i| match *i {
            GeomIssue::UnsplitTJunction {
                line,
                ..
            } => vec![line],
            GeomIssue::OverlappingLines {
                a,
                b,
            } => vec![a, b],
            _ => Vec::new(),
        })
        .collect();
    targets.sort_unstable();
    targets.dedup();
    targets.retain(|&k| map.lines.contains(k));
    if !targets.is_empty() {
        fixes += split_lines_at_intersections(map, &targets, tol).len();
        fixes += dedup_coincident_lines(map).len();
    }

    // Degenerate lines: same vertex or bit-identical coordinates.
    let degenerate: Vec<LineKey> = map
        .lines
        .iter()
        .filter(|(_, l)| {
            l.v1 == l.v2 || {
                let (p1, p2) = (map.vertices[l.v1], map.vertices[l.v2]);
                p1.x.to_bits() == p2.x.to_bits() && p1.y.to_bits() == p2.y.to_bits()
            }
        })
        .map(|(k, _)| k)
        .collect();
    fixes += degenerate.len();
    map.remove_lines(&degenerate);

    // TWO_SIDED must mirror the back side's presence.
    for l in map.lines.values_mut() {
        let want = l.back.is_some();
        if l.flags.contains(LineFlags::TWO_SIDED) != want {
            l.flags.set(LineFlags::TWO_SIDED, want);
            fixes += 1;
        }
    }

    fixes += map.prune_orphan_vertices();
    fixes += map.prune_unused_sectors();
    fixes
}

fn find(parent: &HashMap<VertKey, VertKey>, mut x: VertKey) -> VertKey {
    while let Some(&p) = parent.get(&x) {
        if p == x {
            break;
        }
        x = p;
    }
    x
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::sector_at;
    use crate::model::{DenseLineDef, Vertex};
    use crate::sector_build::{VoidRule, build_sectors};
    use crate::test_fixtures::{def_sector, dline_with, fixture, line_keys, vtx};

    fn dline(v1: u32, v2: u32) -> DenseLineDef {
        dline_with(v1, v2, LineFlags::empty(), None)
    }

    #[test]
    fn clean_square_audits_empty() {
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
        assert_eq!(audit_geometry(&map, 1.0), Vec::new());
        assert_eq!(heal_map(&mut map, 1.0), 0, "clean map needs no fixes");
    }

    #[test]
    fn audit_finds_seeded_defects() {
        // Wall + unsplit stem foot, near-coincident pair, collinear overlap, orphan vertex, unused sector.
        let mut map = fixture(
            vec![
                vtx(0.0, 0.0),
                vtx(64.0, 0.0),
                vtx(32.0, 0.0),
                vtx(32.0, 32.0),
                vtx(100.0, 0.0),
                vtx(100.5, 0.2),
                vtx(200.0, 0.0),
                vtx(264.0, 0.0),
                vtx(232.0, 0.0),
                vtx(296.0, 0.0),
            ],
            vec![dline(0, 1), dline(2, 3), dline(6, 7), dline(8, 9)],
            1,
        );
        map.vertices.insert(Vertex {
            x: 500.0,
            y: 500.0,
        });
        let issues = audit_geometry(&map, 1.0);
        let has = |f: &dyn Fn(&GeomIssue) -> bool| issues.iter().any(f);
        assert!(
            has(&|i| matches!(i, GeomIssue::UnsplitTJunction { .. })),
            "stem foot on wall interior: {issues:?}"
        );
        assert!(
            has(&|i| matches!(i, GeomIssue::NearCoincidentVertices { .. })),
            "pair 0.5 apart: {issues:?}"
        );
        assert!(
            has(&|i| matches!(i, GeomIssue::OverlappingLines { .. })),
            "collinear overlap: {issues:?}"
        );
        assert!(
            has(&|i| matches!(i, GeomIssue::OrphanVertex(_))),
            "floating vertex: {issues:?}"
        );
        assert!(
            has(&|i| matches!(i, GeomIssue::UnusedSector(_))),
            "unreferenced sector: {issues:?}"
        );
    }

    #[test]
    fn heal_fixes_all_seeded_defects() {
        let mut map = fixture(
            vec![
                vtx(0.0, 0.0),
                vtx(64.0, 0.0),
                vtx(32.0, 0.0),
                vtx(32.0, 32.0),
                vtx(200.0, 0.0),
                vtx(264.0, 0.0),
                vtx(232.0, 0.0),
                vtx(296.0, 0.0),
            ],
            vec![dline(0, 1), dline(2, 3), dline(4, 5), dline(6, 7)],
            1,
        );
        map.vertices.insert(Vertex {
            x: 500.0,
            y: 500.0,
        });
        assert!(heal_map(&mut map, 1.0) > 0);
        assert_eq!(audit_geometry(&map, 1.0), Vec::new(), "re-audit clean");
        assert!(map.sectors.is_empty(), "unused sector pruned");
    }

    #[test]
    fn heal_syncs_two_sided_flag_and_drops_degenerate() {
        let mut map = fixture(
            vec![vtx(0.0, 0.0), vtx(64.0, 0.0), vtx(64.0, 0.0)],
            vec![dline_with(0, 1, LineFlags::TWO_SIDED, None), dline(1, 2)],
            0,
        );
        assert!(heal_map(&mut map, 1.0) > 0);
        assert_eq!(map.lines.len(), 1, "degenerate line dropped");
        let l = map.lines.values().next().expect("line");
        assert!(
            !l.flags.contains(LineFlags::TWO_SIDED),
            "flag cleared without back side"
        );
    }

    #[test]
    fn heal_splits_t_junction_and_keeps_sector() {
        // Sectored square with a stem foot resting unsplit on the top wall.
        let mut map = fixture(
            vec![
                vtx(0.0, 0.0),
                vtx(64.0, 0.0),
                vtx(64.0, 64.0),
                vtx(0.0, 64.0),
                vtx(32.0, 64.0),
                vtx(32.0, 96.0),
            ],
            vec![
                dline(0, 1),
                dline(1, 2),
                dline(2, 3),
                dline(3, 0),
                dline(4, 5),
            ],
            0,
        );
        let keys = line_keys(&map);
        build_sectors(&mut map, &keys[..4], def_sector(), VoidRule::KeepPockets);
        let s = sector_at(&map, [32.0, 32.0]).expect("square sectored");
        assert!(heal_map(&mut map, 1.0) > 0);
        assert_eq!(map.lines.len(), 6, "top wall split at the stem foot");
        assert_eq!(audit_geometry(&map, 1.0), Vec::new());
        assert_eq!(sector_at(&map, [32.0, 32.0]), Some(s), "sector survives");
    }
}
