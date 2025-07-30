//! A generic `PixelBuf` that can be drawn to and is blitted to screen by the
//! game, and a generic `PlayRenderer` for rendering the players view of the
//! level.

#[cfg(feature = "hprof")]
use coarse_prof::profile;

mod buffers;
pub mod wipe;

use buffers::DrawBuffer;
use gameplay::{Level, PicData, Player};
use render_trait::{BufferSize, GameRenderer};
use sdl2::render::Canvas;
use sdl2::video::Window;
use software3d::Software3D;
use software25d::SoftwareRenderer;
use wipe::Wipe;

use crate::buffers::SdlBuffer;

/// channels should match pixel format
const SOFT_PIXEL_CHANNELS: usize = 4;

#[derive(Debug, Default, PartialEq, PartialOrd, Clone, Copy)]
pub enum RenderApiType {
    /// Purely software. Typically used with blitting a framebuffer maintained
    /// in memory directly to screen using SDL2
    #[default]
    Software,
    /// Fully 3D software rendering.
    Software3D,
    /// OpenGL
    OpenGL,
    /// Vulkan
    Vulkan,
}

/// A structure holding display data
pub struct RenderTarget {
    software: SoftwareRenderer,
    software3d: Software3D,
    framebuffer: FrameBuffer,
}

impl RenderTarget {
    pub fn new(
        double: bool,
        debug: bool,
        canvas: Canvas<Window>,
        render_type: RenderApiType,
    ) -> RenderTarget {
        let render_target = match render_type {
            RenderApiType::Software => {
                let mut r = RenderTarget::build_soft(double, debug, canvas);
                let width = r.software.buf_width;
                let height = r.software.buf_height;
                r.framebuffer.software = Some(SdlBuffer::new(
                    &r.framebuffer.canvas,
                    width as u32,
                    height as u32,
                ));
                r.framebuffer.api_type = RenderApiType::Software;
                r
            }
            RenderApiType::Software3D => {
                let mut r = RenderTarget::build_soft(double, debug, canvas);
                let width = r.software.buf_width;
                let height = r.software.buf_height;
                r.software3d = Software3D::new(
                    width as f32,
                    height as f32,
                    90.0_f32.to_radians(), // TODO: get from config
                );
                r.framebuffer.software = Some(SdlBuffer::new(
                    &r.framebuffer.canvas,
                    width as u32,
                    height as u32,
                ));
                r.framebuffer.api_type = RenderApiType::Software3D;
                r
            }
            RenderApiType::OpenGL => todo!(),
            RenderApiType::Vulkan => todo!(),
        };

        render_target
    }

    pub fn resize(self, double: bool, debug: bool, render_type: RenderApiType) -> Self {
        let canvas = self.framebuffer.canvas;
        Self::new(double, debug, canvas, render_type)
    }

    fn build_soft(double: bool, debug: bool, canvas: Canvas<Window>) -> Self {
        let size = canvas.window().size();
        let soft = SoftwareRenderer::new(
            90f32.to_radians(),
            size.0 as f32,
            size.1 as f32,
            double,
            debug,
        );
        let width = soft.buf_width;
        let height = soft.buf_height;

        Self {
            framebuffer: FrameBuffer {
                wipe: Wipe::new(width as i32, height as i32),
                api_type: RenderApiType::Software3D,
                buffer1: DrawBuffer::new(width, height),
                buffer2: DrawBuffer::new(width, height),
                software: None,
                canvas,
            },
            software: soft,
            software3d: Software3D::new(
                width as f32,
                height as f32,
                90.0_f32.to_radians(), // TODO: get from config
            ),
        }
    }
}

impl GameRenderer for RenderTarget {
    fn render_player_view(&mut self, player: &Player, level: &mut Level, pic_data: &mut PicData) {
        let r = &mut self.framebuffer;
        match r.api_type {
            RenderApiType::Software => self.software.render_player_view(player, level, pic_data, r),
            RenderApiType::Software3D => {
                self.software3d
                    .render_player_view(player, level, pic_data, r);
            }
            RenderApiType::OpenGL => todo!(),
            RenderApiType::Vulkan => todo!(),
        }
    }

    fn draw_buffer(&mut self) -> &mut impl render_trait::DrawBuffer {
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

pub struct FrameBuffer {
    wipe: Wipe,
    api_type: RenderApiType,
    /// Software rendering draws to the software buffer. If OpenGL or Vulkan are
    /// used then the menus and HUD are drawn to this and blitted on top of the
    /// player view
    buffer1: DrawBuffer,
    buffer2: DrawBuffer,
    /// Total length is width * height * CHANNELS, where CHANNELS is RGB bytes
    software: Option<SdlBuffer>,
    canvas: Canvas<Window>,
}

impl FrameBuffer {
    fn flip(&mut self) {
        std::mem::swap(&mut self.buffer1, &mut self.buffer2);
    }

    /// Throw buffer1 at the screen
    fn blit(&mut self) {
        match self.api_type {
            RenderApiType::Software | RenderApiType::Software3D => {
                let buf = unsafe { self.software.as_mut().unwrap_unchecked() };
                buf.texture
                    .update(None, &self.buffer1.buffer, self.buffer1.stride)
                    .unwrap();
                self.canvas
                    .copy(&buf.texture, None, Some(buf.crop_rect))
                    .unwrap();
                self.canvas.present();
            }
            RenderApiType::OpenGL => todo!(),
            RenderApiType::Vulkan => todo!(),
        }
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
    fn get_buf_index(&self, x: usize, y: usize) -> usize {
        self.buffer2.get_buf_index(x, y)
    }

    fn pitch(&self) -> usize {
        self.buffer2.pitch()
    }

    /// Really only used by seg drawing in plain renderer to draw chunks
    fn buf_mut(&mut self) -> &mut [u8] {
        &mut self.buffer2.buffer
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

    fn debug_blit_draw_buffer(&mut self) {
        let buf = unsafe { self.software.as_mut().unwrap_unchecked() };
        buf.texture
            .update(None, &self.buffer2.buffer, self.buffer2.stride)
            .unwrap();
        self.canvas
            .copy(&buf.texture, None, Some(buf.crop_rect))
            .unwrap();
        self.canvas.present();
    }
}
