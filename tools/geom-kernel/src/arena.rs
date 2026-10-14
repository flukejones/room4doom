//! Generational arena: stable keys survive unrelated removals; a stale key (slot freed/reused) resolves to `None` instead of aliasing.

use std::marker::PhantomData;
use std::ops::{Index, IndexMut};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

/// A typed arena key: slot index + generation. Implemented via [`arena_key!`].
pub trait ArenaKey: Copy + Eq {
    fn new(slot: u32, generation: u32) -> Self;
    fn slot(self) -> u32;
    fn generation(self) -> u32;
}

/// Declare a key newtype for one arena element kind.
#[macro_export]
macro_rules! arena_key {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord,
            serde::Serialize, serde::Deserialize,
        )]
        pub struct $name {
            slot: u32,
            generation: u32,
        }

        impl $crate::arena::ArenaKey for $name {
            fn new(slot: u32, generation: u32) -> Self {
                Self { slot, generation }
            }

            fn slot(self) -> u32 {
                self.slot
            }

            fn generation(self) -> u32 {
                self.generation
            }
        }
    };
}

/// One slot: current generation + live value (`None` = free, awaiting reuse).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Slot<T> {
    generation: u32,
    value: Option<T>,
}

/// Keyed storage with stable generational references: slot-ordered iteration; freed slots reuse with a bumped generation; serde preserves keys (undo identity).
#[derive(Debug, Serialize, Deserialize)]
#[serde(bound(serialize = "T: Serialize", deserialize = "T: Deserialize<'de>"))]
pub struct Arena<K, T> {
    slots: Vec<Slot<T>>,
    free: Vec<u32>,
    #[serde(skip)]
    _key: PhantomData<K>,
    #[serde(skip)]
    changes: Option<ChangeLog>,
    #[serde(skip)]
    checkpoint: u64,
}

#[derive(Debug)]
struct ChangeLog {
    slots: Vec<u32>,
    marked: Vec<bool>,
    free_changed: bool,
}

impl ChangeLog {
    fn mark(&mut self, slot: u32) {
        self.marked
            .resize(self.marked.len().max(slot as usize + 1), false);
        if !self.marked[slot as usize] {
            self.marked[slot as usize] = true;
            self.slots.push(slot);
        }
    }

    fn clear(&mut self) {
        for slot in self.slots.drain(..) {
            self.marked[slot as usize] = false;
        }
        self.free_changed = false;
    }
}

impl<K, T: Clone> Clone for Arena<K, T> {
    fn clone(&self) -> Self {
        Self {
            slots: self.slots.clone(),
            free: self.free.clone(),
            _key: PhantomData,
            changes: None,
            checkpoint: self.checkpoint,
        }
    }
}

impl<K, T: PartialEq> PartialEq for Arena<K, T> {
    fn eq(&self, other: &Self) -> bool {
        self.slots == other.slots && self.free == other.free
    }
}

impl<K, T> Default for Arena<K, T> {
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
            _key: PhantomData,
            changes: None,
            checkpoint: 0,
        }
    }
}

