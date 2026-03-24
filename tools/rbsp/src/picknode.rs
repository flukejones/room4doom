//! Partition selection, side classification, and convexity testing.

use std::collections::HashMap;
use std::collections::hash_map::Entry;

use crate::superblock::{SuperBlock, box_on_line_side};
use crate::types::*;

/// Convexity tolerance. Generous (~2 map units) because map geometry is authored
/// at integer coordinates and adjacent walls from different linedefs may not
/// align precisely. Matches BSP 5.2's 2-unit snap in DoLinesIntersect.
const CONVEX_EPSILON: Float = 2.0;

/// Classify a point relative to a partition line.
/// Uses the original linedef vertex as origin to avoid float drift.
pub fn classify_point(partition: &Seg, point: &Vertex, vertices: &[Vertex]) -> PointSide {
    let px = vertices[partition.linedef_v1].x as f64;
    let py = vertices[partition.linedef_v1].y as f64;
    let cross = (partition.dx as f64) * (point.y as f64 - py)
        - (partition.dy as f64) * (point.x as f64 - px);
    let dist = cross / partition.dir_len as f64;

    if dist.abs() < EPSILON as f64 {
        PointSide::OnLine
    } else if dist > 0.0 {
        PointSide::Left
    } else {
        PointSide::Right
    }
}

/// Classify a seg relative to a partition line.
pub fn classify_seg(partition: &Seg, seg: &Seg, vertices: &[Vertex]) -> SegSide {
    let start_side = classify_point(partition, &vertices[seg.start], vertices);
    let end_side = classify_point(partition, &vertices[seg.end], vertices);

    match (start_side, end_side) {
        (PointSide::Left, PointSide::Left) => SegSide::Left,
        (PointSide::Right, PointSide::Right) => SegSide::Right,
        (PointSide::Left, PointSide::Right) | (PointSide::Right, PointSide::Left) => SegSide::Split,
        (PointSide::OnLine, PointSide::Left) | (PointSide::Left, PointSide::OnLine) => {
            SegSide::Left
        }
        (PointSide::OnLine, PointSide::Right) | (PointSide::Right, PointSide::OnLine) => {
            SegSide::Right
        }
        (PointSide::OnLine, PointSide::OnLine) => {
            let dot = partition.dx * seg.dx + partition.dy * seg.dy;
            if dot >= 0.0 {
                SegSide::Right
            } else {
                SegSide::Left
            }
        }
    }
}

/// Quantize a seg's infinite line into a hash key for fast collinear grouping.
/// Opposite directions (same line) produce the same key.
fn plane_key(seg: &Seg, vertices: &[Vertex]) -> (i64, i64) {
    if seg.len < EPSILON {
        return (i64::MAX, i64::MAX);
    }
    // Unit normal of the line (perpendicular to seg direction)
    let mut nx = seg.dy / seg.len;
    let mut ny = -seg.dx / seg.len;
    // Canonicalize: ensure normal points in a consistent half-plane
    // (positive nx, or if nx==0 then positive ny)
    if nx < -EPSILON || (nx.abs() < EPSILON && ny < 0.0) {
        nx = -nx;
        ny = -ny;
    }
    // Perpendicular distance from origin to the line
    let dist = vertices[seg.start].x * nx + vertices[seg.start].y * ny;
    // Quantize at VERTEX_EPSILON resolution
    let scale = 1.0 / VERTEX_EPSILON;
    (
        (nx * 1e6).round() as i64 * 1_000_000 + (ny * 1e6).round() as i64,
        (dist * scale).round() as i64,
    )
}

/// Group collinear segs into planes. Returns representative seg indices
/// (one per collinear group).
fn group_by_plane(seg_indices: &[usize], segs: &[Seg], vertices: &[Vertex]) -> Vec<usize> {
    let mut groups: HashMap<(i64, i64), usize> = HashMap::with_capacity(seg_indices.len() / 2);
    let mut representatives = Vec::with_capacity(seg_indices.len() / 2);

    for &seg_idx in seg_indices {
        let key = plane_key(&segs[seg_idx], vertices);
        if let Entry::Vacant(e) = groups.entry(key) {
            e.insert(seg_idx);
            representatives.push(seg_idx);
        }
    }

    representatives
}

struct ScoreState {
    left: u32,
    right: u32,
    splits: u32,
    weight: Float,
    axis_penalty: Float,
    best_score: Float,
}

impl ScoreState {
    /// True if we've already exceeded the best known score (early exit).
    fn exceeded(&self) -> bool {
        let split_cost = self.splits as Float * 100.0 * self.weight;
        -(split_cost + self.axis_penalty) < self.best_score
    }

    fn final_score(&self) -> Float {
        if self.left == 0 || self.right == 0 {
            return Float::NEG_INFINITY;
        }
        let split_cost = self.splits as Float * 100.0 * self.weight;
        let balance_cost = (self.left as Float - self.right as Float).abs() * 100.0;
        -(split_cost + balance_cost + self.axis_penalty)
    }
}

