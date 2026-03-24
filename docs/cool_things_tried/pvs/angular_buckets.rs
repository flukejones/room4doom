//! Angular bucket tracking for PVS traversal deduplication.
//!
//! Provides [`AngularBuckets`], a per-subsector 192-bucket angular coverage
//! tracker used to bound repeated exploration of the same subsector in both
//! the mightsee BFS ([`super::mightsee`]) and the full frustum-clip flood
//! ([`super::pvs2d`]).
//!
//! Also provides [`arc_to_mask`] and [`portal_to_mask`] for converting
//! angular arcs or portal endpoints to bucket bitmasks.

use glam::Vec2;

use super::portal_angular_span;

// ============================================================================
// CONSTANTS
// ============================================================================

/// Number of `u32` words per mask. Each word holds 32 buckets.
const WORDS: usize = 6;

/// Total number of angular buckets (`WORDS × 32 = 192`, ~1.875°/bucket).
const BUCKETS: usize = WORDS * 32;

// ============================================================================
// MASK TYPE
// ============================================================================

/// Bitmask covering [`BUCKETS`] angular buckets stored as `[u32; WORDS]`.
pub type Mask = [u32; WORDS];

/// All-zeros mask (no buckets set).
const MASK_EMPTY: Mask = [0u32; WORDS];

/// All-ones mask (all buckets set).
const MASK_FULL: Mask = [u32::MAX; WORDS];

// ============================================================================
// ANGULAR BUCKETS
// ============================================================================

/// Per-subsector 192-bucket angular coverage tracker.
///
/// Each bucket covers approximately `2π / 192 ≈ 1.875°` of the full circle
/// in the `[-π, π]` range. A subsector is only re-explored when the new arc
/// covers at least one bucket not yet marked, bounding stack/recursion depth
/// at `O(192 × n)` per BFS/DFS instead of O(n²).
pub struct AngularBuckets {
    /// `data[ss]` = bitmask of explored buckets for subsector `ss`.
    data: Vec<Mask>,
}

impl AngularBuckets {
    /// Allocate a fresh tracker for `n` subsectors (all buckets unexplored).
    pub fn new(n: usize) -> Self {
        Self {
            data: vec![MASK_EMPTY; n],
        }
    }

    /// Mark subsector `ss` as fully explored (all 192 buckets set).
    pub fn mark_full(&mut self, ss: usize) {
        self.data[ss] = MASK_FULL;
    }

    /// If `mask` covers any unexplored buckets for `ss`, mark them and return
    /// `true`. Returns `false` if all of `mask`'s buckets were already seen.
    pub fn test_and_update(&mut self, ss: usize, mask: Mask) -> bool {
        let slot = &mut self.data[ss];
        let mut fresh = false;
        for (d, &m) in slot.iter_mut().zip(mask.iter()) {
            let f = m & !*d;
            if f != 0 {
                *d |= m;
                fresh = true;
            }
        }
        fresh
    }
}

// ============================================================================
// MASK FUNCTIONS
// ============================================================================

/// Convert an angular arc `(lo, hi)` in `[-π, π]` to a 192-bucket [`Mask`].
///
/// A wrapping arc (`lo > hi`) covers `[lo..π] ∪ [-π..hi]`.
pub fn arc_to_mask(arc: (f32, f32)) -> Mask {
    const PI: f32 = std::f32::consts::PI;
    if arc.0 > arc.1 {
        // Wrapping: [lo..π] ∪ [-π..hi]
        or_masks(mask_non_wrapping(arc.0, PI), mask_non_wrapping(-PI, arc.1))
    } else {
        mask_non_wrapping(arc.0, arc.1)
    }
}

/// Compute a bucket mask for the angular span of portal segment `(v1, v2)`
/// as seen from `ref_pt`. Equivalent to `arc_to_mask(portal_angular_span(v1,
/// v2, ref_pt))`.
pub fn portal_to_mask(ref_pt: Vec2, v1: Vec2, v2: Vec2) -> Mask {
    arc_to_mask(portal_angular_span(v1, v2, ref_pt))
}

/// Bitwise OR of two masks.
fn or_masks(a: Mask, b: Mask) -> Mask {
    let mut r = MASK_EMPTY;
    for i in 0..WORDS {
        r[i] = a[i] | b[i];
    }
    r
}

/// Bucket mask for a non-wrapping arc `[lo, hi]` (`lo ≤ hi`) in `[-π, π]`.
///
/// Bucket `i` covers `[-π + i · 2π/192, -π + (i+1) · 2π/192)`.
fn mask_non_wrapping(lo: f32, hi: f32) -> Mask {
    const PI: f32 = std::f32::consts::PI;
    let scale = BUCKETS as f32 / (2.0 * PI);
    let lo_b = (((lo + PI) * scale).floor() as isize).clamp(0, BUCKETS as isize - 1) as usize;
    let hi_b = (((hi + PI) * scale).floor() as isize).clamp(0, BUCKETS as isize - 1) as usize;

    let mut result = MASK_EMPTY;
    let lo_word = lo_b / 32;
    let hi_word = hi_b / 32;
    for w in lo_word..=hi_word {
        let bit_lo = if w == lo_word { lo_b % 32 } else { 0 };
        let bit_hi = if w == hi_word { hi_b % 32 } else { 31 };
        let count = bit_hi - bit_lo + 1;
        result[w] = if count == 32 {
            u32::MAX
        } else {
            ((1u32 << count) - 1) << bit_lo
        };
    }
    result
}
