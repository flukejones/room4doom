//! Moving the selection: capturing drag-start positions, the live move preview,
//! and committing the move (delegated to the kernel) on release.

use std::collections::{HashMap, HashSet};

use editor_core::geom::derive_thing_heights;
use editor_core::move_vertices;

use super::{LevelEditorState, ON_SEGMENT_TOL_PX};
use crate::level_editor::draw::default_sector;
use crate::render::view::snap;
use crate::state::{Damage, DragState, Overlay, SelItem};
use crate::undo::EditAction;

impl LevelEditorState {
    pub(super) fn begin_move(&mut self, world: [f32; 2]) {
        let Some(map) = &self.map else { return };
        let mut vert_set: HashSet<u32> = HashSet::new();
        let mut things = Vec::new();
        for item in self.selection.items() {
            match *item {
                SelItem::Vertex(i) => {
                    vert_set.insert(i);
                }
                SelItem::Line(i) => {
                    if let Some(line) = map.lines.get(i as usize) {
                        vert_set.insert(line.v1);
                        vert_set.insert(line.v2);
                    }
                }
                SelItem::Thing(i) => {
                    if let Some(t) = map.things.get(i as usize) {
                        things.push((i, [t.x, t.y]));
                    }
                }
                SelItem::Sector(_) => {}
            }
        }
        let verts = vert_set
            .into_iter()
            .filter_map(|i| map.vertices.get(i as usize).map(|v| (i, [v.x, v.y])))
            .collect();
        self.drag = DragState::MoveSel {
            start_world: world,
            verts,
            things,
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
        let moves: Vec<(u32, [f32; 2])> = verts
            .iter()
            .map(|(i, o)| {
                (
                    *i,
                    [snap(o[0] + delta[0], grid), snap(o[1] + delta[1], grid)],
                )
            })
            .collect();
        let thing_moves: Vec<(u32, [i32; 2])> = things
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
        let Some(map) = &mut self.map else {
            return Damage::None;
        };
        self.undo.record(EditAction::MoveSelection, map);
        move_vertices(map, &moves, &thing_moves, tol, default_sector());
        derive_thing_heights(map);
        self.dirty = true;
        Damage::Geometry
    }

    pub(super) fn move_selection_to(&mut self, world: [f32; 2]) -> Damage {
        let DragState::MoveSel {
            start_world,
            verts,
            things,
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

        let preview: HashMap<u32, [f32; 2]> =
            verts.iter().map(|(i, o)| (*i, moved_pos(*o))).collect();
        let at = |v: u32| {
            preview
                .get(&v)
                .copied()
                .or_else(|| map.vertices.get(v as usize).map(|p| [p.x, p.y]))
        };

        let mut segments = Vec::new();
        for line in &map.lines {
            if (preview.contains_key(&line.v1) || preview.contains_key(&line.v2))
                && let (Some(a), Some(b)) = (at(line.v1), at(line.v2))
            {
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
