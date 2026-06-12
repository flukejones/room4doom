//! Build sectors from drawn lines.
//!
//! The unit is a directed line side — an [`Edge`]. From one, the trace follows
//! connected lines by the smallest turning angle to walk the closed boundary
//! that side belongs to. After tracing an outline, a ray cast east from its
//! rightmost vertex finds the next boundary outward; the outermost (clockwise)
//! boundary is the sector's, and a ray that escapes the map means the side
//! bounds nothing (void). Inner contours (holes) are then traced from the
//! remaining rightmost vertices and folded into the same sector.
//!
//! Which side faces a room's interior is decided by the directed trace itself —
//! the sector lands on exactly the `(line, side)` pairs the walk visits.
//!
//! [`build_sectors`] runs once per edit. It re-sectors only the `newly_created`
//! lines; every other affected line keeps its authored sides, so a move never
//! re-derives an authored void pocket. A traced loop bounded by bare frozen walls
//! is left void (SLADE's rule); a genuinely new enclosed loop gets its own sector
//! rather than reusing the enclosing one.

use std::collections::{HashMap, HashSet};
use std::f32::consts::TAU;
use std::mem;

use crate::flags::LineFlags;
use crate::geom::{
    dedup_coincident_lines, distance_to_segment, is_front_side, ring_signed_area, sector_at,
    segment_points,
};
use crate::model::{EditorMap, Sector};

/// East-ray distance tie tolerance.
const RAY_TIE: f32 = 0.001;

/// Which directed side of a line an [`Edge`] walks: `Front` goes `v1`->`v2`,
/// `Back` goes `v2`->`v1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Side {
    Front,
    Back,
}

/// A directed line side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Edge {
    pub line: u32,
    pub side: Side,
}

/// A sector's closed boundary ring as ordered vertex indices, with winding.
/// Outer rings are counter-clockwise; holes are clockwise. Returned by
/// [`sector_loops`] for triangulation/rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectorLoop {
    pub verts: Vec<u32>,
    pub clockwise: bool,
}

/// One traced boundary: its ordered directed sides, winding, and rightmost
/// vertex (the ray-east origin for stepping outward).
struct Outline {
    edges: Vec<Edge>,
    clockwise: bool,
    rightmost: u32,
}

/// Vertex → the lines meeting at it, built once per build pass.
struct Adjacency {
    lines_at: Vec<Vec<u32>>,
}

impl Adjacency {
    fn build(map: &EditorMap) -> Self {
        let mut lines_at = vec![Vec::new(); map.vertices.len()];
        for (i, line) in map.lines.iter().enumerate() {
            lines_at[line.v1 as usize].push(i as u32);
            lines_at[line.v2 as usize].push(i as u32);
        }
        Self {
            lines_at,
        }
    }
}

/// Re-sector after an edit.
///
/// Only `newly_created` lines may gain, lose, or change a sector; every other
/// line in `affected` keeps its authored sides, so a void pocket stays void when
/// a nearby edit re-sectors. Frozen lines are read while tracing (a new loop
/// reuses a bordering sector) but never written.
pub fn build_sectors(
    map: &mut EditorMap,
    affected: &[u32],
    newly_created: &[u32],
    default_record: Sector,
) {
    let new_set: HashSet<u32> = newly_created.iter().copied().collect();
    correct_sectors(map, affected, &new_set, default_record);

    let flipped: Vec<u32> = newly_created
        .iter()
        .copied()
        .filter(|&l| map.lines[l as usize].front.sector.is_none())
        .collect();
    for &line in &flipped {
        flip_line(map, line);
    }
    if !flipped.is_empty() {
        let flipped_set: HashSet<u32> = flipped.iter().copied().collect();
        correct_sectors(map, &flipped, &flipped_set, default_record);
        for &line in &flipped {
            if map.lines[line as usize].front.sector.is_none() {
                flip_line(map, line);
            }
        }
    }

    dedup_coincident_lines(map);
    map.prune_unused_sectors();
}

/// Add a sector to the enclosed but sectorless space containing `world`.
///
/// Traces the boundary loop facing the cursor; if it is a closed (clockwise)
/// interior, pushes a sector with `default_record` and assigns its index to the
/// cursor-side of each bounding line (front→`front.sector`, back→`back.sector`),
/// leaving the opposite side untouched. Returns the new sector index, or `None`
/// when the point is already sectored, in open void, or has no nearby boundary.
pub fn add_sector_in_enclosure(
    map: &mut EditorMap,
    world: [f32; 2],
    default_record: Sector,
) -> Option<u32> {
    if sector_at(map, world).is_some() || map.lines.is_empty() {
        return None;
    }
    // Nearest line, and the side of it that faces the cursor.
    let nearest = (0..map.lines.len() as u32)
        .min_by(|&a, &b| seg_distance(map, a, world).total_cmp(&seg_distance(map, b, world)))?;
    let (p1, p2) = segment_points(map, nearest);
    // The directed side whose enclosed region contains the cursor: the Front
    // walk (v1->v2) bounds the area on its right, which is where `is_front_side`
    // reports the cursor lies.
    let side = if is_front_side(world, p1, p2) {
        Side::Front
    } else {
        Side::Back
    };
    // `trace_sector` returns the bounding edges of an enclosed region, or None
    // when the cursor side faces the void (its east-ray escapes the map).
    let adj = Adjacency::build(map);
    let edges = trace_sector(
        map,
        &adj,
        Edge {
            line: nearest,
            side,
        },
    )?;

    map.sectors.push(default_record);
    let new = (map.sectors.len() - 1) as u32;
    for edge in &edges {
        let line = &mut map.lines[edge.line as usize];
        match edge.side {
            Side::Front => line.front.sector = Some(new),
            Side::Back if line.back.is_some() => {
                if let Some(back) = line.back.as_mut() {
                    back.sector = Some(new);
                }
            }
            // One-sided line walked on its back. If its single side faces the
            // void, flip it inward; if it already fronts a sector (a divider, e.g.
            // a pillar in a room), make it two-sided so both sectors keep a side.
            Side::Back if line.front.sector.is_none() => {
                flip_line(map, edge.line);
                map.lines[edge.line as usize].front.sector = Some(new);
            }
            Side::Back => {
                let mut back = line.front;
                back.sector = Some(new);
                line.back = Some(back);
                line.flags.insert(LineFlags::TWO_SIDED);
            }
        }
    }
    Some(new)
}

