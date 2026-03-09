//! Potentially Visible Set (PVS) computation and storage for room4doom.
//!
//! This module provides the 2D BSP portal PVS system used to cull rendering.
//! The main entry point is [`PVS2D`], which builds a [`RenderPvs`] bitset from
//! the BSP tree.

pub mod angular_buckets;
pub mod mightsee;
pub mod portal;
pub mod pvs2d;
pub mod pvs_cluster;
pub mod traits;

use glam::Vec2;

/// Small epsilon for degenerate segment detection (map units).
pub(crate) const CLIP_EPSILON: f32 = 0.1;

/// Stack-allocated separator line buffer.
///
/// At most 4 candidates are tested; typically 0 or 2 are valid.
/// Avoids heap allocation in the hot path.
pub struct Separators {
    data: [(Vec2, f32); 4],
    len: usize,
}

impl Separators {
    /// Create an empty separator buffer.
    pub fn new() -> Self {
        Self {
            data: [(Vec2::ZERO, 0.0); 4],
            len: 0,
        }
    }

    /// Push a separator halfplane `(normal, dist)` into the buffer.
    pub fn push(&mut self, sep: (Vec2, f32)) {
        self.data[self.len] = sep;
        self.len += 1;
    }

    /// Return the populated slice of separators.
    pub fn as_slice(&self) -> &[(Vec2, f32)] {
        &self.data[..self.len]
    }
}

/// Test whether bit `idx` is set in a packed `u32` bitset row.
pub(crate) fn test_bit(row: &[u32], idx: usize) -> bool {
    let word = idx / 32;
    let bit = idx % 32;
    if word >= row.len() {
        return false;
    }
    (row[word] & (1u32 << bit)) != 0
}

/// Collect set bits from a bitset into a `Vec<usize>`, clamped to `n`.
pub(crate) fn bits_to_vec(bits: &[u32], n: usize) -> Vec<usize> {
    let mut out = Vec::new();
    for (i, &word) in bits.iter().enumerate() {
        let mut w = word;
        let base = i * 32;
        while w != 0 {
            let bit = w.trailing_zeros() as usize;
            let ss = base + bit;
            if ss < n {
                out.push(ss);
            }
            w &= w - 1;
        }
    }
    out
}

/// Generate separator lines between portal segments A and B.
///
/// A separator is a line through one vertex of A and one vertex of B such that
/// A lies entirely on one side and B entirely on the other.
/// Returns up to 4 oriented lines as `(normal, dist)` on the stack.
pub fn generate_separators(a: (Vec2, Vec2), b: (Vec2, Vec2)) -> Separators {
    let (a1, a2) = a;
    let (b1, b2) = b;
    let a_verts = [a1, a2];
    let b_verts = [b1, b2];
    let mut separators = Separators::new();

    for &va in &a_verts {
        for &vb in &b_verts {
            let dir = vb - va;
            let len = dir.length();
            if len < CLIP_EPSILON {
                continue;
            }
            // Normal perpendicular to the separator line (rotated 90°)
            let normal = Vec2::new(-dir.y, dir.x) / len;
            let dist = normal.dot(va);

            // Classify all four vertices
            let classify = |p: Vec2| -> f32 { normal.dot(p) - dist };

            let ca1 = classify(a1);
            let ca2 = classify(a2);
            let cb1 = classify(b1);
            let cb2 = classify(b2);

            // Determine non-zero sides for A and B. Both vertices of each
            // segment must agree (or be ON the line). If they disagree —
            // the segment straddles the candidate line — reject it.
            let side = |s1: f32, s2: f32| -> f32 {
                let sgn1 = if s1.abs() > CLIP_EPSILON {
                    s1.signum()
                } else {
                    0.0
                };
                let sgn2 = if s2.abs() > CLIP_EPSILON {
                    s2.signum()
                } else {
                    0.0
                };
                if sgn1 != 0.0 && sgn2 != 0.0 && sgn1 != sgn2 {
                    // Segment straddles the separator — not a valid candidate
                    f32::NAN
                } else if sgn1 != 0.0 {
                    sgn1
                } else {
                    sgn2 // may be 0 if both ON
                }
            };
            let a_side = side(ca1, ca2);
            let b_side = side(cb1, cb2);

            if a_side.is_finite()
                && b_side.is_finite()
                && a_side != 0.0
                && b_side != 0.0
                && a_side != b_side
            {
                // Valid separator — orient so the "inside" halfspace (B side) is positive
                if b_side < 0.0 {
                    separators.push((-normal, -dist));
                } else {
                    separators.push((normal, dist));
                }
            }
        }
    }

    separators
}

