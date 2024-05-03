use argh::FromArgs;
use gameplay::{log, Skill};
use gamestate::DoomOptions;
use render_target::shaders::Shaders;

use crate::config::{self, MusicType};

/// CLI options for the game-exe
#[derive(Debug, Clone, FromArgs)]
pub struct CLIOptions {
    /// verbose level: off, error, warn, info, debug
    #[argh(option)]
    pub verbose: Option<log::LevelFilter>,
    /// path to game-exe WAD
    #[argh(option, default = "Default::default()")]
    pub iwad: String,
    /// path to patch WAD
    #[argh(option)]
    pub pwad: Vec<String>,
    /// resolution width in pixels
    #[argh(option, default = "0")]
    pub width: u32,
    /// resolution height in pixels
    #[argh(option, default = "0")]
    pub height: u32,
    /// fullscreen?
    #[argh(option)]
    pub fullscreen: Option<bool>,
    /// double-resolution?
    #[argh(option)]
    pub double: Option<bool>,
    /// disable monsters
    #[argh(option, default = "false")]
    pub no_monsters: bool,
    // /// Monsters respawn after being killed
    // pub respawn_parm: bool,
    // /// Monsters move faster
    // pub fast_parm: bool,
    /// developer mode. Screen is cleared with green colour for seg/flat drawing
    /// leak checks
    #[argh(option, default = "false")]
    pub dev_parm: bool,
    //     help = "Start a deathmatch game-exe: 1 = classic, 2 = Start a deathmatch 2.0 game-exe.
    // Weapons do not stay in place and all items respawn after 30 seconds" pub deathmatch: u8,
    // pub autostart: bool,
    /// set the game-exe skill, 0-4 (0: easiest, 4: hardest)
    #[argh(option)]
    pub skill: Option<Skill>,
    /// select episode
    #[argh(option)]
    pub episode: Option<i32>,
    /// select level in episode. If Doom II the episode is ignored
    #[argh(option)]
    pub map: Option<i32>,

    /// palette test, cycles through palette display
    #[argh(option, default = "false")]
    pub palette_test: bool,
    /// image test, pass the sprite name to render
    #[argh(option)]
    pub image_test: Option<String>,
    /// image test, cycle through the patches for texture compose
    #[argh(option, default = "false")]
    pub image_cycle_test: bool,
    /// texture compose test, cycle through the composable textures
    #[argh(option, default = "false")]
    pub texture_test: bool,
    /// flat texture test, cycle through the floor/ceiling flats
    #[argh(option, default = "false")]
    pub flats_test: bool,
    /// sprite test, cycle through the sprites
    #[argh(option, default = "false")]
    pub sprites_test: bool,
    /// rendering type <software, softopengl>
    #[argh(option)]
    pub rendering: Option<config::RenderType>,
    /// screen shader <cgwg, lottes, lottesbasic>, not used with Software
    /// renderer
    #[argh(option)]
    pub shader: Option<Shaders>,
    /// music type <fluidsynth, timidity(default)>. Unfinished
    #[argh(option)]
    pub music_type: Option<MusicType>,
}

impl From<CLIOptions> for DoomOptions {
    fn from(g: CLIOptions) -> Self {
        DoomOptions {
            iwad: g.iwad,
            pwad: g.pwad,
            no_monsters: g.no_monsters,
            dev_parm: g.dev_parm,
            skill: g.skill.unwrap_or_default(),
            episode: g.episode.unwrap_or_default(),
            map: g.map.unwrap_or_default(),
            warp: g.map.is_some() || g.episode.is_some(),
            hi_res: g.double.unwrap_or(true),
            verbose: g.verbose.unwrap_or(log::LevelFilter::Warn),
            respawn_parm: false,
            fast_parm: false,
            deathmatch: 0,
            autostart: false,
        }
    }
}
