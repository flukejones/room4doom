//! Export shareware E1M1 through the full editor pipeline to a PWAD: `cargo run -p editor-core --example export_e1m1 -- <out.wad>`.

use std::path::PathBuf;

use editor_core::wad_export::{ExportOptions, export_map_pwad};
use editor_core::wad_import::import_wad_map;
use wad::WadData;

fn main() {
    let out = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("e1m1_editor.wad"));

    let wad = WadData::new(&test_utils::doom1_wad_path());
    let map = import_wad_map(&wad, "E1M1").expect("shareware E1M1 imports");
    let opts = ExportOptions {
        split_disconnected_sectors: false,
        ..ExportOptions::default()
    };
    let bytes = export_map_pwad(&map, "E1M1", &opts).expect("E1M1 exports");
    std::fs::write(&out, &bytes).expect("output file writes");
    println!("wrote {} ({} bytes)", out.display(), bytes.len());
}
