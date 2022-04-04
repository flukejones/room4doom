//! Everything related to gameplay lives here. This is stuff like:
//! - world movers
//! - monster data and actions
//! - shooty stuff and damage
//! - stuff like that...

pub mod d_thinker; // required by level data
pub mod enemy; // required by states
pub mod map_object; // info, level data, game, bsp
pub mod player;
pub mod player_sprite; // info/states
pub mod specials; // game
pub mod utilities; // level data node // many places

use std::{error::Error, fmt, str::FromStr};

mod ceiling;
mod doors;
mod floor;
mod interaction;
mod lights;
mod movement;
mod platforms;
mod switch;
mod teleport;

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

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Skill {
    NoItems = -1, // the "-skill 0" hack
    Baby = 0,
    Easy = 1,
    Medium = 2,
    Hard = 3,
    Nightmare = 4,
}

impl Default for Skill {
    fn default() -> Self {
        Skill::Medium
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
