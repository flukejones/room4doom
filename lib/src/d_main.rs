use gumdrop::Options;
use std::{error::Error, fmt, str::FromStr};

use crate::doom_def::{GameMission, GameMode};

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
    Easy,
    Medium,
    Hard,
    Nightmare,
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

#[derive(Debug)]
pub struct DoomOptions {
    pub iwad: String,
    pub pwad: Option<String>,
    pub no_monsters: bool,
    pub respawn_parm: bool,
    pub fast_parm: bool,
    pub dev_parm: bool,
    pub deathmatch: u8,
    pub skill: Skill,
    pub episode: u32,
    pub map: u32,
    pub autostart: bool,
    pub verbose: log::LevelFilter,
}

#[derive(Debug, Clone, Copy)]
pub enum Shaders {
    Basic,
    Lottes,
    Cgwg,
}

impl FromStr for Shaders {
    type Err = std::io::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "basic" => Ok(Shaders::Basic),
            "lottes" => Ok(Shaders::Lottes),
            "cgwg" => Ok(Shaders::Cgwg),
            _ => Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "Doh!")),
        }
    }
}

pub fn identify_version(wad: &wad::WadData) -> (GameMode, GameMission, String) {
    let game_mode;
    let game_mission;
    let game_description;

    if wad.lump_exists("MAP01") {
        game_mission = GameMission::Doom2;
    } else if wad.lump_exists("E1M1") {
        game_mission = GameMission::Doom;
    } else {
        panic!("Could not determine IWAD type");
    }

    if game_mission == GameMission::Doom {
        // Doom 1.  But which version?
        if wad.lump_exists("E4M1") {
            game_mode = GameMode::Retail;
            game_description = String::from("The Ultimate DOOM");
        } else if wad.lump_exists("E3M1") {
            game_mode = GameMode::Registered;
            game_description = String::from("DOOM Registered");
        } else {
            game_mode = GameMode::Shareware;
            game_description = String::from("DOOM Shareware");
        }
    } else {
        game_mode = GameMode::Commercial;
        game_description = String::from("DOOM 2: Hell on Earth");
        // TODO: check for TNT or Plutonia
    }
    (game_mode, game_mission, game_description)
}
