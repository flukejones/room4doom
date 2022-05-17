//! A generic `PixelBuf` that can be drawn to and is blitted to screen by the game,
//! and a generic `PlayRenderer` for rendering the players view of the level.

use gameplay::{Level, Player};

/// A structure holding display data
pub struct PixelBuf {
    width: u32,
    height: u32,
    /// Total length is width * height * 4, where 4 is RGBA bytes
    data: Vec<u8>,
}

impl PixelBuf {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            data: vec![0; (width * height * 4) as usize],
        }
    }

    #[inline]
    pub fn clear(&mut self) {
        self.data = vec![0; (self.width * self.height * 4) as usize]
    }

    #[inline]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[inline]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Get width and height as a tuple
    #[inline]
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Set this pixel at X|Y to RGBA colour
    #[inline]
    pub fn set_pixel(&mut self, x: usize, y: usize, r: u8, g: u8, b: u8, a: u8) {
        // Shitty safeguard. Need to find actual cause of fail
        if x >= self.width as usize || y >= self.height as usize {
            return;
        }

        let pos = y * (self.width as usize * 4) + x * 4;
        self.data[pos] = r;
        self.data[pos + 1] = g;
        self.data[pos + 2] = b;
        self.data[pos + 3] = a;
    }

    /// Read the colour of a single pixel at X|Y
    pub fn read_pixel(&self, x: usize, y: usize) -> (u8, u8, u8, u8) {
        let pos = y * (self.width as usize * 4) + x * 4;
        (
            self.data[pos],
            self.data[pos + 1],
            self.data[pos + 2],
            self.data[pos + 3],
        )
    }

    /// Get the array of pixels. The layout of which is [Row<RGBA>]
    pub fn read_pixels(&self) -> &[u8] {
        &self.data
    }
}

pub trait PlayRenderer {
    /// Drawing the full player view to the `PixelBuf`.
    ///
    /// Doom function name `R_RenderPlayerView`
    fn render_player_view(&mut self, player: &Player, level: &Level, buf: &mut PixelBuf);
}

#[cfg(test)]
mod tests {
    use crate::PixelBuf;

    #[test]
    fn write_read_pixel() {
        let mut pixels = PixelBuf::new(320, 200);

        pixels.set_pixel(10, 10, 255, 10, 3, 255);
        pixels.set_pixel(319, 199, 25, 10, 3, 255);

        let px = pixels.read_pixel(10, 10);
        assert_eq!(px, (255, 10, 3, 255));

        let px = pixels.read_pixel(319, 199);
        assert_eq!(px, (25, 10, 3, 255));
    }
}
