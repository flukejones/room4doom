//! The main loop driver. The primary function is the main loop which attempts
//! to run all tics then display the result. Handling of actual game-exe state
//! is done withing the `Game` object.

use std::error::Error;
use std::mem;

use finale_doom::Finale;
use gameplay::log::{self, error, info};
use gameplay::MapObject;
use gamestate::machination::Machinations;
use gamestate::Game;
use gamestate_traits::sdl2::keyboard::Scancode;
use gamestate_traits::sdl2::render::Canvas;
use gamestate_traits::sdl2::video::Window;
use gamestate_traits::{GameState, MachinationTrait};
use hud_doom::Messages;
use input::Input;
use intermission_doom::Intermission;
use menu_doom::MenuDoom;
use render_soft::SoftwareRenderer;
use render_target::{PixelBuffer, PlayRenderer, RenderTarget, RenderType};
use sound_traits::SoundAction;
use statusbar_doom::Statusbar;
use wad::types::{WadFlat, WadPatch};

use crate::cheats::Cheats;
use crate::test_funcs::{flat_select_test, image_test, patch_select_test, texture_select_test};
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
    let mut render_buffer: RenderTarget;
    let mut render_buffer2: RenderTarget;
    let mut render_type = RenderType::Software;
    let double = options.double.is_some() && options.double.unwrap();
    let (buf_width, buf_height) =
        buffer_dimensions(window.size().0 as f32, window.size().1 as f32, double);
    let mut canvas = window
        .into_canvas()
        //        .present_vsync()
        .accelerated()
        .build()
        .unwrap();

    let fov = 90f32.to_radians();

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

    let verbose = options.verbose.unwrap_or(log::LevelFilter::Warn);
    let mut renderer = SoftwareRenderer::new(
        fov,
        buf_width,
        buf_height,
        matches!(verbose, log::LevelFilter::Debug),
    );

    info!("Using {render_type:?}");

    let mut timestep = TimeStep::new();

    if matches!(render_type, RenderType::SoftOpenGL) {
        let buf = unsafe { render_buffer.soft_opengl_unchecked() };
        buf.set_gl_filter().unwrap();
        let buf = unsafe { render_buffer.soft_opengl_unchecked() };
        buf.set_gl_filter().unwrap();
    }

    let mut pal_num = 0;
    let mut image_num = 0;
    let mut tex_num = 0;
    let mut flat_num = 0;
    let mut sprite_num = 119;
    let images: Option<Vec<WadPatch>> = if options.image_cycle_test || options.texture_test {
        Some(game.wad_data.patches_iter().collect())
    } else {
        None
    };
    let flats: Option<Vec<WadFlat>> = if options.flats_test {
        Some(game.wad_data.flats_iter().collect())
    } else {
        None
    };
    let sprites: Option<Vec<WadPatch>> = if options.sprites_test {
        let sprites: Vec<WadPatch> = game.wad_data.sprites_iter().collect();
        let image = &sprites[sprite_num];
        info!("{}", image.name);
        Some(sprites)
    } else {
        None
    };

    let mut cheats = Cheats::new();
    let mut menu = MenuDoom::new(game.game_mode, &game.wad_data);
    menu.init(&game);

    let mut machines = Machinations {
        statusbar: Statusbar::new(game.game_mode, &game.wad_data),
        intermission: Intermission::new(game.game_mode, &game.wad_data),
        hud_msgs: Messages::new(&game.wad_data),
        finale: Finale::new(&game.wad_data),
    };

    loop {
        if !game.running() {
            break;
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
            game.snd_command
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
            &mut renderer,
            &mut menu,
            &mut machines,
            &mut game,
            &mut render_buffer,
            &mut render_buffer2,
            &mut canvas,
            &mut timestep,
        );

        // if options.palette_test {
        //     palette_test(pal_num, &mut game, &mut render_buffer);
        // }

        if let Some(name) = options.image_test.clone() {
            image_test(
                &name.to_ascii_uppercase(),
                &game,
                render_buffer.pixel_buffer(),
            );
        }
        if options.image_cycle_test {
            if let Some(images) = &images {
                patch_select_test(&images[image_num], &game, render_buffer.pixel_buffer());
            }
        }
        if options.flats_test {
            if let Some(flats) = &flats {
                flat_select_test(&flats[flat_num], &game, render_buffer.pixel_buffer());
            }
        }
        if options.sprites_test {
            if let Some(sprites) = &sprites {
                patch_select_test(&sprites[sprite_num], &game, render_buffer.pixel_buffer());
            }
        }
        if options.texture_test {
            texture_select_test(
                game.pic_data.borrow_mut().get_texture(tex_num),
                &game,
                render_buffer.pixel_buffer(),
            );
        }

        render_buffer.blit(&mut canvas);

        // FPS rate updates every second
        if let Some(_fps) = timestep.frame_rate() {
            println!("{:?}", _fps);

            if options.palette_test {
                if pal_num == 13 {
                    pal_num = 0
                } else {
                    pal_num += 1;
                }
            }

            if let Some(images) = &images {
                image_num += 1;
                if image_num == images.len() {
                    image_num = 0;
                }
            }

            if options.texture_test {
                if tex_num < game.pic_data.borrow_mut().num_textures() - 1 {
                    tex_num += 1;
                } else {
                    tex_num = 0;
                }
            }

            if let Some(flats) = &flats {
                flat_num += 1;
                if flat_num == flats.len() {
                    flat_num = 0;
                }
            }

            if let Some(sprites) = &sprites {
                sprite_num += 1;
                if sprite_num == sprites.len() {
                    sprite_num = 0;
                }
                let image = &sprites[sprite_num];
                info!("{}", image.name);
            }
        }
    }
    game.snd_command.send(SoundAction::Shutdown).unwrap();
    // game.snd_thread.join().unwrap();
    Ok(())
}

