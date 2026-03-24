//! Anti-frustum 2D BSP portal PVS builder.
//!
//! [`PVS2D`] constructs a [`RenderPvs`] bitset from the BSP tree using a
//! three-pass approach:
//!
//! 1. Build portals from BSP divline projections ([`Portals::build`]).
//! 2. Build conservative mightsee bitsets ([`Mightsee::build`]).
//! 3. Run a parallel full frustum-clip flood per source subsector and apply a
//!    symmetry pass.

use crate::bsp3d::BSP3D;
use crate::map_defs::{Node, Segment, SubSector};
use glam::Vec2;
use log::info;
use rayon::prelude::*;
use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::angular_buckets::{AngularBuckets, portal_to_mask};
use super::mightsee::Mightsee;
use super::portal::{Portal, Portals};
use super::traits::{MightSee, PvsData, PvsView2D, RenderPvs};
use super::{bits_to_vec, clip_segment, generate_separators, test_bit};

/// Maximum recursion depth for [`full_clip_flood`]. Prevents stack overflow
/// on very large portal graphs (e.g. Sunder MAP03 with 119 k portals).
const MAX_FLOOD_DEPTH: usize = 512;

/// Anti-frustum 2D BSP portal PVS.
///
/// Holds the computed [`RenderPvs`] bitset, the portal adjacency graph
/// ([`Portals`]), and the conservative mightsee bounds ([`Mightsee`]) used
/// during construction.
pub struct PVS2D {
    visibility: RenderPvs,
    portals: Portals,
    mightsee: Mightsee,
}

impl Default for PVS2D {
    fn default() -> Self {
        Self {
            visibility: RenderPvs::default(),
            portals: Portals::default(),
            mightsee: Mightsee::default(),
        }
    }
}

