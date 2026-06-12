//! Editor-native picture types ([`WallPic`]/[`FlatPic`]) and patch import; no `pic-data` dep.

use editor_core::ImportedPatch;

use super::EditorAssets;

/// Vanilla flat side; all Doom flats are 64×64.
pub const FLAT_SIDE: usize = 64;

/// Wall texture as palette indices, column-major (`data[x*h + y]`); `u16::MAX` = transparent.
pub struct WallPic {
    pub data: Vec<u16>,
    pub width: usize,
    pub height: usize,
}

/// A [`FLAT_SIDE`]² flat as palette indices, row-major.
pub struct FlatPic {
    pub data: [u16; FLAT_SIDE * FLAT_SIDE],
    pub width: usize,
    pub height: usize,
}

impl FlatPic {
    /// Decode raw flat lump (palette indices, row-major); short lumps zero-padded.
    pub fn from_lump(lump: &[u8]) -> Self {
        let mut data = [0u16; FLAT_SIDE * FLAT_SIDE];
        for (slot, &b) in data.iter_mut().zip(lump.iter()) {
            *slot = u16::from(b);
        }
        Self {
            data,
            width: FLAT_SIDE,
            height: FLAT_SIDE,
        }
    }
}

impl EditorAssets {
    /// Add an imported patch; errors if name already imported.
    pub fn import_patch(&mut self, patch: ImportedPatch) -> Result<(), &'static str> {
        if self
            .imported_patches_slice()
            .iter()
            .any(|p| p.name == patch.name)
        {
            return Err("a patch with that name is already imported");
        }
        self.imported_patches_push(patch);
        Ok(())
    }
}
