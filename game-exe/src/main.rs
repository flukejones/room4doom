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
use std::path::{Path, PathBuf};

use d_main::d_doom_loop;
use gamestate::Game;

use crate::config::UserConfig;
use gameplay::{MapData, PVS, PicData, log};
use input::Input;
use sound_sdl2::timidity::{GusMemSize, make_timidity_cfg};

use crate::log::{info, warn};
use wad::WadData;

const SOUND_DIR: &str = "room4doom/sound/";
const TIMIDITY_CFG: &str = "timidity.cfg";
const BASE_DIR: &str = "room4doom/";

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

    // Check if PVS preprocessing was requested
    if options.preprocess_pvs {
        info!("Starting PVS preprocessing...");
        if !options.iwad.is_empty() {
            let wad_path: PathBuf = options.iwad.clone().into();
            let wad = WadData::new(&wad_path);
            let pic_data = PicData::init(&wad);
            preprocess_pvs_for_wad(&wad_path, &wad, &pic_data)?;

            if !options.pwad.is_empty() {
                for pwad in &options.pwad {
                    let wad_path: PathBuf = pwad.into();
                    let wad = WadData::new(&wad_path);
                    preprocess_pvs_for_wad(&wad_path, &wad, &pic_data)?;
                }
            }
        }

        info!("PVS preprocessing completed. Exiting.");
        return Ok(());
    }

    let mut user_config = UserConfig::load();
    user_config.sync_cli(&mut options);
    user_config.write();

    let sdl_ctx = sdl2::init()?;
    info!("Init SDL2 main");
    let snd_ctx = sdl_ctx.audio()?;
    info!("Init SDL2 sound");
    let video_ctx = sdl_ctx.video()?;
    info!("Init SDL2 video");

    let wad_path: PathBuf = options.iwad.clone().into();
    let wad = WadData::new(&wad_path);
    setup_timidity(user_config.music_type, user_config.gus_mem_size, &wad);

    let music_type = match user_config.music_type {
        config::MusicType::Timidity => sound_sdl2::MusicType::Timidity,
        config::MusicType::FluidSynth => sound_sdl2::MusicType::FluidSynth,
        config::MusicType::OPL2 => sound_sdl2::MusicType::OPL2,
        config::MusicType::OPL3 => sound_sdl2::MusicType::OPL3,
    };

    let game = Game::new(
        options.clone().into(),
        wad,
        snd_ctx,
        user_config.sfx_vol,
        user_config.mus_vol,
        music_type,
    );

    let num_disp = video_ctx.num_video_displays()?;
    for n in 0..num_disp {
        info!("Found display {:?}", video_ctx.display_name(n)?);
    }

    let mut window = video_ctx.window("ROOM4DOOM", 0, 0).hidden().build()?;

    if let Some(fullscreen) = options.fullscreen {
        if fullscreen {
            window.set_fullscreen(sdl2::video::FullscreenType::Desktop)?;
        } else {
            window.set_fullscreen(sdl2::video::FullscreenType::Off)?;
        }
    }

    let input = Input::new(sdl_ctx.event_pump()?, (&user_config.input).into());

    sdl_ctx.mouse().show_cursor(false);
    sdl_ctx.mouse().set_relative_mouse_mode(true);
    sdl_ctx.mouse().capture(true);

    d_doom_loop(game, input, window, options)?;
    Ok(())
}

fn preprocess_pvs_for_wad(
    wad_path: &Path,
    wad: &WadData,
    pic_data: &PicData,
) -> Result<(), Box<dyn Error>> {
    let wad_name = wad_path.file_stem().unwrap().to_str().unwrap();
    info!("Processing PVS for WAD: {}", wad_name);
    let maps = get_all_maps(&wad);
    info!("Found {} maps to process", maps.len());
    for map_name in maps {
        info!("Processing PVS for map: {}", map_name);
        match process_map_pvs(&wad, &map_name, &pic_data) {
            Ok(_) => info!("Successfully processed PVS for {}", map_name),
            Err(e) => log::error!("Failed to process PVS for {}: {}", map_name, e),
        }
    }

    Ok(())
}

fn get_all_maps(wad: &WadData) -> Vec<String> {
    let mut maps = Vec::new();

    // Look for Doom 1 episode maps (E1M1-E4M9)
    for episode in 1..=9 {
        for map in 1..=9 {
            let map_name = format!("E{}M{}", episode, map);
            if wad.lump_exists(&map_name) {
                maps.push(map_name);
            }
        }
    }

    // Look for Doom 2 maps (MAP01-MAP32)
    for map in 1..=99 {
        let map_name = format!("MAP{:02}", map);
        if wad.lump_exists(&map_name) {
            maps.push(map_name);
        }
    }

    maps
}

fn process_map_pvs(
    wad: &WadData,
    map_name: &str,
    pic_data: &PicData,
) -> Result<(), Box<dyn Error>> {
    let hash = wad.map_bsp_hash(map_name).unwrap_or_default();
    let cache_path = gameplay::PVS::get_pvs_cache_path(map_name, hash)?;

    if !cache_path.exists() {
        let mut map_data = gameplay::MapData::default();
        map_data.load(map_name, &pic_data, wad);

        let MapData {
            subsectors,
            segments,
            bsp_3d,
            pvs,
            ..
        } = &mut map_data;

        // Ensure length is correct
        *pvs = PVS::new(subsectors.len());
        pvs.build(subsectors, segments, bsp_3d);
        info!("Saving PVS data to {cache_path:?}");
        map_data.pvs().save_to_file(&cache_path)?;
    } else {
        warn!("{cache_path:?} exists, skipping");
    }

    Ok(())
}

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
