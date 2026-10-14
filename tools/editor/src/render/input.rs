//! Render-input value types: selection, fill mode, edit overlay.

use editor_core::{LineKey, SectorKey, ThingKey, VertKey};

/// A selectable map element, by its stable arena key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SelItem {
    Vertex(VertKey),
    Line(LineKey),
    Thing(ThingKey),
    Sector(SectorKey),
}

/// The current selection, in insertion order.
#[derive(Debug, Clone, Default)]
pub struct Selection {
    items: Vec<SelItem>,
    revision: u64,
}

impl PartialEq for Selection {
    fn eq(&self, other: &Self) -> bool {
        self.items == other.items
    }
}

impl Selection {
    pub fn contains(&self, item: &SelItem) -> bool {
        self.items.contains(item)
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn items(&self) -> &[SelItem] {
        &self.items
    }

    /// Identifies membership changes without copying the selected items into UI fields.
    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    pub fn clear(&mut self) {
        if !self.items.is_empty() {
            self.items.clear();
            self.revision += 1;
        }
    }

    pub fn replace(&mut self, item: SelItem) {
        if self.items.as_slice() != [item] {
            self.items.clear();
            self.items.push(item);
            self.revision += 1;
        }
    }

    pub fn push(&mut self, item: SelItem) {
        if !self.contains(&item) {
            self.items.push(item);
            self.revision += 1;
        }
    }

    pub fn toggle(&mut self, item: SelItem) {
        if let Some(at) = self.items.iter().position(|i| *i == item) {
            self.items.remove(at);
        } else {
            self.items.push(item);
        }
        self.revision += 1;
    }

    /// Keep only items for which `keep` returns true.
    pub fn retain(&mut self, keep: impl Fn(&SelItem) -> bool) {
        let previous_len = self.items.len();
        self.items.retain(keep);
        if previous_len != self.items.len() {
            self.revision += 1;
        }
    }
}

/// How sector interiors paint on the canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SectorFill {
    #[default]
    None,
    Colour,
    Texture,
}

/// Transient edit preview (world coords) drawn on the GPU overlay layer.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Overlay {
    #[default]
    None,
    Rubber {
        a: [f32; 2],
        b: [f32; 2],
    },
    /// A closed shape preview (rect/triangle/N-gon), vertices in world coords.
    Poly {
        pts: Vec<[f32; 2]>,
        z: i32,
    },
    /// Open polyline plus optional rubber segment to cursor.
    Chain {
        pts: Vec<[f32; 2]>,
        rubber: Option<[f32; 2]>,
        z: i32,
    },
    /// Selection move preview at delta-offset positions (map unmutated until release).
    Move {
        segments: Vec<[[f32; 3]; 2]>,
        points: Vec<[f32; 3]>,
    },
}
