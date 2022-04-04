use std::str::FromStr;

use gameplay::{log, Skill};

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
    pub episode: i32,
    pub map: i32,
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
