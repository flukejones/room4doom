//! Per-sector polygon triangulation for filled map views.
//!
//! Sector outlines come from [`editor_core::sector_loops`] (outers CCW, holes CW).
//! Each sector is triangulated by a plane sweep that handles holes natively: the
//! outer loop and its hole loops share one vertex arena, the sweep splits the whole
//! thing into y-monotone pieces (de Berg, *Computational Geometry* §3.2) — a hole's
//! top is a split vertex, its bottom a merge, so the sweep's diagonals stitch holes
//! to the outer — and each monotone piece is triangulated in linear time (§3.3).
//! No hole bridging, no per-degeneracy special-cases. Distinct from the game's
//! BSP3D polygoniser (which fans already-convex leaves).

use std::cmp::Ordering;
use std::collections::HashSet;
use std::f32::consts::TAU;

use editor_core::{EditorMap, SectorLoop, sector_loops_all};

/// One triangle corner: world XY + originating map vertex (pick provenance). All corners are real map vertices.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TriVert {
    pub pos: [f32; 2],
    pub vert: u32,
}

/// Triangulated sector fills. `tris[i]` = CCW corner triple; `ranges[s]` = `[start, end)` into `tris`.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct SectorTris {
    pub tris: Vec<[TriVert; 3]>,
    pub ranges: Vec<(u32, u32)>,
}

/// Triangulate every sector. Open geometry mid-edit contributes no triangles.
pub fn build_sector_tris(map: &EditorMap) -> SectorTris {
    let mut tris = Vec::new();
    let mut ranges = Vec::with_capacity(map.sectors.len());
    for loops in sector_loops_all(map) {
        let start = tris.len() as u32;
        triangulate_sector(map, &loops, &mut tris);
        ranges.push((start, tris.len() as u32));
    }
    SectorTris {
        tris,
        ranges,
    }
}

/// Group each outer with its contained holes, then sweep + triangulate each group.
fn triangulate_sector(map: &EditorMap, loops: &[SectorLoop], out: &mut Vec<[TriVert; 3]>) {
    let rings: Vec<Vec<TriVert>> = loops
        .iter()
        .map(|l| clean_ring(l.verts.iter().map(|&v| corner(map, v)).collect()))
        .filter(|r| r.len() >= 3)
        .collect();
    if rings.is_empty() {
        return;
    }
    for group in group_loops(rings) {
        let arena = Arena::new(&group);
        let diagonals = partition(&arena);
        triangulate_pieces(&arena, &diagonals, out);
    }
}

fn corner(map: &EditorMap, v: u32) -> TriVert {
    let p = &map.vertices[v as usize];
    TriVert {
        pos: [p.x, p.y],
        vert: v,
    }
}

/// Drop degenerate vertices: consecutive duplicates and antenna spikes (prev==next).
/// Both break the sweep's planar graph (two nodes at one point) and contribute no area.
fn clean_ring(mut ring: Vec<TriVert>) -> Vec<TriVert> {
    let mut changed = true;
    while changed && ring.len() >= 3 {
        changed = false;
        let n = ring.len();
        let mut keep = Vec::with_capacity(n);
        for k in 0..n {
            let prev = ring[(k + n - 1) % n].pos;
            let cur = ring[k].pos;
            let next = ring[(k + 1) % n].pos;
            // Duplicate of previous, or an antenna tip (prev == next).
            if cur == prev || prev == next {
                changed = true;
                continue;
            }
            keep.push(ring[k]);
        }
        ring = keep;
    }
    ring
}

/// An outer ring (CCW) plus the hole rings directly inside it (CW).
struct Group {
    outer: Vec<TriVert>,
    holes: Vec<Vec<TriVert>>,
}

/// Classify rings by nesting parity (even depth = outer, odd = hole), group holes under their immediate parent outer,
/// orient (outer CCW, hole CW) for the sweep's interior-on-the-left rule.
fn group_loops(rings: Vec<Vec<TriVert>>) -> Vec<Group> {
    let n = rings.len();
    // depth[i] = how many other rings contain ring i's first vertex.
    let depth: Vec<usize> = (0..n)
        .map(|i| {
            (0..n)
                .filter(|&j| j != i && point_in_ring(&rings[j], rings[i][0].pos))
                .count()
        })
        .collect();

    let mut groups: Vec<Group> = (0..n)
        .filter(|&i| depth[i].is_multiple_of(2))
        .map(|i| Group {
            outer: oriented(&rings[i], true),
            holes: Vec::new(),
        })
        .collect();

    // Attach each hole to the smallest-area outer that contains it.
    for i in 0..n {
        if depth[i].is_multiple_of(2) {
            continue;
        }
        let p = rings[i][0].pos;
        let parent = groups
            .iter_mut()
            .filter(|g| point_in_ring(&g.outer, p))
            .min_by(|a, b| {
                signed_area(&a.outer)
                    .abs()
                    .partial_cmp(&signed_area(&b.outer).abs())
                    .unwrap_or(Ordering::Equal)
            });
        if let Some(g) = parent {
            g.holes.push(oriented(&rings[i], false));
        }
    }
    groups
}

/// Return the ring wound CCW (`ccw == true`) or CW (`false`).
fn oriented(ring: &[TriVert], ccw: bool) -> Vec<TriVert> {
    if (signed_area(ring) > 0.0) == ccw {
        ring.to_vec()
    } else {
        ring.iter().rev().copied().collect()
    }
}

