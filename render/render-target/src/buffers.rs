use render_trait::BufferSize;
use sdl2::rect::Rect;
use sdl2::render::{Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};

use crate::SOFT_PIXEL_CHANNELS;

pub(crate) struct DrawBuffer {
    pub(crate) size: BufferSize,
    /// Total length is width * height * CHANNELS, where CHANNELS is RGB bytes
    pub(crate) buffer: Vec<u8>,
    pub(crate) stride: usize,
}

impl DrawBuffer {
    pub(crate) fn new(width: usize, height: usize) -> Self {
        Self {
            size: BufferSize::new(width, height),
            buffer: vec![0; (width * height) * SOFT_PIXEL_CHANNELS + SOFT_PIXEL_CHANNELS],
            stride: width * SOFT_PIXEL_CHANNELS,
        }
    }
}

impl DrawBuffer {
    #[inline(always)]
    fn size(&self) -> &BufferSize {
        &self.size
    }

    #[inline(always)]
    pub fn clear_with_colour(&mut self, colour: &[u8; SOFT_PIXEL_CHANNELS]) {
        self.buffer
            .chunks_mut(4)
            .for_each(|n| n.copy_from_slice(colour));
    }

    /// Read the colour of a single pixel at X|Y
    #[inline]
    pub fn read_pixel(&self, x: usize, y: usize) -> [u8; SOFT_PIXEL_CHANNELS] {
        let pos = y * self.stride + x * SOFT_PIXEL_CHANNELS;
        let mut slice = [0u8; SOFT_PIXEL_CHANNELS];
        let end = pos + SOFT_PIXEL_CHANNELS;
        slice.copy_from_slice(&self.buffer[pos..end]);
        slice
    }

    #[inline(always)]
    pub fn set_pixel(&mut self, x: usize, y: usize, colour: &[u8; SOFT_PIXEL_CHANNELS]) {
        #[cfg(feature = "hprof")]
        profile!("set_pixel");
        // Shitty safeguard. Need to find actual cause of fail
        #[cfg(feature = "safety_check")]
        if x >= self.size.width_usize() || y >= self.size.height_usize() {
            dbg!(x, self.size.width_usize(), y, self.size.height_usize());
            panic!();
        }

        let pos = y * self.stride + x * SOFT_PIXEL_CHANNELS;
        #[cfg(not(feature = "safety_check"))]
        unsafe {
            self.buffer
                .get_unchecked_mut(pos..pos + SOFT_PIXEL_CHANNELS)
                .copy_from_slice(colour);
        }
        #[cfg(feature = "safety_check")]
        self.buffer[pos..pos + SOFT_PIXEL_CHANNELS].copy_from_slice(colour);
    }

    /// Read the full buffer
    #[inline(always)]
    fn buf_mut(&mut self) -> &mut [u8] {
        &mut self.buffer
    }

    #[inline(always)]
    pub fn pitch(&self) -> usize {
        self.size.width_usize() * SOFT_PIXEL_CHANNELS
    }

    #[inline(always)]
    fn channels(&self) -> usize {
        SOFT_PIXEL_CHANNELS
    }

    #[inline(always)]
    pub fn get_buf_index(&self, x: usize, y: usize) -> usize {
        y * self.size.width_usize() * SOFT_PIXEL_CHANNELS + x * SOFT_PIXEL_CHANNELS
    }
}

/// A structure holding display data
pub(crate) struct SdlBuffer {
    pub(crate) crop_rect: Rect,
    _tc: TextureCreator<WindowContext>,
    pub(crate) texture: sdl2::render::Texture,
}

impl SdlBuffer {
    pub(crate) fn new(canvas: &Canvas<Window>, r_width: u32, r_height: u32) -> Self {
        let wsize = canvas.window().drawable_size();
        // let ratio = wsize.1 as f32 * 1.333;
        // let xp = (wsize.0 as f32 - ratio) / 2.0;
        let texture_creator = canvas.texture_creator();
        let texture = texture_creator
            .create_texture_streaming(
                Some(sdl2::pixels::PixelFormatEnum::RGBA32),
                r_width,
                r_height,
            )
            .unwrap();
        Self {
            // crop_rect: Rect::new(xp as i32, 0, ratio as u32, wsize.1),
            crop_rect: Rect::new(0, 0, wsize.0, wsize.1),
            _tc: texture_creator,
            texture,
        }
    }
}
