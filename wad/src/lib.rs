//! This crate contains all the structures and tools for processing
//! WAD files, maps, things, textures, basically all the data.
//!
//! The structure of a WAD is this:
//!
//! ```text,ignore
//!                        <───── 32 bits ──────>
//!                        ┌────────────────────┐
//!             ┌──── 0x00 |  ASCII WAD Type    | 0x03
//!             |          | ────────────────── |
//!     Header ─┤     0x04 | # of directories   | 0x07
//!             |          | ────────────────── |
//!             └──── 0x08 | offset to listing ───0x0B ──┐
//!             ┌───────── | ────────────────── |        |
//!             |     0x0C | ┌────────────────┐ |        |
//!             |          | |   Lump Bytes   |<─────┐   |
//!     Lumps ──┤          | |       .        | |    |   |
//!             |          | └────────────────┘ |    |   |
//!             |          |         .          |    |   |
//!             └───────── |         .          |    |   |
//!             ┌───────── | ┌────────────────┐<─────────┘
//!             |          | |   Lump Offset  |──────┘
//!             |          | |----------------| |
//!  Directory ─┤          | |   Lump Size    | |
//!     List    |          | |----------------| |
//!             |          | |   Lump Name    | |
//!             |          | └────────────────┘ |
//!             |          |         .          |
//!             |          |         .          |
//!             |          |         .          |
//!             └───────── └────────────────────┘
//! ```

pub use glam::*;

/// Bring only the WAD structs down to root level
pub use crate::wad::*;

/// The WAD structure and parser
pub mod wad;

/// A Lump is a chunk of data that starts at an offset in the WAD, and ends
/// at a location that is `sizeof<record-in-lump> * num-of-entries`
///
/// The lump module contains the required structures that the lump records
/// need to be parsed in to. Parsing is done via the `wad` module
pub mod lumps;

pub mod nodes;

pub type Vertex = Vec2;
