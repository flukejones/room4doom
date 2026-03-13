//! Conservative mightsee bitsets for the 2D BSP portal PVS system.
//!
//! Provides [`Mightsee`], which pre-computes two families of bitsets used to
//! bound the full frustum-clip traversal in Pass 2:
//!
//! - **Source bitsets** (`source[i]`): angular-span BFS from subsector `i`,
//!   then OR-expanded with the angular-span results of every directly adjacent
//!   subsector. The expansion corrects false negatives from curved/ring
//!   geometry where the fixed ref_pt and BFS hop order disagree angularly.
//! - **Portal-direction bitsets** (`portal_dir[pi * 2 + side]`): plain BFS from
//!   one side of portal `pi` with the portal itself excluded, giving the
//!   forward-only reachable set used for early termination.

use super::angular_buckets::{AngularBuckets, arc_to_mask};
use super::portal::{Portal, Portals};
use super::traits::MightSee;
use super::{intersect_arcs, portal_angular_span, widen_arc};
use rayon::prelude::*;
use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};

// ============================================================================
// CONSTANTS
// ============================================================================

/// Angular span narrowing epsilon — approximately 2°.
///
/// Applied via [`widen_arc`] before [`intersect_arcs`] to avoid discarding
/// portals that are just barely on the edge of the current arc.
const ARC_EPS: f32 = 0.0349;

// ============================================================================
// MIGHTSEE
// ============================================================================

/// Per-subsector and per-portal-direction conservative visibility bitsets.
///
/// Built once before Pass 2 and then queried during the full frustum-clip
/// flood to prune unreachable subsectors and enable early termination.
pub struct Mightsee {
    subsector_count: usize,
    /// Per-source angular-span BFS bitsets, expanded with direct-neighbour OR:
    /// `source[i]` is the mightsee row for subsector `i`.
    source: Vec<Vec<u32>>,
    /// Per-portal directional BFS bitsets: `portal_dir[pi * 2 + side]` is the
    /// forward-only mightsee for portal `pi` entered from `side`
    /// (0 = from `subsector_b`, 1 = from `subsector_a`).
    portal_dir: Vec<Vec<u32>>,
}

impl Default for Mightsee {
    fn default() -> Self {
        Self {
            subsector_count: 0,
            source: Vec::new(),
            portal_dir: Vec::new(),
        }
    }
}

