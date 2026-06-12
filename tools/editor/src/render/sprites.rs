//! Decoded thing-icon sprites as straight RGBA8. Decoding lives in `gfx`.

use std::collections::HashMap;

pub struct SpriteRgba {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Per-kind icon cache. `None` = looked up, no sprite (not retried).
#[derive(Default)]
pub struct ThingSpriteCache {
    by_kind: HashMap<i32, Option<SpriteRgba>>,
}

impl ThingSpriteCache {
    pub fn contains(&self, kind: i32) -> bool {
        self.by_kind.contains_key(&kind)
    }

    pub fn insert(&mut self, kind: i32, sprite: Option<SpriteRgba>) {
        self.by_kind.insert(kind, sprite);
    }

    pub fn get(&self, kind: i32) -> Option<&SpriteRgba> {
        self.by_kind.get(&kind)?.as_ref()
    }
}
