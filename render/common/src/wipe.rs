use std::time::Instant;

use math::m_random;

/// Duration between wipe steps (~60Hz).
const STEP_INTERVAL_MS: u128 = 16;

pub struct Wipe {
    y: Vec<i32>,
    height: i32,
    width: i32,
    /// Snapshot of the old frame taken when the wipe starts.
    snapshot: Vec<u32>,
    /// Time of the last wipe step, used to gate advancement.
    last_step: Instant,
}

impl Wipe {
    pub fn new(width: i32, height: i32) -> Self {
        Self {
            y: Self::init_offsets(width),
            height,
            width,
            snapshot: Vec::new(),
            last_step: Instant::now(),
        }
    }

    /// Generate the random jagged column offsets for a new wipe.
    fn init_offsets(width: i32) -> Vec<i32> {
        let mut y = Vec::with_capacity(width as usize);
        y.push(-(m_random() % 16));

        for i in 1..width as usize {
            let r = (m_random() % 3) - 1;
            y.push(y[i - 1] + r);
            if y[i] > 0 {
                y[i] = 0;
            } else if y[i] <= -16 {
                y[i] = -15;
            }
        }
        y
    }

    pub fn reset(&mut self) {
        self.y = Self::init_offsets(self.width);
        self.snapshot.clear();
    }

    /// Capture the current display buffer as the old frame for melting.
    pub fn start(&mut self, buf: &[u32]) {
        self.snapshot.clear();
        self.snapshot.extend_from_slice(buf);
        self.last_step = Instant::now();
    }

    /// Returns true if the snapshot has been captured (wipe is in progress).
    pub fn is_wiping(&self) -> bool {
        !self.snapshot.is_empty()
    }

    /// Overdraw shifted old-frame columns on top of the display buffer.
    ///
    /// The caller must have already rendered the new scene into `buf`.
    /// This paints the old frame's columns shifted down, covering the
    /// bottom portion where the old scene should still be visible.
    ///
    /// Only advances the melt when at least `STEP_INTERVAL_MS` has elapsed
    /// since the last step; otherwise it redraws the current state without
    /// advancing.
    ///
    /// Returns true when the melt is complete.
    pub fn do_melt_pixels(&mut self, buf: &mut [u32], pitch: usize) -> bool {
        let elapsed = self.last_step.elapsed().as_millis();
        let should_step = elapsed >= STEP_INTERVAL_MS;
        if should_step {
            self.last_step = Instant::now();
        }

        let mut done = true;
        let stepping = self.height as usize / 100;
        let f = self.height / 200;

        for x in (0..self.width as usize - stepping).step_by(stepping) {
            if self.y[x] < 0 {
                if should_step {
                    self.y[x] += stepping as i32 / 2;
                }
                // Column hasn't started melting yet — overdraw entire column
                // with old frame pixels.
                for col in x..x + stepping {
                    for row in 0..self.height as usize {
                        buf[row * pitch + col] = self.snapshot[row * pitch + col];
                    }
                }
                done = false;
            } else if self.y[x] < self.height {
                let melt_y = self.y[x] as usize;

                // Overdraw: paint old-frame pixels shifted down by melt_y.
                // Old row 0..(height - melt_y) appears at display rows
                // melt_y..height.
                for col in x..x + stepping {
                    for src_y in 0..(self.height as usize - melt_y) {
                        let dst_y = src_y + melt_y;
                        buf[dst_y * pitch + col] = self.snapshot[src_y * pitch + col];
                    }
                }

                if should_step {
                    let mut dy = if self.y[x] < (16 * f) {
                        self.y[x] + stepping as i32
                    } else {
                        8 * f
                    };
                    if self.y[x] + dy >= self.height {
                        dy = self.height - self.y[x];
                    }
                    for col in x..x + stepping {
                        if col < self.y.len() {
                            self.y[col] += dy;
                        }
                    }
                }
                done = false;
            }
            // else: column fully melted, new scene shows through
        }
        done
    }
}
