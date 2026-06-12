pub mod faces;
pub mod kvx;
pub mod pk3;
pub mod slices;
pub mod voxeldef;

use std::collections::HashMap;
use std::f32::consts::{PI, TAU};
use std::path::Path;

use faces::{VoxelFace, generate_faces};
use slices::VoxelSlices;
use wad::types::GameMode;

use crate::parallel_map;

/// Default spin speed (tics per full rotation) for pickup item sprites.
const DEFAULT_SPIN: i32 = 150;

/// Sprite codes that spin by default when no PlacedSpin/DroppedSpin is
/// specified.
const PICKUP_SPRITES: &[&str] = &[
    "AMMO", "ARM1", "ARM2", "BFUG", "BKEY", "BON1", "BON2", "BPAK", "BROK", "BSKU", "CELL", "CELP",
    "CLIP", "CSAW", "LAUN", "MEDI", "MEGA", "MGUN", "PINS", "PINV", "PIST", "PLAS", "PMAP", "PSTR",
    "PVIS", "RKEY", "ROCK", "RSKU", "SBOX", "SGN2", "SHEL", "SHOT", "SOUL", "STIM", "SUIT", "YKEY",
    "YSKU",
];

/// Sprite codes whose voxels always orient toward the player.
const FACE_PLAYER_SPRITES: &[&str] = &["CEYE", "FSKU", "MEGA", "PINS", "PINV", "SOUL"];

/// A unique KVX file plus the def fields needed to build its model. Owns its
/// bytes so the heavy build can run off-thread.
struct BuildJob {
    kvx_file: String,
    data: Vec<u8>,
    angle_offset: Option<i32>,
    placed_spin: Option<i32>,
    dropped_spin: Option<i32>,
    sprite_code: String,
}

/// Validate a def and resolve its sprite/frame indices. `None` skips the def.
fn resolve_def(
    def: &voxeldef::VoxelDef,
    sprite_names: &[&str],
    pwad_overrides: &std::collections::HashSet<String>,
) -> Option<(usize, usize, String)> {
    if def.sprite_name.len() < 5 {
        log::warn!(
            "Skipping VOXELDEF entry with short name: {}",
            def.sprite_name
        );
        return None;
    }
    let sprite_code = def.sprite_name[..4].to_uppercase();
    if pwad_overrides.contains(&sprite_code) {
        log::info!(
            "Skipping voxel for '{}' — overridden by PWAD",
            def.sprite_name
        );
        return None;
    }
    let frame_char = def.sprite_name.as_bytes()[4];
    let frame_index = if frame_char.is_ascii_lowercase() {
        (frame_char - b'a') as usize
    } else if frame_char.is_ascii_uppercase() {
        (frame_char - b'A') as usize
    } else if frame_char.is_ascii_digit() {
        (frame_char - b'0') as usize
    } else {
        log::warn!("Bad frame char in VOXELDEF entry: {}", def.sprite_name);
        return None;
    };
    let sprite_index = sprite_names.iter().position(|&n| n == sprite_code)?;
    Some((sprite_index, frame_index, sprite_code))
}

/// Parse + palette-remap + generate slices and GPU faces for one job.
fn build_model(job: &BuildJob, doom_palette: &[u8]) -> Option<(VoxelSlices, Vec<VoxelFace>)> {
    let mut model = match kvx::VoxelModel::load(&job.data) {
        Ok(m) => m,
        Err(e) => {
            log::warn!("Failed to parse KVX '{}': {}", job.kvx_file, e);
            return None;
        }
    };
    model.remap_to_doom_palette(doom_palette);
    let mut slices = slices::generate(&model);
    let angle_deg = job.angle_offset.unwrap_or(0) as f32 - 90.0;
    slices.angle_offset = angle_deg * PI / 180.0;
    let is_pickup = PICKUP_SPRITES.contains(&job.sprite_code.as_str());
    let default_spin = if is_pickup { Some(DEFAULT_SPIN) } else { None };
    if let Some(ps) = job.placed_spin.or(default_spin).filter(|&v| v != 0) {
        slices.placed_spin = TAU / ps as f32;
    }
    if let Some(ds) = job.dropped_spin.or(default_spin).filter(|&v| v != 0) {
        slices.dropped_spin = TAU / ds as f32;
    }
    if FACE_PLAYER_SPRITES.contains(&job.sprite_code.as_str()) {
        slices.face_player = true;
        slices.placed_spin = 0.0;
        slices.dropped_spin = 0.0;
    }
    let faces = generate_faces(&model);
    Some((slices, faces))
}

/// Manages loaded voxel models and provides lookup by (sprite_index,
/// frame_index).
pub struct VoxelManager {
    models: Vec<VoxelSlices>,
    /// GPU face lists parallel to `models` (palette indices, gamma-stable).
    faces: Vec<Vec<VoxelFace>>,
    lookup: HashMap<u32, usize>,
}

