//! `RemapKind` Rust↔Slint conversions.

use crate::generated;
use crate::level_editor::remap::RemapKind;

impl From<RemapKind> for generated::RemapKind {
    fn from(k: RemapKind) -> Self {
        match k {
            RemapKind::Thing => Self::Thing,
            RemapKind::Texture => Self::Texture,
            RemapKind::Flat => Self::Flat,
            RemapKind::LineSpecial => Self::LineSpecial,
            RemapKind::SectorSpecial => Self::SectorSpecial,
        }
    }
}

impl From<generated::RemapKind> for RemapKind {
    fn from(k: generated::RemapKind) -> Self {
        match k {
            generated::RemapKind::Thing => Self::Thing,
            generated::RemapKind::Texture => Self::Texture,
            generated::RemapKind::Flat => Self::Flat,
            generated::RemapKind::LineSpecial => Self::LineSpecial,
            generated::RemapKind::SectorSpecial => Self::SectorSpecial,
        }
    }
}
