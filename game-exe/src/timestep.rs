//! Gameplay speed control. Fixed 35 tics/second with sub-tic tracking
//! for rendering interpolation.

use std::fmt;
use std::time::Instant;

const MS_PER_UPDATE: f32 = 28.571428571;

#[derive(Debug)]
pub struct TimeStep {
    frame_count: u32,
    frame_time: f32,
    run_tics: u32,

    last_tics: u32,
    last_time: Instant,
    delta_time: f32,
    real_lag: f32,
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
            real_lag: 0.0,
        }
    }

    fn delta(&mut self) -> f32 {
        let current_time = Instant::now();
        let delta = current_time.duration_since(self.last_time).as_nanos() as f32 * 0.000001;
        self.last_time = current_time;
        self.delta_time = delta;
        delta
    }

    /// Run game tics for accumulated time. After this returns, `frac()`
    /// gives the sub-tic remainder for rendering interpolation.
    pub fn run_this(&mut self, mut run_this: impl FnMut()) {
        let dt = self.delta();
        self.real_lag += dt;
        while self.real_lag >= MS_PER_UPDATE {
            run_this();
            self.real_lag -= MS_PER_UPDATE;
            self.run_tics += 1;
        }
    }

    /// Fractional progress into the next tic (0.0..1.0).
    /// Used for rendering interpolation between the previous and current tic.
    pub fn frac(&self) -> f32 {
        (self.real_lag / MS_PER_UPDATE).clamp(0.0, 1.0)
    }

    pub fn frame_rate(&mut self) -> Option<FrameData> {
        self.frame_count += 1;
        self.frame_time += self.delta_time;
        // per second
        if self.frame_time >= 1000.0 {
            let frames = self.frame_count;
            let prev_tics = self.last_tics;
            self.frame_count = 0;
            self.frame_time = 0.0;
            self.last_tics = self.run_tics;
            return Some(FrameData {
                tics: self.run_tics - prev_tics,
                frames,
            });
        }

        None
    }
}

impl Default for TimeStep {
    fn default() -> Self {
        Self::new()
    }
}
