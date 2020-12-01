use golem::*;
use gumdrop::Options;
use sdl2;

use gamelib::{
    d_main::d_doom_loop, d_main::GameOptions, game::Game, input::Input,
};

/// The main `game` crate should take care of initialising a few things
fn main() {
    let sdl_ctx = sdl2::init().unwrap();
    let video_ctx = sdl_ctx.video().unwrap();

    let events = sdl_ctx.event_pump().unwrap();
    let input = Input::new(events);

    let options = GameOptions::parse_args_default_or_exit();

    println!("{:?}", options);
    let mut window = video_ctx
        .window("DIIRDOOM", options.width, options.height)
        .position_centered()
        .opengl()
        .build()
        .unwrap();

    if options.fullscreen {
        window
            .set_fullscreen(sdl2::video::FullscreenType::Desktop)
            .unwrap();
    }

    sdl_ctx.mouse().show_cursor(false);
    sdl_ctx.mouse().set_relative_mouse_mode(true);
    sdl_ctx.mouse().capture(true);

    // let canvas = window
    //     .into_canvas()
    //     .index(find_sdl_gl_driver().unwrap())
    //     .accelerated()
    //     .present_vsync()
    //     .build()
    //     .unwrap();

    // initialization
    let gl_attr = video_ctx.gl_attr();
    gl_attr.set_context_profile(sdl2::video::GLProfile::Core);
    gl_attr.set_context_version(3, 0);
    let gl_context = window.gl_create_context().unwrap();
    let context = glow::Context::from_loader_function(|s| {
        video_ctx.gl_get_proc_address(s) as *const _
    });
    let context = Context::from_glow(context).unwrap();

    let game = Game::new(options);

    d_doom_loop(game, input, window, context);
}

fn find_sdl_gl_driver() -> Option<u32> {
    for (index, item) in sdl2::render::drivers().enumerate() {
        if item.name == "opengl" {
            return Some(index as u32);
        }
    }
    None
}
