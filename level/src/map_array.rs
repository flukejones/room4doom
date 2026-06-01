//! Address-stable, non-growable storage for interlinked map geometry.
//!
//! Doom map data is deeply self-referential — linedefs point at vertices and
//! sectors, segs at sectors and linedefs, etc. — and the engine stores those
//! links as raw pointers ([`crate::MapPtr`]) into the geometry arrays. That is
//! only sound if the backing storage never moves: a `Vec` reallocation on
//! growth would dangle every pointer into it.
//!
//! `MapArray<T>` is a [`Box<[T]>`] with no growth API. Once built it cannot be
//! resized or moved internally, so pointers into it are stable for its whole
//! life. The build pattern is "allocate every array to its exact, known count
//! (default-filled), then fill and link in a single pass" — every slot exists
//! up front, so forward references between arrays are never a problem.

use std::ops::{Deref, DerefMut};

/// A fixed-size, address-stable array of map data.
///
/// Derefs to `[T]`, so all slice operations (`len`, `iter`, `get`, indexing)
/// are available. There is deliberately no `push`/`insert`/`resize` — the
/// backing buffer can never grow or relocate, which is what keeps raw pointers
/// into it (via [`crate::MapPtr`]) valid.
#[derive(Debug, Clone)]
pub struct MapArray<T> {
    data: Box<[T]>,
}

impl<T> MapArray<T> {
    /// Build an array of `len` default-filled elements. Allocated once at the
    /// exact size; never grows.
    pub fn filled(len: usize) -> Self
    where
        T: Default,
    {
        let mut v = Vec::with_capacity(len);
        v.resize_with(len, T::default);
        Self {
            data: v.into_boxed_slice(),
        }
    }

    /// Build directly from a fully-populated `Vec`, freezing it at its current
    /// size.
    pub fn from_vec(v: Vec<T>) -> Self {
        Self {
            data: v.into_boxed_slice(),
        }
    }
}

impl<T> Default for MapArray<T> {
    fn default() -> Self {
        Self {
            data: Vec::new().into_boxed_slice(),
        }
    }
}

impl<T> Deref for MapArray<T> {
    type Target = [T];

    fn deref(&self) -> &[T] {
        &self.data
    }
}

impl<T> DerefMut for MapArray<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        &mut self.data
    }
}

impl<T> FromIterator<T> for MapArray<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Self::from_vec(iter.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filled_has_exact_len_of_defaults() {
        let a: MapArray<i32> = MapArray::filled(4);
        assert_eq!(a.len(), 4);
        assert!(a.iter().all(|&x| x == 0));
    }

    #[test]
    fn deref_gives_slice_ops() {
        let a = MapArray::from_vec(vec![10, 20, 30]);
        assert_eq!(a.len(), 3);
        assert_eq!(a[1], 20);
        assert_eq!(a.get(5), None);
        assert_eq!(a.iter().sum::<i32>(), 60);
    }

    #[test]
    fn mut_access_in_place_does_not_reallocate() {
        let mut a = MapArray::filled(3);
        let ptr_before = a.as_ptr();
        a[0] = 1;
        a[2] = 9;
        // In-place writes must not move the backing buffer — the whole point.
        assert_eq!(a.as_ptr(), ptr_before);
        assert_eq!(&*a, &[1, 0, 9]);
    }

    #[test]
    fn from_iter_collects() {
        let a: MapArray<i32> = (0..3).collect();
        assert_eq!(&*a, &[0, 1, 2]);
    }
}