/// Split the boundary loop containing `world` off its sector into a new one.
///
/// Traces the cursor-side loop and reassigns just that side of each bounding line
/// to a fresh sector copying the current record; the rest of the old sector keeps
/// it. Returns the new sector index, or `None` when the point is in the void or
/// its sector is a single loop (nothing to separate).
pub fn unmerge_sector_at(map: &mut EditorMap, world: [f32; 2]) -> Option<u32> {
    let cur = sector_at(map, world)?;
    if sector_loop_count(map, cur) < 2 {
        return None;
    }
    let nearest = (0..map.lines.len() as u32)
        .min_by(|&a, &b| seg_distance(map, a, world).total_cmp(&seg_distance(map, b, world)))?;
    let (p1, p2) = segment_points(map, nearest);
    let side = if is_front_side(world, p1, p2) {
        Side::Front
    } else {
        Side::Back
    };
    let adj = Adjacency::build(map);
    let edges = trace_sector(
        map,
        &adj,
        Edge {
            line: nearest,
            side,
        },
    )?;

    let record = map.sectors[cur as usize];
    map.sectors.push(record);
    let new = (map.sectors.len() - 1) as u32;
    for edge in &edges {
        set_side(map, *edge, Some(new));
    }
    Some(new)
}

/// Whether the sector under `world` has more than one boundary loop — the gate
/// for separating one loop off (`unmerge_sector_at`).
pub fn sector_under_cursor_has_separable_loop(map: &EditorMap, world: [f32; 2]) -> bool {
    sector_at(map, world).is_some_and(|s| sector_loop_count(map, s) > 1)
}

/// The number of disjoint boundary loops bounding `sector`. One loop is a simple
/// room; two or more means separate enclosures share the sector (e.g. a merge).
pub fn sector_loop_count(map: &EditorMap, sector: u32) -> usize {
    let adj = Adjacency::build(map);
    let edges: Vec<Edge> = sector_edges(map, sector);
    let mut claimed: HashSet<Edge> = HashSet::new();
    let mut loops = 0;
    for &start in &edges {
        if claimed.contains(&start) {
            continue;
        }
        loops += 1;
        for e in trace_outline(map, &adj, start).edges {
            claimed.insert(e);
        }
    }
    loops
}

/// Every closed boundary ring of `sector`, as ordered vertex-index loops.
///
/// Outer rings are counter-clockwise, holes clockwise. The sector's own directed
/// boundary edges are chained head-to-tail, resolving a shared vertex by the
/// smallest left turn so the walk hugs the sector. Open geometry (an edge that
/// does not close) contributes no ring.
pub fn sector_loops(map: &EditorMap, sector: u32) -> Vec<SectorLoop> {
    loops_from_edges(map, &sector_edges(map, sector))
}

/// Closed boundary loops for every sector in one pass.
///
/// Equivalent to calling [`sector_loops`] per sector, but the directed edges are
/// bucketed by sector in a single scan of the lines — avoiding the per-sector
/// full-line rescan that makes the per-sector call quadratic on large maps.
pub fn sector_loops_all(map: &EditorMap) -> Vec<Vec<SectorLoop>> {
    let mut by_sector: Vec<Vec<Edge>> = vec![Vec::new(); map.sectors.len()];
    let mut push = |sector: u32, edge: Edge| {
        if let Some(bucket) = by_sector.get_mut(sector as usize) {
            bucket.push(edge);
        }
    };
    for (i, l) in map.lines.iter().enumerate() {
        if let Some(s) = l.front.sector {
            push(s, Edge {
                line: i as u32,
                side: Side::Front,
            });
        }
        if let Some(s) = l.back.and_then(|b| b.sector) {
            push(s, Edge {
                line: i as u32,
                side: Side::Back,
            });
        }
    }
    by_sector
        .iter()
        .map(|edges| loops_from_edges(map, edges))
        .collect()
}

/// Chain a sector's directed edges into closed boundary loops. The edges are this
/// sector's `(line, side)` pairs; each is consumed once and walked by smallest
/// turning angle. Rings that fail to close are dropped.
fn loops_from_edges(map: &EditorMap, edges: &[Edge]) -> Vec<SectorLoop> {
    // Index the directed edges by their start vertex for the chaining walk.
    let mut by_start: HashMap<u32, Vec<usize>> = HashMap::new();
    for (i, &e) in edges.iter().enumerate() {
        by_start.entry(edge_start(map, e)).or_default().push(i);
    }

    let mut used = vec![false; edges.len()];
    let mut loops = Vec::new();
    for i0 in 0..edges.len() {
        if used[i0] {
            continue;
        }
        let mut ring: Vec<u32> = Vec::new();
        let mut cur = i0;
        let start_v = edge_start(map, edges[i0]);
        loop {
            used[cur] = true;
            ring.push(edge_start(map, edges[cur]));
            let to = edge_end(map, edges[cur]);
            if to == start_v {
                break; // closed
            }
            let Some(next) = next_sector_edge(map, edges[cur], by_start.get(&to), edges, &used)
            else {
                ring.clear();
                break; // open: abandon this ring
            };
            cur = next;
        }
        if ring.len() >= 3 {
            let clockwise = ring_signed_area(map, &ring) < 0.0;
            loops.push(SectorLoop {
                verts: ring,
                clockwise,
            });
        }
    }
    loops
}

/// At vertex `to` (the arriving edge's end), the unused candidate continuing the
/// sector boundary with the smallest left turn from the incoming direction.
fn next_sector_edge(
    map: &EditorMap,
    incoming: Edge,
    candidates: Option<&Vec<usize>>,
    edges: &[Edge],
    used: &[bool],
) -> Option<usize> {
    let candidates = candidates?;
    let inc = edge_dir(map, incoming);
    let back = [-inc[0], -inc[1]];
    let mut best: Option<(f32, usize)> = None;
    for &ci in candidates {
        if used[ci] {
            continue;
        }
        let out = edge_dir(map, edges[ci]);
        // Turn measured CCW from the reverse-incoming ray; smallest hugs left.
        let ang = ccw_angle(back, out);
        if best.is_none_or(|(b, _)| ang < b) {
            best = Some((ang, ci));
        }
    }
    best.map(|(_, ci)| ci)
}

/// CCW angle in `[0, 2π)` from `a` to `b`.
fn ccw_angle(a: [f32; 2], b: [f32; 2]) -> f32 {
    let dot = a[0] * b[0] + a[1] * b[1];
    let cross = a[0] * b[1] - a[1] * b[0];
    let ang = cross.atan2(dot);
    if ang < 0.0 { ang + TAU } else { ang }
}

/// Unit-ish direction of a directed edge (start → end), not normalised.
fn edge_dir(map: &EditorMap, e: Edge) -> [f32; 2] {
    let a = map.vertices[edge_start(map, e) as usize];
    let b = map.vertices[edge_end(map, e) as usize];
    [b.x - a.x, b.y - a.y]
}

