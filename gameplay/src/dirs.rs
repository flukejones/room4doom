//! Unified directory paths for Room4Doom.
//!
//! All platform-specific directory resolution lives here so every crate
//! uses the same locations.

use std::path::PathBuf;

const APP_DIR: &str = "room4doom";

/// Base configuration directory: `<platform config>/room4doom/`.
/// On macOS: `~/Library/Application Support/room4doom/`
pub fn config_dir() -> PathBuf {
    let mut dir = dirs::config_dir().expect("could not determine config directory");
    dir.push(APP_DIR);
    dir
}

/// Save game directory: `<platform config>/room4doom/saves/`.
pub fn save_dir() -> PathBuf {
    config_dir().join("saves")
}

/// Cache directory: `<platform cache>/room4doom/`.
/// On macOS: `~/Library/Caches/room4doom/`
pub fn cache_dir() -> PathBuf {
    let mut dir = dirs::cache_dir().expect("could not determine cache directory");
    dir.push(APP_DIR);
    dir
}
