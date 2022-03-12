use std::error::Error;

use doom_lib::{log::info, Game, Texture};
use golem::Context;
use sdl2::{
    keyboard::Scancode,
    pixels::{Color, PixelFormatEnum},
    rect::Rect,
    render::Canvas,
    surface::Surface,
    video::Window,
};
use wad::lumps::{WadFlat, WadPalette, WadPatch};

use crate::{
    input::Input,
    renderer::{software::bsp::SoftwareRenderer, Renderer},
    shaders::{basic::Basic, cgwg_crt::Cgwgcrt, lottes_crt::LottesCRT, Drawer, Shaders},
    timestep::TimeStep,
    GameOptions,
};

/// Never returns
pub fn d_doom_loop(
    mut game: Game,
    mut input: Input,
    gl: Window,
    ctx: Context,
    options: GameOptions,
) -> Result<(), Box<dyn Error>> {
    // TODO: implement an openGL or Vulkan renderer
    let mut renderer = SoftwareRenderer::new(game.textures.clone());

    let mut timestep = TimeStep::new();
    let mut render_buffer = Surface::new(320, 200, PixelFormatEnum::RGBA32)?.into_canvas()?;

    // TODO: sort this block of stuff out
    let wsize = gl.drawable_size();
    let ratio = wsize.1 as f32 * 1.333333;
    let xp = (wsize.0 as f32 - ratio) / 2.0;

    let crop_rect = Rect::new(xp as i32, 0, ratio as u32, wsize.1);

    ctx.set_viewport(
        crop_rect.x() as u32,
        crop_rect.y() as u32,
        crop_rect.width(),
        crop_rect.height(),
    );

    let mut shader: Box<dyn Drawer> = if let Some(shader) = options.shader {
        match shader {
            Shaders::Basic => Box::new(Basic::new(&ctx)),
            Shaders::Lottes => Box::new(LottesCRT::new(&ctx)),
            Shaders::Cgwg => Box::new(Cgwgcrt::new(&ctx, crop_rect.width(), crop_rect.height())),
        }
    } else {
        Box::new(Basic::new(&ctx))
    };
    shader.set_tex_filter().unwrap();

    let mut pal_num = 0;
    let mut image_num = 0;
    let mut tex_num = 0;
    let mut flat_num = 0;
    let mut sprite_num = 119;
    let images: Option<Vec<WadPatch>> = if options.texpatch_test || options.texture_test {
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

    loop {
        if !game.running() {
            break;
        }
        // // Update the game state
        try_run_tics(&mut game, &mut input, &mut timestep);

        // TODO: S_UpdateSounds(players[consoleplayer].mo); // move positional sounds
        // Draw everything to the buffer
        d_display(&mut renderer, &game, &mut render_buffer);

        if options.palette_test {
            palette_test(pal_num, &mut game, &mut render_buffer);
        }

        if let Some(name) = options.image_test.clone() {
            image_test(&name.to_ascii_uppercase(), &mut game, &mut render_buffer);
        }
        if let Some(images) = &images {
            patch_select_test(&images[image_num], &mut game, &mut render_buffer);
        }
        if let Some(flats) = &flats {
            flat_select_test(&flats[flat_num], &game, &mut render_buffer);
        }
        if let Some(sprites) = &sprites {
            patch_select_test(&sprites[sprite_num], &game, &mut render_buffer);
        }
        if options.texture_test {
            texture_select_test(
                game.textures.borrow_mut().get_texture(tex_num),
                &game,
                &mut render_buffer,
            );
        }

        let pix = render_buffer
            .read_pixels(render_buffer.surface().rect(), PixelFormatEnum::RGBA32)
            .unwrap();

        shader.clear();
        shader.set_image_data(&pix, render_buffer.surface().size());
        shader.draw().unwrap();

        gl.gl_swap_window();

        // FPS rate updates every second
        if let Some(_fps) = timestep.frame_rate() {
            //println!("{:?}", fps);

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
                if tex_num < game.textures.borrow_mut().num_textures() - 1 {
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
    Ok(())
}

/// D_Display
/// Does a bunch of stuff in Doom...
fn d_display(rend: &mut impl Renderer, game: &Game, canvas: &mut Canvas<Surface>) {
    //if (gamestate == GS_LEVEL && !automapactive && gametic)

    if let Some(ref level) = game.level {
        if !game.player_in_game[0] {
            return;
        }

        let player = &game.players[game.consoleplayer];
        rend.render_player_view(player, level, canvas);
    }

    //canvas.present();

    // // menus go directly to the screen
    // TODO: M_Drawer();	 // menu is drawn even on top of everything
    // net update does i/o and buildcmds...
    // TODO: NetUpdate(); // send out any new accumulation
}

fn try_run_tics(game: &mut Game, input: &mut Input, timestep: &mut TimeStep) {
    // TODO: net.c starts here
    input.update(game); // D_ProcessEvents

    let console_player = game.consoleplayer;
    // net update does i/o and buildcmds...
    // TODO: NetUpdate(); // send out any new accumulation

    // temporary block
    game.set_running(!input.get_quit());

    // TODO: Network code would update each player slot with incoming TicCmds...
    let cmd = input.tic_events.build_tic_cmd(&input.config);
    game.netcmds[console_player][0] = cmd;

    // Special key check
    if input.tic_events.is_kb_pressed(Scancode::Escape) {
        game.set_running(false);
    }

    // Build tics here?
    // TODO: Doom-like timesteps
    timestep.run_this(|_| {
        // G_Ticker
        game.ticker();
    });
}

fn palette_test(pal_num: usize, game: &mut Game, canvas: &mut Canvas<Surface>) {
    let height = canvas.surface().height();

    let row_count: i32 = 16;
    let block_size = height as i32 / row_count;

    let x_start = (canvas.surface().width() / 2) as i32 - block_size * row_count / 2;
    let y_start = (canvas.surface().height() / 2) as i32 - block_size * row_count / 2;

    let pals: Vec<WadPalette> = game.wad_data.playpal_iter().collect();

    for (i, c) in pals[pal_num].0.iter().enumerate() {
        canvas.set_draw_color(Color::RGB(c.r, c.g, c.b));
        canvas
            .fill_rect(Rect::new(
                i as i32 % row_count * block_size + x_start,
                i as i32 / row_count * block_size + y_start,
                block_size as u32,
                block_size as u32,
            ))
            .unwrap();
    }
}

fn image_test(name: &str, game: &Game, canvas: &mut Canvas<Surface>) {
    let lump = game.wad_data.get_lump(name).unwrap();
    let image = WadPatch::from_lump(lump);
    let pals: Vec<WadPalette> = game.wad_data.playpal_iter().collect();

    let xs = ((canvas.surface().width() - image.width as u32) / 2) as i32;
    let ys = ((canvas.surface().height() - image.height as u32) / 2) as i32;

    let mut x = 0;
    for c in image.columns.iter() {
        for (y, p) in c.pixels.iter().enumerate() {
            let colour = pals[0].0[*p];
            canvas.set_draw_color(Color::RGB(colour.r, colour.g, colour.b));
            canvas
                .fill_rect(Rect::new(
                    xs + x as i32,                     // - (image.left_offset as i32),
                    ys + y as i32 + c.y_offset as i32, // - image.top_offset as i32 - 30,
                    1,
                    1,
                ))
                .unwrap();
        }
        if c.y_offset == 255 {
            x += 1;
        }
    }
}

fn patch_select_test(image: &WadPatch, game: &Game, canvas: &mut Canvas<Surface>) {
    let pals: Vec<WadPalette> = game.wad_data.playpal_iter().collect();

    let xs = ((canvas.surface().width() - image.width as u32) / 2) as i32;
    let ys = ((canvas.surface().height() - image.height as u32) / 2) as i32;

    let mut x = 0;
    for c in image.columns.iter() {
        for (y, p) in c.pixels.iter().enumerate() {
            let colour = pals[0].0[*p];
            canvas.set_draw_color(Color::RGB(colour.r, colour.g, colour.b));
            canvas
                .fill_rect(Rect::new(
                    xs + x as i32,                     // - (image.left_offset as i32),
                    ys + y as i32 + c.y_offset as i32, // - image.top_offset as i32 - 30,
                    1,
                    1,
                ))
                .unwrap();
        }
        if c.y_offset == 255 {
            x += 1;
        }
    }
}

fn texture_select_test(texture: &Texture, game: &Game, canvas: &mut Canvas<Surface>) {
    let width = texture.data.len() as u32;
    let height = texture.data[0].len() as u32;
    let pals: Vec<WadPalette> = game.wad_data.playpal_iter().collect();

    let xs = ((canvas.surface().width() - width) / 2) as i32;
    let ys = ((canvas.surface().height() - height) / 2) as i32;
    let pal = pals[0].0;

    for (x_pos, column) in texture.data.iter().enumerate() {
        for (y_pos, idx) in column.iter().enumerate() {
            if *idx >= pal.len() {
                continue;
            }
            let colour = pal[*idx];
            canvas.set_draw_color(Color::RGB(colour.r, colour.g, colour.b));
            canvas
                .fill_rect(Rect::new(xs + x_pos as i32, ys + y_pos as i32, 1, 1))
                .unwrap();
        }
    }
}

fn flat_select_test(flat: &WadFlat, game: &Game, canvas: &mut Canvas<Surface>) {
    let pals: Vec<WadPalette> = game.wad_data.playpal_iter().collect();

    let xs = ((canvas.surface().width() - 64) / 2) as i32;
    let ys = ((canvas.surface().height() - 64) / 2) as i32;
    let pal = pals[0].0;

    for (y, col) in flat.data.chunks(64).enumerate() {
        for (x, px) in col.iter().enumerate() {
            if *px as usize >= pal.len() {
                continue;
            }
            let colour = pal[*px as usize];
            canvas.set_draw_color(Color::RGB(colour.r, colour.g, colour.b));
            canvas
                .fill_rect(Rect::new(xs + x as i32, ys + y as i32, 1, 1))
                .unwrap();
        }
    }
}
