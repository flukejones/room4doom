//! Sky filler walls: extend the perimeter walls of sky sectors up to the global
//! max sky ceiling (and down to the global min sky floor) so the sky plane is
//! visually closed where adjacent non-sky sectors are taller/lower.

use crate::bsp3d::input::{Bsp3dInput, NO_REF};

use super::Bsp3dBuilder;
use super::types::{WallEdge, WallType};

impl Bsp3dBuilder {
    /// Create sky filler walls on perimeter walls of sky sectors.
    /// Upper filler extends above sky-ceiling perimeter walls to max_ceil.
    /// Lower filler extends below sky-floor perimeter walls to min_floor.
    pub(super) fn sky_filler_pass(
        &mut self,
        input: &Bsp3dInput,
        sky_max_ceil: &[f32],
        sky_min_floor: &[f32],
    ) {
        for sector_id in 0..input.sectors.len() {
            let sector = &input.sectors[sector_id];
            if !sector.sky_ceil && !sector.sky_floor {
                continue;
            }

            let sky_ceil = sector.ceil_h;
            let sky_floor = sector.floor_h;
            let max_h = sky_max_ceil[sector_id];
            let min_h = sky_min_floor[sector_id];
            let needs_ceil_filler = sector.sky_ceil && max_h > sky_ceil;
            let needs_floor_filler = sector.sky_floor && min_h < sky_floor;

            if !needs_ceil_filler && !needs_floor_filler {
                continue;
            }

            let ss_ids: Vec<usize> = self.sector_subsectors[sector_id].clone();

            for &ss_id in &ss_ids {
                let ss = &input.subsectors[ss_id];
                let start = ss.start_seg as usize;
                let end = start + ss.seg_count as usize;
                for seg in &input.segs[start..end] {
                    // Only perimeter segs: skip interior (same-sector) and
                    // sky-to-sky boundaries.
                    let back =
                        (seg.backsector != NO_REF).then(|| &input.sectors[seg.backsector as usize]);
                    let is_perimeter_ceil = match back {
                        Some(b) => seg.backsector != seg.frontsector && !b.sky_ceil,
                        None => true,
                    };
                    let is_perimeter_floor = match back {
                        Some(b) => seg.backsector != seg.frontsector && !b.sky_floor,
                        None => true,
                    };

                    if needs_ceil_filler && is_perimeter_ceil && sky_ceil < max_h {
                        self.add_wall_quad(
                            input,
                            seg,
                            WallEdge::flat(sky_ceil),
                            WallEdge::flat(max_h),
                            WallType::Upper,
                            sector_id,
                            true,
                            ss_id,
                            None,
                        );
                    }
                    if needs_floor_filler && is_perimeter_floor && min_h < sky_floor {
                        self.add_wall_quad(
                            input,
                            seg,
                            WallEdge::flat(min_h),
                            WallEdge::flat(sky_floor),
                            WallType::Lower,
                            sector_id,
                            true,
                            ss_id,
                            None,
                        );
                    }
                }
            }
        }
    }
}

/// Compute global sky bounds for the level.
/// Returns (sky_max_ceil, sky_min_floor) indexed by sector id.
/// All sky-ceiling sectors get the global max sky ceiling height.
/// All sky-floor sectors get the global min sky floor height.
pub(super) fn compute_sky_bounds(input: &Bsp3dInput) -> (Vec<f32>, Vec<f32>) {
    let global_max_ceil = input
        .sectors
        .iter()
        .map(|s| s.ceil_h)
        .fold(f32::NEG_INFINITY, f32::max);
    let global_min_floor = input
        .sectors
        .iter()
        .map(|s| s.floor_h)
        .fold(f32::INFINITY, f32::min);

    let max_ceil: Vec<f32> = input
        .sectors
        .iter()
        .map(|s| {
            if s.sky_ceil {
                global_max_ceil
            } else {
                s.ceil_h
            }
        })
        .collect();
    let min_floor: Vec<f32> = input
        .sectors
        .iter()
        .map(|s| {
            if s.sky_floor {
                global_min_floor
            } else {
                s.floor_h
            }
        })
        .collect();

    (max_ceil, min_floor)
}
