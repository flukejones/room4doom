use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use game_config::GameMode;
use zip::ZipArchive;

pub struct Pk3Voxels {
    pub voxeldef_text: String,
    pub kvx_files: HashMap<String, Vec<u8>>,
}

pub fn extract_voxels(path: &Path, game_mode: GameMode) -> Option<Pk3Voxels> {
    let file = File::open(path)
        .map_err(|e| log::warn!("Failed to open PK3 {:?}: {}", path, e))
        .ok()?;
    let mut archive = ZipArchive::new(BufReader::new(file))
        .map_err(|e| log::warn!("Failed to read PK3 {:?}: {}", path, e))
        .ok()?;

    let filter_prefix = match game_mode {
        GameMode::Commercial => "filter/doom.id.doom2/",
        _ => "filter/doom.id.doom1/",
    };

    // Collect entry names first to avoid borrow issues
    let names: Vec<String> = (0..archive.len())
        .filter_map(|i| archive.by_index(i).ok().map(|e| e.name().to_string()))
        .collect();

    let mut voxeldef_text = String::new();
    let mut kvx_files: HashMap<String, Vec<u8>> = HashMap::new();

    // Root VOXELDEF first
    if let Some(name) = names
        .iter()
        .find(|n| n.eq_ignore_ascii_case("VOXELDEF.txt"))
    {
        if let Ok(mut entry) = archive.by_name(name) {
            let mut text = String::new();
            entry.read_to_string(&mut text).ok();
            voxeldef_text.push_str(&text);
            voxeldef_text.push('\n');
        }
    }

    // Filter-specific VOXELDEF appended (last-match-wins)
    let filter_voxeldef = format!("{filter_prefix}VOXELDEF.txt");
    for name in &names {
        if name.eq_ignore_ascii_case(&filter_voxeldef) {
            if let Ok(mut entry) = archive.by_name(name) {
                let mut text = String::new();
                entry.read_to_string(&mut text).ok();
                voxeldef_text.push_str(&text);
                voxeldef_text.push('\n');
            }
            break;
        }
    }

    if voxeldef_text.is_empty() {
        log::warn!("No VOXELDEF.txt found in PK3 {:?}", path);
        return None;
    }

    // Collect KVX files: root voxels/ first, then filter overrides
    let root_voxels = "voxels/";
    let filter_voxels = format!("{filter_prefix}voxels/");

    for name in &names {
        let lower = name.to_ascii_lowercase();
        let stem = if lower.starts_with(&filter_voxels.to_ascii_lowercase()) {
            kvx_stem(&lower[filter_voxels.len()..])
        } else if lower.starts_with(root_voxels) && lower.len() > root_voxels.len() {
            kvx_stem(&lower[root_voxels.len()..])
        } else {
            continue;
        };

        let Some(stem) = stem else { continue };

        if let Ok(mut entry) = archive.by_name(name) {
            let mut data = Vec::with_capacity(entry.size() as usize);
            if entry.read_to_end(&mut data).is_ok() {
                // Filter entries processed after root entries in names list order,
                // but both root and filter are intermixed. HashMap::insert overwrites,
                // so whichever comes last wins. To ensure filter wins, we insert root
                // entries only if absent, and always insert filter entries.
                let is_filter = lower.starts_with(&filter_voxels.to_ascii_lowercase());
                if is_filter {
                    kvx_files.insert(stem, data);
                } else {
                    kvx_files.entry(stem).or_insert(data);
                }
            }
        }
    }

    Some(Pk3Voxels {
        voxeldef_text,
        kvx_files,
    })
}

fn kvx_stem(filename: &str) -> Option<String> {
    // Strip subdirectory components — take only the final filename
    let filename = filename.rsplit('/').next().unwrap_or(filename);
    if filename.is_empty() {
        return None;
    }
    // Strip .kvx extension if present
    Some(
        filename
            .strip_suffix(".kvx")
            .unwrap_or(filename)
            .to_string(),
    )
}