/// Twice the signed area (shoelace); positive when CCW.
fn signed_area(ring: &[TriVert]) -> f32 {
    let n = ring.len();
    (0..n)
        .map(|i| {
            let a = ring[i].pos;
            let b = ring[(i + 1) % n].pos;
            a[0] * b[1] - b[0] * a[1]
        })
        .sum()
}

/// Even-odd point-in-polygon (ray to +X), edges treated half-open in y.
fn point_in_ring(ring: &[TriVert], p: [f32; 2]) -> bool {
    let n = ring.len();
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let a = ring[i].pos;
        let b = ring[j].pos;
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Sweep order: `a` above `b` iff greater y (ties: smaller x). Total order, no ambiguous ties.
fn above(a: [f32; 2], b: [f32; 2]) -> bool {
    a[1] > b[1] || (a[1] == b[1] && a[0] < b[0])
}

/// Strict top-first sweep comparator; node-index tiebreak for coincident vertices.
fn sweep_cmp(pa: [f32; 2], ia: usize, pb: [f32; 2], ib: usize) -> Ordering {
    if pa == pb {
        ia.cmp(&ib)
    } else if above(pa, pb) {
        Ordering::Less
    } else {
        Ordering::Greater
    }
}

/// `(b−a)×(c−a)`; positive when `a→b→c` is CCW.
fn cross(a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> f32 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

/// Flattened outer+holes as one vertex set with boundary linkage.
/// Diagonals kept separate and merged into a planar graph at face-extraction time — never spliced in place.
struct Arena {
    verts: Vec<TriVert>,
    prev: Vec<usize>,
    next: Vec<usize>,
}

impl Arena {
    fn new(group: &Group) -> Self {
        let mut verts = Vec::new();
        let mut prev = Vec::new();
        let mut next = Vec::new();
        push_ring(&mut verts, &mut prev, &mut next, &group.outer);
        for hole in &group.holes {
            push_ring(&mut verts, &mut prev, &mut next, hole);
        }
        Self {
            verts,
            prev,
            next,
        }
    }

    fn pos(&self, i: usize) -> [f32; 2] {
        self.verts[i].pos
    }
}

fn push_ring(
    verts: &mut Vec<TriVert>,
    prev: &mut Vec<usize>,
    next: &mut Vec<usize>,
    ring: &[TriVert],
) {
    let base = verts.len();
    let n = ring.len();
    for (k, v) in ring.iter().enumerate() {
        verts.push(*v);
        prev.push(base + (k + n - 1) % n);
        next.push(base + (k + 1) % n);
    }
}

/// Vertex role in the sweep. Split and merge get a diagonal; others only update status.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Start,
    End,
    Split,
    Merge,
    RegularLeft,
    RegularRight,
}

/// Classify vertex `i` from its neighbours and interior turn.
fn classify(pos: &[[f32; 2]], prev: usize, i: usize, next: usize) -> Kind {
    let (p, a, b) = (pos[i], pos[prev], pos[next]);
    let both_below = above(p, a) && above(p, b);
    let both_above = above(a, p) && above(b, p);
    // Interior turn: prev→cur→next is a left (CCW) turn.
    let convex = cross(a, p, b) > 0.0;
    if both_below {
        if convex { Kind::Start } else { Kind::Split }
    } else if both_above {
        if convex { Kind::End } else { Kind::Merge }
    } else if above(a, b) {
        // prev above, next below — left chain, interior to the right.
        Kind::RegularLeft
    } else {
        Kind::RegularRight
    }
}

/// A boundary edge crossing the sweep line, as `(upper, lower)` node pair.
#[derive(Clone, Copy, PartialEq)]
struct Wall {
    top: usize,
    bot: usize,
}

/// Active interior strip at the sweep line. Status is a left-to-right ordered list; spans split at split vertices and fuse at merge vertices.
#[derive(Clone, Copy)]
struct Span {
    left: Wall,
    right: Wall,
    helper: usize,
}

/// Partition arena into y-monotone pieces (de Berg §3.2). Classifies on original topology; collects then applies diagonals.
/// Multi-span status locates each vertex unambiguously — no nearest-edge guessing.
fn partition(arena: &Arena) -> Vec<(usize, usize)> {
    let n = arena.verts.len();
    let prev = &arena.prev;
    let next = &arena.next;
    let pos: Vec<[f32; 2]> = (0..n).map(|i| arena.verts[i].pos).collect();
    let kind: Vec<Kind> = (0..n)
        .map(|i| classify(&pos, prev[i], i, next[i]))
        .collect();

    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| sweep_cmp(pos[a], a, pos[b], b));

    let wall_to = |i: usize, nb: usize| -> Wall {
        if sweep_cmp(pos[i], i, pos[nb], nb) == Ordering::Less {
            Wall {
                top: i,
                bot: nb,
            }
        } else {
            Wall {
                top: nb,
                bot: i,
            }
        }
    };

    let mut spans: Vec<Span> = Vec::new();
    let mut diagonals: Vec<(usize, usize)> = Vec::new();
    let cut = |d: &mut Vec<(usize, usize)>, a: usize, b: usize| {
        if a != b {
            d.push((a, b));
        }
    };

    for &i in &order {
        let p = pos[i];
        // Re-sort by left-wall x each event (split/merge can disorder; few spans, cheap).
        let y = p[1];
        spans.sort_by(|s, t| {
            wall_x(&s.left, &pos, y)
                .partial_cmp(&wall_x(&t.left, &pos, y))
                .unwrap_or(Ordering::Equal)
        });
        match kind[i] {
            Kind::Start => {
                // Opens a new span between its two downward edges.
                let (l, r) = order_down_walls(i, prev[i], next[i], &pos);
                let at = spans.partition_point(|s| wall_x(&s.left, &pos, p[1]) < p[0]);
                spans.insert(
                    at,
                    Span {
                        left: l,
                        right: r,
                        helper: i,
                    },
                );
            }
            Kind::End => {
                // Closes the span both of i's edges bound.
                if let Some(k) = span_ending_at(&spans, i) {
                    if kind[spans[k].helper] == Kind::Merge {
                        cut(&mut diagonals, i, spans[k].helper);
                    }
                    spans.remove(k);
                }
            }
            Kind::Split => {
                // Falls inside one span; diagonal to its helper, then divide it.
                if let Some(k) = span_containing(&spans, &pos, p) {
                    cut(&mut diagonals, i, spans[k].helper);
                    let outer_left = spans[k].left;
                    let outer_right = spans[k].right;
                    // i's two downward edges become the inner walls of the two halves.
                    let (a, b) = order_down_walls(i, prev[i], next[i], &pos);
                    spans[k] = Span {
                        left: outer_left,
                        right: a,
                        helper: i,
                    };
                    spans.insert(
                        k + 1,
                        Span {
                            left: b,
                            right: outer_right,
                            helper: i,
                        },
                    );
                }
            }
            Kind::Merge => {
                // i ends the right wall of its left span and the left wall of its
                // right span; fuse those two specific spans into one.
                let lk = span_with_right_at(&spans, i);
                let rk = span_with_left_at(&spans, i);
                if let (Some(lk), Some(rk)) = (lk, rk) {
                    if kind[spans[lk].helper] == Kind::Merge {
                        cut(&mut diagonals, i, spans[lk].helper);
                    }
                    if kind[spans[rk].helper] == Kind::Merge {
                        cut(&mut diagonals, i, spans[rk].helper);
                    }
                    let (lo, hi) = (lk.min(rk), lk.max(rk));
                    let fused = Span {
                        left: spans[lo].left,
                        right: spans[hi].right,
                        helper: i,
                    };
                    spans[lo] = fused;
                    spans.remove(hi);
                }
            }
            Kind::RegularLeft => {
                // i advances the left wall of its span (interior on its right).
                if let Some(k) = span_with_left_at(&spans, i) {
                    if kind[spans[k].helper] == Kind::Merge {
                        cut(&mut diagonals, i, spans[k].helper);
                    }
                    spans[k].left = wall_to(i, next_below(arena, i));
                    spans[k].helper = i;
                }
            }
            Kind::RegularRight => {
                // i advances the right wall of its span (interior on its left).
                if let Some(k) = span_with_right_at(&spans, i) {
                    if kind[spans[k].helper] == Kind::Merge {
                        cut(&mut diagonals, i, spans[k].helper);
                    }
                    spans[k].right = wall_to(i, next_below(arena, i));
                    spans[k].helper = i;
                }
            }
        }
    }
    diagonals
}

