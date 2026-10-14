//! Moving the selection: capturing drag-start positions, the live move preview, and committing the move (delegated to the kernel) on release — or, for a single dragged line, holding for the Move/Extrude choice.

use std::collections::{HashMap, HashSet};

use editor_core::geom::{resolve_vertex_moves, thing_floor_z};
use editor_core::ops::extrude_line_to;
use editor_core::{LineKey, SectorKey, ThingKey, VertKey, move_vertices};

use super::{LevelEditorState, ON_SEGMENT_TOL_PX};
use crate::level_editor::draw::default_sector;
use crate::level_editor::state::{Damage, DragState, Overlay, SelItem};
use crate::render::view::snap;
use crate::undo::EditAction;

/// Padding around the containment-change region when re-deriving thing Z.
const THING_Z_REGION_PAD: f32 = 1.0;

struct MoveTargets {
    vertices: Vec<(VertKey, [f32; 2])>,
    things: Vec<(ThingKey, [i32; 3])>,
}

impl LevelEditorState {
    pub(super) fn begin_move(&mut self, world: [f32; 2]) {
        let Some(map) = &self.map else { return };
        let mut vert_set: HashSet<VertKey> = HashSet::new();
        let mut things = Vec::new();
        for item in self.selection.items() {
            match *item {
                SelItem::Vertex(k) => {
                    vert_set.insert(k);
                }
                SelItem::Line(k) => {
                    if let Some(line) = map.lines.get(k) {
                        vert_set.insert(line.v1);
                        vert_set.insert(line.v2);
                    }
                }
                SelItem::Thing(k) => {
                    if let Some(t) = map.things.get(k) {
                        things.push((k, [t.x, t.y]));
                    }
                }
                SelItem::Sector(_) => {}
            }
        }
        let lines = map
            .lines
            .iter()
            .filter(|(_, l)| vert_set.contains(&l.v1) || vert_set.contains(&l.v2))
            .map(|(k, _)| k)
            .collect();
        let verts = vert_set
            .into_iter()
            .filter_map(|k| map.vertices.get(k).map(|v| (k, [v.x, v.y])))
            .collect();
        self.drag = DragState::MoveSel {
            start_world: world,
            delta: [0.0, 0.0],
            verts,
            things,
            lines,
        };
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "map and viewport coordinates deliberately use float precision; GPU and image records retain their packed widths"
    )]
    pub(super) fn finish_move(&mut self) -> Damage {
        self.overlay = Overlay::None;
        let DragState::MoveSel {
            delta,
            verts,
            ..
        } = &self.drag
        else {
            return Damage::None;
        };
        let delta = *delta;
        if delta == [0.0, 0.0] {
            return Damage::None;
        }
        let Some(targets) = self.move_targets() else {
            return Damage::None;
        };
        let MoveTargets {
            vertices: moves,
            things: resolved_things,
        } = targets;
        let thing_moves: Vec<_> = resolved_things
            .iter()
            .map(|&(k, p)| (k, [p[0], p[1]]))
            .collect();
        let tol = ON_SEGMENT_TOL_PX / self.camera.zoom_level();
        // Containment can change anywhere inside a touched sector (a deleted divider merges whole rooms): the Z re-derive region spans the FULL extents of every sector bordering a moved line, plus whatever the kernel re-sectors.
        let mut region: Vec<[f32; 2]> = Vec::new();
        if let Some(map) = &self.map {
            let moved_verts: HashSet<VertKey> = moves.iter().map(|(k, _)| *k).collect();
            let mut bordering: HashSet<SectorKey> = HashSet::new();
            for line in map.lines.values() {
                if moved_verts.contains(&line.v1) || moved_verts.contains(&line.v2) {
                    bordering.extend(line.sides().filter_map(|s| s.sector));
                }
            }
            for line in map.lines.values() {
                if line
                    .sides()
                    .any(|s| s.sector.is_some_and(|k| bordering.contains(&k)))
                {
                    for v in [line.v1, line.v2] {
                        if let Some(p) = map.vertices.get(v) {
                            region.push([p.x, p.y]);
                        }
                    }
                }
            }
        }
        // Old and new positions of the moved vertices cover sector-less chains.
        region.extend(verts.iter().map(|(_, o)| *o));
        region.extend(moves.iter().map(|(_, p)| *p));
        let moved_things: HashSet<ThingKey> = thing_moves.iter().map(|(k, _)| *k).collect();
        let Some(map) = &mut self.map else {
            return Damage::None;
        };
        self.undo.record(EditAction::MoveSelection, map);
        let newly = move_vertices(map, &moves, &thing_moves, tol, default_sector());
        for k in newly {
            if let Some(line) = map.lines.get(k) {
                for v in [line.v1, line.v2] {
                    if let Some(p) = map.vertices.get(v) {
                        region.push([p.x, p.y]);
                    }
                }
            }
        }
        let lo = region
            .iter()
            .fold([f32::INFINITY; 2], |a, p| [a[0].min(p[0]), a[1].min(p[1])]);
        let hi = region.iter().fold([f32::NEG_INFINITY; 2], |a, p| {
            [a[0].max(p[0]), a[1].max(p[1])]
        });
        let pad = tol.max(THING_Z_REGION_PAD);
        let targets: Vec<(ThingKey, [f32; 2])> = map
            .things
            .iter()
            .filter(|(k, t)| {
                let (x, y) = (t.x as f32, t.y as f32);
                (moved_things.contains(k) && !moves.is_empty())
                    || (!moves.is_empty()
                        && x >= lo[0] - pad
                        && x <= hi[0] + pad
                        && y >= lo[1] - pad
                        && y <= hi[1] + pad)
            })
            .map(|(k, t)| (k, [t.x as f32, t.y as f32]))
            .collect();
        for (k, p) in targets {
            let z = thing_floor_z(map, p);
            if let Some(t) = map.things.get_mut(k) {
                t.z = z;
            }
        }
        // For thing-only moves, use the same 3D floor hit displayed by the preview.
        if moves.is_empty() {
            for (key, position) in resolved_things {
                if let Some(thing) = map.things.get_mut(key) {
                    thing.z = position[2];
                }
            }
        }
        self.dirty = true;
        Damage::Edited
    }

    /// Last preview displacement of the in-flight move; `[0,0]` when no move drag.
    fn move_drag_delta(&self) -> [f32; 2] {
        let DragState::MoveSel {
            delta,
            ..
        } = &self.drag
        else {
            return [0.0, 0.0];
        };
        *delta
    }

    /// The dragged line when the move gesture holds exactly one selected line and actually moved.
    pub fn line_drag_pending(&self) -> Option<LineKey> {
        if !matches!(self.drag, DragState::MoveSel { .. }) || self.move_drag_delta() == [0.0, 0.0] {
            return None;
        }
        match self.selection.items() {
            &[SelItem::Line(k)] => Some(k),
            _ => None,
        }
    }

    /// Resolve a held line drag as a plain move.
    pub fn commit_pending_move(&mut self) -> Damage {
        let damage = self.finish_move();
        self.drag = DragState::None;
        damage
    }

    /// Resolve a held line drag by extruding the line along the drag delta; the map was untouched during the drag, so the quad grows from the original position.
    pub fn commit_pending_extrude(&mut self) -> Damage {
        let Some(line) = self.line_drag_pending() else {
            return Damage::None;
        };
        let Some(targets) = self.move_targets() else {
            return Damage::None;
        };
        let Some(map) = &self.map else {
            return Damage::None;
        };
        let wall = map.lines[line];
        let endpoint = |key| {
            targets
                .vertices
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, p)| *p)
        };
        let (Some(a), Some(b)) = (endpoint(wall.v1), endpoint(wall.v2)) else {
            return Damage::None;
        };
        self.drag = DragState::None;
        self.overlay = Overlay::None;
        let tol = ON_SEGMENT_TOL_PX / self.camera.zoom_level();
        let sector = self.draw_brush.sector();
        let Some(map) = &mut self.map else {
            return Damage::None;
        };
        self.undo.record(EditAction::Extrude, map);
        let new = extrude_line_to(map, line, [a, b], tol, sector);
        let Some(&far) = new.get(1).or_else(|| new.first()) else {
            self.undo.discard_last();
            return Damage::Overlay;
        };
        self.selection.replace(SelItem::Line(far));
        self.dirty = true;
        Damage::Edited
    }

    /// Drop a held line drag without committing; the map was never mutated.
    pub fn cancel_pending_drag(&mut self) -> Damage {
        self.drag = DragState::None;
        self.overlay = Overlay::None;
        Damage::Overlay
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "map and viewport coordinates deliberately use float precision; GPU and image records retain their packed widths"
    )]
    pub(super) fn move_selection_to(&mut self, world: [f32; 2]) -> Damage {
        let DragState::MoveSel {
            start_world,
            delta: previous_delta,
            ..
        } = &mut self.drag
        else {
            return Damage::None;
        };
        let delta = [world[0] - start_world[0], world[1] - start_world[1]];
        if delta == *previous_delta {
            return Damage::None;
        }
        *previous_delta = delta;
        let Some(targets) = self.move_targets() else {
            return Damage::None;
        };
        let DragState::MoveSel {
            lines,
            ..
        } = &self.drag
        else {
            return Damage::None;
        };
        let mut overlay = self.vertex_move_overlay(&targets.vertices, lines);
        if let Overlay::Move {
            points,
            ..
        } = &mut overlay
        {
            points.extend(targets.things.iter().map(|(_, p)| p.map(|c| c as f32)));
        }
        self.overlay = overlay;
        Damage::Overlay
    }

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        reason = "map thing coordinates are stored as integers"
    )]
    fn move_targets(&self) -> Option<MoveTargets> {
        let map = self.map.as_ref()?;
        let DragState::MoveSel {
            delta,
            verts,
            things,
            ..
        } = &self.drag
        else {
            return None;
        };
        let moved = |p: [f32; 2]| {
            [
                snap(p[0] + delta[0], self.grid),
                snap(p[1] + delta[1], self.grid),
            ]
        };
        let mut vertices: Vec<_> = verts.iter().map(|&(key, p)| (key, moved(p))).collect();
        resolve_vertex_moves(
            map,
            &mut vertices,
            ON_SEGMENT_TOL_PX / self.camera.zoom_level(),
        );
        let top = if things.is_empty() {
            0.0
        } else {
            map.sectors
                .values()
                .map(|s| s.floor_height)
                .max()
                .unwrap_or(0) as f32
                + 1.0
        };
        let things = things
            .iter()
            .map(|&(key, p)| {
                let xy = moved(p.map(|c| c as f32)).map(|c| c.round() as i32);
                let z = self
                    .floor_height_below([xy[0] as f32, xy[1] as f32, top])
                    .unwrap_or(0);
                (key, [xy[0], xy[1], z])
            })
            .collect();
        Some(MoveTargets {
            vertices,
            things,
        })
    }

    /// Shared world-space geometry for moving and transforming vertices.
    #[expect(clippy::cast_precision_loss, reason = "GPU world coordinates use f32")]
    pub(super) fn vertex_move_overlay(
        &self,
        moves: &[(VertKey, [f32; 2])],
        lines: &[LineKey],
    ) -> Overlay {
        let Some(map) = &self.map else {
            return Overlay::None;
        };
        let mut positions: HashMap<_, _> =
            moves.iter().map(|&(k, p)| (k, [p[0], p[1], 0.0])).collect();
        let mut segments = Vec::with_capacity(lines.len());
        for key in lines {
            let Some(line) = map.lines.get(*key) else {
                continue;
            };
            let z = line
                .front
                .sector
                .and_then(|s| map.sectors.get(s))
                .map_or(0.0, |s| s.floor_height as f32);
            let mut at = |key| {
                if let Some(p) = positions.get_mut(&key) {
                    p[2] = z;
                    Some(*p)
                } else {
                    map.vertices.get(key).map(|p| [p.x, p.y, z])
                }
            };
            if let (Some(a), Some(b)) = (at(line.v1), at(line.v2)) {
                segments.push([a, b]);
            }
        }
        Overlay::Move {
            segments,
            points: positions.into_values().collect(),
        }
    }
}
