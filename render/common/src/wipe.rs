use std::time::Instant;

use math::m_random;
use pic_data::PixelFmt;

/// Duration between wipe steps (~60Hz).
const STEP_INTERVAL_MS: u128 = 16;
const MELT_HEIGHT: i32 = 200;

/// Per-column melt offset state shared by the CPU [`Wipe`] and the GPU melt pass.
///
/// Holds the jagged `y[x]` heights, the step clock, and the advance logic. Owning
/// the RNG-seeded `init_offsets` in one place keeps both renderers consuming the
/// same `m_random` sequence (demo determinism).
pub struct MeltColumns {
    y: Vec<i32>,
    progress: Vec<i32>,
    width: i32,
    height: i32,
    /// Time of the last wipe step, used to gate advancement.
    last_step: Instant,
}

impl MeltColumns {
    pub fn new(width: i32, height: i32) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        Self {
            y: vec![-1; width as usize],
            progress: Self::init_offsets(width),
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
        self.progress = Self::init_offsets(self.width);
        self.y.fill(-1);
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
    #[expect(
        clippy::cast_possible_truncation,
        reason = "rendering intentionally quantizes float coordinates and packs pixel channels and bounded buffer indices"
    )]
    pub fn advance(&mut self) -> bool {
        let should_step = self.take_step();
        let stepping = (self.height as usize / 100).max(1);
        let mut done = true;
        for x in (0..self.width as usize).step_by(stepping) {
            let end = (x + stepping).min(self.y.len());
            let mut progress = self.progress[x];
            if progress < MELT_HEIGHT {
                done = false;
                if should_step {
                    progress += if progress < 0 {
                        1
                    } else if progress < 16 {
                        progress + 2
                    } else {
                        8
                    };
                    progress = progress.min(MELT_HEIGHT);
                }
            }
            self.progress[x..end].fill(progress);
            let offset = if progress < 0 {
                -1
            } else {
                (i64::from(progress) * i64::from(self.height) / i64::from(MELT_HEIGHT)) as i32
            };
            self.y[x..end].fill(offset);
        }
        done
    }
}

/// CPU melt-wipe: overdraws columns of the *last presented frame* (held by the
/// surface provider's back buffer) shifted down over the freshly rendered frame.
///
/// Holds no pixel buffer — the old frame lives in the provider's spare surface.
/// `height` tracks the geometry the column offsets were seeded for.
pub struct Wipe {
    melt: MeltColumns,
    height: i32,
    active: bool,
}

impl Wipe {
    pub fn new(width: i32, height: i32) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        Self {
            melt: MeltColumns::new(width, height),
            height,
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
        let offsets = self.melt.offsets();

        for (x, &offset) in offsets.iter().enumerate() {
            if offset < self.height {
                let melt_y = offset.max(0) as usize;
                for src_y in 0..(self.height as usize - melt_y) {
                    new[(src_y + melt_y) * new_pitch + x] = old[src_y * old_pitch + x];
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

#[cfg(test)]
mod tests {
    use super::Wipe;
    use std::time::{Duration, Instant};

    #[test]
    fn melts_every_column_at_small_and_fractional_sizes() {
        for (w, h) in [(1, 1), (7, 53), (267, 200), (721, 541)] {
            let mut wipe = Wipe::new(w, h);
            let pitch = w as usize + 3;
            let old = vec![0xFFFF_0000u32; pitch * h as usize];
            let mut new = vec![0u32; old.len()];
            wipe.start();
            wipe.do_melt_pixels(&mut new, pitch, &old, pitch);
            assert_eq!(new[(h as usize - 1) * pitch + w as usize - 1], old[0]);
            let mut steps = 0;
            while wipe.is_wiping() && steps < 64 {
                wipe.melt.last_step = Instant::now()
                    .checked_sub(Duration::from_millis(17))
                    .expect("test step fits the monotonic clock");
                new.fill(0);
                wipe.do_melt_pixels(&mut new, pitch, &old, pitch);
                steps += 1;
            }
            assert!(!wipe.is_wiping(), "wipe did not finish at {w}×{h}");
            assert!(new.iter().all(|&pixel| pixel == 0));
        }
    }
}
