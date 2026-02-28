//! Coarse Portal Visibility System.
//!
//! Partitions subsectors into coarse regions, identifies portal boundaries,
//! and runs source→pass→target anti-penumbra frustum clipping to determine
//! region-level visibility. The result is expanded to a subsector-level PVS.
//!
//! Pipeline:
//! 1. BSP Frontier Adjacency + Collinear Touch Filter
//! 2. Edge Weight Assignment
//! 3. Flood-Fill Grouping (threshold T)
//! 4. BSP-Divline Subdivision (max count, min area)
//! 5. Spatial Connectivity Validation
//! 6. Solid Wall Inventory
//! 7. Portal Identification (linedef-backed + fallbacks)
//! 8. Visibility Determination (BFS with 2D+height frustum)

use crate::bsp3d::{BSP3D, carve_subsector_polygons_2d, is_sector_mover};
use crate::map_defs::{LineDef, Node, Sector, Segment, SubSector};
use glam::Vec2;
use log::info;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::sync::atomic::Ordering;

// ============================================================================
// COMPACT PVS BITSET
// ============================================================================

#[derive(Default, Clone)]
pub(crate) struct CompactPVS {
    pub(crate) subsector_count: usize,
    pub(crate) data: Vec<u32>,
}

impl CompactPVS {
    pub fn new(subsector_count: usize) -> Self {
        let row_words = (subsector_count + 31) / 32;
        let words_needed = subsector_count * row_words;
        Self {
            subsector_count,
            data: vec![0; words_needed],
        }
    }

    pub fn set_visible_atomic(&self, from: usize, to: usize) {
        let bit_index = from * self.subsector_count + to;
        let word_index = bit_index / 32;
        let bit_offset = bit_index % 32;
        let mask = 1u32 << bit_offset;
        let ptr = self.data.as_ptr() as *mut std::sync::atomic::AtomicU32;
        unsafe {
            let atomic_word = &*ptr.add(word_index);
            atomic_word.fetch_or(mask, Ordering::Relaxed);
        }
    }

    pub fn is_visible(&self, from: usize, to: usize) -> bool {
        if self.data.is_empty() {
            return true;
        }
        let bit_index = from * self.subsector_count + to;
        let word_index = bit_index / 32;
        let bit_offset = bit_index % 32;
        (self.data[word_index] & (1u32 << bit_offset)) != 0
    }

    pub fn get_visible_subsectors(&self, from: usize) -> Vec<usize> {
        if self.data.is_empty() {
            return (0..self.subsector_count).collect();
        }
        let w = (self.subsector_count + 31) / 32;
        let start = from * w;
        let mut visible = Vec::with_capacity(self.subsector_count / 4);
        for i in 0..w {
            let mut word = self.data[start + i];
            let base = i * 32;
            while word != 0 {
                let bit = word.trailing_zeros() as usize;
                let ss = base + bit;
                if ss < self.subsector_count {
                    visible.push(ss);
                }
                word &= word - 1;
            }
        }
        visible
    }

    pub fn count_visible_pairs(&self) -> u64 {
        self.data.iter().map(|w| w.count_ones() as u64).sum()
    }

    pub fn memory_usage(&self) -> usize {
        std::mem::size_of::<Self>() + self.data.len() * std::mem::size_of::<u32>()
    }
}

// ============================================================================
// CONFIGURATION
// ============================================================================

/// Tolerance for collinear polygon-edge touch detection (map units).
const TOUCH_TOLERANCE: f32 = 1.5;

/// Minimum 1D overlap length for polygon-edge touch (map units).
const MIN_OVERLAP: f32 = 0.5;

/// Flood-fill edge weight threshold. Edges with weight >= T are not crossed.
const FLOOD_THRESHOLD: f32 = 80.0;

/// Maximum subsector count per region before subdivision.
const MAX_REGION_SIZE: usize = 16;

/// Minimum bounding box area (sq map units) to allow subdivision.
const MIN_SUBDIVISION_AREA: f32 = 360_000.0;

/// Maximum BFS traversal depth for visibility determination.
const MAX_BFS_DEPTH: u32 = 40;

/// Maximum times a region may be visited during BFS (allows alternative paths).
const MAX_REGION_VISITS: u32 = 4;

/// Collinearity tolerance for portal segment matching (map units).
const SEGMENT_OVERLAP_TOL: f32 = 1.0;

/// BSP node child ID bit indicating a subsector leaf.
const IS_SS_MASK: u32 = 0x8000_0000;

// -- Edge weight constants (Stage 2) --

/// Weight added when linedef has a special or either sector is a mover.
pub const WEIGHT_SPECIAL: f32 = 50.0;
/// Extra weight when sector is a confirmed mover (on top of WEIGHT_SPECIAL).
pub const WEIGHT_MOVER: f32 = 50.0;
/// Weight when the opening between sectors is sealed (height <= 0).
pub const WEIGHT_SEALED: f32 = 80.0;
/// Weight when opening height < OPENING_NARROW.
pub const WEIGHT_NARROW: f32 = 40.0;
/// Weight when opening height < OPENING_MEDIUM.
pub const WEIGHT_MEDIUM: f32 = 20.0;
/// Opening height threshold for "narrow" penalty.
pub const OPENING_NARROW: f32 = 64.0;
/// Opening height threshold for "medium" penalty.
pub const OPENING_MEDIUM: f32 = 128.0;
/// Weight for large floor-ratio difference (> FLOOR_RATIO_HIGH).
pub const WEIGHT_FLOOR_RATIO_HIGH: f32 = 30.0;
/// Weight for moderate floor-ratio difference (> FLOOR_RATIO_LOW).
pub const WEIGHT_FLOOR_RATIO_LOW: f32 = 15.0;
/// Floor-ratio diff threshold for large penalty.
pub const FLOOR_RATIO_HIGH: f32 = 0.4;
/// Floor-ratio diff threshold for moderate penalty.
pub const FLOOR_RATIO_LOW: f32 = 0.2;
/// Light level diff threshold for penalty.
pub const LIGHT_DIFF_THRESHOLD: u32 = 32;
/// Weight when light diff exceeds threshold.
pub const WEIGHT_LIGHT_DIFF: f32 = 15.0;

// ============================================================================
// INTERNAL TYPES
// ============================================================================

/// Result of the region-building pipeline (stages 1–7).
struct RegionBuildResult {
    polygons: Vec<Vec<Vec2>>,
    regions: Vec<Vec<usize>>,
    ss_to_region: Vec<usize>,
    region_walls: Vec<Vec<(Vec2, Vec2)>>,
    region_portals: Vec<Vec<Portal>>,
}

/// A portal between two adjacent regions.
#[derive(Clone)]
struct Portal {
    neighbor: usize,
    segs: Vec<(Vec2, Vec2)>,
    floor: f32,
    ceil: f32,
}

/// BFS queue entry for visibility traversal.
struct BfsEntry {
    region: usize,
    src_segs: Vec<(Vec2, Vec2)>,
    src_floor: f32,
    src_ceil: f32,
    pass_segs: Vec<(Vec2, Vec2)>,
    pass_floor: f32,
    pass_ceil: f32,
    depth: u32,
}

// ============================================================================
// STAGE 1: BSP FRONTIER ADJACENCY + COLLINEAR TOUCH FILTER
// ============================================================================

