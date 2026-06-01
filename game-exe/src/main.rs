#![doc = include_str!("../../README.md")]

mod cheats;
mod cli;
mod config;
mod d_main;
#[cfg(feature = "display-sdl2")]
mod loop_sdl2;
#[cfg(any(feature = "display-softbuffer", feature = "display-pixels"))]
mod loop_winit;
mod timestep;

use cli::*;
use mimalloc::MiMalloc;
use simplelog::TermLogger;
use std::error::Error;
use std::path::{Path, PathBuf};

use gamestate::{Game, prepare_wad};

use crate::config::UserConfig;
#[cfg(feature = "display-sdl2")]
use log::info;
use log::warn;
use sound_common::{SndServerTx, SoundAction};
use wad::WadData;

#[cfg(feature = "display-sdl2")]
use config::WindowMode;
#[cfg(feature = "display-sdl2")]
use loop_sdl2::d_doom_loop_sdl2;
#[cfg(all(
    any(feature = "display-softbuffer", feature = "display-pixels"),
    not(feature = "display-sdl2")
))]
use loop_winit::DoomApp;
#[cfg(feature = "display-sdl2")]
use render_backend::DisplayBackend;
#[cfg(all(
    any(feature = "display-softbuffer", feature = "display-pixels"),
    not(feature = "display-sdl2")
))]
use winit::event_loop::EventLoop;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

/// The main `game-exe` crate should take care of initialising a few things
fn main() -> Result<(), Box<dyn Error>> {
    let mut options: CLIOptions = argh::from_env();

    TermLogger::init(
        options.verbose.unwrap_or(log::LevelFilter::Info),
        simplelog::ConfigBuilder::default()
            .set_time_level(log::LevelFilter::Trace)
            .build(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    )?;

    let mut user_config = UserConfig::load();
    user_config.sync_cli(&mut options);
    user_config.write();

    let wad_path: PathBuf = options.iwad.clone().into();
    let wad = WadData::new(&wad_path);
    let (game_options, wad) = prepare_wad(options.clone().into(), wad);

    #[cfg(feature = "display-sdl2")]
    {
        run_sdl2(game_options, wad, &user_config, options)?;
    }

    #[cfg(all(
        any(feature = "display-softbuffer", feature = "display-pixels"),
        not(feature = "display-sdl2")
    ))]
    {
        run_winit(game_options, wad, &user_config, options)?;
    }

    Ok(())
}

#[cfg(feature = "display-sdl2")]
fn run_sdl2(
    game_options: game_config::GameOptions,
    wad: WadData,
    user_config: &UserConfig,
    options: CLIOptions,
) -> Result<(), Box<dyn Error>> {
    let sdl_ctx = sdl2::init()?;
    info!("Init SDL2 main");
    let video_ctx = sdl_ctx.video()?;
    info!("Init SDL2 video");

    let (snd_tx, snd_thread) = init_sound(&sdl_ctx, &wad, user_config);

    let num_disp = video_ctx.num_video_displays()?;
    for n in 0..num_disp {
        info!("Found display {:?}", video_ctx.display_name(n)?);
    }

    let mut window = video_ctx
        .window("ROOM4DOOM", options.width, options.height)
        .hidden()
        .position_centered()
        .build()?;

    match options.window_mode.unwrap_or(WindowMode::Windowed) {
        WindowMode::Windowed => {
            window.set_fullscreen(sdl2::video::FullscreenType::Off)?;
        }
        WindowMode::Borderless => {
            window.set_fullscreen(sdl2::video::FullscreenType::Desktop)?;
        }
        WindowMode::Exclusive => {
            window.set_fullscreen(sdl2::video::FullscreenType::True)?;
        }
    }

    let input = input::InputSdl2::new(sdl_ctx.event_pump()?, (&user_config.input).into());

    sdl_ctx.mouse().show_cursor(false);
    sdl_ctx.mouse().set_relative_mouse_mode(true);
    sdl_ctx.mouse().capture(true);

    let mut canvas_builder = window.into_canvas().target_texture();
    if matches!(options.vsync, Some(true)) {
        canvas_builder = canvas_builder.present_vsync();
    }
    let mut canvas = canvas_builder.build()?;
    info!("Built display window");
    canvas.window_mut().show();
    {
        let w = canvas.window();
        let (win_w, win_h) = w.size();
        let (draw_w, draw_h) = w.drawable_size();
        info!(
            "Window: {}x{}, drawable: {}x{}, fullscreen: {:?}",
            win_w,
            win_h,
            draw_w,
            draw_h,
            w.fullscreen_state()
        );
    }

    let display = DisplayBackend::new_sdl2(canvas);
    let mut game = Game::new(
        game_options,
        wad,
        snd_tx,
        snd_thread,
        user_config.to_config_array(),
    );
    game.pic_data.set_crt_gamma(user_config.crt_gamma);
    d_doom_loop_sdl2(game, input, display, options, user_config.clone())?;
    Ok(())
}

