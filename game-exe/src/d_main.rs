//! `d_main()` is the main loop. The task it has is to do a number of things:
//! 1. Set up the `Game`
//! 2. Run the ticks for all subsystems (input, menus, hud etc)
//! 3. Display the results on screen
//!
//! Most things are separated out as much as possible. The `Game` is its
//! own world, and various subsystems rely on getting data from it. These
//! subsystems are:
//! - `Statusbar`
//! - `Intermission`
//! - `Messages`
//! - `Finale`
//! - others may be added such as automap
//!
//! Note that the sound system runs on its own thread.
//!
//! `Game` also contains a lot of state ranging from the world state, demo
//! playback state, options used to setup the world, players and their stats,
//! and the overall gamestate.

use std::error::Error;

use finale_doom::Finale;
use gameplay::MapObject;
use gameplay::log::{error, info};
use gameplay::tic_cmd::{BASELOOKDIRMAX, BASELOOKDIRMIN, LOOKDIRMAX, LOOKDIRMIN, LOOKDIRS};
use gamestate::Game;
use gamestate::subsystems::GameSubsystem;
use gamestate_traits::sdl2::event::{Event, WindowEvent};
use gamestate_traits::sdl2::keyboard::Scancode;
use gamestate_traits::sdl2::video::Window;
use gamestate_traits::{
    GameState, PixelBuffer, PlayViewRenderer, RenderTrait, SubsystemTrait, sdl2,
};
use hud_doom::Messages;
use input::Input;
use intermission_doom::Intermission;
use menu_doom::MenuDoom;
use render_target::RenderTarget;
use sound_traits::SoundAction;
use statusbar_doom::Statusbar;
use wad::types::WadPatch;

use crate::CLIOptions;
use crate::cheats::Cheats;
use crate::timestep::TimeStep;

const fn set_lookdirs(options: &CLIOptions) {
    unsafe {
        LOOKDIRMIN = BASELOOKDIRMIN;
        LOOKDIRMAX = BASELOOKDIRMAX;
        if options.hi_res {
            LOOKDIRMAX *= 2;
            LOOKDIRMIN *= 2;
        }
        LOOKDIRS = 1 + LOOKDIRMIN + LOOKDIRMAX;
    }
}

/// Never returns until `game.running` is set to false
pub fn d_doom_loop(
    mut game: Game,
    mut input: Input,
    window: Window,
    gl_ctx: golem::Context,
    options: CLIOptions,
) -> Result<(), Box<dyn Error>> {
    // TODO: implement an openGL or Vulkan renderer
    // TODO: check res aspect and set widescreen or no
    let mut timestep = TimeStep::new(true);
    let mut cheats = Cheats::new();
    let mut menu = MenuDoom::new(game.game_type.mode, &game.wad_data);
    menu.init(&game);

    let mut machines = GameSubsystem {
        statusbar: Statusbar::new(game.game_type.mode, &game.wad_data),
        intermission: Intermission::new(game.game_type.mode, &game.wad_data),
        hud_msgs: Messages::new(&game.wad_data),
        finale: Finale::new(&game.wad_data),
    };
    info!("Loaded subsystems");

    let mut canvas = window
        .into_canvas()
        .accelerated()
        .present_vsync()
        .target_texture()
        .build()?;
    info!("Built display window");
    canvas.window_mut().show();
    info!("Setup window canvas");

    // Start demo playback and titlescreens +
    if options.episode.is_none() && options.map.is_none() {
        game.start_title();
    }
    info!("Started title sequence");

    // BEGIN SETUP
    set_lookdirs(&options);
    let mut render_target = RenderTarget::new(
        options.hi_res,
        options.dev_parm,
        canvas,
        &gl_ctx,
        options.rendering.unwrap_or_default().into(),
        options.shader.unwrap_or_default(),
    );
    // END

    loop {
        if !game.running() {
            break;
        }
        // The game-exe is split in to two parts:
        // - tickers, these update all states (game-exe, menu, hud, automap etc)
        // - drawers, these take a state from above and display it to the user

        // Update the game-exe state
        if let Some(event) = try_run_tics(
            &mut game,
            &mut input,
            &mut menu,
            &mut machines,
            &mut cheats,
            &mut timestep,
        ) {
            match event {
                Event::Window {
                    timestamp: _,
                    window_id: _,
                    win_event,
                } => match win_event {
                    sdl2::event::WindowEvent::SizeChanged(..) => {
                        // BEGIN SETUP
                        set_lookdirs(&options);
                        let canvas = render_target.framebuffer.canvas;
                        render_target = RenderTarget::new(
                            options.hi_res,
                            options.dev_parm,
                            canvas,
                            &gl_ctx,
                            options.rendering.unwrap_or_default().into(),
                            options.shader.unwrap_or_default(),
                        );
                        // END
                        info!("Resized game window");
                    }
                    _ => {}
                },
                _ => {}
            }
        }

        // Update the positional sounds
        // Update the listener of the sound server. Will always be consoleplayer.
        if let Some(mobj) = game.players[game.consoleplayer].mobj() {
            let uid = mobj as *const MapObject as usize;
            game.sound_cmd
                .send(SoundAction::UpdateListener {
                    uid,
                    x: mobj.xy.x,
                    y: mobj.xy.y,
                    angle: mobj.angle.rad(),
                })
                .unwrap();
        }

        // Draw everything to the buffer
        d_display(&mut render_target, &mut menu, &mut machines, &mut game);

        // FPS rate updates every second
        if let Some(fps) = timestep.frame_rate() {
            info!("{:?}", fps);
            coarse_prof::write(&mut std::io::stdout()).unwrap();
        }
    }

    // Explicit drop to ensure shutdown happens
    drop(game);
    drop(gl_ctx);
    Ok(())
}

