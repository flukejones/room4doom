use std::time::Instant;

const MS_PER_UPDATE: f32 = 28.57;

#[derive(Debug)]
pub struct TimeStep {
    last_time:   Instant,
    delta_time:  f32,
    frame_count: u32,
    frame_time:  f32,
    lag:         f32,
}

impl TimeStep {
    pub fn new() -> TimeStep {
        TimeStep {
            last_time:   Instant::now(),
            delta_time:  0.0,
            frame_count: 0,
            frame_time:  0.0,
            lag:         0.0,
        }
    }

    pub fn delta(&mut self) -> f32 {
        let current_time = Instant::now();
        let delta = current_time.duration_since(self.last_time).as_micros()
            as f32
            * 0.001;
        self.last_time = current_time;
        self.delta_time = delta;
        delta
    }

    /// Increments self time and returns current lag
    pub fn run_this(&mut self, mut run_this: impl FnMut(f32)) {
        let dt = self.delta();
        self.lag += dt;
        while self.lag >= MS_PER_UPDATE {
            run_this(dt);
            self.lag -= MS_PER_UPDATE;
        }
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