/// Build the subsector adjacency graph from BSP frontier pairs filtered by
/// collinear polygon-edge touch.
fn build_adjacency(
    nodes: &[Node],
    start_node: u32,
    polygons: &[Vec<Vec2>],
    _subsectors: &[SubSector],
    _segments: &[Segment],
    num_ss: usize,
) -> HashMap<usize, HashSet<usize>> {
    // Precompute subsector bounding boxes
    let ss_bboxes: Vec<Option<(f32, f32, f32, f32)>> = (0..num_ss)
        .map(|ss| {
            let p = &polygons[ss];
            if p.is_empty() {
                None
            } else {
                let mut min_x = f32::MAX;
                let mut min_y = f32::MAX;
                let mut max_x = f32::MIN;
                let mut max_y = f32::MIN;
                for &v in p {
                    min_x = min_x.min(v.x);
                    min_y = min_y.min(v.y);
                    max_x = max_x.max(v.x);
                    max_y = max_y.max(v.y);
                }
                Some((min_x, min_y, max_x, max_y))
            }
        })
        .collect();

    // Phase 1: BSP frontier candidates (all-leaves + bbox overlap)
    let mut candidates: HashSet<(usize, usize)> = HashSet::new();
    build_frontier_adj(start_node, nodes, &ss_bboxes, num_ss, &mut candidates);
    info!("BSP frontier candidates: {}", candidates.len());

    // Phase 2: Collinear touch filter
    let mut adj: HashMap<usize, HashSet<usize>> = HashMap::new();
    for &(a, b) in &candidates {
        let pa = &polygons[a];
        let pb = &polygons[b];
        if pa.is_empty() || pb.is_empty() {
            continue;
        }
        if polys_touch(pa, pb, TOUCH_TOLERANCE, MIN_OVERLAP) {
            adj.entry(a).or_default().insert(b);
            adj.entry(b).or_default().insert(a);
        }
    }

    // Also add exact polygon-edge matches (catches any BSP frontier misses)
    let mut edge_to_ss: HashMap<((i32, i32), (i32, i32)), HashSet<usize>> = HashMap::new();
    for ss_id in 0..num_ss {
        let p = &polygons[ss_id];
        if p.is_empty() {
            continue;
        }
        let n = p.len();
        for i in 0..n {
            let p1 = quantize_point(p[i]);
            let p2 = quantize_point(p[(i + 1) % n]);
            let key = if p1 < p2 { (p1, p2) } else { (p2, p1) };
            edge_to_ss.entry(key).or_default().insert(ss_id);
        }
    }
    for ss_set in edge_to_ss.values() {
        let list: Vec<usize> = ss_set.iter().copied().collect();
        for i in 0..list.len() {
            for j in (i + 1)..list.len() {
                let a = list[i];
                let b = list[j];
                adj.entry(a).or_default().insert(b);
                adj.entry(b).or_default().insert(a);
            }
        }
    }

    let edge_count: usize = adj.values().map(|s| s.len()).sum::<usize>() / 2;
    info!("Adjacency edges (post-filter): {}", edge_count);
    adj
}

fn quantize_point(v: Vec2) -> (i32, i32) {
    ((v.x * 10.0).round() as i32, (v.y * 10.0).round() as i32)
}

/// Collect all subsector leaf indices under a BSP node.
fn node_leaves(node_id: u32, nodes: &[Node], num_ss: usize, result: &mut Vec<usize>) {
    if node_id & IS_SS_MASK != 0 {
        let ss = (node_id ^ IS_SS_MASK) as usize;
        if ss < num_ss {
            result.push(ss);
        }
        return;
    }
    let idx = node_id as usize;
    if idx >= nodes.len() {
        return;
    }
    let n = &nodes[idx];
    node_leaves(n.children[0], nodes, num_ss, result);
    node_leaves(n.children[1], nodes, num_ss, result);
}

/// Build BSP frontier adjacency: for each BSP node, find left/right leaf pairs
/// whose bounding boxes overlap.
fn build_frontier_adj(
    node_id: u32,
    nodes: &[Node],
    ss_bboxes: &[Option<(f32, f32, f32, f32)>],
    num_ss: usize,
    pairs: &mut HashSet<(usize, usize)>,
) {
    if node_id & IS_SS_MASK != 0 {
        return;
    }
    let idx = node_id as usize;
    if idx >= nodes.len() {
        return;
    }
    let n = &nodes[idx];

    let mut left_ss = Vec::new();
    let mut right_ss = Vec::new();
    node_leaves(n.children[0], nodes, num_ss, &mut left_ss);
    node_leaves(n.children[1], nodes, num_ss, &mut right_ss);

    for &lss in &left_ss {
        let lb = match ss_bboxes[lss] {
            Some(b) => b,
            None => continue,
        };
        for &rss in &right_ss {
            let rb = match ss_bboxes[rss] {
                Some(b) => b,
                None => continue,
            };
            // 2D bbox overlap check
            if lb.0 <= rb.2 && rb.0 <= lb.2 && lb.1 <= rb.3 && rb.1 <= lb.3 {
                let pair = if lss < rss { (lss, rss) } else { (rss, lss) };
                pairs.insert(pair);
            }
        }
    }

    build_frontier_adj(n.children[0], nodes, ss_bboxes, num_ss, pairs);
    build_frontier_adj(n.children[1], nodes, ss_bboxes, num_ss, pairs);
}

/// Check if any edge of polygon A has a collinear overlap with any edge of B.
fn polys_touch(a: &[Vec2], b: &[Vec2], tol: f32, min_overlap: f32) -> bool {
    if a.len() < 2 || b.len() < 2 {
        return false;
    }
    for i in 0..a.len() {
        let a1 = a[i];
        let a2 = a[(i + 1) % a.len()];
        let a_dir = a2 - a1;
        let a_len = a_dir.length();
        if a_len < 1e-6 {
            continue;
        }
        let a_norm = a_dir / a_len;
        let a_perp = Vec2::new(-a_norm.y, a_norm.x);

        for j in 0..b.len() {
            let b1 = b[j];
            let b2 = b[(j + 1) % b.len()];

            let d1 = (b1 - a1).dot(a_perp).abs();
            let d2 = (b2 - a1).dot(a_perp).abs();
            if d1 > tol || d2 > tol {
                continue;
            }

            let b_t1 = (b1 - a1).dot(a_norm);
            let b_t2 = (b2 - a1).dot(a_norm);
            let b_min = b_t1.min(b_t2);
            let b_max = b_t1.max(b_t2);

            let overlap_start = 0f32.max(b_min);
            let overlap_end = a_len.min(b_max);
            if overlap_end - overlap_start > min_overlap {
                return true;
            }
        }
    }
    false
}

// ============================================================================
// STAGE 2: EDGE WEIGHT ASSIGNMENT
// ============================================================================

/// Compute the edge weight between two adjacent subsectors.
/// Higher weight = stronger boundary (doors, height changes, light
/// transitions). Finds a two-sided segment between the two subsectors'
/// sectors, then assigns weight based on geometry.
/// If no two-sided linedef is found, returns 0 (internal BSP split).
fn compute_edge_weight(
    ss_a: usize,
    ss_b: usize,
    subsectors: &[SubSector],
    segments: &[Segment],
    _sectors: &[Sector],
    linedefs: &[LineDef],
    _polygons: &[Vec<Vec2>],
) -> f32 {
    let sec_a = &*subsectors[ss_a].sector;
    let sec_b = &*subsectors[ss_b].sector;

    // Find a two-sided segment connecting these two sectors
    let mut found_seg: Option<&Segment> = None;
    for seg in segments.iter() {
        if let Some(ref bs) = seg.backsector {
            let fs_id = seg.frontsector.num;
            let bs_id = bs.num;
            if (fs_id == sec_a.num && bs_id == sec_b.num)
                || (fs_id == sec_b.num && bs_id == sec_a.num)
            {
                found_seg = Some(seg);
                break;
            }
        }
    }

    let seg = match found_seg {
        Some(s) => s,
        None => return 0.0, // No two-sided linedef found → internal BSP split
    };

    let ld = &*seg.linedef;
    let mut weight = 0.0f32;

    let a_is_mover = is_sector_mover(sec_a, linedefs);
    let b_is_mover = is_sector_mover(sec_b, linedefs);

    if ld.special != 0 || a_is_mover || b_is_mover {
        weight += WEIGHT_SPECIAL;
        if a_is_mover || b_is_mover {
            weight += WEIGHT_MOVER;
        }
    }

    // Opening height
    let open_floor = sec_a.floorheight.max(sec_b.floorheight);
    let open_ceil = sec_a.ceilingheight.min(sec_b.ceilingheight);
    let opening = (open_ceil - open_floor).max(0.0);

    if opening <= 0.0 {
        weight += WEIGHT_SEALED;
    } else if opening < OPENING_NARROW {
        weight += WEIGHT_NARROW;
    } else if opening < OPENING_MEDIUM {
        weight += WEIGHT_MEDIUM;
    }

    // Floor ratio diff
    let ha = sec_a.ceilingheight - sec_a.floorheight;
    let hb = sec_b.ceilingheight - sec_b.floorheight;
    if ha > 0.0 && hb > 0.0 {
        let ratio_diff = (sec_a.floorheight / ha - sec_b.floorheight / hb).abs();
        if ratio_diff > FLOOR_RATIO_HIGH {
            weight += WEIGHT_FLOOR_RATIO_HIGH;
        } else if ratio_diff > FLOOR_RATIO_LOW {
            weight += WEIGHT_FLOOR_RATIO_LOW;
        }
    }

    // Light level diff
    let light_diff = (sec_a.lightlevel as i32 - sec_b.lightlevel as i32).unsigned_abs();
    if light_diff > LIGHT_DIFF_THRESHOLD {
        weight += WEIGHT_LIGHT_DIFF;
    }

    weight
}

// ============================================================================
// STAGE 3: FLOOD-FILL GROUPING
// ============================================================================

