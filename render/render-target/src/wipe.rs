use std::thread::sleep;
use std::time::Duration;

use gameplay::m_random;

use crate::PixelBuffer;

pub(crate) struct Wipe {
    y: Vec<i32>,
    height: i32,
    width: i32,
    delay: i32,
}

impl Wipe {
    pub(crate) fn new(width: i32, height: i32) -> Self {
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

        Self {
            y,
            height,
            width,
            delay: 16,
        }
    }

    pub(crate) fn reset(&mut self) {
        *self = Self::new(self.width, self.height);
    }

    pub(crate) fn do_melt_pixels(
        &mut self,
        disp_buf: &mut dyn PixelBuffer, // Display from this buffer
        draw_buf: &mut dyn PixelBuffer, /* Draw to this buffer */
    ) -> bool {
        if self.delay > 0 {
            self.delay -= 1;
            sleep(Duration::from_micros(100));
            return false;
        }
        self.delay = 16;
        let mut done = true;
        let f = disp_buf.size().height() / 200;
        for x in 0..self.width as usize {
            if self.y[x] < 0 {
                // This is the offset to start with, sort of like a timer
                self.y[x] += 1;
                done = false;
            } else if self.y[x] < self.height {
                let mut dy = if self.y[x] < (16 * f) {
                    self.y[x] + 1
                } else {
                    8 * f
                };
                if self.y[x] + dy >= self.height {
                    dy = self.height - self.y[x];
                }

                let mut y = self.y[x] as usize;
                for _ in (0..dy).rev() {
                    let px = draw_buf.read_pixel(x, y);
                    disp_buf.set_pixel(x, y, &px);
                    y += 1;
                }
                self.y[x] += dy;

                for c in 0..=self.height - self.y[x] - dy {
                    let y = self.height - c - dy;
                    let px = disp_buf.read_pixel(x, y as usize);
                    disp_buf.set_pixel(x, (self.height - c - 1) as usize, &px);
                }
                done = false;
            }
        }
        done
    }
}