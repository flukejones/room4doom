use std::error::Error;

use doom_lib::Game;
use golem::Context;
use sdl2::{
    keyboard::Scancode,
    pixels::{Color, PixelFormatEnum},
    rect::Rect,
    render::Canvas,
    surface::Surface,
    video::Window,
};
use wad::lumps::{WadPalette, WadPatch, WadTexture};

use crate::{
    input::Input,
    renderer::{bsp::BspRenderer, plane::VisPlaneCtrl, RenderData},
    shaders::{basic::Basic, cgwg_crt::Cgwgcrt, lottes_crt::LottesCRT, Drawer, Shaders},
    timestep::TimeStep,
    GameOptions,
};

struct Renderer {
    bsp_renderer: BspRenderer,
    r_data: RenderData,
    visplanes: VisPlaneCtrl,
    crop_rect: Rect,
}

impl Renderer {
    fn new() -> Self {
        Self {
            bsp_renderer: BspRenderer::default(),
            r_data: RenderData::default(),
            visplanes: VisPlaneCtrl::default(),
            crop_rect: Rect::new(0, 0, 1, 1),
        }
    }

    /// D_Display
    // TODO: Move
    pub fn render_player_view(&mut self, game: &Game, canvas: &mut Canvas<Surface>) {
        if !game.player_in_game[0] {
            return;
        }

        if let Some(ref level) = game.level {
            let map = &level.map_data;

            let player = &game.players[game.consoleplayer];

            self.visplanes.clear_planes();
            self.bsp_renderer.clear_clip_segs();
            self.r_data.clear_data();
            // The state machine will handle which state renders to the surface
            //self.states.render(dt, &mut self.canvas);

            let colour = sdl2::pixels::Color::RGBA(90, 80, 80, 255);
            canvas.set_draw_color(colour);
            canvas.fill_rect(Rect::new(0, 0, 320, 100)).unwrap();
            let colour = sdl2::pixels::Color::RGBA(90, 90, 90, 255);
            canvas.set_draw_color(colour);
            canvas.fill_rect(Rect::new(0, 100, 320, 100)).unwrap();
            self.bsp_renderer.render_bsp_node(
                map,
                player,
                map.start_node(),
                &mut self.r_data,
                canvas,
            );
        }
    }
}

/// Never returns
pub fn d_doom_loop(
    mut game: Game,
    mut input: Input,
    gl: Window,
    ctx: Context,
    options: GameOptions,
) -> Result<(), Box<dyn Error>> {
    let mut renderer = Renderer::new();

    let mut timestep = TimeStep::new();
    let mut render_buffer = Surface::new(320, 200, PixelFormatEnum::RGBA32)?.into_canvas()?;

    // TODO: sort this block of stuff out
    let wsize = gl.drawable_size();
    let ratio = wsize.1 as f32 * 1.333333;
    let xp = (wsize.0 as f32 - ratio) / 2.0;
    renderer.crop_rect = Rect::new(xp as i32, 0, ratio as u32, wsize.1);

    ctx.set_viewport(
        renderer.crop_rect.x() as u32,
        renderer.crop_rect.y() as u32,
        renderer.crop_rect.width(),
        renderer.crop_rect.height(),
    );

    let mut shader: Box<dyn Drawer> = if let Some(shader) = options.shader {
        match shader {
            Shaders::Basic => Box::new(Basic::new(&ctx)),
            Shaders::Lottes => Box::new(LottesCRT::new(&ctx)),
            Shaders::Cgwg => Box::new(Cgwgcrt::new(
                &ctx,
                renderer.crop_rect.width(),
                renderer.crop_rect.height(),
            )),
        }
    } else {
        Box::new(Basic::new(&ctx))
    };
    shader.set_tex_filter().unwrap();

    let mut pal_num = 0;
    let mut image_num = 0;
    let mut tex_num = 0;
    let images: Option<Vec<WadPatch>> = if options.texpatch_test || options.texture_test {
        Some(game.wad_data.patches_iter().collect())
    } else {
        None
    };
    let textures: Option<Vec<WadTexture>> = if options.texture_test {
        Some(game.wad_data.texture_iter().collect())
    } else {
        None
    };

    loop {
        if !game.running() {
            break;
        }

        render_buffer.set_draw_color(Color::RGBA(15, 0, 0, 0));
        render_buffer.clear();

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
        if options.texpatch_test {
            if let Some(images) = &images {
                patch_cycle_test(&images[image_num], &mut game, &mut render_buffer);
            }
        }
        if options.texture_test {
            if let Some(images) = &images {
                if let Some(textures) = &textures {
                    texture_cycle_test(&textures[tex_num], images, &mut game, &mut render_buffer);
                }
            }
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

            if options.texpatch_test {
                if let Some(images) = &images {
                    if image_num < images.len() - 1 {
                        image_num += 1;
                    } else {
                        image_num = 0;
                    }
                }
            }

            if options.texture_test {
                if let Some(tex) = &textures {
                    if tex_num < tex.len() - 1 {
                        tex_num += 1;
                    } else {
                        tex_num = 0;
                    }
                }
            }
        }
    }
    Ok(())
}