impl Mightsee {
    /// Build both bitset families from the portal graph.
    ///
    /// # Pass 1 — angular-span source mightsee
    /// Runs one angular-span BFS per subsector in parallel (rayon).
    ///
    /// # Pass 1a — neighbour OR expansion
    /// For each subsector S, ORs its angular-span row with the angular-span
    /// rows of every directly adjacent subsector. If S sees N directly, S's
    /// mightsee should include everything N's mightsee covers. This corrects
    /// false negatives in curved/ring geometry (e.g. E5M1 concentric rings)
    /// where the fixed ref_pt and BFS hop order disagree angularly.
    ///
    /// # Pass 1b — directional portal mightsee
    /// Runs two plain BFS per portal (one per side) in parallel, each
    /// excluding the entry portal to get a forward-only reachable set.
    /// Progress is printed to stderr.
    pub fn build(portals: &Portals) -> Self {
        let n = portals.subsector_count();
        let portals_slice = portals.portals_slice();
        let portal_offsets = portals.offsets();
        let portal_ids = portals.ids();

        // Pass 1: angular-span mightsee per subsector.
        let progress = AtomicUsize::new(0);
        let source_raw: Vec<Vec<u32>> = (0..n)
            .into_par_iter()
            .map(|s| {
                let done = progress.fetch_add(1, Ordering::Relaxed) + 1;
                if done % 64 == 0 || done == n {
                    eprint!(
                        "\r  PVS2D angular:  {done}/{n} ({:.0}%)   ",
                        done as f32 / n as f32 * 100.0
                    );
                    let _ = std::io::stderr().flush();
                }
                angular_span_mightsee(s, portals_slice, portal_offsets, portal_ids, n)
            })
            .collect();
        eprintln!();

        // Pass 1a: expand each source row by OR-ing direct neighbours' rows.
        // Reads source_raw (immutable) and writes into a new allocation so
        // parallel iteration is race-free.
        let progress = AtomicUsize::new(0);
        let source: Vec<Vec<u32>> = (0..n)
            .into_par_iter()
            .map(|s| {
                let done = progress.fetch_add(1, Ordering::Relaxed) + 1;
                if done % 64 == 0 || done == n {
                    eprint!(
                        "\r  PVS2D expand:   {done}/{n} ({:.0}%)   ",
                        done as f32 / n as f32 * 100.0
                    );
                    let _ = std::io::stderr().flush();
                }
                let mut bits = source_raw[s].clone();
                if portal_offsets.is_empty() {
                    return bits;
                }
                let src_start = portal_offsets[s] as usize;
                let src_end = portal_offsets[s + 1] as usize;
                for &pi in &portal_ids[src_start..src_end] {
                    let neighbour = portals_slice[pi as usize].other(s);
                    for (r, &d) in bits.iter_mut().zip(source_raw[neighbour].iter()) {
                        *r |= d;
                    }
                }
                bits
            })
            .collect();
        eprintln!();

        // Pass 1b: directional per-portal mightsee (2 BFS per portal).
        //   portal_dir[pi * 2 + 0] = BFS from pi.subsector_b excluding pi
        //   portal_dir[pi * 2 + 1] = BFS from pi.subsector_a excluding pi
        // Excluding the entry portal disconnects corridor graphs at that
        // portal, giving a tight forward-only bound for early termination in
        // full_clip_flood.
        let num_portals = portals.len();
        let portal_dir: Vec<Vec<u32>> = {
            let progress = AtomicUsize::new(0);
            let total = num_portals * 2;
            let result: Vec<Vec<u32>> = (0..total)
                .into_par_iter()
                .map(|i| {
                    let done = progress.fetch_add(1, Ordering::Relaxed) + 1;
                    if done % 128 == 0 || done == total {
                        eprint!(
                            "\r  PVS2D mightsee: {done}/{total} ({:.0}%)   ",
                            done as f32 / total as f32 * 100.0
                        );
                        let _ = std::io::stderr().flush();
                    }
                    let pi = i / 2;
                    let from_sub = if i % 2 == 0 {
                        portals_slice[pi].subsector_b
                    } else {
                        portals_slice[pi].subsector_a
                    };
                    coarse_pvs_excluding(from_sub, pi, portals_slice, portal_offsets, portal_ids, n)
                })
                .collect();
            eprintln!();
            result
        };

        Self {
            subsector_count: n,
            source,
            portal_dir,
        }
    }
}

impl MightSee for Mightsee {
    /// Total number of subsectors covered by this mightsee structure.
    fn subsector_count(&self) -> usize {
        self.subsector_count
    }

    /// Bitset row for the conservative set of subsectors visible from `source`.
    fn source_bits(&self, source: usize) -> &[u32] {
        &self.source[source]
    }

    /// Bitset row for the conservative set of subsectors visible through portal
    /// `pi` in direction `side` (0 = from `subsector_b`, 1 = from
    /// `subsector_a`).
    fn portal_dir_bits(&self, pi: usize, side: usize) -> &[u32] {
        &self.portal_dir[pi * 2 + side]
    }
}

// ============================================================================
// PRIVATE HELPERS
// ============================================================================

