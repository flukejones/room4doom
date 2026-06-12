//! Editing operations on lines and things: field edits, flip, split, the
//! texture eyedropper, and thing placement.

use std::f32::consts::FRAC_PI_4;

use editor_core::geom::{
    nearest_point_on_segment, split_line_at, split_lines_at_intersections, thing_floor_z,
};
use editor_core::{
    LineDef, LineFlags, Thing, flip_lines, lines_share_vertex_within_angle, merge_collinear_lines,
};

use super::{LevelEditorState, ON_SEGMENT_TOL_PX};
use crate::state::{ChangedElems, Damage, SelItem};
use crate::undo::EditAction;

/// Max deviation from straight (45°) for two lines to be mergeable.
const MERGE_MAX_DEVIATION: f32 = FRAC_PI_4;

/// Sidedef or two-sided flag change → geometry damage; other fields patch.
fn line_walls_changed(old: &LineDef, new: &LineDef) -> bool {
    old.front != new.front
        || old.back != new.back
        || old.flags.contains(LineFlags::TWO_SIDED) != new.flags.contains(LineFlags::TWO_SIDED)
}

impl LevelEditorState {
    pub(super) fn place_thing(&mut self, world: [f32; 2]) -> Damage {
        let p = self.snap_point(world);
        let Some(map) = &mut self.map else {
            return Damage::None;
        };
        self.undo.record(EditAction::PlaceThing, map);
        self.dirty = true;
        let z = thing_floor_z(map, p);
        map.things.push(Thing {
            x: p[0] as i32,
            y: p[1] as i32,
            z,
            angle: self.thing_template.angle,
            kind: self.thing_template.kind,
            options: self.thing_template.options,
        });
        let item = SelItem::Thing((map.things.len() - 1) as u32);
        self.selection.replace(item);
        Damage::Geometry
    }

    pub fn apply_line(&mut self, index: u32, new: LineDef) -> Damage {
        let Some(old) = self.map.as_ref().and_then(|m| m.lines.get(index as usize)) else {
            return Damage::None;
        };
        if *old == new {
            return Damage::None;
        }
        let walls_changed = line_walls_changed(old, &new);
        let map = self.map.as_mut().expect("checked above");
        self.undo.record(EditAction::EditLine, map);
        self.dirty = true;
        if let Some(slot) = map.lines.get_mut(index as usize) {
            slot.overwrite_fields(new);
        }
        if walls_changed {
            Damage::Geometry
        } else {
            Damage::Patch(ChangedElems::line(index))
        }
    }

    pub fn split_selected_at_intersections(&mut self) -> Damage {
        let lines = self.selected_lines();
        if lines.is_empty() || self.map.is_none() {
            return Damage::None;
        }
        let map = self.map.as_mut().expect("checked above");
        self.undo.record(EditAction::SplitLines, map);
        let tol = ON_SEGMENT_TOL_PX / self.camera.zoom_level();
        split_lines_at_intersections(map, &lines, tol);
        self.dirty = true;
        Damage::Geometry
    }

    /// Split selected line at the nearest point to `world`. Head keeps the selection.
    pub fn split_selected_line_at(&mut self, world: [f32; 2]) -> Damage {
        let Some(&SelItem::Line(i)) = self
            .selection
            .items()
            .iter()
            .find(|it| matches!(it, SelItem::Line(_)))
        else {
            return Damage::None;
        };
        let Some(map) = &self.map else {
            return Damage::None;
        };
        let Some(line) = map.lines.get(i as usize) else {
            return Damage::None;
        };
        let (a, b) = (
            map.vertices[line.v1 as usize],
            map.vertices[line.v2 as usize],
        );
        let point = nearest_point_on_segment(world, [a.x, a.y], [b.x, b.y]);
        let map = self.map.as_mut().expect("checked above");
        self.undo.record(EditAction::SplitLines, map);
        split_line_at(map, i, point);
        self.dirty = true;
        Damage::Geometry
    }

    /// True when selection is exactly two near-collinear lines sharing a vertex.
    pub fn lines_mergeable(&self) -> bool {
        let lines = self.selected_lines();
        let Some(map) = &self.map else {
            return false;
        };
        lines.len() == 2
            && lines_share_vertex_within_angle(map, lines[0], lines[1], MERGE_MAX_DEVIATION)
    }

