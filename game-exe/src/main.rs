mod cheats;
mod config;
mod d_main;
mod shaders;
mod test_funcs;
mod timestep;
mod wipe;

use dirs::{cache_dir, data_dir};
use std::{env::set_var, error::Error, fs::File, io::Write, path::PathBuf, str::FromStr};

use d_main::d_doom_loop;
use env_logger::fmt::Color;
use gamestate::{DoomOptions, Game};
use golem::*;
use gumdrop::Options;

use crate::config::UserConfig;
use gameplay::{log, Skill};
use input::Input;
use shaders::Shaders;
use sound_sdl2::timidity::{make_timidity_cfg, GusMemSize};

use crate::log::{info, warn};
use wad::WadData;

const SOUND_DIR: &str = "room4doom/sound/";
const TIMIDITY_CFG: &str = "timidity.cfg";
const BASE_DIR: &str = "room4doom/";

#[derive(Debug, Clone, Copy)]
pub enum ShaderType {
    Basic,
    Lottes,
    Cgwg,
}

impl FromStr for ShaderType {
    type Err = std::io::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "basic" => Ok(ShaderType::Basic),
            "lottes" => Ok(ShaderType::Lottes),
            "cgwg" => Ok(ShaderType::Cgwg),
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "Doh! ShaderType invalid",
            )),
        }
    }
}

/// CLI options for the game-exe
#[derive(Debug, Clone, Options)]
pub struct CLIOptions {
    #[options(
        help = "verbose level: off, error, warn, info, debug",
        default = "info"
    )]
    pub verbose: log::LevelFilter,
    #[options(no_short, meta = "", help = "path to game-exe WAD")]
    pub iwad: String,
    #[options(free, help = "path to patch WAD")]
    pub pwad: Vec<String>,
    #[options(meta = "", help = "resolution width in pixels", default = "0")]
    pub width: u32,
    #[options(meta = "", help = "resolution height in pixels", default = "0")]
    pub height: u32,
    #[options(meta = "", help = "fullscreen?")]
    pub fullscreen: Option<bool>,

    #[options(help = "Disable monsters")]
    pub no_monsters: bool,
    // #[options(help = "Monsters respawn after being killed")]
    // pub respawn_parm: bool,
    // #[options(help = "Monsters move faster")]
    // pub fast_parm: bool,
    // #[options(
    //     no_short,
    //     help = "Developer mode. F1 saves a screenshot in the current working directory"
    // )]
    // pub dev_parm: bool,
    // #[options(
    //     meta = "",
    //     help = "Start a deathmatch game-exe: 1 = classic, 2 = Start a deathmatch 2.0 game-exe.  Weapons do not stay in place and all items respawn after 30 seconds"
    // )]
    // pub deathmatch: u8,
    // pub autostart: bool,
    #[options(
        meta = "",
        help = "Set the game-exe skill, 0-4 (0: easiest, 4: hardest)"
    )]
    pub skill: Skill,
    #[options(meta = "", help = "Select episode", default = "0")]
    pub episode: i32,
    #[options(meta = "", help = "Select level in episode", default = "0")]
    pub map: i32,
    #[options(help = "game-exe options help")]
    pub help: bool,

    #[options(help = "palette test, cycles through palette display")]
    pub palette_test: bool,
    #[options(meta = "", help = "image test, pass the sprite name to render")]
    pub image_test: Option<String>,
    #[options(help = "image test, cycle through the patches for texture compose")]
    pub image_cycle_test: bool,
    #[options(help = "texture compose test, cycle through the composable textures")]
    pub texture_test: bool,
    #[options(help = "flat texture test, cycle through the floor/ceiling flats")]
    pub flats_test: bool,
    #[options(help = "sprite test, cycle through the sprites")]
    pub sprites_test: bool,

    #[options(meta = "", help = "Screen shader <basic, cgwg, lottes>")]
    pub shader: Option<Shaders>,
}

