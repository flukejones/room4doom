//! A texture set tagged with its source WAD + lump. Replaces the provenance-less
//! `Vec<Vec<TextureDef>>`: each `TEXTURE<n>` lump of each WAD is one group, so a
//! map can target a specific WAD and lookups override by name without merging
//! distinct WADs' textures into one namespace.

use doomed_parser::TextureDef;
use geom_kernel::Name8;
use serde::{Deserialize, Serialize};

/// One `TEXTURE<n>` lump's textures, with the WAD + lump it came from. `edited`
/// marks a project-authored or modified group (source groups stay `false` and
/// are not re-emitted on save).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextureGroup {
    pub wad_name: String,
    pub lump: Name8,
    pub defs: Vec<TextureDef>,
    #[serde(default)]
    pub edited: bool,
}
