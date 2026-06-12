//! Render-input value types: selection, fill mode, edit overlay.
//! Dependency-free (`std` only) so the render module stays a leaf.

/// A selectable map element, by its index in the map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelItem {
    Vertex(u32),
    Line(u32),
    Thing(u32),
    Sector(u32),
}

/// The current selection, in insertion order.
#[derive(Debug, Default, PartialEq)]
pub struct Selection(Vec<SelItem>);

impl Selection {
    pub fn contains(&self, item: &SelItem) -> bool {
        self.0.contains(item)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn items(&self) -> &[SelItem] {
        &self.0
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    pub fn replace(&mut self, item: SelItem) {
        self.0.clear();
        self.0.push(item);
    }

    pub fn push(&mut self, item: SelItem) {
        if !self.contains(&item) {
            self.0.push(item);
        }
    }

    pub fn toggle(&mut self, item: SelItem) {
        if let Some(at) = self.0.iter().position(|i| *i == item) {
            self.0.remove(at);
        } else {
            self.0.push(item);
        }
    }

    /// Keep only items for which `keep` returns true.
    pub fn retain(&mut self, keep: impl Fn(&SelItem) -> bool) {
        self.0.retain(|i| keep(i));
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
    },
    /// Open polyline plus optional rubber segment to cursor.
    Chain {
        pts: Vec<[f32; 2]>,
        rubber: Option<[f32; 2]>,
    },
    /// Selection move preview at delta-offset positions (map unmutated until release).
    Move {
        segments: Vec<[[f32; 2]; 2]>,
        points: Vec<[f32; 2]>,
    },
}
