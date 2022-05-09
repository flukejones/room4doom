use game_traits::PixelBuf;
use gameplay::m_random;

#[derive(Debug)]
pub(crate) struct Wipe {
    y: Vec<i32>,
    height: i32,
    width: i32,
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

        Self { y, height, width }
    }

    pub(crate) fn do_melt(
        &mut self,
        disp_buf: &mut PixelBuf, // Display from this buffer
        draw_buf: &mut PixelBuf, // Draw to this buffer
    ) -> bool {
        let mut done = true;
        for x in 0..self.width as usize {
            if self.y[x] < 0 {
                // This is the offset to start with, sort of like a timer
                self.y[x] += 1;
                done = false;
            } else if self.y[x] < self.height {
                let mut dy = if self.y[x] < 16 { self.y[x] + 1 } else { 8 };
                if self.y[x] + dy >= self.height {
                    dy = self.height - self.y[x];
                }
                let mut y = self.y[x] as usize;
                for _ in (0..dy).rev() {
                    let px = draw_buf.read_pixel(x, y);
                    disp_buf.set_pixel(x, y, px.0, px.1, px.2, px.3);
                    y += 1;
                }
                self.y[x] += dy;
                done = false;
            }
        }
        done
    }
}
