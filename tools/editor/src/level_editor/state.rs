//! Editing values shared by map operations, picking and rendering.

pub use crate::boundary::SkillFilter;
use crate::boundary::{DrawShape, SelectMode};
pub use crate::render::input::{Overlay, SectorFill, SelItem, Selection};
use editor_core::{EditorMap, LineKey, Sector, ThingKey, VertKey};
use std::collections::HashSet;

/// Map clipboard: `fragment` = self-contained geometry, `anchor` = min-corner for paste offset, `sectors` = copied sector records applied on paste.
#[derive(Debug, Default, PartialEq)]
pub struct MapClipboard {
    pub anchor: [f32; 2],
    pub fragment: EditorMap,
    pub sectors: Vec<Sector>,
}

impl MapClipboard {
    pub fn is_empty(&self) -> bool {
        self.fragment.lines.is_empty() && self.fragment.things.is_empty() && self.sectors.is_empty()
    }
}

#[derive(Debug, PartialEq, Default)]
pub enum DragState {
    #[default]
    None,
    /// Moving selection. Positions captured at drag start; `snap(original + delta)` per element.
    MoveSel {
        start_world: [f32; 2],
        /// Last preview displacement, retained while the pointer moves over Move/Extrude.
        delta: [f32; 2],
        verts: Vec<(VertKey, [f32; 2])>,
        things: Vec<(ThingKey, [i32; 2])>,
        /// Lines touching a moved vertex (topology is stable mid-drag).
        lines: Vec<LineKey>,
    },
    /// Rubber-band selection in world coordinates, restricted to a select mode.
    Rubber { start: [f32; 2], mode: SelectMode },
    /// Rotating/scaling the selection about `pivot` via a handle drag; positions captured at drag start, map untouched until release.
    Transform {
        pivot: [f32; 2],
        mode: TransformMode,
        verts: Vec<(VertKey, [f32; 2])>,
        /// Lines touching a captured vertex (preview + mirror flip).
        lines: Vec<LineKey>,
    },
}

/// Which transform handle is being dragged.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransformMode {
    /// Free rotation; `start_angle` is pivot→cursor at drag start.
    Rotate { start_angle: f32 },
    /// Corner scale about the opposite corner, along the box's unit `axis` and its perpendicular.
    Scale {
        anchor: [f32; 2],
        start: [f32; 2],
        axis: [f32; 2],
    },
}

/// In-progress line chain. Points are overlay-only until chain finishes.
#[derive(Debug, Clone, PartialEq)]
pub struct PolyChain {
    /// Snapped points placed so far, in order.
    pub points: Vec<[f32; 2]>,
    /// Line keys present at chain start; lines not in this set were drawn by the chain.
    pub base: HashSet<LineKey>,
}

/// Two-click shape draw (rect/triangle/N-gon): click 1 anchors, click 2 commits.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ShapeDraw {
    #[default]
    None,
    Anchored {
        shape: DrawShape,
        /// Corner (Rect) or centre (Triangle/N-gon).
        anchor: [f32; 2],
    },
}

/// What a handled input event invalidated, driving renderer work on the wgpu canvas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Damage {
    None,
    /// Camera moved; grid regenerated, mesh reused.
    View,
    /// Grid rebuilt; geometry unchanged (theme/spacing/skill filter).
    Repaint,
    /// Transient overlay only (rubber band, draw, move preview).
    Overlay,
    /// The map or selection changed; the reconciler diffs against the last-synced snapshot and patches exactly the affected GPU slots/spans — every edit (value, topology, undo/redo, paste, delete) flows through here.
    Edited,
    /// Colours baked into GPU instances went stale (theme/gradient change): full resync.
    Restyle,
}

impl Damage {
    /// Merge two damages. `None` = identity; `Restyle` > `Edited` dominate; view mixes → `Repaint`.
    pub fn combine(self, other: Self) -> Self {
        match (self, other) {
            (Self::None, d) | (d, Self::None) => d,
            (Self::Restyle, _) | (_, Self::Restyle) => Self::Restyle,
            (Self::Edited, _) | (_, Self::Edited) => Self::Edited,
            _ => Self::Repaint,
        }
    }
}