/// Cluster subsectors into regions by BFS across low-weight edges.
fn flood_fill_regions(
    adj: &HashMap<usize, HashSet<usize>>,
    weights: &HashMap<(usize, usize), f32>,
    threshold: f32,
    subsector_count: usize,
) -> Vec<Vec<usize>> {
    let mut assigned = vec![false; subsector_count];
    let mut regions = Vec::new();

    for start in 0..subsector_count {
        if assigned[start] {
            continue;
        }
        let mut region = Vec::new();
        let mut queue = VecDeque::new();
        queue.push_back(start);
        assigned[start] = true;

        while let Some(ss) = queue.pop_front() {
            region.push(ss);
            if let Some(neighbors) = adj.get(&ss) {
                for &n in neighbors {
                    if assigned[n] {
                        continue;
                    }
                    let key = if ss < n { (ss, n) } else { (n, ss) };
                    let w = weights.get(&key).copied().unwrap_or(0.0);
                    if w < threshold {
                        assigned[n] = true;
                        queue.push_back(n);
                    }
                }
            }
        }
        regions.push(region);
    }
    regions
}

// ============================================================================
// STAGE 4: BSP-DIVLINE SUBDIVISION
// ============================================================================

/// Split oversized regions using BSP divlines.
fn subdivide_regions(
    regions: Vec<Vec<usize>>,
    polygons: &[Vec<Vec2>],
    nodes: &[Node],
    max_count: usize,
    min_area: f32,
) -> Vec<Vec<usize>> {
    let mut result = Vec::new();
    for region in regions {
        subdivide_single(&region, polygons, nodes, max_count, min_area, &mut result);
    }
    result
}

fn subdivide_single(
    region: &[usize],
    polygons: &[Vec<Vec2>],
    nodes: &[Node],
    max_count: usize,
    min_area: f32,
    result: &mut Vec<Vec<usize>>,
) {
    if region.len() <= max_count {
        result.push(region.to_vec());
        return;
    }

    let (bbox_min, bbox_max) = region_bbox(region, polygons);
    let area = (bbox_max.x - bbox_min.x) * (bbox_max.y - bbox_min.y);
    if area < min_area {
        result.push(region.to_vec());
        return;
    }

    let centroids: Vec<Vec2> = region
        .iter()
        .map(|&ss| polygon_centroid(&polygons[ss]))
        .collect();

    if let Some((origin, normal)) = find_best_divline(nodes, &centroids) {
        let mut left = Vec::new();
        let mut right = Vec::new();
        for (i, &ss) in region.iter().enumerate() {
            if (centroids[i] - origin).dot(normal) >= 0.0 {
                right.push(ss);
            } else {
                left.push(ss);
            }
        }
        if left.is_empty() || right.is_empty() {
            result.push(region.to_vec());
            return;
        }
        subdivide_single(&left, polygons, nodes, max_count, min_area, result);
        subdivide_single(&right, polygons, nodes, max_count, min_area, result);
    } else {
        result.push(region.to_vec());
    }
}

/// Find the BSP divline that most evenly bisects the centroids.
/// Returns (origin, normal) of the best split.
fn find_best_divline(nodes: &[Node], centroids: &[Vec2]) -> Option<(Vec2, Vec2)> {
    let mut best_score = usize::MAX;
    let mut best = None;

    for node in nodes {
        let len = node.delta.length();
        if len < 1e-6 {
            continue;
        }
        let normal = Vec2::new(-node.delta.y, node.delta.x) / len;

        let mut left = 0usize;
        let mut right = 0usize;
        for &c in centroids {
            if (c - node.xy).dot(normal) >= 0.0 {
                right += 1;
            } else {
                left += 1;
            }
        }

        if left == 0 || right == 0 {
            continue;
        }
        let imbalance = left.abs_diff(right);
        if imbalance < best_score {
            best_score = imbalance;
            best = Some((node.xy, normal));
        }
    }
    best
}

// ============================================================================
// STAGE 5: SPATIAL CONNECTIVITY VALIDATION
// ============================================================================

/// Split regions with disconnected spatial components into separate regions.
fn split_disconnected(
    regions: Vec<Vec<usize>>,
    adj: &HashMap<usize, HashSet<usize>>,
) -> Vec<Vec<usize>> {
    let mut result = Vec::new();
    for region in regions {
        let members: HashSet<usize> = region.iter().copied().collect();
        let mut visited = HashSet::new();

        for &ss in &region {
            if visited.contains(&ss) {
                continue;
            }
            let mut component = Vec::new();
            let mut queue = VecDeque::new();
            queue.push_back(ss);
            visited.insert(ss);

            while let Some(v) = queue.pop_front() {
                component.push(v);
                if let Some(neighbors) = adj.get(&v) {
                    for &n in neighbors {
                        if members.contains(&n) && !visited.contains(&n) {
                            visited.insert(n);
                            queue.push_back(n);
                        }
                    }
                }
            }
            result.push(component);
        }
    }
    result
}

// ============================================================================
// STAGE 6: SOLID WALL INVENTORY
// ============================================================================

/// Collect one-sided wall segments per region for shadow subtraction.
fn collect_region_walls(
    regions: &[Vec<usize>],
    subsectors: &[SubSector],
    segments: &[Segment],
) -> Vec<Vec<(Vec2, Vec2)>> {
    regions
        .iter()
        .map(|region| {
            let mut walls = Vec::new();
            for &ss in region {
                let ss_data = &subsectors[ss];
                let start = ss_data.start_seg as usize;
                let end = start + ss_data.seg_count as usize;
                for seg_idx in start..end.min(segments.len()) {
                    let seg = &segments[seg_idx];
                    if seg.backsector.is_none() {
                        walls.push((*seg.v1, *seg.v2));
                    }
                }
            }
            walls
        })
        .collect()
}

// ============================================================================
// STAGE 7: PORTAL IDENTIFICATION
// ============================================================================

