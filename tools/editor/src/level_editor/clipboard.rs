//! Clipboard (copy/cut/paste), delete, and undo/redo.

use editor_core::geom::{delete_vertex, vertex_at};
use editor_core::{
    EditorMap, LineDef, delete_sector, extract_fragment, fragment_min_corner, merge_sectors,
    paste_fragment,
};

use super::LevelEditorState;
use crate::state::{Damage, MapClipboard, SelItem};
use crate::undo::{EditAction, UndoStack};

impl LevelEditorState {
    /// Apply an undo-stack step (`UndoStack::undo`/`redo`): on success clear the
    /// selection, mark dirty, and report `Geometry`; otherwise `None`.
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
            Damage::Geometry
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

    /// Delete selection. Two-sided wall delete merges adjacent sectors (lower index survives).
    /// Geometry outside the selection is never re-derived.
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
                SelItem::Line(i) => lines.push(i),
                SelItem::Thing(i) => things.push(i),
                SelItem::Vertex(i) => {
                    if let Some(v) = map.vertices.get(i as usize) {
                        vert_pos.push([v.x, v.y]);
                    }
                }
                SelItem::Sector(i) => sector_dels.push(i),
            }
        }

        let mut merges: Vec<(u32, u32)> = Vec::new();
        let sector_pair = |l: &LineDef, merges: &mut Vec<(u32, u32)>| {
            if let (Some(front), Some(back)) = (l.front.sector, l.back.and_then(|b| b.sector))
                && front != back
            {
                merges.push((front, back));
            }
        };
        for &i in &lines {
            if let Some(l) = map.lines.get(i as usize) {
                sector_pair(l, &mut merges);
            }
        }
        for p in &vert_pos {
            if let Some(v) = vertex_at(map, *p) {
                for l in &map.lines {
                    if (l.v1 == v || l.v2 == v) && l.back.is_some() {
                        sector_pair(l, &mut merges);
                    }
                }
            }
        }

        // Highest index first so earlier indices stay valid; must precede merge_sectors.
        sector_dels.sort_unstable();
        for &i in sector_dels.iter().rev() {
            delete_sector(map, i);
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
        Damage::Geometry
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
                SelItem::Line(i) => lines.push(i),
                SelItem::Thing(i) => things.push(i),
                SelItem::Sector(i) => {
                    if let Some(s) = map.sectors.get(i as usize).copied() {
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

    pub fn paste(&mut self) -> Damage {
        if self.clipboard.is_empty() || self.map.is_none() {
            return Damage::None;
        }
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
                undo,
                ..
            } = self;
            let map = map.as_mut().expect("checked above");
            undo.record(EditAction::Paste, map);
            selection.clear();
            let (lines, things) = paste_fragment(map, &clipboard.fragment, delta);
            for i in lines {
                selection.push(SelItem::Line(i));
            }
            for i in things {
                selection.push(SelItem::Thing(i));
            }
            self.dirty = true;
            Damage::Geometry
        } else {
            Damage::None
        };
        if let Some(record) = self.clipboard.sectors.first().copied() {
            let world = self.cursor_world;
            if let Some(target) = self.sector_under(world) {
                damage = damage.combine(self.apply_sector(target, record));
            } else if self.can_add_sector(world) {
                let before = self.current_sector;
                let added = self.add_sector_at(world);
                if self.current_sector != before
                    && let Some(cur) = self.current_sector
                {
                    damage = damage.combine(self.apply_sector(cur, record));
                }
                damage = damage.combine(added);
            }
        }
        self.clipboard = MapClipboard::default();
        damage
    }
}
