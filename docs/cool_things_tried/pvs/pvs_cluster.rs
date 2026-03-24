//! Cluster-based coarse PVS.
//!
//! Groups subsectors into spatial clusters using a self-limiting density-ratio
//! algorithm with an adaptive size cap derived from the map's wall-to-area
//! ratio and BSP portal length density. Then runs a 2D frustum-clip PVS flood
//! at the cluster level and expands the result back to a subsector-level
//! [`RenderPvs`].
//!
//! ## Adaptive dim_cap formula
//!
//! ```text
//! effective  = wall_ratio + portal_len_density * 20_000
//! dim_cap    = 22.2 * sqrt(effective), clamped to [384, 768]
//! ```
//!
//! - `wall_ratio`         = total polygon area / total one-sided linedef length
//! - `portal_len_density` = total BSP portal segment length / total polygon
//!   area

use crate::bsp3d::BSP3D;
use crate::map_defs::{LineDef, Node, Sector, Segment, SubSector};
use glam::Vec2;
use log::info;
use rayon::prelude::*;
use std::io::Write as _;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::mightsee::Mightsee;
use super::portal::Portals;
use super::traits::{MightSee, PvsData, PvsView2D, RenderPvs};
use super::{bits_to_vec, clip_segment, generate_separators};

// ============================================================================
// CONSTANTS
// ============================================================================

/// Density-ratio clustering threshold (Pipeline B, d = 1e-05).
const DENSITY_THRESHOLD: f32 = 1e-5;
/// Maximum bbox aspect ratio during cluster growth.
const MAX_ASPECT: f32 = 3.0;
/// Clusters smaller than this area are absorbed into neighbours.
const MIN_CLUSTER_AREA: f32 = 4096.0;
/// Absorption passes.
const ABSORB_PASSES: usize = 3;
/// Self-limiting growth margin: tighten axis limit to 1.3× current extent.
const SELF_LIMIT_MARGIN: f32 = 1.3;

/// Formula constants.
const K: f32 = 22.2;
const S: f32 = 20_000.0;
const DIM_MIN: f32 = 384.0;
const DIM_MAX: f32 = 768.0;

/// When `true`, cluster mightsee is derived from the subsector-level mightsee:
/// cluster A mightsee cluster B iff any subsector in A mightsee any subsector
/// in B. When `false`, every cluster can potentially see every other — the
/// frustum clip alone determines visibility (safe but slower).
const USE_CLUSTER_MIGHTSEE: bool = false;

// ============================================================================
// PvsCluster
// ============================================================================

/// Cluster-based coarse PVS.
///
/// Holds the subsector-level portal graph, mightsee bitsets, and the final
/// [`RenderPvs`] bitset computed by clustering + cluster-level frustum flood.
pub struct PvsCluster {
    portals: Portals,
    mightsee: Mightsee,
    visibility: RenderPvs,
}

impl PvsCluster {
    /// Build a [`PvsCluster`] for the given map data.
    pub fn build(
        subsectors: &[SubSector],
        segments: &[Segment],
        bsp: &BSP3D,
        _sectors: &[Sector],
        linedefs: &[LineDef],
        nodes: &[Node],
        start_node: u32,
    ) -> Self {
        let n = subsectors.len();
        info!("PvsCluster: building for {n} subsectors");

        // Phase 1: portals (adjacency graph).
        let portals = Portals::build(start_node, nodes, subsectors, segments, bsp);
        info!("PvsCluster: {} portals", portals.len());

        // Phase 2: carved polygons for areas / bboxes.
        let carved = &bsp.carved_polygons;

        let areas: Vec<f32> = carved.iter().map(|p| polygon_area(p)).collect();
        let bboxes: Vec<[f32; 4]> = carved.iter().map(|p| polygon_bbox(p)).collect();
        let total_area: f32 = areas.iter().sum();

        // Phase 3: compute adaptive dim_cap.
        let wall_ratio = compute_wall_ratio(linedefs, total_area);
        let portal_len_density = compute_portal_len_density(&portals, total_area);
        let dim_cap = compute_adaptive_dim(wall_ratio, portal_len_density);
        info!(
            "PvsCluster: wall_ratio={wall_ratio:.1}, pld={portal_len_density:.6}, dim_cap={dim_cap:.0}"
        );

        // Phase 4: density per subsector (cross-sector portal count / area).
        let density = compute_subsector_density(n, &portals, subsectors, &areas);

        // Phase 5: self-limiting density-ratio clustering.
        let (ss_to_cluster, num_clusters) =
            cluster_subsectors(n, &portals, &areas, &bboxes, &density, dim_cap);
        info!("PvsCluster: {num_clusters} clusters");

        // Phase 6: build cluster-level portal graph.
        let (_cluster_portals, cluster_portal_list) =
            build_cluster_portals(&portals, &ss_to_cluster, num_clusters);

        // Build cluster → subsector membership lists.
        let mut cluster_members: Vec<Vec<usize>> = vec![Vec::new(); num_clusters];
        for (ss, &c) in ss_to_cluster.iter().enumerate() {
            if c != u32::MAX {
                cluster_members[c as usize].push(ss);
            }
        }

        // Phase 7: cluster-level mightsee + frustum flood.
        // Build subsector-level mightsee first (needed for trait impl and
        // optionally for deriving cluster mightsee).
        let mightsee = Mightsee::build(&portals);
        let cluster_mightsee =
            ClusterMightsee::build(&mightsee, &ss_to_cluster, &cluster_members, num_clusters);
        let cluster_pvs = cluster_flood(num_clusters, &cluster_portal_list, &cluster_mightsee);

        // Phase 8: expand cluster PVS to subsector-level RenderPvs.
        let visibility = expand_to_subsector_pvs(n, &ss_to_cluster, num_clusters, &cluster_pvs);
        info!(
            "PvsCluster: {} visible pairs",
            visibility.count_visible_pairs()
        );

        Self {
            portals,
            mightsee,
            visibility,
        }
    }