/// Walk the superblock tree to score a partition candidate.
/// Returns true if early-exit triggered (score already worse than best).
fn score_superblock(
    block: &SuperBlock,
    partition: &Seg,
    segs: &[Seg],
    vertices: &[Vertex],
    state: &mut ScoreState,
) -> bool {
    // Bulk-classify the entire block bbox against the partition line.
    let side = box_on_line_side(block, partition, vertices);

    if side < 0 {
        state.left += block.count;
        return false;
    }
    if side > 0 {
        state.right += block.count;
        return false;
    }

    // Block straddles — classify individual segs at this level.
    for &seg_idx in &block.seg_indices {
        match classify_seg(partition, &segs[seg_idx], vertices) {
            SegSide::Left => state.left += 1,
            SegSide::Right => state.right += 1,
            SegSide::Split => {
                state.splits += 1;
                state.left += 1;
                state.right += 1;
            }
        }
        if state.exceeded() {
            return true;
        }
    }

    // Recurse into children.
    for child in &block.children {
        if let Some(c) = child {
            if score_superblock(c, partition, segs, vertices, state) {
                return true;
            }
        }
    }

    false
}

/// Score a partition candidate using the superblock tree.
fn score_partition(
    partition: &Seg,
    block: &SuperBlock,
    segs: &[Seg],
    vertices: &[Vertex],
    effective_weight: Float,
    best_score: Float,
) -> Float {
    let mut state = ScoreState {
        left: 0,
        right: 0,
        splits: 0,
        weight: effective_weight,
        axis_penalty: if partition.dx.abs() < EPSILON || partition.dy.abs() < EPSILON {
            0.0
        } else {
            25.0
        },
        best_score,
    };

    if score_superblock(block, partition, segs, vertices, &mut state) {
        return Float::NEG_INFINITY;
    }

    state.final_score()
}

/// Select the best partition with early-exit scoring.
/// Returns None if no candidate puts segs on both sides.
pub fn select_best_partition(
    seg_indices: &[usize],
    segs: &[Seg],
    vertices: &[Vertex],
    options: &BspOptions,
    block: &SuperBlock,
) -> Option<usize> {
    debug_assert!(!seg_indices.is_empty());

    let representatives = group_by_plane(seg_indices, segs, vertices);
    let w = options.split_weight;

    let mut best_idx = None;
    let mut best_score = Float::NEG_INFINITY;

    for &idx in &representatives {
        let score = score_partition(&segs[idx], block, segs, vertices, w, best_score);
        if score > best_score {
            best_score = score;
            best_idx = Some(idx);
        }
    }

    best_idx
}

/// Return all partition candidates ranked by score (best first).
/// No early exit — scores all candidates fully.
pub fn ranked_partitions(
    seg_indices: &[usize],
    segs: &[Seg],
    vertices: &[Vertex],
    options: &BspOptions,
    block: &SuperBlock,
) -> Vec<usize> {
    debug_assert!(!seg_indices.is_empty());

    let representatives = group_by_plane(seg_indices, segs, vertices);
    let w = options.split_weight;

    let mut scored: Vec<(usize, Float)> = representatives
        .iter()
        .filter_map(|&idx| {
            let score = score_partition(&segs[idx], block, segs, vertices, w, Float::NEG_INFINITY);
            if score > Float::NEG_INFINITY {
                Some((idx, score))
            } else {
                None
            }
        })
        .collect();

    scored.sort_unstable_by(|a, b| b.1.total_cmp(&a.1));
    scored.into_iter().map(|(idx, _)| idx).collect()
}

/// Test if a set of segs is convex (ready to become a subsector leaf).
///
/// Matches glBSP's approach: convexity is purely geometric — no single-sector
/// requirement. Multi-sector subsectors are valid (the engine handles them).
/// Empty or single-seg sets are trivially convex.
pub fn is_convex(seg_indices: &[usize], segs: &[Seg], vertices: &[Vertex]) -> bool {
    if seg_indices.len() <= 1 {
        return true;
    }

    // A linedef with both front and back segs in the set must be split —
    // the polygon would span both sectors with no boundary between them.
    for (i, &idx_a) in seg_indices.iter().enumerate() {
        for &idx_b in &seg_indices[i + 1..] {
            if segs[idx_a].linedef == segs[idx_b].linedef && segs[idx_a].side != segs[idx_b].side {
                return false;
            }
        }
    }



    for &idx_a in seg_indices {
        let seg_a = &segs[idx_a];
        for &idx_b in seg_indices {
            if idx_a == idx_b {
                continue;
            }
            let seg_b = &segs[idx_b];
            // Use raw perpendicular distance, not classify_point (which has a tiny epsilon)
            let px = vertices[seg_a.start].x;
            let py = vertices[seg_a.start].y;
            let cross_s = seg_a.dx * (vertices[seg_b.start].y - py)
                - seg_a.dy * (vertices[seg_b.start].x - px);
            let cross_e =
                seg_a.dx * (vertices[seg_b.end].y - py) - seg_a.dy * (vertices[seg_b.end].x - px);
            let dist_s = cross_s / seg_a.len;
            let dist_e = cross_e / seg_a.len;
            if dist_s > CONVEX_EPSILON || dist_e > CONVEX_EPSILON {
                return false;
            }
        }
    }

    true
}
