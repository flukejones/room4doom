//! This crate contains all the structures and tools for processing
//! WAD files, maps, things, textures, basically all the data.

/// Data structures for a map, such as vertexes, lines, nodes
pub mod map;
/// The WAD structure and parser
pub mod wad;

pub mod lumps;

/// Bring only the WAD structs down to root level
pub use crate::wad::*;
