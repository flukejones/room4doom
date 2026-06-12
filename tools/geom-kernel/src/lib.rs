//! Pure map-geometry kernel for the map editor.
//!
//! A CAD-style toolkit over the [`EditorMap`] document: the data model, vertex/
//! line/sector primitives, geometric queries ([`geom`]), the sector-tracing
//! builder ([`sector_build`]), and higher-level editing operations ([`ops`]).
//! No UI, selection, undo, view transform, or I/O — those live above this layer.

pub mod flags;
pub mod geom;
pub mod model;
pub mod name8;
pub mod ops;
pub mod sector_build;

pub use flags::{LineFlags, ThingFlags};
pub use model::{EditorMap, GROWTH_HEADROOM, LineDef, Sector, SideDef, Thing, Vertex};
pub use name8::{Name8, NameError};
pub use ops::{
    MoveResult, WeldResult, add_edge, delete_sector, derive_sectors, extract_fragment, flip_lines,
    fragment_min_corner, lines_share_vertex_within_angle, merge_collinear_lines, merge_sectors,
    move_vertices, ngon_points, paste_fragment, rect_corners, sectors_share_two_sided_wall,
    weld_cluster,
};
pub use sector_build::{
    SectorLoop, add_sector_in_enclosure, build_sectors, sector_loops, sector_loops_all,
    sector_under_cursor_has_separable_loop, unmerge_sector_at,
};
