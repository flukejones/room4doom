//! Portal collection and representation for the 2D BSP PVS system.
//!
//! A [`Portal`] is a shared boundary segment between two adjacent subsectors.
//! The [`Portals`] collection builds these from the BSP tree by projecting
//! carved subsector polygons onto each node's partition (divline) and finding
//! overlapping intervals across the left/right subtrees.

use crate::bsp3d::BSP3D;
use crate::map_defs::{Node, Segment, SubSector, is_subsector, subsector_index};
use glam::Vec2;
use std::collections::HashSet;

/// Max perpendicular distance from a carved polygon vertex to its parent
/// divline for the vertex to be considered "on" the divline.
const PERP_THRESHOLD: f32 = 2.0;

// ============================================================================
// PORTAL
// ============================================================================

/// A portal segment connecting two adjacent subsectors across a BSP divline.
///
/// Portals are the fundamental adjacency primitive for PVS computation.
/// Each portal has a directed segment (`v1`→`v2`) whose implicit normal
/// (`-dir.y`, `dir.x`) points consistently toward `subsector_b`.
#[derive(Clone, Debug)]
pub struct Portal {
    /// The subsector on the right side of the partition that generated this
    /// portal.
    pub subsector_a: usize,
    /// The subsector on the left side of the partition that generated this
    /// portal.
    pub subsector_b: usize,
    /// First endpoint of the portal segment.
    pub v1: Vec2,
    /// Second endpoint of the portal segment.
    pub v2: Vec2,
}

impl Portal {
    /// Return the subsector on the opposite side of this portal from `ss`.
    pub fn other(&self, ss: usize) -> usize {
        if ss == self.subsector_a {
            self.subsector_b
        } else {
            self.subsector_a
        }
    }

    /// Return the portal segment as a `(v1, v2)` tuple.
    pub fn segment(&self) -> (Vec2, Vec2) {
        (self.v1, self.v2)
    }
}

// ============================================================================
// PORTALS COLLECTION
// ============================================================================

/// Compressed adjacency structure mapping every subsector to its portals.
///
/// Internally uses a CSR (Compressed Sparse Row) layout:
/// `ids[offsets[ss]..offsets[ss+1]]` gives all portal indices for subsector
/// `ss`. This eliminates inner-Vec pointer indirection in hot flood loops.
pub struct Portals {
    portals: Vec<Portal>,
    offsets: Vec<u32>,
    ids: Vec<u32>,
    subsector_count: usize,
}

impl Default for Portals {
    fn default() -> Self {
        Self {
            portals: Vec::new(),
            offsets: Vec::new(),
            ids: Vec::new(),
            subsector_count: 0,
        }
    }
}

impl Portals {
    /// Build the portal graph by carving subsector polygons and projecting them
    /// onto every BSP partition line.
    ///
    /// Only portals between sectors connected by a two-sided linedef are
    /// created, preventing false visibility across solid walls.
    pub fn build(
        start_node: u32,
        nodes: &[Node],
        subsectors: &[SubSector],
        segments: &[Segment],
        bsp: &BSP3D,
    ) -> Self {
        let carved = &bsp.carved_polygons;
        let carved_bboxes = compute_carved_bboxes(start_node, nodes, carved);

        // Build set of sector pairs connected by two-sided linedefs.
        // Cross-sector portals are only created between connected sectors.
        let mut connected_sectors: HashSet<(i32, i32)> = HashSet::new();
        for seg in segments.iter() {
            if let Some(ref bs) = seg.backsector {
                let a = seg.frontsector.num;
                let b = bs.num;
                let pair = if a < b { (a, b) } else { (b, a) };
                connected_sectors.insert(pair);
            }
        }

        let (mut portals, _) = collect_divline_portals(
            start_node,
            nodes,
            subsectors,
            carved,
            &carved_bboxes,
            &connected_sectors,
        );
        dedup_portals(&mut portals);

        let n = subsectors.len();
        let mut rebuild_portal_vecs: Vec<Vec<usize>> = vec![Vec::new(); n];
        for (pi, p) in portals.iter().enumerate() {
            rebuild_portal_vecs[p.subsector_a].push(pi);
            rebuild_portal_vecs[p.subsector_b].push(pi);
        }
        let mut offsets = vec![0u32; n + 1];
        for (ss, ps) in rebuild_portal_vecs.iter().enumerate() {
            offsets[ss + 1] = offsets[ss] + ps.len() as u32;
        }
        let mut ids: Vec<u32> = Vec::with_capacity(offsets[n] as usize);
        for ps in &rebuild_portal_vecs {
            ids.extend(ps.iter().map(|&x| x as u32));
        }

        Self {
            portals,
            offsets,
            ids,
            subsector_count: n,
        }
    }

