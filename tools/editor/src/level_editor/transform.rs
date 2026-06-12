//! Transform handles: rotate/scale the selection about its bbox pivot by dragging corner/rotate handles, plus the exact menu transforms (rotate 90°, mirror). Positions stay uncommitted until release; the commit runs through the full move pipeline.

use std::collections::{HashMap, HashSet};
use std::f32::consts::PI;

use editor_core::geom::derive_thing_heights;
use editor_core::{Axis, LineKey, VertKey, mirror_fixup, move_vertices, transform_moves};

use super::{LevelEditorState, ON_SEGMENT_TOL_PX, default_sector};
use crate::boundary::Tool;
use crate::state::{Damage, DragState, Overlay, TransformMode};
use crate::undo::EditAction;

/// Handle hit radius (screen px).
const HANDLE_HIT_PX: f32 = 8.0;
/// Rotate handle offset above the bbox top edge (screen px).
const ROTATE_HANDLE_PX: f32 = 24.0;
/// Below this world span an axis is degenerate: scale locks to 1 on it.
const DEGENERATE_SPAN: f32 = 1e-3;
/// Rotation quantisation when angle snap is on (15°).
const ROTATE_SNAP_STEP: f32 = PI / 12.0;

/// Handle layout for the current selection, in world coordinates.
pub struct TransformHandles {
    pub min: [f32; 2],
    pub max: [f32; 2],
    pub pivot: [f32; 2],
    pub rotate: [f32; 2],
}

impl TransformHandles {
    /// The four bbox corners, x-then-y order: min, (max,min), max, (min,max).
    pub fn corners(&self) -> [[f32; 2]; 4] {
        [
            self.min,
            [self.max[0], self.min[1]],
            self.max,
            [self.min[0], self.max[1]],
        ]
    }
}

impl LevelEditorState {
    /// Handle layout when the Select tool has a multi-vertex selection at rest; `None` hides the handles.
    pub fn transform_handles(&self) -> Option<TransformHandles> {
        if !matches!(self.tool, Tool::Select(_)) || !matches!(self.drag, DragState::None) {
            return None;
        }
        let map = self.map.as_ref()?;
        let verts = self.selection_vertex_set();
        if verts.len() < 2 {
            return None;
        }
        let mut min = [f32::INFINITY; 2];
        let mut max = [f32::NEG_INFINITY; 2];
        for &k in &verts {
            let v = map.vertices.get(k)?;
            min = [min[0].min(v.x), min[1].min(v.y)];
            max = [max[0].max(v.x), max[1].max(v.y)];
        }
        let pivot = [min[0].midpoint(max[0]), min[1].midpoint(max[1])];
        let offset = ROTATE_HANDLE_PX / self.camera.zoom_level();
        Some(TransformHandles {
            min,
            max,
            pivot,
            rotate: [pivot[0], max[1] + offset],
        })
    }

    /// Start a handle drag when `pos` (screen px) hits one; returns whether the gesture was claimed.
    pub(super) fn begin_transform_drag(&mut self, pos: [f32; 2]) -> bool {
        let Some(h) = self.transform_handles() else {
            return false;
        };
        let near = |p: [f32; 2]| {
            let s = self.camera.world_to_screen(p);
            (s[0] - pos[0]).hypot(s[1] - pos[1]) <= HANDLE_HIT_PX
        };
        let corners = h.corners();
        let mode = if near(h.rotate) {
            let w = self.screen_to_world(pos);
            TransformMode::Rotate {
                start_angle: (w[1] - h.pivot[1]).atan2(w[0] - h.pivot[0]),
            }
        } else if let Some(i) = (0..4).find(|&i| near(corners[i])) {
            TransformMode::Scale {
                anchor: corners[(i + 2) % 4],
                start: corners[i],
            }
        } else {
            return false;
        };
        let Some(map) = &self.map else {
            return false;
        };
        let vert_keys = self.selection_vertex_set();
        let vert_set: HashSet<VertKey> = vert_keys.iter().copied().collect();
        let verts: Vec<(VertKey, [f32; 2])> = vert_keys
            .iter()
            .filter_map(|&k| map.vertices.get(k).map(|v| (k, [v.x, v.y])))
            .collect();
        let lines: Vec<[VertKey; 2]> = map
            .lines
            .values()
            .filter(|l| vert_set.contains(&l.v1) || vert_set.contains(&l.v2))
            .map(|l| [l.v1, l.v2])
            .collect();
        self.drag = DragState::Transform {
            pivot: h.pivot,
            mode,
            verts,
            lines,
        };
        true
    }