fn page_drawer(game: &mut Game, draw_buf: &mut dyn PixelBuffer) {
    let f = draw_buf.size().height() / 200;
    let mut ytmp = 0;
    let mut xtmp = f - 1;
    for column in game.page_cache.columns.iter() {
        for n in 0..f {
            for p in column.pixels.iter() {
                let colour = game.pic_data.borrow().palette()[*p];
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
    menu: &mut impl MachinationTrait,
    machines: &mut Machinations<
        impl MachinationTrait,
        impl MachinationTrait,
        impl MachinationTrait,
        impl MachinationTrait,
    >,
    game: &mut Game,
    disp_buf: &mut RenderTarget, // Display from this buffer
    draw_buf: &mut RenderTarget, // Draw to this buffer
    canvas: &mut Canvas<Window>,
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
                if !game.player_in_game[0] {
                    return;
                }
                if game.players[0].mobj().is_none() {
                    error!("Active console player has no MapObject, can't render player view");
                } else {
                    let player = &game.players[game.consoleplayer];
                    let mut pic_data = level.pic_data.borrow_mut();
                    if game.options.dev_parm {
                        draw_buf.pixel_buffer().clear_with_colour(&[0, 164, 0, 255]);
                    }
                    rend.render_player_view(player, level, &mut pic_data, draw_buf);
                }
            }
        }
        // Fake crosshair
        draw_buf.pixel_buffer().set_pixel(
            disp_buf.width() / 2,
            disp_buf.height() / 2,
            &[200, 14, 14, 255],
        );
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
            // TODO: we're clearing here to make the menu visible (for now)
            if game.page_cache.name != game.page_name {
                let lump = game
                    .wad_data
                    .get_lump("TITLEPIC")
                    .expect("TITLEPIC missing");
                game.page_cache = WadPatch::from_lump(lump);
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
            disp_buf.blit(canvas);
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
    menu: &mut impl MachinationTrait,
    machinations: &mut Machinations<
        impl MachinationTrait,
        impl MachinationTrait,
        impl MachinationTrait,
        impl MachinationTrait,
    >,
    cheats: &mut Cheats,
    timestep: &mut TimeStep,
) {
    // TODO: net.c starts here
    process_events(game, input, menu, machinations, cheats); // D_ProcessEvents

    // Build tics here?
    timestep.run_this(|_| {
        if game.demo_advance {
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
    menu: &mut impl MachinationTrait,
    machinations: &mut Machinations<
        impl MachinationTrait,
        impl MachinationTrait,
        impl MachinationTrait,
        impl MachinationTrait,
    >,
    cheats: &mut Cheats,
) {
    // required for cheats and menu so they don't receive multiple key-press fo same
    // key
    let callback = |sc: Scancode| {
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

    if !input.update(callback) {
        let console_player = game.consoleplayer;
        // net update does i/o and buildcmds...
        // TODO: NetUpdate(); // send out any new accumulation

        // TODO: Network code would update each player slot with incoming TicCmds...
        let cmd = input.events.build_tic_cmd(&input.config);
        game.netcmds[console_player][0] = cmd;
    }
}
