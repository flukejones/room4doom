//! Transform handles: rotate/scale the selection about its bbox pivot by dragging corner/rotate handles, plus the exact menu transforms (rotate 90°, mirror). Positions stay uncommitted until release; the commit runs through the full move pipeline.

use std::collections::HashSet;
use std::f32::consts::PI;

use editor_core::geom::{derive_thing_heights, resolve_vertex_moves};
use editor_core::{Axis, LineKey, VertKey, mirror_fixup, move_vertices, transform_moves};

use super::{LevelEditorState, ON_SEGMENT_TOL_PX, default_sector};
use crate::boundary::Tool;
use crate::level_editor::state::{Damage, DragState, Overlay, SelItem, TransformMode};
use crate::undo::EditAction;

/// Handle hit radius (screen px).
const HANDLE_HIT_PX: f32 = 8.0;
/// Rotate handle offset beyond the selection box edge (screen px).
const ROTATE_HANDLE_PX: f32 = 24.0;
/// Keep the line's selection outline clear of its stroke and endpoint markers.
const LINE_PADDING_PX: f32 = 8.0;
/// Below this world span an axis is degenerate: scale locks to 1 on it.
const DEGENERATE_SPAN: f32 = 1e-3;
/// Rotation quantisation when angle snap is on (15°).
const ROTATE_SNAP_STEP: f32 = PI / 12.0;

/// Handle layout for the current selection, in world coordinates.
pub struct TransformHandles {
    pub corners: [[f32; 2]; 4],
    pub pivot: [f32; 2],
    pub rotate: [f32; 2],
}

impl LevelEditorState {
    /// A padded, line-aligned box for a single line; a shared box for larger selections.
    pub fn transform_handles(&self) -> Option<TransformHandles> {
        if !matches!(self.drag, DragState::None) {
            return None;
        }
        let map = self.map.as_ref()?;
        let zoom = self.camera.zoom_level();
        let offset = ROTATE_HANDLE_PX / zoom;
        if let [SelItem::Line(key)] = self.selection.items() {
            let line = map.lines.get(*key)?;
            let a = map.vertices.get(line.v1)?;
            let b = map.vertices.get(line.v2)?;
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            let length = dx.hypot(dy);
            if !length.is_finite() || length < DEGENERATE_SPAN {
                return None;
            }
            let tangent = [dx / length, dy / length];
            // Doom's front normal points right of v1 -> v2, like the rendered normal tick.
            let normal = [tangent[1], -tangent[0]];
            let pad = LINE_PADDING_PX / zoom;
            let corner = |p: [f32; 2], along: f32, across: f32| {
                [
                    p[0] + pad * (along * tangent[0] + across * normal[0]),
                    p[1] + pad * (along * tangent[1] + across * normal[1]),
                ]
            };
            let pivot = [a.x.midpoint(b.x), a.y.midpoint(b.y)];
            return Some(TransformHandles {
                corners: [
                    corner([a.x, a.y], -1.0, -1.0),
                    corner([b.x, b.y], 1.0, -1.0),
                    corner([b.x, b.y], 1.0, 1.0),
                    corner([a.x, a.y], -1.0, 1.0),
                ],
                pivot,
                rotate: [
                    pivot[0] + normal[0] * (pad + offset),
                    pivot[1] + normal[1] * (pad + offset),
                ],
            });
        }
        if !matches!(self.tool, Tool::Select(_)) {
            return None;
        }
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
        Some(TransformHandles {
            corners: [min, [max[0], min[1]], max, [min[0], max[1]]],
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
        let corners = h.corners;
        let mode = if near(h.rotate) {
            let w = self.screen_to_world(pos);
            TransformMode::Rotate {
                start_angle: (w[1] - h.pivot[1]).atan2(w[0] - h.pivot[0]),
            }
        } else if let Some(i) = (0..4).find(|&i| near(corners[i])) {
            let edge = [corners[1][0] - corners[0][0], corners[1][1] - corners[0][1]];
            let length = edge[0].hypot(edge[1]);
            TransformMode::Scale {
                anchor: corners[(i + 2) % 4],
                start: corners[i],
                axis: if length < DEGENERATE_SPAN {
                    [1.0, 0.0]
                } else {
                    [edge[0] / length, edge[1] / length]
                },
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
        let lines = map
            .lines
            .iter()
            .filter(|(_, l)| vert_set.contains(&l.v1) || vert_set.contains(&l.v2))
            .map(|(k, _)| k)
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
                axis,
            } => {
                let axis_scale = |direction: [f32; 2]| {
                    let span = (start[0] - anchor[0]) * direction[0]
                        + (start[1] - anchor[1]) * direction[1];
                    if span.abs() < DEGENERATE_SPAN {
                        1.0
                    } else {
                        ((world[0] - anchor[0]) * direction[0]
                            + (world[1] - anchor[1]) * direction[1])
                            / span
                    }
                };
                (
                    anchor,
                    0.0,
                    [axis_scale(axis), axis_scale([-axis[1], axis[0]])],
                )
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
            mode,
            ..
        } = &self.drag
        else {
            return Vec::new();
        };
        let (s, c) = rot.sin_cos();
        let axis = match mode {
            TransformMode::Scale {
                axis,
                ..
            } => *axis,
            TransformMode::Rotate {
                ..
            } => [1.0, 0.0],
        };
        let normal = [-axis[1], axis[0]];
        let mut moves: Vec<_> = verts
            .iter()
            .map(|&(k, p)| {
                let relative = [p[0] - centre[0], p[1] - centre[1]];
                let along = (relative[0] * axis[0] + relative[1] * axis[1]) * scale[0];
                let across = (relative[0] * normal[0] + relative[1] * normal[1]) * scale[1];
                let d = [
                    along * axis[0] + across * normal[0],
                    along * axis[1] + across * normal[1],
                ];
                (
                    k,
                    [
                        centre[0] + d[0] * c - d[1] * s,
                        centre[1] + d[0] * s + d[1] * c,
                    ],
                )
            })
            .collect();
        if let Some(map) = &self.map {
            moves.retain(|&(key, p)| map.vertices.get(key).is_some_and(|v| [v.x, v.y] != p));
            resolve_vertex_moves(
                map,
                &mut moves,
                ON_SEGMENT_TOL_PX / self.camera.zoom_level(),
            );
        }
        moves
    }

    /// Live preview of the handle drag (map untouched).
    pub(super) fn transform_drag_to(&mut self, world: [f32; 2]) -> Damage {
        let Some((centre, rot, scale)) = self.transform_params(world) else {
            return Damage::None;
        };
        let moved = self.transformed_verts(centre, rot, scale);
        let DragState::Transform {
            lines,
            ..
        } = &self.drag
        else {
            return Damage::None;
        };
        self.overlay = self.vertex_move_overlay(&moved, lines);
        Damage::Overlay
    }

    /// Commit the handle drag through the move pipeline; a mirroring scale flips the fully-contained lines.
    pub(super) fn finish_transform(&mut self, world: [f32; 2]) -> Damage {
        self.overlay = Overlay::None;
        let Some((centre, rot, scale)) = self.transform_params(world) else {
            self.drag = DragState::None;
            return Damage::None;
        };
        let moves = self.transformed_verts(centre, rot, scale);
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