impl<K: ArenaKey, T> Arena<K, T> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Live element count.
    pub fn len(&self) -> usize {
        self.slots.len() - self.free.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Total slots ever allocated (live + free) — the size a slot-indexed GPU table needs.
    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }

    pub fn insert(&mut self, value: T) -> K {
        let key = if let Some(slot) = self.free.pop() {
            let s = &mut self.slots[slot as usize];
            s.value = Some(value);
            K::new(slot, s.generation)
        } else {
            let slot = u32::try_from(self.slots.len()).expect("arena exceeds u32 slot capacity");
            self.slots.push(Slot {
                generation: 0,
                value: Some(value),
            });
            K::new(slot, 0)
        };
        if let Some(changes) = &mut self.changes {
            changes.mark(key.slot());
            changes.free_changed = true;
        } else {
            self.checkpoint = 0;
        }
        key
    }

    /// Remove by key; the freed slot's generation is bumped so the key goes stale.
    pub fn remove(&mut self, key: K) -> Option<T> {
        let s = self.slots.get_mut(key.slot() as usize)?;
        if s.generation != key.generation() || s.value.is_none() {
            return None;
        }
        let value = s.value.take();
        s.generation = s.generation.wrapping_add(1);
        self.free.push(key.slot());
        if let Some(changes) = &mut self.changes {
            changes.mark(key.slot());
            changes.free_changed = true;
        } else {
            self.checkpoint = 0;
        }
        value
    }

    pub fn contains(&self, key: K) -> bool {
        self.get(key).is_some()
    }

    pub fn get(&self, key: K) -> Option<&T> {
        let s = self.slots.get(key.slot() as usize)?;
        (s.generation == key.generation())
            .then_some(s.value.as_ref())
            .flatten()
    }

    pub fn get_mut(&mut self, key: K) -> Option<&mut T> {
        let s = self.slots.get_mut(key.slot() as usize)?;
        if s.generation != key.generation() {
            return None;
        }
        let value = s.value.as_mut()?;
        if let Some(changes) = &mut self.changes {
            changes.mark(key.slot());
        } else {
            self.checkpoint = 0;
        }
        Some(value)
    }

    /// The live key occupying `slot` (UI identifier → key resolution).
    pub fn key_at_slot(&self, slot: u32) -> Option<K> {
        let s = self.slots.get(slot as usize)?;
        s.value.as_ref().map(|_| K::new(slot, s.generation))
    }

    pub fn keys(&self) -> impl Iterator<Item = K> + '_ {
        self.iter().map(|(k, _)| k)
    }

    pub fn values(&self) -> impl Iterator<Item = &T> {
        self.slots.iter().filter_map(|s| s.value.as_ref())
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.iter_mut().map(|(_, value)| value)
    }

    #[expect(
        clippy::cast_possible_truncation,
        reason = "insert bounds every slot index to u32"
    )]
    pub fn iter(&self) -> impl Iterator<Item = (K, &T)> {
        self.slots.iter().enumerate().filter_map(|(i, s)| {
            s.value
                .as_ref()
                .map(|v| (K::new(i as u32, s.generation), v))
        })
    }

    #[expect(
        clippy::cast_possible_truncation,
        reason = "insert bounds every slot index to u32"
    )]
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (K, &mut T)> {
        let changes = &mut self.changes;
        let checkpoint = &mut self.checkpoint;
        self.slots.iter_mut().enumerate().filter_map(move |(i, s)| {
            let generation = s.generation;
            s.value.as_mut().map(|v| {
                let key = K::new(i as u32, generation);
                if let Some(changes) = changes {
                    changes.mark(key.slot());
                } else {
                    *checkpoint = 0;
                }
                (key, v)
            })
        })
    }

    /// Slots exposed for mutation since `snapshot` was synchronized. Each slot
    /// appears once; values may be unchanged. `None` requires a full comparison
    /// (for example after deserialization, replacement or an older checkpoint).
    /// Only mutations through the arena's mutable APIs are tracked.
    pub fn changed_slots_since(&self, snapshot: &Self) -> Option<&[u32]> {
        self.changes
            .as_ref()
            .filter(|_| self.checkpoint == snapshot.checkpoint)
            .map(|changes| changes.slots.as_slice())
    }

    /// Refresh a read-only snapshot, copying only touched slots when possible.
    /// Starts/acknowledges change tracking on `source`, retaining its log capacity.
    /// Clones and deserialized arenas have tracking disabled until synchronized.
    pub fn sync_snapshot_from(&mut self, source: &mut Self)
    where
        T: Clone,
    {
        static NEXT_CHECKPOINT: AtomicU64 = AtomicU64::new(1);
        if let Some(slots) = source.changed_slots_since(self) {
            self.slots.resize_with(source.slots.len(), || Slot {
                generation: 0,
                value: None,
            });
            for &slot in slots {
                let slot = slot as usize;
                self.slots[slot].clone_from(&source.slots[slot]);
            }
            if source
                .changes
                .as_ref()
                .is_some_and(|changes| changes.free_changed)
            {
                self.free.clone_from(&source.free);
            }
        } else {
            self.slots.clone_from(&source.slots);
            self.free.clone_from(&source.free);
        }
        source.checkpoint = NEXT_CHECKPOINT.fetch_add(1, Ordering::Relaxed);
        let changes = source.changes.get_or_insert_with(|| ChangeLog {
            slots: Vec::new(),
            marked: Vec::new(),
            free_changed: false,
        });
        changes.clear();
        self.checkpoint = source.checkpoint;
        self.changes = None;
    }

    /// Remove every element failing the predicate; returns how many were removed.
    pub fn retain(&mut self, mut keep: impl FnMut(K, &T) -> bool) -> usize {
        let doomed: Vec<K> = self
            .iter()
            .filter(|(k, v)| !keep(*k, v))
            .map(|(k, _)| k)
            .collect();
        for &k in &doomed {
            self.remove(k);
        }
        doomed.len()
    }
}

impl<K: ArenaKey, T> Index<K> for Arena<K, T> {
    type Output = T;