impl PVS2D {
    /// Build the PVS for a map.
    ///
    /// Runs portal collection and mightsee precomputation. When
    /// `mightsee_only` is false (the default), also runs the parallel full
    /// frustum-clip flood and symmetry pass. When true, the mightsee source
    /// rows are used directly as the final visibility bitset — faster but
    /// more conservative (more false positives).
    pub fn build(
        subsectors: &[SubSector],
        segments: &[Segment],
        bsp: &BSP3D,
        nodes: &[Node],
        start_node: u32,
        mightsee_only: bool,
    ) -> Self {
        info!("PVS2D: building for {} subsectors", subsectors.len());

        let portals = Portals::build(start_node, nodes, subsectors, segments, bsp);
        info!("PVS2D: {} portals", portals.len());

        let mightsee = Mightsee::build(&portals);

        let n = subsectors.len();
        let row_words = (n + 31) / 32;

        let flat = if mightsee_only {
            info!("PVS2D: using mightsee as final PVS (frustum pass skipped)");
            (0..n)
                .flat_map(|s| mightsee.source_bits(s).iter().copied())
                .collect()
        } else {
            let portals_slice = portals.portals_slice();
            let portal_offsets = portals.offsets();
            let portal_ids = portals.ids();

            let progress = AtomicUsize::new(0);

            let rows: Vec<Vec<u32>> = (0..n)
                .into_par_iter()
                .map(|source| {
                    let done = progress.fetch_add(1, Ordering::Relaxed) + 1;
                    if done % 64 == 0 || done == n {
                        eprint!(
                            "\r  PVS2D flood:    {done}/{n} ({:.0}%)   ",
                            done as f32 / n as f32 * 100.0
                        );
                        let _ = std::io::stderr().flush();
                    }

                    let mut pvs_row = vec![0u32; row_words];
                    pvs_row[source / 32] |= 1u32 << (source % 32);

                    let mut on_path = vec![0u32; row_words];
                    on_path[source / 32] |= 1u32 << (source % 32);

                    let src_mightsee = mightsee.source_bits(source);

                    let ref_point =
                        source_ref_point(source, portals_slice, portal_offsets, portal_ids);

                    let src_start = portal_offsets[source] as usize;
                    let src_end = portal_offsets[source + 1] as usize;
                    for &pi in &portal_ids[src_start..src_end] {
                        let pi = pi as usize;
                        let far_sub = portals_slice[pi].other(source);
                        pvs_row[far_sub / 32] |= 1u32 << (far_sub % 32);
                        let seg = portals_slice[pi].segment();
                        on_path[far_sub / 32] |= 1u32 << (far_sub % 32);
                        // Independent explored_angles per initial portal: each
                        // initial portal has its own frustum and angular budget.
                        let mut explored_angles = AngularBuckets::new(n);
                        full_clip_flood(
                            seg,
                            seg,
                            far_sub,
                            pi,
                            portals_slice,
                            portal_offsets,
                            portal_ids,
                            src_mightsee,
                            &mightsee,
                            &mut pvs_row,
                            &mut on_path,
                            src_mightsee,
                            ref_point,
                            &mut explored_angles,
                            0,
                        );
                        on_path[far_sub / 32] &= !(1u32 << (far_sub % 32));
                    }

                    pvs_row
                })
                .collect();
            eprintln!();

            let mut flat: Vec<u32> = rows.into_iter().flatten().collect();

            for a in 0..n {
                for b in (a + 1)..n {
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
        };

        let visibility = RenderPvs {
            subsector_count: n,
            data: flat,
        };

        info!(
            "PVS2D: {} portals, {} visible pairs",
            portals.len(),
            visibility.count_visible_pairs()
        );

        Self {
            visibility,
            portals,
            mightsee,
        }
    }

    /// Clone and return the computed [`RenderPvs`] bitset.
    pub fn clone_render_pvs(&self) -> RenderPvs {
        self.visibility.clone()
    }

    /// Serialize and write the PVS to the platform cache directory.
    pub fn save_to_cache(
        &self,
        map_name: &str,
        map_hash: u64,
    ) -> Result<(), super::traits::PvsFileError> {
        use super::traits::PvsFile;
        self.visibility.save_to_cache(map_name, map_hash)
    }

    /// Conservative mightsee: own PVS row unioned with the PVS rows of all
    /// adjacent subsectors (connected via a portal).
    ///
    /// Zero false negatives: own PVS row guarantees every pair directly
    /// computed (or symmetry-added) for `from` is included. Adjacent rows add
    /// a small halo of "might reach" beyond.
    ///
    /// Tighter than plain BFS: bounded by actual visibility sets, not the
    /// whole graph.
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
}

impl PvsView2D for PVS2D {
    fn portals_2d(&self) -> &Portals {
        &self.portals
    }
}

impl MightSee for PVS2D {
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

impl From<PVS2D> for RenderPvs {
    fn from(pvs: PVS2D) -> RenderPvs {
        pvs.visibility
    }
}

/// Compute a reference point for angular measurements: centroid of all portal
/// midpoints adjacent to the source subsector.
fn source_ref_point(
    source: usize,
    portals: &[Portal],
    portal_offsets: &[u32],
    portal_ids: &[u32],
) -> Vec2 {
    let start = portal_offsets[source] as usize;
    let end = portal_offsets[source + 1] as usize;
    if start == end {
        return Vec2::ZERO;
    }
    let mut sum = Vec2::ZERO;
    let count = (end - start) as f32;
    for &pi in &portal_ids[start..end] {
        let seg = portals[pi as usize].segment();
        sum += (seg.0 + seg.1) * 0.5;
    }
    sum / count
}

/// Recursive full frustum clip flood.
///
/// `pvs_row` is a flat bitset for the source subsector's visibility row.
///
/// `on_path[ss]` tracks subsectors on the current DFS path back to source.
/// Backtracking prevents cycles while allowing revisits via independent paths
/// with different frustum angles.
///
/// `explored_angles[ss]` is a u32 bitmask of angular buckets (from
/// `ref_point`) already explored through subsector `ss`. Re-entry is allowed
/// when the new frustum's entry direction covers at least one unexplored
/// bucket. Keyed on `clipped_target`: its angular span tells us from which
/// direction we enter `far_sub`.
///
/// `source_portal` is the ever-narrowing window from the source subsector.
/// `pass_portal` is the most recently traversed portal.
fn full_clip_flood(
    source_portal: (Vec2, Vec2),
    pass_portal: (Vec2, Vec2),
    current_sub: usize,
    pass_portal_idx: usize,
    portals: &[Portal],
    portal_offsets: &[u32],
    portal_ids: &[u32],
    global_mightsee: &[u32],
    mightsee: &impl MightSee,
    pvs_row: &mut Vec<u32>,
    on_path: &mut Vec<u32>,
    parent_might: &[u32],
    ref_point: Vec2,
    explored_angles: &mut AngularBuckets,
    depth: usize,
) {
    if depth >= MAX_FLOOD_DEPTH {
        return;
    }
    let entry_side = if current_sub == portals[pass_portal_idx].subsector_b {
        0
    } else {
        1
    };
    let pass_pm = mightsee.portal_dir_bits(pass_portal_idx, entry_side);

    // Progressive mightsee narrowing (Quake-style): AND parent's narrowed
    // mightsee with the directional mightsee for the portal just traversed,
    // then AND with global mightsee to stay within the source's coarse bound.
    let might: Vec<u32> = {
        let mut m: Vec<u32> = parent_might
            .iter()
            .zip(pass_pm.iter())
            .map(|(&pm, &dm)| pm & dm)
            .collect();
        for (w, &g) in m.iter_mut().zip(global_mightsee.iter()) {
            *w &= g;
        }
        m
    };

    // Early termination: everything reachable through this chain already visible.
    if might.iter().zip(pvs_row.iter()).all(|(&m, &p)| m & !p == 0) {
        return;
    }

    let separators_fwd = generate_separators(source_portal, pass_portal);

    let start = portal_offsets[current_sub] as usize;
    let end = portal_offsets[current_sub + 1] as usize;
    for &pt_idx in &portal_ids[start..end] {
        let pt_idx = pt_idx as usize;
        if pt_idx == pass_portal_idx {
            continue;
        }
        let pt = &portals[pt_idx];
        let far_sub = pt.other(current_sub);

        if !test_bit(&might, far_sub) {
            continue;
        }

        if on_path[far_sub / 32] & (1u32 << (far_sub % 32)) != 0 {
            continue;
        }

        let clipped_target = match clip_segment(pt.segment(), separators_fwd.as_slice()) {
            Some(s) => s,
            None => continue,
        };

        let separators_bwd = generate_separators(clipped_target, pass_portal);

        let clipped_source = match clip_segment(source_portal, separators_bwd.as_slice()) {
            Some(s) => s,
            None => continue,
        };

        let mask = portal_to_mask(ref_point, clipped_target.0, clipped_target.1);
        if !explored_angles.test_and_update(far_sub, mask) {
            pvs_row[far_sub / 32] |= 1u32 << (far_sub % 32);
            continue;
        }

        pvs_row[far_sub / 32] |= 1u32 << (far_sub % 32);
        on_path[far_sub / 32] |= 1u32 << (far_sub % 32);
        full_clip_flood(
            clipped_source,
            clipped_target,
            far_sub,
            pt_idx,
            portals,
            portal_offsets,
            portal_ids,
            global_mightsee,
            mightsee,
            pvs_row,
            on_path,
            &might,
            ref_point,
            explored_angles,
            depth + 1,
        );
        on_path[far_sub / 32] &= !(1u32 << (far_sub % 32));
    }
}
