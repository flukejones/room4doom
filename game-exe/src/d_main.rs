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
use std::mem;

use finale_doom::Finale;
use gameplay::log::{self, error, info};
use gameplay::tic_cmd::{BASELOOKDIRMAX, BASELOOKDIRMIN, LOOKDIRMAX, LOOKDIRMIN, LOOKDIRS};
use gameplay::MapObject;
use gamestate::subsystems::GameSubsystem;
use gamestate::Game;
use gamestate_traits::sdl2::keyboard::Scancode;
use gamestate_traits::sdl2::pixels::PixelFormatEnum;
use gamestate_traits::sdl2::render::Canvas;
use gamestate_traits::sdl2::video::Window;
use gamestate_traits::{sdl2, GameState, SubsystemTrait};
use hud_doom::Messages;
use input::Input;
use intermission_doom::Intermission;
use menu_doom::MenuDoom;
use render_soft::SoftwareRenderer;
use render_target::{PixelBuffer, PlayRenderer, RenderTarget, RenderType};
use sound_traits::SoundAction;
use statusbar_doom::Statusbar;
use wad::types::WadPatch;

use crate::cheats::Cheats;
use crate::timestep::TimeStep;
use crate::wipe::Wipe;
use crate::CLIOptions;

/// Used to set correct buffer width for screen dimensions matching the OD Doom
/// height
fn buffer_dimensions(width: f32, height: f32, double: bool) -> (usize, usize) {
    let screen_ratio = width / height;
    let mut buf_height = 200;

    let mut buf_width = (buf_height as f32 * screen_ratio) as i32;
    if double {
        buf_width *= 2;
        buf_height *= 2;
    }
    (buf_width as usize, buf_height)
}

struct RenderGroup {
    buf_size: (usize, usize),
    render_buffer: RenderTarget,
    render_buffer2: RenderTarget,
    renderer: SoftwareRenderer,
}