/// Angular-span narrowing BFS mightsee from `source`.
///
/// Returns a bitset (`(n + 31) / 32` words) where bit `i` is set if subsector
/// `i` might be visible from `source`. Narrower than a plain flood: the arc of
/// angles is tightened at each portal hop, pruning geometrically implausible
/// subsectors.
///
/// Runs one independent BFS per initial portal from `source`. Each BFS uses
/// its own per-subsector angular bucket mask and the midpoint of the entry
/// portal as `ref_pt`. Results are OR-ed together.
///
/// Per-portal independence prevents cross-portal contamination where a
/// narrow-arc visit from one ref_pt blocks a wider-arc visit from another.
///
/// Within each BFS, the binary visited check is replaced with a 32-bucket
/// angular mask per subsector. A far subsector is re-pushed whenever the new
/// arc covers at least one bucket not yet explored, allowing multiple
/// non-overlapping angular paths to the same subsector. This eliminates false
/// negatives in large open maps (e.g. Sunder MAP03) where the same subsector
/// is reachable from geometrically distinct angular windows.
fn angular_span_mightsee(
    source: usize,
    portals: &[Portal],
    portal_offsets: &[u32],
    portal_ids: &[u32],
    n: usize,
) -> Vec<u32> {
    let row_words = (n + 31) / 32;
    let mut result = vec![0u32; row_words];
    if portal_offsets.is_empty() || source >= n {
        return result;
    }
    result[source / 32] |= 1u32 << (source % 32);

    let src_start = portal_offsets[source] as usize;
    let src_end = portal_offsets[source + 1] as usize;

    for &pi in &portal_ids[src_start..src_end] {
        let pi = pi as usize;
        let p = &portals[pi];
        let to_ss = p.other(source);
        let ref_pt = (p.v1 + p.v2) * 0.5;

        // Per-subsector angular bucket masks: `seen_buckets[ss]` records which
        // 32 angular buckets (each ~11.25°) have already been explored from
        // `ss`. A subsector is only re-pushed when the new arc covers fresh
        // (unexplored) buckets — bounding stack depth at O(32 × n).
        let mut seen_buckets = AngularBuckets::new(n);
        seen_buckets.mark_full(source);
        seen_buckets.mark_full(to_ss);

        // Separate result bitset: tracks which subsectors were reached at all.
        let mut bits = vec![0u32; row_words];
        bits[source / 32] |= 1u32 << (source % 32);
        bits[to_ss / 32] |= 1u32 << (to_ss % 32);

        // Start with the full circle so all of to_ss's exit portals are
        // reachable from this hop. Subsequent hops narrow the arc correctly.
        //
        // A pre-computed initial_arc from min/max of exit-portal angles fails
        // when portals span more than π: the wrapping check inverts the arc to
        // its narrow complement, silently blocking wide-open areas in maps
        // like Sunder MAP03.
        let initial_arc = (-std::f32::consts::PI, std::f32::consts::PI);
        let mut stack: Vec<(usize, usize, (f32, f32))> = vec![(to_ss, pi, initial_arc)];

        while let Some((ss, entry_pi, arc)) = stack.pop() {
            let start = portal_offsets[ss] as usize;
            let end = portal_offsets[ss + 1] as usize;
            for &qi in &portal_ids[start..end] {
                let qi = qi as usize;
                if qi == entry_pi {
                    continue;
                }
                let q = &portals[qi];
                let far_ss = q.other(ss);
                let exit_arc = widen_arc(portal_angular_span(q.v1, q.v2, ref_pt), ARC_EPS);
                if let Some(narrowed) = intersect_arcs(arc, exit_arc) {
                    let mask = arc_to_mask(narrowed);
                    if seen_buckets.test_and_update(far_ss, mask) {
                        bits[far_ss / 32] |= 1u32 << (far_ss % 32);
                        stack.push((far_ss, qi, narrowed));
                    }
                }
            }
        }

        // Merge this initial portal's findings into the combined result.
        for (r, &b) in result.iter_mut().zip(bits.iter()) {
            *r |= b;
        }
    }

    result
}

/// Plain BFS flood from `source` across the portal graph, skipping
/// `excluded_pi`.
///
/// Used to compute *directional* per-portal mightsee: by excluding the entry
/// portal the BFS only discovers subsectors reachable going forward (not back
/// through the entry). For corridor maps this disconnects the graph at the
/// excluded portal, giving a tight bound that makes the mightsee prune and
/// early termination in full_clip_flood actually useful.
fn coarse_pvs_excluding(
    source: usize,
    excluded_pi: usize,
    portals: &[Portal],
    portal_offsets: &[u32],
    portal_ids: &[u32],
    n: usize,
) -> Vec<u32> {
    let row_words = (n + 31) / 32;
    let mut bits = vec![0u32; row_words];

    let set_bit = |bits: &mut Vec<u32>, idx: usize| bits[idx / 32] |= 1u32 << (idx % 32);
    let test_bit =
        |bits: &Vec<u32>, idx: usize| -> bool { bits[idx / 32] & (1u32 << (idx % 32)) != 0 };

    set_bit(&mut bits, source);

    let src_start = portal_offsets[source] as usize;
    let src_end = portal_offsets[source + 1] as usize;
    let mut stack: Vec<usize> = portal_ids[src_start..src_end]
        .iter()
        .filter(|&&pi| pi as usize != excluded_pi)
        .map(|&pi| portals[pi as usize].other(source))
        .collect();

    while let Some(ss) = stack.pop() {
        if test_bit(&bits, ss) {
            continue;
        }
        set_bit(&mut bits, ss);
        let start = portal_offsets[ss] as usize;
        let end = portal_offsets[ss + 1] as usize;
        for &pi in &portal_ids[start..end] {
            if pi as usize == excluded_pi {
                continue;
            }
            let next = portals[pi as usize].other(ss);
            if !test_bit(&bits, next) {
                stack.push(next);
            }
        }
    }

    bits
}
