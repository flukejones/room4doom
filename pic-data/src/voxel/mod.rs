pub mod kvx;
pub mod pk3;
pub mod slices;
pub mod voxeldef;

use std::collections::HashMap;
use std::f32::consts::{PI, TAU};
use std::path::Path;

use game_config::GameMode;
use slices::VoxelSlices;

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

/// Manages loaded voxel models and provides lookup by (sprite_index,
/// frame_index).
pub struct VoxelManager {
    /// All loaded VoxelSlices, deduplicated by KVX file.
    models: Vec<VoxelSlices>,
    /// Maps (sprite_index << 16 | frame_index) to index in `models`.
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
            lookup: HashMap::new(),
        };

        let voxeldef_path = dir.parent().unwrap_or(dir).join("VOXELDEF.txt");
        let alt_path = dir.join("VOXELDEF.txt");
        let text =
            std::fs::read_to_string(&voxeldef_path).or_else(|_| std::fs::read_to_string(&alt_path));

        let text = match text {
            Ok(t) => t,
            Err(e) => {
                log::warn!(
                    "No VOXELDEF.txt found at {:?} or {:?}: {}",
                    voxeldef_path,
                    alt_path,
                    e
                );
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
                .map_err(|e| log::warn!("Failed to read {:?}: {}", kvx_path, e))
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
            lookup: HashMap::new(),
        };

        let pk3_data = match pk3::extract_voxels(pk3_path, game_mode) {
            Some(d) => d,
            None => return mgr,
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
        let mut kvx_to_idx: HashMap<String, usize> = HashMap::new();
        let total = defs.len();
        let dot_interval = (total / 30).max(1);

        for (def_idx, def) in defs.iter().enumerate() {
            if def_idx % dot_interval == 0 {
                use std::io::Write;
                print!(".");
                std::io::stdout().flush().ok();
            }

            if def.sprite_name.len() < 5 {
                log::warn!(
                    "Skipping VOXELDEF entry with short name: {}",
                    def.sprite_name
                );
                continue;
            }
            let sprite_code = def.sprite_name[..4].to_uppercase();

            if pwad_overrides.contains(&sprite_code) {
                log::info!(
                    "Skipping voxel for '{}' — sprite overridden by PWAD",
                    def.sprite_name
                );
                continue;
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
                continue;
            };

            let sprite_index = match sprite_names.iter().position(|&n| n == sprite_code) {
                Some(i) => i,
                None => {
                    log::debug!(
                        "VOXELDEF sprite code '{}' not found in SPRNAMES",
                        sprite_code
                    );
                    continue;
                }
            };

            let model_idx = if let Some(&idx) = kvx_to_idx.get(&def.kvx_file) {
                idx
            } else {
                let data = match resolve_kvx(&def.kvx_file) {
                    Some(d) => d,
                    None => continue,
                };
                let mut model = match kvx::VoxelModel::load(&data) {
                    Ok(m) => m,
                    Err(e) => {
                        log::warn!("Failed to parse KVX '{}': {}", def.kvx_file, e);
                        continue;
                    }
                };
                model.remap_to_doom_palette(doom_palette);
                let mut slices = slices::generate(&model);
                let angle_deg = def.angle_offset.unwrap_or(0) as f32 - 90.0;
                slices.angle_offset = angle_deg * PI / 180.0;
                let is_pickup = PICKUP_SPRITES.contains(&sprite_code.as_str());
                let default_spin = if is_pickup { Some(DEFAULT_SPIN) } else { None };
                let ps = def.placed_spin.or(default_spin);
                if let Some(ps) = ps.filter(|&v| v != 0) {
                    slices.placed_spin = TAU / ps as f32;
                }
                let ds = def.dropped_spin.or(default_spin);
                if let Some(ds) = ds.filter(|&v| v != 0) {
                    slices.dropped_spin = TAU / ds as f32;
                }
                if FACE_PLAYER_SPRITES.contains(&sprite_code.as_str()) {
                    slices.face_player = true;
                    slices.placed_spin = 0.0;
                    slices.dropped_spin = 0.0;
                }
                let idx = self.models.len();
                self.models.push(slices);
                kvx_to_idx.insert(def.kvx_file.clone(), idx);
                idx
            };

            let key = ((sprite_index as u32) << 16) | (frame_index as u32);
            self.lookup.insert(key, model_idx);
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

    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }
}
