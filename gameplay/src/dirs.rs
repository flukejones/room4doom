//! Unified directory paths for Room4Doom.
//!
//! All platform-specific directory resolution lives here so every crate
//! uses the same locations.

use std::path::PathBuf;

use log::warn;

const APP_DIR: &str = "room4doom";

/// Base configuration directory: `<platform config>/room4doom/`.
/// On macOS: `~/Library/Application Support/room4doom/`
///
/// Falls back to `./room4doom/` (relative to the working directory) on
/// platforms where the user config directory can't be resolved.
pub fn config_dir() -> PathBuf {
    let mut dir = dirs::config_dir().unwrap_or_else(|| {
        warn!("Could not determine config directory, using current directory");
        PathBuf::new()
    });
    dir.push(APP_DIR);
    dir
}

/// Save game directory: `<platform config>/room4doom/saves/`.
pub fn save_dir() -> PathBuf {
    config_dir().join("saves")
}

/// Cache directory: `<platform cache>/room4doom/`.
/// On macOS: `~/Library/Caches/room4doom/`
///
/// Falls back to `./room4doom/` when the platform cache directory can't be
/// resolved.
pub fn cache_dir() -> PathBuf {
    let mut dir = dirs::cache_dir().unwrap_or_else(|| {
        warn!("Could not determine cache directory, using current directory");
        PathBuf::new()
    });
    dir.push(APP_DIR);
    dir
}
