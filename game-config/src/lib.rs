//! Startup configuration, input command types, and game-mode enums.
//!
//! These types are used across many crates (input, gamestate, game-exe,
//! renderers) but have no dependency on game logic or map data.

mod doom_def;
pub mod tic_cmd;

pub use doom_def::WeaponType;
// GameMode/GameMission are IWAD identity and live with the WAD data; the
// re-export keeps existing `game_config::GameMode` imports working.
pub use tic_cmd::{
    ANGLETURN, BASELOOKDIRMAX, BASELOOKDIRMIN, ButtonCode, FORWARDMOVE, MAXPLMOVE, SIDEMOVE,
    SLOWTURNTICS, TIC_CMD_BUTTONS, TicCmd,
};
pub use wad::types::{GameMission, GameMode};

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
            Self::InvalidSkill(m) => write!(f, "{m}"),
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
            iwad: "doom.wad".to_owned(),
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

#[expect(
    clippy::fallible_impl_from,
    reason = "preserve the existing From API's panic contract for invalid Doom skill indices"
)]
impl From<i32> for Skill {
    fn from(w: i32) -> Self {
        match w {
            -1 => Self::NoItems,
            0 => Self::Baby,
            1 => Self::Easy,
            2 => Self::Medium,
            3 => Self::Hard,
            4 => Self::Nightmare,
            _ => panic!("{w} is not a variant of Skill"),
        }
    }
}

impl From<u8> for Skill {
    fn from(w: u8) -> Self {
        Self::from(i32::from(w))
    }
}

#[expect(
    clippy::fallible_impl_from,
    reason = "preserve the existing From API's panic contract for invalid Doom skill indices"
)]
impl From<usize> for Skill {
    fn from(w: usize) -> Self {
        match w {
            0 => Self::Baby,
            1 => Self::Easy,
            2 => Self::Medium,
            3 => Self::Hard,
            4 => Self::Nightmare,
            _ => panic!("{w} is not a variant of Skill"),
        }
    }
}

impl FromStr for Skill {
    type Err = DoomArgError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "1" => Ok(Self::Baby),
            "2" => Ok(Self::Easy),
            "3" => Ok(Self::Medium),
            "4" => Ok(Self::Hard),
            "5" => Ok(Self::Nightmare),
            _ => Err(DoomArgError::InvalidSkill("Invalid arg".to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Skill;

    #[test]
    fn skill_no_items_sentinel_is_preserved() {
        assert_eq!(Skill::from(-1), Skill::NoItems);
    }

    #[test]
    #[should_panic(expected = "not a variant of Skill")]
    fn skill_below_no_items_is_rejected() {
        let _ = Skill::from(-2);
    }

    #[test]
    #[should_panic(expected = "not a variant of Skill")]
    fn oversized_skill_index_is_rejected() {
        let _ = Skill::from(usize::MAX);
    }
}
