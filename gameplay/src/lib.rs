//! The gameplay crate is purely gameplay. It loads a level from the wad, all
//! definitions, and level state.
//!
//! The `Gameplay` is very self contained, such that it really only expects
//! input, the player thinkers to be run, and the MapObject thinkers to be run.
//! The owner of the `Gameplay` is then expected to get what is required to
//! display the results from the exposed public API.

// #![feature(const_fn_floating_point_arithmetic)]
#![allow(clippy::new_without_default)]

use std::f32::consts::TAU;
use std::fmt::{self, Debug};
use std::ops::{Deref, DerefMut};

#[cfg(feature = "null_check")]
use std::panic;
use std::ptr::null_mut;

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

pub use doom_def::{
    AmmoType, Card, DOOM_VERSION, GameAction, GameMission, GameMode, MAXPLAYERS, PowerType,
    TICRATE, WEAPON_INFO, WeaponType,
};
pub use env::specials::{respawn_specials, spawn_specials, update_specials};
pub use env::teleport::teleport_move;
pub use info::{MapObjKind, STATES, StateNum};
pub use lang::english;
pub use level::Level;
pub use level::bsp3d::{
    AABB, BSP3D, BSPLeaf3D, MovementType, Node3D, SurfaceKind, SurfacePolygon, WallTexPin, WallType,
};
pub use level::flags::LineDefFlags;
pub use level::map_data::MapData;
pub use level::map_defs::{Node, Sector, Segment, SubSector};
// pub use level::portals::{Portal, Portal3D, PortalType};
pub use level::pvs::PVS;
pub use level::triangulation::DivLine;
pub use math::{Angle, m_clear_random, m_random, p_random, point_to_angle_2};
pub use pic::{FlatPic, MipLevel, PicAnimation, PicData, Switches, WallPic};
pub use player::{Player, PlayerCheat, PlayerState, PlayerStatus, WorldEndPlayerInfo};
pub use player_sprite::PspDef;
use std::error::Error;
use std::str::FromStr;
pub use thing::{MapObjFlag, MapObject};
// re-export
pub use {glam, log};

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

/// Options specific to gameplay
#[derive(Clone)]
pub struct GameOptions {
    pub iwad: String,
    pub pwad: Vec<String>,
    pub no_monsters: bool,
    pub respawn_parm: bool,
    pub fast_parm: bool,
    pub dev_parm: bool,
    pub deathmatch: u8,
    pub warp: bool,
    pub skill: Skill,
    pub episode: usize,
    pub map: usize,
    pub hi_res: bool,
    pub verbose: log::LevelFilter,
    pub respawn_monsters: bool,
    pub autostart: bool,
    pub enable_demos: bool,
    pub netgame: bool,
    pub preprocess_pvs: bool,
}

impl Default for GameOptions {
    fn default() -> Self {
        Self {
            iwad: "doom.wad".to_string(),
            pwad: Default::default(),
            no_monsters: Default::default(),
            respawn_parm: Default::default(),
            fast_parm: Default::default(),
            dev_parm: Default::default(),
            deathmatch: Default::default(),
            skill: Default::default(),
            episode: Default::default(),
            map: Default::default(),
            respawn_monsters: false,
            warp: false,
            autostart: Default::default(),
            hi_res: true,
            verbose: log::LevelFilter::Info,
            enable_demos: false,
            netgame: false,
            preprocess_pvs: false,
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

impl From<u8> for Skill {
    fn from(w: u8) -> Self {
        Self::from(w as i32)
    }
}

impl From<usize> for Skill {
    fn from(w: usize) -> Self {
        if w > Skill::Nightmare as usize {
            panic!("{} is not a variant of Skill", w);
        }
        unsafe { std::mem::transmute(w as i32) }
    }
}

impl FromStr for Skill {
    type Err = DoomArgError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "1" => Ok(Skill::Baby),
            "2" => Ok(Skill::Easy),
            "3" => Ok(Skill::Medium),
            "4" => Ok(Skill::Hard),
            "5" => Ok(Skill::Nightmare),
            _ => Err(DoomArgError::InvalidSkill("Invalid arg".to_owned())),
        }
    }
}

/// This exists to allow breaking the rules of borrows and in some cases
/// lifetimes.
///
/// Where you will see it used most is in references to the map
/// structure - things like linkng segs with lines, subsectors etc, the maps in
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
    fn new(t: &mut T) -> MapPtr<T> {
        MapPtr { inner: t as *mut _ }
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
    unsafe fn new_null() -> MapPtr<T> {
        MapPtr { inner: null_mut() }
    }

    fn is_null(&self) -> bool {
        self.inner.is_null()
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
        MapPtr { inner: self.inner }
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
    if rad < 0.0 {
        return rad + TAU;
    } else if rad >= TAU {
        return rad - TAU;
    }
    rad
}
