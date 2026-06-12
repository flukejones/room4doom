//! Clipboard (copy/cut/paste), delete, and undo/redo.

use editor_core::geom::{delete_vertex, vertex_at};
use editor_core::{
    EditorMap, LineDef, SectorKey, delete_sector, extract_fragment, fragment_min_corner,
    merge_sectors, paste_fragment,
};

use super::LevelEditorState;
use crate::state::{Damage, MapClipboard, SelItem};
use crate::undo::{EditAction, EditSession, UndoStack};

impl LevelEditorState {
    /// Apply an undo-stack step (`UndoStack::undo`/`redo`): on success clear the selection, mark dirty, and report `Geometry`; otherwise `None`.
    fn apply_history<T>(
        &mut self,
        step: impl Fn(&mut UndoStack, &mut EditorMap) -> Option<T>,
    ) -> Damage {
        let Some(map) = &mut self.map else {
            return Damage::None;
        };
        if step(&mut self.undo, map).is_some() {
            self.selection.clear();
            self.dirty = true;
            Damage::Edited
        } else {
            Damage::None
        }
    }

    pub fn undo(&mut self) -> Damage {
        self.apply_history(UndoStack::undo)
    }

    pub fn redo(&mut self) -> Damage {
        self.apply_history(UndoStack::redo)
    }

    /// Open a popup edit session: live edits write through with no undo record.
    pub fn begin_session(&self) -> Option<EditSession> {
        let map = self.map.as_ref()?;
        Some(EditSession {
            snapshot: bincode::serialize(map).expect("EditorMap always serializes"),
            was_dirty: self.dirty,
        })
    }

    /// Apply: record the session's open state as ONE undo step; no-op if nothing changed.
    pub fn commit_session(&mut self, action: EditAction, session: EditSession) {
        let unchanged = self.map.as_ref().is_some_and(|m| {
            bincode::serialize(m).expect("EditorMap always serializes") == session.snapshot
        });
        if unchanged {
            self.dirty = session.was_dirty;
            return;
        }
        self.undo.record_snapshot(action, session.snapshot);
        self.dirty = true;
    }

    /// Cancel: restore the session's open state; no undo record.
    pub fn cancel_session(&mut self, session: EditSession) -> Damage {
        let Some(map) = self.map.as_mut() else {
            return Damage::None;
        };
        *map = bincode::deserialize(&session.snapshot).expect("snapshots round-trip");
        self.dirty = session.was_dirty;
        Damage::Edited
    }

    /// Delete selection. Two-sided wall delete merges adjacent sectors (lower index survives); geometry outside the selection is never re-derived.
    pub fn delete_selection(&mut self) -> Damage {
        if self.selection.is_empty() {
            return Damage::None;
        }
        let Some(map) = &mut self.map else {
            return Damage::None;
        };
        self.undo.record(EditAction::DeleteSelection, map);

        let mut lines = Vec::new();
        let mut things = Vec::new();
        let mut vert_pos = Vec::new();
        let mut sector_dels = Vec::new();
        for item in self.selection.items() {
            match *item {
                SelItem::Line(k) => lines.push(k),
                SelItem::Thing(k) => things.push(k),
                SelItem::Vertex(k) => {
                    if let Some(v) = map.vertices.get(k) {
                        vert_pos.push([v.x, v.y]);
                    }
                }
                SelItem::Sector(k) => sector_dels.push(k),
            }
        }

        let mut merges: Vec<(SectorKey, SectorKey)> = Vec::new();
        let sector_pair = |l: &LineDef, merges: &mut Vec<(SectorKey, SectorKey)>| {
            if let (Some(front), Some(back)) =
                (l.front.sector, l.back.as_ref().and_then(|b| b.sector))
                && front != back
            {
                merges.push((front, back));
            }
        };
        for &k in &lines {
            if let Some(l) = map.lines.get(k) {
                sector_pair(l, &mut merges);
            }
        }
        for p in &vert_pos {
            if let Some(v) = vertex_at(map, *p) {
                for l in map.lines.values() {
                    if (l.v1 == v || l.v2 == v) && l.back.is_some() {
                        sector_pair(l, &mut merges);
                    }
                }
            }
        }

        for &k in &sector_dels {
            delete_sector(map, k);
        }

        map.remove_things(&things);
        map.remove_lines(&lines);
        for p in &vert_pos {
            if let Some(v) = vertex_at(map, *p) {
                delete_vertex(map, v);
            }
        }

        merge_sectors(map, &merges);

        self.selection.clear();
        self.dirty = true;
        Damage::Edited
    }

    pub fn copy_selection(&mut self) -> Damage {
        let Some(map) = &self.map else {
            return Damage::None;
        };
        let mut lines = Vec::new();
        let mut things = Vec::new();
        let mut sectors = Vec::new();
        for item in self.selection.items() {
            match *item {
                SelItem::Line(k) => lines.push(k),
                SelItem::Thing(k) => things.push(k),
                SelItem::Sector(k) => {
                    if let Some(s) = map.sectors.get(k).copied() {
                        sectors.push(s);
                    }
                }
                SelItem::Vertex(_) => {}
            }
        }
        if lines.is_empty() && things.is_empty() && sectors.is_empty() {
            return Damage::None;
        }
        let fragment = extract_fragment(map, &lines, &things);
        let anchor = fragment_min_corner(&fragment);
        self.clipboard = MapClipboard {
            anchor,
            fragment,
            sectors,
        };
        Damage::None
    }

    pub fn cut_selection(&mut self) -> Damage {
        self.copy_selection();
        if self.clipboard.is_empty() {
            return Damage::None;
        }
        self.delete_selection()
    }

    pub fn can_paste(&self) -> bool {
        if self.clipboard.is_empty() {
            return false;
        }
        if !self.clipboard.fragment.lines.is_empty() || !self.clipboard.fragment.things.is_empty() {
            return true;
        }
        let world = self.cursor_world;
        self.sector_under(world).is_some() || self.can_add_sector(world)
    }

    /// Paste as one undo step: record once, non-recording internals, discard on no-op.
    pub fn paste(&mut self) -> Damage {
        if self.clipboard.is_empty() || self.map.is_none() {
            return Damage::None;
        }
        let map = self.map.as_ref().expect("checked above");
        self.undo.record(EditAction::Paste, map);
        let mut damage = if !self.clipboard.fragment.lines.is_empty()
            || !self.clipboard.fragment.things.is_empty()
        {
            let drop = self.snap_point(self.cursor_world);
            let delta = [
                drop[0] - self.clipboard.anchor[0],
                drop[1] - self.clipboard.anchor[1],
            ];
            let Self {
                map,
                clipboard,
                selection,
                ..
            } = self;
            let map = map.as_mut().expect("checked above");
            selection.clear();
            let (lines, things) = paste_fragment(map, &clipboard.fragment, delta);
            for k in lines {
                selection.push(SelItem::Line(k));
            }
            for k in things {
                selection.push(SelItem::Thing(k));
            }
            self.dirty = true;
            Damage::Edited
        } else {
            Damage::None
        };
        if let Some(record) = self.clipboard.sectors.first().copied() {
            let world = self.cursor_world;
            if let Some(target) = self.sector_under(world) {
                damage = damage.combine(self.set_sector(target, record));
            } else if self.can_add_sector(world)
                && let Some(new) = self.add_sector(world)
            {
                damage = damage.combine(self.set_sector(new, record));
                damage = damage.combine(self.finish_new_sector(new));
            }
        }
        self.clipboard = MapClipboard::default();
        if matches!(damage, Damage::None) {
            self.undo.discard_last();
        }
        damage
    }
}