/// The leading vertex of a directed edge: `v1` on the front side, `v2` on the back.
fn edge_start(map: &EditorMap, edge: Edge) -> u32 {
    let line = &map.lines[edge.line as usize];
    match edge.side {
        Side::Front => line.v1,
        Side::Back => line.v2,
    }
}

/// The trailing vertex of a directed edge.
fn edge_end(map: &EditorMap, edge: Edge) -> u32 {
    let line = &map.lines[edge.line as usize];
    match edge.side {
        Side::Front => line.v2,
        Side::Back => line.v1,
    }
}

/// The directed sides facing `sector`: each line whose front or back references it.
fn sector_edges(map: &EditorMap, sector: u32) -> Vec<Edge> {
    let mut edges = Vec::new();
    for (i, l) in map.lines.iter().enumerate() {
        if l.front.sector == Some(sector) {
            edges.push(Edge {
                line: i as u32,
                side: Side::Front,
            });
        }
        if l.back.is_some_and(|b| b.sector == Some(sector)) {
            edges.push(Edge {
                line: i as u32,
                side: Side::Back,
            });
        }
    }
    edges
}

/// One sector-building pass: trace each new line's sides, sector or void each
/// loop, and write only to new lines (frozen lines are read, never changed).
fn correct_sectors(
    map: &mut EditorMap,
    affected: &[u32],
    new_set: &HashSet<u32>,
    default_record: Sector,
) {
    let adj = Adjacency::build(map);

    let mut edges: Vec<Edge> = Vec::new();
    let mut claimed: Vec<bool> = Vec::new();
    for &line in affected {
        if !new_set.contains(&line) {
            continue;
        }
        edges.push(Edge {
            line,
            side: Side::Front,
        });
        claimed.push(false);
        if back_edge_wanted(map, line) {
            edges.push(Edge {
                line,
                side: Side::Back,
            });
            claimed.push(false);
        }
    }

    let mut reused: Vec<u32> = Vec::new();
    for i in 0..edges.len() {
        if claimed[i] {
            continue;
        }
        let Some(traced) = trace_sector(map, &adj, edges[i]) else {
            continue;
        };
        for e in &traced {
            if let Some(idx) = edge_index(&edges, *e) {
                claimed[idx] = true;
            }
        }
        if encloses_void(map, &traced, new_set) {
            continue;
        }
        let bordering = bordering_sectors(map, &traced, new_set);
        let sector = choose_sector(map, &traced, &bordering, &mut reused, default_record);
        // A loop bordering two distinct existing sectors bridges them; its frozen
        // walls keep their own sector and only the void sides take the new one.
        // A loop within one sector (or none) re-sectors its new sides freely.
        let bridges = bordering.len() > 1;
        // When a split halves a sector, a frozen side still facing that sector
        // belongs to whichever loop traced it; let it move to the chosen sector.
        let split_source = if bridges {
            None
        } else {
            bordering.first().copied()
        };
        for e in &traced {
            let writable = if bridges {
                side_sector(map, *e).is_none()
            } else {
                new_set.contains(&e.line)
                    || side_sector(map, *e).is_none()
                    || side_sector(map, *e) == split_source
            };
            if writable {
                set_side(map, *e, Some(sector));
            }
        }
    }

    for (e, done) in edges.iter().zip(&claimed) {
        if !done {
            set_side(map, *e, None);
        }
    }

    for &line in affected {
        if new_set.contains(&line) {
            flip_if_back_only(map, line);
        }
    }
}

/// A traced loop is void if its only frozen boundary walls are bare on the traced
/// side — single-sided walls facing into the loop mean it is enclosed void (a void
/// pocket's lobe), not a sector.
fn encloses_void(map: &EditorMap, traced: &[Edge], new_set: &HashSet<u32>) -> bool {
    let mut frozen_in_loop = false;
    let mut frozen_faces_sector = false;
    for e in traced {
        if new_set.contains(&e.line) {
            continue;
        }
        frozen_in_loop = true;
        if side_sector(map, *e).is_some() {
            frozen_faces_sector = true;
        }
    }
    frozen_in_loop && !frozen_faces_sector
}

/// Whether a new line's back side should be considered: when the line is already
/// two-sided, or when its midpoint sits inside an existing sector.
fn back_edge_wanted(map: &EditorMap, line: u32) -> bool {
    let l = &map.lines[line as usize];
    if l.back.is_some() {
        return true;
    }
    let mid = line_midpoint(map, line);
    sector_at(map, mid).is_some()
}

/// Trace the full sector boundary the `start` side bounds — its outer outline
/// plus any inner (hole) outlines — or `None` when the side faces the void.
///
/// Trace the outline; while it is anticlockwise (an inner contour), step to the
/// boundary just outside via an east-ray ([`outer_edge`]) and re-trace, until the
/// outline winds clockwise (the outermost), accumulating each traced outline's
/// edges. A ray that escapes the map means the side faces the void → `None`. Then
/// trace inner contours (holes) from the remaining rightmost vertices and fold
/// them into the same sector.
fn trace_sector(map: &EditorMap, adj: &Adjacency, start: Edge) -> Option<Vec<Edge>> {
    let mut valid = vec![true; map.vertices.len()];
    let mut sector_edges: Vec<Edge> = Vec::new();
    let cap = map.lines.len() + 2;

    let mut edge = start;
    for _ in 0..cap {
        let outline = trace_outline(map, adj, edge);
        discard_outline_vertices(map, &outline, &mut valid);
        discard_outside(map, &outline, &mut valid);
        sector_edges.extend(outline.edges.iter().copied());
        if outline.clockwise {
            break;
        }
        // Anticlockwise: this is an inner contour; step to the boundary outside.
        edge = outer_edge(map, outline.rightmost)?;
    }

    // Trace inner contours (holes) from the remaining rightmost vertices.
    for _ in 0..cap {
        let Some(inner) = inner_edge(map, adj, &mut valid) else {
            break;
        };
        let inner_outline = trace_outline(map, adj, inner);
        discard_outline_vertices(map, &inner_outline, &mut valid);
        discard_outside(map, &inner_outline, &mut valid);
        sector_edges.extend(inner_outline.edges.iter().copied());
    }

    Some(sector_edges)
}