#[cfg(all(
    any(feature = "display-softbuffer", feature = "display-pixels"),
    not(feature = "display-sdl2")
))]
fn run_winit(
    game_options: game_config::GameOptions,
    wad: WadData,
    user_config: &UserConfig,
    options: CLIOptions,
) -> Result<(), Box<dyn Error>> {
    let (snd_tx, snd_thread) = init_sound_no_sdl(&wad, user_config);
    let input_state = input::InputState::new((&user_config.input).into());
    let event_loop = EventLoop::new().expect("failed to create winit event loop");

    let mut game = Game::new(
        game_options,
        wad,
        snd_tx,
        snd_thread,
        user_config.to_config_array(),
    );
    game.pic_data.set_crt_gamma(user_config.crt_gamma);
    let mut app = DoomApp::new(game, input_state, options, user_config.clone());
    event_loop.run_app(&mut app)?;
    Ok(())
}

/// Initialise the sound server (SDL2 display path).
#[cfg(feature = "display-sdl2")]
fn init_sound(
    _sdl_ctx: &sdl2::Sdl,
    wad: &WadData,
    config: &UserConfig,
) -> (SndServerTx, std::thread::JoinHandle<()>) {
    init_sound_rodio(wad, config)
}

/// Initialise the sound server (winit display path).
#[cfg(any(feature = "display-softbuffer", feature = "display-pixels"))]
fn init_sound_no_sdl(
    wad: &WadData,
    config: &UserConfig,
) -> (SndServerTx, std::thread::JoinHandle<()>) {
    init_sound_rodio(wad, config)
}

/// Spawn the rodio sound server thread. Asset loading happens here on
/// the calling thread (so the spawned thread doesn't block on file I/O),
/// then `sound_rodio::spawn` constructs the server and audio sink
/// entirely on the new thread — keeping the `!Send` cpal handle from
/// ever crossing a thread boundary. The server runs silently if no
/// audio output device is available, so this never falls back to a stub.
fn init_sound_rodio(
    wad: &WadData,
    config: &UserConfig,
) -> (SndServerTx, std::thread::JoinHandle<()>) {
    let music_type = match config.music_type {
        config::MusicType::GUS => sound_common::MusicType::GUS,
        config::MusicType::OPL3 => sound_common::MusicType::OPL3,
        config::MusicType::OPL2 => sound_common::MusicType::OPL2,
    };
    let sf2_path = if config.sf2_path.is_empty() {
        None
    } else {
        let p = Path::new(&config.sf2_path);
        if p.is_absolute() {
            Some(p.to_path_buf())
        } else {
            Some(gameplay::dirs::config_dir().join(&config.sf2_path))
        }
    };
    let snd_config = sound_rodio::SndConfig::from_wad(wad, music_type, sf2_path.as_deref());
    let (tx, thread) = sound_rodio::spawn(snd_config);
    if let Err(e) = tx.send(SoundAction::SfxVolume(config.sfx_vol)) {
        warn!("Failed to send initial sfx volume: {e}");
    }
    if let Err(e) = tx.send(SoundAction::MusicVolume(config.mus_vol)) {
        warn!("Failed to send initial music volume: {e}");
    }
    (tx, thread)
}
