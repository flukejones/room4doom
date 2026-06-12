//! Build sectors from drawn lines by an angular trace of directed line sides (an [`Edge`]): from one, follow connected lines by smallest turning angle to walk the closed boundary, ray-cast east from the rightmost vertex to step to the next boundary outward, and fold traced inner contours (holes) into the same sector; a ray escaping the map means the side bounds nothing (void), and which side faces a room's interior is decided by the trace itself. [`build_sectors`] re-sectors only its `newly_created` lines — every other line is frozen, read as trace context but never rewritten, so a move never re-derives an authored void pocket; a loop bounded only by bare frozen walls stays void (SLADE's rule), while a genuinely new enclosed loop gets its own sector rather than reusing the enclosing one.

use std::collections::{HashMap, HashSet};
use std::f32::consts::TAU;
use std::mem;

use crate::flags::LineFlags;
use crate::geom::{
    dedup_coincident_lines, distance_to_segment, flip_line, is_front_side, ring_signed_area,
    sector_at, segment_points,
};
use crate::model::{EditorMap, LineKey, Sector, SectorKey, VertKey};

/// East-ray distance tie tolerance.
const RAY_TIE: f32 = 0.001;
/// East reference offset for the angle comparison; any positive value works.
const EAST_OFFSET: f32 = 32.0;

/// Which directed side of a line an [`Edge`] walks: `Front` goes `v1`->`v2`, `Back` goes `v2`->`v1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Side {
    Front,
    Back,
}

/// A directed line side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Edge {
    pub line: LineKey,
    pub side: Side,
}

/// A sector's closed boundary ring as ordered vertex keys, with winding: outer rings are counter-clockwise, holes clockwise. Returned by [`sector_loops`] for triangulation/rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectorLoop {
    /// Vertex keys in trace order.
    pub verts: Vec<VertKey>,
    /// Winding in Doom map space (Y-up): outer rings are CCW, holes CW.
    pub clockwise: bool,
}

/// One traced boundary: its ordered directed sides, winding, and rightmost vertex (the ray-east origin for stepping outward).
struct Outline {
    edges: Vec<Edge>,
    clockwise: bool,
    rightmost: VertKey,
}

/// Vertex → the lines meeting at it, built once per build pass.
struct Adjacency {
    lines_at: HashMap<VertKey, Vec<LineKey>>,
}

impl Adjacency {
    fn build(map: &EditorMap) -> Self {
        let mut lines_at: HashMap<VertKey, Vec<LineKey>> = HashMap::new();
        for (k, line) in map.lines.iter() {
            lines_at.entry(line.v1).or_default().push(k);
            lines_at.entry(line.v2).or_default().push(k);
        }
        Self {
            lines_at,
        }
    }

    fn at(&self, v: VertKey) -> &[LineKey] {
        self.lines_at.get(&v).map(Vec::as_slice).unwrap_or(&[])
    }
}

/// How a traced loop bounded by bare frozen walls is treated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoidRule {
    /// Bare-frozen-bounded loops stay void (moves must not re-derive authored pockets).
    KeepPockets,
    /// A drawn closed loop always sectors; a bare frozen wall it closes against goes two-sided.
    SectorDrawnLoops,
}

/// Re-sector after an edit: only `newly_created` lines may gain, lose, or change a sector; every line not in the set is frozen — read while tracing (a new loop reuses a bordering sector) but never written, so a void pocket stays void when a nearby edit re-sectors.
pub fn build_sectors(
    map: &mut EditorMap,
    newly_created: &[LineKey],
    default_record: Sector,
    void_rule: VoidRule,
) {
    // A drawn edge over an existing wall leaves a coincident twin; two lines between the same vertices make the angular trace weave both, so fold BEFORE tracing.
    dedup_coincident_lines(map);
    let newly: Vec<LineKey> = newly_created
        .iter()
        .copied()
        .filter(|&k| map.lines.contains(k))
        .collect();
    let new_set: HashSet<LineKey> = newly.iter().copied().collect();
    // Both passes share one adjacency: flips only swap within a line, so vertex incidence never changes.
    let adj = Adjacency::build(map);
    correct_sectors(map, &adj, &newly, &new_set, default_record, void_rule);

    let flipped: Vec<LineKey> = newly
        .iter()
        .copied()
        .filter(|&l| map.lines[l].front.sector.is_none())
        .collect();
    for &line in &flipped {
        flip_line(map, line);
    }
    if !flipped.is_empty() {
        let flipped_set: HashSet<LineKey> = flipped.iter().copied().collect();
        correct_sectors(map, &adj, &flipped, &flipped_set, default_record, void_rule);
        for &line in &flipped {
            if map.lines[line].front.sector.is_none() {
                flip_line(map, line);
            }
        }
    }

    map.prune_unused_sectors();
}

/// Add a sector to the enclosed but sectorless space containing `world`: traces the boundary loop facing the cursor, and if it closes (clockwise), inserts a sector with `default_record` and assigns it to the cursor-side of each bounding line, leaving the opposite side untouched. Returns the new sector key, or `None` when the point is already sectored, in open void, or has no nearby boundary.
pub fn add_sector_in_enclosure(
    map: &mut EditorMap,
    world: [f32; 2],
    default_record: Sector,
) -> Option<SectorKey> {
    if sector_at(map, world).is_some() {
        return None;
    }
    let edges = trace_loop_at(map, world)?;
    let new = map.sectors.insert(default_record);
    for edge in &edges {
        let line = &map.lines[edge.line];
        // A one-sided line walked on its back with a void front flips inward; every other case writes the cursor side, promoting a divider (e.g. a pillar) to two-sided so both sectors keep a side.
        if edge.side == Side::Back && line.back.is_none() && line.front.sector.is_none() {
            flip_line(map, edge.line);
            map.lines[edge.line].front.sector = Some(new);
        } else {
            set_side(map, *edge, Some(new));
        }
    }
    Some(new)
}

