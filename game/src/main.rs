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
        window.set_fullscreen(sdl2::video::FullscreenType::Desktop).unwrap();
    }

    sdl_ctx.mouse().show_cursor(false);
    sdl_ctx.mouse().set_relative_mouse_mode(true);
    sdl_ctx.mouse().capture(true);

    let canvas = window
        .into_canvas()
        .accelerated()
        .present_vsync()
        .build()
        .unwrap();

    let game = Game::new(options);

    d_doom_loop(game, input, canvas);
}