    /// Clone the internal [`RenderPvs`].
    pub fn clone_render_pvs(&self) -> RenderPvs {
        self.visibility.clone()
    }

    /// Conservative mightsee set for `from`.
    pub fn get_mightsee_subsectors(&self, from: usize) -> Vec<usize> {
        let n = self.mightsee.subsector_count();
        if from >= n || self.portals.offsets().is_empty() {
            return Vec::new();
        }
        if self.visibility.data.is_empty() {
            return (0..n).collect();
        }
        let row_words = (n + 31) / 32;
        let mut bits = vec![0u32; row_words];
        {
            let pvs_row = self.visibility.row_slice(from);
            for (b, &r) in bits.iter_mut().zip(pvs_row.iter()) {
                *b |= r;
            }
        }
        let portal_offsets = self.portals.offsets();
        let portal_ids = self.portals.ids();
        let src_start = portal_offsets[from] as usize;
        let src_end = portal_offsets[from + 1] as usize;
        for &pi in &portal_ids[src_start..src_end] {
            let adj = self.portals.get(pi as usize).other(from);
            let pvs_row = self.visibility.row_slice(adj);
            for (b, &r) in bits.iter_mut().zip(pvs_row.iter()) {
                *b |= r;
            }
        }
        bits_to_vec(&bits, n)
    }

    /// Write the PVS to the platform cache directory.
    pub fn save_to_cache(
        &self,
        map_name: &str,
        map_hash: u64,
    ) -> Result<(), super::traits::PvsFileError> {
        use super::traits::PvsFile;
        self.visibility.save_to_cache(map_name, map_hash)
    }
}

// ============================================================================
// Trait impls
// ============================================================================

impl PvsData for PvsCluster {
    fn is_visible(&self, from: usize, to: usize) -> bool {
        self.visibility.is_visible(from, to)
    }

    fn get_visible_subsectors(&self, from: usize) -> Vec<usize> {
        self.visibility.get_visible_subsectors(from)
    }

    fn subsector_count(&self) -> usize {
        self.visibility.subsector_count()
    }

    fn count_visible_pairs(&self) -> u64 {
        self.visibility.count_visible_pairs()
    }
}

impl PvsView2D for PvsCluster {
    fn portals_2d(&self) -> &Portals {
        &self.portals
    }
}

impl MightSee for PvsCluster {
    fn subsector_count(&self) -> usize {
        self.mightsee.subsector_count()
    }

    fn source_bits(&self, source: usize) -> &[u32] {
        self.mightsee.source_bits(source)
    }

    fn portal_dir_bits(&self, pi: usize, side: usize) -> &[u32] {
        self.mightsee.portal_dir_bits(pi, side)
    }
}

impl From<PvsCluster> for RenderPvs {
    fn from(pvs: PvsCluster) -> RenderPvs {
        pvs.visibility
    }
}

// ============================================================================
// Geometry helpers
// ============================================================================

fn polygon_area(poly: &[Vec2]) -> f32 {
    if poly.len() < 3 {
        return 0.0;
    }
    let mut a = 0.0f32;
    let n = poly.len();
    for i in 0..n {
        let j = (i + 1) % n;
        a += poly[i].x * poly[j].y - poly[j].x * poly[i].y;
    }
    a.abs() * 0.5
}

fn polygon_bbox(poly: &[Vec2]) -> [f32; 4] {
    if poly.is_empty() {
        return [0.0; 4];
    }
    let mut xmin = f32::MAX;
    let mut ymin = f32::MAX;
    let mut xmax = f32::MIN;
    let mut ymax = f32::MIN;
    for v in poly {
        xmin = xmin.min(v.x);
        ymin = ymin.min(v.y);
        xmax = xmax.max(v.x);
        ymax = ymax.max(v.y);
    }
    [xmin, ymin, xmax, ymax]
}

// ============================================================================
// Map metrics
// ============================================================================

