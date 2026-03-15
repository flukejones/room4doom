use std::path::PathBuf;

/// Path to the shareware Doom1 WAD included in the repo under `test_wads/`.
pub fn doom1_wad_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("test_wads/doom1.wad")
}

/// Path to the shareware Doom2 WAD included in the repo under `test_wads/`.
pub fn doom2_wad_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("test_wads/doom2.wad")
}

/// Full commercial Doom WAD — requires a local copy, not in repo.
pub const DOOM_WAD: &str = "/Users/lukejones/DOOM/doom.wad";
/// Sigil episode WAD — requires a local copy, not in repo.
pub const SIGIL_WAD: &str = "/Users/lukejones/DOOM/sigil.wad";
/// Sigil 2 episode WAD — requires a local copy, not in repo.
pub const SIGIL2_WAD: &str = "/Users/lukejones/DOOM/sigil2.wad";
/// Commercial Doom2 WAD — requires a local copy, not in repo.
pub const DOOM2_WAD: &str = "/Users/lukejones/DOOM/doom2.wad";
/// Sunder megawad — requires a local copy, not in repo.
pub const SUNDER_WAD: &str = "/Users/lukejones/DOOM/sunder.wad";