    pub fn merge_selected_lines(&mut self) -> Damage {
        let lines = self.selected_lines();
        if lines.len() != 2 || self.map.is_none() {
            return Damage::None;
        }
        let map = self.map.as_mut().expect("checked above");
        self.undo.record(EditAction::SplitLines, map);
        if merge_collinear_lines(map, lines[0], lines[1], MERGE_MAX_DEVIATION) {
            self.selection.clear();
            self.dirty = true;
            Damage::Geometry
        } else {
            self.undo.discard_last();
            Damage::None
        }
    }

    /// Copy the first selected line's front-middle texture into the draw brush.
    pub fn pick_brush_from_selection(&mut self) -> bool {
        let Some(map) = &self.map else {
            return false;
        };
        let Some(&SelItem::Line(i)) = self
            .selection
            .items()
            .iter()
            .find(|i| matches!(i, SelItem::Line(_)))
        else {
            return false;
        };
        let Some(line) = map.lines.get(i as usize) else {
            return false;
        };
        self.draw_brush.wall_tex = line.front.middle_tex;
        true
    }

    pub fn flip_selected_lines(&mut self, indices: &[u32]) -> Damage {
        if indices.is_empty() || self.map.is_none() {
            return Damage::None;
        }
        let map = self.map.as_mut().expect("checked above");
        self.undo.record(EditAction::FlipLines, map);
        self.dirty = true;
        flip_lines(map, indices);
        Damage::Patch(ChangedElems {
            lines: indices.to_vec(),
            ..Default::default()
        })
    }

    pub fn apply_lines(&mut self, indices: &[u32], edit: impl Fn(&LineDef) -> LineDef) -> Damage {
        let Some(map) = &self.map else {
            return Damage::None;
        };
        let edits: Vec<(u32, LineDef)> = indices
            .iter()
            .filter_map(|&i| {
                let old = map.lines.get(i as usize)?;
                let new = edit(old);
                (*old != new).then_some((i, new))
            })
            .collect();
        if edits.is_empty() {
            return Damage::None;
        }
        let walls_changed = edits.iter().any(|(i, new)| {
            map.lines
                .get(*i as usize)
                .is_some_and(|old| line_walls_changed(old, new))
        });
        let map = self.map.as_mut().expect("checked above");
        self.undo.record(EditAction::EditLine, map);
        self.dirty = true;
        let touched: Vec<u32> = edits.iter().map(|(i, _)| *i).collect();
        for (i, new) in edits {
            if let Some(slot) = map.lines.get_mut(i as usize) {
                slot.overwrite_fields(new);
            }
        }
        if walls_changed {
            Damage::Geometry
        } else {
            Damage::Patch(ChangedElems {
                lines: touched,
                ..Default::default()
            })
        }
    }

    pub fn apply_things(&mut self, indices: &[u32], edit: impl Fn(&Thing) -> Thing) -> Damage {
        let Some(map) = &self.map else {
            return Damage::None;
        };
        let edits: Vec<(u32, Thing)> = indices
            .iter()
            .filter_map(|&i| {
                let old = map.things.get(i as usize)?;
                let new = edit(old);
                (*old != new).then_some((i, new))
            })
            .collect();
        if edits.is_empty() {
            return Damage::None;
        }
        // Kind change requires atlas repack → Geometry damage.
        let kind_changed = edits.iter().any(|(i, new)| {
            map.things
                .get(*i as usize)
                .is_some_and(|old| old.kind != new.kind)
        });
        let map = self.map.as_mut().expect("checked above");
        self.undo.record(EditAction::EditThing, map);
        self.dirty = true;
        let touched: Vec<u32> = edits.iter().map(|(i, _)| *i).collect();
        for (i, new) in &edits {
            if let Some(slot) = map.things.get_mut(*i as usize) {
                *slot = *new;
            }
        }
        if kind_changed {
            Damage::Geometry
        } else {
            Damage::Patch(ChangedElems {
                things: touched,
                ..Default::default()
            })
        }
    }
}