    /// Return a reference to the portal at index `idx`.
    pub fn get(&self, idx: usize) -> &Portal {
        &self.portals[idx]
    }

    /// Return the slice of portal indices for subsector `ss`.
    pub fn subsector_portals(&self, ss: usize) -> &[u32] {
        &self.ids[self.offsets[ss] as usize..self.offsets[ss + 1] as usize]
    }

    /// Total number of portals in the graph.
    pub fn len(&self) -> usize {
        self.portals.len()
    }

    /// Returns `true` if there are no portals.
    pub fn is_empty(&self) -> bool {
        self.portals.is_empty()
    }

    /// Number of subsectors this portal graph was built for.
    pub fn subsector_count(&self) -> usize {
        self.subsector_count
    }

    /// Iterate over all portals in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = &Portal> {
        self.portals.iter()
    }

    /// Raw slice of all portals; used in sibling modules on hot paths.
    pub(crate) fn portals_slice(&self) -> &[Portal] {
        &self.portals
    }

    /// Raw CSR offset array; used in sibling modules on hot paths.
    pub(crate) fn offsets(&self) -> &[u32] {
        &self.offsets
    }

    /// Raw CSR id array; used in sibling modules on hot paths.
    pub(crate) fn ids(&self) -> &[u32] {
        &self.ids
    }
}

// ============================================================================
// PHASE 2 — PORTAL COLLECTION (BSP divline projection)
// ============================================================================
//
// At each BSP node the partition line (divline) separates the left and right
// subtrees. We collect "frontier leaves" on each side — subsectors whose
// carved polygon touches the divline — and project them onto the divline
// direction to get 1D intervals. Overlapping intervals from opposite sides
// produce portals.

/// Deduplicate portals with the same `(subsector_a, subsector_b)` pair,
/// keeping the widest one (largest segment length) per pair. Ancestor BSP
/// nodes produce overlapping spans on the same divline for the same pair;
/// the widest span subsumes the narrower ones.
fn dedup_portals(portals: &mut Vec<Portal>) {
    use std::collections::HashMap;
    // Map (ss_a, ss_b) → index of the current best portal.
    let mut best: HashMap<(usize, usize), usize> = HashMap::with_capacity(portals.len());
    let mut keep = vec![true; portals.len()];

    for (i, p) in portals.iter().enumerate() {
        // Normalize key so (A,B) and (B,A) collide — ancestor BSP nodes may
        // produce the same portal pair with opposite subsector_a/b ordering.
        let key = (
            p.subsector_a.min(p.subsector_b),
            p.subsector_a.max(p.subsector_b),
        );
        match best.get(&key) {
            None => {
                best.insert(key, i);
            }
            Some(&j) => {
                let old_len = (portals[j].v2 - portals[j].v1).length_squared();
                let new_len = (p.v2 - p.v1).length_squared();
                if new_len > old_len {
                    keep[j] = false;
                    best.insert(key, i);
                } else {
                    keep[i] = false;
                }
            }
        }
    }

    let mut i = 0;
    portals.retain(|_| {
        let k = keep[i];
        i += 1;
        k
    });
}

/// Walk the BSP tree and collect all portals via divline projection.
fn collect_divline_portals(
    start_node: u32,
    nodes: &[Node],
    subsectors: &[SubSector],
    carved: &[Vec<Vec2>],
    carved_bboxes: &[[[Vec2; 2]; 2]],
    connected_sectors: &HashSet<(i32, i32)>,
) -> (Vec<Portal>, Vec<Vec<usize>>) {
    let n = subsectors.len();
    let mut portals: Vec<Portal> = Vec::new();
    let mut subsector_portals: Vec<Vec<usize>> = vec![Vec::new(); n];

    collect_divline_portals_recursive(
        start_node,
        nodes,
        subsectors,
        carved,
        carved_bboxes,
        connected_sectors,
        &mut portals,
        &mut subsector_portals,
    );

    (portals, subsector_portals)
}