/// `wall_ratio = total_polygon_area / total_one_sided_linedef_length`.
fn compute_wall_ratio(linedefs: &[LineDef], total_area: f32) -> f32 {
    let mut one_sided_len = 0.0f32;
    for ld in linedefs {
        if ld.back_sidedef.is_none() {
            let v1 = ld.v1.pos;
            let v2 = ld.v2.pos;
            one_sided_len += (v2 - v1).length();
        }
    }
    if one_sided_len > 0.0 {
        total_area / one_sided_len
    } else {
        200.0
    }
}

/// `portal_len_density = total_portal_segment_length / total_polygon_area`.
fn compute_portal_len_density(portals: &Portals, total_area: f32) -> f32 {
    let mut total_len = 0.0f32;
    for p in portals.iter() {
        total_len += (p.v2 - p.v1).length();
    }
    if total_area > 0.0 {
        total_len / total_area
    } else {
        0.0
    }
}

/// `dim_cap = K * sqrt(wall_ratio + portal_len_density * S)`, clamped.
fn compute_adaptive_dim(wall_ratio: f32, portal_len_density: f32) -> f32 {
    let effective = wall_ratio + portal_len_density * S;
    let raw = K * effective.sqrt();
    raw.clamp(DIM_MIN, DIM_MAX)
}

/// Per-subsector density: cross-sector portal edge count / area.
fn compute_subsector_density(
    n: usize,
    portals: &Portals,
    subsectors: &[SubSector],
    areas: &[f32],
) -> Vec<f32> {
    let mut edge_count = vec![0u32; n];
    for p in portals.iter() {
        let sec_a = subsectors[p.subsector_a].sector.num;
        let sec_b = subsectors[p.subsector_b].sector.num;
        if sec_a != sec_b {
            edge_count[p.subsector_a] += 1;
            edge_count[p.subsector_b] += 1;
        }
    }
    (0..n)
        .map(|i| {
            if areas[i] >= 1.0 {
                edge_count[i] as f32 / areas[i]
            } else {
                0.0
            }
        })
        .collect()
}

// ============================================================================
// Self-limiting density-ratio clustering
// ============================================================================