/// Find portals between adjacent regions.
fn find_portals(
    regions: &[Vec<usize>],
    ss_to_region: &[usize],
    adj: &HashMap<usize, HashSet<usize>>,
    subsectors: &[SubSector],
    segments: &[Segment],
    polygons: &[Vec<Vec2>],
    _sectors: &[Sector],
    bsp: &BSP3D,
    _linedefs: &[LineDef],
) -> Vec<Vec<Portal>> {
    let region_count = regions.len();

    // Keyed by (min_region, max_region) → list of (segment, floor, ceil)
    let mut portal_segs: HashMap<(usize, usize), Vec<((Vec2, Vec2), f32, f32)>> = HashMap::new();

    // §4.1: Linedef-backed portal construction
    for (ss_id, ss) in subsectors.iter().enumerate() {
        let r_a = ss_to_region[ss_id];
        let start = ss.start_seg as usize;
        let end = start + ss.seg_count as usize;

        for seg_idx in start..end.min(segments.len()) {
            let seg = &segments[seg_idx];
            let back_sector = match &seg.backsector {
                Some(bs) => bs,
                None => continue,
            };

            let v1: Vec2 = *seg.v1;
            let v2: Vec2 = *seg.v2;
            let front_sector = &*seg.frontsector;
            let back_sector_id = back_sector.num as usize;

            if back_sector_id >= bsp.sector_subsectors.len() {
                continue;
            }
            let candidates = &bsp.sector_subsectors[back_sector_id];

            let mut found = false;
            for &cand_ss in candidates {
                if cand_ss >= ss_to_region.len() {
                    continue;
                }
                if ss_to_region[cand_ss] == r_a {
                    continue;
                }
                let poly = &polygons[cand_ss];
                if poly.is_empty() {
                    continue;
                }

                if let Some(overlap) = collinear_segment_overlap(v1, v2, poly, SEGMENT_OVERLAP_TOL)
                {
                    let r_b = ss_to_region[cand_ss];
                    let key = if r_a < r_b { (r_a, r_b) } else { (r_b, r_a) };
                    let (floor, ceil) = portal_height(front_sector, &**back_sector);
                    portal_segs
                        .entry(key)
                        .or_default()
                        .push((overlap, floor, ceil));
                    found = true;
                    break;
                }
            }

            // §4.1.1 Fallback: nearest candidate, use shared polygon boundary
            if !found {
                let seg_mid = (v1 + v2) * 0.5;
                let mut best_dist = f32::MAX;
                let mut best_cand = None;
                for &cand_ss in candidates {
                    if cand_ss >= ss_to_region.len() {
                        continue;
                    }
                    if ss_to_region[cand_ss] == r_a {
                        continue;
                    }
                    let poly = &polygons[cand_ss];
                    if poly.is_empty() {
                        continue;
                    }
                    let centroid = polygon_centroid(poly);
                    let dist = (centroid - seg_mid).length_squared();
                    if dist < best_dist {
                        best_dist = dist;
                        best_cand = Some(cand_ss);
                    }
                }
                if let Some(cand) = best_cand {
                    let r_b = ss_to_region[cand];
                    if r_b != r_a {
                        let key = if r_a < r_b { (r_a, r_b) } else { (r_b, r_a) };
                        let (floor, ceil) = portal_height(front_sector, &**back_sector);
                        let poly_src = &polygons[ss_id];
                        let poly_dst = &polygons[cand];
                        if let Some(shared) = shared_boundary_segment(poly_src, poly_dst) {
                            portal_segs
                                .entry(key)
                                .or_default()
                                .push((shared, floor, ceil));
                        }
                    }
                }
            }
        }
    }

    let linedef_portal_count = portal_segs.len();

    // §4.3: BSP-adjacency fallback for cross-region pairs with no existing portal
    for (&ss_a, neighbors) in adj.iter() {
        for &ss_b in neighbors {
            if ss_a >= ss_b {
                continue;
            }
            let r_a = ss_to_region[ss_a];
            let r_b = ss_to_region[ss_b];
            if r_a == r_b {
                continue;
            }
            let key = if r_a < r_b { (r_a, r_b) } else { (r_b, r_a) };
            if portal_segs.contains_key(&key) {
                continue;
            }

            let poly_a = &polygons[ss_a];
            let poly_b = &polygons[ss_b];
            let seg = shared_boundary_segment(poly_a, poly_b).unwrap_or_else(|| {
                // Centroid fallback
                let ca = polygon_centroid(poly_a);
                let cb = polygon_centroid(poly_b);
                let mid = (ca + cb) * 0.5;
                let dir = (cb - ca).normalize_or_zero();
                let perp = Vec2::new(-dir.y, dir.x);
                (mid - perp * 0.5, mid + perp * 0.5)
            });

            let sector_a = &*subsectors[ss_a].sector;
            let sector_b = &*subsectors[ss_b].sector;
            let (floor, ceil) = portal_height(sector_a, sector_b);
            portal_segs.entry(key).or_default().push((seg, floor, ceil));
        }
    }

    let synthetic_count = portal_segs.len() - linedef_portal_count;
    info!(
        "Portal pairs: {} linedef-backed, {} synthetic",
        linedef_portal_count, synthetic_count
    );
    // §4.4: Deduplicate and split disconnected segment groups
    let mut region_portals: Vec<Vec<Portal>> = vec![Vec::new(); region_count];

    for ((r_a, r_b), segs) in portal_segs {
        // Deduplicate segments by quantized endpoints
        let mut seen: HashSet<(i32, i32, i32, i32)> = HashSet::new();
        let mut unique_segs = Vec::new();
        for s in &segs {
            let p1 = quantize_point(s.0.0);
            let p2 = quantize_point(s.0.1);
            let key = (p1.0, p1.1, p2.0, p2.1);
            if seen.insert(key) {
                unique_segs.push(s.clone());
            }
        }

        let groups = split_connected_segments(&unique_segs);

        for group in groups {
            let (floor, ceil) = group
                .iter()
                .fold((f32::MAX, f32::MIN), |(f, c), &(_, fl, cl)| {
                    (f.min(fl), c.max(cl))
                });
            let seg_list: Vec<(Vec2, Vec2)> = group.into_iter().map(|(s, ..)| s).collect();

            region_portals[r_a].push(Portal {
                neighbor: r_b,
                segs: seg_list.clone(),
                floor,
                ceil,
            });
            region_portals[r_b].push(Portal {
                neighbor: r_a,
                segs: seg_list,
                floor,
                ceil,
            });
        }
    }

    let entry_count: usize = region_portals.iter().map(|p| p.len()).sum::<usize>() / 2;
    info!(
        "Portal entries (after segment group splitting): {}",
        entry_count
    );

    region_portals
}

/// Find the collinear overlap of a segment with a polygon edge.
fn collinear_segment_overlap(
    seg_v1: Vec2,
    seg_v2: Vec2,
    polygon: &[Vec2],
    tolerance: f32,
) -> Option<(Vec2, Vec2)> {
    let seg_dir = seg_v2 - seg_v1;
    let seg_len = seg_dir.length();
    if seg_len < 1e-6 {
        return None;
    }
    let seg_norm = seg_dir / seg_len;
    let seg_perp = Vec2::new(-seg_norm.y, seg_norm.x);

    for i in 0..polygon.len() {
        let e1 = polygon[i];
        let e2 = polygon[(i + 1) % polygon.len()];

        let d1 = (e1 - seg_v1).dot(seg_perp).abs();
        let d2 = (e2 - seg_v1).dot(seg_perp).abs();
        if d1 > tolerance || d2 > tolerance {
            continue;
        }

        let t1 = (e1 - seg_v1).dot(seg_norm);
        let t2 = (e2 - seg_v1).dot(seg_norm);
        let e_min = t1.min(t2);
        let e_max = t1.max(t2);

        let overlap_start = 0f32.max(e_min);
        let overlap_end = seg_len.min(e_max);
        if overlap_end - overlap_start > 0.5 {
            return Some((
                seg_v1 + seg_norm * overlap_start,
                seg_v1 + seg_norm * overlap_end,
            ));
        }
    }
    None
}

/// Find a shared boundary segment between two polygons.
fn shared_boundary_segment(poly_a: &[Vec2], poly_b: &[Vec2]) -> Option<(Vec2, Vec2)> {
    for i in 0..poly_a.len() {
        let a1 = poly_a[i];
        let a2 = poly_a[(i + 1) % poly_a.len()];
        if let Some(overlap) = collinear_segment_overlap(a1, a2, poly_b, TOUCH_TOLERANCE) {
            if (overlap.0 - overlap.1).length_squared() > 0.25 {
                return Some(overlap);
            }
        }
    }
    None
}

/// Compute portal opening height from the sectors on each side.
fn portal_height(front: &Sector, back: &Sector) -> (f32, f32) {
    let open_floor = front.floorheight.max(back.floorheight);
    let open_ceil = front.ceilingheight.min(back.ceilingheight);
    if open_ceil > open_floor {
        (open_floor, open_ceil)
    } else {
        // Zero-height door fallback: assume fully open
        (
            front.floorheight.min(back.floorheight),
            front.ceilingheight.max(back.ceilingheight),
        )
    }
}

/// Split portal segments into connected groups by shared quantized endpoints.
fn split_connected_segments(
    segs: &[((Vec2, Vec2), f32, f32)],
) -> Vec<Vec<((Vec2, Vec2), f32, f32)>> {
    let n = segs.len();
    if n <= 1 {
        return vec![segs.to_vec()];
    }

    let mut remaining: Vec<usize> = (0..n).collect();
    let mut groups = Vec::new();

    while !remaining.is_empty() {
        let first = remaining.remove(0);
        let mut grp = vec![first];
        let mut pts: HashSet<(i32, i32)> = HashSet::new();
        let (s, ..) = &segs[first];
        pts.insert(quantize_point(s.0));
        pts.insert(quantize_point(s.1));

        let mut changed = true;
        while changed {
            changed = false;
            let mut new_remaining = Vec::new();
            for &idx in &remaining {
                let (s, ..) = &segs[idx];
                let p1 = quantize_point(s.0);
                let p2 = quantize_point(s.1);
                if pts.contains(&p1) || pts.contains(&p2) {
                    grp.push(idx);
                    pts.insert(p1);
                    pts.insert(p2);
                    changed = true;
                } else {
                    new_remaining.push(idx);
                }
            }
            remaining = new_remaining;
        }

        groups.push(grp.into_iter().map(|i| segs[i].clone()).collect());
    }

    groups
}

// ============================================================================
// STAGE 8: VISIBILITY DETERMINATION
// ============================================================================

/// Compute region centroid from all polygon vertices.
fn region_centroid(region: &[usize], polygons: &[Vec<Vec2>]) -> Vec2 {
    let mut sum = Vec2::ZERO;
    let mut count = 0usize;
    for &ss in region {
        for &v in &polygons[ss] {
            sum += v;
            count += 1;
        }
    }
    if count > 0 {
        sum / count as f32
    } else {
        Vec2::ZERO
    }
}