/// Trace the boundary loop whose cursor-facing side encloses `world`: the nearest line by segment distance, walked on the side facing the cursor. `None` when the map has no lines or that side faces open void (its east-ray escapes the map).
fn trace_loop_at(map: &EditorMap, world: [f32; 2]) -> Option<Vec<Edge>> {
    let nearest = map
        .lines
        .keys()
        .min_by(|&a, &b| seg_distance(map, a, world).total_cmp(&seg_distance(map, b, world)))?;
    let (p1, p2) = segment_points(map, nearest);
    // The directed side whose enclosed region contains the cursor: the Front walk (v1->v2) bounds the area on its right, matching `is_front_side`.
    let side = if is_front_side(world, p1, p2) {
        Side::Front
    } else {
        Side::Back
    };
    let adj = Adjacency::build(map);
    trace_sector(
        map,
        &adj,
        Edge {
            line: nearest,
            side,
        },
    )
}

/// Split the boundary loop containing `world` off its sector into a new one: traces the cursor-side loop and reassigns just that side of each bounding line to a fresh sector copying the current record, the rest of the old sector keeps it. Returns the new sector key, or `None` when the point is in the void or its sector is a single loop (nothing to separate).
pub fn unmerge_sector_at(map: &mut EditorMap, world: [f32; 2]) -> Option<SectorKey> {
    let cur = sector_at(map, world)?;
    if sector_loop_count(map, cur) < 2 {
        return None;
    }
    let edges = trace_loop_at(map, world)?;
    let record = map.sectors[cur];
    let new = map.sectors.insert(record);
    for edge in &edges {
        set_side(map, *edge, Some(new));
    }
    Some(new)
}

/// Whether the sector under `world` has more than one boundary loop — the gate for separating one loop off (`unmerge_sector_at`).
pub fn sector_under_cursor_has_separable_loop(map: &EditorMap, world: [f32; 2]) -> bool {
    sector_at(map, world).is_some_and(|s| sector_loop_count(map, s) > 1)
}