    /// The in-flight drag's fixed point, rotation, and scale for the cursor at `world`: rotation is about the pivot, a corner scale is about its anchor corner.
    fn transform_params(&self, world: [f32; 2]) -> Option<([f32; 2], f32, [f32; 2])> {
        let DragState::Transform {
            pivot,
            mode,
            ..
        } = &self.drag
        else {
            return None;
        };
        Some(match *mode {
            TransformMode::Rotate {
                start_angle,
            } => {
                let angle = (world[1] - pivot[1]).atan2(world[0] - pivot[0]);
                let mut delta = angle - start_angle;
                if self.angle_snap {
                    delta = (delta / ROTATE_SNAP_STEP).round() * ROTATE_SNAP_STEP;
                }
                (*pivot, delta, [1.0, 1.0])
            }
            TransformMode::Scale {
                anchor,
                start,
            } => {
                let axis_scale = |i: usize| {
                    let span = start[i] - anchor[i];
                    if span.abs() < DEGENERATE_SPAN {
                        1.0
                    } else {
                        (world[i] - anchor[i]) / span
                    }
                };
                (anchor, 0.0, [axis_scale(0), axis_scale(1)])
            }
        })
    }

    /// Transformed positions of the captured vertices for the given params.
    fn transformed_verts(
        &self,
        centre: [f32; 2],
        rot: f32,
        scale: [f32; 2],
    ) -> Vec<(VertKey, [f32; 2])> {
        let DragState::Transform {
            verts,
            ..
        } = &self.drag
        else {
            return Vec::new();
        };
        let (s, c) = rot.sin_cos();
        verts
            .iter()
            .map(|&(k, p)| {
                let d = [(p[0] - centre[0]) * scale[0], (p[1] - centre[1]) * scale[1]];
                (
                    k,
                    [
                        centre[0] + d[0] * c - d[1] * s,
                        centre[1] + d[0] * s + d[1] * c,
                    ],
                )
            })
            .collect()
    }

    /// Live preview of the handle drag (map untouched).
    pub(super) fn transform_drag_to(&mut self, world: [f32; 2]) -> Damage {
        let Some((centre, rot, scale)) = self.transform_params(world) else {
            return Damage::None;
        };
        let moved = self.transformed_verts(centre, rot, scale);
        let at: HashMap<VertKey, [f32; 2]> = moved.iter().copied().collect();
        let DragState::Transform {
            lines,
            ..
        } = &self.drag
        else {
            return Damage::None;
        };
        let Some(map) = &self.map else {
            return Damage::None;
        };
        let pos = |v: VertKey| {
            at.get(&v)
                .copied()
                .or_else(|| map.vertices.get(v).map(|p| [p.x, p.y]))
        };
        let mut segments = Vec::with_capacity(lines.len());
        for &[v1, v2] in lines {
            if let (Some(a), Some(b)) = (pos(v1), pos(v2)) {
                segments.push([a, b]);
            }
        }
        self.overlay = Overlay::Move {
            segments,
            points: moved.iter().map(|(_, p)| *p).collect(),
        };
        Damage::Overlay
    }