/// Run the full region-building pipeline (stages 1–7).
fn build_regions(
    subsectors: &[SubSector],
    segments: &[Segment],
    bsp: &BSP3D,
    sectors: &[Sector],
    linedefs: &[LineDef],
    nodes: &[Node],
    start_node: u32,
) -> RegionBuildResult {
    let subsector_count = subsectors.len();

    let polygons = carve_subsector_polygons_2d(
        start_node,
        nodes,
        subsectors,
        segments,
        &bsp.sector_subsectors,
    );

    // Stage 1: Build adjacency
    let adj = build_adjacency(
        nodes,
        start_node,
        &polygons,
        subsectors,
        segments,
        subsector_count,
    );

    // Stage 2: Edge weights
    let mut weights: HashMap<(usize, usize), f32> = HashMap::new();
    for (&ss, neighbors) in &adj {
        for &n in neighbors {
            if ss < n {
                let w =
                    compute_edge_weight(ss, n, subsectors, segments, sectors, linedefs, &polygons);
                weights.insert((ss, n), w);
            }
        }
    }

    // Stage 3: Flood fill
    let regions = flood_fill_regions(&adj, &weights, FLOOD_THRESHOLD, subsector_count);
    info!("Regions after flood fill: {}", regions.len());

    // Stage 4: BSP-divline subdivision
    let regions = subdivide_regions(
        regions,
        &polygons,
        nodes,
        MAX_REGION_SIZE,
        MIN_SUBDIVISION_AREA,
    );
    info!("Regions after subdivision: {}", regions.len());

    // Stage 5: Connectivity split
    let regions = split_disconnected(regions, &adj);
    info!("Regions after connectivity split: {}", regions.len());

    // Build ss_to_region mapping
    let mut ss_to_region = vec![0usize; subsector_count];
    for (rid, region) in regions.iter().enumerate() {
        for &ss in region {
            ss_to_region[ss] = rid;
        }
    }

    // Stage 6: Solid walls
    let region_walls = collect_region_walls(&regions, subsectors, segments);

    // Stage 7: Portal identification
    let region_portals = find_portals(
        &regions,
        &ss_to_region,
        &adj,
        subsectors,
        segments,
        &polygons,
        sectors,
        bsp,
        linedefs,
    );

    RegionBuildResult {
        polygons,
        regions,
        ss_to_region,
        region_walls,
        region_portals,
    }
}

/// Compute which regions are visible from a source region via BFS with
/// source→pass→target anti-penumbra frustum clipping.
/// Returns (visible regions, max visit count reached for any single region).
fn compute_visible_regions(
    source: usize,
    region_portals: &[Vec<Portal>],
    region_walls: &[Vec<(Vec2, Vec2)>],
    regions: &[Vec<usize>],
    polygons: &[Vec<Vec2>],
) -> (HashSet<usize>, u32) {
    let mut visible = HashSet::new();
    visible.insert(source);
    let mut overall_max_visits: u32 = 0;

    for portal in &region_portals[source] {
        let neighbor = portal.neighbor;
        visible.insert(neighbor);

        let src_segs = portal.segs.clone();
        let sp = portal_pts(&src_segs);
        if sp.len() < 2 {
            continue;
        }
        let src_fl = portal.floor;
        let src_cl = portal.ceil;

        let mut queue: VecDeque<BfsEntry> = VecDeque::new();
        let mut visit_count: HashMap<usize, u32> = HashMap::new();
        *visit_count.entry(source).or_default() += 1;
        *visit_count.entry(neighbor).or_default() += 1;

        queue.push_back(BfsEntry {
            region: neighbor,
            src_segs,
            src_floor: src_fl,
            src_ceil: src_cl,
            pass_segs: portal.segs.clone(),
            pass_floor: src_fl,
            pass_ceil: src_cl,
            depth: 1,
        });

        while let Some(entry) = queue.pop_front() {
            if entry.depth > MAX_BFS_DEPTH {
                continue;
            }

            let cur_sp = portal_pts(&entry.src_segs);
            if cur_sp.len() < 2 {
                continue;
            }
            let pp = portal_pts(&entry.pass_segs);
            if pp.len() < 2 {
                continue;
            }

            let sp_center = centroid_of_points(&cur_sp);
            let pp_center = centroid_of_points(&pp);
            let mut fwd = pp_center - sp_center;
            let mut fwd_len = fwd.length();

            // Fallback: use region centroid if fwd too short
            if fwd_len < 1.0 {
                let rc = region_centroid(&regions[entry.region], polygons);
                fwd = rc - sp_center;
                fwd_len = fwd.length();
                if fwd_len < 1.0 {
                    continue;
                }
            }

            let fwd_dx = fwd.x;
            let fwd_dy = fwd.y;
            let fwd_norm = fwd / fwd_len;

            for target_portal in &region_portals[entry.region] {
                let target_region = target_portal.neighbor;
                let count = visit_count.get(&target_region).copied().unwrap_or(0);
                if count >= MAX_REGION_VISITS {
                    continue;
                }

                // Step 1: Frustum clip target through source→pass
                let clipped_target = clip_target_segs(&target_portal.segs, &cur_sp, &pp, fwd_norm);
                if clipped_target.is_empty() {
                    continue;
                }

                // Step 2: Wall shadow subtraction
                let mut clipped = clipped_target;
                for wall in &region_walls[entry.region] {
                    if clipped.is_empty() {
                        break;
                    }
                    clipped = subtract_wall_shadow(&clipped, wall, &cur_sp, fwd_dx, fwd_dy);
                }
                if clipped.is_empty() {
                    continue;
                }

                let ct_pts = portal_pts(&clipped);
                if ct_pts.len() < 2 {
                    continue;
                }

                // Step 3: Mutual refinement (reverse clip)
                let rev_fwd = -fwd_norm;
                let clipped_source = clip_target_segs(&entry.src_segs, &ct_pts, &pp, rev_fwd);
                let final_src =
                    if !clipped_source.is_empty() && portal_pts(&clipped_source).len() >= 2 {
                        clipped_source
                    } else {
                        entry.src_segs.clone()
                    };

                // Step 4: Height clip (Euclidean distances)
                let tgt_fl = target_portal.floor;
                let tgt_cl = target_portal.ceil;

                let d_sp = ((pp_center.x - sp_center.x).powi(2)
                    + (pp_center.y - sp_center.y).powi(2))
                .sqrt();

                let tc = centroid_of_points(&ct_pts);
                let d_st = ((tc.x - sp_center.x).powi(2) + (tc.y - sp_center.y).powi(2)).sqrt();

                let (vis_fl, vis_cl) = if d_sp > 1.0 {
                    let ratio = d_st / d_sp;
                    let mut frust_cl =
                        entry.src_floor + (entry.pass_ceil - entry.src_floor) * ratio;
                    let mut frust_fl = entry.src_ceil + (entry.pass_floor - entry.src_ceil) * ratio;
                    // Swap if inverted
                    if frust_fl > frust_cl {
                        std::mem::swap(&mut frust_fl, &mut frust_cl);
                    }
                    let vf = tgt_fl.max(frust_fl);
                    let vc = tgt_cl.min(frust_cl);
                    if vc <= vf {
                        continue;
                    }
                    (vf, vc)
                } else {
                    (tgt_fl, tgt_cl)
                };

                visible.insert(target_region);
                *visit_count.entry(target_region).or_default() += 1;

                queue.push_back(BfsEntry {
                    region: target_region,
                    src_segs: final_src,
                    src_floor: entry.src_floor,
                    src_ceil: entry.src_ceil,
                    pass_segs: clipped,
                    pass_floor: vis_fl,
                    pass_ceil: vis_cl,
                    depth: entry.depth + 1,
                });
            }
        }
        let chain_max = visit_count.values().copied().max().unwrap_or(0);
        overall_max_visits = overall_max_visits.max(chain_max);
    }

    (visible, overall_max_visits)
}

// ============================================================================
// GEOMETRIC OPERATIONS
// ============================================================================

/// Signed cross product of (b-o) × (p-o).
fn cross2d(o: Vec2, b: Vec2, p: Vec2) -> f32 {
    (b.x - o.x) * (p.y - o.y) - (b.y - o.y) * (p.x - o.x)
}

/// Find leftmost and rightmost points relative to a forward direction.
fn get_lr(pts: &[Vec2], fwd: Vec2) -> (Vec2, Vec2) {
    let perp = Vec2::new(-fwd.y, fwd.x);
    let mut min_dot = f32::MAX;
    let mut max_dot = f32::MIN;
    let mut left = pts[0];
    let mut right = pts[0];
    for &p in pts {
        let d = perp.dot(p);
        if d > max_dot {
            max_dot = d;
            left = p;
        }
        if d < min_dot {
            min_dot = d;
            right = p;
        }
    }
    (left, right)
}

/// Clip segment p1-p2 against the half-plane of line la→lb.
fn clip_seg_half(p1: Vec2, p2: Vec2, la: Vec2, lb: Vec2, keep_left: bool) -> Option<(Vec2, Vec2)> {
    let mut c1 = cross2d(la, lb, p1);
    let mut c2 = cross2d(la, lb, p2);
    if !keep_left {
        c1 = -c1;
        c2 = -c2;
    }

    if c1 >= 0.0 && c2 >= 0.0 {
        return Some((p1, p2));
    }
    if c1 < 0.0 && c2 < 0.0 {
        return None;
    }

    let t = c1 / (c1 - c2);
    let intersection = p1 + t * (p2 - p1);
    if c1 >= 0.0 {
        Some((p1, intersection))
    } else {
        Some((intersection, p2))
    }
}

