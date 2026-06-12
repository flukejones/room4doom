//! Moving the selection: capturing drag-start positions, the live move preview, and committing the move (delegated to the kernel) on release — or, for a single dragged line, holding for the Move/Extrude choice.

use std::collections::{HashMap, HashSet};

use editor_core::geom::thing_floor_z;
use editor_core::{LineKey, SectorKey, ThingKey, VertKey, extrude_line, move_vertices};

use super::{LevelEditorState, ON_SEGMENT_TOL_PX};
use crate::level_editor::draw::default_sector;
use crate::render::view::snap;
use crate::state::{Damage, DragState, Overlay, SelItem};
use crate::undo::EditAction;

/// Padding around the containment-change region when re-deriving thing Z.
const THING_Z_REGION_PAD: f32 = 1.0;

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
        let lines: Vec<[VertKey; 2]> = map
            .lines
            .values()
            .filter(|l| vert_set.contains(&l.v1) || vert_set.contains(&l.v2))
            .map(|l| [l.v1, l.v2])
            .collect();
        let verts = vert_set
            .into_iter()
            .filter_map(|k| map.vertices.get(k).map(|v| (k, [v.x, v.y])))
            .collect();
        self.drag = DragState::MoveSel {
            start_world: world,
            verts,
            things,
            lines,
        };
    }

    pub(super) fn finish_move(&mut self) -> Damage {
        self.overlay = Overlay::None;
        let DragState::MoveSel {
            start_world,
            verts,
            things,
            ..
        } = &self.drag
        else {
            return Damage::None;
        };
        let delta = [
            self.cursor_world[0] - start_world[0],
            self.cursor_world[1] - start_world[1],
        ];
        if delta == [0.0, 0.0] {
            return Damage::None;
        }
        let grid = self.grid;
        let moves: Vec<(VertKey, [f32; 2])> = verts
            .iter()
            .map(|(i, o)| {
                (
                    *i,
                    [snap(o[0] + delta[0], grid), snap(o[1] + delta[1], grid)],
                )
            })
            .collect();
        let thing_moves: Vec<(ThingKey, [i32; 2])> = things
            .iter()
            .map(|(i, o)| {
                (
                    *i,
                    [
                        snap(o[0] as f32 + delta[0], grid).round() as i32,
                        snap(o[1] as f32 + delta[1], grid).round() as i32,
                    ],
                )
            })
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
                moved_things.contains(k)
                    || (x >= lo[0] - pad
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
        self.dirty = true;
        Damage::Edited
    }

    /// Cursor − drag-start of the in-flight move; `[0,0]` when no move drag.
    fn move_drag_delta(&self) -> [f32; 2] {
        let DragState::MoveSel {
            start_world,
            ..
        } = &self.drag
        else {
            return [0.0, 0.0];
        };
        [
            self.cursor_world[0] - start_world[0],
            self.cursor_world[1] - start_world[1],
        ]
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
        let DragState::MoveSel {
            verts,
            ..
        } = &self.drag
        else {
            return Damage::None;
        };
        let Some(&(_, origin)) = verts.first() else {
            return Damage::None;
        };
        let raw = self.move_drag_delta();
        let grid = self.grid;
        // Snap the leading corner as a move would; one delta for both corners keeps the quad a parallelogram.
        let delta = [
            snap(origin[0] + raw[0], grid) - origin[0],
            snap(origin[1] + raw[1], grid) - origin[1],
        ];
        self.drag = DragState::None;
        self.overlay = Overlay::None;
        let tol = ON_SEGMENT_TOL_PX / self.camera.zoom_level();
        let sector = self.draw_brush.sector();
        let Some(map) = &mut self.map else {
            return Damage::None;
        };
        self.undo.record(EditAction::Extrude, map);
        let new = extrude_line(map, line, delta, tol, sector);
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

    pub(super) fn move_selection_to(&mut self, world: [f32; 2]) -> Damage {
        let DragState::MoveSel {
            start_world,
            verts,
            things,
            lines,
            ..
        } = &self.drag
        else {
            return Damage::None;
        };
        let Some(map) = &self.map else {
            return Damage::None;
        };
        let delta = [world[0] - start_world[0], world[1] - start_world[1]];
        let grid = self.grid;
        let moved_pos = |orig: [f32; 2]| {
            [
                snap(orig[0] + delta[0], grid),
                snap(orig[1] + delta[1], grid),
            ]
        };

        let preview: HashMap<VertKey, [f32; 2]> =
            verts.iter().map(|(k, o)| (*k, moved_pos(*o))).collect();
        let at = |v: VertKey| {
            preview
                .get(&v)
                .copied()
                .or_else(|| map.vertices.get(v).map(|p| [p.x, p.y]))
        };

        let mut segments = Vec::with_capacity(lines.len());
        for &[v1, v2] in lines {
            if let (Some(a), Some(b)) = (at(v1), at(v2)) {
                segments.push([a, b]);
            }
        }
        let mut points: Vec<[f32; 2]> = preview.values().copied().collect();
        points.extend(
            things
                .iter()
                .map(|(_, o)| moved_pos([o[0] as f32, o[1] as f32])),
        );

        self.overlay = Overlay::Move {
            segments,
            points,
        };
        Damage::Overlay
    }
}
