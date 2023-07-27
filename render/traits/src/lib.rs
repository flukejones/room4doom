//! A generic `PixelBuf` that can be drawn to and is blitted to screen by the game,
//! and a generic `PlayRenderer` for rendering the players view of the level.

use gameplay::{Level, Player};
use golem::{ColorFormat, Context, GolemError, Texture, TextureFilter};

const CHANNELS: usize = 4;

#[derive(Debug, Default, PartialEq, PartialOrd, Clone, Copy)]
pub enum RenderType {
    /// Purely software. Typically used with blitting a framebuffer maintained in memory
    /// directly to screen using SDL2
    #[default]
    Software,
    /// Software framebuffer blitted to screen using OpenGL (and can use shaders)
    SoftOpenGL,
    /// OpenGL
    OpenGL,
    /// Vulkan
    Vulkan,
}

pub trait PixelBuffer {
    fn width(&self) -> usize;
    fn height(&self) -> usize;
    fn clear(&mut self);
    fn set_pixel(&mut self, x: usize, y: usize, rgba: (u8, u8, u8, u8));
    fn read_softbuf_pixel(&self, x: usize, y: usize) -> (u8, u8, u8, u8);
    fn read_softbuf_pixels(&mut self) -> &mut [u8];
}

/// A structure holding display data
pub struct SoftFramebuffer {
    width: usize,
    height: usize,
    /// Total length is width * height * CHANNELS, where CHANNELS is RGB bytes
    buffer: Vec<u8>,
}

impl SoftFramebuffer {
    fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            buffer: vec![0; (width * height) * CHANNELS],
        }
    }
}

impl PixelBuffer for SoftFramebuffer {
    #[inline]
    fn width(&self) -> usize {
        self.width
    }

    #[inline]
    fn height(&self) -> usize {
        self.height
    }

    #[inline]
    fn clear(&mut self) {
        self.buffer.iter_mut().for_each(|n| *n = 0);
    }

    #[inline]
    fn set_pixel(&mut self, x: usize, y: usize, rgba: (u8, u8, u8, u8)) {
        // Shitty safeguard. Need to find actual cause of fail
        if x >= self.width || y >= self.height {
            return;
        }

        let pos = y * (self.width * CHANNELS) + x * CHANNELS;
        self.buffer[pos] = rgba.0;
        self.buffer[pos + 1] = rgba.1;
        self.buffer[pos + 2] = rgba.2;
        self.buffer[pos + 3] = rgba.3;
    }

    /// Read the colour of a single pixel at X|Y
    #[inline]
    fn read_softbuf_pixel(&self, x: usize, y: usize) -> (u8, u8, u8, u8) {
        let pos = y * (self.width * CHANNELS) + x * CHANNELS;
        (
            self.buffer[pos],
            self.buffer[pos + 1],
            self.buffer[pos + 2],
            self.buffer[pos + 3],
        )
    }

    /// Read the full buffer
    #[inline]
    fn read_softbuf_pixels(&mut self) -> &mut [u8] {
        &mut self.buffer
    }
}

/// A structure holding display data
pub struct SoftOpenGL {
    width: usize,
    height: usize,
    frame_buffer: SoftFramebuffer,
    gl_texture: Texture,
}

impl SoftOpenGL {
    fn new(width: usize, height: usize, ctx: &Context) -> Self {
        let mut gl_texture = Texture::new(ctx).unwrap();
        gl_texture.set_image(None, width as u32, height as u32, golem::ColorFormat::RGB);
        Self {
            width,
            height,
            frame_buffer: SoftFramebuffer::new(width, height),
            gl_texture,
        }
    }

    #[inline]
    pub fn frame_buffer(&mut self) -> &mut SoftFramebuffer {
        &mut self.frame_buffer
    }

    #[inline]
    pub const fn gl_texture(&self) -> &Texture {
        &self.gl_texture
    }

    pub fn set_gl_filter(&self) -> Result<(), GolemError> {
        self.gl_texture.set_minification(TextureFilter::Linear)?;
        self.gl_texture.set_magnification(TextureFilter::Linear)
    }

