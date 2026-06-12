//! Texture-set editing with independent undo/redo (separate from map undo).

use editor_core::{AnimDef, TextureDef};

use super::EditorAssets;

const UNDO_DEPTH: usize = 32;

/// One undo step: the active group's textures plus the animation set.
struct TexSnapshot {
    textures: Vec<TextureDef>,
    animations: Vec<AnimDef>,
}

/// Undo/redo stack for the active texture set + animations.
#[derive(Default)]
pub(crate) struct TextureHistory {
    undo: Vec<TexSnapshot>,
    redo: Vec<TexSnapshot>,
}

impl EditorAssets {
    pub fn texture_mut(&mut self, index: usize) -> Option<&mut TextureDef> {
        self.textures_vec_mut().get_mut(index)
    }
}

impl TextureHistory {
    /// Snapshot before a mutation; consecutive identical states collapse.
    pub fn record(&mut self, assets: &EditorAssets) {
        let same = self.undo.last().is_some_and(|s| {
            s.textures.as_slice() == assets.textures()
                && s.animations.as_slice() == assets.animations()
        });
        if same {
            return;
        }
        if self.undo.len() == UNDO_DEPTH {
            self.undo.remove(0);
        }
        self.undo.push(TexSnapshot {
            textures: assets.textures().to_vec(),
            animations: assets.animations().to_vec(),
        });
        self.redo.clear();
    }

    /// Pop undo, push to redo; returns restored texture count or `None` if empty.
    pub fn undo(&mut self, assets: &mut EditorAssets) -> Option<usize> {
        let snapshot = self.undo.pop()?;
        self.redo.push(swap_in(assets, snapshot));
        Some(assets.textures().len())
    }

    /// Pop redo, push to undo; returns restored texture count or `None` if empty.
    pub fn redo(&mut self, assets: &mut EditorAssets) -> Option<usize> {
        let snapshot = self.redo.pop()?;
        self.undo.push(swap_in(assets, snapshot));
        Some(assets.textures().len())
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}

/// Install `snapshot`, returning the displaced state.
fn swap_in(assets: &mut EditorAssets, snapshot: TexSnapshot) -> TexSnapshot {
    TexSnapshot {
        textures: assets.replace_textures(snapshot.textures),
        animations: assets.replace_animations(snapshot.animations),
    }
}