fn collect_divline_portals_recursive(
    node_id: u32,
    nodes: &[Node],
    subsectors: &[SubSector],
    carved: &[Vec<Vec2>],
    carved_bboxes: &[[[Vec2; 2]; 2]],
    connected_sectors: &HashSet<(i32, i32)>,
    portals: &mut Vec<Portal>,
    subsector_portals: &mut Vec<Vec<usize>>,
) {
    if is_subsector(node_id) {
        return;
    }
    let Some(node) = nodes.get(node_id as usize) else {
        return;
    };

    let divline_dir = node.delta;
    let divline_origin = node.xy;
    let divline_len_sq = divline_dir.length_squared();

    if divline_len_sq > 1e-6 {
        // Collect frontier leaves on each side whose carved polygon touches
        // this node's divline.
        let mut right_leaves = Vec::new();
        let mut left_leaves = Vec::new();
        collect_frontier_leaves(
            node.children[0],
            nodes,
            carved,
            carved_bboxes,
            divline_origin,
            divline_dir,
            divline_len_sq,
            &mut right_leaves,
        );
        collect_frontier_leaves(
            node.children[1],
            nodes,
            carved,
            carved_bboxes,
            divline_origin,
            divline_dir,
            divline_len_sq,
            &mut left_leaves,
        );

        let inv_len = 1.0 / divline_len_sq.sqrt();

        // Create portals between right×left pairs with overlapping intervals.
        for &(right_id, r_min, r_max) in &right_leaves {
            for &(left_id, l_min, l_max) in &left_leaves {
                if right_id == left_id {
                    continue;
                }
                let t_min = r_min.max(l_min);
                let t_max = r_max.min(l_max);
                if t_max <= t_min + 1e-3 {
                    continue;
                }

                // Filter: same sector → always portal. Different sectors →
                // only if connected by a two-sided linedef.
                let sec_a = subsectors[right_id].sector.num;
                let sec_b = subsectors[left_id].sector.num;
                if sec_a != sec_b {
                    let pair = if sec_a < sec_b {
                        (sec_a, sec_b)
                    } else {
                        (sec_b, sec_a)
                    };
                    if !connected_sectors.contains(&pair) {
                        continue;
                    }
                }

                let v1 = divline_origin + divline_dir * (t_min * inv_len);
                let v2 = divline_origin + divline_dir * (t_max * inv_len);

                let pi = portals.len();
                // subsector_a = right child, subsector_b = left child.
                // The segment direction is divline_dir, so the normal
                // (-divline_dir.y, divline_dir.x) consistently points
                // toward subsector_b (left child).
                portals.push(Portal {
                    subsector_a: right_id,
                    subsector_b: left_id,
                    v1,
                    v2,
                });
                subsector_portals[right_id].push(pi);
                subsector_portals[left_id].push(pi);
            }
        }
    }

    // Recurse into children.
    collect_divline_portals_recursive(
        node.children[0],
        nodes,
        subsectors,
        carved,
        carved_bboxes,
        connected_sectors,
        portals,
        subsector_portals,
    );
    collect_divline_portals_recursive(
        node.children[1],
        nodes,
        subsectors,
        carved,
        carved_bboxes,
        connected_sectors,
        portals,
        subsector_portals,
    );
}

/// Walk a subtree collecting all leaf subsectors whose carved polygon touches
/// the divline. Uses carved bboxes for early pruning.
fn collect_frontier_leaves(
    child_id: u32,
    nodes: &[Node],
    carved: &[Vec<Vec2>],
    carved_bboxes: &[[[Vec2; 2]; 2]],
    divline_origin: Vec2,
    divline_dir: Vec2,
    divline_len_sq: f32,
    out: &mut Vec<(usize, f32, f32)>,
) {
    if is_subsector(child_id) {
        if child_id == u32::MAX {
            return;
        }
        let ss_id = subsector_index(child_id);
        if let Some((t_min, t_max)) = project_polygon_onto_divline(
            &carved[ss_id],
            divline_origin,
            divline_dir,
            divline_len_sq,
        ) {
            out.push((ss_id, t_min, t_max));
        }
        return;
    }
    let Some(node) = nodes.get(child_id as usize) else {
        return;
    };

    let node_bboxes = if (child_id as usize) < carved_bboxes.len() {
        &carved_bboxes[child_id as usize]
    } else {
        &node.bboxes
    };

    for side in 0..2 {
        if bbox_touches_divline(&node_bboxes[side], divline_origin, divline_dir) {
            collect_frontier_leaves(
                node.children[side],
                nodes,
                carved,
                carved_bboxes,
                divline_origin,
                divline_dir,
                divline_len_sq,
                out,
            );
        }
    }
}