/// Returns `(ss_to_cluster, num_clusters)`.
///
/// Seeds sorted by density descending. BFS growth with self-limiting bbox,
/// density-ratio gating, and aspect-ratio checks. Post-pass absorbs tiny
/// clusters into neighbours.
fn cluster_subsectors(
    n: usize,
    portals: &Portals,
    areas: &[f32],
    bboxes: &[[f32; 4]],
    density: &[f32],
    dim_cap: f32,
) -> (Vec<u32>, usize) {
    // Build adjacency list from portal graph.
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for p in portals.iter() {
        let a = p.subsector_a;
        let b = p.subsector_b;
        if !adj[a].contains(&b) {
            adj[a].push(b);
        }
        if !adj[b].contains(&a) {
            adj[b].push(a);
        }
    }

    // Seed order: density descending, skip empty polygons.
    let mut sorted_ss: Vec<usize> = (0..n).filter(|&i| areas[i] > 0.0).collect();
    sorted_ss.sort_by(|&a, &b| density[b].partial_cmp(&density[a]).unwrap());

    let mut assigned = vec![u32::MAX; n];
    let mut clusters: Vec<Vec<usize>> = Vec::new();
    let mut cluster_bboxes: Vec<[f32; 4]> = Vec::new();
    let mut frontier: Vec<usize> = Vec::new();
    let mut in_frontier = vec![false; n];

    for &seed in &sorted_ss {
        if assigned[seed] != u32::MAX {
            continue;
        }
        let cid = clusters.len() as u32;
        assigned[seed] = cid;
        let mut cluster = vec![seed];

        let sb = bboxes[seed];
        let mut c_xmin = sb[0];
        let mut c_ymin = sb[1];
        let mut c_xmax = sb[2];
        let mut c_ymax = sb[3];

        // Dynamic per-axis limits — start at dim_cap.
        let mut x_limit = dim_cap;
        let mut y_limit = dim_cap;

        // Priority frontier sorted by density descending.
        frontier.clear();
        in_frontier.fill(false);
        for &nb in &adj[seed] {
            if assigned[nb] == u32::MAX && !in_frontier[nb] {
                frontier.push(nb);
                in_frontier[nb] = true;
            }
        }
        frontier.sort_by(|&a, &b| density[b].partial_cmp(&density[a]).unwrap());

        let mut fi = 0;
        while fi < frontier.len() {
            let candidate = frontier[fi];
            fi += 1;
            if assigned[candidate] != u32::MAX {
                continue;
            }
            if areas[candidate] <= 0.0 {
                continue;
            }

            let cb = bboxes[candidate];
            // Tentative bbox.
            let t_xmin = c_xmin.min(cb[0]);
            let t_ymin = c_ymin.min(cb[1]);
            let t_xmax = c_xmax.max(cb[2]);
            let t_ymax = c_ymax.max(cb[3]);
            let t_w = t_xmax - t_xmin;
            let t_h = t_ymax - t_ymin;

            // Check axis limits.
            if t_w > x_limit || t_h > y_limit {
                continue;
            }

            // Check aspect ratio.
            let short = t_w.min(t_h);
            let long = t_w.max(t_h);
            if short > 1.0 && long / short > MAX_ASPECT {
                continue;
            }

            // Density check: stop growing when cluster density drops below
            // threshold and cluster has more than 3 members.
            if cluster.len() > 3 {
                let mut c_edges = 0.0f32;
                let mut c_area = 0.0f32;
                for &ss in &cluster {
                    c_edges += density[ss] * areas[ss]; // recover edge count
                    c_area += areas[ss];
                }
                c_edges += density[candidate] * areas[candidate];
                c_area += areas[candidate];
                if c_area > 1.0 && c_edges / c_area < DENSITY_THRESHOLD {
                    continue;
                }
            }

            // Accept candidate.
            assigned[candidate] = cid;
            cluster.push(candidate);
            c_xmin = t_xmin;
            c_ymin = t_ymin;
            c_xmax = t_xmax;
            c_ymax = t_ymax;

            // Self-limiting: tighten axis limits.
            let cur_w = c_xmax - c_xmin;
            let cur_h = c_ymax - c_ymin;
            if cur_w >= DIM_MIN {
                x_limit = x_limit
                    .min(cur_w * SELF_LIMIT_MARGIN)
                    .max(DIM_MIN)
                    .min(dim_cap);
            }
            if cur_h >= DIM_MIN {
                y_limit = y_limit
                    .min(cur_h * SELF_LIMIT_MARGIN)
                    .max(DIM_MIN)
                    .min(dim_cap);
            }

            // Add new neighbours to frontier.
            for &nb in &adj[candidate] {
                if assigned[nb] == u32::MAX && !in_frontier[nb] {
                    frontier.push(nb);
                    in_frontier[nb] = true;
                }
            }
            // Re-sort remaining frontier by density.
            let remaining = &mut frontier[fi..];
            remaining.sort_by(|&a, &b| density[b].partial_cmp(&density[a]).unwrap());
        }

        cluster_bboxes.push([c_xmin, c_ymin, c_xmax, c_ymax]);
        clusters.push(cluster);
    }

    // Absorb tiny clusters.
    absorb_tiny_clusters(&mut clusters, &mut assigned, areas, &adj);

    // Compact: remove empty clusters and re-assign.
    let mut remap = vec![u32::MAX; clusters.len()];
    let mut num_clusters = 0u32;
    for (old, c) in clusters.iter().enumerate() {
        if !c.is_empty() {
            remap[old] = num_clusters;
            num_clusters += 1;
        }
    }
    for a in assigned.iter_mut() {
        if *a != u32::MAX {
            *a = remap[*a as usize];
        }
    }

    // Split disconnected components within each cluster.
    let (assigned, num_clusters) =
        split_disconnected(&clusters, &remap, &adj, n, num_clusters as usize);

    (assigned, num_clusters)
}

/// Merge clusters with area < MIN_CLUSTER_AREA into their most-connected
/// neighbour.
fn absorb_tiny_clusters(
    clusters: &mut Vec<Vec<usize>>,
    assigned: &mut Vec<u32>,
    areas: &[f32],
    adj: &[Vec<usize>],
) {
    let nc = clusters.len();
    let mut cadj: Vec<Vec<(usize, u32)>> = vec![Vec::new(); nc];
    for _pass in 0..ABSORB_PASSES {
        // Build cluster adjacency with shared edge counts.
        for v in &mut cadj {
            v.clear();
        }
        for (ss_a, nbs) in adj.iter().enumerate() {
            if assigned[ss_a] == u32::MAX {
                continue;
            }
            let ca = assigned[ss_a] as usize;
            for &ss_b in nbs {
                if assigned[ss_b] == u32::MAX {
                    continue;
                }
                let cb = assigned[ss_b] as usize;
                if ca != cb {
                    if let Some(entry) = cadj[ca].iter_mut().find(|e| e.0 == cb) {
                        entry.1 += 1;
                    } else {
                        cadj[ca].push((cb, 1));
                    }
                }
            }
        }

        // Sort clusters by area ascending, absorb small ones.
        let mut c_by_area: Vec<(usize, f32)> = clusters
            .iter()
            .enumerate()
            .filter(|(_, c)| !c.is_empty())
            .map(|(cid, c)| (cid, c.iter().map(|&ss| areas[ss]).sum::<f32>()))
            .collect();
        c_by_area.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

        let mut absorbed = 0;
        for (cid, area) in c_by_area {
            if clusters[cid].is_empty() || area >= MIN_CLUSTER_AREA {
                break;
            }
            // Find best neighbour (most shared edges).
            let best = cadj[cid]
                .iter()
                .filter(|(nb, _)| !clusters[*nb].is_empty() && *nb != cid)
                .max_by_key(|(_, count)| *count);
            if let Some(&(best_nb, _)) = best {
                let members: Vec<usize> = clusters[cid].drain(..).collect();
                for ss in members {
                    assigned[ss] = best_nb as u32;
                    clusters[best_nb].push(ss);
                }
                absorbed += 1;
            }
        }
        if absorbed == 0 {
            break;
        }
    }
}