/// Clip a line segment against a set of oriented halfplanes.
///
/// Each halfplane is `(normal, dist)`: the inside is `normal·p - dist >= 0`.
/// Returns `None` if the segment is fully clipped.
pub fn clip_segment(seg: (Vec2, Vec2), separators: &[(Vec2, f32)]) -> Option<(Vec2, Vec2)> {
    let (mut p1, mut p2) = seg;

    for &(normal, dist) in separators {
        let d1 = normal.dot(p1) - dist;
        let d2 = normal.dot(p2) - dist;

        if d1 >= 0.0 && d2 >= 0.0 {
            // Both inside — keep
            continue;
        }
        if d1 < 0.0 && d2 < 0.0 {
            // Both outside — fully clipped
            return None;
        }
        // Straddles: compute intersection
        let t = d1 / (d1 - d2);
        let intersection = p1 + t * (p2 - p1);
        if d1 >= 0.0 {
            p2 = intersection;
        } else {
            p1 = intersection;
        }
        // Degenerate result
        if (p2 - p1).length() < CLIP_EPSILON {
            return None;
        }
    }

    Some((p1, p2))
}

/// Widen an arc by `eps` radians on each side (conservative pad).
///
/// For both wrapping (`lo > hi`) and non-wrapping arcs the operation is the
/// same: decrease `lo` and increase `hi` to expand coverage. For a wrapping
/// arc `[lo..π] ∪ [-π..hi]` this shrinks the uncovered gap `[hi..lo]`.
/// Clamps to ±π. If widening a wrapping arc causes `lo ≤ hi` (i.e., it now
/// covers the full circle), returns `(-π, π)`.
#[inline]
pub(crate) fn widen_arc(arc: (f32, f32), eps: f32) -> (f32, f32) {
    const PI: f32 = std::f32::consts::PI;
    let lo = (arc.0 - eps).max(-PI);
    let hi = (arc.1 + eps).min(PI);
    if arc.0 > arc.1 && lo <= hi {
        (-PI, PI)
    } else {
        (lo, hi)
    }
}

/// Intersect two angular arcs. Returns `None` if they don't overlap.
///
/// An arc `(lo, hi)` with `lo > hi` wraps through ±π.
pub(crate) fn intersect_arcs(a: (f32, f32), b: (f32, f32)) -> Option<(f32, f32)> {
    const PI: f32 = std::f32::consts::PI;
    let a_wraps = a.0 > a.1;
    let b_wraps = b.0 > b.1;

    match (a_wraps, b_wraps) {
        (false, false) => {
            let lo = a.0.max(b.0);
            let hi = a.1.min(b.1);
            if lo < hi { Some((lo, hi)) } else { None }
        }
        (true, false) => {
            // A wraps: [a.0..π] ∪ [-π..a.1] — any overlap with B is sufficient.
            intersect_arcs((a.0, PI), b).or_else(|| intersect_arcs((-PI, a.1), b))
        }
        (false, true) => intersect_arcs(b, a),
        (true, true) => {
            // Both wrap — their union covers almost the full circle; intersection
            // is [max(lo)..π] ∪ [-π..min(hi)].
            let lo = a.0.max(b.0);
            let hi = a.1.min(b.1);
            Some((lo, hi)) // still a wrapping arc
        }
    }
}

/// Angular span of the segment `(v1, v2)` as seen from `reference`.
///
/// Returns `(lo, hi)` in radians. If `lo > hi` the arc wraps through ±π.
pub(crate) fn portal_angular_span(v1: Vec2, v2: Vec2, reference: Vec2) -> (f32, f32) {
    let a1 = (v1 - reference).to_angle();
    let a2 = (v2 - reference).to_angle();
    let lo = a1.min(a2);
    let hi = a1.max(a2);
    // If the direct arc is wider than π, the shorter arc wraps through ±π.
    if hi - lo > std::f32::consts::PI {
        (hi, lo) // lo > hi signals a wrapping arc
    } else {
        (lo, hi)
    }
}

pub use angular_buckets::AngularBuckets;
pub use mightsee::Mightsee;
pub use portal::{Portal, Portals};
pub use pvs_cluster::PvsCluster;
pub use pvs2d::PVS2D;
pub use traits::{
    MightSee, PvsData, PvsFile, PvsFileError, PvsFileHeader, PvsView2D, RenderPvs, pvs_load_from_cache
};
