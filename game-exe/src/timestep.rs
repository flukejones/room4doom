//! Gameplay speed control. It attempts to mimic/limit progression to
//! the 30 tics per second Doom used.

use std::fmt;
use std::time::Instant;

use gamestate_traits::sdl2::sys::SDL_GetTicks;

const MS_PER_UPDATE: f32 = 28.571428571;
const TICRATE: u32 = 35;

#[derive(Debug)]
pub struct TimeStep {
    frame_count: u32,
    frame_time: f32,
    run_tics: u32,

    last_tics: u32,
    last_time: Instant,
    delta_time: f32,
    real_lag: f32,

    base_time: u32,
    last_dt: u32,
    doom_style: bool,
}

#[derive(Debug)]
pub struct FrameData {
    pub tics: u32,
    pub frames: u32,
}

impl fmt::Display for FrameData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!(
            "FrameData (per-second):\n  - tics: {}\n  -  fps: {}",
            self.tics, self.frames
        ))
    }
}

impl TimeStep {
    pub fn new(doom_style: bool) -> TimeStep {
        TimeStep {
            last_time: Instant::now(),
            delta_time: 0.0,
            frame_count: 0,
            frame_time: 0.0,
            run_tics: 0,
            last_tics: 0,
            base_time: unsafe { SDL_GetTicks() },
            last_dt: 0,
            real_lag: 0.0,
            doom_style,
        }
    }

    // in millis since start
    fn get_time(&self) -> u32 {
        let now = unsafe { SDL_GetTicks() };
        let now = now - self.base_time;
        now * TICRATE / 1000
    }

    fn run_this_doom(&mut self, mut run_this: impl FnMut(f32)) {
        self.delta();
        let dt = self.get_time();
        let real = dt - self.last_dt;
        self.last_dt = dt;
        let mut counts = real;
        while counts != 0 {
            run_this(dt as f32);
            counts -= 1;
            self.run_tics += 1;
        }
    }

    fn delta(&mut self) -> f32 {
        let current_time = Instant::now();
        let delta = current_time.duration_since(self.last_time).as_nanos() as f32 * 0.000001;
        self.last_time = current_time;
        self.delta_time = delta;
        delta
    }

    /// Increments self time and returns current lag. `run_this` is run only for
    /// `n` tics available.
    fn run_this_real(&mut self, mut run_this: impl FnMut(f32)) {
        let dt = self.delta();
        self.real_lag += dt;
        while self.real_lag > MS_PER_UPDATE {
            run_this(dt);
            self.real_lag -= MS_PER_UPDATE;
            self.run_tics += 1;
        }
    }

    pub fn run_this(&mut self, run_this: impl FnMut(f32)) {
        if self.doom_style {
            self.run_this_doom(run_this);
        } else {
            self.run_this_real(run_this);
        }
    }

    pub fn frame_rate(&mut self) -> Option<FrameData> {
        self.frame_count += 1;
        self.frame_time += self.delta_time;
        let tmp;
        let tmp2;
        // per second
        if self.frame_time >= 1000.0 {
            tmp = self.frame_count;
            tmp2 = self.last_tics;
            self.frame_count = 0;
            self.frame_time = 0.0;
            self.last_tics = self.run_tics;
            return Some(FrameData {
                tics: self.run_tics - tmp2,
                frames: tmp,
            });
        }

        None
    }
}

impl Default for TimeStep {
    // shutup clippy!
    fn default() -> Self {
        Self::new(false)
    }
}
