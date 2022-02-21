use std::error::Error;

use doom_lib::game::Game;
use golem::Context;
use sdl2::{
    keyboard::Scancode,
    pixels::{Color, PixelFormatEnum},
    rect::Rect,
    render::Canvas,
    surface::Surface,
    video::Window,
};

use crate::{
    input::Input,
    renderer::{bsp::BspRenderer, plane::VisPlaneCtrl, RenderData},
    shaders::{lottes_crt::LottesCRT, Drawer},
    timestep::TimeStep,
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

    //let mut rend = Basic::new(&ctx);
    let mut shader = LottesCRT::new(&ctx);
    //let mut rend = CGWGCRT::new(&ctx, game.crop_rect.width(), game.crop_rect.height());
    shader.set_tex_filter().unwrap();

    loop {
        if !game.running() {
            break;
        }

        render_buffer.set_draw_color(Color::RGBA(15, 0, 0, 0));
        render_buffer.clear();

        // Update the game state
        try_run_tics(&mut game, &mut input, &mut timestep);

        // TODO: S_UpdateSounds(players[consoleplayer].mo); // move positional sounds
        // Draw everything to the buffer
        d_display(&mut renderer, &game, &mut render_buffer);

        let pix = render_buffer
            .read_pixels(render_buffer.surface().rect(), PixelFormatEnum::RGBA32)
            .unwrap();

        shader.clear();
        shader.set_image_data(&pix, render_buffer.surface().size());
        shader.draw().unwrap();

        gl.gl_swap_window();

        if let Some(_fps) = timestep.frame_rate() {
            //println!("{:?}", fps);
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