/// Trace one closed outline from `start`, taking the smallest-angle turn at each
/// vertex and reversing at dead ends (turn-at-ends). Reports its winding (via a
/// shoelace sum) and rightmost vertex. A line may be walked once per direction,
/// tracked by a front/back bit per line.
fn trace_outline(map: &EditorMap, adj: &Adjacency, start: Edge) -> Outline {
    let mut edges = vec![start];
    // Per line: bit 0 = front walked, bit 1 = back walked.
    let mut visited = vec![0u8; map.lines.len()];
    let mut edge_sum = 0.0;
    let mut rightmost = map.lines[start.line as usize].v1;
    let mut edge = start;

    let cap = map.lines.len() * 2 + 2;
    for _ in 0..cap {
        edge_sum += shoelace_term(map, edge);
        rightmost = right_vertex(map, rightmost, edge.line);

        let next = match next_edge(map, adj, edge, &visited) {
            Some(e) => e,
            // Dead end: go back along the other side of the same line.
            None => Edge {
                line: edge.line,
                side: opposite(edge.side),
            },
        };
        visited[next.line as usize] |= side_bit(next.side);

        if next == start {
            break;
        }
        edges.push(next);
        edge = next;
    }

    Outline {
        clockwise: edge_sum < 0.0,
        rightmost,
        edges,
    }
}

/// The next edge continuing the outline from `edge`: at its destination vertex,
/// the connected line making the smallest turn. `None` when the destination is a
/// dead end (only `edge.line` connects) or every candidate is exhausted.
fn next_edge(map: &EditorMap, adj: &Adjacency, edge: Edge, visited: &[u8]) -> Option<Edge> {
    let dst = edge_dst(map, edge);
    let prev = edge_src(map, edge);
    let here = map.vertices[dst as usize];
    let here = [here.x, here.y];
    let prev = {
        let p = map.vertices[prev as usize];
        [p.x, p.y]
    };

    let mut best: Option<(f32, Edge)> = None;
    for &line in &adj.lines_at[dst as usize] {
        if line == edge.line {
            continue;
        }
        let l = &map.lines[line as usize];
        if l.v1 == l.v2 {
            continue;
        }
        // Leaving `dst` along `line`; the side is Front when v1 == dst.
        let (other, side) = if l.v1 == dst {
            (l.v2, Side::Front)
        } else {
            (l.v1, Side::Back)
        };
        // Skip a line already walked in this direction.
        if visited[line as usize] & side_bit(side) != 0 {
            continue;
        }
        let np = map.vertices[other as usize];
        let angle = turn_angle(prev, here, [np.x, np.y]);
        if best.is_none_or(|(a, _)| angle < a) {
            best = Some((
                angle,
                Edge {
                    line,
                    side,
                },
            ));
        }
    }
    best.map(|(_, e)| e)
}

/// The edge just outside an outline: ray east from `rightmost` to the nearest
/// line it crosses; the side that line presents back toward the vertex. `None`
/// when nothing lies east — the outline is the outermost, against the void.
fn outer_edge(map: &EditorMap, rightmost: u32) -> Option<Edge> {
    let v = map.vertices[rightmost as usize];
    let (vx, vy) = (v.x, v.y);

    let mut nearest: Option<(f32, u32)> = None;
    for (i, line) in map.lines.iter().enumerate() {
        let (p1, p2) = (
            map.vertices[line.v1 as usize],
            map.vertices[line.v2 as usize],
        );
        if p1.x <= vx && p2.x <= vx {
            continue; // entirely west of the vertex
        }
        if p1.y == p2.y {
            continue; // horizontal: never crossed by an east ray
        }
        if (p1.y < vy && p2.y < vy) || (p1.y > vy && p2.y > vy) {
            continue; // does not span the ray's y
        }
        let frac = (vy - p1.y) / (p2.y - p1.y);
        let ix = p1.x + (p2.x - p1.x) * frac;
        let dist = (ix - vx).abs();
        match nearest {
            None => nearest = Some((dist, i as u32)),
            Some((best, _)) if dist < best - RAY_TIE => nearest = Some((dist, i as u32)),
            Some((best, best_line))
                if (dist - best).abs() <= RAY_TIE
                // Ray hit a shared vertex: prefer the line nearer the vertex,
                // else picking an inner edge.
                && seg_distance(map, i as u32, [vx, vy]) < seg_distance(map, best_line, [vx, vy]) =>
            {
                nearest = Some((dist, i as u32));
            }
            _ => {}
        }
    }

    let (_, line) = nearest?;
    let (p1, p2) = segment_points(map, line);
    let side = if is_front_side([vx, vy], p1, p2) {
        Side::Front
    } else {
        Side::Back
    };
    Some(Edge {
        line,
        side,
    })
}

/// The next inner-contour edge: from the rightmost still-valid vertex, the
/// connected line whose direction is the smallest angle from due east. `None`
/// once every vertex has been discarded.
fn inner_edge(map: &EditorMap, adj: &Adjacency, valid: &mut [bool]) -> Option<Edge> {
    loop {
        let mut rightmost: Option<u32> = None;
        for (i, &ok) in valid.iter().enumerate() {
            if !ok {
                continue;
            }
            if rightmost.is_none_or(|r| map.vertices[i].x > map.vertices[r as usize].x) {
                rightmost = Some(i as u32);
            }
        }
        let rv = rightmost?;
        let here = map.vertices[rv as usize];
        let east = [here.x + 32.0, here.y];
        let here = [here.x, here.y];

        let mut best: Option<(f32, Edge)> = None;
        for &line in &adj.lines_at[rv as usize] {
            let l = &map.lines[line as usize];
            if l.v1 == l.v2 {
                continue;
            }
            let (other, side) = if l.v1 == rv {
                (l.v2, Side::Front)
            } else {
                (l.v1, Side::Back)
            };
            let op = map.vertices[other as usize];
            let angle = turn_angle(east, here, [op.x, op.y]);
            if best.is_none_or(|(a, _)| angle < a) {
                best = Some((
                    angle,
                    Edge {
                        line,
                        side,
                    },
                ));
            }
        }
        match best {
            Some((_, e)) => return Some(e),
            // Vertex has no usable line; discard and retry.
            None => valid[rv as usize] = false,
        }
    }
}

/// Discard every vertex an outline's edges touch, so an inner-contour scan never
/// restarts from a boundary already traced.
fn discard_outline_vertices(map: &EditorMap, outline: &Outline, valid: &mut [bool]) {
    for e in &outline.edges {
        let l = &map.lines[e.line as usize];
        valid[l.v1 as usize] = false;
        valid[l.v2 as usize] = false;
    }
}

/// Discard every vertex lying outside `outline`, so inner-contour tracing only
/// starts from vertices the current outline encloses.
fn discard_outside(map: &EditorMap, outline: &Outline, valid: &mut [bool]) {
    for (i, ok) in valid.iter_mut().enumerate() {
        if !*ok {
            continue;
        }
        let p = map.vertices[i];
        if !point_in_outline(map, outline, [p.x, p.y]) {
            *ok = false;
        }
    }
}