/// Split disconnected components within each cluster into separate clusters.
fn split_disconnected(
    clusters: &[Vec<usize>],
    remap: &[u32],
    adj: &[Vec<usize>],
    n: usize,
    _old_count: usize,
) -> (Vec<u32>, usize) {
    let mut assigned = vec![u32::MAX; n];
    let mut new_cid = 0u32;

    for (old_cid, members) in clusters.iter().enumerate() {
        if remap[old_cid] == u32::MAX || members.is_empty() {
            continue;
        }
        if members.len() == 1 {
            assigned[members[0]] = new_cid;
            new_cid += 1;
            continue;
        }

        // BFS to find connected components within this cluster.
        let member_set: std::collections::HashSet<usize> = members.iter().copied().collect();
        let mut visited = std::collections::HashSet::new();

        for &start in members {
            if visited.contains(&start) {
                continue;
            }
            let mut queue = std::collections::VecDeque::new();
            queue.push_back(start);
            visited.insert(start);
            assigned[start] = new_cid;
            while let Some(cur) = queue.pop_front() {
                for &nb in &adj[cur] {
                    if member_set.contains(&nb) && !visited.contains(&nb) {
                        visited.insert(nb);
                        assigned[nb] = new_cid;
                        queue.push_back(nb);
                    }
                }
            }
            new_cid += 1;
        }
    }

    (assigned, new_cid as usize)
}

// ============================================================================
// Cluster-level portal graph
// ============================================================================

/// A portal between two clusters.
///
/// After collection, collinear adjacent subsector portals between the same
/// cluster pair are merged into wider segments to reduce flood work.
#[derive(Clone)]
struct ClusterPortal {
    /// Cluster on one side of this portal.
    cluster_a: usize,
    /// Cluster on the other side.
    cluster_b: usize,
    /// First endpoint of the (possibly merged) portal segment.
    v1: Vec2,
    /// Second endpoint of the (possibly merged) portal segment.
    v2: Vec2,
}

/// Perpendicular distance threshold for considering two portals collinear.
const MERGE_PERP_THRESHOLD: f32 = 2.0;
/// Minimum gap (in parametric units along the line) to keep portals separate.
const MERGE_GAP_THRESHOLD: f32 = 1.0;

/// Build cluster-level portal graph. Cross-cluster subsector portals are
/// collected, then collinear/adjacent ones between the same pair are merged
/// into wider segments.
///
/// Returns:
/// - adjacency list `[cluster] -> Vec<cluster_portal_index>`
/// - the merged cluster portal segments
fn build_cluster_portals(
    portals: &Portals,
    ss_to_cluster: &[u32],
    num_clusters: usize,
) -> (Vec<Vec<usize>>, Vec<ClusterPortal>) {
    use std::collections::HashMap;

    // Group subsector portals by (min_cluster, max_cluster) pair.
    let mut by_pair: HashMap<(usize, usize), Vec<(Vec2, Vec2)>> = HashMap::new();

    for p in portals.iter() {
        let ca = ss_to_cluster[p.subsector_a];
        let cb = ss_to_cluster[p.subsector_b];
        if ca == u32::MAX || cb == u32::MAX || ca == cb {
            continue;
        }
        let ca = ca as usize;
        let cb = cb as usize;
        let key = (ca.min(cb), ca.max(cb));
        by_pair.entry(key).or_default().push((p.v1, p.v2));
    }

    let mut portal_list: Vec<ClusterPortal> = Vec::new();
    let mut cluster_portals: Vec<Vec<usize>> = vec![Vec::new(); num_clusters];

    for ((ca, cb), segments) in by_pair {
        let merged = merge_collinear_segments(&segments);
        for (v1, v2) in merged {
            let idx = portal_list.len();
            portal_list.push(ClusterPortal {
                cluster_a: ca,
                cluster_b: cb,
                v1,
                v2,
            });
            cluster_portals[ca].push(idx);
            cluster_portals[cb].push(idx);
        }
    }

    (cluster_portals, portal_list)
}

