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
#[cfg(feature = "sound-sdl2")]
use dirs::data_dir;
use mimalloc::MiMalloc;
use simplelog::TermLogger;
use std::error::Error;
use std::path::{Path, PathBuf};

use gamestate::{Game, prepare_wad};

use crate::config::UserConfig;
use log::warn;
use sound_common::{SndServerTx, SoundAction, SoundServer, SoundServerTic};
use wad::WadData;

#[cfg(feature = "sound-sdl2")]
use config::MusicType;
#[cfg(feature = "display-sdl2")]
use config::WindowMode;
#[cfg(feature = "sound-sdl2")]
use dirs::cache_dir;
#[cfg(feature = "sound-sdl2")]
use sound_sdl2::timidity::{GusMemSize, make_timidity_cfg};
#[cfg(feature = "sound-sdl2")]
use std::env::set_var;
#[cfg(feature = "sound-sdl2")]
use std::fs::File;
#[cfg(feature = "sound-sdl2")]
use std::io::Write;

#[cfg(feature = "sound-sdl2")]
const SOUND_DIR: &str = "room4doom/sound/";
#[cfg(feature = "sound-sdl2")]
const TIMIDITY_CFG: &str = "timidity.cfg";

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
    use loop_sdl2::d_doom_loop_sdl2;
    use render_backend::DisplayBackend;

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
    use loop_winit::DoomApp;
    use winit::event_loop::EventLoop;

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

/// Initialise the SDL2 sound backend.
#[cfg(all(feature = "sound-sdl2", feature = "display-sdl2"))]
fn init_sound(
    sdl_ctx: &sdl2::Sdl,
    wad: &WadData,
    config: &UserConfig,
) -> (SndServerTx, std::thread::JoinHandle<()>) {
    let snd_ctx = sdl_ctx.audio().expect("SDL2 audio init failed");
    info!("Init SDL2 sound");

    setup_timidity(config.music_type, config.gus_mem_size, wad);

    let music_type = match config.music_type {
        config::MusicType::OPL3 => sound_sdl2::MusicType::OPL3,
        config::MusicType::OPL2 | config::MusicType::GUS => sound_sdl2::MusicType::OPL2,
    };

    match sound_sdl2::Snd::new(snd_ctx, wad, music_type) {
        Ok(mut s) => {
            let tx = s.init().unwrap();
            let thread = std::thread::spawn(move || while s.tic() {});
            tx.send(SoundAction::SfxVolume(config.sfx_vol)).unwrap();
            tx.send(SoundAction::MusicVolume(config.mus_vol)).unwrap();
            (tx, thread)
        }
        Err(e) => {
            warn!("Could not set up SDL2 sound server: {e}");
            init_nosnd()
        }
    }
}

/// Initialise the rodio sound backend (SDL2 display path).
#[cfg(all(
    feature = "sound-rodio",
    not(feature = "sound-sdl2"),
    feature = "display-sdl2"
))]
fn init_sound(
    _sdl_ctx: &sdl2::Sdl,
    wad: &WadData,
    config: &UserConfig,
) -> (SndServerTx, std::thread::JoinHandle<()>) {
    init_sound_rodio(wad, config)
}

/// No sound backend selected (SDL2 display path).
#[cfg(all(
    not(any(feature = "sound-sdl2", feature = "sound-rodio")),
    feature = "display-sdl2"
))]
fn init_sound(
    _sdl_ctx: &sdl2::Sdl,
    wad: &WadData,
    _config: &UserConfig,
) -> (SndServerTx, std::thread::JoinHandle<()>) {
    init_nosnd()
}

/// Initialise sound without SDL2 context (winit path).
#[cfg(all(
    feature = "sound-rodio",
    any(feature = "display-softbuffer", feature = "display-pixels")
))]
fn init_sound_no_sdl(
    wad: &WadData,
    config: &UserConfig,
) -> (SndServerTx, std::thread::JoinHandle<()>) {
    init_sound_rodio(wad, config)
}

#[cfg(all(
    not(feature = "sound-rodio"),
    any(feature = "display-softbuffer", feature = "display-pixels"),
    not(feature = "display-sdl2")
))]
fn init_sound_no_sdl(
    wad: &WadData,
    _config: &UserConfig,
) -> (SndServerTx, std::thread::JoinHandle<()>) {
    init_nosnd()
}

/// Initialise the rodio sound backend.
#[cfg(feature = "sound-rodio")]
fn init_sound_rodio(
    wad: &WadData,
    config: &UserConfig,
) -> (SndServerTx, std::thread::JoinHandle<()>) {
    let music_type = match config.music_type {
        config::MusicType::GUS => sound_rodio::MusicType::GUS,
        config::MusicType::OPL3 => sound_rodio::MusicType::OPL3,
        _ => sound_rodio::MusicType::OPL2,
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
    match sound_rodio::Snd::new(wad, music_type, sf2_path.as_deref()) {
        Ok(mut s) => {
            let tx = s.init().unwrap();
            let thread = std::thread::spawn(move || while s.tic() {});
            tx.send(SoundAction::SfxVolume(config.sfx_vol)).unwrap();
            tx.send(SoundAction::MusicVolume(config.mus_vol)).unwrap();
            (tx, thread)
        }
        Err(e) => {
            warn!("Could not set up rodio sound server: {e}");
            init_nosnd()
        }
    }
}

/// Fallback: no-sound backend.
fn init_nosnd() -> (SndServerTx, std::thread::JoinHandle<()>) {
    let mut s = sound_nosnd::Snd::new().unwrap();
    let tx = s.init().unwrap();
    let thread = std::thread::spawn(move || while s.tic() {});
    (tx, thread)
}

#[cfg(feature = "sound-sdl2")]
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
