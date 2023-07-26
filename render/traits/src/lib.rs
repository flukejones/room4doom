//! A generic `PixelBuf` that can be drawn to and is blitted to screen by the game,
//! and a generic `PlayRenderer` for rendering the players view of the level.

use gameplay::{Level, Player};

const CHANNELS: usize = 3;

/// A structure holding display data
pub struct PixelBuf {
    width: u32,
    height: u32,
    is_software: bool,
    /// Total length is width * height * CHANNELS, where CHANNELS is RGB bytes
    software_buffer: Vec<u8>,
}

impl PixelBuf {
    pub fn new(width: u32, height: u32, is_software: bool) -> Self {
        Self {
            width,
            height,
            is_software,
            software_buffer: if is_software {
                vec![0; (width * height) as usize * CHANNELS]
            } else {
                Vec::default()
            },
        }
    }

    // #[inline]
    // pub fn clear(&mut self) {
    //     self.data = vec![0; (self.width * self.height) as usize * CHANNELS]
    // }

    #[inline]
    pub const fn width(&self) -> u32 {
        self.width
    }

    #[inline]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Set this pixel at X|Y to RGBA colour
    #[inline]
    pub fn set_pixel(&mut self, x: usize, y: usize, r: u8, g: u8, b: u8, _a: u8) {
        // Shitty safeguard. Need to find actual cause of fail
        if x >= self.width as usize || y >= self.height as usize {
            return;
        }

        let pos = y * (self.width as usize * CHANNELS) + x * CHANNELS;
        self.software_buffer[pos] = r;
        self.software_buffer[pos + 1] = g;
        self.software_buffer[pos + 2] = b;
    }

    /// Read the colour of a single pixel at X|Y
    pub fn read_pixel(&self, x: usize, y: usize) -> (u8, u8, u8, u8) {
        let pos = y * (self.width as usize * CHANNELS) + x * CHANNELS;
        (
            self.software_buffer[pos],
            self.software_buffer[pos + 1],
            self.software_buffer[pos + 2],
            0,
        )
    }

    pub fn is_software(&self) -> bool {
        self.is_software
    }

    // /// Get the array of pixels. The layout of which is [Row<RGBA>]
    pub fn read_pixels(&self) -> &[u8] {
        &self.software_buffer
    }

    pub fn read_pixels_mut(&mut self) -> &mut [u8] {
        &mut self.software_buffer
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
        let mut pixels = PixelBuf::new(320, 200, true);

        pixels.set_pixel(10, 10, 255, 10, 3, 255);
        pixels.set_pixel(319, 199, 25, 10, 3, 255);

        let px = pixels.read_pixel(10, 10);
        assert_eq!(px, (255, 10, 3, 0));

        let px = pixels.read_pixel(319, 199);
        assert_eq!(px, (25, 10, 3, 0));
    }
}