/// Merge collinear, adjacent or overlapping segments into wider ones.
///
/// Segments are grouped by direction: two segments are candidates if they
/// share a collinear axis (perpendicular distance < threshold). Within each
/// group, overlapping/adjacent intervals are merged.
fn merge_collinear_segments(segments: &[(Vec2, Vec2)]) -> Vec<(Vec2, Vec2)> {
    if segments.len() <= 1 {
        return segments.to_vec();
    }

    let mut result: Vec<(Vec2, Vec2)> = Vec::new();
    let mut used = vec![false; segments.len()];
    let mut intervals: Vec<(f32, f32)> = Vec::new();
    let mut merged_intervals: Vec<(f32, f32)> = Vec::new();

    for i in 0..segments.len() {
        if used[i] {
            continue;
        }

        let (a1, a2) = segments[i];
        let dir = a2 - a1;
        let len = dir.length();
        if len < 1e-6 {
            result.push((a1, a2));
            used[i] = true;
            continue;
        }

        let axis = dir / len;
        let normal = Vec2::new(-axis.y, axis.x);
        let origin = a1;

        // Project this segment onto its own axis.
        // t1 = 0, t2 = len.
        intervals.clear();
        intervals.push((0.0, len));
        used[i] = true;

        // Find all other segments collinear with this one.
        for j in (i + 1)..segments.len() {
            if used[j] {
                continue;
            }
            let (b1, b2) = segments[j];

            // Check perpendicular distance of both endpoints.
            let d1 = (b1 - origin).dot(normal).abs();
            let d2 = (b2 - origin).dot(normal).abs();
            if d1 > MERGE_PERP_THRESHOLD || d2 > MERGE_PERP_THRESHOLD {
                continue;
            }

            // Project onto the axis.
            let t1 = (b1 - origin).dot(axis);
            let t2 = (b2 - origin).dot(axis);
            let (tmin, tmax) = if t1 < t2 { (t1, t2) } else { (t2, t1) };
            intervals.push((tmin, tmax));
            used[j] = true;
        }

        // Merge overlapping/adjacent intervals.
        intervals.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        merged_intervals.clear();
        let (mut cur_min, mut cur_max) = intervals[0];
        for &(lo, hi) in &intervals[1..] {
            if lo <= cur_max + MERGE_GAP_THRESHOLD {
                cur_max = cur_max.max(hi);
            } else {
                merged_intervals.push((cur_min, cur_max));
                cur_min = lo;
                cur_max = hi;
            }
        }
        merged_intervals.push((cur_min, cur_max));

        // Convert intervals back to world-space segments.
        for &(tmin, tmax) in &merged_intervals {
            result.push((origin + axis * tmin, origin + axis * tmax));
        }
    }

    result
}

// ============================================================================
// Cluster-level mightsee (derived from subsector mightsee)
// ============================================================================

/// Cluster-level mightsee bitsets.
///
/// When [`USE_CLUSTER_MIGHTSEE`] is `true`, derived from the subsector-level
/// [`Mightsee`]: cluster A mightsee cluster B iff any subsector in A mightsee
/// any subsector in B. When `false`, all bits are set (unconstrained).
struct ClusterMightsee {
    /// Total cluster count.
    num_clusters: usize,
    /// Row-major bitset: `data[c * row_words .. (c+1) * row_words]`.
    data: Vec<u32>,
}

impl ClusterMightsee {
    /// Build from subsector-level mightsee by OR-ing subsector rows.
    fn build(
        ss_mightsee: &Mightsee,
        ss_to_cluster: &[u32],
        cluster_members: &[Vec<usize>],
        num_clusters: usize,
    ) -> Self {
        let row_words = (num_clusters + 31) / 32;
        let mut data = vec![0u32; num_clusters * row_words];

        if !USE_CLUSTER_MIGHTSEE {
            // All-visible: set every bit.
            for c in 0..num_clusters {
                let row = &mut data[c * row_words..(c + 1) * row_words];
                for w in row.iter_mut() {
                    *w = u32::MAX;
                }
                // Mask trailing bits in last word.
                let trailing = num_clusters % 32;
                if trailing != 0 {
                    row[row_words - 1] = (1u32 << trailing) - 1;
                }
            }
            return Self {
                num_clusters,
                data,
            };
        }

        // For each cluster, OR together the subsector mightsee rows of all
        // its members, then project the result into cluster space.
        let ss_count = ss_mightsee.subsector_count();
        let ss_row_words = (ss_count + 31) / 32;

        for (ca, members) in cluster_members.iter().enumerate() {
            // Union of all subsector mightsee rows for this cluster.
            let mut ss_union = vec![0u32; ss_row_words];
            for &ss in members {
                let bits = ss_mightsee.source_bits(ss);
                for (u, &b) in ss_union.iter_mut().zip(bits.iter()) {
                    *u |= b;
                }
            }

            // Project: for each set subsector bit, set its cluster bit.
            let cl_row = &mut data[ca * row_words..(ca + 1) * row_words];
            for wi in 0..ss_row_words {
                let mut word = ss_union[wi];
                let base = wi * 32;
                while word != 0 {
                    let bit = word.trailing_zeros() as usize;
                    let ss = base + bit;
                    if ss < ss_count {
                        let cb = ss_to_cluster[ss];
                        if cb != u32::MAX {
                            let cb = cb as usize;
                            cl_row[cb / 32] |= 1u32 << (cb % 32);
                        }
                    }
                    word &= word - 1;
                }
            }
        }

        Self {
            num_clusters,
            data,
        }
    }

