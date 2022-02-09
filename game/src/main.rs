use std::error::Error;

use golem::*;
use gumdrop::Options;
use sdl2;

use gamelib::{d_main::d_doom_loop, d_main::GameOptions, game::Game, input::Input};

/// The main `game` crate should take care of initialising a few things
fn main() -> Result<(), Box<dyn Error>> {
    let sdl_ctx = sdl2::init()?;
    let video_ctx = sdl_ctx.video()?;

    let events = sdl_ctx.event_pump()?;
    let input = Input::new(events);

    let options = GameOptions::parse_args_default_or_exit();

    println!("{:?}", options);
    let mut window = video_ctx
        .window("DIIRDOOM", options.width, options.height)
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

    sdl_ctx.mouse().show_cursor(false);
    sdl_ctx.mouse().set_relative_mouse_mode(true);
    sdl_ctx.mouse().capture(true);

    d_doom_loop(game, input, window, context)
}
