//! Derived geometry, texture and lighting caches for the open map.

use super::atlas::AtlasMaps;
use super::frame3d::SurfaceSlots;
use super::input::{SectorFill, Selection};
use super::triangulate::SectorTris;
use super::wgpu::{LineInst, MarkerInst};
use crate::assets::AssetGen;
use crate::light_anim::SectorLight;
use editor_core::{EditorMap, LineDef, LineKey, SectorKey, VertKey};
use std::collections::HashMap;

/// Render caches for the open map; replaced wholesale on map load.
#[derive(Default)]
pub(crate) struct MapRender {
    pub(crate) preview_lines: Vec<LineInst>,
    pub(crate) preview_markers: Vec<MarkerInst>,
    pub(crate) links: MapLinks,
    /// Swept sector fill triangles, keyed by sector; dirty sectors re-triangulate in place.
    pub(crate) sector_tris: SectorTris,
    /// Lowest bordering floor Z per vertex slot, for wireframe mode.
    pub(crate) vertex_floor_z: Vec<f32>,
    /// Atlas lookup maps (flat tiles + thing sprite slots).
    pub(crate) atlas_maps: AtlasMaps,
    /// Content key of the last-packed atlas. Unchanged → skip RGBA pack + GPU upload.
    pub(crate) atlas_key: Option<u64>,
    /// Per-sector light effects; feeds GPU brightness buffer each tic.
    pub(crate) light_anim: Vec<SectorLight>,
    /// Set when a reconcile touched light-effect inputs (sectors/lines); consumed by the light-set refresh.
    pub(crate) light_set_dirty: bool,
    /// The map as last pushed to the GPU; the reconciler diffs against it. `None` = nothing synced yet → the one full build (map load).
    pub(crate) last_synced: Option<EditorMap>,
    /// Selection as last pushed (drives instance-colour and sector-tint patches).
    pub(crate) last_selection: Selection,
    /// Highlighted sectors as last pushed (floor-tint diff).
    pub(crate) last_highlighted: Vec<SectorKey>,
    /// Grid-plane Z as last pushed; a move re-emits the instance layers riding it.
    pub(crate) last_grid_z: f32,
    /// Fill mode as last pushed; a switch re-emits instances and the wire layer.
    pub(crate) last_fill: SectorFill,
    /// Pixel ratio the instance layers were last built with; a DPI change re-emits them.
    pub(crate) last_pixel_ratio: f32,
    /// Asset generation the atlases were last packed against (texture-editor edits move it).
    pub(crate) last_asset_gen: Option<AssetGen>,
    /// Resource set that produced the current atlases; local asset counters restart on reload.
    pub(crate) last_resource_generation: Option<u64>,
    /// Surface3d span table: per-sector fills + per-line walls over the CPU mirror.
    pub(crate) surface_slots: SurfaceSlots,
}

/// Incidence in the last-synced map, updated with its changed line records.
#[derive(Default)]
pub(crate) struct MapLinks {
    pub(super) vertices: HashMap<VertKey, Vec<LineKey>>,
    pub(super) sectors: HashMap<SectorKey, Vec<LineKey>>,
}

impl MapLinks {
    pub(super) fn build(map: &EditorMap) -> Self {
        let mut links = Self::default();
        for (key, line) in map.lines.iter() {
            links.insert(key, line);
        }
        links
    }

    fn insert(&mut self, key: LineKey, line: &LineDef) {
        self.vertices.entry(line.v1).or_default().push(key);
        if line.v2 != line.v1 {
            self.vertices.entry(line.v2).or_default().push(key);
        }
        let front = line.front.sector;
        let back = line.back.as_ref().and_then(|side| side.sector);
        for sector in [front, back.filter(|&sector| Some(sector) != front)]
            .into_iter()
            .flatten()
        {
            self.sectors.entry(sector).or_default().push(key);
        }
    }

    pub(super) fn update(
        &mut self,
        old: &EditorMap,
        new: &EditorMap,
        changed: &[LineKey],
        removed: &[LineKey],
    ) {
        fn remove<K: Eq + std::hash::Hash>(
            index: &mut HashMap<K, Vec<LineKey>>,
            key: &K,
            line: LineKey,
        ) {
            if let Some(lines) = index.get_mut(key) {
                lines.retain(|&candidate| candidate != line);
                if lines.is_empty() {
                    index.remove(key);
                }
            }
        }
        for &key in changed.iter().chain(removed) {
            let before = old.lines.get(key);
            let after = new.lines.get(key);
            if before.zip(after).is_some_and(|(a, b)| {
                a.v1 == b.v1
                    && a.v2 == b.v2
                    && a.front.sector == b.front.sector
                    && a.back.as_ref().and_then(|side| side.sector)
                        == b.back.as_ref().and_then(|side| side.sector)
            }) {
                continue;
            }
            if let Some(line) = before {
                remove(&mut self.vertices, &line.v1, key);
                remove(&mut self.vertices, &line.v2, key);
                for sector in line.sides().filter_map(|side| side.sector) {
                    remove(&mut self.sectors, &sector, key);
                }
            }
            if let Some(line) = after {
                self.insert(key, line);
            }
        }
    }
}
