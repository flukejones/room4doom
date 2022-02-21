mod d_main;
mod input;
mod renderer;
mod shaders;
mod timestep;
mod utilities;

use std::error::Error;

use d_main::d_doom_loop;
use golem::*;
use gumdrop::Options;

use doom_lib::{d_main::GameOptions, game::Game};
use input::Input;

/// The main `game` crate should take care of initialising a few things
fn main() -> Result<(), Box<dyn Error>> {
    let sdl_ctx = sdl2::init()?;
    let video_ctx = sdl_ctx.video()?;

    let events = sdl_ctx.event_pump()?;
    let input = Input::new(events);

    let options = GameOptions::parse_args_default_or_exit();

    let mut window = video_ctx
        .window("ROOM (Rusty DOOM)", options.width, options.height)
        .position_centered()
        .opengl()
        .hidden()
        .build()?;
    let _gl_ctx = window.gl_create_context()?;

    // initialization
    let gl_attr = video_ctx.gl_attr();
    gl_attr.set_context_profile(sdl2::video::GLProfile::Core);
    gl_attr.set_context_version(3, 2);

    let context = unsafe {
        Context::from_glow(glow::Context::from_loader_function(|s| {
            video_ctx.gl_get_proc_address(s) as *const _
        }))
        .unwrap()
    };

    let game = Game::new(options);

    window.show();

    if game.game_options.fullscreen {
        let mode = if game.game_options.width != 320 {
            sdl2::video::FullscreenType::Desktop
        } else {
            sdl2::video::FullscreenType::True
        };
        window.set_fullscreen(mode)?;
        window.set_bordered(false);
    }

    // sdl_ctx.mouse().show_cursor(false);
    // sdl_ctx.mouse().set_relative_mouse_mode(true);
    // sdl_ctx.mouse().capture(true);

    d_doom_loop(game, input, window, context)
}
