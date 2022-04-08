mod cheats;
mod d_main;
mod game;
mod input;
mod shaders;
mod test_funcs;
mod timestep;

use std::{error::Error, io::Write, str::FromStr};

use d_main::d_doom_loop;
use env_logger::fmt::Color;
use game::Game;
use golem::*;
use gumdrop::Options;

use gameplay::{log, Skill};
use input::Input;
use shaders::Shaders;

/// Options specific to Doom. This will get phased out for `GameOptions`
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

/// CLI options for the game
#[derive(Debug, Clone, Options)]
pub struct GameOptions {
    #[options(
        help = "verbose level: off, error, warn, info, debug",
        default = "info"
    )]
    pub verbose: log::LevelFilter,
    #[options(
        no_short,
        meta = "",
        help = "path to game WAD",
        default = "./doom1.wad"
    )]
    pub iwad: String,
    #[options(no_short, meta = "", help = "path to patch WAD")]
    pub pwad: Option<String>,
    #[options(meta = "", help = "resolution width in pixels", default = "640")]
    pub width: u32,
    #[options(meta = "", help = "resolution height in pixels", default = "480")]
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
        meta = "",
        help = "Start a deathmatch game: 1 = classic, 2 = Start a deathmatch 2.0 game.  Weapons do not stay in place and all items respawn after 30 seconds"
    )]
    pub deathmatch: u8,
    #[options(
        meta = "",
        help = "Set the game skill, 1-5 (1: easiest, 5: hardest). A skill of 0 disables all monsters"
    )]
    pub skill: Skill,
    #[options(meta = "", help = "Select episode", default = "1")]
    pub episode: i32,
    #[options(meta = "", help = "Select level in episode", default = "1")]
    pub map: i32,
    pub autostart: bool,
    #[options(help = "game options help")]
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

impl From<GameOptions> for DoomOptions {
    fn from(g: GameOptions) -> Self {
        DoomOptions {
            iwad: g.iwad,
            pwad: g.pwad,
            no_monsters: g.no_monsters,
            respawn_parm: g.respawn_parm,
            fast_parm: g.fast_parm,
            dev_parm: g.dev_parm,
            deathmatch: g.deathmatch,
            skill: g.skill,
            episode: g.episode,
            map: g.map,
            autostart: g.autostart,
            verbose: g.verbose,
        }
    }
}

/// The main `game` crate should take care of initialising a few things
fn main() -> Result<(), Box<dyn Error>> {
    let sdl_ctx = sdl2::init()?;
    let video_ctx = sdl_ctx.video()?;

    let events = sdl_ctx.event_pump()?;
    let input = Input::new(events);

    let options = GameOptions::parse_args_default_or_exit();

    let mut logger = env_logger::Builder::new();
    logger
        .target(env_logger::Target::Stdout)
        .format(|buf, record| {
            let mut style = buf.style();
            let colour = match record.level() {
                log::Level::Error => Color::Red,
                log::Level::Warn => Color::Yellow,
                log::Level::Info => Color::Green,
                log::Level::Debug => Color::Magenta,
                log::Level::Trace => Color::Magenta,
            };
            style.set_color(colour);
            writeln!(buf, "{}: {}", style.value(record.level()), record.args())
        })
        .filter(None, options.verbose)
        .init();

    let mut window = video_ctx
        .window("ROOM (Rusty DOOM)", options.width, options.height)
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

    let game = Game::new(options.clone().into());

    if options.fullscreen {
        let mode = if options.width != 320 {
            sdl2::video::FullscreenType::Desktop
        } else {
            sdl2::video::FullscreenType::True
        };
        window.set_fullscreen(mode)?;
    }
    window.show();

    sdl_ctx.mouse().show_cursor(false);
    sdl_ctx.mouse().set_relative_mouse_mode(true);
    sdl_ctx.mouse().capture(true);

    d_doom_loop(game, input, window, context, options)
}