/// Clip target portal segments against the source→pass frustum.
fn clip_target_segs(
    target_segs: &[(Vec2, Vec2)],
    src_pts: &[Vec2],
    pass_pts: &[Vec2],
    fwd: Vec2,
) -> Vec<(Vec2, Vec2)> {
    let (src_left, src_right) = get_lr(src_pts, fwd);
    let (pass_left, pass_right) = get_lr(pass_pts, fwd);

    let mut result = Vec::new();
    for &(p1, p2) in target_segs {
        // Right boundary: src_right → pass_left (keep right side)
        let clipped = match clip_seg_half(p1, p2, src_right, pass_left, false) {
            Some(s) => s,
            None => continue,
        };
        // Left boundary: src_left → pass_right (keep left side)
        if let Some(s) = clip_seg_half(clipped.0, clipped.1, src_left, pass_right, true) {
            result.push(s);
        }
    }
    result
}

/// Subtract the shadow cone cast by a one-sided wall from target segments
/// using a parametric shadow cone.
fn subtract_wall_shadow(
    target_segs: &[(Vec2, Vec2)],
    wall: &(Vec2, Vec2),
    src_pts: &[Vec2],
    fwd_dx: f32,
    fwd_dy: f32,
) -> Vec<(Vec2, Vec2)> {
    if src_pts.len() < 2 {
        return target_segs.to_vec();
    }

    let fwd = Vec2::new(fwd_dx, fwd_dy);
    let (src_left, src_right) = get_lr(src_pts, fwd);
    let wall_pts = [wall.0, wall.1];
    let (wall_left, wall_right) = get_lr(&wall_pts, fwd);

    // Shadow edges: left = src_left→wall_right, right = src_right→wall_left
    let sl_a = src_left;
    let sl_b = wall_right;
    let sr_a = src_right;
    let sr_b = wall_left;

    let mut result = Vec::new();
    for &(p1, p2) in target_segs {
        let x1 = p1.x;
        let y1 = p1.y;
        let x2 = p2.x;
        let y2 = p2.y;

        let cl1 = cross2d(sl_a, sl_b, p1);
        let cl2 = cross2d(sl_a, sl_b, p2);
        let cr1 = cross2d(sr_a, sr_b, p1);
        let cr2 = cross2d(sr_a, sr_b, p2);

        let in1 = cl1 < 0.0 && cr1 > 0.0;
        let in2 = cl2 < 0.0 && cr2 > 0.0;

        if !in1 && !in2 {
            // Neither endpoint in shadow
            if cl1 >= 0.0 && cl2 >= 0.0 {
                result.push((p1, p2));
                continue;
            }
            if cr1 <= 0.0 && cr2 <= 0.0 {
                result.push((p1, p2));
                continue;
            }
            // Segment may pass through shadow cone
            let tl = if (cl1 - cl2).abs() > 1e-10 {
                Some(cl1 / (cl1 - cl2))
            } else {
                None
            };
            let tr = if (cr1 - cr2).abs() > 1e-10 {
                Some(cr1 / (cr1 - cr2))
            } else {
                None
            };

            let sl_range = if cl1 < 0.0 {
                (0.0, tl.filter(|&t| (0.0..=1.0).contains(&t)).unwrap_or(0.0))
            } else {
                (tl.filter(|&t| (0.0..=1.0).contains(&t)).unwrap_or(1.0), 1.0)
            };
            let sr_range = if cr1 > 0.0 {
                (0.0, tr.filter(|&t| (0.0..=1.0).contains(&t)).unwrap_or(0.0))
            } else {
                (tr.filter(|&t| (0.0..=1.0).contains(&t)).unwrap_or(1.0), 1.0)
            };

            let s_start = sl_range.0.max(sr_range.0);
            let s_end = sl_range.1.min(sr_range.1);
            if s_start >= s_end - 0.001 {
                result.push((p1, p2));
                continue;
            }
            let dx = x2 - x1;
            let dy = y2 - y1;
            if s_start > 0.001 {
                result.push((p1, Vec2::new(x1 + s_start * dx, y1 + s_start * dy)));
            }
            if s_end < 0.999 {
                result.push((Vec2::new(x1 + s_end * dx, y1 + s_end * dy), p2));
            }
            continue;
        }

        if in1 && in2 {
            // Both in shadow — fully occluded
            continue;
        }

        // One in, one out
        let dx = x2 - x1;
        let dy = y2 - y1;
        let tl = if (cl1 - cl2).abs() > 1e-10 {
            Some(cl1 / (cl1 - cl2))
        } else {
            None
        };
        let tr = if (cr1 - cr2).abs() > 1e-10 {
            Some(cr1 / (cr1 - cr2))
        } else {
            None
        };

        if in1 && !in2 {
            // p1 in shadow, p2 outside — keep from exit point to p2
            let valid: Vec<f32> = [tl, tr]
                .iter()
                .filter_map(|t| t.filter(|&v| (0.0..=1.0).contains(&v)))
                .collect();
            let t_exit = valid.iter().copied().fold(0.0f32, f32::max);
            if t_exit < 0.999 {
                result.push((Vec2::new(x1 + t_exit * dx, y1 + t_exit * dy), p2));
            }
        } else {
            // p2 in shadow, p1 outside — keep from p1 to entry point
            let valid: Vec<f32> = [tl, tr]
                .iter()
                .filter_map(|t| t.filter(|&v| (0.0..=1.0).contains(&v)))
                .collect();
            let t_enter = valid.iter().copied().fold(1.0f32, f32::min);
            if t_enter > 0.001 {
                result.push((p1, Vec2::new(x1 + t_enter * dx, y1 + t_enter * dy)));
            }
        }
    }
    result
}

// ============================================================================
// HELPERS
// ============================================================================

fn polygon_centroid(poly: &[Vec2]) -> Vec2 {
    if poly.is_empty() {
        return Vec2::ZERO;
    }
    let sum: Vec2 = poly.iter().copied().sum();
    sum / poly.len() as f32
}

fn region_bbox(region: &[usize], polygons: &[Vec<Vec2>]) -> (Vec2, Vec2) {
    let mut min = Vec2::new(f32::MAX, f32::MAX);
    let mut max = Vec2::new(f32::MIN, f32::MIN);
    for &ss in region {
        for &v in &polygons[ss] {
            min = min.min(v);
            max = max.max(v);
        }
    }
    (min, max)
}

/// Collect unique portal endpoints using set-based dedup.
fn portal_pts(segs: &[(Vec2, Vec2)]) -> Vec<Vec2> {
    let mut seen: HashSet<(u32, u32)> = HashSet::new();
    let mut pts = Vec::new();
    for &(a, b) in segs {
        let ka = (a.x.to_bits(), a.y.to_bits());
        let kb = (b.x.to_bits(), b.y.to_bits());
        if seen.insert(ka) {
            pts.push(a);
        }
        if seen.insert(kb) {
            pts.push(b);
        }
    }
    pts
}

fn centroid_of_points(pts: &[Vec2]) -> Vec2 {
    if pts.is_empty() {
        return Vec2::ZERO;
    }
    let sum: Vec2 = pts.iter().copied().sum();
    sum / pts.len() as f32
}

// ============================================================================
// PVS PUBLIC INTERFACE
// ============================================================================

/// A portal segment between two adjacent regions, for visualization.
#[derive(Clone, Default)]
pub struct RegionPortal {
    pub region_a: usize,
    pub region_b: usize,
    pub segs: Vec<(Vec2, Vec2)>,
}

#[derive(Default, Clone)]
pub struct PVS {
    subsector_count: usize,
    visibility_data: CompactPVS,
    subsector_to_region: Vec<usize>,
    region_count: usize,
    /// For each region, the set of visible region indices.
    region_visibility: Vec<Vec<usize>>,
    /// Portal segments between regions (for visualization, not serialized).
    region_portals: Vec<RegionPortal>,
}

impl PVS {
    pub fn new(subsector_count: usize) -> Self {
        Self {
            subsector_count,
            visibility_data: CompactPVS::new(subsector_count),
            subsector_to_region: Vec::new(),
            region_count: 0,
            region_visibility: Vec::new(),
            region_portals: Vec::new(),
        }
    }

