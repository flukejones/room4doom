//! A generic `PixelBuf` that can be drawn to and is blitted to screen by the game,
//! and a generic `PlayRenderer` for rendering the players view of the level.

pub mod shaders;

use gameplay::{Level, PicData, Player};
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
    fn size(&self) -> &BufferSize;
    fn clear(&mut self);
    fn clear_with_colour(&mut self, colour: &[u8; 4]);
    fn set_pixel(&mut self, x: usize, y: usize, rgba: &[u8; 4]);
    fn read_pixel(&self, x: usize, y: usize) -> [u8; 4];
    fn unsafe_read_pixel(&self, x: usize, y: usize) -> &[u8; 4];
    fn read_pixels(&mut self) -> &mut [u8];
}

pub struct BufferSize {
    width: usize,
    height: usize,
    width_i32: i32,
    height_i32: i32,
    width_f32: f32,
    height_f32: f32,
    half_width: i32,
    half_height: i32,
    half_width_f32: f32,
    half_height_f32: f32,
}

impl BufferSize {
    #[inline]
    pub const fn width(&self) -> i32 {
        self.width_i32
    }
    #[inline]
    pub const fn height(&self) -> i32 {
        self.height_i32
    }
    #[inline]
    pub const fn half_width(&self) -> i32 {
        self.half_width
    }
    #[inline]
    pub const fn half_height(&self) -> i32 {
        self.half_height
    }

    #[inline]
    pub const fn width_usize(&self) -> usize {
        self.width
    }
    #[inline]
    pub const fn height_usize(&self) -> usize {
        self.height
    }

    #[inline]
    pub const fn width_f32(&self) -> f32 {
        self.width_f32
    }
    #[inline]
    pub const fn height_f32(&self) -> f32 {
        self.height_f32
    }
    #[inline]
    pub const fn half_width_f32(&self) -> f32 {
        self.half_width_f32
    }
    #[inline]
    pub const fn half_height_f32(&self) -> f32 {
        self.half_height_f32
    }
}

pub struct Buffer {
    size: BufferSize,
    /// Total length is width * height * CHANNELS, where CHANNELS is RGB bytes
    buffer: Vec<u8>,
    stride: usize,
}

impl Buffer {
    fn new(width: usize, height: usize) -> Self {
        Self {
            size: BufferSize {
                width,
                height,
                width_i32: width as i32,
                height_i32: height as i32,
                half_width: width as i32 / 2,
                half_height: height as i32 / 2,
                width_f32: width as f32,
                height_f32: height as f32,
                half_width_f32: width as f32 / 2.0,
                half_height_f32: height as f32 / 2.0,
            },
            buffer: vec![0; (width * height) * CHANNELS],
            stride: width * CHANNELS,
        }
    }
}

impl PixelBuffer for Buffer {
    #[inline]
    fn size(&self) -> &BufferSize {
        &self.size
    }

    #[inline]
    fn clear(&mut self) {
        self.buffer
            .chunks_mut(4)
            .for_each(|n| n.copy_from_slice(&[0, 0, 0, 255]));
    }

    #[inline]
    fn clear_with_colour(&mut self, colour: &[u8; 4]) {
        self.buffer
            .chunks_mut(4)
            .for_each(|n| n.copy_from_slice(colour));
    }

    #[inline]
    fn set_pixel(&mut self, x: usize, y: usize, rgba: &[u8; 4]) {
        // Shitty safeguard. Need to find actual cause of fail
        if x >= self.size.width || y >= self.size.height {
            return;
        }

        let pos = y * self.stride + x * CHANNELS;
        self.buffer[pos..pos + 4].copy_from_slice(rgba);
    }

    /// Read the colour of a single pixel at X|Y
    #[inline]
    fn read_pixel(&self, x: usize, y: usize) -> [u8; 4] {
        let pos = y * self.stride + x * CHANNELS;
        let mut slice = [0u8; 4];
        slice.copy_from_slice(&self.buffer[pos..pos + 4]);
        slice
    }

    #[inline]
    fn unsafe_read_pixel(&self, x: usize, y: usize) -> &[u8; 4] {
        let pos = y * self.stride + x * CHANNELS;
        unsafe { &*(self.buffer[pos..pos + 4].as_ptr() as *const [u8; 4]) }
    }

    /// Read the full buffer
    #[inline]
    fn read_pixels(&mut self) -> &mut [u8] {
        &mut self.buffer
    }
}

/// A structure holding display data
pub struct SoftFramebuffer {
    crop_rect: Rect,
    tex_creator: TextureCreator<WindowContext>,
}

impl SoftFramebuffer {
    fn new(canvas: &Canvas<Window>) -> Self {
        let wsize = canvas.window().drawable_size();
        // let ratio = wsize.1 as f32 * 1.333;
        // let xp = (wsize.0 as f32 - ratio) / 2.0;

        let tex_creator = canvas.texture_creator();
        Self {
            // crop_rect: Rect::new(xp as i32, 0, ratio as u32, wsize.1),
            crop_rect: Rect::new(0, 0, wsize.0, wsize.1),
            tex_creator,
        }
    }
}

/// A structure holding display data
pub struct SoftOpenGL {
    gl_texture: Texture,
    screen_shader: Box<dyn ShaderDraw>,
}

impl SoftOpenGL {
    fn new(width: usize, height: usize, gl_ctx: &Context, screen_shader: Shaders) -> Self {
        let mut gl_texture = Texture::new(gl_ctx).unwrap();
        gl_texture.set_image(None, width as u32, height as u32, golem::ColorFormat::RGBA);

        Self {
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

    pub fn copy_softbuf_to_gl_texture(&mut self, buffer: &Buffer) {
        self.gl_texture.set_image(
            Some(&buffer.buffer),
            buffer.size.width as u32,
            buffer.size.height as u32,
            ColorFormat::RGBA,
        );
    }
}

/// A structure holding display data
pub struct RenderTarget {
    width: usize,
    height: usize,
    render_type: RenderType,
    buffer: Buffer,
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
            buffer: Buffer::new(width, height),
            software: None,
            soft_opengl: None,
        }
    }

    // TODO: should we return the pixelbuffer directly?
    pub fn pixel_buffer(&mut self) -> &mut dyn PixelBuffer {
        &mut self.buffer
    }

    pub fn with_software(mut self, canvas: &Canvas<Window>) -> Self {
        if self.soft_opengl.is_some() {
            panic!("Rendering already set up for software-opengl");
        }
        self.software = Some(SoftFramebuffer::new(canvas));
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
                ogl.copy_softbuf_to_gl_texture(&self.buffer);
                ogl.screen_shader.draw(&ogl.gl_texture).unwrap();
                sdl_canvas.window().gl_swap_window();
            }
            RenderType::Software => {
                let w = self.width() as u32;
                let h = self.height() as u32;
                let render_buffer = unsafe { self.software.as_mut().unwrap_unchecked() };
                let texc = &render_buffer.tex_creator;
                let surf = surface::Surface::from_data(
                    &mut self.buffer.buffer,
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
    fn render_player_view(
        &mut self,
        player: &Player,
        level: &Level,
        pic_data: &mut PicData,
        buf: &mut RenderTarget,
    );
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
