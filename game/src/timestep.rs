//! Gameplay speed control. It attempts to mimic/limit progression to
//! the 30 tics per second Doom used.

use std::{fmt, time::Instant};

const MS_PER_UPDATE: f32 = 28.57;

#[derive(Debug)]
pub struct TimeStep {
    last_time: Instant,
    delta_time: f32,
    frame_count: u32,
    frame_time: f32,
    run_tics: u32,
    last_tics: u32,
    lag: f32,
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
    pub fn new() -> TimeStep {
        TimeStep {
            last_time: Instant::now(),
            delta_time: 0.0,
            frame_count: 0,
            frame_time: 0.0,
            run_tics: 0,
            last_tics: 0,
            lag: 0.0,
        }
    }

    pub fn delta(&mut self) -> f32 {
        let current_time = Instant::now();
        let delta = current_time.duration_since(self.last_time).as_micros() as f32 * 0.001;
        self.last_time = current_time;
        self.delta_time = delta;
        delta
    }

    /// Increments self time and returns current lag. `run_this` is run only for
    /// `n` tics available.
    pub fn run_this(&mut self, mut run_this: impl FnMut(f32)) {
        let dt = self.delta();
        self.lag += dt;
        while self.lag >= MS_PER_UPDATE {
            run_this(dt);
            self.lag -= MS_PER_UPDATE;
            self.run_tics += 1;
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
        Self::new()
    }
}