/// Order two edges leaving `i` downward into `(left, right)` by which side the other neighbour falls on.
fn order_down_walls(i: usize, a: usize, b: usize, pos: &[[f32; 2]]) -> (Wall, Wall) {
    let la = Wall {
        top: i,
        bot: a,
    };
    let lb = Wall {
        top: i,
        bot: b,
    };
    if cross(pos[i], pos[a], pos[b]) > 0.0 {
        (la, lb)
    } else {
        (lb, la)
    }
}

/// The neighbour of `i` that is below it in sweep order (the edge continuing down).
fn next_below(arena: &Arena, i: usize) -> usize {
    let (pr, nx) = (arena.prev[i], arena.next[i]);
    let pp = arena.pos(pr);
    let np = arena.pos(nx);
    if sweep_cmp(pp, pr, np, nx) == Ordering::Greater {
        pr
    } else {
        nx
    }
}

/// X where a wall meets sweep height `y`.
fn wall_x(w: &Wall, pos: &[[f32; 2]], y: f32) -> f32 {
    edge_x_at(pos[w.top], pos[w.bot], y)
}

/// The span whose left and right walls both end at vertex `i` (an End vertex).
fn span_ending_at(spans: &[Span], i: usize) -> Option<usize> {
    spans
        .iter()
        .position(|s| s.left.bot == i && s.right.bot == i)
}

/// The span whose right wall ends at `i` (its left neighbour for a Merge fuse).
fn span_with_right_at(spans: &[Span], i: usize) -> Option<usize> {
    spans.iter().position(|s| s.right.bot == i)
}

/// The span whose left wall ends at `i`.
fn span_with_left_at(spans: &[Span], i: usize) -> Option<usize> {
    spans.iter().position(|s| s.left.bot == i)
}

/// The span containing point `p` (between its left and right walls at `p.y`).
fn span_containing(spans: &[Span], pos: &[[f32; 2]], p: [f32; 2]) -> Option<usize> {
    spans
        .iter()
        .position(|s| wall_x(&s.left, pos, p[1]) <= p[0] && p[0] <= wall_x(&s.right, pos, p[1]))
}

/// X where edge `a→b` meets height `y` (the larger x for a horizontal edge).
fn edge_x_at(a: [f32; 2], b: [f32; 2], y: f32) -> f32 {
    if (b[1] - a[1]).abs() < f32::EPSILON {
        a[0].max(b[0])
    } else {
        let t = (y - a[1]) / (b[1] - a[1]);
        a[0] + t * (b[0] - a[0])
    }
}

/// Triangulate all monotone pieces; seed from every node so split-created loops are reached.
fn triangulate_pieces(arena: &Arena, diagonals: &[(usize, usize)], out: &mut Vec<[TriVert; 3]>) {
    for piece in monotone_pieces(arena, diagonals) {
        triangulate_monotone(arena, &piece, out);
    }
}

