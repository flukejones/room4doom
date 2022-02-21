use gumdrop::Options;
use std::{error::Error, fmt, str::FromStr};

use crate::{doom_def::GameMission, doom_def::GameMode};

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

#[derive(Debug, Options)]
pub struct GameOptions {
    #[options(
        help = "verbose level: off, error, warn, info, debug",
        default = "warn"
    )]
    pub verbose: log::LevelFilter,
    #[options(no_short, help = "path to game WAD", default = "./doom1.wad")]
    pub iwad: String,
    #[options(no_short, help = "path to patch WAD")]
    pub pwad: Option<String>,
    #[options(help = "resolution width in pixels", default = "640")]
    pub width: u32,
    #[options(help = "resolution height in pixels", default = "480")]
    pub height: u32,
    #[options(help = "fullscreen?")]
    pub fullscreen: bool,

    #[options(help = "Disable monsters")]
    pub no_monsters: bool,
    #[options(help = "Monsters respawn after being killed")]
    pub respawn_parm: bool,
    #[options(help = "Monsters move faster")]
    pub fast_parm: bool,
    #[options(
        no_short,
        help = "Developer mode. F1 saves a screenshot in the current working directory"
    )]
    pub dev_parm: bool,
    #[options(
        help = "Start a deathmatch game: 1 = classic, 2 = Start a deathmatch 2.0 game.  Weapons do not stay in place and all items respawn after 30 seconds"
    )]
    pub deathmatch: u8,
    #[options(
        help = "Set the game skill, 1-5 (1: easiest, 5: hardest). A skill of 0 disables all monsters"
    )]
    pub skill: Skill,
    #[options(help = "Select episode", default = "1")]
    pub episode: u32,
    #[options(help = "Select level in episode", default = "1")]
    pub map: u32,
    pub autostart: bool,
    #[options(help = "game options help")]
    pub help: bool,
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
