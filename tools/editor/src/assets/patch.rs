//! Editor-native picture types ([`WallPic`]/[`FlatPic`]) and patch import; no `pic-data` dep.

use editor_core::ImportedPatch;

use super::EditorAssets;

/// Wall texture as palette indices, column-major (`data[x*h + y]`); `u16::MAX` = transparent.
pub struct WallPic {
    pub data: Vec<u16>,
    pub width: usize,
    pub height: usize,
}

/// A 64×64 flat as palette indices, row-major.
pub struct FlatPic {
    pub data: [u16; 64 * 64],
    pub width: usize,
    pub height: usize,
}

impl FlatPic {
    /// Decode raw flat lump (64×64 palette indices, row-major); short lumps zero-padded.
    pub fn from_lump(lump: &[u8]) -> Self {
        let mut data = [0u16; 64 * 64];
        for (slot, &b) in data.iter_mut().zip(lump.iter()) {
            *slot = u16::from(b);
        }
        Self {
            data,
            width: 64,
            height: 64,
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
