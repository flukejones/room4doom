//! Undo/redo via bincode whole-map snapshots. Exact (f32 bit-preserving), no
//! inverse-operation logic. Not `.dwd` — that format is lossy for WAD-imported maps.

use std::collections::VecDeque;

use editor_core::EditorMap;

pub const UNDO_DEPTH: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditAction {
    MoveSelection,
    DrawLine,
    PlaceThing,
    DeleteSelection,
    EditLine,
    EditSector,
    EditThing,
    RemapApply,
    Paste,
    SplitLines,
    FlipLines,
}

/// Snapshot stacks. Record once at drag start to coalesce a gesture into one step.
/// Deque so depth-cap eviction is O(1).
pub struct UndoStack {
    undo: VecDeque<(EditAction, Vec<u8>)>,
    redo: Vec<(EditAction, Vec<u8>)>,
}

impl Default for UndoStack {
    fn default() -> Self {
        Self::new()
    }
}

impl UndoStack {
    pub fn new() -> Self {
        Self {
            undo: VecDeque::with_capacity(UNDO_DEPTH),
            redo: Vec::with_capacity(UNDO_DEPTH),
        }
    }

    /// Snapshot map before mutation. Clears redo.
    pub fn record(&mut self, action: EditAction, map: &EditorMap) {
        let bytes = bincode::serialize(map).expect("EditorMap always serializes");
        if self.undo.len() == UNDO_DEPTH {
            self.undo.pop_front();
        }
        self.undo.push_back((action, bytes));
        self.redo.clear();
    }

    /// Drop last snapshot (op was a no-op). Redo stays cleared.
    pub fn discard_last(&mut self) {
        self.undo.pop_back();
    }

    /// Restore latest snapshot; pushes current to redo.
    pub fn undo(&mut self, map: &mut EditorMap) -> Option<EditAction> {
        let (action, bytes) = self.undo.pop_back()?;
        let current = bincode::serialize(map).expect("EditorMap always serializes");
        self.redo.push((action, current));
        *map = bincode::deserialize(&bytes).expect("snapshots round-trip");
        Some(action)
    }

    /// Re-apply latest undone state; pushes current back to undo.
    pub fn redo(&mut self, map: &mut EditorMap) -> Option<EditAction> {
        let (action, bytes) = self.redo.pop()?;
        let current = bincode::serialize(map).expect("EditorMap always serializes");
        self.undo.push_back((action, current));
        *map = bincode::deserialize(&bytes).expect("snapshots round-trip");
        Some(action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use editor_core::{Thing, ThingFlags, import_wad_map};

    fn e1m1() -> EditorMap {
        let wad = wad::WadData::new(&test_utils::doom1_wad_path());
        import_wad_map(&wad, "E1M1").expect("E1M1 imports")
    }

    fn thing() -> Thing {
        Thing {
            x: 0,
            y: 0,
            z: 0,
            angle: 0,
            kind: 1,
            options: ThingFlags::from_bits_retain(7),
        }
    }

    #[test]
    fn undo_restores_exact_state() {
        let mut map = e1m1();
        let before = bincode::serialize(&map).expect("serializes");
        let mut stack = UndoStack::new();

        stack.record(EditAction::PlaceThing, &map);
        map.things.push(thing());

        assert_eq!(stack.undo(&mut map), Some(EditAction::PlaceThing));
        assert_eq!(bincode::serialize(&map).expect("serializes"), before);
        assert_eq!(stack.undo(&mut map), None);
    }

    #[test]
    fn redo_after_undo_then_new_edit_clears_redo() {
        let mut map = e1m1();
        let mut stack = UndoStack::new();

        stack.record(EditAction::PlaceThing, &map);
        map.things.push(thing());
        let after = bincode::serialize(&map).expect("serializes");

        stack.undo(&mut map);
        assert_eq!(stack.redo(&mut map), Some(EditAction::PlaceThing));
        assert_eq!(bincode::serialize(&map).expect("serializes"), after);

        stack.undo(&mut map);
        stack.record(EditAction::DrawLine, &map);
        assert_eq!(stack.redo(&mut map), None, "new edit clears redo");
    }

    #[test]
    fn depth_cap_evicts_oldest() {
        let mut map = EditorMap::default();
        let mut stack = UndoStack::new();
        for i in 0..(UNDO_DEPTH + 10) {
            stack.record(EditAction::PlaceThing, &map);
            map.things.push(Thing {
                x: i as i32,
                ..thing()
            });
        }
        let mut count = 0;
        while stack.undo(&mut map).is_some() {
            count += 1;
        }
        assert_eq!(count, UNDO_DEPTH);
        assert_eq!(map.things.len(), 10, "oldest ten edits are baked in");
    }
}
