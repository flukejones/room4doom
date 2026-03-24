use std::path::{Path, PathBuf};

use level::LevelData;
use wad::WadData;

/// Path to the shareware Doom1 WAD included in the repo under `data/`.
pub fn doom1_wad_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("data/doom1.wad")
}

/// Directory for user-supplied WADs (~/doom/).
fn user_wad_dir() -> PathBuf {
    dirs::home_dir().expect("no home dir").join("doom")
}

pub fn doom_wad_path() -> PathBuf {
    user_wad_dir().join("doom.wad")
}

pub fn doom2_wad_path() -> PathBuf {
    user_wad_dir().join("doom2.wad")
}

pub fn sigil_wad_path() -> PathBuf {
    user_wad_dir().join("sigil.wad")
}

pub fn sigil2_wad_path() -> PathBuf {
    user_wad_dir().join("sigil2.wad")
}

pub fn sunder_wad_path() -> PathBuf {
    user_wad_dir().join("sunder.wad")
}

/// Path to a KVX voxel file under ~/doom/cheello_voxels/voxels/.
pub fn kvx_path(name: &str) -> PathBuf {
    user_wad_dir().join("cheello_voxels/voxels").join(name)
}

/// Load a map with a no-op flat lookup (BSP/PVS tests don't need real flats).
pub fn load_map(wad_path: &Path, map_name: &str) -> LevelData {
    let wad = WadData::new(wad_path);
    let mut map = LevelData::default();
    map.load(map_name, |_| None, &wad, None, None);
    map
}

/// Load a map from a base WAD with a PWAD merged.
pub fn load_map_with_pwad(base_wad: &Path, pwad: &Path, map_name: &str) -> LevelData {
    let mut wad = WadData::new(base_wad);
    wad.add_file(pwad.into());
    let mut map = LevelData::default();
    map.load(map_name, |_| None, &wad, None, None);
    map
}

pub fn eviternity_wad_path() -> PathBuf {
    user_wad_dir().join("Eviternity.wad")
}

/// Load a map with flat name tracking so sky sectors get correct flat indices.
/// Returns (LevelData, Option<sky_flat_index>).
///
/// Uses manual lump scan for flat list (LumpIter has a bug with multi-chunk
/// flat sections where IWAD flats get dropped when PWAD has its own section).
pub fn load_map_with_flats(wad: &WadData, map_name: &str) -> (LevelData, Option<usize>) {
    let mut flats = Vec::new();
    let mut in_flats = false;
    for l in wad.lumps() {
        if l.name == "F_START" || l.name == "FF_START" {
            in_flats = true;
            continue;
        }
        if l.name == "F_END" || l.name == "FF_END" {
            in_flats = false;
            continue;
        }
        if in_flats && !l.data.is_empty() {
            flats.push(l.name.clone());
        }
    }
    // Deduplicate: last occurrence wins (PWAD override)
    let mut deduped: Vec<String> = Vec::new();
    for name in flats.iter().rev() {
        if !deduped.iter().any(|n| n == name) {
            deduped.push(name.clone());
        }
    }
    deduped.reverse();

    let sky_num = deduped.iter().position(|n| n == "F_SKY1");
    let flat_lookup = move |name: &str| -> Option<usize> { deduped.iter().position(|n| n == name) };
    let mut map = LevelData::default();
    map.load(map_name, flat_lookup, wad, sky_num, None);
    (map, sky_num)
}
