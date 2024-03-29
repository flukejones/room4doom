//! The gameplay crate is purely gameplay. It loads a level from the wad, all definitions, and level state.
//!
//! The `Gameplay` is very self contained, such that it really only expects input, the player thinkers to be
//! run, and the MapObject thinkers to be run. Theowner of the `Gameplay` is then expected to get what is
//! required to display the results from the exposed public API.

// #![feature(const_fn_floating_point_arithmetic)]
#![allow(clippy::new_without_default)]

use std::{
    f32::consts::PI,
    fmt::{self, Debug},
    ops::{Deref, DerefMut},
};

#[cfg(null_check)]
use std::panic;

mod angle;
mod doom_def;
pub(crate) mod env;
#[rustfmt::skip]
mod info;
mod lang;
mod level;
mod pic;
mod player;
mod player_sprite;
mod thing;
mod thinker;
pub mod tic_cmd;
pub(crate) mod utilities;

pub use angle::Angle;
pub use doom_def::{
    AmmoType, Card, GameAction, GameMission, GameMode, WeaponType, DOOM_VERSION, MAXPLAYERS,
    TICRATE, WEAPON_INFO,
};
pub use env::specials::{spawn_specials, update_specials};
pub use info::MapObjKind;
pub use lang::english;
pub use level::{
    flags::LineDefFlags,
    map_data::{MapData, IS_SSECTOR_MASK},
    map_defs::{Node, Sector, Segment, SubSector},
    Level,
};
pub use pic::{FlatPic, PicAnimation, PicData, Switches, WallPic};
pub use player::{Player, PlayerCheat, PlayerState, PlayerStatus, WBPlayerStruct, WBStartStruct};
pub use player_sprite::PspDef;
use std::{error::Error, str::FromStr};
pub use thing::{MapObjFlag, MapObject};
pub use utilities::{m_clear_random, m_random, p_random, point_to_angle_2};

// re-export
pub use glam;
pub use log;

#[derive(Debug)]
pub enum DoomArgError {
    InvalidSkill(String),
}

impl Error for DoomArgError {}

impl fmt::Display for DoomArgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DoomArgError::InvalidSkill(m) => write!(f, "{}", m),
        }
    }
}

#[repr(i32)]
#[derive(Debug, Default, Copy, Clone, PartialEq, PartialOrd)]
pub enum Skill {
    NoItems = -1, // the "-skill 0" hack
    Baby = 0,
    Easy = 1,
    #[default]
    Medium = 2,
    Hard = 3,
    Nightmare = 4,
}

impl From<i32> for Skill {
    fn from(w: i32) -> Self {
        if w > Skill::Nightmare as i32 {
            panic!("{} is not a variant of Skill", w);
        }
        unsafe { std::mem::transmute(w) }
    }
}

impl FromStr for Skill {
    type Err = DoomArgError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "0" => Ok(Skill::Baby),
            "1" => Ok(Skill::Easy),
            "2" => Ok(Skill::Medium),
            "3" => Ok(Skill::Hard),
            "4" => Ok(Skill::Nightmare),
            _ => Err(DoomArgError::InvalidSkill("Invalid arg".to_owned())),
        }
    }
}

/// Functions purely as a safe fn wrapper around a `NonNull` because we know that
/// the Map structure is not going to change under us
pub struct DPtr<T: Debug> {
    inner: *mut T,
}

impl<T: Debug> DPtr<T> {
    fn new(t: &mut T) -> DPtr<T> {
        DPtr { inner: t as *mut _ }
    }
}

impl<T: Debug> PartialEq for DPtr<T> {
    fn eq(&self, other: &Self) -> bool {
        #[cfg(null_check)]
        if self.inner.is_null() {
            panic!("NULL");
        }
        self.inner == other.inner
    }
}

impl<T: Debug> Clone for DPtr<T> {
    fn clone(&self) -> DPtr<T> {
        #[cfg(null_check)]
        if self.inner.is_null() {
            panic!("NULL");
        }
        DPtr { inner: self.inner }
    }
}

impl<T: Debug> Deref for DPtr<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        #[cfg(null_check)]
        if self.inner.is_null() {
            panic!("NULL");
        }
        unsafe { &*self.inner }
    }
}

impl<T: Debug> DerefMut for DPtr<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        #[cfg(null_check)]
        if self.inner.is_null() {
            panic!("NULL");
        }
        unsafe { &mut *self.inner }
    }
}

impl<T: Debug> AsRef<T> for DPtr<T> {
    fn as_ref(&self) -> &T {
        #[cfg(null_check)]
        if self.inner.is_null() {
            panic!("NULL");
        }
        unsafe { &*self.inner }
    }
}

impl<T: Debug> AsMut<T> for DPtr<T> {
    fn as_mut(&mut self) -> &mut T {
        #[cfg(null_check)]
        if self.inner.is_null() {
            panic!("NULL");
        }
        unsafe { &mut *self.inner }
    }
}

#[cfg(null_check)]
impl<T: Debug> Drop for DPtr<T> {
    fn drop(&mut self) {
        if self.inner.is_null() {
            panic!("Can not drop DPtr with an inner null");
        }
    }
}

impl<T: Debug> Debug for DPtr<T> {
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
