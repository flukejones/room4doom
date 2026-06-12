//! Editing operations on lines and things: field edits, flip, split, merge/dissolve, the texture eyedropper, and thing placement.

use std::collections::HashSet;
use std::f32::consts::FRAC_PI_4;

use editor_core::geom::{
    nearest_point_on_segment, split_line_at, split_lines_at_intersections, thing_floor_z,
};
use editor_core::{
    Axis, EditorMap, LineDef, LineKey, Thing, ThingKey, VertKey, align_vertices, any_dissolvable,
    can_merge_collinear, can_trim_corner, chamfer_vertex, dissolve_collinear_vertices,
    distribute_vertices, fillet_vertex, flip_lines, heal_map, merge_collinear_lines, move_vertices,
    straighten_chain,
};

use super::{HEAL_TOL, LevelEditorState, ON_SEGMENT_TOL_PX, default_sector};
use crate::state::{Damage, SelItem};
use crate::undo::EditAction;

/// Max deviation from straight (45°) for two lines to be mergeable.
const MERGE_MAX_DEVIATION: f32 = FRAC_PI_4;

impl LevelEditorState {
    pub(super) fn place_thing(&mut self, world: [f32; 2]) -> Damage {
        let p = self.snap_point(world);
        let Some(map) = &mut self.map else {
            return Damage::None;
        };
        self.undo.record(EditAction::PlaceThing, map);
        self.dirty = true;
        let z = thing_floor_z(map, p);
        let key = map.things.insert(Thing {
            x: p[0] as i32,
            y: p[1] as i32,
            z,
            angle: self.thing_template.angle,
            kind: self.thing_template.kind,
            options: self.thing_template.options,
        });
        self.selection.replace(SelItem::Thing(key));
        Damage::Edited
    }

    /// Write `new` over a line; no undo record (sessions snapshot once).
    pub fn set_line(&mut self, key: LineKey, new: LineDef) -> Damage {
        let Some(old) = self.map.as_ref().and_then(|m| m.lines.get(key)) else {
            return Damage::None;
        };
        if *old == new {
            return Damage::None;
        }
        let map = self.map.as_mut().expect("checked above");
        self.dirty = true;
        if let Some(slot) = map.lines.get_mut(key) {
            slot.overwrite_fields(new);
        }
        Damage::Edited
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
        Damage::Edited
    }

    /// Split selected line at the nearest point to `world`. Head keeps the selection.
    pub fn split_selected_line_at(&mut self, world: [f32; 2]) -> Damage {
        let Some(&SelItem::Line(key)) = self
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
        let Some(line) = map.lines.get(key) else {
            return Damage::None;
        };
        let (a, b) = (map.vertices[line.v1], map.vertices[line.v2]);
        let point = nearest_point_on_segment(world, [a.x, a.y], [b.x, b.y]);
        let map = self.map.as_mut().expect("checked above");
        self.undo.record(EditAction::SplitLines, map);
        split_line_at(map, key, point);
        self.dirty = true;
        Damage::Edited
    }