/// Whether `p` is within `outline`: bbox reject with a clockwise short-circuit,
/// then the side of the nearest edge.
fn point_in_outline(map: &EditorMap, outline: &Outline, p: [f32; 2]) -> bool {
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for e in &outline.edges {
        let (a, b) = segment_points(map, e.line);
        for v in [&a, &b] {
            min_x = min_x.min(v[0]);
            min_y = min_y.min(v[1]);
            max_x = max_x.max(v[0]);
            max_y = max_y.max(v[1]);
        }
    }
    let in_bbox = p[0] >= min_x && p[0] <= max_x && p[1] >= min_y && p[1] <= max_y;
    if !in_bbox {
        // Outside the bbox: inside iff the outline is anticlockwise (it wraps
        // everything not enclosed by the clockwise hole it bounds).
        return !outline.clockwise;
    }

    let mut min_dist = f32::MAX;
    let mut nearest: Option<Edge> = None;
    for e in &outline.edges {
        let d = seg_distance(map, e.line, p);
        if d < min_dist {
            min_dist = d;
            nearest = Some(*e);
        }
    }
    match nearest {
        Some(e) => {
            let (a, b) = segment_points(map, e.line);
            let front = is_front_side(p, a, b);
            (front && e.side == Side::Front) || (!front && e.side == Side::Back)
        }
        None => false,
    }
}

/// Sector for a traced boundary: reuse one borne by a frozen traced side, else a
/// fresh record copying a neighbour's, else `default_record`. An all-new loop has
/// no frozen side, so it gets its own sector rather than the enclosing one's.
fn choose_sector(
    map: &mut EditorMap,
    traced: &[Edge],
    bordering: &[u32],
    reused: &mut Vec<u32>,
    default_record: Sector,
) -> u32 {
    // Reuse the loop's single bordering sector; a loop bridging two (or bordering
    // none) gets a fresh record so distinct rooms never collapse together.
    if let [existing] = *bordering
        && !reused.contains(&existing)
    {
        reused.push(existing);
        return existing;
    }
    // A piece carved off an existing sector inherits its record. Source order:
    // the bordering sector; else the one the loop's traced sides already face (a
    // re-traced ring carries it even when every edge is "new"); else a neighbour.
    let record = bordering
        .first()
        .copied()
        .or_else(|| traced_side_sector(map, traced))
        .and_then(|s| map.sectors.get(s as usize).copied())
        .or_else(|| copy_sector(map, traced))
        .unwrap_or(default_record);
    map.sectors.push(record);
    (map.sectors.len() - 1) as u32
}

/// The most common sector the traced loop's edges face on their own side — the
/// sector being subdivided. Ties break to the lowest index (HashMap order is not
/// deterministic).
fn traced_side_sector(map: &EditorMap, traced: &[Edge]) -> Option<u32> {
    let mut counts: HashMap<u32, u32> = HashMap::new();
    for e in traced {
        if let Some(s) = side_sector(map, *e) {
            *counts.entry(s).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))
        .map(|(s, _)| s)
}

/// The distinct existing sectors the traced loop borders along its frozen (not
/// newly-created) sides. New edges carry stale inherited sides and are skipped.
fn bordering_sectors(map: &EditorMap, traced: &[Edge], new_set: &HashSet<u32>) -> Vec<u32> {
    let mut found: Vec<u32> = Vec::new();
    for e in traced.iter().filter(|e| !new_set.contains(&e.line)) {
        if let Some(s) = side_sector(map, *e)
            && !found.contains(&s)
        {
            found.push(s);
        }
    }
    found
}

/// A neighbouring sector's record to copy surfaces/light from: the sector on
/// either side of any traced line.
fn copy_sector(map: &EditorMap, traced: &[Edge]) -> Option<Sector> {
    for e in traced {
        for side in [Side::Front, Side::Back] {
            if let Some(s) = side_sector(
                map,
                Edge {
                    line: e.line,
                    side,
                },
            ) {
                return map.sectors.get(s as usize).copied();
            }
        }
    }
    None
}

/// Read the sector a directed side references.
fn side_sector(map: &EditorMap, edge: Edge) -> Option<u32> {
    let line = &map.lines[edge.line as usize];
    match edge.side {
        Side::Front => line.front.sector,
        Side::Back => {
            let b = line.back?;
            b.sector
        }
    }
}

/// Write the sector for a directed side, keeping the two-sided flag and the back
/// `SideDef` in sync: a back sector makes the line two-sided; clearing it drops
/// the back side and the flag.
fn set_side(map: &mut EditorMap, edge: Edge, sector: Option<u32>) {
    let line = &mut map.lines[edge.line as usize];
    match edge.side {
        Side::Front => line.front.sector = sector,
        Side::Back if sector.is_some() => {
            let mut back = line.back.unwrap_or(line.front);
            back.sector = sector;
            line.back = Some(back);
            line.flags.insert(LineFlags::TWO_SIDED);
        }
        Side::Back => {
            line.back = None;
            line.flags.remove(LineFlags::TWO_SIDED);
        }
    }
}

/// Reverse line `idx`'s direction (and swap its sides if two-sided), so what
/// faced the back now faces the front. Used to retry sectoring on a line whose
/// front bounded nothing.
fn flip_line(map: &mut EditorMap, idx: u32) {
    let line = &mut map.lines[idx as usize];
    mem::swap(&mut line.v1, &mut line.v2);
    if let Some(back) = line.back.take() {
        line.back = Some(line.front);
        line.front = back;
    }
}

/// Flip line `idx` when it carries a sector only on its back (the Doom
/// front-side invariant); two-sided and front-facing lines are untouched.
fn flip_if_back_only(map: &mut EditorMap, idx: u32) {
    let line = &mut map.lines[idx as usize];
    if line.front.sector.is_none()
        && let Some(back) = line.back
        && back.sector.is_some()
    {
        mem::swap(&mut line.v1, &mut line.v2);
        line.front = back;
        line.back = None;
        line.flags.remove(LineFlags::TWO_SIDED);
    }
}

/// Position of an edge in the new-line edge list, if present.
fn edge_index(edges: &[Edge], edge: Edge) -> Option<usize> {
    edges.iter().position(|e| *e == edge)
}

/// The vertex an edge points at.
fn edge_dst(map: &EditorMap, edge: Edge) -> u32 {
    let line = &map.lines[edge.line as usize];
    match edge.side {
        Side::Front => line.v2,
        Side::Back => line.v1,
    }
}

