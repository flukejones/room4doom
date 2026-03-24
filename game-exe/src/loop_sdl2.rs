//! SDL2 poll-based game loop. Never returns until `game.running` is false.

use doom_ui::{Finale, GameMenu, Intermission, Messages, Statusbar};
use gamestate::Game;
use gamestate::subsystems::GameSubsystem;
use gamestate_traits::{ConfigTraits, GameTraits, KeyCode, SubsystemTrait};
use log::info;
use render_backend::{DisplayBackend, RenderTarget};
use render_common::{GameRenderer, STBAR_HEIGHT};

use crate::CLIOptions;
use crate::cheats::Cheats;
use crate::d_main::{
    d_display, input_responder, load_voxels, run_game_tic, set_lookdirs, set_lookdirs_hires, update_sound
};
use crate::timestep::TimeStep;

/// Backend-agnostic window events returned from input processing.
enum WindowAction {
    Resized,
}

/// SDL2 poll-based game loop. Never returns until `game.running` is false.
pub fn d_doom_loop_sdl2(
    mut game: Game,
    mut input: input::InputSdl2,
    display: DisplayBackend,
    options: CLIOptions,
    mut user_config: crate::config::UserConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut timestep = TimeStep::new();
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

    set_lookdirs(&options);
    let debug_draw = options.debug_draw();
    let mut render_backend = RenderTarget::new(
        options.hi_res.unwrap_or(true),
        options.dev_parm,
        &debug_draw,
        display,
        options.rendering.unwrap_or_default().into(),
    );
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
    if let Some(ref vm) = voxel_manager {
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
            set_lookdirs_hires(user_config.hi_res);
            let prev_menu_state = menu.save_state();
            render_backend = render_backend.resize(
                user_config.hi_res,
                options.dev_parm,
                &debug_draw,
                user_config.renderer.into(),
            );
            if user_config.hud_size == 1 {
                render_backend.set_statusbar_height(STBAR_HEIGHT);
            }
            if user_config.voxels {
                if let Some(ref vm) = voxel_manager {
                    render_backend.set_voxel_manager(vm.clone());
                }
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
        d_display(
            &mut render_backend,
            &mut menu,
            &mut machines,
            &mut game,
            frac,
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
                    if let Some(ref vm) = voxel_manager {
                        render_backend.set_voxel_manager(vm.clone());
                    }
                } else {
                    render_backend.clear_voxel_manager();
                }
            }

            if old.music_type != user_config.music_type {
                let type_val = game.config_values[gamestate_traits::ConfigKey::MusicType as usize];
                let _ = game
                    .sound_cmd
                    .send(sound_common::SoundAction::SetMusicType(type_val));
                game.replay_current_music();
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
                set_lookdirs_hires(user_config.hi_res);
                let prev_menu_state = menu.save_state();
                render_backend = render_backend.resize(
                    user_config.hi_res,
                    options.dev_parm,
                    &debug_draw,
                    user_config.renderer.into(),
                );
                if user_config.hud_size == 1 {
                    render_backend.set_statusbar_height(STBAR_HEIGHT);
                }
                if user_config.voxels {
                    if let Some(ref vm) = voxel_manager {
                        render_backend.set_voxel_manager(vm.clone());
                    }
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

        if let Some(fps) = timestep.frame_rate() {
            if user_config.show_fps {
                render_backend.set_debug_line(format!("FPS {}", fps.frames));
            } else {
                render_backend.set_debug_line(String::new());
            }
            coarse_prof::write(&mut std::io::stdout()).unwrap();
        }
    }

    drop(game);
    Ok(())
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