    /// True when selection is exactly two near-collinear lines sharing a vertex.
    pub fn lines_mergeable(&self) -> bool {
        let lines = self.selected_lines();
        let Some(map) = &self.map else {
            return false;
        };
        lines.len() == 2 && can_merge_collinear(map, lines[0], lines[1], MERGE_MAX_DEVIATION)
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
            Damage::Edited
        } else {
            self.undo.discard_last();
            Damage::None
        }
    }

    /// Selected vertices plus the endpoints of selected lines, in key order.
    pub(super) fn selection_vertex_set(&self) -> Vec<VertKey> {
        let Some(map) = &self.map else {
            return Vec::new();
        };
        let mut set: HashSet<VertKey> = self.selected_vertices().into_iter().collect();
        for k in self.selected_lines() {
            if let Some(l) = map.lines.get(k) {
                set.insert(l.v1);
                set.insert(l.v2);
            }
        }
        let mut verts: Vec<VertKey> = set.into_iter().collect();
        verts.sort_unstable();
        verts
    }

    pub fn can_dissolve(&self) -> bool {
        self.map
            .as_ref()
            .is_some_and(|m| any_dissolvable(m, &self.selection_vertex_set(), MERGE_MAX_DEVIATION))
    }

    /// Dissolve every selected vertex (and selected-line endpoint) whose two lines are near-collinear with matching sides.
    pub fn dissolve_selected(&mut self) -> Damage {
        let verts = self.selection_vertex_set();
        if verts.is_empty() || self.map.is_none() {
            return Damage::None;
        }
        let map = self.map.as_mut().expect("checked above");
        self.undo.record(EditAction::DissolveVertices, map);
        if dissolve_collinear_vertices(map, &verts, MERGE_MAX_DEVIATION) == 0 {
            self.undo.discard_last();
            return Damage::None;
        }
        self.selection.clear();
        self.dirty = true;
        Damage::Edited
    }

    /// True when the selection is exactly one vertex whose corner can host a fillet/chamfer.
    pub fn can_fillet(&self) -> bool {
        let verts = self.selected_vertices();
        verts.len() == 1
            && self
                .map
                .as_ref()
                .is_some_and(|m| can_trim_corner(m, verts[0]))
    }

    pub fn fillet_selected(&mut self, radius: f32, segments: u32) -> Damage {
        let verts = self.selected_vertices();
        if verts.len() != 1 || self.map.is_none() {
            return Damage::None;
        }
        let map = self.map.as_mut().expect("checked above");
        self.undo.record(EditAction::Fillet, map);
        let new = fillet_vertex(map, verts[0], radius, segments);
        if new.is_empty() {
            self.undo.discard_last();
            return Damage::None;
        }
        self.selection.clear();
        for k in new {
            self.selection.push(SelItem::Line(k));
        }
        self.dirty = true;
        Damage::Edited
    }

    pub fn chamfer_selected(&mut self, dist: f32) -> Damage {
        let verts = self.selected_vertices();
        if verts.len() != 1 || self.map.is_none() {
            return Damage::None;
        }
        let map = self.map.as_mut().expect("checked above");
        self.undo.record(EditAction::Chamfer, map);
        let Some(cut) = chamfer_vertex(map, verts[0], dist) else {
            self.undo.discard_last();
            return Damage::None;
        };
        self.selection.replace(SelItem::Line(cut));
        self.dirty = true;
        Damage::Edited
    }

    /// True when the selection maps to enough vertices for align/distribute.
    pub fn can_align(&self) -> bool {
        self.selection_vertex_set().len() >= 3
    }

    /// True when the selection is a straightenable chain that is not already straight.
    pub fn can_straighten(&self) -> bool {
        self.map
            .as_ref()
            .is_some_and(|m| !straighten_chain(m, &self.selection_vertex_set()).is_empty())
    }

    pub fn align_selected(&mut self, axis: Axis) -> Damage {
        let verts = self.selection_vertex_set();
        self.commit_moves(|map| align_vertices(map, &verts, axis))
    }

    pub fn distribute_selected(&mut self, axis: Axis) -> Damage {
        let verts = self.selection_vertex_set();
        self.commit_moves(|map| distribute_vertices(map, &verts, axis))
    }

    pub fn straighten_selected(&mut self) -> Damage {
        let verts = self.selection_vertex_set();
        self.commit_moves(|map| straighten_chain(map, &verts))
    }

    /// Commit a computed move list through the full move pipeline (split/weld/dedup/re-sector), as one undo step.
    fn commit_moves(
        &mut self,
        compute: impl FnOnce(&EditorMap) -> Vec<(VertKey, [f32; 2])>,
    ) -> Damage {
        let Some(map) = &self.map else {
            return Damage::None;
        };
        let moves = compute(map);
        if moves.is_empty() {
            return Damage::None;
        }
        let tol = ON_SEGMENT_TOL_PX / self.camera.zoom_level();
        let map = self.map.as_mut().expect("checked above");
        self.undo.record(EditAction::MoveSelection, map);
        move_vertices(map, &moves, &[], tol, default_sector());
        self.dirty = true;
        Damage::Edited
    }

    /// Repair geometric defects map-wide (weld near-coincident, split T-junctions, fold overlaps, sync flags, prune); one undo step.
    pub fn heal_geometry(&mut self) -> Damage {
        let Some(map) = &mut self.map else {
            return Damage::None;
        };
        self.undo.record(EditAction::Heal, map);
        if heal_map(map, HEAL_TOL) == 0 {
            self.undo.discard_last();
            return Damage::None;
        }
        self.selection.clear();
        self.dirty = true;
        Damage::Edited
    }

    /// Copy the first selected line's front-middle texture into the draw brush.
    pub fn pick_brush_from_selection(&mut self) -> bool {
        let Some(map) = &self.map else {
            return false;
        };
        let Some(&SelItem::Line(key)) = self
            .selection
            .items()
            .iter()
            .find(|i| matches!(i, SelItem::Line(_)))
        else {
            return false;
        };
        let Some(line) = map.lines.get(key) else {
            return false;
        };
        self.draw_brush.wall_tex = line.front.middle_tex;
        true
    }

    pub fn flip_selected_lines(&mut self, keys: &[LineKey]) -> Damage {
        if keys.is_empty() || self.map.is_none() {
            return Damage::None;
        }
        let map = self.map.as_mut().expect("checked above");
        self.undo.record(EditAction::FlipLines, map);
        self.dirty = true;
        flip_lines(map, keys);
        Damage::Edited
    }

    pub fn apply_lines(&mut self, keys: &[LineKey], edit: impl Fn(&LineDef) -> LineDef) -> Damage {
        let Some(map) = &self.map else {
            return Damage::None;
        };
        let edits: Vec<(LineKey, LineDef)> = keys
            .iter()
            .filter_map(|&k| {
                let old = map.lines.get(k)?;
                let new = edit(old);
                (*old != new).then_some((k, new))
            })
            .collect();
        if edits.is_empty() {
            return Damage::None;
        }
        let map = self.map.as_mut().expect("checked above");
        self.undo.record(EditAction::EditLine, map);
        self.dirty = true;
        for (k, new) in edits {
            if let Some(slot) = map.lines.get_mut(k) {
                slot.overwrite_fields(new);
            }
        }
        Damage::Edited
    }

    pub fn apply_things(&mut self, keys: &[ThingKey], edit: impl Fn(&Thing) -> Thing) -> Damage {
        let Some(map) = &self.map else {
            return Damage::None;
        };
        let edits: Vec<(ThingKey, Thing)> = keys
            .iter()
            .filter_map(|&k| {
                let old = map.things.get(k)?;
                let new = edit(old);
                (*old != new).then_some((k, new))
            })
            .collect();
        if edits.is_empty() {
            return Damage::None;
        }
        let map = self.map.as_mut().expect("checked above");
        self.undo.record(EditAction::EditThing, map);
        self.dirty = true;
        for (k, new) in &edits {
            if let Some(slot) = map.things.get_mut(*k) {
                *slot = *new;
            }
        }
        Damage::Edited
    }
}
