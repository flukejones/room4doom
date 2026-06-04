//! SDL2 poll-based game loop. Never returns until `game.running` is false.

use doom_ui::{Finale, GameMenu, Intermission, Messages, Statusbar};
use gamestate::Game;
use gamestate::subsystems::GameSubsystem;
use gamestate_traits::{ConfigTraits as _, KeyCode, SubsystemTrait};
use log::info;
use pic_data::PixelFmt;
use render_backend::{ActiveBackend, RenderKind, RenderStack, RenderType};
use render_common::STBAR_HEIGHT;
use sdl2::VideoSubsystem;
use sdl2::video::FullscreenType;

use crate::CLIOptions;
use crate::cheats::Cheats;
#[cfg(feature = "wgpu3d")]
use crate::config::PostEffect as CfgPostEffect;
use crate::config::{UserConfig, WindowMode};
use crate::d_main::{d_display, input_responder, load_voxels, run_game_tic, update_sound};
use crate::timestep::TimeStep;
#[cfg(feature = "wgpu3d")]
use render_backend::PostEffect;

/// Backend-agnostic window events returned from input processing.
enum WindowAction {
    Resized,
}

/// Build the SDL2 window for `render_type` and wrap it in the matching backend
/// (software canvas, or wgpu on the bare window). The window is created hidden,
/// then shown once the canvas/surface exists.
fn build_sdl2_backend<P: PixelFmt>(
    video: &VideoSubsystem,
    options: &CLIOptions,
    render_type: RenderType,
) -> ActiveBackend<P> {
    let mut window = video
        .window("ROOM4DOOM", options.width, options.height)
        .hidden()
        .position_centered()
        .build()
        .expect("failed to build SDL2 window");

    let fs = match options.window_mode.unwrap_or(WindowMode::Windowed) {
        WindowMode::Windowed => FullscreenType::Off,
        WindowMode::Borderless => FullscreenType::Desktop,
        WindowMode::Exclusive => FullscreenType::True,
    };
    window
        .set_fullscreen(fs)
        .expect("failed to set SDL2 fullscreen mode");

    match render_type.kind() {
        RenderKind::Software => {
            let mut cb = window.into_canvas().target_texture();
            if matches!(options.vsync, Some(true)) {
                cb = cb.present_vsync();
            }
            let mut canvas = cb.build().expect("failed to build SDL2 canvas");
            canvas.window_mut().show();
            render_backend::new_sdl2_software::<P>(canvas)
        }
        #[cfg(feature = "wgpu3d")]
        RenderKind::Hardware => {
            window.show();
            let post = sdl2_post_chain(options);
            render_backend::new_sdl2_hardware::<P>(window, options.vsync.unwrap_or(true), post)
        }
        #[cfg(not(feature = "wgpu3d"))]
        RenderKind::Hardware => {
            unreachable!("hardware render kind without wgpu3d")
        }
    }
}

/// Parse the post-process chain from CLI options into backend effects.
#[cfg(feature = "wgpu3d")]
fn sdl2_post_chain(options: &CLIOptions) -> Vec<PostEffect> {
    let chain = match options.post.as_deref() {
        Some(s) => crate::config::parse_post_chain(s).unwrap_or_else(|e| {
            log::warn!("ignoring --post: {e}");
            Vec::new()
        }),
        None => Vec::new(),
    };
    chain
        .iter()
        .map(|p| match p {
            CfgPostEffect::Stretch => PostEffect::Stretch,
            CfgPostEffect::Crt => PostEffect::Crt,
        })
        .collect()
}

