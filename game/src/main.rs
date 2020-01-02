#![allow(clippy::pedantic)]
#![allow(clippy::new_without_default)]

mod game;
mod input;

use crate::game::Game;
use gumdrop::{Options, ParsingStyle};
use sdl2;
use std::env;
use std::time::Instant;

#[derive(Default, Debug, Options)]
pub struct GameOptions {
    #[options(help = "path to game WAD", required)]
    pub iwad: String,
    #[options(help = "path to patch WAD")]
    pub pwad: Option<String>,
    #[options(help = "resolution width in pixels")]
    pub width: Option<u32>,
    #[options(help = "resolution height in pixels")]
    pub height: Option<u32>,
    #[options(help = "waesgr")]
    pub fullscreen: Option<bool>,
}

type FP = f64;
const MS_PER_UPDATE: FP = 4.0;

#[derive(Debug)]
pub struct TimeStep {
    last_time: Instant,
    delta_time: FP,
    frame_count: u32,
    frame_time: FP,
}

impl TimeStep {
    pub fn new() -> TimeStep {
        TimeStep {
            last_time: Instant::now(),
            delta_time: 0.0,
            frame_count: 0,
            frame_time: 0.0,
        }
    }

    pub fn delta(&mut self) -> FP {
        let current_time = Instant::now();
        let delta = current_time.duration_since(self.last_time).as_micros() as FP * 0.001;
        self.last_time = current_time;
        self.delta_time = delta;
        delta
    }

    pub fn frame_rate(&mut self) -> Option<u32> {
        self.frame_count += 1;
        self.frame_time += self.delta_time;
        let tmp;
        if self.frame_time >= 1000.0 {
            tmp = self.frame_count;
            self.frame_count = 0;
            self.frame_time = 0.0;
            return Some(tmp);
        }
        None
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    // An SDL context is needed before we can procceed
    let mut sdl_ctx = sdl2::init().unwrap();
    let mut game = match GameOptions::parse_args(&args[1..], ParsingStyle::AllOptions) {
        Ok(opts) => {
            println!("{:?}", opts);
            Game::new(&mut sdl_ctx, opts)
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