impl VoxelManager {
    /// Load voxels from a directory containing KVX files and a VOXELDEF.txt
    /// in its parent directory (or the same directory).
    pub fn load_from_directory(
        dir: &Path,
        sprite_names: &[&str],
        doom_palette: &[u8],
        pwad_overrides: &std::collections::HashSet<String>,
    ) -> Self {
        let mut mgr = Self {
            models: Vec::new(),
            faces: Vec::new(),
            lookup: HashMap::new(),
        };

        let voxeldef_path = dir.parent().unwrap_or(dir).join("VOXELDEF.txt");
        let alt_path = dir.join("VOXELDEF.txt");
        let text =
            std::fs::read_to_string(&voxeldef_path).or_else(|_| std::fs::read_to_string(&alt_path));

        let text = match text {
            Ok(t) => t,
            Err(e) => {
                log::warn!("No VOXELDEF.txt found at {voxeldef_path:?} or {alt_path:?}: {e}");
                return mgr;
            }
        };

        let defs = voxeldef::parse(&text);
        let mut resolve = |kvx_name: &str| -> Option<Vec<u8>> {
            let kvx_path = if kvx_name.contains('/') || kvx_name.contains('\\') {
                dir.parent().unwrap_or(dir).join(kvx_name)
            } else if kvx_name.ends_with(".kvx") {
                dir.join(kvx_name)
            } else {
                dir.join(format!("{kvx_name}.kvx"))
            };
            std::fs::read(&kvx_path)
                .map_err(|e| log::warn!("Failed to read {kvx_path:?}: {e}"))
                .ok()
        };
        mgr.load_defs(
            &defs,
            sprite_names,
            doom_palette,
            pwad_overrides,
            &mut resolve,
        );
        mgr
    }

    /// Load voxels from a PK3 (ZIP) archive.
    pub fn load_from_pk3(
        pk3_path: &Path,
        game_mode: GameMode,
        sprite_names: &[&str],
        doom_palette: &[u8],
        pwad_overrides: &std::collections::HashSet<String>,
    ) -> Self {
        let mut mgr = Self {
            models: Vec::new(),
            faces: Vec::new(),
            lookup: HashMap::new(),
        };

        let Some(pk3_data) = pk3::extract_voxels(pk3_path, game_mode) else {
            return mgr;
        };

        let defs = voxeldef::parse(&pk3_data.voxeldef_text);
        let kvx_files = pk3_data.kvx_files;
        let mut resolve = |kvx_name: &str| -> Option<Vec<u8>> {
            let key = kvx_name
                .strip_suffix(".kvx")
                .unwrap_or(kvx_name)
                .to_ascii_lowercase();
            kvx_files.get(&key).cloned()
        };
        mgr.load_defs(
            &defs,
            sprite_names,
            doom_palette,
            pwad_overrides,
            &mut resolve,
        );
        mgr
    }

    fn load_defs(
        &mut self,
        defs: &[voxeldef::VoxelDef],
        sprite_names: &[&str],
        doom_palette: &[u8],
        pwad_overrides: &std::collections::HashSet<String>,
        resolve_kvx: &mut dyn FnMut(&str) -> Option<Vec<u8>>,
    ) {
        // Phase 1 (serial): resolve each def to a lookup key, and gather the raw
        // bytes + build params for each unique KVX file. `resolve_kvx` reads from
        // disk/zip and is not `Send`, so it must run here.
        let mut kvx_to_job: HashMap<String, usize> = HashMap::new();
        let mut jobs: Vec<BuildJob> = Vec::new();
        let mut mappings: Vec<(u32, usize)> = Vec::new();

        for def in defs {
            let Some((sprite_index, frame_index, sprite_code)) =
                resolve_def(def, sprite_names, pwad_overrides)
            else {
                continue;
            };

            let job_idx = if let Some(&idx) = kvx_to_job.get(&def.kvx_file) {
                idx
            } else {
                let Some(data) = resolve_kvx(&def.kvx_file) else {
                    continue;
                };
                let idx = jobs.len();
                jobs.push(BuildJob {
                    kvx_file: def.kvx_file.clone(),
                    data,
                    angle_offset: def.angle_offset,
                    placed_spin: def.placed_spin,
                    dropped_spin: def.dropped_spin,
                    sprite_code,
                });
                kvx_to_job.insert(def.kvx_file.clone(), idx);
                idx
            };
            let key = ((sprite_index as u32) << 16) | (frame_index as u32);
            mappings.push((key, job_idx));
        }

        // Phase 2 (parallel): parse + palette-remap + slice/face precompute per
        // file. Independent per model — fan out across the available cores.
        let palette = doom_palette;
        let results = parallel_map(&jobs, |job| build_model(job, palette));

        // Phase 3 (serial): stitch in source order, keeping `models`/`faces`
        // parallel with stable indices. Failed builds collapse their defs.
        let mut job_to_model: Vec<Option<usize>> = vec![None; jobs.len()];
        for (job_idx, result) in results.into_iter().enumerate() {
            if let Some((slices, faces)) = result {
                job_to_model[job_idx] = Some(self.models.len());
                self.models.push(slices);
                self.faces.push(faces);
            }
        }
        for (key, job_idx) in mappings {
            if let Some(model_idx) = job_to_model[job_idx] {
                self.lookup.insert(key, model_idx);
            }
        }

        log::info!(
            "Loaded {} voxel models, {} sprite-frame mappings",
            self.models.len(),
            self.lookup.len()
        );
    }

    /// Look up voxel slices for a sprite by engine sprite index and frame
    /// index.
    pub fn get(&self, sprite_index: usize, frame_index: usize) -> Option<&VoxelSlices> {
        let key = ((sprite_index as u32) << 16) | (frame_index as u32);
        self.lookup.get(&key).map(|&idx| &self.models[idx])
    }

    /// Look up the model index + slices for a sprite/frame. The index is stable
    /// and parallel to [`Self::faces`], so a GPU renderer can map a thing to its
    /// baked face buffer.
    pub fn get_indexed(
        &self,
        sprite_index: usize,
        frame_index: usize,
    ) -> Option<(usize, &VoxelSlices)> {
        let key = ((sprite_index as u32) << 16) | (frame_index as u32);
        self.lookup.get(&key).map(|&idx| (idx, &self.models[idx]))
    }

    /// GPU face lists in model-index order. Indices match [`Self::get_indexed`].
    pub fn faces(&self) -> &[Vec<VoxelFace>] {
        &self.faces
    }

    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }
}
