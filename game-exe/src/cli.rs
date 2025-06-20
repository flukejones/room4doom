use argh::FromArgs;
use gameplay::{GameOptions, Skill, log};
use render_target::shaders::Shaders;

use crate::config::{self, MusicType};

/// CLI options for the game-exe
#[derive(Debug, Clone, FromArgs)]
pub struct CLIOptions {
    /// verbose level: off, error, warn, info, debug
    #[argh(option, short = 'v')]
    pub verbose: Option<log::LevelFilter>,
    /// path to game-exe WAD
    #[argh(option, default = "Default::default()", short = 'i')]
    pub iwad: String,
    /// path to patch WAD
    #[argh(option, short = 'p')]
    pub pwad: Vec<String>,
    /// resolution width in pixels
    #[argh(option, default = "0", short = 'w')]
    pub width: u32,
    /// resolution height in pixels
    #[argh(option, default = "0", short = 'h')]
    pub height: u32,
    /// fullscreen?
    #[argh(option, short = 'f')]
    pub fullscreen: Option<bool>,
    /// set high-res is using software rendering
    #[argh(switch, short = 'H')]
    pub hi_res: bool,
    /// set low-res is using software rendering, If used with hi-res switch then
    /// lo-res takes precedence
    #[argh(switch, short = 'L')]
    pub lo_res: bool,
    /// disable monsters
    #[argh(switch, short = 'n')]
    pub no_monsters: bool,
    // /// Monsters respawn after being killed
    // pub respawn_parm: bool,
    // /// Monsters move faster
    // pub fast_parm: bool,
    /// developer mode. Screen is cleared with green colour for seg/flat drawing
    /// leak checks
    #[argh(switch)]
    pub dev_parm: bool,
    //     help = "Start a deathmatch game-exe: 1 = classic, 2 = Start a deathmatch 2.0 game-exe.
    // Weapons do not stay in place and all items respawn after 30 seconds" pub deathmatch: u8,
    // pub autostart: bool,
    /// set the game-exe skill, 0-4 (0: easiest, 4: hardest)
    #[argh(option, short = 's')]
    pub skill: Option<Skill>,
    /// select episode
    #[argh(option, short = 'e')]
    pub episode: Option<usize>,
    /// select level in episode. If Doom II the episode is ignored
    #[argh(option, short = 'm')]
    pub map: Option<usize>,
    /// rendering type <software, software3d, softopengl>
    #[argh(option, short = 'r')]
    pub rendering: Option<config::RenderType>,
    /// screen shader <lottes, lottesbasic>, not used with Software
    /// renderer
    #[argh(option, short = 'S')]
    pub shader: Option<Shaders>,
    /// music type <fluidsynth, timidity(default), opl2>
    #[argh(option, short = 'M')]
    pub music_type: Option<MusicType>,
    /// enable demo playback (currently bad due to f32 used in movements)
    #[argh(switch, short = 'E')]
    pub enable_demos: bool,
    /// preprocess PVS data for loaded WADs and exit
    #[argh(switch)]
    pub preprocess_pvs: bool,
}

impl From<CLIOptions> for GameOptions {
    fn from(g: CLIOptions) -> Self {
        GameOptions {
            iwad: g.iwad,
            pwad: g.pwad,
            no_monsters: g.no_monsters,
            dev_parm: g.dev_parm,
            skill: g.skill.unwrap_or_default(),
            episode: g.episode.unwrap_or_default(),
            map: g.map.unwrap_or_default(),
            warp: g.map.is_some() || g.episode.is_some(),
            hi_res: g.hi_res && !g.lo_res,
            verbose: g.verbose.unwrap_or(log::LevelFilter::Warn),
            respawn_parm: false,
            respawn_monsters: false,
            fast_parm: false,
            deathmatch: 0,
            autostart: false,
            enable_demos: g.enable_demos,
            netgame: false,
            preprocess_pvs: g.preprocess_pvs,
        }
    }
}
