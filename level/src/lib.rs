//! Map data structures, BSP construction, and 3D polygon generation.
//!
//! This crate contains all the level geometry data types and algorithms
//! extracted from the gameplay crate for reuse by tools and renderers.
//! BSP construction is handled by the `rbsp` crate.

#![allow(clippy::new_without_default)]

use std::fmt::{self, Debug};
use std::ops::{Deref, DerefMut};
use std::ptr::null_mut;

pub mod bsp3d;
pub mod flags;
pub mod level_data;
pub mod map_defs;
// Re-exports for convenience
pub use bsp3d::movers::is_sector_mover;
pub use bsp3d::{
    AABB, BSP3D, BSPLeaf3D, MovementType, Node3D, OcclusionSeg, SurfaceKind, SurfacePolygon, WallTexPin, WallType
};
pub use flags::LineDefFlags;
pub use level_data::LevelData;
pub use map_defs::{
    BBox, Blockmap, IS_SSECTOR_MASK, LineDef, Node, Sector, SectorHeight, Segment, SideDef, SlopeType, SubSector, Vertex, is_subsector, mark_subsector, subsector_index
};
/// This exists to allow breaking the rules of borrows and in some cases
/// lifetimes.
///
/// Where you will see it used most is in references to the map
/// structure - things like linking segs with lines, subsectors etc, the maps in
/// Doom are very self-referential with a need to be able to follow any
/// subsector to any other, from any line or seg.
///
/// It is also for allowing thinkers (e.g, Doors, Lights) to keep a mutable
/// reference to Sectors or lines they need to control (without having to jump
/// through flaming hoops).
pub struct MapPtr<T: Debug> {
    inner: *mut T,
}

impl<T: Debug> MapPtr<T> {
    pub fn new(t: &mut T) -> MapPtr<T> {
        MapPtr {
            inner: t as *mut _,
        }
    }

    /// This should only ever be used in cases where the `MapPtr` itself will be
    /// replaced.
    ///
    /// # Safety
    ///
    /// Either replace the `MapPtr` with a valid type before use, or check null
    /// status with `is_null()` (it will always be null as there is no way to
    /// set the internal pointer).
    ///
    /// Test builds should be run with `null_check` feature occasionally.
    pub unsafe fn new_null() -> MapPtr<T> {
        MapPtr {
            inner: null_mut(),
        }
    }

    pub fn is_null(&self) -> bool {
        self.inner.is_null()
    }

    /// Get the raw pointer to the inner value.
    pub fn as_ptr(&self) -> *mut T {
        self.inner
    }
}

impl<T: Debug> PartialEq for MapPtr<T> {
    fn eq(&self, other: &Self) -> bool {
        #[cfg(feature = "null_check")]
        if self.inner.is_null() {
            panic!("NULL");
        }
        self.inner == other.inner
    }
}

impl<T: Debug> Clone for MapPtr<T> {
    fn clone(&self) -> MapPtr<T> {
        #[cfg(feature = "null_check")]
        if self.inner.is_null() {
            panic!("NULL");
        }
        MapPtr {
            inner: self.inner,
        }
    }
}

impl<T: Debug> Deref for MapPtr<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        #[cfg(feature = "null_check")]
        if self.inner.is_null() {
            panic!("NULL");
        }
        unsafe { &*self.inner }
    }
}

impl<T: Debug> DerefMut for MapPtr<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        #[cfg(feature = "null_check")]
        if self.inner.is_null() {
            panic!("NULL");
        }
        unsafe { &mut *self.inner }
    }
}

impl<T: Debug> AsRef<T> for MapPtr<T> {
    fn as_ref(&self) -> &T {
        #[cfg(feature = "null_check")]
        if self.inner.is_null() {
            panic!("NULL");
        }
        unsafe { &*self.inner }
    }
}

impl<T: Debug> AsMut<T> for MapPtr<T> {
    fn as_mut(&mut self) -> &mut T {
        #[cfg(feature = "null_check")]
        if self.inner.is_null() {
            panic!("NULL");
        }
        unsafe { &mut *self.inner }
    }
}

#[cfg(feature = "null_check")]
impl<T: Debug> Drop for MapPtr<T> {
    fn drop(&mut self) {
        if self.inner.is_null() {
            panic!("Can not drop DPtr with an inner null");
        }
    }
}

impl<T: Debug> Debug for MapPtr<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ptr->{:?}->{:#?}", self.inner, unsafe {
            self.inner.as_ref()
        })
    }
}

pub fn radian_range(rad: f32) -> f32 {
    use std::f32::consts::TAU;
    if rad < 0.0 {
        return rad + TAU;
    } else if rad >= TAU {
        return rad - TAU;
    }
    rad
}
