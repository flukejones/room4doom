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

/// Bring only the WAD structs down to root level
pub use crate::wad::*;

/// The WAD structure and parser, headers, lumps, wad stuff
pub mod wad;

pub mod iterators;

/// The specific types, these are contained within the Lumps
pub mod types;

/// ZDoom BSP support (and maybe others in future)
pub mod extended;
