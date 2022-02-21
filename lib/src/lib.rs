// #![feature(const_fn_floating_point_arithmetic)]

use std::f32::consts::PI;
use std::fmt;
use std::ops::{Deref, DerefMut};

pub mod angle;
pub mod d_main;
pub mod doom_def;
pub mod errors;
pub mod flags;
pub mod game;
pub mod info;
pub mod level_data;
pub mod play;
pub mod sounds;
pub mod tic_cmd;

/// Functions purely as a safe fn wrapper around a `NonNull` because we know that
/// the Map structure is not going to change under us
pub struct DPtr<T> {
    p: *mut T,
}

impl<T> DPtr<T> {
    fn new(t: &T) -> DPtr<T> {
        DPtr {
            p: t as *const _ as *mut _,
        }
    }

    fn as_ptr(&self) -> *mut T {
        self.p
    }
}

impl<T> PartialEq for DPtr<T> {
    fn eq(&self, other: &Self) -> bool {
        self.p == other.p
    }
}

impl<T> Clone for DPtr<T> {
    fn clone(&self) -> DPtr<T> {
        DPtr { p: self.p }
    }
}

impl<T> Deref for DPtr<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.p }
    }
}

impl<T> DerefMut for DPtr<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.p }
    }
}

impl<T> AsRef<T> for DPtr<T> {
    fn as_ref(&self) -> &T {
        unsafe { &*self.p }
    }
}

impl<T> AsMut<T> for DPtr<T> {
    fn as_mut(&mut self) -> &mut T {
        unsafe { &mut *self.p }
    }
}

impl<T: fmt::Debug> fmt::Debug for DPtr<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ptr->{:?}->{:#?}", self.p, unsafe { self.p.as_ref() })
    }
}

pub fn radian_range(rad: f32) -> f32 {
    if rad < 0.0 {
        return rad + 2.0 * PI;
    } else if rad >= 2.0 * PI {
        return rad - 2.0 * PI;
    }
    rad
}