    pub fn build(
        &mut self,
        subsectors: &[SubSector],
        segments: &[Segment],
        bsp: &BSP3D,
        sectors: &[Sector],
        linedefs: &[LineDef],
        nodes: &[Node],
        start_node: u32,
    ) {
        let subsector_count = subsectors.len();
        info!("Coarse PVS build: {} subsectors", subsector_count);

        let rb = build_regions(
            subsectors, segments, bsp, sectors, linedefs, nodes, start_node,
        );
        let RegionBuildResult {
            polygons,
            regions,
            ss_to_region,
            region_walls,
            region_portals,
            ..
        } = rb;

        // Extract portal data for visualization (deduplicated by pair)
        let mut portal_vis = Vec::new();
        let mut seen_pairs: HashSet<(usize, usize)> = HashSet::new();
        for (r, portals) in region_portals.iter().enumerate() {
            for p in portals {
                let key = (r.min(p.neighbor), r.max(p.neighbor));
                if seen_pairs.insert(key) {
                    portal_vis.push(RegionPortal {
                        region_a: key.0,
                        region_b: key.1,
                        segs: p.segs.clone(),
                    });
                }
            }
        }

        // Stage 8: Visibility determination
        self.visibility_data = CompactPVS::new(subsector_count);
        self.subsector_count = subsector_count;
        self.subsector_to_region = ss_to_region;
        self.region_count = regions.len();
        self.region_portals = portal_vis;

        info!("Computing visibility for {} regions...", regions.len());

        let mut all_region_vis = vec![Vec::new(); regions.len()];
        let mut global_max_visits: u32 = 0;
        for source_region in 0..regions.len() {
            let (visible_regions, max_visits) = compute_visible_regions(
                source_region,
                &region_portals,
                &region_walls,
                &regions,
                &polygons,
            );
            global_max_visits = global_max_visits.max(max_visits);

            // Expand: all subsectors in visible regions are visible from source region
            for &vis_region in &visible_regions {
                for &from_ss in &regions[source_region] {
                    for &to_ss in &regions[vis_region] {
                        self.visibility_data.set_visible_atomic(from_ss, to_ss);
                    }
                }
            }
            all_region_vis[source_region] = visible_regions.into_iter().collect();
        }
        self.region_visibility = all_region_vis;

        let pairs = self.visibility_data.count_visible_pairs();
        let total = subsector_count as u64 * subsector_count as u64;
        let cull = if total > 0 {
            100.0 * (1.0 - pairs as f64 / total as f64)
        } else {
            0.0
        };
        info!(
            "Coarse PVS complete: {} visible pairs / {} total ({:.1}% cull rate), max region visits: {}/{}",
            pairs, total, cull, global_max_visits, MAX_REGION_VISITS
        );
    }

    pub fn is_visible(&self, from: usize, to: usize) -> bool {
        self.visibility_data.is_visible(from, to)
    }

    pub fn get_visible_subsectors(&self, from: usize) -> Vec<usize> {
        self.visibility_data.get_visible_subsectors(from)
    }

    pub fn max_flood_depth(&self, _ss: usize) -> u32 {
        0
    }

    pub fn count_visible_pairs(&self) -> u64 {
        self.visibility_data.count_visible_pairs()
    }

    pub fn memory_usage(&self) -> usize {
        self.visibility_data.memory_usage()
    }

    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), Box<dyn std::error::Error>> {
        use std::io::Write;
        let mut file = std::fs::File::create(path)?;
        file.write_all(b"PVS8")?;
        file.write_all(&self.subsector_count.to_le_bytes())?;
        file.write_all(&self.visibility_data.data.len().to_le_bytes())?;
        let data_bytes: Vec<u8> = self
            .visibility_data
            .data
            .iter()
            .flat_map(|&word| word.to_le_bytes())
            .collect();
        file.write_all(&data_bytes)?;
        Ok(())
    }

    pub fn load_from_cache(
        map_name: &str,
        map_hash: u64,
        expected_subsectors: usize,
    ) -> Option<Self> {
        match Self::get_pvs_cache_path(map_name, map_hash) {
            Ok(cache_path) => {
                if cache_path.exists() {
                    info!("Found PVS data at {cache_path:?}");
                    match Self::load_from_file(&cache_path) {
                        Ok(pvs) => {
                            if pvs.subsector_count == expected_subsectors {
                                Some(pvs)
                            } else {
                                None
                            }
                        }
                        Err(_) => None,
                    }
                } else {
                    None
                }
            }
            Err(_) => None,
        }
    }

    pub fn load_from_file(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        use std::io::Read;
        let mut file = std::fs::File::open(path)?;

        let mut header = [0u8; 4];
        file.read_exact(&mut header)?;
        if &header != b"PVS8" {
            return Err("Invalid PVS file format (expected PVS8)".into());
        }

        let mut size_buffer = [0u8; 8];
        file.read_exact(&mut size_buffer)?;
        let subsector_count = usize::from_le_bytes(size_buffer);

        file.read_exact(&mut size_buffer)?;
        let data_len = usize::from_le_bytes(size_buffer);

        let mut data_bytes = vec![0u8; data_len * 4];
        file.read_exact(&mut data_bytes)?;
        let data: Vec<u32> = data_bytes
            .chunks_exact(4)
            .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect();

        let visibility_data = CompactPVS {
            subsector_count,
            data,
        };

        Ok(Self {
            subsector_count,
            visibility_data,
            subsector_to_region: Vec::new(),
            region_count: 0,
            region_visibility: Vec::new(),
            region_portals: Vec::new(),
        })
    }

    pub fn get_pvs_cache_path(
        map_name: &str,
        map_hash: u64,
    ) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
        let cache_dir = dirs::cache_dir()
            .ok_or("Could not determine cache directory")?
            .join("room4doom")
            .join("pvs");
        std::fs::create_dir_all(&cache_dir)?;
        let filename = format!("{map_name}_{map_hash}.pvs");
        Ok(cache_dir.join(filename))
    }

    /// Return coarse region count.
    pub fn region_count(&self) -> usize {
        self.region_count
    }

    /// Return subsector-to-region mapping.
    pub fn subsector_to_region(&self) -> &[usize] {
        &self.subsector_to_region
    }

    /// Per-region visibility: for each region, the list of visible region
    /// indices.
    pub fn region_visibility(&self) -> &[Vec<usize>] {
        &self.region_visibility
    }

    /// Portal segments between regions (for visualization).
    pub fn region_portals(&self) -> &[RegionPortal] {
        &self.region_portals
    }

    /// Trace visibility between two sectors, printing detailed diagnostic
    /// output showing region mapping, portal chains, and BFS traversal.
    pub fn trace_sector_visibility(
        sector_a: usize,
        sector_b: usize,
        subsectors: &[SubSector],
        segments: &[Segment],
        bsp: &BSP3D,
        sectors: &[Sector],
        linedefs: &[LineDef],
        nodes: &[Node],
        start_node: u32,
    ) {
        let subsector_count = subsectors.len();

        let rb = build_regions(
            subsectors, segments, bsp, sectors, linedefs, nodes, start_node,
        );
        let RegionBuildResult {
            polygons,
            regions,
            ss_to_region,
            region_walls,
            region_portals,
            ..
        } = rb;

        // Find subsectors belonging to each sector
        let ss_a: Vec<usize> = (0..subsector_count)
            .filter(|&i| subsectors[i].sector.num as usize == sector_a)
            .collect();
        let ss_b: Vec<usize> = (0..subsector_count)
            .filter(|&i| subsectors[i].sector.num as usize == sector_b)
            .collect();

        println!("=== Trace: sector {} → sector {} ===", sector_a, sector_b);
        println!();

        // Sector info
        let print_sector = |id: usize| {
            if id < sectors.len() {
                let s = &sectors[id];
                println!(
                    "Sector {}: floor={}, ceil={}, light={}, special={}",
                    id, s.floorheight, s.ceilingheight, s.lightlevel, s.special
                );
            }
        };
        print_sector(sector_a);
        print_sector(sector_b);
        println!();

        // Subsector → region mapping
        let regions_a: HashSet<usize> = ss_a.iter().map(|&ss| ss_to_region[ss]).collect();
        let regions_b: HashSet<usize> = ss_b.iter().map(|&ss| ss_to_region[ss]).collect();

        println!("Sector {} subsectors: {:?}", sector_a, ss_a);
        println!("  → regions: {:?}", regions_a);
        println!("Sector {} subsectors: {:?}", sector_b, ss_b);
        println!("  → regions: {:?}", regions_b);
        println!();

        // For each source region in sector_a, trace BFS toward sector_b
        println!("--- Region portal graph (relevant entries) ---");
        let all_relevant: HashSet<usize> = regions_a.union(&regions_b).copied().collect();
        for &r in &all_relevant {
            let portals = &region_portals[r];
            if portals.is_empty() {
                println!(
                    "Region {} ({} subsectors, {} walls): NO PORTALS",
                    r,
                    regions[r].len(),
                    region_walls[r].len()
                );
            } else {
                let neighbors: Vec<usize> = portals.iter().map(|p| p.neighbor).collect();
                println!(
                    "Region {} ({} subsectors, {} walls): portals to {:?}",
                    r,
                    regions[r].len(),
                    region_walls[r].len(),
                    neighbors
                );
                for p in portals {
                    println!(
                        "  → region {}: {} segs, floor={:.0}, ceil={:.0}",
                        p.neighbor,
                        p.segs.len(),
                        p.floor,
                        p.ceil
                    );
                    for (i, seg) in p.segs.iter().enumerate() {
                        println!(
                            "    seg[{}]: ({:.1},{:.1})→({:.1},{:.1})",
                            i, seg.0.x, seg.0.y, seg.1.x, seg.1.y
                        );
                    }
                }
            }
        }
        println!();

        // Run detailed BFS trace from each region_a to see if any region_b is reached
        println!("--- BFS visibility trace ---");
        for &src_region in &regions_a {
            println!("\nSource region {} (sector {}):", src_region, sector_a);

            let visible = trace_bfs_detailed(
                src_region,
                &regions_b,
                &region_portals,
                &region_walls,
                &regions,
                &polygons,
            );

            let reached: Vec<usize> = visible.intersection(&regions_b).copied().collect();
            if reached.is_empty() {
                println!(
                    "  RESULT: sector {} NOT visible from region {}",
                    sector_b, src_region
                );
            } else {
                println!(
                    "  RESULT: sector {} VISIBLE via regions {:?}",
                    sector_b, reached
                );
            }
        }

        println!();
        println!("=== End trace ===");
    }
}

