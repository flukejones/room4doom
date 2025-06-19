#![doc = include_str!("../../README.md")]

mod cheats;
mod cli;
mod config;
mod d_main;
mod timestep;

use cli::*;
use config::MusicType;
use dirs::{cache_dir, data_dir};
use gamestate_traits::sdl2::{self};
use mimalloc::MiMalloc;
use simplelog::TermLogger;
use std::env::set_var;
use std::error::Error;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use d_main::d_doom_loop;
use gamestate::Game;

use crate::config::UserConfig;
use gameplay::log;
use input::Input;
use sound_sdl2::timidity::{GusMemSize, make_timidity_cfg};

use crate::log::{info, warn};
use wad::WadData;

const SOUND_DIR: &str = "room4doom/sound/";
const TIMIDITY_CFG: &str = "timidity.cfg";
const BASE_DIR: &str = "room4doom/";

fn setup_timidity(music_type: MusicType, gus_mem: GusMemSize, wad: &WadData) {
    if music_type == MusicType::FluidSynth {
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { set_var("SDL_MIXER_DISABLE_FLUIDSYNTH", "0") };
        info!("Using fluidsynth for sound");
        return;
    }
    if let Some(mut path) = data_dir() {
        path.push(SOUND_DIR);
        if path.exists() {
            let mut cache_dir = cache_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
            cache_dir.push(TIMIDITY_CFG);
            if let Some(cfg) = make_timidity_cfg(wad, path, gus_mem) {
                let mut file = File::create(cache_dir.as_path()).unwrap();
                file.write_all(&cfg).unwrap();
                // TODO: Audit that the environment access only happens in single-threaded code.
                unsafe { set_var("SDL_MIXER_DISABLE_FLUIDSYNTH", "1") };
                // TODO: Audit that the environment access only happens in single-threaded code.
                unsafe { set_var("TIMIDITY_CFG", cache_dir.as_path()) };
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

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

/// The main `game-exe` crate should take care of initialising a few things
fn main() -> Result<(), Box<dyn Error>> {
    let mut options: CLIOptions = argh::from_env();

    TermLogger::init(
        log::LevelFilter::Info,
        simplelog::ConfigBuilder::default()
            .set_time_level(log::LevelFilter::Trace)
            .build(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    )?;

    let mut user_config = UserConfig::load();
    user_config.sync_cli(&mut options);
    user_config.write();

    let sdl_ctx = sdl2::init()?;
    info!("Init SDL2 main");
    let snd_ctx = sdl_ctx.audio()?;
    info!("Init SDL2 sound");
    let video_ctx = sdl_ctx.video()?;
    info!("Init SDL2 video");

    let wad = WadData::new(user_config.iwad.clone().into());
    setup_timidity(user_config.music_type, user_config.gus_mem_size, &wad);

    let game = Game::new(
        options.clone().into(),
        wad,
        snd_ctx,
        user_config.sfx_vol,
        user_config.mus_vol,
    );

    let num_disp = video_ctx.num_video_displays()?;
    for n in 0..num_disp {
        info!("Found display {:?}", video_ctx.display_name(n)?);
    }
    let mut window = video_ctx
        .window("ROOM4DOOM", 0, 0)
        .opengl()
        .hidden()
        .build()?;

    if let Some(fullscreen) = options.fullscreen {
        if fullscreen {
            window.set_fullscreen(sdl2::video::FullscreenType::Desktop)?;
        } else {
            window.set_fullscreen(sdl2::video::FullscreenType::Off)?;
        }
    }

    let _gl_ctx = window.gl_create_context()?;
    let gl_ctx = unsafe {
        golem::Context::from_glow(golem::glow::Context::from_loader_function(|s| {
            video_ctx.gl_get_proc_address(s) as *const _
        }))
        .unwrap()
    };

    let gl_attr = video_ctx.gl_attr();
    gl_attr.set_context_profile(sdl2::video::GLProfile::Core);

    let input = Input::new(sdl_ctx.event_pump()?, (&user_config.input).into());

    sdl_ctx.mouse().show_cursor(false);
    sdl_ctx.mouse().set_relative_mouse_mode(true);
    sdl_ctx.mouse().capture(true);

    d_doom_loop(game, input, window, gl_ctx, options)?;
    Ok(())
}