fn page_drawer(game: &mut Game, draw_buf: &mut impl PixelBuffer) {
    let f = draw_buf.size().height() / 200;
    let mut ytmp = 0;
    let mut xtmp = f - 1;
    for column in game.page.cache.columns.iter() {
        for n in 0..f {
            for p in column.pixels.iter() {
                let colour = game.pic_data.palette()[*p];
                for _ in 0..f {
                    let x = (xtmp - n) as usize;
                    let y = (ytmp + column.y_offset * f) as usize;
                    draw_buf.set_pixel(
                        x, // - (image.left_offset as i32),
                        y, /* - image.top_offset as i32 - 30, */
                        &colour,
                    );
                    ytmp += 1;
                }
            }
            ytmp = 0;

            if column.y_offset == 255 {
                xtmp += 1;
            }
        }
    }
}

/// Does a bunch of stuff in Doom...
/// `pixels` is the buffer that is always drawn, so drawing in to `pixels2` then
/// flipping ensures the buffer is drawn. But if we draw in to `pixels2` and
/// don't flip, we can do the screen-melt by progressively drawing from
/// `pixels2` to `pixels`.
///
/// D_Display
#[allow(clippy::too_many_arguments)]
fn d_display<R>(
    rend_target: &mut R,
    menu: &mut impl SubsystemTrait,
    machines: &mut GameSubsystem<
        impl SubsystemTrait,
        impl SubsystemTrait,
        impl SubsystemTrait,
        impl SubsystemTrait,
    >,
    game: &mut Game,
) where
    R: RenderTrait + PlayViewRenderer,
{
    let wipe = game.gamestate != game.wipe_game_state;
    let automap_active = false;
    //if (gamestate == GS_LEVEL && !automapactive && gametic)

    // Drawing order is different for RUST4DOOM as the screensize-statusbar is
    // never taken in to account. A full Doom-style statusbar will never be added
    // instead an "overlay" style bar will be done.
    if game.gamestate == GameState::Level && game.game_tic != 0 {
        if !automap_active {
            match game.level {
                Some(ref level) => {
                    if !game.players_in_game[game.consoleplayer] {
                        return;
                    }
                    if game.players[0].mobj().is_none() {
                        error!("Active console player has no MapObject, can't render player view");
                    } else {
                        let player = &game.players[game.consoleplayer];
                        if game.options.dev_parm {
                            rend_target.debug_clear();
                        }
                        rend_target.render_player_view(player, level, &mut game.pic_data);
                    }
                }
                _ => {}
            }
        }
    }

    match game.gamestate {
        GameState::Level => {
            // TODO: Automap draw
            machines.statusbar.draw(rend_target.draw_buffer());
            machines.hud_msgs.draw(rend_target.draw_buffer());
        }
        GameState::Intermission => machines.intermission.draw(rend_target.draw_buffer()),
        GameState::Finale => machines.finale.draw(rend_target.draw_buffer()),
        GameState::DemoScreen => {
            if game.page.cache.name != game.page.name {
                let lump = game
                    .wad_data
                    .get_lump(game.page.name)
                    .expect("TITLEPIC missing");
                game.page.cache = WadPatch::from_lump(lump);
            }
            page_drawer(game, rend_target.draw_buffer());
        }
        _ => {}
    }

    // draw_buf.clear();
    // net update does i/o and buildcmds...
    // TODO: NetUpdate(); // send out any new accumulation

    #[cfg(feature = "debug_draw")]
    {
        game.wipe_game_state = game.gamestate;
        rend_target.flip();
        rend_target.clear();
        return;
    }

    if wipe {
        if rend_target.do_wipe() {
            game.wipe_game_state = game.gamestate;
        }
        // menu is drawn on top of wipes
        menu.draw(rend_target.blit_buffer());
    } else {
        menu.draw(rend_target.draw_buffer());
        rend_target.flip();
    }
    rend_target.blit();
}