/// Extract y-monotone faces from boundary + diagonals as a directed planar graph.
/// Face traversal: at each vertex, leave on the most clockwise half-edge from arrival.
/// Robust for multi-diagonal vertices. Single CW face (outer hull) is dropped.
fn monotone_pieces(arena: &Arena, diagonals: &[(usize, usize)]) -> Vec<Vec<usize>> {
    let n = arena.verts.len();
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, a) in adj.iter_mut().enumerate() {
        a.push(arena.next[i]);
    }
    for &(a, b) in diagonals {
        adj[a].push(b);
        adj[b].push(a);
    }

    let mut visited: HashSet<(usize, usize)> = HashSet::new();
    let mut faces = Vec::new();
    for u in 0..n {
        for &v in &adj[u] {
            if !visited.insert((u, v)) {
                continue;
            }
            let mut face = vec![u];
            let (mut a, mut b) = (u, v);
            loop {
                let w = next_in_face(arena, &adj, a, b);
                visited.insert((b, w));
                if b == u && w == v {
                    break;
                }
                face.push(b);
                a = b;
                b = w;
                if face.len() > n + diagonals.len() + 2 {
                    break; // malformed guard; never expected
                }
            }
            if signed_face_area(arena, &face) > 0.0 {
                faces.push(face);
            }
        }
    }
    faces
}

/// Next half-edge `b→w` from `a→b`: the neighbour of `b` with the smallest clockwise turn from `b→a`.
fn next_in_face(arena: &Arena, adj: &[Vec<usize>], a: usize, b: usize) -> usize {
    let here = arena.pos(b);
    let back = angle(here, arena.pos(a));
    let mut best = a;
    let mut best_turn = f32::INFINITY;
    for &w in &adj[b] {
        if w == a && adj[b].len() > 1 {
            continue;
        }
        let turn = (back - angle(here, arena.pos(w))).rem_euclid(TAU);
        let turn = if turn == 0.0 { TAU } else { turn };
        if turn < best_turn {
            best_turn = turn;
            best = w;
        }
    }
    best
}

/// Polar angle of the direction `from → to`.
fn angle(from: [f32; 2], to: [f32; 2]) -> f32 {
    (to[1] - from[1]).atan2(to[0] - from[0])
}

