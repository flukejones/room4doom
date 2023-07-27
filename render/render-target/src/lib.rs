//! A generic `PixelBuf` that can be drawn to and is blitted to screen by the game,
//! and a generic `PlayRenderer` for rendering the players view of the level.

pub mod shaders;

use gameplay::{Level, Player};
use golem::{ColorFormat, Context, GolemError, Texture, TextureFilter};
use sdl2::{
    pixels,
    rect::Rect,
    render::{Canvas, TextureCreator},
    surface,
    video::{Window, WindowContext},
};
use shaders::{basic::Basic, cgwg_crt::Cgwgcrt, lottes_crt::LottesCRT, ShaderDraw, Shaders};

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
    crop_rect: Rect,
    tex_creator: TextureCreator<WindowContext>,
}

impl SoftFramebuffer {
    fn new(width: usize, height: usize, canvas: &Canvas<Window>) -> Self {
        let wsize = canvas.window().drawable_size();
        let ratio = wsize.1 as f32 * 1.333;
        let xp = (wsize.0 as f32 - ratio) / 2.0;

        let tex_creator = canvas.texture_creator();
        Self {
            width,
            height,
            buffer: vec![0; (width * height) * CHANNELS],
            crop_rect: Rect::new(xp as i32, 0, ratio as u32, wsize.1),
            tex_creator,
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
    buffer: Vec<u8>,
    gl_texture: Texture,
    screen_shader: Box<dyn ShaderDraw>,
}

impl SoftOpenGL {
    fn new(width: usize, height: usize, gl_ctx: &Context, screen_shader: Shaders) -> Self {
        let mut gl_texture = Texture::new(gl_ctx).unwrap();
        gl_texture.set_image(None, width as u32, height as u32, golem::ColorFormat::RGB);

        Self {
            width,
            height,
            buffer: vec![0; (width * height) * CHANNELS],
            gl_texture,
            screen_shader: match screen_shader {
                Shaders::Basic => Box::new(Basic::new(gl_ctx)),
                Shaders::Lottes => Box::new(LottesCRT::new(gl_ctx)),
                Shaders::LottesBasic => Box::new(shaders::lottes_reduced::LottesCRT::new(gl_ctx)),
                Shaders::Cgwg => Box::new(Cgwgcrt::new(gl_ctx, width as u32, height as u32)),
            },
        }
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
            Some(&self.buffer),
            self.width as u32,
            self.height as u32,
            ColorFormat::RGBA,
        );
    }
}

impl PixelBuffer for SoftOpenGL {
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
pub struct RenderTarget {
    width: usize,
    height: usize,
    render_type: RenderType,
    /// Total length is width * height * CHANNELS, where CHANNELS is RGB bytes
    software: Option<SoftFramebuffer>,
    soft_opengl: Option<SoftOpenGL>,
}

impl RenderTarget {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            render_type: RenderType::Software,
            software: None,
            soft_opengl: None,
        }
    }

    pub fn with_software(mut self, canvas: &Canvas<Window>) -> Self {
        if self.soft_opengl.is_some() {
            panic!("Rendering already set up for software-opengl");
        }
        self.software = Some(SoftFramebuffer::new(self.width, self.height, canvas));
        self.render_type = RenderType::Software;
        self
    }

    pub fn with_gl(
        mut self,
        canvas: &Canvas<Window>,
        gl_ctx: &Context,
        screen_shader: Shaders,
    ) -> Self {
        if self.software.is_some() {
            panic!("Rendering already set up for software");
        }
        self.soft_opengl = Some(SoftOpenGL::new(
            self.width,
            self.height,
            gl_ctx,
            screen_shader,
        ));
        self.render_type = RenderType::SoftOpenGL;

        let wsize = canvas.window().drawable_size();
        let ratio = wsize.1 as f32 * 1.333;
        let xp = (wsize.0 as f32 - ratio) / 2.0;

        gl_ctx.set_viewport(xp as u32, 0, ratio as u32, wsize.1);
        self
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

    pub fn blit(&mut self, sdl_canvas: &mut Canvas<Window>) {
        match self.render_type {
            RenderType::SoftOpenGL => {
                let ogl = unsafe { self.soft_opengl.as_mut().unwrap_unchecked() };
                // shader.shader.clear();
                ogl.copy_softbuf_to_gl_texture();
                ogl.screen_shader.draw(&ogl.gl_texture).unwrap();
                sdl_canvas.window().gl_swap_window();
            }
            RenderType::Software => {
                let w = self.width() as u32;
                let h = self.height() as u32;
                let render_buffer = unsafe { self.software.as_mut().unwrap_unchecked() };
                let texc = &render_buffer.tex_creator;
                let surf = surface::Surface::from_data(
                    &mut render_buffer.buffer,
                    w,
                    h,
                    4 * w,
                    pixels::PixelFormatEnum::RGBA32,
                )
                .unwrap()
                .as_texture(texc)
                .unwrap();
                sdl_canvas
                    .copy(&surf, None, Some(render_buffer.crop_rect))
                    .unwrap();
                sdl_canvas.present();
            }
            RenderType::OpenGL => todo!(),
            RenderType::Vulkan => todo!(),
        }
    }
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