    /// Number of u32 words per row.
    fn row_words(&self) -> usize {
        (self.num_clusters + 31) / 32
    }

    /// Bitset row for cluster `source`.
    fn source_bits(&self, source: usize) -> &[u32] {
        let rw = self.row_words();
        &self.data[source * rw..(source + 1) * rw]
    }
}

// ============================================================================
// Cluster-level frustum-clip flood
// ============================================================================

/// Run a parallel frustum-clip PVS flood at cluster granularity.
///
/// Each source cluster's flood tracks the widest source-portal (by length²)
/// that has entered every other cluster. Re-entry is only allowed when a
/// strictly wider frustum is available, preventing the combinatorial explosion
/// caused by many subsector portals between the same cluster pair.
///
/// Returns a flat row-major bitset of size `num_clusters × row_words`.
fn cluster_flood(
    num_clusters: usize,
    portal_list: &[ClusterPortal],
    mightsee: &ClusterMightsee,
) -> Vec<u32> {
    let row_words = (num_clusters + 31) / 32;

    // Build CSR for cluster portals.
    let mut csr_adj: Vec<Vec<usize>> = vec![Vec::new(); num_clusters];
    for (pi, p) in portal_list.iter().enumerate() {
        csr_adj[p.cluster_a].push(pi);
        csr_adj[p.cluster_b].push(pi);
    }

    let progress = AtomicUsize::new(0);

    let rows: Vec<Vec<u32>> = (0..num_clusters)
        .into_par_iter()
        .map(|source| {
            let done = progress.fetch_add(1, Ordering::Relaxed) + 1;
            if done % 16 == 0 || done == num_clusters {
                eprint!(
                    "\r  PvsCluster flood: {done}/{num_clusters} ({:.0}%)   ",
                    done as f32 / num_clusters as f32 * 100.0
                );
                let _ = std::io::stderr().flush();
            }

            let mut pvs_row = vec![0u32; row_words];
            pvs_row[source / 32] |= 1u32 << (source % 32);

            let mut on_path = vec![0u32; row_words];
            on_path[source / 32] |= 1u32 << (source % 32);

            // Widest source-portal length² that has entered each cluster.
            // Only re-enter if strictly wider.
            let mut best_src_len_sq = vec![0.0f32; num_clusters];
            best_src_len_sq[source] = f32::MAX;

            let src_mightsee = mightsee.source_bits(source);

            for &pi in &csr_adj[source] {
                let p = &portal_list[pi];
                let far = if p.cluster_a == source {
                    p.cluster_b
                } else {
                    p.cluster_a
                };
                pvs_row[far / 32] |= 1u32 << (far % 32);
                let seg = (p.v1, p.v2);
                let seg_len_sq = (seg.1 - seg.0).length_squared();
                best_src_len_sq[far] = best_src_len_sq[far].max(seg_len_sq);

                on_path[far / 32] |= 1u32 << (far % 32);
                cluster_clip_flood(
                    seg,
                    seg,
                    far,
                    pi,
                    portal_list,
                    &csr_adj,
                    src_mightsee,
                    mightsee,
                    &mut pvs_row,
                    &mut on_path,
                    src_mightsee,
                    &mut best_src_len_sq,
                );
                on_path[far / 32] &= !(1u32 << (far % 32));
            }

            pvs_row
        })
        .collect();
    eprintln!();

    // Flatten + symmetry pass.
    let mut flat: Vec<u32> = rows.into_iter().flatten().collect();
    for a in 0..num_clusters {
        for b in (a + 1)..num_clusters {
            let ab_w = a * row_words + b / 32;
            let ab_b = 1u32 << (b % 32);
            let ba_w = b * row_words + a / 32;
            let ba_b = 1u32 << (a % 32);
            let ab = flat[ab_w] & ab_b != 0;
            let ba = flat[ba_w] & ba_b != 0;
            if ab || ba {
                flat[ab_w] |= ab_b;
                flat[ba_w] |= ba_b;
            }
        }
    }

    flat
}

