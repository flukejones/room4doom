#![allow(clippy::new_without_default)]

use std::env;
use std::time::Instant;

use gumdrop::{Options, ParsingStyle};
use sdl2;
use sdl2::{render::Canvas, video::Window};

use crate::{game::Game, input::Input};

mod game;
mod input;

#[derive(Default, Debug, Options)]
pub struct GameOptions {
    #[options(help = "path to game WAD", required)]
    pub iwad:       String,
    #[options(help = "path to patch WAD")]
    pub pwad:       Option<String>,
    #[options(help = "resolution width in pixels")]
    pub width:      Option<u32>,
    #[options(help = "resolution height in pixels")]
    pub height:     Option<u32>,
    #[options(help = "map to load")]
    pub map:        Option<String>,
    #[options(help = "waesgr")]
    pub fullscreen: Option<bool>,
}

type FP = f32;
const MS_PER_UPDATE: FP = 4.0;

#[derive(Debug)]
pub struct TimeStep {
    last_time:   Instant,
    delta_time:  FP,
    frame_count: u32,
    frame_time:  FP,
}

impl TimeStep {
    pub fn new() -> TimeStep {
        TimeStep {
            last_time:   Instant::now(),
            delta_time:  0.0,
            frame_count: 0,
            frame_time:  0.0,
        }
    }

    pub fn delta(&mut self) -> FP {
        let current_time = Instant::now();
        let delta = current_time.duration_since(self.last_time).as_micros()
            as FP
            * 0.001;
        self.last_time = current_time;
        self.delta_time = delta;
        delta
    }

    pub fn frame_rate(&mut self) -> Option<u32> {
        self.frame_count += 1;
        self.frame_time += self.delta_time;
        let tmp;
        // per second
        if self.frame_time >= 1000.0 {
            tmp = self.frame_count;
            self.frame_count = 0;
            self.frame_time = 0.0;
            return Some(tmp);
        }
        None
    }
}

/// The main `game` crate should take care of only a few things:
///
/// - WAD/PWAD loading
/// - SDL system init (window, sound)
/// - Input
/// - Commandline arg parsing
/// - Base settings such as window/fullscreen
///
/// And anything else not directly related to levels, sprites, textures, game logic etc
fn main() {
    let args: Vec<String> = env::args().collect();
    // An SDL context is needed before we can procceed
    let sdl_ctx = sdl2::init().unwrap();
    let video_ctx = sdl_ctx.video().unwrap();
    // Create a window
    let window: Window;
    let mut canvas: Canvas<Window>;

    let events = sdl_ctx.event_pump().unwrap();
    let mut input = Input::new(events);

    let mut game =
        match GameOptions::parse_args(&args[1..], ParsingStyle::AllOptions) {
            Ok(opts) => {
                println!("{:?}", opts);
                window = video_ctx
                    .window(
                        "DIIRDOOM",
                        opts.width.unwrap_or(320),
                        opts.height.unwrap_or(200),
                    )
                    .position_centered()
                    .opengl()
                    .build()
                    .unwrap();
                canvas = window
                    .into_canvas()
                    .accelerated()
                    .present_vsync()
                    .build()
                    .unwrap();

                Game::new(&mut canvas, &mut input, opts)
            }
            Err(err) => {
                panic!("\n{}\n{}", err, GameOptions::usage());
            }
        };

    let mut timestep = TimeStep::new();
    let mut lag = 0.0;

    'running: loop {
        if !game.running() {
            break 'running;
        }
        game.handle_events();

        lag += timestep.delta();

        while lag >= MS_PER_UPDATE {
            game.update(MS_PER_UPDATE * 0.01);
            lag -= MS_PER_UPDATE;
        }

        game.render(lag);
    }
}
