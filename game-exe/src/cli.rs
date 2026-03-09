use argh::FromArgs;
use gameplay::{GameOptions, Skill, log};
use software3d::{DebugColourMode, DebugDrawOptions, DebugOverlay};

use crate::config::{self, MusicType};

fn parse_debug_draw_mod(input: &str) -> DebugDrawOptions {
    let mut opts = DebugDrawOptions::default();
    for token in input.split(',') {
        let token = token.trim();
        if token == "outline" {
            opts.outline = true;
        } else if token == "normals" {
            opts.normals = true;
        } else if token == "no_depth" {
            opts.no_depth = true;
        } else if let Some(hex) = token.strip_prefix("clear_") {
            opts.clear_colour = parse_hex_colour(hex);
        } else if let Some(val) = token.strip_prefix("alpha_") {
            opts.alpha = val.parse().ok();
        }
    }
    opts
}

fn parse_hex_colour(hex: &str) -> Option<[u8; 4]> {
    let s = hex.strip_prefix('#').unwrap_or(hex);
    if s.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some([r, g, b, 255])
}

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
    /// music type <fluidsynth, timidity(default), opl2>
    #[argh(option, short = 'M')]
    pub music_type: Option<MusicType>,
    /// enable demo playback (currently bad due to f32 used in movements)
    #[argh(switch, short = 'E')]
    pub enable_demos: bool,
    /// preprocess PVS data for loaded WADs and exit
    #[argh(switch)]
    pub preprocess_pvs: bool,
    /// use cluster-based PVS instead of loading from cache
    #[argh(switch)]
    pub pvs_cluster: bool,
    /// debug overlay mode (mutually exclusive): sector_id, depth, overdraw,
    /// wireframe
    #[argh(option)]
    pub dbg_draw_overlay: Option<DebugOverlay>,
    /// debug draw modifiers (comma-separated): outline, normals, clear_<hex>,
    /// alpha_<0-255>, no_depth
    #[argh(option)]
    pub dbg_draw_mod: Option<String>,
}

impl CLIOptions {
    pub fn debug_draw(&self) -> DebugDrawOptions {
        let mut opts = self
            .dbg_draw_mod
            .as_deref()
            .map(parse_debug_draw_mod)
            .unwrap_or_default();

        match self
            .dbg_draw_overlay
            .as_ref()
            .unwrap_or(&DebugOverlay::None)
        {
            DebugOverlay::None => {}
            DebugOverlay::SectorId => opts.colour_mode = DebugColourMode::SectorId,
            DebugOverlay::Depth => opts.colour_mode = DebugColourMode::Depth,
            DebugOverlay::Overdraw => opts.colour_mode = DebugColourMode::Overdraw,
            DebugOverlay::Wireframe => opts.wireframe = true,
        }

        opts
    }
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