fn try_run_tics(
    game: &mut Game,
    input: &mut Input,
    menu: &mut impl SubsystemTrait,
    machinations: &mut GameSubsystem<
        impl SubsystemTrait,
        impl SubsystemTrait,
        impl SubsystemTrait,
        impl SubsystemTrait,
    >,
    cheats: &mut Cheats,
    timestep: &mut TimeStep,
) -> Option<Event> {
    // TODO: net.c starts here
    // Build tics here?
    let mut event_return = None;
    timestep.run_this(|tics| {
        // D_ProcessEvents
        if let Some(e) = process_events(game, input, menu, machinations, cheats) {
            event_return.replace(e);
        }

        if game.demo.advance {
            game.do_advance_demo();
        }
        // Did menu take control?
        if !menu.ticker(game) && game.wipe_game_state != GameState::ForceWipe {
            game.ticker(machinations); // G_Ticker
        }
        game.game_tic = tics as u32;
    });
    event_return
}

fn process_events(
    game: &mut Game,
    input: &mut Input,
    menu: &mut impl SubsystemTrait,
    machinations: &mut GameSubsystem<
        impl SubsystemTrait,
        impl SubsystemTrait,
        impl SubsystemTrait,
        impl SubsystemTrait,
    >,
    cheats: &mut Cheats,
) -> Option<Event> {
    // required for cheats and menu so they don't receive multiple key-press fo same
    // key
    let input_callback = |sc: Scancode| {
        if game.level.is_some() {
            cheats.check_input(sc, game);
        }

        // Menu also has hotkeys like F1, so check at all times
        if menu.responder(sc, game) {
            return true; // Menu took event
        }

        if machinations.hud_msgs.responder(sc, game) {
            return true; // Menu took event
        }

        // We want intermission to check checks only if the level isn't loaded
        if game.level.is_none() {
            match game.gamestate {
                GameState::Intermission => {
                    if machinations.intermission.responder(sc, game) {
                        return true; // Menu took event
                    }
                }
                GameState::Finale => {
                    if machinations.finale.responder(sc, game) {
                        return true; // Menu took event
                    }
                }
                _ => {}
            }
        }

        false
    };

    let mut event_return = None;
    let event_callback = |event: sdl2::event::Event| match event {
        sdl2::event::Event::Window {
            timestamp: _,
            window_id: _,
            win_event,
        } => {
            if matches!(win_event, WindowEvent::SizeChanged(_, _)) {
                event_return = Some(event);
            }
        }
        _ => {}
    };

    input.update(input_callback, event_callback);
    let console_player = game.consoleplayer;
    // net update does i/o and buildcmds...
    // TODO: NetUpdate(); // send out any new accumulation
    // TODO: Network code would update each player slot with incoming TicCmds...
    if game.gamestate == game.wipe_game_state {
        let cmd = input.events.build_tic_cmd(&input.config);
        game.netcmds[console_player][0] = cmd;
    }

    event_return
}