/// Project a polygon's divline-adjacent vertices onto the divline direction.
/// Returns `(min_t, max_t)` or `None` if no vertices are close to the divline.
fn project_polygon_onto_divline(
    polygon: &[Vec2],
    divline_origin: Vec2,
    divline_dir: Vec2,
    divline_len_sq: f32,
) -> Option<(f32, f32)> {
    if polygon.is_empty() {
        return None;
    }
    let inv_len = 1.0 / divline_len_sq.sqrt();
    let normal = Vec2::new(-divline_dir.y, divline_dir.x) * inv_len;

    let mut min_t = f32::MAX;
    let mut max_t = f32::MIN;
    let mut has_close = false;

    for &v in polygon {
        let offset = v - divline_origin;
        let perp_dist = offset.dot(normal).abs();
        if perp_dist <= PERP_THRESHOLD {
            let t = offset.dot(divline_dir) * inv_len;
            min_t = min_t.min(t);
            max_t = max_t.max(t);
            has_close = true;
        }
    }

    if has_close && min_t < max_t {
        Some((min_t, max_t))
    } else {
        None
    }
}

/// Check if a bounding box is within [`PERP_THRESHOLD`] of an infinite divline.
///
/// Carved polygon vertices in a BSP subtree lie on or to one side of their
/// ancestor's divline. Due to floating-point drift, vertices that should be
/// exactly on the divline (perpendicular distance = 0) may be at distance ε.
/// A simple straddling test (corners on both sides) misses these near-zero
/// cases, incorrectly pruning frontier subtrees. Using [`PERP_THRESHOLD`] as
/// the acceptance band matches the tolerance used in
/// [`project_polygon_onto_divline`].
fn bbox_touches_divline(bbox: &[Vec2; 2], origin: Vec2, dir: Vec2) -> bool {
    let len = dir.length();
    if len < 1e-6 {
        return false;
    }
    let inv_len = 1.0 / len;
    let nx = -dir.y * inv_len;
    let ny = dir.x * inv_len;

    let corners = [
        Vec2::new(bbox[0].x, bbox[0].y),
        Vec2::new(bbox[1].x, bbox[0].y),
        Vec2::new(bbox[0].x, bbox[1].y),
        Vec2::new(bbox[1].x, bbox[1].y),
    ];

    let mut min_d = f32::MAX;
    let mut max_d = f32::MIN;
    for c in &corners {
        let d = (c.x - origin.x) * nx + (c.y - origin.y) * ny;
        min_d = min_d.min(d);
        max_d = max_d.max(d);
    }
    // The bbox touches the divline if the signed-distance range [min_d, max_d]
    // overlaps with [-PERP_THRESHOLD, PERP_THRESHOLD].
    max_d >= -PERP_THRESHOLD && min_d <= PERP_THRESHOLD
}

/// Compute bounding boxes from carved polygon extents for every node.
fn compute_carved_bboxes(
    node_id: u32,
    nodes: &[Node],
    carved: &[Vec<Vec2>],
) -> Vec<[[Vec2; 2]; 2]> {
    let mut bboxes = vec![[[Vec2::ZERO, Vec2::ZERO], [Vec2::ZERO, Vec2::ZERO]]; nodes.len()];
    compute_carved_bboxes_recursive(node_id, nodes, carved, &mut bboxes);
    bboxes
}

fn compute_carved_bboxes_recursive(
    child_id: u32,
    nodes: &[Node],
    carved: &[Vec<Vec2>],
    bboxes: &mut Vec<[[Vec2; 2]; 2]>,
) -> [Vec2; 2] {
    if is_subsector(child_id) {
        if child_id == u32::MAX {
            return [Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)];
        }
        let ss_id = subsector_index(child_id);
        let poly = &carved[ss_id];
        if poly.is_empty() {
            return [Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)];
        }
        let mut mn = Vec2::splat(f32::MAX);
        let mut mx = Vec2::splat(f32::MIN);
        for &v in poly {
            mn = mn.min(v);
            mx = mx.max(v);
        }
        return [mn, mx];
    }

    let Some(node) = nodes.get(child_id as usize) else {
        return [Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)];
    };

    let right_bb = compute_carved_bboxes_recursive(node.children[0], nodes, carved, bboxes);
    let left_bb = compute_carved_bboxes_recursive(node.children[1], nodes, carved, bboxes);

    bboxes[child_id as usize] = [right_bb, left_bb];

    [
        Vec2::new(
            right_bb[0].x.min(left_bb[0].x),
            right_bb[0].y.min(left_bb[0].y),
        ),
        Vec2::new(
            right_bb[1].x.max(left_bb[1].x),
            right_bb[1].y.max(left_bb[1].y),
        ),
    ]
}
