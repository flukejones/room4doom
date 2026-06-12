//! Map data model and file I/O for the ReDoomEd-style map editor: a pure-data [`EditorMap`] with first-class sectors, native RON ([`map_ron`]) and TOML project ([`project`]) formats, DoomEd `.dwd`/`.dpr` import (read-only), vanilla-map import from WADs, structural validation, and PWAD export with BSP nodes built by `rbsp`. No UI — that lives in the editor application layer.

pub mod map_ron;
pub mod project;
pub mod texture_group;
pub mod texture_lumps;
pub mod validate;
pub mod wad_export;
pub mod wad_import;

// The pure geometry kernel, re-exported so editor-core is the editor's single data-layer facade (downstream keeps using `editor_core::geom`/`::model`/`::EditorMap`) and so this crate's I/O modules reach the model via `crate::model`/`crate::name8`.
pub use geom_kernel::{
    Arena, ArenaKey, Axis, DenseError, DenseLineDef, DenseMap, DenseSideDef, EditorMap, GeomIssue,
    LineDef, LineFlags, LineKey, Name8, Sector, SectorKey, SectorLoop, SideDef, Thing, ThingFlags,
    ThingKey, VertKey, Vertex, add_edge, add_sector_in_enclosure, align_vertices, any_dissolvable,
    audit_geometry, can_merge_collinear, can_trim_corner, chamfer_vertex, delete_sector,
    derive_sectors, dissolve_collinear_vertices, distribute_vertices, extract_fragment,
    extrude_line, fillet_vertex, flip_lines, fragment_min_corner, geom, heal_map,
    merge_collinear_lines, merge_sectors, mirror_fixup, model, move_vertices, name8, ngon_points,
    ops, paste_fragment, rect_corners, sector_build, sector_loops, sector_loops_all,
    sector_loops_for, sector_under_cursor_has_separable_loop, sectors_share_two_sided_wall,
    straighten_chain, transform_moves, unmerge_sector_at, weld_cluster,
};

// The DoomEd ASCII format layer (.dwd maps, .dsp/.dpr defs), re-exported as the data-layer facade and so this crate's I/O modules reach the parsers via `crate::dwd`/`crate::dsp`.
pub use doomed_parser::{
    AnimDef, DspError, PatchPlacement, SpecialDef, TextureDef, ThingDef, dsp, dwd, parse_dwd,
};

pub use map_ron::{MapRonError, load_map_ron, parse_map_ron, save_map_ron};
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