    fn index(&self, key: K) -> &T {
        self.get(key).expect("stale arena key")
    }
}

impl<K: ArenaKey, T> IndexMut<K> for Arena<K, T> {
    fn index_mut(&mut self, key: K) -> &mut T {
        self.get_mut(key).expect("stale arena key")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    arena_key!(TestKey);

    #[test]
    fn snapshots_follow_mutations_slot_reuse_and_replacements() {
        let mut live = Arena::<TestKey, i32>::new();
        let first = live.insert(10);
        let second = live.insert(20);
        let mut snapshot = Arena::new();
        snapshot.sync_snapshot_from(&mut live);
        let stale_snapshot = snapshot.clone();
        live[first] = 11;
        *live.values_mut().nth(1).unwrap() = 21;
        live.remove(first);
        let reused = live.insert(30);
        let appended = live.insert(40);
        let slots = live.changed_slots_since(&snapshot).unwrap();
        assert_eq!(
            slots.len(),
            3,
            "repeated mutation and slot reuse stay bounded"
        );
        assert!(
            slots.contains(&first.slot())
                && slots.contains(&second.slot())
                && slots.contains(&appended.slot())
        );
        snapshot.sync_snapshot_from(&mut live);
        assert_eq!(snapshot, live);
        assert_eq!(snapshot.get(first), None);
        assert_eq!(snapshot.get(reused), Some(&30));
        assert!(live.changed_slots_since(&stale_snapshot).is_none());

        // A clone is an independent branch; undo is an independent deserialization.
        let mut branch = live.clone();
        branch[second] = 99;
        assert!(branch.changed_slots_since(&snapshot).is_none());
        assert!(live.changed_slots_since(&branch).is_none());
        snapshot.sync_snapshot_from(&mut branch);
        assert_eq!(snapshot.get(second), Some(&99));
        assert_eq!(live.get(second), Some(&21));
        let saved = ron::to_string(&live).unwrap();
        let mut restored: Arena<TestKey, i32> = ron::from_str(&saved).unwrap();
        assert!(restored.changed_slots_since(&snapshot).is_none());
        snapshot.sync_snapshot_from(&mut restored);
        assert_eq!(snapshot, live);
        assert_eq!(ron::to_string(&snapshot).unwrap(), saved);
    }

    #[test]
    fn insert_get_remove_round_trip() {
        let mut a: Arena<TestKey, i32> = Arena::new();
        let k = a.insert(7);
        assert_eq!(a.get(k), Some(&7));
        assert_eq!(a.remove(k), Some(7));
        assert_eq!(a.get(k), None);
        assert_eq!(a.remove(k), None);
    }

    #[test]
    fn reused_slot_stales_the_old_key() {
        let mut a: Arena<TestKey, i32> = Arena::new();
        let k1 = a.insert(1);
        a.remove(k1);
        let k2 = a.insert(2);
        assert_eq!(k2.slot(), k1.slot(), "slot reused");
        assert_ne!(k2.generation(), k1.generation());
        assert_eq!(a.get(k1), None, "old key is stale");
        assert_eq!(a.get(k2), Some(&2));
        assert_eq!(
            a.key_at_slot(k1.slot()),
            Some(k2),
            "slot resolves to the live key"
        );
        a.remove(k2);
        assert_eq!(
            a.key_at_slot(k1.slot()),
            None,
            "free slot resolves to nothing"
        );
    }

    #[test]
    fn iteration_is_slot_ordered_and_skips_free() {
        let mut a: Arena<TestKey, i32> = Arena::new();
        let k1 = a.insert(1);
        let k2 = a.insert(2);
        let k3 = a.insert(3);
        a.remove(k2);
        assert_eq!(
            a.get(k1),
            Some(&1),
            "unrelated removal never invalidates a key"
        );
        assert_eq!(a.get(k3), Some(&3));
        assert_eq!(a.len(), 2);
        let values: Vec<i32> = a.values().copied().collect();
        assert_eq!(values, [1, 3]);
    }

    #[test]
    fn serde_snapshot_preserves_keys() {
        let mut a: Arena<TestKey, i32> = Arena::new();
        let k1 = a.insert(1);
        a.remove(k1);
        let k2 = a.insert(2);
        let text = ron::to_string(&a).expect("serializes");
        let back: Arena<TestKey, i32> = ron::from_str(&text).expect("round-trips");
        assert_eq!(back.get(k2), Some(&2));
        assert_eq!(back.get(k1), None, "generation survived the snapshot");
    }
}
