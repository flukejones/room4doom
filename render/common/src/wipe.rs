use std::time::Instant;

use math::m_random;
use pic_data::PixelFmt;

/// Duration between wipe steps (~60Hz).
const STEP_INTERVAL_MS: u128 = 16;

/// Per-column melt offset state shared by the CPU [`Wipe`] and the GPU melt pass.
///
/// Holds the jagged `y[x]` heights, the step clock, and the advance logic. Owning
/// the RNG-seeded `init_offsets` in one place keeps both renderers consuming the
/// same `m_random` sequence (demo determinism).
pub struct MeltColumns {
    y: Vec<i32>,
    width: i32,
    height: i32,
    /// Time of the last wipe step, used to gate advancement.
    last_step: Instant,
}

impl MeltColumns {
    pub fn new(width: i32, height: i32) -> Self {
        Self {
            y: Self::init_offsets(width),
            width,
            height,
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

    /// Re-seed the column offsets and reset the step clock.
    pub fn reset(&mut self) {
        self.y = Self::init_offsets(self.width);
        self.last_step = Instant::now();
    }

    /// The current per-column melt heights (px).
    pub fn offsets(&self) -> &[i32] {
        &self.y
    }

    /// Restart the step clock (a new wipe begins).
    pub fn reset_clock(&mut self) {
        self.last_step = Instant::now();
    }

    /// True if at least one step interval has elapsed; resets the clock when so.
    fn take_step(&mut self) -> bool {
        let should = self.last_step.elapsed().as_millis() >= STEP_INTERVAL_MS;
        if should {
            self.last_step = Instant::now();
        }
        should
    }

    /// Advance the offsets one melt step (no pixel work). Returns true when every
    /// column has fully melted. Same stepping as the CPU [`Wipe::do_melt_pixels`].
    pub fn advance(&mut self) -> bool {
        let should_step = self.take_step();
        let stepping = self.height as usize / 100;
        let f = self.height / 200;
        let mut done = true;
        for x in (0..self.width as usize - stepping).step_by(stepping) {
            if self.y[x] < 0 {
                if should_step {
                    self.y[x] += stepping as i32 / 2;
                }
                done = false;
            } else if self.y[x] < self.height {
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
        }
        done
    }
}

/// CPU melt-wipe: overdraws columns of the *last presented frame* (held by the
/// surface provider's back buffer) shifted down over the freshly rendered frame.
///
/// Holds no pixel buffer — the old frame lives in the provider's spare surface.
/// `width`/`height` track the geometry the column offsets were seeded for.
pub struct Wipe {
    melt: MeltColumns,
    height: i32,
    width: i32,
    active: bool,
}

impl Wipe {
    pub fn new(width: i32, height: i32) -> Self {
        Self {
            melt: MeltColumns::new(width, height),
            height,
            width,
            active: false,
        }
    }

    /// Begin a wipe: re-seed the column offsets and restart the step clock. The
    /// old frame is whatever the provider's back buffer holds (last present).
    pub fn start(&mut self) {
        self.melt.reset();
        self.active = true;
    }

    /// True while a wipe is in progress.
    pub fn is_wiping(&self) -> bool {
        self.active
    }

    /// Overdraw shifted last-frame columns onto the freshly rendered surface.
    ///
    /// `new` is the current surface (just rendered, `new_pitch` elements/row);
    /// `old` is the last presented frame (`old_pitch` elements/row). Columns of
    /// `old` are painted shifted down by their per-column melt offset, covering
    /// the portion where the old scene should still show. Returns true (and
    /// clears the active flag) when the melt completes.
    pub fn do_melt_pixels<P: PixelFmt>(
        &mut self,
        new: &mut [P],
        new_pitch: usize,
        old: &[P],
        old_pitch: usize,
    ) -> bool {
        let stepping = self.height as usize / 100;
        let offsets = self.melt.offsets();

        for x in (0..self.width as usize - stepping).step_by(stepping) {
            if offsets[x] < 0 {
                // Column hasn't started melting — overdraw the whole column.
                for col in x..x + stepping {
                    for row in 0..self.height as usize {
                        new[row * new_pitch + col] = old[row * old_pitch + col];
                    }
                }
            } else if offsets[x] < self.height {
                let melt_y = offsets[x] as usize;
                for col in x..x + stepping {
                    for src_y in 0..(self.height as usize - melt_y) {
                        let dst_y = src_y + melt_y;
                        new[dst_y * new_pitch + col] = old[src_y * old_pitch + col];
                    }
                }
            }
            // else: column fully melted, new scene shows through
        }
        let done = self.melt.advance();
        if done {
            self.active = false;
        }
        done
    }
}
