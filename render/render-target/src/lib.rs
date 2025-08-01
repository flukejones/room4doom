#[cfg(feature = "hprof")]
use coarse_prof::profile;

pub mod wipe;

use gameplay::{Level, PicData, Player};
use render_trait::{BufferSize, GameRenderer};
use sdl2::rect::Rect;
use sdl2::render::{Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};
use software3d::Software3D;
use software25d::Software25D;
use wipe::Wipe;

/// channels should match pixel format
const SOFT_PIXEL_CHANNELS: usize = 4;

#[derive(Debug, Default, PartialEq, PartialOrd, Clone, Copy)]
pub enum RenderType {
    /// Purely software. Typically used with blitting a framebuffer maintained
    /// in memory directly to screen using SDL2
    #[default]
    Software,
    /// Fully 3D software rendering.
    Software3D,
}

/// A structure holding display data
pub struct RenderTarget {
    /// Classic 2.5D rendering, draws to a framebuffer
    software: Software25D,
    /// Full 3D software rendering, draws to a framebuffer
    software3d: Software3D,
    framebuffer: FrameBuffer,
    api_type: RenderType,
}

impl RenderTarget {
    pub fn new(
        double: bool,
        debug: bool,
        canvas: Canvas<Window>,
        render_type: RenderType,
    ) -> RenderTarget {
        let render_target = match render_type {
            RenderType::Software => {
                let mut r = RenderTarget::build_soft(double, debug, canvas);
                r.api_type = RenderType::Software;
                r
            }
            RenderType::Software3D => {
                let mut r = RenderTarget::build_soft(double, debug, canvas);
                r.software3d = Software3D::new(
                    r.software.buf_width as f32,
                    r.software.buf_height as f32,
                    90.0_f32.to_radians(), // TODO: get from config
                );
                r.api_type = RenderType::Software3D;
                r
            }
        };

        render_target
    }

    pub fn resize(self, double: bool, debug: bool, render_type: RenderType) -> Self {
        let canvas = self.framebuffer.canvas;
        Self::new(double, debug, canvas, render_type)
    }

    fn build_soft(double: bool, debug: bool, canvas: Canvas<Window>) -> Self {
        let size = canvas.window().size();
        let soft = Software25D::new(
            90f32.to_radians(),
            size.0 as f32,
            size.1 as f32,
            double,
            debug,
        );
        let width = soft.buf_width;
        let height = soft.buf_height;

        let wsize = canvas.window().drawable_size();
        let texture_creator = canvas.texture_creator();
        let texture = texture_creator
            .create_texture_streaming(
                Some(sdl2::pixels::PixelFormatEnum::RGBA32),
                width as u32,
                height as u32,
            )
            .unwrap();

        Self {
            framebuffer: FrameBuffer {
                wipe: Wipe::new(width as i32, height as i32),
                buffer1: DrawBuffer::new(width, height),
                buffer2: DrawBuffer::new(width, height),
                // crop_rect: Rect::new(xp as i32, 0, ratio as u32, wsize.1),
                crop_rect: Rect::new(0, 0, wsize.0, wsize.1),
                _tc: texture_creator,
                texture,
                canvas,
            },
            software: soft,
            software3d: Software3D::new(
                width as f32,
                height as f32,
                90.0_f32.to_radians(), // TODO: get from config
            ),
            api_type: RenderType::Software3D,
        }
    }
}

impl GameRenderer for RenderTarget {
    fn render_player_view(&mut self, player: &Player, level: &mut Level, pic_data: &mut PicData) {
        let r = &mut self.framebuffer;
        match self.api_type {
            RenderType::Software => self.software.draw_view(player, level, pic_data, r),
            RenderType::Software3D => self.software3d.draw_view(player, level, pic_data, r),
        }
    }

    fn frame_buffer(&mut self) -> &mut impl render_trait::DrawBuffer {
        &mut self.framebuffer
    }

    fn flip_and_present(&mut self) {
        self.framebuffer.flip();
        self.framebuffer.blit();
    }

    fn flip(&mut self) {
        self.framebuffer.flip();
    }

    fn do_wipe(&mut self) -> bool {
        self.framebuffer.do_wipe()
    }

    fn buffer_size(&self) -> &BufferSize {
        &self.framebuffer.buffer2.size
    }
}

struct DrawBuffer {
    size: BufferSize,
    /// Total length is width * height * CHANNELS, where CHANNELS is RGB bytes
    buffer: Vec<u8>,
    stride: usize,
}

impl DrawBuffer {
    fn new(width: usize, height: usize) -> Self {
        Self {
            size: BufferSize::new(width, height),
            buffer: vec![0; (width * height) * SOFT_PIXEL_CHANNELS + SOFT_PIXEL_CHANNELS],
            stride: width * SOFT_PIXEL_CHANNELS,
        }
    }
}

impl DrawBuffer {
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

    #[inline(always)]
    pub fn pitch(&self) -> usize {
        self.size.width_usize() * SOFT_PIXEL_CHANNELS
    }

    #[inline(always)]
    pub fn get_buf_index(&self, x: usize, y: usize) -> usize {
        y * self.size.width_usize() * SOFT_PIXEL_CHANNELS + x * SOFT_PIXEL_CHANNELS
    }
}

pub struct FrameBuffer {
    wipe: Wipe,
    buffer1: DrawBuffer,
    buffer2: DrawBuffer,
    crop_rect: Rect,
    _tc: TextureCreator<WindowContext>,
    texture: sdl2::render::Texture,
    canvas: Canvas<Window>,
}

impl FrameBuffer {
    fn flip(&mut self) {
        std::mem::swap(&mut self.buffer1, &mut self.buffer2);
    }

    /// Throw buffer1 at the screen
    fn blit(&mut self) {
        self.texture
            .update(None, &self.buffer1.buffer, self.buffer1.stride)
            .unwrap();
        self.canvas
            .copy(&self.texture, None, Some(self.crop_rect))
            .unwrap();
        self.canvas.present();
    }

    /// Must do a blit after to show the results
    fn do_wipe(&mut self) -> bool {
        let done = self
            .wipe
            .do_melt_pixels(&mut self.buffer1, &mut self.buffer2);
        if done {
            self.wipe.reset();
        }
        done
    }
}

impl render_trait::DrawBuffer for FrameBuffer {
    /// Really only used by seg drawing in plain renderer to draw chunks
    fn buf_mut(&mut self) -> &mut [u8] {
        &mut self.buffer2.buffer
    }

    fn get_buf_index(&self, x: usize, y: usize) -> usize {
        self.buffer2.get_buf_index(x, y)
    }

    fn pitch(&self) -> usize {
        self.buffer2.pitch()
    }

    fn size(&self) -> &BufferSize {
        &self.buffer2.size
    }

    fn set_pixel(&mut self, x: usize, y: usize, colour: &[u8; 4]) {
        self.buffer2.set_pixel(x, y, colour);
    }

    fn read_pixel(&self, x: usize, y: usize) -> [u8; SOFT_PIXEL_CHANNELS] {
        self.buffer2.read_pixel(x, y)
    }
}