impl From<CLIOptions> for DoomOptions {
    fn from(g: CLIOptions) -> Self {
        DoomOptions {
            iwad: g.iwad,
            pwad: g.pwad,
            no_monsters: g.no_monsters,
            // respawn_parm: g.respawn_parm,
            // fast_parm: g.fast_parm,
            // dev_parm: g.dev_parm,
            // deathmatch: g.deathmatch,
            // autostart: g.autostart,
            skill: g.skill,
            episode: g.episode,
            map: g.map,
            warp: g.map != 0 || g.episode != 0,
            verbose: g.verbose,
            ..DoomOptions::default()
        }
    }
}

fn setup_timidity(wad: &WadData) {
    if let Some(mut path) = data_dir() {
        path.push(SOUND_DIR);
        if path.exists() {
            let mut cache_dir = cache_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
            cache_dir.push(TIMIDITY_CFG);
            if let Some(cfg) = make_timidity_cfg(wad, path, GusMemSize::Perfect) {
                let mut file = File::create(cache_dir.as_path()).unwrap();
                file.write_all(&cfg).unwrap();
                set_var("SDL_MIXER_DISABLE_FLUIDSYNTH", "1");
                set_var("TIMIDITY_CFG", cache_dir.as_path());
                info!("Using timidity for sound");
            } else {
                warn!("Sound fonts were missing, using fluidsynth instead");
            }
        } else {
            info!("No sound fonts installed to {:?}", path);
            info!("Using fluidsynth for sound");
        }
    }
}

/// The main `game-exe` crate should take care of initialising a few things
fn main() -> Result<(), Box<dyn Error>> {
    let mut options = CLIOptions::parse_args_default_or_exit();

    let mut logger = env_logger::Builder::new();
    logger
        .target(env_logger::Target::Stdout)
        .format(move |buf, record| {
            let mut style = buf.style();
            let colour = match record.level() {
                log::Level::Error => Color::Red,
                log::Level::Warn => Color::Yellow,
                log::Level::Info => Color::Green,
                log::Level::Debug => Color::Magenta,
                log::Level::Trace => Color::Magenta,
            };
            style.set_color(colour);

            if options.verbose == log::Level::Debug {
                writeln!(
                    buf,
                    "{}: {}: {}",
                    style.value(record.level()),
                    record.target(),
                    record.args()
                )
            } else {
                //record.target().split("::").last().unwrap_or("")
                writeln!(buf, "{}: {}", style.value(record.level()), record.args())
            }
        })
        .filter(None, options.verbose)
        .init();

    let mut user_config = UserConfig::load();

    let sdl_ctx = sdl2::init()?;
    let snd_ctx = sdl_ctx.audio()?;
    let video_ctx = sdl_ctx.video()?;

    let events = sdl_ctx.event_pump()?;
    let input = Input::new(events);

    user_config.sync_cli(&mut options);
    user_config.write();

    let mut window = video_ctx
        .window("ROOM for DOOM", options.width, options.height)
        .allow_highdpi()
        .position_centered()
        .opengl()
        .build()?;
    let _gl_ctx = window.gl_create_context()?;

    let context = unsafe {
        Context::from_glow(glow::Context::from_loader_function(|s| {
            video_ctx.gl_get_proc_address(s) as *const _
        }))
        .unwrap()
    };

    let wad = WadData::new(options.iwad.clone().into());
    setup_timidity(&wad);
    let game = Game::new(
        options.clone().into(),
        wad,
        snd_ctx,
        user_config.sfx_vol,
        user_config.mus_vol,
    );

    if let Some(fullscreen) = options.fullscreen {
        if fullscreen {
            let mode = if options.width != 320 {
                sdl2::video::FullscreenType::Desktop
            } else {
                sdl2::video::FullscreenType::True
            };
            window.set_fullscreen(mode)?;
        }
    }
    window.show();

    sdl_ctx.mouse().show_cursor(false);
    sdl_ctx.mouse().set_relative_mouse_mode(true);
    sdl_ctx.mouse().capture(true);

    d_doom_loop(game, input, window, context, options)?;
    Ok(())
}
