//! Texture-set editing with independent undo/redo (separate from map undo).

use editor_core::TextureDef;

use super::EditorAssets;

const UNDO_DEPTH: usize = 32;

/// Undo/redo stack for the active texture set.
#[derive(Default)]
pub(crate) struct TextureHistory {
    undo: Vec<Vec<TextureDef>>,
    redo: Vec<Vec<TextureDef>>,
}

impl EditorAssets {
    pub fn texture_mut(&mut self, index: usize) -> Option<&mut TextureDef> {
        self.textures_vec_mut().get_mut(index)
    }
}

impl TextureHistory {
    /// Snapshot before a mutation; consecutive identical states collapse.
    pub fn record(&mut self, assets: &EditorAssets) {
        let set = assets.textures();
        if self.undo.last().map(Vec::as_slice) == Some(set) {
            return;
        }
        if self.undo.len() == UNDO_DEPTH {
            self.undo.remove(0);
        }
        self.undo.push(set.to_vec());
        self.redo.clear();
    }

    /// Pop undo, push to redo; returns restored length or `None` if empty.
    pub fn undo(&mut self, assets: &mut EditorAssets) -> Option<usize> {
        let snapshot = self.undo.pop()?;
        let current = assets.replace_textures(snapshot);
        self.redo.push(current);
        Some(assets.textures().len())
    }

    /// Pop redo, push to undo; returns restored length or `None` if empty.
    pub fn redo(&mut self, assets: &mut EditorAssets) -> Option<usize> {
        let snapshot = self.redo.pop()?;
        let current = assets.replace_textures(snapshot);
        self.undo.push(current);
        Some(assets.textures().len())
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}