    pub fn copy_softbuf_to_gl_texture(&mut self) {
        self.gl_texture.set_image(
            Some(&self.frame_buffer.buffer),
            self.width as u32,
            self.height as u32,
            ColorFormat::RGBA,
        );
    }
}

impl PixelBuffer for SoftOpenGL {
    #[inline]
    fn width(&self) -> usize {
        self.frame_buffer.width
    }

    #[inline]
    fn height(&self) -> usize {
        self.frame_buffer.height
    }

    #[inline]
    fn clear(&mut self) {
        self.frame_buffer.clear();
    }

    #[inline]
    fn set_pixel(&mut self, x: usize, y: usize, rgba: (u8, u8, u8, u8)) {
        self.frame_buffer.set_pixel(x, y, rgba);
    }

    /// Read the colour of a single pixel at X|Y
    #[inline]
    fn read_softbuf_pixel(&self, x: usize, y: usize) -> (u8, u8, u8, u8) {
        self.frame_buffer.read_softbuf_pixel(x, y)
    }

    /// Read the full buffer
    #[inline]
    fn read_softbuf_pixels(&mut self) -> &mut [u8] {
        &mut self.frame_buffer.buffer
    }
}

/// A structure holding display data
pub struct RenderTarget {
    width: usize,
    height: usize,
    render_type: RenderType,
    /// Total length is width * height * CHANNELS, where CHANNELS is RGB bytes
    software: Option<SoftFramebuffer>,
    soft_opengl: Option<SoftOpenGL>,
}

impl RenderTarget {
    pub fn new(width: usize, height: usize, ctx: &Context, render_type: RenderType) -> Self {
        let mut software = None;
        let mut soft_opengl = None;

        match render_type {
            RenderType::Software => software = Some(SoftFramebuffer::new(width, height)),
            RenderType::SoftOpenGL => soft_opengl = Some(SoftOpenGL::new(width, height, ctx)),
            RenderType::OpenGL => todo!(),
            RenderType::Vulkan => todo!(),
        }

        Self {
            width,
            height,
            render_type,
            software,
            soft_opengl,
        }
    }

    #[inline]
    pub fn width(&self) -> usize {
        self.width
    }

    #[inline]
    pub fn height(&self) -> usize {
        self.height
    }

    #[inline]
    pub fn render_type(&self) -> RenderType {
        self.render_type
    }

    #[inline]
    pub unsafe fn software(&mut self) -> Option<&mut SoftFramebuffer> {
        self.software.as_mut()
    }

    #[inline]
    pub unsafe fn software_unchecked(&mut self) -> &mut SoftFramebuffer {
        self.software.as_mut().unwrap_unchecked()
    }

    #[inline]
    pub unsafe fn soft_opengl(&mut self) -> Option<&mut SoftOpenGL> {
        self.soft_opengl.as_mut()
    }

    #[inline]
    pub unsafe fn soft_opengl_unchecked(&mut self) -> &mut SoftOpenGL {
        self.soft_opengl.as_mut().unwrap_unchecked()
    }

    // // /// Get the array of pixels. The layout of which is [Row<RGBA>]
    // pub fn softbuf_pixels(&self) -> &[u8] {
    //     &self.software
    // }

    // pub fn softbuf_pixels_mut(&mut self) -> &mut [u8] {
    //     &mut self.software
    // }
}

pub trait PlayRenderer {
    /// Drawing the full player view to the `PixelBuf`.
    ///
    /// Doom function name `R_RenderPlayerView`
    fn render_player_view(&mut self, player: &Player, level: &Level, buf: &mut RenderTarget);
}

// TODO: somehow test with gl context
// #[cfg(test)]
// mod tests {
//     use crate::PixelBuf;

//     #[test]
//     fn write_read_pixel() {
//         let mut pixels = PixelBuf::new(320, 200, true);

//         pixels.set_pixel(10, 10, 255, 10, 3, 255);
//         pixels.set_pixel(319, 199, 25, 10, 3, 255);

//         let px = pixels.read_pixel(10, 10);
//         assert_eq!(px, (255, 10, 3, 0));

//         let px = pixels.read_pixel(319, 199);
//         assert_eq!(px, (25, 10, 3, 0));
//     }
// }
