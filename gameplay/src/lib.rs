// #![feature(const_fn_floating_point_arithmetic)]
#![allow(clippy::new_without_default)]

use std::{
    f32::consts::PI,
    fmt,
    ops::{Deref, DerefMut},
};

mod angle;
mod doom_def;
mod info;
mod lang;
mod level;
mod pic;
pub(crate) mod play;
mod thinker;
pub mod tic_cmd;

pub use angle::Angle;
pub use doom_def::{GameAction, GameMission, GameMode, WeaponType, DOOM_VERSION, MAXPLAYERS};
pub use glam;
pub use info::MapObjectType;
pub use lang::english;
pub use level::{
    flags::LineDefFlags,
    map_data::{MapData, IS_SSECTOR_MASK},
    map_defs::{Node, Sector, Segment, SubSector},
    Level,
};
pub use log;
pub use pic::{FlatPic, PicAnimation, PicData, Switches, WallPic};
pub use play::{
    mobj::MapObject,
    player::{Player, PlayerCheat, PlayerState, WBStartStruct},
    player_sprite::PspDef,
    specials::{spawn_specials, update_specials},
    utilities::{m_clear_random, p_random},
    Skill,
};

/// Functions purely as a safe fn wrapper around a `NonNull` because we know that
/// the Map structure is not going to change under us
pub struct DPtr<T> {
    inner: *mut T,
}

impl<T> DPtr<T> {
    fn new(t: &mut T) -> DPtr<T> {
        DPtr { inner: t as *mut _ }
    }

    fn as_ptr(&self) -> *mut T {
        self.inner
    }
}

impl<T> PartialEq for DPtr<T> {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl<T> Clone for DPtr<T> {
    fn clone(&self) -> DPtr<T> {
        DPtr { inner: self.inner }
    }
}

impl<T> Deref for DPtr<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.inner }
    }
}

impl<T> DerefMut for DPtr<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.inner }
    }
}

impl<T> AsRef<T> for DPtr<T> {
    fn as_ref(&self) -> &T {
        unsafe { &*self.inner }
    }
}

impl<T> AsMut<T> for DPtr<T> {
    fn as_mut(&mut self) -> &mut T {
        unsafe { &mut *self.inner }
    }
}

impl<T: fmt::Debug> fmt::Debug for DPtr<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ptr->{:?}->{:#?}", self.inner, unsafe {
            self.inner.as_ref()
        })
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