/// The vertex an edge starts from.
fn edge_src(map: &EditorMap, edge: Edge) -> u32 {
    let line = &map.lines[edge.line as usize];
    match edge.side {
        Side::Front => line.v1,
        Side::Back => line.v2,
    }
}

/// The shoelace contribution of `edge` in its travel direction; the total's sign
/// is the outline's winding.
fn shoelace_term(map: &EditorMap, edge: Edge) -> f32 {
    let l = &map.lines[edge.line as usize];
    let (a, b) = (map.vertices[l.v1 as usize], map.vertices[l.v2 as usize]);
    match edge.side {
        Side::Front => a.x * b.y - b.x * a.y,
        Side::Back => b.x * a.y - a.x * b.y,
    }
}

/// The rightmost of `current` and line `idx`'s two endpoints, by x.
fn right_vertex(map: &EditorMap, current: u32, idx: u32) -> u32 {
    let l = &map.lines[idx as usize];
    let mut best = current;
    for v in [l.v1, l.v2] {
        if map.vertices[v as usize].x > map.vertices[best as usize].x {
            best = v;
        }
    }
    best
}

/// The world midpoint of line `idx`.
fn line_midpoint(map: &EditorMap, idx: u32) -> [f32; 2] {
    let (a, b) = segment_points(map, idx);
    [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5]
}

/// Distance from `p` to line `idx`'s segment.
fn seg_distance(map: &EditorMap, idx: u32, p: [f32; 2]) -> f32 {
    let (a, b) = segment_points(map, idx);
    distance_to_segment(p, a, b)
}

/// The interior angle (0..2π) at vertex `here` between the incoming edge from
/// `prev` and the outgoing edge to `next`: the unsigned angle between the two
/// edge vectors out of `here`, with the determinant sign deciding which side, so
/// the smallest value is the tightest turn that keeps the trace hugging one
/// region.
fn turn_angle(prev: [f32; 2], here: [f32; 2], next: [f32; 2]) -> f32 {
    let ab = [here[0] - prev[0], here[1] - prev[1]];
    let cb = [here[0] - next[0], here[1] - next[1]];
    let dot = ab[0] * cb[0] + ab[1] * cb[1];
    let det = ab[0] * cb[1] - ab[1] * cb[0];
    // Unsigned angle between the two vectors in [0, π], then resolve the side.
    let mut a = det.abs().atan2(dot);
    if det < 0.0 {
        a = TAU - a;
    }
    a
}

fn opposite(side: Side) -> Side {
    match side {
        Side::Front => Side::Back,
        Side::Back => Side::Front,
    }
}