    /// Commit the handle drag through the move pipeline; a mirroring scale flips the fully-contained lines.
    pub(super) fn finish_transform(&mut self, world: [f32; 2]) -> Damage {
        self.overlay = Overlay::None;
        let Some((centre, rot, scale)) = self.transform_params(world) else {
            self.drag = DragState::None;
            return Damage::None;
        };
        let moves: Vec<(VertKey, [f32; 2])> = self
            .transformed_verts(centre, rot, scale)
            .into_iter()
            .filter(|&(k, p)| {
                self.map
                    .as_ref()
                    .and_then(|m| m.vertices.get(k))
                    .is_some_and(|v| [v.x, v.y] != p)
            })
            .collect();
        self.drag = DragState::None;
        if moves.is_empty() {
            return Damage::Overlay;
        }
        self.apply_transform_moves(&moves, scale[0] * scale[1] < 0.0)
    }

    /// One undo step: move through the kernel pipeline, flip fully-moved lines on a mirror, re-derive thing heights.
    fn apply_transform_moves(&mut self, moves: &[(VertKey, [f32; 2])], mirrored: bool) -> Damage {
        let tol = ON_SEGMENT_TOL_PX / self.camera.zoom_level();
        let moved_set: HashSet<VertKey> = moves.iter().map(|(k, _)| *k).collect();
        let Some(map) = &mut self.map else {
            return Damage::None;
        };
        self.undo.record(EditAction::Transform, map);
        let contained: Vec<LineKey> = map
            .lines
            .iter()
            .filter(|(_, l)| moved_set.contains(&l.v1) && moved_set.contains(&l.v2))
            .map(|(k, _)| k)
            .collect();
        move_vertices(map, moves, &[], tol, default_sector());
        if mirrored {
            let survivors: Vec<LineKey> = contained
                .into_iter()
                .filter(|&k| map.lines.contains(k))
                .collect();
            mirror_fixup(map, &survivors);
        }
        derive_thing_heights(map);
        self.dirty = true;
        Damage::Edited
    }

    pub fn can_transform(&self) -> bool {
        self.selection_vertex_set().len() >= 2
    }

    /// Rotate the selection exactly 90° about its bbox centre (`cw` = clockwise in Y-up).
    pub fn rotate_selected_90(&mut self, cw: bool) -> Damage {
        let Some((pivot, verts)) = self.menu_transform_input() else {
            return Damage::None;
        };
        let map = self.map.as_ref().expect("input checked");
        let moves: Vec<(VertKey, [f32; 2])> = verts
            .iter()
            .filter_map(|&k| {
                let v = map.vertices.get(k)?;
                let d = [v.x - pivot[0], v.y - pivot[1]];
                let q = if cw {
                    [pivot[0] + d[1], pivot[1] - d[0]]
                } else {
                    [pivot[0] - d[1], pivot[1] + d[0]]
                };
                ([v.x, v.y] != q).then_some((k, q))
            })
            .collect();
        if moves.is_empty() {
            return Damage::None;
        }
        self.apply_transform_moves(&moves, false)
    }

    /// Mirror the selection across its bbox centre, negating `axis`.
    pub fn mirror_selected(&mut self, axis: Axis) -> Damage {
        let Some((pivot, verts)) = self.menu_transform_input() else {
            return Damage::None;
        };
        let map = self.map.as_ref().expect("input checked");
        let scale = match axis {
            Axis::X => [-1.0, 1.0],
            Axis::Y => [1.0, -1.0],
        };
        let moves = transform_moves(map, &verts, pivot, 0.0, scale);
        if moves.is_empty() {
            return Damage::None;
        }
        self.apply_transform_moves(&moves, true)
    }

    /// Pivot (bbox centre) and vertex set for a menu transform; `None` below two vertices.
    fn menu_transform_input(&self) -> Option<([f32; 2], Vec<VertKey>)> {
        let map = self.map.as_ref()?;
        let verts = self.selection_vertex_set();
        if verts.len() < 2 {
            return None;
        }
        let mut min = [f32::INFINITY; 2];
        let mut max = [f32::NEG_INFINITY; 2];
        for &k in &verts {
            let v = map.vertices.get(k)?;
            min = [min[0].min(v.x), min[1].min(v.y)];
            max = [max[0].max(v.x), max[1].max(v.y)];
        }
        Some(([min[0].midpoint(max[0]), min[1].midpoint(max[1])], verts))
    }
}