/// Recursive frustum-clip flood at cluster level.
///
/// `best_src_len_sq` tracks the widest source-portal length² that has entered
/// each cluster across the entire flood for this source. A cluster is only
/// re-entered if the new clipped source portal is strictly wider than the
/// previous best — this prevents combinatorial explosion from many subsector
/// portals crossing the same cluster boundary while still allowing genuinely
/// wider frustums to reveal new visibility.
fn cluster_clip_flood(
    source_portal: (Vec2, Vec2),
    pass_portal: (Vec2, Vec2),
    current: usize,
    pass_portal_idx: usize,
    portal_list: &[ClusterPortal],
    csr_adj: &[Vec<usize>],
    global_mightsee: &[u32],
    mightsee: &ClusterMightsee,
    pvs_row: &mut Vec<u32>,
    on_path: &mut Vec<u32>,
    parent_might: &[u32],
    best_src_len_sq: &mut Vec<f32>,
) {
    // Progressive mightsee narrowing: AND parent with directional mightsee
    // of the portal we just traversed. For cluster-level we use the source
    // bits of the current cluster as the directional bound.
    let pass_pm = mightsee.source_bits(current);
    let mut might: Vec<u32> = parent_might
        .iter()
        .zip(pass_pm.iter())
        .map(|(&pm, &dm)| pm & dm)
        .collect();
    for (m, &g) in might.iter_mut().zip(global_mightsee.iter()) {
        *m &= g;
    }

    // Early termination: everything reachable is already marked visible.
    if might.iter().zip(pvs_row.iter()).all(|(&m, &p)| m & !p == 0) {
        return;
    }

    let separators_fwd = generate_separators(source_portal, pass_portal);

    for &pt_idx in &csr_adj[current] {
        if pt_idx == pass_portal_idx {
            continue;
        }
        let pt = &portal_list[pt_idx];
        let far = if pt.cluster_a == current {
            pt.cluster_b
        } else {
            pt.cluster_a
        };

        // Mightsee gate.
        if might[far / 32] & (1u32 << (far % 32)) == 0 {
            continue;
        }
        // Cycle prevention on current DFS stack.
        if on_path[far / 32] & (1u32 << (far % 32)) != 0 {
            continue;
        }

        // Clip target portal against forward separators.
        let target_seg = (pt.v1, pt.v2);
        let clipped_target = match clip_segment(target_seg, separators_fwd.as_slice()) {
            Some(s) => s,
            None => continue,
        };

        // Backward separators from (clipped_target, pass_portal).
        let separators_bwd = generate_separators(clipped_target, pass_portal);

        // Clip source portal against backward separators.
        let clipped_source = match clip_segment(source_portal, separators_bwd.as_slice()) {
            Some(s) => s,
            None => continue,
        };

        // Frustum-width gate: only re-enter `far` if this clipped source
        // portal is strictly wider than any previous entry. This is the key
        // dedup that prevents O(N^depth) blowup when many subsector portals
        // span the same cluster boundary.
        let src_len_sq = (clipped_source.1 - clipped_source.0).length_squared();
        if src_len_sq <= best_src_len_sq[far] {
            // Already explored with an equal or wider frustum — skip.
            pvs_row[far / 32] |= 1u32 << (far % 32);
            continue;
        }
        best_src_len_sq[far] = src_len_sq;

        // Mark visible + recurse.
        pvs_row[far / 32] |= 1u32 << (far % 32);
        on_path[far / 32] |= 1u32 << (far % 32);
        cluster_clip_flood(
            clipped_source,
            clipped_target,
            far,
            pt_idx,
            portal_list,
            csr_adj,
            global_mightsee,
            mightsee,
            pvs_row,
            on_path,
            &might,
            best_src_len_sq,
        );
        on_path[far / 32] &= !(1u32 << (far % 32));
    }
}

// ============================================================================
// Expand cluster PVS → subsector RenderPvs
// ============================================================================

/// Expand cluster-level visibility to subsector-level RenderPvs.
///
/// If cluster A can see cluster B, every subsector in A can see every
/// subsector in B.
fn expand_to_subsector_pvs(
    n: usize,
    ss_to_cluster: &[u32],
    num_clusters: usize,
    cluster_pvs: &[u32],
) -> RenderPvs {
    let row_words_ss = (n + 31) / 32;
    let row_words_cl = (num_clusters + 31) / 32;
    let mut data = vec![0u32; n * row_words_ss];

    // Build cluster → subsector members.
    let mut cluster_members: Vec<Vec<usize>> = vec![Vec::new(); num_clusters];
    for (ss, &c) in ss_to_cluster.iter().enumerate() {
        if c != u32::MAX {
            cluster_members[c as usize].push(ss);
        }
    }

    // For each subsector, look up its cluster's visible clusters, and set all
    // member subsectors of those clusters as visible.
    for from_ss in 0..n {
        let from_c = ss_to_cluster[from_ss];
        if from_c == u32::MAX {
            // Unassigned subsector: mark only self as visible.
            data[from_ss * row_words_ss + from_ss / 32] |= 1u32 << (from_ss % 32);
            continue;
        }
        let from_c = from_c as usize;
        let cl_row = &cluster_pvs[from_c * row_words_cl..(from_c + 1) * row_words_cl];

        let ss_row = &mut data[from_ss * row_words_ss..(from_ss + 1) * row_words_ss];

        // Walk the cluster PVS row bitset.
        for wi in 0..row_words_cl {
            let mut word = cl_row[wi];
            let base = wi * 32;
            while word != 0 {
                let bit = word.trailing_zeros() as usize;
                let to_c = base + bit;
                if to_c < num_clusters {
                    // Set all subsectors of to_c as visible from from_ss.
                    for &to_ss in &cluster_members[to_c] {
                        ss_row[to_ss / 32] |= 1u32 << (to_ss % 32);
                    }
                }
                word &= word - 1;
            }
        }
    }

    RenderPvs {
        subsector_count: n,
        data,
    }
}
