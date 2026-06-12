//! Map data model and file I/O for the ReDoomEd-style map editor.
//!
//! A pure-data [`EditorMap`] with first-class sectors, the native RON map
//! format ([`map_ron`]) and TOML project format ([`project`]) the editor reads
//! and writes, DoomEd `.dwd`/`.dpr` import (read-only), vanilla-map import from
//! WADs, structural validation, and PWAD export with BSP nodes built by
//! `rbsp`. No UI — that lives in the editor application layer.

pub mod map_ron;
pub mod project;
pub mod texture_group;
pub mod texture_lumps;
pub mod validate;
pub mod wad_export;
pub mod wad_import;

// The pure geometry kernel. Re-exported so editor-core is the editor's single
// data-layer facade (downstream keeps using `editor_core::geom`, `::model`,
// `::EditorMap`, …) and so this crate's own I/O modules reach the model via
// `crate::model` / `crate::name8`.
pub use geom_kernel::{
    EditorMap, LineDef, LineFlags, MoveResult, Name8, NameError, Sector, SectorLoop, SideDef,
    Thing, ThingFlags, Vertex, add_edge, add_sector_in_enclosure, build_sectors, delete_sector,
    derive_sectors, extract_fragment, flip_lines, fragment_min_corner, geom,
    lines_share_vertex_within_angle, merge_collinear_lines, merge_sectors, model, move_vertices,
    name8, ngon_points, ops, paste_fragment, rect_corners, sector_build, sector_loops,
    sector_loops_all, sector_under_cursor_has_separable_loop, sectors_share_two_sided_wall,
    unmerge_sector_at, weld_cluster,
};

// The DoomEd ASCII format layer (.dwd maps, .dsp/.dpr defs). Re-exported as the
// data-layer facade and so this crate's I/O modules reach the parsers via
// `crate::dwd` / `crate::dsp`.
pub use doomed_parser::{
    AnimDef, DspError, DwdError, PatchPlacement, SpecialDef, TextureDef, ThingDef, dsp, dwd,
    load_dwd, parse_dwd,
};

pub use map_ron::{MapRonError, load_map_ron, parse_map_ron, save_map_ron, write_map_ron};
pub use project::{
    ImportedPatch, Project, ProjectError, ProjectPreferences, TextureMode,
    import_wad_texture_groups,
};
pub use rbsp::wad_io::NodesFormat;
pub use texture_group::TextureGroup;
pub use texture_lumps::{TextureLumpError, encode_texture_lumps};
pub use validate::{Issue, validate};
pub use wad_export::{ExportError, ExportOptions, export_map_pwad};
pub use wad_import::{WadImportError, import_wad_map};