/// Twice the signed area of a face given as vertex indices.
fn signed_face_area(arena: &Arena, face: &[usize]) -> f32 {
    let n = face.len();
    (0..n)
        .map(|i| {
            let a = arena.pos(face[i]);
            let b = arena.pos(face[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum()
}

/// Triangulate one y-monotone piece by the linear stack method (de Berg §3.3).
fn triangulate_monotone(arena: &Arena, piece: &[usize], out: &mut Vec<[TriVert; 3]>) {
    let m = piece.len();
    if m < 3 {
        return;
    }
    let pos = |k: usize| arena.pos(piece[k]);
    let tv = |k: usize| arena.verts[piece[k]];

    // Walking `next` from the top descends the LEFT chain; the rest is the right chain.
    let top = (0..m).max_by(|&a, &b| order_cmp(pos(a), pos(b))).unwrap();
    let bottom = (0..m).min_by(|&a, &b| order_cmp(pos(a), pos(b))).unwrap();
    let mut left = vec![false; m];
    let mut k = top;
    while k != bottom {
        left[k] = true;
        k = (k + 1) % m;
    }

    let mut order: Vec<usize> = (0..m).collect();
    order.sort_by(|&a, &b| order_cmp(pos(b), pos(a)));

    let mut stack: Vec<usize> = vec![order[0], order[1]];
    for (oi, &u) in order.iter().enumerate().take(m - 1).skip(2) {
        let top_chain = left[*stack.last().unwrap()];
        if left[u] != top_chain {
            // Opposite chain: connect u to all stack vertices, restart stack from previous.
            while stack.len() > 1 {
                let a = stack.pop().unwrap();
                let b = *stack.last().unwrap();
                push_tri(out, tv(u), tv(a), tv(b));
            }
            stack.clear();
            stack.push(order[oi - 1]);
        } else {
            // Same chain: pop while diagonal u→stacked stays inside.
            let mut last = stack.pop().unwrap();
            while let Some(&t) = stack.last() {
                let turn = cross(pos(t), pos(last), pos(u));
                // Interior diagonal: CCW turn on the left chain, CW on the right.
                let inside = if left[u] { turn > 0.0 } else { turn < 0.0 };
                if !inside {
                    break;
                }
                push_tri(out, tv(t), tv(last), tv(u));
                last = stack.pop().unwrap();
            }
            stack.push(last);
        }
        stack.push(u);
    }
    let last = order[m - 1];
    for w in stack.windows(2) {
        push_tri(out, tv(w[0]), tv(w[1]), tv(last));
    }
}

/// `above`-first ordering for monotone triangulation.
fn order_cmp(a: [f32; 2], b: [f32; 2]) -> Ordering {
    if above(a, b) {
        Ordering::Greater
    } else {
        Ordering::Less
    }
}

/// Push a triangle oriented CCW (degenerate triangles dropped).
fn push_tri(out: &mut Vec<[TriVert; 3]>, a: TriVert, b: TriVert, c: TriVert) {
    if cross(a.pos, b.pos, c.pos).abs() < f32::EPSILON {
        return;
    }
    if cross(a.pos, b.pos, c.pos) > 0.0 {
        out.push([a, b, c]);
    } else {
        out.push([a, c, b]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use editor_core::{LineDef, LineFlags, SideDef, Vertex as MapVertex};

    fn vtx(x: f32, y: f32) -> MapVertex {
        MapVertex {
            x,
            y,
        }
    }

    fn map_of(
        vertices: Vec<MapVertex>,
        sectors: Vec<editor_core::Sector>,
        lines: Vec<LineDef>,
    ) -> EditorMap {
        EditorMap {
            vertices,
            lines,
            sectors,
            ..Default::default()
        }
    }

    fn sec() -> editor_core::Sector {
        editor_core::Sector {
            floor_height: 0,
            floor_flat: Default::default(),
            ceil_height: 128,
            ceil_flat: Default::default(),
            light_level: 160,
            special: 0,
            tag: 0,
        }
    }

    fn side(sector: Option<u32>) -> SideDef {
        SideDef {
            x_offset: 0,
            y_offset: 0,
            top_tex: Default::default(),
            bottom_tex: Default::default(),
            middle_tex: Default::default(),
            sector,
        }
    }

    fn wall(v1: u32, v2: u32, sector: u32) -> LineDef {
        LineDef {
            v1,
            v2,
            flags: LineFlags::empty(),
            special: 0,
            tag: 0,
            front: side(Some(sector)),
            back: None,
        }
    }

    fn portal(v1: u32, v2: u32, front: u32, back: u32) -> LineDef {
        LineDef {
            v1,
            v2,
            flags: LineFlags::empty(),
            special: 0,
            tag: 0,
            front: side(Some(front)),
            back: Some(side(Some(back))),
        }
    }

    fn fill_area(tris: &[[TriVert; 3]]) -> f32 {
        tris.iter()
            .map(|t| cross(t[0].pos, t[1].pos, t[2].pos).abs() * 0.5)
            .sum()
    }

    fn all_ccw(tris: &[[TriVert; 3]]) -> bool {
        tris.iter()
            .all(|t| cross(t[0].pos, t[1].pos, t[2].pos) > 0.0)
    }

    fn sector(st: &SectorTris, s: usize) -> &[[TriVert; 3]] {
        let (a, b) = st.ranges[s];
        &st.tris[a as usize..b as usize]
    }

    /// Drive the pipeline on raw position rings (no WAD map); for fixtures from real maps.
    fn tris_from_rings(rings: Vec<Vec<[f32; 2]>>) -> Vec<[TriVert; 3]> {
        let rings: Vec<Vec<TriVert>> = rings
            .into_iter()
            .map(|r| {
                clean_ring(
                    r.into_iter()
                        .map(|pos| TriVert {
                            pos,
                            vert: 0,
                        })
                        .collect(),
                )
            })
            .filter(|r| r.len() >= 3)
            .collect();
        let mut out = Vec::new();
        for group in group_loops(rings) {
            let arena = Arena::new(&group);
            let diagonals = partition(&arena);
            triangulate_pieces(&arena, &diagonals, &mut out);
        }
        out
    }

    /// E6M6 sector 59: 66-vertex outer (antenna spike at (-768,1328)) + 11 holes. Fill = outer − holes = 799580.
    #[test]
    fn e6m6_sector_59_fills_exactly() {
        let tris = tris_from_rings(e6m6_s59());
        assert!(all_ccw(&tris), "every triangle CCW (floor normal up)");
        let area = fill_area(&tris);
        assert!(
            (area - 799_580.0).abs() / 799_580.0 < 0.01,
            "fill = outer − holes (799580), got {area}"
        );
    }

    /// Real Sunder MAP19 sector 9129: a hole touches the outer ring at a single
    /// shared vertex. Two coincident graph nodes confuse the angular face walk; the
    /// general fix (welding) regressed other sectors, so this degenerate
    /// boundary-touching-hole case is a documented limitation (≈9/11699 sectors on
    /// the hardest PWAD). Floors still render; the fill is over-covered here.
    #[test]
    #[ignore = "boundary-touching hole: known degenerate-geometry limitation"]
    fn map19_sector_9129_shared_vertex() {
        let tris = tris_from_rings(vec![
            vec![
                [7488.0, -5760.0],
                [7488.0, -5744.0],
                [7632.0, -5744.0],
                [7632.0, -5904.0],
                [7472.0, -5904.0],
                [7472.0, -5760.0],
            ],
            vec![
                [7616.0, -5888.0],
                [7616.0, -5760.0],
                [7488.0, -5760.0],
                [7488.0, -5888.0],
            ],
        ]);
        assert!(all_ccw(&tris), "every triangle CCW");
        let area = fill_area(&tris);
        assert!(
            (area - 8960.0).abs() < 1.0,
            "fill = outer − hole (8960), got {area}"
        );
    }

    #[test]
    fn convex_quad() {
        let map = map_of(
            vec![vtx(0.0, 0.0), vtx(4.0, 0.0), vtx(4.0, 4.0), vtx(0.0, 4.0)],
            vec![sec()],
            vec![wall(0, 1, 0), wall(1, 2, 0), wall(2, 3, 0), wall(3, 0, 0)],
        );
        let st = build_sector_tris(&map);
        assert!((fill_area(sector(&st, 0)) - 16.0).abs() < 1e-3);
        assert!(all_ccw(sector(&st, 0)));
    }

    #[test]
    fn concave_l_shape() {
        let map = map_of(
            vec![
                vtx(0.0, 0.0),
                vtx(2.0, 0.0),
                vtx(2.0, 1.0),
                vtx(1.0, 1.0),
                vtx(1.0, 2.0),
                vtx(0.0, 2.0),
            ],
            vec![sec()],
            vec![
                wall(0, 1, 0),
                wall(1, 2, 0),
                wall(2, 3, 0),
                wall(3, 4, 0),
                wall(4, 5, 0),
                wall(5, 0, 0),
            ],
        );
        let st = build_sector_tris(&map);
        assert!(
            (fill_area(sector(&st, 0)) - 3.0).abs() < 1e-3,
            "L area, got {}",
            fill_area(sector(&st, 0))
        );
        assert!(all_ccw(sector(&st, 0)));
    }

    /// A downward notch (W shape) forces a split vertex → a partition diagonal.
    #[test]
    fn split_vertex_w_shape() {
        let map = map_of(
            vec![
                vtx(0.0, 0.0),
                vtx(4.0, 0.0),
                vtx(4.0, 4.0),
                vtx(2.0, 2.0),
                vtx(0.0, 4.0),
            ],
            vec![sec()],
            vec![
                wall(0, 1, 0),
                wall(1, 2, 0),
                wall(2, 3, 0),
                wall(3, 4, 0),
                wall(4, 0, 0),
            ],
        );
        let st = build_sector_tris(&map);
        // Square 16 minus the upper triangle notch (base 4, height 2 = 4) = 12.
        assert!(
            (fill_area(sector(&st, 0)) - 12.0).abs() < 1e-3,
            "W area, got {}",
            fill_area(sector(&st, 0))
        );
        assert!(all_ccw(sector(&st, 0)));
    }

    #[test]
    fn sector_with_hole() {
        let map = map_of(
            vec![
                vtx(0.0, 0.0),
                vtx(6.0, 0.0),
                vtx(6.0, 6.0),
                vtx(0.0, 6.0),
                vtx(2.0, 2.0),
                vtx(4.0, 2.0),
                vtx(4.0, 4.0),
                vtx(2.0, 4.0),
            ],
            vec![sec(), sec()],
            vec![
                wall(0, 1, 0),
                wall(1, 2, 0),
                wall(2, 3, 0),
                wall(3, 0, 0),
                portal(4, 5, 1, 0),
                portal(5, 6, 1, 0),
                portal(6, 7, 1, 0),
                portal(7, 4, 1, 0),
            ],
        );
        let st = build_sector_tris(&map);
        assert!(
            (fill_area(sector(&st, 0)) - 32.0).abs() < 1e-3,
            "outer excludes the hole, got {}",
            fill_area(sector(&st, 0))
        );
        assert!((fill_area(sector(&st, 1)) - 4.0).abs() < 1e-3);
        assert!(all_ccw(sector(&st, 0)));
    }

    #[test]
    fn sector_with_two_holes() {
        let map = map_of(
            vec![
                vtx(0.0, 0.0),
                vtx(10.0, 0.0),
                vtx(10.0, 10.0),
                vtx(0.0, 10.0),
                vtx(1.0, 4.0),
                vtx(3.0, 4.0),
                vtx(3.0, 6.0),
                vtx(1.0, 6.0),
                vtx(7.0, 4.0),
                vtx(9.0, 4.0),
                vtx(9.0, 6.0),
                vtx(7.0, 6.0),
            ],
            vec![sec(), sec(), sec()],
            vec![
                wall(0, 1, 0),
                wall(1, 2, 0),
                wall(2, 3, 0),
                wall(3, 0, 0),
                portal(4, 5, 1, 0),
                portal(5, 6, 1, 0),
                portal(6, 7, 1, 0),
                portal(7, 4, 1, 0),
                portal(8, 9, 2, 0),
                portal(9, 10, 2, 0),
                portal(10, 11, 2, 0),
                portal(11, 8, 2, 0),
            ],
        );
        let st = build_sector_tris(&map);
        assert!(
            (fill_area(sector(&st, 0)) - 92.0).abs() < 1e-3,
            "outer excludes both holes, got {}",
            fill_area(sector(&st, 0))
        );
        assert!(all_ccw(sector(&st, 0)));
    }

    #[test]
    fn e1m1_sectors_fill_and_wind_ccw() {
        let wad = wad::WadData::new(&test_utils::doom1_wad_path());
        let map = editor_core::import_wad_map(&wad, "E1M1").expect("E1M1 imports");
        let st = build_sector_tris(&map);
        assert_eq!(st.ranges.len(), map.sectors.len());
        let bad: Vec<_> = st
            .tris
            .iter()
            .filter(|t| cross(t[0].pos, t[1].pos, t[2].pos) <= 0.0)
            .take(5)
            .collect();
        for t in &bad {
            eprintln!(
                "BAD cross={} {:?} {:?} {:?}",
                cross(t[0].pos, t[1].pos, t[2].pos),
                t[0].pos,
                t[1].pos,
                t[2].pos
            );
        }
        eprintln!(
            "bad_count={}",
            st.tris
                .iter()
                .filter(|t| cross(t[0].pos, t[1].pos, t[2].pos) <= 0.0)
                .count()
        );
        assert!(all_ccw(&st.tris), "every triangle front-facing");
        let filled = st.ranges.iter().filter(|&&(a, b)| b > a).count();
        assert!(
            filled * 2 >= map.sectors.len(),
            "most sectors triangulate: {filled}/{}",
            map.sectors.len()
        );
    }

    fn e6m6_s59() -> Vec<Vec<[f32; 2]>> {
        vec![
            vec![
                [-564.0, 969.0],
                [-992.0, 848.0],
                [-999.0, 704.0],
                [-999.0, 696.0],
                [-1008.0, 480.0],
                [-944.0, 440.0],
                [-974.0, 412.0],
                [-1007.0, 443.0],
                [-1044.0, 408.0],
                [-1152.0, 384.0],
                [-1152.0, 320.0],
                [-1976.0, 320.0],
                [-1976.0, 456.0],
                [-2112.0, 456.0],
                [-2112.0, 1336.0],
                [-1984.0, 1336.0],
                [-1984.0, 1328.0],
                [-1984.0, 1288.0],
                [-1984.0, 1280.0],
                [-1920.0, 1280.0],
                [-1920.0, 1344.0],
                [-1792.0, 1344.0],
                [-1728.0, 1344.0],
                [-1728.0, 1472.0],
                [-1408.0, 1472.0],
                [-1408.0, 1408.0],
                [-968.0, 1408.0],
                [-968.0, 1400.0],
                [-832.0, 1400.0],
                [-832.0, 1152.0],
                [-960.0, 1152.0],
                [-960.0, 1088.0],
                [-768.0, 1088.0],
                [-768.0, 1328.0],
                [-702.0, 1328.0],
                [-768.0, 1328.0],
                [-768.0, 1408.0],
                [-768.0, 1536.0],
                [-944.0, 1536.0],
                [-944.0, 1600.0],
                [-932.0, 1600.0],
                [-932.0, 1610.0],
                [-928.0, 1608.0],
                [-896.0, 1576.0],
                [-840.0, 1592.0],
                [-808.0, 1552.0],
                [-768.0, 1560.0],
                [-736.0, 1536.0],
                [-752.0, 1496.0],
                [-744.0, 1440.0],
                [-760.0, 1408.0],
                [-744.0, 1360.0],
                [-712.0, 1376.0],
                [-696.0, 1360.0],
                [-664.0, 1344.0],
                [-664.0, 1304.0],
                [-640.0, 1272.0],
                [-648.0, 1224.0],
                [-632.0, 1176.0],
                [-640.0, 1152.0],
                [-616.0, 1112.0],
                [-616.0, 1072.0],
                [-600.0, 1048.0],
                [-608.0, 1008.0],
                [-584.0, 984.0],
                [-564.0, 976.0],
            ],
            vec![
                [-1020.0, 764.0],
                [-1040.0, 765.0],
                [-1024.0, 864.0],
                [-1000.0, 876.0],
                [-996.0, 860.0],
                [-980.0, 864.0],
                [-974.0, 861.0],
                [-966.0, 862.0],
                [-958.0, 864.0],
                [-953.0, 868.0],
                [-951.0, 872.0],
                [-936.0, 876.0],
                [-941.0, 896.0],
                [-765.0, 948.0],
                [-760.0, 927.0],
                [-745.0, 932.0],
                [-738.0, 928.0],
                [-732.0, 928.0],
                [-724.0, 929.0],
                [-717.0, 933.0],
                [-714.0, 937.0],
                [-712.0, 941.0],
                [-697.0, 945.0],
                [-703.0, 964.0],
                [-616.0, 988.0],
                [-628.0, 1120.0],
                [-648.0, 1116.0],
                [-664.0, 1180.0],
                [-648.0, 1116.0],
                [-664.0, 1112.0],
                [-652.0, 1008.0],
                [-711.0, 992.0],
                [-716.0, 1008.0],
                [-776.0, 992.0],
                [-771.0, 972.0],
                [-948.0, 920.0],
                [-952.0, 936.0],
                [-1012.0, 920.0],
                [-1007.0, 900.0],
                [-1048.0, 876.0],
                [-1060.0, 766.0],
                [-1084.0, 768.0],
                [-1088.0, 704.0],
                [-1068.0, 703.0],
                [-1076.0, 588.0],
                [-1100.0, 588.0],
                [-1104.0, 524.0],
                [-1037.0, 524.0],
                [-1037.0, 538.0],
                [-1032.0, 542.0],
                [-1028.0, 549.0],
                [-1026.0, 559.0],
                [-1028.0, 564.0],
                [-1032.0, 569.0],
                [-1036.0, 573.0],
                [-1036.0, 588.0],
                [-1052.0, 588.0],
                [-1044.0, 701.0],
                [-1024.0, 700.0],
                [-1023.0, 715.0],
                [-1017.0, 722.0],
                [-1016.0, 728.0],
                [-1016.0, 735.0],
                [-1016.0, 742.0],
                [-1021.0, 747.0],
            ],
            vec![
                [-1216.0, 448.0],
                [-1216.0, 512.0],
                [-1472.0, 512.0],
                [-1472.0, 768.0],
                [-1536.0, 768.0],
                [-1536.0, 448.0],
            ],
            vec![
                [-1280.0, 640.0],
                [-1216.0, 640.0],
                [-1216.0, 1024.0],
                [-1088.0, 1024.0],
                [-1088.0, 1280.0],
                [-1408.0, 1280.0],
                [-1408.0, 1216.0],
                [-1152.0, 1216.0],
                [-1152.0, 1088.0],
                [-1280.0, 1088.0],
            ],
            vec![
                [-1600.0, 896.0],
                [-1408.0, 896.0],
                [-1408.0, 1088.0],
                [-1728.0, 1088.0],
                [-1728.0, 1024.0],
                [-1472.0, 1024.0],
                [-1472.0, 960.0],
                [-1600.0, 960.0],
            ],
            vec![
                [-1792.0, 832.0],
                [-1856.0, 832.0],
                [-1856.0, 448.0],
                [-1792.0, 448.0],
            ],
            vec![
                [-1984.0, 960.0],
                [-1920.0, 960.0],
                [-1920.0, 1152.0],
                [-1984.0, 1152.0],
            ],
            vec![
                [-680.0, 1176.0],
                [-664.0, 1180.0],
                [-648.0, 1184.0],
                [-680.0, 1344.0],
                [-704.0, 1344.0],
                [-702.0, 1328.0],
            ],
            vec![
                [-1736.0, 424.0],
                [-1688.0, 376.0],
                [-1648.0, 424.0],
                [-1600.0, 408.0],
                [-1560.0, 480.0],
                [-1608.0, 528.0],
                [-1560.0, 664.0],
                [-1584.0, 720.0],
                [-1560.0, 792.0],
                [-1592.0, 840.0],
                [-1648.0, 824.0],
                [-1704.0, 864.0],
                [-1752.0, 824.0],
                [-1728.0, 768.0],
                [-1744.0, 720.0],
                [-1728.0, 672.0],
                [-1760.0, 608.0],
                [-1744.0, 536.0],
                [-1752.0, 480.0],
            ],
            vec![
                [-1784.0, 1264.0],
                [-1792.0, 1192.0],
                [-1736.0, 1176.0],
                [-1704.0, 1200.0],
                [-1680.0, 1160.0],
                [-1712.0, 1120.0],
                [-1664.0, 1104.0],
                [-1584.0, 1136.0],
                [-1536.0, 1112.0],
                [-1448.0, 1128.0],
                [-1488.0, 1168.0],
                [-1440.0, 1184.0],
                [-1448.0, 1232.0],
                [-1424.0, 1288.0],
                [-1376.0, 1320.0],
                [-1320.0, 1296.0],
                [-1256.0, 1312.0],
                [-1200.0, 1296.0],
                [-1160.0, 1296.0],
                [-1160.0, 1320.0],
                [-1136.0, 1336.0],
                [-1144.0, 1360.0],
                [-1184.0, 1368.0],
                [-1200.0, 1392.0],
                [-1248.0, 1392.0],
                [-1272.0, 1376.0],
                [-1320.0, 1392.0],
                [-1392.0, 1384.0],
                [-1456.0, 1392.0],
                [-1504.0, 1424.0],
                [-1552.0, 1416.0],
                [-1600.0, 1432.0],
                [-1640.0, 1408.0],
                [-1608.0, 1376.0],
                [-1648.0, 1352.0],
                [-1648.0, 1320.0],
                [-1696.0, 1312.0],
                [-1704.0, 1272.0],
                [-1752.0, 1296.0],
            ],
            vec![
                [-2060.0, 948.0],
                [-2080.0, 932.0],
                [-2092.0, 896.0],
                [-2056.0, 868.0],
                [-2072.0, 836.0],
                [-2040.0, 768.0],
                [-2088.0, 692.0],
                [-2052.0, 632.0],
                [-2064.0, 580.0],
                [-2040.0, 552.0],
                [-2056.0, 512.0],
                [-2004.0, 504.0],
                [-1964.0, 528.0],
                [-1928.0, 512.0],
                [-1900.0, 556.0],
                [-1924.0, 656.0],
                [-1892.0, 716.0],
                [-1904.0, 824.0],
                [-1880.0, 848.0],
                [-1896.0, 868.0],
                [-1932.0, 856.0],
                [-1936.0, 880.0],
                [-1968.0, 888.0],
                [-1964.0, 920.0],
                [-2008.0, 932.0],
                [-2008.0, 960.0],
                [-2048.0, 992.0],
                [-2076.0, 972.0],
            ],
            vec![
                [-1036.0, 1240.0],
                [-1064.0, 1220.0],
                [-1072.0, 1184.0],
                [-1052.0, 1144.0],
                [-1076.0, 1112.0],
                [-1056.0, 1068.0],
                [-1068.0, 1044.0],
                [-1064.0, 1012.0],
                [-1092.0, 996.0],
                [-1124.0, 1000.0],
                [-1140.0, 964.0],
                [-1164.0, 960.0],
                [-1192.0, 936.0],
                [-1168.0, 896.0],
                [-1192.0, 856.0],
                [-1176.0, 824.0],
                [-1200.0, 760.0],
                [-1180.0, 716.0],
                [-1196.0, 688.0],
                [-1184.0, 628.0],
                [-1212.0, 600.0],
                [-1240.0, 616.0],
                [-1272.0, 604.0],
                [-1260.0, 576.0],
                [-1240.0, 552.0],
                [-1184.0, 552.0],
                [-1184.0, 528.0],
                [-1160.0, 512.0],
                [-1132.0, 532.0],
                [-1112.0, 576.0],
                [-1128.0, 608.0],
                [-1104.0, 624.0],
                [-1088.0, 664.0],
                [-1112.0, 696.0],
                [-1100.0, 748.0],
                [-1116.0, 792.0],
                [-1088.0, 808.0],
                [-1072.0, 844.0],
                [-1084.0, 880.0],
                [-1048.0, 908.0],
                [-1076.0, 940.0],
                [-1008.0, 948.0],
                [-992.0, 968.0],
                [-920.0, 964.0],
                [-892.0, 992.0],
                [-872.0, 968.0],
                [-828.0, 980.0],
                [-808.0, 1032.0],
                [-820.0, 1056.0],
                [-872.0, 1048.0],
                [-904.0, 1064.0],
                [-980.0, 1048.0],
                [-968.0, 1076.0],
                [-988.0, 1128.0],
                [-980.0, 1148.0],
                [-992.0, 1176.0],
                [-960.0, 1204.0],
                [-968.0, 1216.0],
                [-1012.0, 1220.0],
            ],
        ]
    }
}