fn create_renderer(
    canvas: &Canvas<Window>,
    gl_ctx: &golem::Context,
    options: &CLIOptions,
) -> RenderGroup {
    unsafe {
        LOOKDIRMIN = BASELOOKDIRMIN;
        if options.hi_res {
            LOOKDIRMIN *= 2;
        }
        LOOKDIRMAX = BASELOOKDIRMAX;
        if options.hi_res {
            LOOKDIRMAX *= 2;
        }
        LOOKDIRS = 1 + LOOKDIRMIN + LOOKDIRMAX;
    }

    let verbose = options.verbose.unwrap_or(log::LevelFilter::Warn);
    let fov = 90f32.to_radians();
    let double = options.hi_res;

    let mut render_buffer: RenderTarget;
    let mut render_buffer2: RenderTarget;
    let mut render_type = RenderType::Software;

    let size = canvas.window().size();
    let (buf_width, buf_height) = buffer_dimensions(size.0 as f32, size.1 as f32, double);

    let renderer = SoftwareRenderer::new(
        fov,
        buf_width,
        buf_height,
        matches!(verbose, log::LevelFilter::Debug),
    );

    match options.rendering.unwrap() {
        crate::config::RenderType::Software => {
            render_buffer = RenderTarget::new(buf_width, buf_height).with_software(&canvas);
            render_buffer2 = RenderTarget::new(buf_width, buf_height).with_software(&canvas);
        }
        crate::config::RenderType::SoftOpenGL => {
            let shader = options.shader.unwrap_or_default();
            render_type = RenderType::SoftOpenGL;
            render_buffer =
                RenderTarget::new(buf_width, buf_height).with_gl(&canvas, &gl_ctx, shader);
            render_buffer2 =
                RenderTarget::new(buf_width, buf_height).with_gl(&canvas, &gl_ctx, shader);
        }
        crate::config::RenderType::OpenGL => todo!(),
        crate::config::RenderType::Vulkan => todo!(),
    }

    if matches!(render_type, RenderType::SoftOpenGL) {
        let buf = unsafe { render_buffer.soft_opengl_unchecked() };
        buf.set_gl_filter().unwrap();
        let buf = unsafe { render_buffer2.soft_opengl_unchecked() };
        buf.set_gl_filter().unwrap();
    }

    RenderGroup {
        buf_size: (buf_width, buf_height),
        render_buffer,
        render_buffer2,
        renderer,
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
    let mut timestep = TimeStep::new();
    let mut cheats = Cheats::new();
    let mut menu = MenuDoom::new(game.game_type.mode, &game.wad_data);
    menu.init(&game);

    let mut machines = GameSubsystem {
        statusbar: Statusbar::new(game.game_type.mode, &game.wad_data),
        intermission: Intermission::new(game.game_type.mode, &game.wad_data),
        hud_msgs: Messages::new(&game.wad_data),
        finale: Finale::new(&game.wad_data),
    };

    // Start demo playback and titlescreens +
    if options.episode.is_none() && options.map.is_none() {
        game.start_title();
    }

    let mut canvas = window.into_canvas().accelerated().build()?;
    canvas.window_mut().show();
    let mut rend_group = create_renderer(&canvas, &gl_ctx, &options);
    let texture_creator = canvas.texture_creator();
    let mut texture = texture_creator.create_texture_target(
        Some(PixelFormatEnum::RGBA32),
        rend_group.buf_size.0 as u32,
        rend_group.buf_size.1 as u32,
    )?;

    let mut old_size = canvas.window().size();
    loop {
        if !game.running() {
            break;
        }
        let new_size = canvas.window().size();
        if old_size != new_size {
            drop(rend_group);
            rend_group = create_renderer(&canvas, &gl_ctx, &options);
            texture = texture_creator.create_texture_target(
                Some(PixelFormatEnum::RGBA32),
                rend_group.buf_size.0 as u32,
                rend_group.buf_size.1 as u32,
            )?;
            old_size = new_size;
            info!("Resized game window");
        }
        // The game-exe is split in to two parts:
        // - tickers, these update all states (game-exe, menu, hud, automap etc)
        // - drawers, these take a state from above and display it to the user

        // Update the game-exe state
        try_run_tics(
            &mut game,
            &mut input,
            &mut menu,
            &mut machines,
            &mut cheats,
            &mut timestep,
        );

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
        d_display(
            &mut rend_group.renderer,
            &mut menu,
            &mut machines,
            &mut game,
            &mut rend_group.render_buffer,
            &mut rend_group.render_buffer2,
            &mut canvas,
            &mut texture,
            &mut timestep,
        );

        // FPS rate updates every second
        if let Some(fps) = timestep.frame_rate() {
            info!("{:?}", fps);
        }

        rend_group.render_buffer.blit(&mut canvas, &mut texture);
    }

    // Explicit drop to ensure shutdown happens
    drop(game);
    drop(rend_group);
    // drop(window);
    drop(gl_ctx);
    Ok(())
}

fn page_drawer(game: &mut Game, draw_buf: &mut dyn PixelBuffer) {
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
                        &colour.0,
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
fn d_display(
    rend: &mut impl PlayRenderer,
    menu: &mut impl SubsystemTrait,
    machines: &mut GameSubsystem<
        impl SubsystemTrait,
        impl SubsystemTrait,
        impl SubsystemTrait,
        impl SubsystemTrait,
    >,
    game: &mut Game,
    disp_buf: &mut RenderTarget, // Display from this buffer
    draw_buf: &mut RenderTarget, // Draw to this buffer
    canvas: &mut Canvas<Window>,
    texture: &mut sdl2::render::Texture,
    timestep: &mut TimeStep,
) {
    let automap_active = false;
    //if (gamestate == GS_LEVEL && !automapactive && gametic)

    let wipe = game.gamestate != game.wipe_game_state;

    // Drawing order is different for RUST4DOOM as the screensize-statusbar is
    // never taken in to account. A full Doom-style statusbar will never be added
    // instead an "overlay" style bar will be done.
    if game.gamestate == GameState::Level && game.game_tic != 0 {
        if !automap_active {
            if let Some(ref level) = game.level {
                if !game.players_in_game[game.consoleplayer] {
                    return;
                }
                if game.players[0].mobj().is_none() {
                    error!("Active console player has no MapObject, can't render player view");
                } else {
                    let player = &game.players[game.consoleplayer];
                    if game.options.dev_parm {
                        draw_buf.pixel_buffer().clear_with_colour(&[0, 164, 0, 255]);
                    }
                    rend.render_player_view(player, level, &mut game.pic_data, draw_buf);
                }
            }
        }
        // TODO: option for various crosshair
        let crosshair = false;
        if crosshair {
            draw_buf.pixel_buffer().set_pixel(
                disp_buf.width() / 2,
                disp_buf.height() / 2,
                &[200, 14, 14, 255],
            );
        }
    }

    match game.gamestate {
        GameState::Level => {
            // TODO: Automap draw
            machines.statusbar.draw(draw_buf.pixel_buffer());
            machines.hud_msgs.draw(draw_buf.pixel_buffer());
        }
        GameState::Intermission => machines.intermission.draw(draw_buf.pixel_buffer()),
        GameState::Finale => machines.finale.draw(draw_buf.pixel_buffer()),
        GameState::DemoScreen => {
            if game.page.cache.name != game.page.name {
                let lump = game
                    .wad_data
                    .get_lump(game.page.name)
                    .expect("TITLEPIC missing");
                game.page.cache = WadPatch::from_lump(lump);
            }
            page_drawer(game, draw_buf.pixel_buffer());
        }
        _ => {}
    }

    // menus go directly to the screen
    // draw_buf.clear();
    menu.draw(draw_buf.pixel_buffer()); // menu is drawn even on top of everything
                                        // net update does i/o and buildcmds...
                                        // TODO: NetUpdate(); // send out any new accumulation

    if !wipe {
        mem::swap(disp_buf, draw_buf);
        return;
    }

    // Doom uses a loop here. The thing about it is that while the loop is running
    // there can be no input, so the menu can't be activated. I think with Doom the
    // input event queue was still filled via interrupt.
    let mut wipe = Wipe::new(disp_buf.width() as i32, disp_buf.height() as i32);
    loop {
        let mut done = false;
        timestep.run_this(|_| {
            done = wipe.do_melt(disp_buf.pixel_buffer(), draw_buf.pixel_buffer());
            disp_buf.blit(canvas, texture);
        });

        if done {
            break;
        }
        std::thread::sleep(std::time::Duration::from_micros(1));
    }
    game.wipe_game_state = game.gamestate;
    //menu.draw(disp_buf); // menu is drawn on top of wipes too
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
) {
    // TODO: net.c starts here
    // Build tics here?
    timestep.run_this(|_| {
        process_events(game, input, menu, machinations, cheats); // D_ProcessEvents

        if game.demo.advance {
            game.do_advance_demo();
        }
        // Did menu take control?
        if !menu.ticker(game) {
            game.ticker(machinations); // G_Ticker
        }
        game.game_tic += 1;
    });
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
) {
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

    let event_callback = |event: sdl2::event::Event| match event {
        sdl2::event::Event::Window {
            timestamp: _,
            window_id: _,
            win_event,
        } => {
            match win_event {
                // sdl2::event::WindowEvent::Moved(..) => todo!("Moved"),
                // sdl2::event::WindowEvent::Resized(..) => todo!("Resized"),
                // sdl2::event::WindowEvent::SizeChanged(..) => todo!("SizeChanged"),
                // sdl2::event::WindowEvent::DisplayChanged(_) => todo!("DisplayChanged"),
                _ => {}
            }
            // println!(
            //     "sdl2::event::Event::Window: {:?} {:?}",
            //     window_id, win_event
            // );
        }
        _ => {}
    };

    input.update(input_callback, event_callback);
    let console_player = game.consoleplayer;
    // net update does i/o and buildcmds...
    // TODO: NetUpdate(); // send out any new accumulation
    // TODO: Network code would update each player slot with incoming TicCmds...
    let cmd = input.events.build_tic_cmd(&input.config);
    game.netcmds[console_player][0] = cmd;
}