/// The visited-bit for a side (front = 1, back = 2).
fn side_bit(side: Side) -> u8 {
    match side {
        Side::Front => 1,
        Side::Back => 2,
    }
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

    fn void_side() -> SideDef {
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
            front: void_side(),
            back: None,
        }
    }

    fn rec() -> Sector {
        Sector {
            floor_height: 0,
            floor_flat: Name8::EMPTY,
            ceil_height: 128,
            ceil_flat: Name8::EMPTY,
            light_level: 192,
            special: 0,
            tag: 0,
        }
    }

    /// The sector facing `interior` across line `idx`, via the front-is-right
    /// rule (the editor's own convention). `None` = that face is void.
    fn facing(map: &EditorMap, idx: u32, interior: [f32; 2]) -> Option<u32> {
        let l = &map.lines[idx as usize];
        let p1 = map.vertices[l.v1 as usize];
        let p2 = map.vertices[l.v2 as usize];
        if is_front_side(interior, [p1.x, p1.y], [p2.x, p2.y]) {
            l.front.sector
        } else {
            let b = l.back?;
            b.sector
        }
    }

    fn box_lines(base: u32) -> Vec<LineDef> {
        vec![
            line(base, base + 1),
            line(base + 1, base + 2),
            line(base + 2, base + 3),
            line(base + 3, base),
        ]
    }

    #[test]
    fn ccw_box_one_sector_on_interior_face() {
        let mut map = EditorMap {
            vertices: vec![vtx(0.0, 0.0), vtx(4.0, 0.0), vtx(4.0, 4.0), vtx(0.0, 4.0)],
            lines: box_lines(0),
            ..Default::default()
        };
        build_sectors(&mut map, &[0, 1, 2, 3], &[0, 1, 2, 3], rec());
        assert_eq!(map.sectors.len(), 1, "one enclosed box → one sector");
        for i in 0..4u32 {
            assert!(map.lines[i as usize].back.is_none(), "single-sided");
            assert_eq!(facing(&map, i, [2.0, 2.0]), Some(0));
        }
    }

    #[test]
    fn cw_box_one_sector_no_useless_flip() {
        let mut map = EditorMap {
            vertices: vec![vtx(0.0, 0.0), vtx(0.0, 4.0), vtx(4.0, 4.0), vtx(4.0, 0.0)],
            lines: box_lines(0),
            ..Default::default()
        };
        build_sectors(&mut map, &[0, 1, 2, 3], &[0, 1, 2, 3], rec());
        assert_eq!(map.sectors.len(), 1);
        for i in 0..4u32 {
            assert!(map.lines[i as usize].back.is_none(), "single-sided");
            assert_eq!(facing(&map, i, [2.0, 2.0]), Some(0));
        }
    }

    #[test]
    fn lone_line_no_sector() {
        let mut map = EditorMap {
            vertices: vec![vtx(0.0, 0.0), vtx(4.0, 0.0)],
            lines: vec![line(0, 1)],
            ..Default::default()
        };
        build_sectors(&mut map, &[0], &[0], rec());
        assert!(map.sectors.is_empty(), "no enclosure → no sector");
        assert_eq!(map.lines[0].front.sector, None);
        // A void line keeps its drawn direction (no spurious flip).
        assert_eq!((map.lines[0].v1, map.lines[0].v2), (0, 1));
    }

    #[test]
    fn open_chain_no_sector() {
        // An L: two connected lines, not closed.
        let mut map = EditorMap {
            vertices: vec![vtx(0.0, 0.0), vtx(4.0, 0.0), vtx(4.0, 4.0)],
            lines: vec![line(0, 1), line(1, 2)],
            ..Default::default()
        };
        build_sectors(&mut map, &[0, 1], &[0, 1], rec());
        assert!(map.sectors.is_empty());
        assert_eq!(map.lines[0].front.sector, None);
        assert_eq!(map.lines[1].front.sector, None);
    }

    /// Two boxes sharing the middle wall: tracing L0 front weaves through both as
    /// one clockwise outline. The expected edge sequence is pinned exactly.
    #[test]
    fn trace_shared_wall_is_one_outline() {
        let map = EditorMap {
            vertices: vec![
                vtx(0.0, 0.0),
                vtx(4.0, 0.0),
                vtx(4.0, 4.0),
                vtx(0.0, 4.0),
                vtx(8.0, 0.0),
                vtx(8.0, 4.0),
            ],
            lines: vec![
                line(0, 3),
                line(3, 2),
                line(2, 1),
                line(1, 0),
                line(1, 2),
                line(2, 5),
                line(5, 4),
                line(4, 1),
            ],
            ..Default::default()
        };
        let adj = Adjacency::build(&map);
        let o = trace_outline(
            &map,
            &adj,
            Edge {
                line: 0,
                side: Side::Front,
            },
        );
        let seq: Vec<(u32, bool)> = o
            .edges
            .iter()
            .map(|e| (e.line, e.side == Side::Front))
            .collect();
        // L0f L1f L2f L4f L5f L6f L7f L2b L4b L3f
        let expected = vec![
            (0, true),
            (1, true),
            (2, true),
            (4, true),
            (5, true),
            (6, true),
            (7, true),
            (2, false),
            (4, false),
            (3, true),
        ];
        assert_eq!(seq, expected);
        assert!(o.clockwise);
    }

    /// A CW box already split by a vertical divider wall (L6, v4->v5) into two
    /// halves: the divider is two-sided, one sector each side. This is the genuine
    /// two-room configuration the editor produces when a drawn line splits an
    /// existing room.
    #[test]
    fn slice_two_sectors_divider_two_sided() {
        let mut map = EditorMap {
            vertices: vec![
                vtx(0.0, 0.0),
                vtx(0.0, 4.0),
                vtx(4.0, 4.0),
                vtx(4.0, 0.0),
                vtx(2.0, 0.0),
                vtx(2.0, 4.0),
            ],
            lines: vec![
                line(0, 1),
                line(1, 5),
                line(5, 2),
                line(2, 3),
                line(3, 4),
                line(4, 0),
                line(4, 5), // divider
            ],
            ..Default::default()
        };
        build_sectors(
            &mut map,
            &(0..7).collect::<Vec<_>>(),
            &(0..7).collect::<Vec<_>>(),
            rec(),
        );
        assert_eq!(map.sectors.len(), 2, "two halves");
        let divider = &map.lines[6];
        assert!(divider.back.is_some(), "divider two-sided");
        assert!(divider.flags.contains(LineFlags::TWO_SIDED));
        let left = facing(&map, 6, [1.0, 2.0]);
        let right = facing(&map, 6, [3.0, 2.0]);
        assert!(left.is_some() && right.is_some());
        assert_ne!(left, right, "different sector each side of the divider");
    }

    #[test]
    fn bridge_between_rooms_void() {
        let mut map = EditorMap {
            vertices: vec![
                vtx(0.0, 0.0),
                vtx(4.0, 0.0),
                vtx(4.0, 4.0),
                vtx(0.0, 4.0),
                vtx(10.0, 0.0),
                vtx(14.0, 0.0),
                vtx(14.0, 4.0),
                vtx(10.0, 4.0),
            ],
            lines: vec![
                line(0, 1),
                line(1, 2),
                line(2, 3),
                line(3, 0),
                line(4, 5),
                line(5, 6),
                line(6, 7),
                line(7, 4),
                line(1, 4), // bridge
            ],
            ..Default::default()
        };
        build_sectors(
            &mut map,
            &(0..9).collect::<Vec<_>>(),
            &(0..9).collect::<Vec<_>>(),
            rec(),
        );
        assert_eq!(map.sectors.len(), 2, "two rooms, no phantom");
        let bridge = &map.lines[8];
        assert_eq!(bridge.front.sector, None, "bridge front void");
        assert!(bridge.back.is_none(), "bridge back void");
    }

    #[test]
    fn concave_zigzag_one_sector_inside() {
        // Box with a W-shaped right wall (two inward notches at x=140).
        let mut map = EditorMap {
            vertices: vec![
                vtx(0.0, 0.0),
                vtx(200.0, 0.0),
                vtx(140.0, 50.0),
                vtx(200.0, 100.0),
                vtx(140.0, 150.0),
                vtx(200.0, 200.0),
                vtx(0.0, 200.0),
            ],
            lines: vec![
                line(0, 1),
                line(1, 2),
                line(2, 3),
                line(3, 4),
                line(4, 5),
                line(5, 6),
                line(6, 0),
            ],
            ..Default::default()
        };
        build_sectors(
            &mut map,
            &(0..7).collect::<Vec<_>>(),
            &(0..7).collect::<Vec<_>>(),
            rec(),
        );
        assert_eq!(map.sectors.len(), 1, "one concave room");
        for i in 0..7u32 {
            assert!(map.lines[i as usize].back.is_none(), "single-sided wall");
            assert_eq!(
                map.lines[i as usize].front.sector,
                Some(0),
                "front faces room"
            );
        }
    }

    fn box_in_box() -> EditorMap {
        let mut lines = box_lines(0);
        lines.extend(box_lines(4));
        EditorMap {
            vertices: vec![
                vtx(0.0, 0.0),
                vtx(10.0, 0.0),
                vtx(10.0, 10.0),
                vtx(0.0, 10.0),
                vtx(3.0, 3.0),
                vtx(7.0, 3.0),
                vtx(7.0, 7.0),
                vtx(3.0, 7.0),
            ],
            lines,
            ..Default::default()
        }
    }

    /// Box-in-box (CCW), pinned trace sequences:
    ///   L0 back  → 0b 3b 2b 1b 5f 6f 7f 4f   (the ring: outer inner + inner outer)
    ///   L4 back  → 4b 7b 6b 5b               (the inner box interior)
    #[test]
    fn box_in_box_trace_sequences() {
        let map = box_in_box();
        let adj = Adjacency::build(&map);
        let ring = trace_sector(
            &map,
            &adj,
            Edge {
                line: 0,
                side: Side::Back,
            },
        )
        .expect("L0 back bounds the ring");
        let ring_seq: Vec<(u32, bool)> = ring
            .iter()
            .map(|e| (e.line, e.side == Side::Front))
            .collect();
        assert_eq!(
            ring_seq,
            vec![
                (0, false),
                (3, false),
                (2, false),
                (1, false),
                (5, true),
                (6, true),
                (7, true),
                (4, true),
            ],
            "ring trace"
        );
        let inner = trace_sector(
            &map,
            &adj,
            Edge {
                line: 4,
                side: Side::Back,
            },
        )
        .expect("L4 back bounds the inner interior");
        let inner_seq: Vec<(u32, bool)> = inner
            .iter()
            .map(|e| (e.line, e.side == Side::Front))
            .collect();
        assert_eq!(
            inner_seq,
            vec![(4, false), (7, false), (6, false), (5, false)],
            "inner interior trace"
        );
        // L4 front → 4f 5f 6f 7f 1b 0b 3b 2b (the ring, via the outward walk).
        let ring_f = trace_sector(
            &map,
            &adj,
            Edge {
                line: 4,
                side: Side::Front,
            },
        )
        .expect("L4 front bounds the ring");
        let ring_f_seq: Vec<(u32, bool)> = ring_f
            .iter()
            .map(|e| (e.line, e.side == Side::Front))
            .collect();
        assert_eq!(
            ring_f_seq,
            vec![
                (4, true),
                (5, true),
                (6, true),
                (7, true),
                (1, false),
                (0, false),
                (3, false),
                (2, false),
            ],
            "L4 front ring trace"
        );
    }

    /// Box-in-box drawn in one pass yields a single sector — the ring between the
    /// boxes. The trace of the inner box's outer face folds in the outer box's
    /// inner faces; the inner box's interior is never put in the edge list (its
    /// wall midpoints are not inside any sector at build time), so it stays void.
    /// Drawing the inner box as a separate pass (its midpoints then inside the
    /// ring) is what makes the interior its own sector.
    #[test]
    fn box_in_box_one_pass_is_ring_only() {
        let mut map = box_in_box();
        build_sectors(
            &mut map,
            &(0..8).collect::<Vec<_>>(),
            &(0..8).collect::<Vec<_>>(),
            rec(),
        );
        assert_eq!(map.sectors.len(), 1, "the ring is one sector");
        let ring = facing(&map, 0, [5.0, 0.5]).expect("outer wall inner face is the ring");
        assert_eq!(
            facing(&map, 4, [5.0, 0.5]),
            Some(ring),
            "inner box outer face is the ring"
        );
        assert_eq!(facing(&map, 0, [5.0, -1.0]), None, "outside outer box void");
        assert_eq!(
            facing(&map, 4, [5.0, 5.0]),
            None,
            "inner interior void in one pass"
        );
    }

    #[test]
    fn add_sector_in_enclosure_fills_an_empty_box() {
        // A closed CCW box of bare (void) lines; clicking inside fills it.
        let mut map = EditorMap {
            vertices: vec![vtx(0.0, 0.0), vtx(8.0, 0.0), vtx(8.0, 8.0), vtx(0.0, 8.0)],
            lines: vec![line(0, 1), line(1, 2), line(2, 3), line(3, 0)],
            ..Default::default()
        };
        let new = add_sector_in_enclosure(&mut map, [4.0, 4.0], rec());
        assert_eq!(new, Some(0), "first sector pushed");
        assert_eq!(map.sectors.len(), 1);
        // The interior is now resolvable as that sector.
        assert_eq!(sector_at(&map, [4.0, 4.0]), Some(0));
        // Clicking outside the box does not fill (open void).
        let mut empty = EditorMap {
            vertices: map.vertices.clone(),
            lines: vec![line(0, 1)],
            ..Default::default()
        };
        assert_eq!(
            add_sector_in_enclosure(&mut empty, [4.0, -4.0], rec()),
            None
        );
    }

    /// Two disjoint boxes built as their own sectors, then collapsed onto one
    /// sector index — a correctly-oriented two-loop sector (as a merge produces).
    fn two_boxes_one_sector() -> EditorMap {
        let mut lines = box_lines(0);
        lines.extend(box_lines(4));
        let mut map = EditorMap {
            vertices: vec![
                vtx(0.0, 0.0),
                vtx(4.0, 0.0),
                vtx(4.0, 4.0),
                vtx(0.0, 4.0),
                vtx(8.0, 0.0),
                vtx(12.0, 0.0),
                vtx(12.0, 4.0),
                vtx(8.0, 4.0),
            ],
            lines,
            ..Default::default()
        };
        build_sectors(
            &mut map,
            &(0..8).collect::<Vec<_>>(),
            &(0..8).collect::<Vec<_>>(),
            rec(),
        );
        assert_eq!(map.sectors.len(), 2, "two boxes, two sectors");
        for l in &mut map.lines {
            if l.front.sector == Some(1) {
                l.front.sector = Some(0);
            }
            if let Some(b) = l.back.as_mut()
                && b.sector == Some(1)
            {
                b.sector = Some(0);
            }
        }
        map.sectors.truncate(1);
        map
    }

    #[test]
    fn sector_loop_count_counts_disjoint_loops() {
        let map = two_boxes_one_sector();
        assert_eq!(
            sector_loop_count(&map, 0),
            2,
            "two disjoint boxes, one sector"
        );
    }

    #[test]
    fn sector_loop_count_single_box_is_one() {
        let mut map = EditorMap {
            vertices: vec![vtx(0.0, 0.0), vtx(4.0, 0.0), vtx(4.0, 4.0), vtx(0.0, 4.0)],
            lines: box_lines(0),
            ..Default::default()
        };
        build_sectors(&mut map, &[0, 1, 2, 3], &[0, 1, 2, 3], rec());
        assert_eq!(sector_loop_count(&map, 0), 1);
    }

    #[test]
    fn unmerge_splits_one_loop_off() {
        let mut map = two_boxes_one_sector();
        // A point inside the second box separates that loop into its own sector.
        let new = unmerge_sector_at(&mut map, [10.0, 2.0]).expect("two loops to split");
        assert_eq!(new, 1, "new sector pushed");
        assert_eq!(
            sector_at(&map, [2.0, 2.0]),
            Some(0),
            "first box keeps sector 0"
        );
        assert_eq!(sector_at(&map, [10.0, 2.0]), Some(1), "second box is new");
        assert_eq!(sector_loop_count(&map, 0), 1, "each sector now one loop");
        assert_eq!(sector_loop_count(&map, 1), 1);
    }

    #[test]
    fn unmerge_single_loop_is_noop() {
        let mut map = EditorMap {
            vertices: vec![vtx(0.0, 0.0), vtx(4.0, 0.0), vtx(4.0, 4.0), vtx(0.0, 4.0)],
            lines: box_lines(0),
            ..Default::default()
        };
        build_sectors(&mut map, &[0, 1, 2, 3], &[0, 1, 2, 3], rec());
        assert_eq!(unmerge_sector_at(&mut map, [2.0, 2.0]), None);
        assert_eq!(map.sectors.len(), 1);
    }
}