/// SDL2 poll-based game loop. Never returns until `game.running` is false.
pub fn d_doom_loop_sdl2<P>(
    mut game: Game,
    mut input: input::InputSdl2,
    video: VideoSubsystem,
    options: CLIOptions,
    mut user_config: UserConfig,
) where
    P: PixelFmt,
{
    let mut timestep = TimeStep::new();
    let mut fps_text = String::new();
    let mut cheats = Cheats::new();

    let mut machines = GameSubsystem {
        statusbar: Statusbar::new(game.game_type.mode, &game.wad_data),
        intermission: Intermission::new(game.game_type.mode, &game.wad_data, &game.umapinfo),
        hud_msgs: Messages::new(&game.wad_data),
        finale: Finale::new(&game.wad_data),
    };
    info!("Loaded subsystems");

    if let Some(name) = options.demo.clone() {
        game.start_demo(name);
    } else if options.episode.is_none() && options.map.is_none() {
        game.start_title();
    }
    info!("Started title sequence");

    let mut render_type: RenderType = options.rendering.unwrap_or_default().into();
    let backend = build_sdl2_backend::<P>(&video, &options, render_type);
    let mut render_backend =
        RenderStack::<P>::new(options.hi_res.unwrap_or(true), backend, render_type);
    // softbuffer can't host hardware; sdl2 can when wgpu3d is compiled. If the
    // configured kind isn't supported, fall back to the software default.
    if !render_backend.supports(render_type.kind()) {
        log::warn!(
            "renderer {render_type:?} unsupported by this backend; falling back to software"
        );
        render_type = RenderType::default();
        let backend = build_sdl2_backend::<P>(&video, &options, render_type);
        render_backend =
            RenderStack::<P>::new(options.hi_res.unwrap_or(true), backend, render_type);
    }
    if user_config.hud_size == 1 {
        render_backend.set_statusbar_height(STBAR_HEIGHT);
    }
    input
        .state
        .events
        .set_mouse_scale((user_config.mouse_sensitivity, 1));
    input.state.events.set_invert_y(user_config.invert_y);
    let mut voxel_manager = load_voxels(
        &options,
        &game.wad_data,
        game.game_type.mode,
        game.pic_data.pwad_sprite_overrides(),
    );
    if let Some(vm) = &voxel_manager {
        render_backend.set_voxel_manager(vm.clone());
    }
    let mut menu = GameMenu::new(
        game.game_type.mode,
        &game.wad_data,
        render_backend.buffer_size().width(),
    );
    menu.init(&game);

    loop {
        if !game.running() {
            break;
        }

        // Poll SDL2 events
        if let Some(WindowAction::Resized) = try_run_tics_sdl2(
            &mut game,
            &mut input,
            &mut menu,
            &mut machines,
            &mut cheats,
            &mut timestep,
        ) {
            let prev_menu_state = menu.save_state();
            render_backend = render_backend.resize(user_config.hi_res, user_config.renderer.into());
            if user_config.hud_size == 1 {
                render_backend.set_statusbar_height(STBAR_HEIGHT);
            }
            if user_config.voxels
                && let Some(vm) = &voxel_manager
            {
                render_backend.set_voxel_manager(vm.clone());
            }
            menu = GameMenu::new(
                game.game_type.mode,
                &game.wad_data,
                render_backend.buffer_size().width(),
            );
            menu.init(&game);
            menu.restore_state(prev_menu_state);
            info!("Resized game window");
        }

        let frac = if user_config.frame_interpolation {
            timestep.frac()
        } else {
            1.0
        };
        update_sound(&game);
        #[cfg(feature = "wgpu3d")]
        {
            render_backend.set_light_gamma(user_config.light_gamma as f32 / 100.0);
            render_backend.set_dynamic_sky(user_config.dynamic_sky);
        }
        let fps = if user_config.show_fps {
            fps_text.as_str()
        } else {
            ""
        };
        d_display(
            &mut render_backend,
            &mut menu,
            &mut machines,
            &mut game,
            frac,
            fps,
        );

        if game.is_config_dirty() {
            let old = user_config.clone();
            user_config.apply_config_array(&game.config_snapshot());
            let (w, h) = render_backend.window_size();
            user_config.width = w;
            user_config.height = h;
            user_config.write();
            game.clear_config_dirty();

            if old.crt_gamma != user_config.crt_gamma {
                game.pic_data.set_crt_gamma(user_config.crt_gamma);
            }

            if old.window_mode != user_config.window_mode {
                let mode = game.config_snapshot()[gamestate_traits::ConfigKey::WindowMode as usize];
                render_backend.set_fullscreen(mode as u8);
            }

            if old.voxels != user_config.voxels {
                if user_config.voxels {
                    if voxel_manager.is_none() {
                        voxel_manager = load_voxels(
                            &options,
                            &game.wad_data,
                            game.game_type.mode,
                            game.pic_data.pwad_sprite_overrides(),
                        );
                    }
                    if let Some(vm) = &voxel_manager {
                        render_backend.set_voxel_manager(vm.clone());
                    }
                } else {
                    render_backend.clear_voxel_manager();
                }
            }

            if old.music_type != user_config.music_type {
                let type_val = game.config_values[gamestate_traits::ConfigKey::MusicType as usize];
                if let Ok(music_type) = sound_common::MusicType::try_from(type_val) {
                    let _ = game
                        .sound_cmd
                        .send(sound_common::SoundAction::SetMusicType(music_type));
                    game.replay_current_music();
                } else {
                    log::warn!("Invalid MusicType config value: {type_val}");
                }
            }

            if old.mouse_sensitivity != user_config.mouse_sensitivity {
                input
                    .state
                    .events
                    .set_mouse_scale((user_config.mouse_sensitivity, 1));
            }
            if old.invert_y != user_config.invert_y {
                input.state.events.set_invert_y(user_config.invert_y);
            }

            if old.hud_size != user_config.hud_size {
                let bar_h = if user_config.hud_size == 1 {
                    STBAR_HEIGHT
                } else {
                    0
                };
                render_backend.set_statusbar_height(bar_h);
            }

            if old.renderer != user_config.renderer || old.hi_res != user_config.hi_res {
                let prev_menu_state = menu.save_state();
                let new_type: RenderType = user_config.renderer.into();
                if new_type.kind() != render_backend.render_type().kind() {
                    // Render kind changed (software <-> hardware): the sdl2 backend
                    // itself differs (canvas vs bare-window+wgpu), so rebuild it
                    // with a fresh window rather than reusing the old backend.
                    let backend = build_sdl2_backend::<P>(&video, &options, new_type);
                    render_backend = RenderStack::<P>::new(user_config.hi_res, backend, new_type);
                } else {
                    render_backend = render_backend.resize(user_config.hi_res, new_type);
                }
                if user_config.hud_size == 1 {
                    render_backend.set_statusbar_height(STBAR_HEIGHT);
                }
                if user_config.voxels
                    && let Some(vm) = &voxel_manager
                {
                    render_backend.set_voxel_manager(vm.clone());
                }
                menu = GameMenu::new(
                    game.game_type.mode,
                    &game.wad_data,
                    render_backend.buffer_size().width(),
                );
                menu.init(&game);
                menu.restore_state(prev_menu_state);
            }
        }

        if let Some(fd) = timestep.frame_rate() {
            fps_text = format!("FPS {}", fd.frames);
            coarse_prof::write(&mut std::io::stdout()).unwrap();
        }
    }

    drop(game);
}

/// Run tics using SDL2 poll-based input.
fn try_run_tics_sdl2(
    game: &mut Game,
    input: &mut input::InputSdl2,
    menu: &mut impl SubsystemTrait,
    machinations: &mut GameSubsystem<
        impl SubsystemTrait,
        impl SubsystemTrait,
        impl SubsystemTrait,
        impl SubsystemTrait,
    >,
    cheats: &mut Cheats,
    timestep: &mut TimeStep,
) -> Option<WindowAction> {
    let mut action_return = None;
    let mut resized = false;
    {
        let input_callback = |sc: KeyCode| input_responder(sc, game, menu, machinations, cheats);
        let event_callback = |_: input::RawEvent| {
            resized = true;
        };
        input.update(input_callback, event_callback);
    }
    if resized {
        action_return = Some(WindowAction::Resized);
    }

    timestep.run_this(|| {
        run_game_tic(game, &mut input.state, menu, machinations);
    });
    action_return
}
