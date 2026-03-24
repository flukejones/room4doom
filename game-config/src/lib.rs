//! Startup configuration, input command types, and game-mode enums.
//!
//! These types are used across many crates (input, gamestate, game-exe,
//! renderers) but have no dependency on game logic or map data.

mod doom_def;
pub mod tic_cmd;

pub use doom_def::{GameMission, GameMode, WeaponType};
pub use tic_cmd::{
    ANGLETURN, BASELOOKDIRMAX, BASELOOKDIRMIN, ButtonCode, FORWARDMOVE, MAXPLMOVE, SIDEMOVE, SLOWTURNTICS, TIC_CMD_BUTTONS, TicCmd
};

use std::error::Error;
use std::fmt;
use std::str::FromStr;

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
    /// Play this demo lump immediately and exit when done.
    pub demo: Option<String>,
    pub netgame: bool,
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
            demo: None,
            netgame: false,
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