/// BFS with detailed per-step logging for trace diagnostics.
fn trace_bfs_detailed(
    source: usize,
    target_regions: &HashSet<usize>,
    region_portals: &[Vec<Portal>],
    region_walls: &[Vec<(Vec2, Vec2)>],
    regions: &[Vec<usize>],
    polygons: &[Vec<Vec2>],
) -> HashSet<usize> {
    let mut visible = HashSet::new();
    visible.insert(source);

    for (pi, portal) in region_portals[source].iter().enumerate() {
        let neighbor = portal.neighbor;
        visible.insert(neighbor);
        println!("  Direct neighbor via portal[{}]: region {}", pi, neighbor);

        let src_segs = portal.segs.clone();
        let sp = portal_pts(&src_segs);
        if sp.len() < 2 {
            println!("    (source portal <2 unique pts, skipping BFS chain)");
            continue;
        }

        let mut queue: VecDeque<BfsEntry> = VecDeque::new();
        let mut visit_count: HashMap<usize, u32> = HashMap::new();
        *visit_count.entry(source).or_default() += 1;
        *visit_count.entry(neighbor).or_default() += 1;

        queue.push_back(BfsEntry {
            region: neighbor,
            src_segs,
            src_floor: portal.floor,
            src_ceil: portal.ceil,
            pass_segs: portal.segs.clone(),
            pass_floor: portal.floor,
            pass_ceil: portal.ceil,
            depth: 1,
        });

        while let Some(entry) = queue.pop_front() {
            if entry.depth > MAX_BFS_DEPTH {
                continue;
            }

            let cur_sp = portal_pts(&entry.src_segs);
            if cur_sp.len() < 2 {
                continue;
            }
            let pp = portal_pts(&entry.pass_segs);
            if pp.len() < 2 {
                continue;
            }

            let sp_center = centroid_of_points(&cur_sp);
            let pp_center = centroid_of_points(&pp);
            let mut fwd = pp_center - sp_center;
            let mut fwd_len = fwd.length();

            if fwd_len < 1.0 {
                let rc = region_centroid(&regions[entry.region], polygons);
                fwd = rc - sp_center;
                fwd_len = fwd.length();
                if fwd_len < 1.0 {
                    continue;
                }
            }

            let fwd_dx = fwd.x;
            let fwd_dy = fwd.y;
            let fwd_norm = fwd / fwd_len;

            // Only log details for portals leading toward target regions
            let is_near_target = target_regions.iter().any(|&tr| {
                region_portals[entry.region]
                    .iter()
                    .any(|p| p.neighbor == tr)
                    || entry.region == tr
            });

            for target_portal in &region_portals[entry.region] {
                let target_region = target_portal.neighbor;
                let count = visit_count.get(&target_region).copied().unwrap_or(0);
                if count >= MAX_REGION_VISITS {
                    continue;
                }

                let log = is_near_target || target_regions.contains(&target_region);

                // Step 1: Frustum clip
                let clipped_target = clip_target_segs(&target_portal.segs, &cur_sp, &pp, fwd_norm);
                if clipped_target.is_empty() {
                    if log {
                        println!(
                            "    depth={} region {}→{}: BLOCKED by frustum clip (target fully clipped)",
                            entry.depth, entry.region, target_region
                        );
                    }
                    continue;
                }

                // Step 2: Wall shadows
                let mut clipped = clipped_target;
                let mut wall_blocked = false;
                for (wi, wall) in region_walls[entry.region].iter().enumerate() {
                    if clipped.is_empty() {
                        if log {
                            println!(
                                "    depth={} region {}→{}: BLOCKED by wall shadow (wall[{}] ({:.1},{:.1})→({:.1},{:.1}))",
                                entry.depth,
                                entry.region,
                                target_region,
                                wi,
                                wall.0.x,
                                wall.0.y,
                                wall.1.x,
                                wall.1.y
                            );
                        }
                        wall_blocked = true;
                        break;
                    }
                    clipped = subtract_wall_shadow(&clipped, wall, &cur_sp, fwd_dx, fwd_dy);
                }
                if wall_blocked || clipped.is_empty() {
                    if log && !wall_blocked {
                        println!(
                            "    depth={} region {}→{}: BLOCKED by wall shadows (all segs removed)",
                            entry.depth, entry.region, target_region
                        );
                    }
                    continue;
                }

                let ct_pts = portal_pts(&clipped);
                if ct_pts.len() < 2 {
                    if log {
                        println!(
                            "    depth={} region {}→{}: BLOCKED (clipped target <2 unique pts)",
                            entry.depth, entry.region, target_region
                        );
                    }
                    continue;
                }

                // Step 3: Mutual refinement
                let rev_fwd = -fwd_norm;
                let clipped_source = clip_target_segs(&entry.src_segs, &ct_pts, &pp, rev_fwd);
                let final_src =
                    if !clipped_source.is_empty() && portal_pts(&clipped_source).len() >= 2 {
                        clipped_source
                    } else {
                        entry.src_segs.clone()
                    };

                // Step 4: Height clip
                let tgt_fl = target_portal.floor;
                let tgt_cl = target_portal.ceil;
                let d_sp = ((pp_center.x - sp_center.x).powi(2)
                    + (pp_center.y - sp_center.y).powi(2))
                .sqrt();
                let tc = centroid_of_points(&ct_pts);
                let d_st = ((tc.x - sp_center.x).powi(2) + (tc.y - sp_center.y).powi(2)).sqrt();

                let (vis_fl, vis_cl) = if d_sp > 1.0 {
                    let ratio = d_st / d_sp;
                    let mut frust_cl =
                        entry.src_floor + (entry.pass_ceil - entry.src_floor) * ratio;
                    let mut frust_fl = entry.src_ceil + (entry.pass_floor - entry.src_ceil) * ratio;
                    if frust_fl > frust_cl {
                        std::mem::swap(&mut frust_fl, &mut frust_cl);
                    }
                    let vf = tgt_fl.max(frust_fl);
                    let vc = tgt_cl.min(frust_cl);
                    if vc <= vf {
                        if log {
                            println!(
                                "    depth={} region {}→{}: BLOCKED by height clip (vis_floor={:.1} >= vis_ceil={:.1}, frust=[{:.1},{:.1}], tgt=[{:.1},{:.1}])",
                                entry.depth,
                                entry.region,
                                target_region,
                                vf,
                                vc,
                                frust_fl,
                                frust_cl,
                                tgt_fl,
                                tgt_cl
                            );
                        }
                        continue;
                    }
                    (vf, vc)
                } else {
                    (tgt_fl, tgt_cl)
                };

                if log {
                    println!(
                        "    depth={} region {}→{}: VISIBLE (height=[{:.0},{:.0}], {} clipped segs)",
                        entry.depth,
                        entry.region,
                        target_region,
                        vis_fl,
                        vis_cl,
                        clipped.len()
                    );
                }

                visible.insert(target_region);
                *visit_count.entry(target_region).or_default() += 1;

                queue.push_back(BfsEntry {
                    region: target_region,
                    src_segs: final_src,
                    src_floor: entry.src_floor,
                    src_ceil: entry.src_ceil,
                    pass_segs: clipped,
                    pass_floor: vis_fl,
                    pass_ceil: vis_cl,
                    depth: entry.depth + 1,
                });
            }
        }
    }

    visible
}