/// D_Display
/// Does a bunch of stuff in Doom...
fn d_display(rend: &mut Renderer, game: &Game, canvas: &mut Canvas<Surface>) {
    //if (gamestate == GS_LEVEL && !automapactive && gametic)
    rend.render_player_view(game, canvas);
    //canvas.present();

    // // menus go directly to the screen
    // TODO: M_Drawer();	 // menu is drawn even on top of everything
    // net update does i/o and buildcmds...
    // TODO: NetUpdate(); // send out any new accumulation
}

fn try_run_tics(game: &mut Game, input: &mut Input, timestep: &mut TimeStep) {
    // TODO: net.c starts here
    input.update(); // D_ProcessEvents

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

fn image_test(name: &str, game: &mut Game, canvas: &mut Canvas<Surface>) {
    let lump = game.wad_data.find_lump_or_panic(name);
    let image = WadPatch::from_lump(lump, &game.wad_data);
    let pals: Vec<WadPalette> = game.wad_data.playpal_iter().collect();

    let mut x = 0;
    for c in image.columns.iter() {
        for (y, p) in c.pixels.iter().enumerate() {
            let colour = pals[0].0[*p];
            canvas.set_draw_color(Color::RGB(colour.r, colour.g, colour.b));
            canvas
                .fill_rect(Rect::new(
                    x as i32 + (image.left_offset as i32 / 4),
                    y as i32 + c.y_offset as i32 + (image.top_offset as i32 / 4),
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

fn patch_cycle_test(image: &WadPatch, game: &mut Game, canvas: &mut Canvas<Surface>) {
    let pals: Vec<WadPalette> = game.wad_data.playpal_iter().collect();

    let mut x = 0;
    for c in image.columns.iter() {
        for (y, p) in c.pixels.iter().enumerate() {
            let colour = pals[0].0[*p];
            canvas.set_draw_color(Color::RGB(colour.r, colour.g, colour.b));
            canvas
                .fill_rect(Rect::new(
                    x as i32 + (image.left_offset as i32 / 4),
                    y as i32 + c.y_offset as i32 + (image.top_offset as i32 / 4),
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

fn texture_cycle_test(
    texture: &WadTexture,
    patches: &[WadPatch],
    game: &mut Game,
    canvas: &mut Canvas<Surface>,
) {
    let pals: Vec<WadPalette> = game.wad_data.playpal_iter().collect();

    let xs = ((canvas.surface().width() - texture.width) / 2) as i32;
    let ys = ((canvas.surface().height() - texture.height) / 2) as i32;

    for patch_pos in &texture.patches {
        let patch = &patches[patch_pos.patch_index];
        // draw patch
        let mut x_pos = patch_pos.origin_x;
        for c in patch.columns.iter() {
            if x_pos == texture.width as i32 {
                break;
            }
            for (y, p) in c.pixels.iter().enumerate() {
                let y_pos = y as i32 + patch_pos.origin_y + c.y_offset as i32;
                if y_pos > 0 && y_pos < texture.height as i32 {
                    let colour = pals[0].0[*p];
                    canvas.set_draw_color(Color::RGB(colour.r, colour.g, colour.b));
                    canvas
                        .fill_rect(Rect::new(xs + x_pos, ys + y_pos, 1, 1))
                        .unwrap();
                }
            }
            if c.y_offset == 255 {
                x_pos += 1;
            }
        }
    }
}
