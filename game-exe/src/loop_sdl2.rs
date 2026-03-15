//! SDL2 poll-based game loop. Never returns until `game.running` is false.

use gameplay::log::info;
use gamestate::Game;
use gamestate::subsystems::GameSubsystem;
use gamestate_traits::{GameRenderer, SubsystemTrait};
use hud_doom::Messages;
use input::InputState;
use intermission_doom::Intermission;
use menu_doom::MenuDoom;
use render_target::{DisplayBackend, RenderTarget};
use statusbar_doom::Statusbar;

use crate::CLIOptions;
use crate::cheats::Cheats;
use crate::d_main::{d_display, input_responder, run_game_tic, set_lookdirs, update_sound};
use crate::timestep::TimeStep;

use finale_doom::Finale;
use gamestate_traits::KeyCode;

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
) -> Result<(), Box<dyn std::error::Error>> {
    let mut timestep = TimeStep::new(true);
    let mut cheats = Cheats::new();

    let mut machines = GameSubsystem {
        statusbar: Statusbar::new(game.game_type.mode, &game.wad_data),
        intermission: Intermission::new(game.game_type.mode, &game.wad_data),
        hud_msgs: Messages::new(&game.wad_data),
        finale: Finale::new(&game.wad_data),
    };
    info!("Loaded subsystems");

    if options.episode.is_none() && options.map.is_none() {
        game.start_title();
    }
    info!("Started title sequence");

    set_lookdirs(&options);
    let debug_draw = options.debug_draw();
    let mut render_target = RenderTarget::new(
        options.hi_res.unwrap_or(true),
        options.dev_parm,
        &debug_draw,
        display,
        options.rendering.unwrap_or_default().into(),
    );
    let mut menu = MenuDoom::new(
        game.game_type.mode,
        &game.wad_data,
        render_target.buffer_size().width(),
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
            set_lookdirs(&options);
            render_target = render_target.resize(
                options.hi_res.unwrap_or(true),
                options.dev_parm,
                &debug_draw,
                options.rendering.unwrap_or_default().into(),
            );
            menu = MenuDoom::new(
                game.game_type.mode,
                &game.wad_data,
                render_target.buffer_size().width(),
            );
            menu.init(&game);
            info!("Resized game window");
        }

        update_sound(&game);
        d_display(&mut render_target, &mut menu, &mut machines, &mut game);

        if let Some(fps) = timestep.frame_rate() {
            render_target.set_debug_line(format!("FPS {}", fps.frames));
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
    timestep.run_this(|tics| {
        let mut resized = false;
        {
            let input_callback =
                |sc: KeyCode| input_responder(sc, game, menu, machinations, cheats);
            let event_callback = |event: input::RawEvent| {
                if let input::RawEvent::Resized = event {
                    resized = true;
                }
            };
            input.update(input_callback, event_callback);
        }
        if resized {
            action_return = Some(WindowAction::Resized);
        }

        run_game_tic(game, &mut input.state, menu, machinations, tics);
    });
    action_return
}
