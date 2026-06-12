//! Pure map-geometry kernel for the map editor: a CAD-style toolkit over the [`EditorMap`] document — keyed arenas ([`arena`]), the data model ([`model`]), geometric queries ([`geom`]), the sector-tracing builder ([`sector_build`]), and editing operations ([`ops`]); no UI, selection, undo, view transform, or I/O.

pub mod arena;
pub mod audit;
pub mod flags;
pub mod geom;
pub mod model;
pub mod name8;
pub mod ops;
pub mod sector_build;
#[cfg(test)]
mod test_fixtures;

pub use arena::{Arena, ArenaKey};
pub use audit::{GeomIssue, audit_geometry, heal_map};
pub use flags::{LineFlags, ThingFlags};
pub use model::{
    DenseError, DenseLineDef, DenseMap, DenseSideDef, EditorMap, GROWTH_HEADROOM, LineDef, LineKey,
    Sector, SectorKey, SideDef, Thing, ThingKey, VertKey, Vertex,
};
pub use name8::{Name8, NameError};
pub use ops::{
    Axis, add_edge, align_vertices, any_dissolvable, can_merge_collinear, can_trim_corner,
    chamfer_vertex, delete_sector, derive_sectors, dissolve_collinear_vertices,
    distribute_vertices, extract_fragment, extrude_line, fillet_vertex, flip_lines,
    fragment_min_corner, merge_collinear_lines, merge_sectors, mirror_fixup, move_vertices,
    ngon_points, paste_fragment, rect_corners, sectors_share_two_sided_wall, straighten_chain,
    transform_moves, weld_cluster,
};
pub use sector_build::{
    SectorLoop, VoidRule, add_sector_in_enclosure, build_sectors, sector_loops, sector_loops_all,
    sector_loops_for, sector_under_cursor_has_separable_loop, unmerge_sector_at,
};
