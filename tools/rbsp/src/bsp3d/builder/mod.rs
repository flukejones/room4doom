//! 3D-BSP construction.
//!
//! [`Bsp3dBuilder`] owns all construction scratch (vertex dedup map, zh wall
//! records, per-leaf polygon buckets) and emits a flat [`Bsp3dLump`] via its
//! condense step. Nothing here survives to runtime — the engine's runtime
//! structure is parsed from the lump.
//!
//! Submodules by purpose:
//! - [`types`]: construction-time records and shared math.
//! - [`walls`]: wall-quad construction (two/one-sided, zero-height, shared).
//! - [`flats`]: floor/ceiling N-gons from carved subsector polygons.
//! - [`sky`]: sky filler walls and global sky bounds.
//! - [`condense`]: flatten the scratch into the serializable lump.
//!
//! The zero-height mover vertex pass lives in the sibling
//! [`movers`](crate::bsp3d::movers) module.

mod condense;
mod flats;
mod sky;
pub mod types;
mod walls;

use std::collections::HashMap;

use glam::Vec3;

use crate::bsp3d::input::{Bsp3dInput, NO_REF};
use crate::bsp3d::lump::{Bsp3dLump, tree_from_nodes};
use crate::types::Node;

pub use types::{HEIGHT_EPSILON, QUANT_PRECISION};
// Re-exported for the sibling `movers` module (the zero-height vertex pass runs
// on `Bsp3dBuilder` and inspects these construction records).
pub(crate) use types::{BuildKind, QuantizedVec3, WallType};

use sky::compute_sky_bounds;
use types::{BuildLeaf, BuildPolygon, ZhWallRecord};

/// Accumulates 3D geometry during construction, then condenses to a [`Bsp3dLump`].
pub struct Bsp3dBuilder {
    pub(crate) polygons: Vec<BuildPolygon>,
    pub(crate) leaves: Vec<BuildLeaf>,
    pub(crate) vertices: Vec<Vec3>,
    pub(crate) sector_subsectors: Vec<Vec<usize>>,
    pub(crate) zh_wall_records: Vec<ZhWallRecord>,
    pub(crate) vertex_map: HashMap<QuantizedVec3, usize>,
}

impl Bsp3dBuilder {
    /// Build the flat 3D geometry lump.
    ///
    /// - Creates wall quads, floor/ceiling N-gons (from the pre-carved convex
    ///   subsector polygons), and sky filler geometry
    /// - Runs the mover vertex pass for zero-height boundary sectors
    /// - Condenses into a leaf-contiguous [`Bsp3dLump`]
    pub fn build(input: &Bsp3dInput, nodes: &[Node]) -> Bsp3dLump {
        let mut builder = Self {
            polygons: Vec::new(),
            leaves: vec![BuildLeaf::default(); input.subsectors.len()],
            vertices: Vec::new(),
            sector_subsectors: vec![Vec::new(); input.sectors.len()],
            zh_wall_records: Vec::new(),
            vertex_map: HashMap::with_capacity(input.segs.len() * 2),
        };

        for (ss_id, ss) in input.subsectors.iter().enumerate() {
            let sector_id = ss.sector as usize;
            if sector_id < input.sectors.len() {
                builder.sector_subsectors[sector_id].push(ss_id);
            }
        }

        // Walls from segs.
        for (ss_id, ss) in input.subsectors.iter().enumerate() {
            builder.leaves[ss_id].sector_id = ss.sector as usize;
            let start = ss.start_seg as usize;
            let end = start + ss.seg_count as usize;
            for seg in &input.segs[start..end] {
                if seg.backsector != NO_REF {
                    builder.create_two_sided_walls(input, seg, ss_id);
                } else {
                    builder.create_one_sided_wall(input, seg, ss_id);
                }
            }
        }

        // Floor/ceiling N-gons from carved polygons (sky flats skipped — the
        // sky is drawn by the renderers' sky pass plus the filler walls).
        for (ssid, ss) in input.subsectors.iter().enumerate() {
            builder.create_floor_ceiling_polygons(
                input,
                ssid,
                ss.sector as usize,
                &input.carved[ssid],
            );
        }

        // Mover vertex pass — separate shared vertices at zero-height
        // boundaries, connect wall vertices, set moves flags.
        builder.mover_vertex_pass(input);

        // Sky filler — extend perimeter walls of sky sectors up to max
        // adjacent sky ceiling / down to min adjacent sky floor.
        if input.sky_fillers {
            let (sky_max_ceil, sky_min_floor) = compute_sky_bounds(input);
            builder.sky_filler_pass(input, &sky_max_ceil, &sky_min_floor);
        }

        let mut lump = builder.condense();
        lump.tree = tree_from_nodes(nodes);
        lump
    }
}
