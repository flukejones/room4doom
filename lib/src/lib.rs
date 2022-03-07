// #![feature(const_fn_floating_point_arithmetic)]

use std::{
    f32::consts::PI,
    fmt,
    ops::{Deref, DerefMut},
};

mod angle;
mod cheats;
mod d_main;
mod doom_def;
mod errors;
mod flags;
mod game;
mod info;
mod level_data;
mod play;
mod sounds;
mod textures;
pub mod tic_cmd;

pub use angle::Angle;
pub use cheats::Cheats;
pub use d_main::{DoomOptions, Shaders, Skill};
pub use doom_def::{GameMission, WeaponType};
pub use flags::LineDefFlags;
pub use game::Game;
pub use level_data::{
    map_data::{MapData, IS_SSECTOR_MASK},
    map_defs::{Sector, Segment, SubSector},
    Level,
};
pub use play::{
    map_object::MapObject,
    player::{Player, PlayerCheat},
};
pub use textures::TextureData;

pub use log;

pub type Texture = Vec<Vec<usize>>;

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