/// The number of disjoint boundary loops bounding `sector`: one is a simple room, two or more means separate enclosures share the sector (e.g. a merge).
pub(crate) fn sector_loop_count(map: &EditorMap, sector: SectorKey) -> usize {
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

/// Every closed boundary ring of `sector`, as ordered vertex-key loops (outer CCW, holes CW): the sector's directed boundary edges are chained head-to-tail, resolving a shared vertex by the smallest left turn so the walk hugs the sector; open geometry contributes no ring.
pub fn sector_loops(map: &EditorMap, sector: SectorKey) -> Vec<SectorLoop> {
    loops_from_edges(map, &sector_edges(map, sector))
}

/// Closed boundary loops for every sector in one pass, in sector slot order: equivalent to calling [`sector_loops`] per sector, but edges are bucketed by sector in a single line scan, avoiding the per-sector full-line rescan that makes the per-call approach quadratic on large maps.
pub fn sector_loops_all(map: &EditorMap) -> Vec<(SectorKey, Vec<SectorLoop>)> {
    let mut by_sector: HashMap<SectorKey, Vec<Edge>> = HashMap::new();
    for (k, l) in map.lines.iter() {
        if let Some(s) = l.front.sector {
            by_sector.entry(s).or_default().push(Edge {
                line: k,
                side: Side::Front,
            });
        }
        if let Some(s) = l.back.and_then(|b| b.sector) {
            by_sector.entry(s).or_default().push(Edge {
                line: k,
                side: Side::Back,
            });
        }
    }
    map.sectors
        .keys()
        .map(|s| {
            let loops = by_sector
                .get(&s)
                .map(|edges| loops_from_edges(map, edges))
                .unwrap_or_default();
            (s, loops)
        })
        .collect()
}

/// Closed boundary loops for just `sectors`, bucketed in one line scan: the batch form of [`sector_loops`] for a dirty subset (e.g. retriangulation), avoiding its per-sector full-line rescan. Every requested key gets an entry; a sector with no edges maps to an empty Vec.
pub fn sector_loops_for(
    map: &EditorMap,
    sectors: &[SectorKey],
) -> HashMap<SectorKey, Vec<SectorLoop>> {
    let wanted: HashSet<SectorKey> = sectors.iter().copied().collect();
    let mut by_sector: HashMap<SectorKey, Vec<Edge>> = HashMap::new();
    for (k, l) in map.lines.iter() {
        if let Some(s) = l.front.sector.filter(|s| wanted.contains(s)) {
            by_sector.entry(s).or_default().push(Edge {
                line: k,
                side: Side::Front,
            });
        }
        if let Some(s) = l.back.and_then(|b| b.sector).filter(|s| wanted.contains(s)) {
            by_sector.entry(s).or_default().push(Edge {
                line: k,
                side: Side::Back,
            });
        }
    }
    sectors
        .iter()
        .map(|&s| {
            let loops = by_sector
                .get(&s)
                .map(|edges| loops_from_edges(map, edges))
                .unwrap_or_default();
            (s, loops)
        })
        .collect()
}

/// Chain a sector's directed edges into closed boundary loops: the edges are this sector's `(line, side)` pairs, each consumed once and walked by smallest turning angle; rings that fail to close are dropped.
fn loops_from_edges(map: &EditorMap, edges: &[Edge]) -> Vec<SectorLoop> {
    // Index the directed edges by their start vertex for the chaining walk.
    let mut by_start: HashMap<VertKey, Vec<usize>> = HashMap::new();
    for (i, &e) in edges.iter().enumerate() {
        by_start.entry(edge_start(map, e)).or_default().push(i);
    }

    let mut used = vec![false; edges.len()];
    let mut loops = Vec::new();
    for i0 in 0..edges.len() {
        if used[i0] {
            continue;
        }
        let mut ring: Vec<VertKey> = Vec::new();
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

/// At vertex `to` (the arriving edge's end), the unused candidate continuing the sector boundary with the smallest left turn from the incoming direction.
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

/// Direction of a directed edge (start → end), not normalised.
fn edge_dir(map: &EditorMap, e: Edge) -> [f32; 2] {
    let a = map.vertices[edge_start(map, e)];
    let b = map.vertices[edge_end(map, e)];
    [b.x - a.x, b.y - a.y]
}

/// The leading vertex of a directed edge: `v1` on the front side, `v2` on the back.
fn edge_start(map: &EditorMap, edge: Edge) -> VertKey {
    let line = &map.lines[edge.line];
    match edge.side {
        Side::Front => line.v1,
        Side::Back => line.v2,
    }
}

/// The trailing vertex of a directed edge.
fn edge_end(map: &EditorMap, edge: Edge) -> VertKey {
    let line = &map.lines[edge.line];
    match edge.side {
        Side::Front => line.v2,
        Side::Back => line.v1,
    }
}

/// The directed sides facing `sector`: each line whose front or back references it.
fn sector_edges(map: &EditorMap, sector: SectorKey) -> Vec<Edge> {
    let mut edges = Vec::new();
    for (k, l) in map.lines.iter() {
        if l.front.sector == Some(sector) {
            edges.push(Edge {
                line: k,
                side: Side::Front,
            });
        }
        if l.back.is_some_and(|b| b.sector == Some(sector)) {
            edges.push(Edge {
                line: k,
                side: Side::Back,
            });
        }
    }
    edges
}

/// One sector-building pass: trace each new line's sides, sector or void each loop, and write only to new lines (frozen lines are read, never changed).
fn correct_sectors(
    map: &mut EditorMap,
    adj: &Adjacency,
    newly: &[LineKey],
    new_set: &HashSet<LineKey>,
    default_record: Sector,
    void_rule: VoidRule,
) {
    let mut edges: Vec<Edge> = Vec::new();
    for &line in newly {
        edges.push(Edge {
            line,
            side: Side::Front,
        });
        // Always try the back too: a side facing open void traces to None and the unclaimed cleanup clears it, so guessing sidedness up front is needless.
        edges.push(Edge {
            line,
            side: Side::Back,
        });
    }

    let mut claimed: HashSet<Edge> = HashSet::new();
    let mut reused: Vec<SectorKey> = Vec::new();
    for &edge in &edges {
        if claimed.contains(&edge) {
            continue;
        }
        let Some(traced) = trace_sector(map, adj, edge) else {
            continue;
        };
        for e in &traced {
            if new_set.contains(&e.line) {
                claimed.insert(*e);
            }
        }
        if void_rule == VoidRule::KeepPockets && encloses_void(map, &traced, new_set) {
            continue;
        }
        let bordering = bordering_sectors(map, &traced, new_set);
        let sector = choose_sector(map, &traced, &bordering, &mut reused, default_record);
        // A loop bordering two distinct existing sectors bridges them (frozen walls keep their own sector, only void sides take the new one); a loop within one sector (or none) re-sectors its new sides freely.
        let bridges = bordering.len() > 1;
        // When a split halves a sector, a frozen side still facing that sector belongs to whichever loop traced it; let it move to the chosen sector.
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

    for e in &edges {
        if !claimed.contains(e) {
            set_side(map, *e, None);
        }
    }

    for &line in newly {
        flip_if_back_only(map, line);
    }
}

/// A traced loop is void if its only frozen boundary walls are bare on the traced side — single-sided walls facing into the loop mean it is enclosed void (a void pocket's lobe), not a sector.
fn encloses_void(map: &EditorMap, traced: &[Edge], new_set: &HashSet<LineKey>) -> bool {
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

/// Trace the full sector boundary the `start` side bounds — outer outline plus any inner (hole) outlines — or `None` when the side faces the void. Trace the outline; while anticlockwise (an inner contour), step to the boundary outside via an east-ray ([`outer_edge`]) and re-trace, until it winds clockwise (the outermost), accumulating edges; a ray escaping the map means void → `None`. Then trace inner contours from the remaining rightmost vertices and fold them into the same sector.
fn trace_sector(map: &EditorMap, adj: &Adjacency, start: Edge) -> Option<Vec<Edge>> {
    let mut valid: HashSet<VertKey> = map.vertices.keys().collect();
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

/// Trace one closed outline from `start`, taking the smallest-angle turn at each vertex and reversing at dead ends (turn-at-ends); reports its winding (via a shoelace sum) and rightmost vertex. A line may be walked once per direction, tracked by a front/back bit per line.
fn trace_outline(map: &EditorMap, adj: &Adjacency, start: Edge) -> Outline {
    let mut edges = vec![start];
    // Per line: bit 0 = front walked, bit 1 = back walked.
    let mut visited: HashMap<LineKey, u8> = HashMap::new();
    let mut edge_sum = 0.0;
    let mut rightmost = map.lines[start.line].v1;
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
        *visited.entry(next.line).or_default() |= side_bit(next.side);

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

/// The next edge continuing the outline from `edge`: at its destination vertex, the connected line making the smallest turn. `None` when the destination is a dead end (only `edge.line` connects) or every candidate is exhausted.
fn next_edge(
    map: &EditorMap,
    adj: &Adjacency,
    edge: Edge,
    visited: &HashMap<LineKey, u8>,
) -> Option<Edge> {
    let dst = edge_end(map, edge);
    let prev = edge_start(map, edge);
    let here = map.vertices[dst];
    let here = [here.x, here.y];
    let prev = {
        let p = map.vertices[prev];
        [p.x, p.y]
    };

    let mut best: Option<(f32, Edge)> = None;
    for &line in adj.at(dst) {
        if line == edge.line {
            continue;
        }
        let l = &map.lines[line];
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
        if visited.get(&line).copied().unwrap_or(0) & side_bit(side) != 0 {
            continue;
        }
        let np = map.vertices[other];
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

/// The edge just outside an outline: ray east from `rightmost` to the nearest line it crosses, the side that line presents back toward the vertex. `None` when nothing lies east — the outline is the outermost, against the void.
fn outer_edge(map: &EditorMap, rightmost: VertKey) -> Option<Edge> {
    let v = map.vertices[rightmost];
    let (vx, vy) = (v.x, v.y);

    let mut nearest: Option<(f32, LineKey)> = None;
    for (k, line) in map.lines.iter() {
        let (p1, p2) = (map.vertices[line.v1], map.vertices[line.v2]);
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
            None => nearest = Some((dist, k)),
            Some((best, _)) if dist < best - RAY_TIE => nearest = Some((dist, k)),
            Some((best, best_line))
                if (dist - best).abs() <= RAY_TIE
                // Ray hit a shared vertex: prefer the line nearer the vertex, else picking an inner edge.
                && seg_distance(map, k, [vx, vy]) < seg_distance(map, best_line, [vx, vy]) =>
            {
                nearest = Some((dist, k));
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

/// The next inner-contour edge: from the rightmost still-valid vertex, the connected line whose direction is the smallest angle from due east. `None` once every vertex has been discarded.
fn inner_edge(map: &EditorMap, adj: &Adjacency, valid: &mut HashSet<VertKey>) -> Option<Edge> {
    loop {
        // Ties on x break to the lowest key so the trace start is deterministic.
        let rv = valid.iter().copied().reduce(|a, b| {
            let (ax, bx) = (map.vertices[a].x, map.vertices[b].x);
            if bx > ax || (bx == ax && b < a) { b } else { a }
        })?;
        let here = map.vertices[rv];
        let east = [here.x + EAST_OFFSET, here.y];
        let here = [here.x, here.y];

        let mut best: Option<(f32, Edge)> = None;
        for &line in adj.at(rv) {
            let l = &map.lines[line];
            if l.v1 == l.v2 {
                continue;
            }
            let (other, side) = if l.v1 == rv {
                (l.v2, Side::Front)
            } else {
                (l.v1, Side::Back)
            };
            let op = map.vertices[other];
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
            None => {
                valid.remove(&rv);
            }
        }
    }
}

/// Discard every vertex an outline's edges touch, so an inner-contour scan never restarts from a boundary already traced.
fn discard_outline_vertices(map: &EditorMap, outline: &Outline, valid: &mut HashSet<VertKey>) {
    for e in &outline.edges {
        let l = &map.lines[e.line];
        valid.remove(&l.v1);
        valid.remove(&l.v2);
    }
}

/// Discard every vertex lying outside `outline`, so inner-contour tracing only starts from vertices the current outline encloses; the bbox is computed once per outline, so the outermost (clockwise) trace cheaply cuts `valid` down to its bounds before any nearest-edge scan.
fn discard_outside(map: &EditorMap, outline: &Outline, valid: &mut HashSet<VertKey>) {
    let (min, max) = outline_bbox(map, outline);
    valid.retain(|&k| {
        let v = map.vertices[k];
        let p = [v.x, v.y];
        if p[0] < min[0] || p[0] > max[0] || p[1] < min[1] || p[1] > max[1] {
            // Outside the bbox: inside iff the outline is anticlockwise (it wraps everything not enclosed by the clockwise hole it bounds).
            return !outline.clockwise;
        }
        point_in_outline(map, outline, p)
    });
}

/// Axis-aligned bounds of an outline's segment endpoints.
fn outline_bbox(map: &EditorMap, outline: &Outline) -> ([f32; 2], [f32; 2]) {
    let (mut min, mut max) = ([f32::MAX, f32::MAX], [f32::MIN, f32::MIN]);
    let mut fold = |p: [f32; 2]| {
        min[0] = min[0].min(p[0]);
        min[1] = min[1].min(p[1]);
        max[0] = max[0].max(p[0]);
        max[1] = max[1].max(p[1]);
    };
    for e in &outline.edges {
        let (a, b) = segment_points(map, e.line);
        fold(a);
        fold(b);
    }
    (min, max)
}

/// Whether `p` — already inside the outline's bbox — is within `outline`: the side of the nearest edge.
fn point_in_outline(map: &EditorMap, outline: &Outline, p: [f32; 2]) -> bool {
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

/// Sector for a traced boundary: reuse one borne by a frozen traced side, else a fresh record copying a neighbour's, else `default_record`. An all-new loop has no frozen side, so it gets its own sector rather than the enclosing one's.
fn choose_sector(
    map: &mut EditorMap,
    traced: &[Edge],
    bordering: &[SectorKey],
    reused: &mut Vec<SectorKey>,
    default_record: Sector,
) -> SectorKey {
    // Reuse the loop's single bordering sector; a loop bridging two (or bordering none) gets a fresh record so distinct rooms never collapse together.
    if let [existing] = *bordering
        && !reused.contains(&existing)
    {
        reused.push(existing);
        return existing;
    }
    // A piece carved off an existing sector inherits its record; source order: the bordering sector, else the one the loop's traced sides already face (a re-traced ring carries it even when every edge is "new"), else a neighbour.
    let record = bordering
        .first()
        .copied()
        .or_else(|| traced_side_sector(map, traced))
        .and_then(|s| map.sectors.get(s).copied())
        .or_else(|| copy_sector(map, traced))
        .unwrap_or(default_record);
    map.sectors.insert(record)
}

/// The most common sector the traced loop's edges face on their own side — the sector being subdivided. Ties break to the lowest key (HashMap order is not deterministic).
fn traced_side_sector(map: &EditorMap, traced: &[Edge]) -> Option<SectorKey> {
    let mut counts: HashMap<SectorKey, u32> = HashMap::new();
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

/// The distinct existing sectors the traced loop borders along its frozen (not newly-created) sides. New edges carry stale inherited sides and are skipped.
fn bordering_sectors(
    map: &EditorMap,
    traced: &[Edge],
    new_set: &HashSet<LineKey>,
) -> Vec<SectorKey> {
    let mut found: Vec<SectorKey> = Vec::new();
    for e in traced.iter().filter(|e| !new_set.contains(&e.line)) {
        if let Some(s) = side_sector(map, *e)
            && !found.contains(&s)
        {
            found.push(s);
        }
    }
    found
}

/// A neighbouring sector's record to copy surfaces/light from: the sector on either side of any traced line.
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
                return map.sectors.get(s).copied();
            }
        }
    }
    None
}

/// Read the sector a directed side references.
fn side_sector(map: &EditorMap, edge: Edge) -> Option<SectorKey> {
    let line = &map.lines[edge.line];
    match edge.side {
        Side::Front => line.front.sector,
        Side::Back => {
            let b = line.back?;
            b.sector
        }
    }
}

/// Write the sector for a directed side, keeping the two-sided flag and back `SideDef` in sync: a back sector makes the line two-sided; clearing it drops the back side and the flag.
fn set_side(map: &mut EditorMap, edge: Edge, sector: Option<SectorKey>) {
    let line = &mut map.lines[edge.line];
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

/// Flip `line` when it carries a sector only on its back (the Doom front-side invariant); two-sided and front-facing lines are untouched.
fn flip_if_back_only(map: &mut EditorMap, line: LineKey) {
    let line = &mut map.lines[line];
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

/// The shoelace contribution of `edge` in its travel direction; the total's sign is the outline's winding.
fn shoelace_term(map: &EditorMap, edge: Edge) -> f32 {
    let l = &map.lines[edge.line];
    let (a, b) = (map.vertices[l.v1], map.vertices[l.v2]);
    match edge.side {
        Side::Front => a.x * b.y - b.x * a.y,
        Side::Back => b.x * a.y - a.x * b.y,
    }
}

/// The rightmost of `current` and `line`'s two endpoints, by x.
fn right_vertex(map: &EditorMap, current: VertKey, line: LineKey) -> VertKey {
    let l = &map.lines[line];
    let mut best = current;
    for v in [l.v1, l.v2] {
        if map.vertices[v].x > map.vertices[best].x {
            best = v;
        }
    }
    best
}

/// Distance from `p` to `line`'s segment.
fn seg_distance(map: &EditorMap, line: LineKey, p: [f32; 2]) -> f32 {
    let (a, b) = segment_points(map, line);
    distance_to_segment(p, a, b)
}

/// The interior angle (0..2π) at vertex `here` between the incoming edge from `prev` and outgoing edge to `next`: the unsigned angle between the two edge vectors out of `here`, with the determinant sign deciding which side, so the smallest value is the tightest turn hugging one region.
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
    use crate::model::{DenseLineDef, Vertex};
    use crate::test_fixtures::{def_sector, dline_with, line_keys, vtx};

    fn dline(v1: u32, v2: u32) -> DenseLineDef {
        dline_with(v1, v2, LineFlags::empty(), None)
    }

    /// A distinctive record (light 192) so inheritance asserts can spot it.
    fn rec() -> Sector {
        Sector {
            light_level: 192,
            ..def_sector()
        }
    }

    fn fixture(vertices: Vec<Vertex>, lines: Vec<DenseLineDef>) -> EditorMap {
        crate::test_fixtures::fixture(vertices, lines, 0)
    }

    /// The sector facing `interior` across `line`, via the front-is-right rule.
    fn facing(map: &EditorMap, line: LineKey, interior: [f32; 2]) -> Option<SectorKey> {
        let l = &map.lines[line];
        let p1 = map.vertices[l.v1];
        let p2 = map.vertices[l.v2];
        if is_front_side(interior, [p1.x, p1.y], [p2.x, p2.y]) {
            l.front.sector
        } else {
            let b = l.back?;
            b.sector
        }
    }

    fn box_dlines(base: u32) -> Vec<DenseLineDef> {
        vec![
            dline(base, base + 1),
            dline(base + 1, base + 2),
            dline(base + 2, base + 3),
            dline(base + 3, base),
        ]
    }

    /// Build sectors over every line, treating all as newly created.
    fn build_all(map: &mut EditorMap) {
        let keys = line_keys(map);
        build_sectors(map, &keys, rec(), VoidRule::SectorDrawnLoops);
    }

    #[test]
    fn ccw_box_one_sector_on_interior_face() {
        let mut map = fixture(
            vec![vtx(0.0, 0.0), vtx(4.0, 0.0), vtx(4.0, 4.0), vtx(0.0, 4.0)],
            box_dlines(0),
        );
        build_all(&mut map);
        assert_eq!(map.sectors.len(), 1, "one enclosed box → one sector");
        let s = map.sectors.keys().next();
        for k in line_keys(&map) {
            assert!(map.lines[k].back.is_none(), "single-sided");
            assert_eq!(facing(&map, k, [2.0, 2.0]), s);
        }
    }

    #[test]
    fn cw_box_one_sector_no_useless_flip() {
        let mut map = fixture(
            vec![vtx(0.0, 0.0), vtx(0.0, 4.0), vtx(4.0, 4.0), vtx(4.0, 0.0)],
            box_dlines(0),
        );
        build_all(&mut map);
        assert_eq!(map.sectors.len(), 1);
        let s = map.sectors.keys().next();
        for k in line_keys(&map) {
            assert!(map.lines[k].back.is_none(), "single-sided");
            assert_eq!(facing(&map, k, [2.0, 2.0]), s);
        }
    }

    #[test]
    fn lone_line_no_sector() {
        let mut map = fixture(vec![vtx(0.0, 0.0), vtx(4.0, 0.0)], vec![dline(0, 1)]);
        let k = line_keys(&map)[0];
        let (v1, v2) = (map.lines[k].v1, map.lines[k].v2);
        build_all(&mut map);
        assert!(map.sectors.is_empty(), "no enclosure → no sector");
        assert_eq!(map.lines[k].front.sector, None);
        // A void line keeps its drawn direction (no spurious flip).
        assert_eq!((map.lines[k].v1, map.lines[k].v2), (v1, v2));
    }

    #[test]
    fn open_chain_no_sector() {
        // An L: two connected lines, not closed.
        let mut map = fixture(
            vec![vtx(0.0, 0.0), vtx(4.0, 0.0), vtx(4.0, 4.0)],
            vec![dline(0, 1), dline(1, 2)],
        );
        build_all(&mut map);
        assert!(map.sectors.is_empty());
        for k in line_keys(&map) {
            assert_eq!(map.lines[k].front.sector, None);
        }
    }

    /// Two boxes sharing the middle wall: tracing L0 front weaves through both as one clockwise outline; the expected edge sequence is pinned exactly.
    #[test]
    fn trace_shared_wall_is_one_outline() {
        let map = fixture(
            vec![
                vtx(0.0, 0.0),
                vtx(4.0, 0.0),
                vtx(4.0, 4.0),
                vtx(0.0, 4.0),
                vtx(8.0, 0.0),
                vtx(8.0, 4.0),
            ],
            vec![
                dline(0, 3),
                dline(3, 2),
                dline(2, 1),
                dline(1, 0),
                dline(1, 2),
                dline(2, 5),
                dline(5, 4),
                dline(4, 1),
            ],
        );
        let keys = line_keys(&map);
        let adj = Adjacency::build(&map);
        let o = trace_outline(
            &map,
            &adj,
            Edge {
                line: keys[0],
                side: Side::Front,
            },
        );
        let seq: Vec<(LineKey, bool)> = o
            .edges
            .iter()
            .map(|e| (e.line, e.side == Side::Front))
            .collect();
        // L0f L1f L2f L4f L5f L6f L7f L2b L4b L3f
        let expected = vec![
            (keys[0], true),
            (keys[1], true),
            (keys[2], true),
            (keys[4], true),
            (keys[5], true),
            (keys[6], true),
            (keys[7], true),
            (keys[2], false),
            (keys[4], false),
            (keys[3], true),
        ];
        assert_eq!(seq, expected);
        assert!(o.clockwise);
    }

    /// A CW box already split by a vertical divider wall into two halves: the divider is two-sided, one sector each side.
    #[test]
    fn slice_two_sectors_divider_two_sided() {
        let mut map = fixture(
            vec![
                vtx(0.0, 0.0),
                vtx(0.0, 4.0),
                vtx(4.0, 4.0),
                vtx(4.0, 0.0),
                vtx(2.0, 0.0),
                vtx(2.0, 4.0),
            ],
            vec![
                dline(0, 1),
                dline(1, 5),
                dline(5, 2),
                dline(2, 3),
                dline(3, 4),
                dline(4, 0),
                dline(4, 5), // divider
            ],
        );
        let divider = line_keys(&map)[6];
        build_all(&mut map);
        assert_eq!(map.sectors.len(), 2, "two halves");
        let d = &map.lines[divider];
        assert!(d.back.is_some(), "divider two-sided");
        assert!(d.flags.contains(LineFlags::TWO_SIDED));
        let left = facing(&map, divider, [1.0, 2.0]);
        let right = facing(&map, divider, [3.0, 2.0]);
        assert!(left.is_some() && right.is_some());
        assert_ne!(left, right, "different sector each side of the divider");
    }

    #[test]
    fn bridge_between_rooms_void() {
        let mut map = fixture(
            vec![
                vtx(0.0, 0.0),
                vtx(4.0, 0.0),
                vtx(4.0, 4.0),
                vtx(0.0, 4.0),
                vtx(10.0, 0.0),
                vtx(14.0, 0.0),
                vtx(14.0, 4.0),
                vtx(10.0, 4.0),
            ],
            vec![
                dline(0, 1),
                dline(1, 2),
                dline(2, 3),
                dline(3, 0),
                dline(4, 5),
                dline(5, 6),
                dline(6, 7),
                dline(7, 4),
                dline(1, 4), // bridge
            ],
        );
        let bridge = line_keys(&map)[8];
        build_all(&mut map);
        assert_eq!(map.sectors.len(), 2, "two rooms, no phantom");
        let b = &map.lines[bridge];
        assert_eq!(b.front.sector, None, "bridge front void");
        assert!(b.back.is_none(), "bridge back void");
    }

    #[test]
    fn concave_zigzag_one_sector_inside() {
        // Box with a W-shaped right wall (two inward notches at x=140).
        let mut map = fixture(
            vec![
                vtx(0.0, 0.0),
                vtx(200.0, 0.0),
                vtx(140.0, 50.0),
                vtx(200.0, 100.0),
                vtx(140.0, 150.0),
                vtx(200.0, 200.0),
                vtx(0.0, 200.0),
            ],
            vec![
                dline(0, 1),
                dline(1, 2),
                dline(2, 3),
                dline(3, 4),
                dline(4, 5),
                dline(5, 6),
                dline(6, 0),
            ],
        );
        build_all(&mut map);
        assert_eq!(map.sectors.len(), 1, "one concave room");
        let s = map.sectors.keys().next();
        for k in line_keys(&map) {
            assert!(map.lines[k].back.is_none(), "single-sided wall");
            assert_eq!(map.lines[k].front.sector, s, "front faces room");
        }
    }

    fn box_in_box() -> EditorMap {
        let mut lines = box_dlines(0);
        lines.extend(box_dlines(4));
        fixture(
            vec![
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
        )
    }

    /// Box-in-box (CCW), pinned trace sequences: L0 back → 0b 3b 2b 1b 5f 6f 7f 4f (the ring: outer inner + inner outer); L4 back → 4b 7b 6b 5b (the inner box interior).
    #[test]
    fn box_in_box_trace_sequences() {
        let map = box_in_box();
        let keys = line_keys(&map);
        let adj = Adjacency::build(&map);
        let ring = trace_sector(
            &map,
            &adj,
            Edge {
                line: keys[0],
                side: Side::Back,
            },
        )
        .expect("L0 back bounds the ring");
        let ring_seq: Vec<(LineKey, bool)> = ring
            .iter()
            .map(|e| (e.line, e.side == Side::Front))
            .collect();
        assert_eq!(
            ring_seq,
            vec![
                (keys[0], false),
                (keys[3], false),
                (keys[2], false),
                (keys[1], false),
                (keys[5], true),
                (keys[6], true),
                (keys[7], true),
                (keys[4], true),
            ],
            "ring trace"
        );
        let inner = trace_sector(
            &map,
            &adj,
            Edge {
                line: keys[4],
                side: Side::Back,
            },
        )
        .expect("L4 back bounds the inner interior");
        let inner_seq: Vec<(LineKey, bool)> = inner
            .iter()
            .map(|e| (e.line, e.side == Side::Front))
            .collect();
        assert_eq!(
            inner_seq,
            vec![
                (keys[4], false),
                (keys[7], false),
                (keys[6], false),
                (keys[5], false),
            ],
            "inner interior trace"
        );
        // L4 front → 4f 5f 6f 7f 1b 0b 3b 2b (the ring, via the outward walk).
        let ring_f = trace_sector(
            &map,
            &adj,
            Edge {
                line: keys[4],
                side: Side::Front,
            },
        )
        .expect("L4 front bounds the ring");
        let ring_f_seq: Vec<(LineKey, bool)> = ring_f
            .iter()
            .map(|e| (e.line, e.side == Side::Front))
            .collect();
        assert_eq!(
            ring_f_seq,
            vec![
                (keys[4], true),
                (keys[5], true),
                (keys[6], true),
                (keys[7], true),
                (keys[1], false),
                (keys[0], false),
                (keys[3], false),
                (keys[2], false),
            ],
            "L4 front ring trace"
        );
    }

    /// Box-in-box drawn in one pass: the ring and the inner interior are both sectors — a drawn closed loop always sectors.
    #[test]
    fn box_in_box_one_pass_sectors_ring_and_inner() {
        let mut map = box_in_box();
        let keys = line_keys(&map);
        build_all(&mut map);
        assert_eq!(map.sectors.len(), 2, "ring + inner interior");
        let ring = facing(&map, keys[0], [5.0, 0.5]).expect("outer wall inner face is the ring");
        assert_eq!(
            facing(&map, keys[4], [5.0, 0.5]),
            Some(ring),
            "inner box outer face is the ring"
        );
        assert_eq!(
            facing(&map, keys[0], [5.0, -1.0]),
            None,
            "outside outer box void"
        );
        let inner = facing(&map, keys[4], [5.0, 5.0]).expect("inner interior is a sector");
        assert_ne!(inner, ring, "inner interior distinct from the ring");
    }

    /// A one-sided pillar wall fronting its own sector, traced on its bare back by the fill: promoted to two-sided so both sectors keep a side.
    #[test]
    fn add_sector_in_enclosure_promotes_pillar_to_two_sided() {
        // Void outer box around a CW inner box whose fronts face its interior sector.
        let mut lines = box_dlines(0);
        lines.extend([
            dline_with(4, 7, LineFlags::empty(), Some(0)),
            dline_with(7, 6, LineFlags::empty(), Some(0)),
            dline_with(6, 5, LineFlags::empty(), Some(0)),
            dline_with(5, 4, LineFlags::empty(), Some(0)),
        ]);
        let mut map = crate::test_fixtures::fixture(
            vec![
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
            1,
        );
        let keys = line_keys(&map);
        let inner = map.sectors.keys().next().expect("pillar sector");
        let new = add_sector_in_enclosure(&mut map, [5.0, 0.5], rec()).expect("ring filled");
        assert_eq!(sector_at(&map, [5.0, 0.5]), Some(new), "ring sectored");
        assert_eq!(sector_at(&map, [5.0, 5.0]), Some(inner), "interior kept");
        for &k in &keys[4..8] {
            let l = &map.lines[k];
            assert_eq!(l.front.sector, Some(inner), "pillar keeps its front");
            assert_eq!(l.back.expect("promoted two-sided").sector, Some(new));
            assert!(l.flags.contains(LineFlags::TWO_SIDED));
        }
    }

    #[test]
    fn add_sector_in_enclosure_fills_an_empty_box() {
        // A closed CCW box of bare (void) lines; clicking inside fills it.
        let mut map = fixture(
            vec![vtx(0.0, 0.0), vtx(8.0, 0.0), vtx(8.0, 8.0), vtx(0.0, 8.0)],
            vec![dline(0, 1), dline(1, 2), dline(2, 3), dline(3, 0)],
        );
        let new = add_sector_in_enclosure(&mut map, [4.0, 4.0], rec());
        assert!(new.is_some(), "sector inserted");
        assert_eq!(map.sectors.len(), 1);
        // The interior is now resolvable as that sector.
        assert_eq!(sector_at(&map, [4.0, 4.0]), new);
        // Clicking outside the box does not fill (open void).
        let mut empty = fixture(
            vec![vtx(0.0, 0.0), vtx(8.0, 0.0), vtx(8.0, 8.0), vtx(0.0, 8.0)],
            vec![dline(0, 1)],
        );
        assert_eq!(
            add_sector_in_enclosure(&mut empty, [4.0, -4.0], rec()),
            None
        );
    }

    /// Two disjoint boxes built as their own sectors, then collapsed onto one sector key — a correctly-oriented two-loop sector (as a merge produces).
    fn two_boxes_one_sector() -> (EditorMap, SectorKey) {
        let mut lines = box_dlines(0);
        lines.extend(box_dlines(4));
        let mut map = fixture(
            vec![
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
        );
        build_all(&mut map);
        assert_eq!(map.sectors.len(), 2, "two boxes, two sectors");
        let keys: Vec<SectorKey> = map.sectors.keys().collect();
        let (keep, fold) = (keys[0], keys[1]);
        for l in map.lines.values_mut() {
            if l.front.sector == Some(fold) {
                l.front.sector = Some(keep);
            }
            if let Some(b) = l.back.as_mut()
                && b.sector == Some(fold)
            {
                b.sector = Some(keep);
            }
        }
        map.sectors.remove(fold);
        (map, keep)
    }

    #[test]
    fn sector_loops_for_matches_per_sector_calls() {
        let mut map = box_in_box();
        build_all(&mut map);
        let sectors: Vec<SectorKey> = map.sectors.keys().collect();
        let all = sector_loops_for(&map, &sectors);
        assert_eq!(all.len(), sectors.len());
        for &s in &sectors {
            assert_eq!(all[&s], sector_loops(&map, s));
        }
        let one = sector_loops_for(&map, &sectors[..1]);
        assert_eq!(one.len(), 1, "only the requested sector is bucketed");
        assert_eq!(one[&sectors[0]], sector_loops(&map, sectors[0]));
    }

    #[test]
    fn sector_loop_count_counts_disjoint_loops() {
        let (map, s) = two_boxes_one_sector();
        assert_eq!(
            sector_loop_count(&map, s),
            2,
            "two disjoint boxes, one sector"
        );
    }

    #[test]
    fn sector_loop_count_single_box_is_one() {
        let mut map = fixture(
            vec![vtx(0.0, 0.0), vtx(4.0, 0.0), vtx(4.0, 4.0), vtx(0.0, 4.0)],
            box_dlines(0),
        );
        build_all(&mut map);
        let s = map.sectors.keys().next().expect("one sector");
        assert_eq!(sector_loop_count(&map, s), 1);
    }

    #[test]
    fn unmerge_splits_one_loop_off() {
        let (mut map, s) = two_boxes_one_sector();
        // A point inside the second box separates that loop into its own sector.
        let new = unmerge_sector_at(&mut map, [10.0, 2.0]).expect("two loops to split");
        assert_eq!(
            sector_at(&map, [2.0, 2.0]),
            Some(s),
            "first box keeps its sector"
        );
        assert_eq!(sector_at(&map, [10.0, 2.0]), Some(new), "second box is new");
        assert_eq!(sector_loop_count(&map, s), 1, "each sector now one loop");
        assert_eq!(sector_loop_count(&map, new), 1);
    }

    #[test]
    fn unmerge_single_loop_is_noop() {
        let mut map = fixture(
            vec![vtx(0.0, 0.0), vtx(4.0, 0.0), vtx(4.0, 4.0), vtx(0.0, 4.0)],
            box_dlines(0),
        );
        build_all(&mut map);
        assert_eq!(unmerge_sector_at(&mut map, [2.0, 2.0]), None);
        assert_eq!(map.sectors.len(), 1);
    }
}
