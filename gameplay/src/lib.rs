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

pub mod dirs;
mod doom_def;
pub(crate) mod env;
#[rustfmt::skip]
mod info;
mod lang;
mod level;
mod pic;
mod player;
mod player_sprite;
pub mod save;
pub(crate) mod sector_ext;
mod thing;
mod thinker;
pub mod tic_cmd;
pub(crate) mod utilities;

pub use doom_def::{
    AmmoType, Card, DOOM_VERSION, GameAction, GameMission, GameMode, MAXPLAYERS, PowerType, TICRATE, WEAPON_INFO, WeaponType
};
pub use env::specials::{respawn_specials, spawn_specials, update_specials};
pub use env::teleport::teleport_move;
pub use info::{MapObjKind, STATES, StateNum};
pub use lang::english;
pub use level::Level;
// Re-export from map-data crate
pub use map_data::{
    AABB, BBox, BSP3D, BSPLeaf3D, DivLine, IS_SSECTOR_MASK, LineDefFlags, MapData, MapPtr, MightSee, Mightsee, MovementType, Node, Node3D, OcclusionSeg, PVS2D, Portal, Portals, PvsCluster, PvsData, PvsFile, PvsFileError, PvsView2D, RenderPvs, Sector, Segment, SubSector, SurfaceKind, SurfacePolygon, WallTexPin, WallType, is_subsector, mark_subsector, pvs_load_from_cache, subsector_index
};
pub use math::{Angle, m_clear_random, m_random, p_random, point_to_angle_2};
pub use pic::{FlatPic, MipLevel, PicAnimation, PicData, Switches, WallPic};
pub use player::{Player, PlayerCheat, PlayerState, PlayerStatus, WorldEndPlayerInfo};
pub use player_sprite::PspDef;
pub use sector_ext::SectorExt;
use std::error::Error;
use std::fmt::{self, Debug};
use std::str::FromStr;
pub use thing::{MapObjFlag, MapObject};
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

/// PVS preprocessing mode for `--preprocess-pvs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreprocessPvsMode {
    /// Full frustum-clip flood pass (most accurate).
    Full,
    /// Mightsee only — skip frustum pass (faster, more conservative).
    Mightsee,
    /// Cluster-based PVS.
    Cluster,
}

impl FromStr for PreprocessPvsMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "full" => Ok(Self::Full),
            "mightsee" => Ok(Self::Mightsee),
            "cluster" => Ok(Self::Cluster),
            other => Err(format!(
                "unknown pvs mode '{other}'; expected full, mightsee, or cluster"
            )),
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
    /// PVS preprocessing mode. `None` means no preprocessing.
    pub preprocess_pvs: Option<PreprocessPvsMode>,
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
            preprocess_pvs: None,
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

pub fn radian_range(rad: f32) -> f32 {
    if rad < 0.0 {
        return rad + TAU;
    } else if rad >= TAU {
        return rad - TAU;
    }
    rad
}
