use std::{error::Error, str::FromStr, fmt};

use gumdrop::{Options};

#[derive(Debug)]
pub enum DoomArgError {
    InvalidSkill(String)
}

impl Error for DoomArgError {}

impl fmt::Display for DoomArgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DoomArgError::InvalidSkill(m) => write!(f, "{}", m),
        }
    }
}

#[derive(Debug)]
pub enum Skill {
    NoItems = -1, // the "-skill 0" hack
    Baby    = 0,
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
            _ => Err(DoomArgError::InvalidSkill("Invalid arg".to_owned()))
        }
    }
}

#[derive(Debug, Options)]
pub struct GameOptions {
    #[options(help = "path to game WAD", required)]
    pub iwad:       String,
    #[options(help = "path to patch WAD")]
    pub pwad:       Option<String>,
    #[options(help = "resolution width in pixels")]
    pub width:      u32,
    #[options(help = "resolution height in pixels")]
    pub height:     u32,
    #[options(help = "fullscreen?")]
    pub fullscreen: bool,

    #[options(help = "Disable monsters")]
    pub no_monsters:   bool,
    #[options(help = "Monsters respawn after being killed")]
    pub respawn_parm:  bool,
    #[options(help = "Monsters move faster")]
    pub fast_parm:     bool,
    #[options(
        help = "Developer mode. F1 saves a screenshot in the current working directory"
    )]
    pub dev_parm:      bool,
    #[options(
        help = "Start a deathmatch game: 1 = classic, 2 = Start a deathmatch 2.0 game.  Weapons do not stay in place and all items respawn after 30 seconds"
    )]
    pub deathmatch:    u8,
    #[options(
        help = "Set the game skill, 1-5 (1: easiest, 5: hardest). A skill of 0 disables all monsters"
    )]
    pub start_skill:   Skill,
    #[options(help = "Select episode")]
    pub start_episode: u32,
    #[options(help = "Select map in episode")]
    pub start_map:     u32,
    pub autostart:     bool,
}

impl Default for GameOptions {
    fn default() -> Self {
        Self {
            fullscreen: false,
            iwad: "./doom1.wad".to_owned(),
            pwad: None,
            height: 640,
            width: 480,
            //
            no_monsters:   false,
            respawn_parm:  false,
            fast_parm:     false,
            dev_parm:      false,
            deathmatch:    2,
            start_skill:   Skill::Medium,
            start_episode: 1,
            start_map:     1,
            autostart:     false,
        }
    }
}
