//! A generic `PixelBuf` that can be drawn to and is blitted to screen by the
//! game, and a generic `PlayRenderer` for rendering the players view of the
//! level.

pub mod shaders;
pub mod wipe;

use gameplay::{Level, PicData, Player};
use golem::{ColorFormat, Context, GolemError, Texture, TextureFilter};
use sdl2::rect::Rect;
use sdl2::render::{Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};
use shaders::basic::Basic;
use shaders::cgwg_crt::Cgwgcrt;
use shaders::lottes_crt::LottesCRT;
use shaders::{ShaderDraw, Shaders};
use wipe::Wipe;

/// channels should match pixel format
const SOFT_PIXEL_CHANNELS: usize = 4;

#[derive(Debug, Default, PartialEq, PartialOrd, Clone, Copy)]
pub enum RenderType {
    /// Purely software. Typically used with blitting a framebuffer maintained
    /// in memory directly to screen using SDL2
    #[default]
    Software,
    /// Software framebuffer blitted to screen using OpenGL (and can use
    /// shaders)
    SoftOpenGL,
    /// OpenGL
    OpenGL,
    /// Vulkan
    Vulkan,
}

pub trait PixelBuffer {
    fn size(&self) -> &BufferSize;
    fn clear(&mut self);
    fn clear_with_colour(&mut self, colour: &[u8; SOFT_PIXEL_CHANNELS]);
    fn set_pixel(&mut self, x: usize, y: usize, colour: &[u8; SOFT_PIXEL_CHANNELS]);
    fn read_pixel(&self, x: usize, y: usize) -> [u8; SOFT_PIXEL_CHANNELS];
    fn buf_mut(&mut self) -> &mut [u8];
    /// The pitch that should be added/subtracted to go up or down the Y while
    /// keeping X position
    fn pitch(&self) -> usize;
    /// Amount of colour channels, e.g: [R, G, B] == 3
    fn channels(&self) -> usize;
    /// Get an index point for this coord to copy a colour array too
    fn get_buf_index(&self, x: usize, y: usize) -> usize;
}

pub struct BufferSize {
    hi_res: bool,
    width_usize: usize,
    height_usize: usize,
    width: i32,
    height: i32,
    width_f32: f32,
    height_f32: f32,
    half_width: i32,
    half_height: i32,
    half_width_f32: f32,
    half_height_f32: f32,
}

impl BufferSize {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            hi_res: height > 200,
            width_usize: width,
            height_usize: height,
            width: width as i32,
            height: height as i32,
            half_width: width as i32 / 2,
            half_height: height as i32 / 2,
            width_f32: width as f32,
            height_f32: height as f32,
            half_width_f32: width as f32 / 2.0,
            half_height_f32: height as f32 / 2.0,
        }
    }

    pub const fn hi_res(&self) -> bool {
        self.hi_res
    }

    pub const fn width(&self) -> i32 {
        self.width
    }

    pub const fn height(&self) -> i32 {
        self.height
    }

    pub const fn half_width(&self) -> i32 {
        self.half_width
    }

    pub const fn half_height(&self) -> i32 {
        self.half_height
    }

    pub const fn width_usize(&self) -> usize {
        self.width_usize
    }

    pub const fn height_usize(&self) -> usize {
        self.height_usize
    }

    pub const fn width_f32(&self) -> f32 {
        self.width_f32
    }

    pub const fn height_f32(&self) -> f32 {
        self.height_f32
    }

    pub const fn half_width_f32(&self) -> f32 {
        self.half_width_f32
    }

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
            size: BufferSize::new(width, height),
            buffer: vec![0; (width * height) * SOFT_PIXEL_CHANNELS],
            stride: width * SOFT_PIXEL_CHANNELS,
        }
    }
}

impl PixelBuffer for Buffer {
    fn size(&self) -> &BufferSize {
        &self.size
    }

    fn clear(&mut self) {
        self.buffer
            .chunks_mut(4)
            .for_each(|n| n.copy_from_slice(&[0, 0, 0, 255]));
    }

    fn clear_with_colour(&mut self, colour: &[u8; SOFT_PIXEL_CHANNELS]) {
        self.buffer
            .chunks_mut(4)
            .for_each(|n| n.copy_from_slice(colour));
    }

    fn set_pixel(&mut self, x: usize, y: usize, colour: &[u8; SOFT_PIXEL_CHANNELS]) {
        // Shitty safeguard. Need to find actual cause of fail
        #[cfg(feature = "safety_check")]
        if x >= self.size.width || y >= self.size.height {
            dbg!(x, y);
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

    /// Read the colour of a single pixel at X|Y
    fn read_pixel(&self, x: usize, y: usize) -> [u8; SOFT_PIXEL_CHANNELS] {
        let pos = y * self.stride + x * SOFT_PIXEL_CHANNELS;
        let mut slice = [0u8; SOFT_PIXEL_CHANNELS];
        slice.copy_from_slice(&self.buffer[pos..pos + SOFT_PIXEL_CHANNELS]);
        slice
    }

    /// Read the full buffer
    fn buf_mut(&mut self) -> &mut [u8] {
        &mut self.buffer
    }

    fn pitch(&self) -> usize {
        self.size().width_usize() * SOFT_PIXEL_CHANNELS
    }

    fn channels(&self) -> usize {
        SOFT_PIXEL_CHANNELS
    }

    fn get_buf_index(&self, x: usize, y: usize) -> usize {
        y * self.size().width_usize() * SOFT_PIXEL_CHANNELS + x * SOFT_PIXEL_CHANNELS
    }
}

/// A structure holding display data
pub struct SoftFramebuffer {
    crop_rect: Rect,
    _tc: TextureCreator<WindowContext>,
    texture: sdl2::render::Texture,
}

impl SoftFramebuffer {
    fn new(canvas: &Canvas<Window>, r_width: u32, r_height: u32) -> Self {
        let wsize = canvas.window().drawable_size();
        // let ratio = wsize.1 as f32 * 1.333;
        // let xp = (wsize.0 as f32 - ratio) / 2.0;
        let texture_creator = canvas.texture_creator();
        let texture = texture_creator
            .create_texture_target(
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

/// A structure holding display data
pub struct SoftOpenGL {
    gl_texture: Texture,
    screen_shader: Box<dyn ShaderDraw>,
}

impl SoftOpenGL {
    fn new(width: usize, height: usize, gl_ctx: &Context, screen_shader: Shaders) -> Self {
        let mut gl_texture = Texture::new(gl_ctx).unwrap();
        gl_texture.set_image(None, width as u32, height as u32, ColorFormat::RGB);

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
            buffer.size.width_usize as u32,
            buffer.size.height_usize as u32,
            ColorFormat::RGBA,
        );
    }
}

/// A structure holding display data
pub struct RenderTarget {
    wipe: Wipe,
    width: usize,
    height: usize,
    render_type: RenderType,
    /// Software rendering draws to the software buffer. If OpenGL or Vulkan are
    /// used then the menus and HUD are drawn to this and blitted on top of the
    /// player view
    buffer1: Buffer,
    buffer2: Buffer,
    /// Total length is width * height * CHANNELS, where CHANNELS is RGB bytes
    software: Option<SoftFramebuffer>,
    soft_opengl: Option<SoftOpenGL>,
}

impl RenderTarget {
    pub fn new(
        width: usize,
        height: usize,
        canvas: &Canvas<Window>,
        gl_ctx: &golem::Context,
        render_type: RenderType,
        soft_size: (u32, u32),
        shader: Shaders,
    ) -> RenderTarget {
        let render_target = match render_type {
            RenderType::Software => {
                RenderTarget::build(width, height).with_software(&canvas, soft_size.0, soft_size.1)
            }
            RenderType::SoftOpenGL => {
                RenderTarget::build(width, height).with_gl(&canvas, &gl_ctx, shader)
            }
            RenderType::OpenGL => todo!(),
            RenderType::Vulkan => todo!(),
        };

        render_target
    }

    fn build(width: usize, height: usize) -> Self {
        Self {
            wipe: Wipe::new(width as i32, height as i32),
            width,
            height,
            render_type: RenderType::Software,
            buffer1: Buffer::new(width, height),
            buffer2: Buffer::new(width, height),
            software: None,
            soft_opengl: None,
        }
    }

    /// Get the buffer currently being drawn to
    pub fn draw_buffer(&mut self) -> &mut impl PixelBuffer {
        &mut self.buffer2
    }

    /// Get the buffer that will be blitted to screen
    pub fn blit_buffer(&mut self) -> &mut impl PixelBuffer {
        &mut self.buffer1
    }

    pub fn with_software(mut self, canvas: &Canvas<Window>, r_width: u32, r_height: u32) -> Self {
        if self.soft_opengl.is_some() {
            panic!("Rendering already set up for software-opengl");
        }
        self.software = Some(SoftFramebuffer::new(canvas, r_width, r_height));
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
        let gl = SoftOpenGL::new(self.width, self.height, gl_ctx, screen_shader);
        gl.set_gl_filter().unwrap();
        self.soft_opengl = Some(gl);
        self.render_type = RenderType::SoftOpenGL;

        let wsize = canvas.window().drawable_size();
        // let ratio = wsize.1 as f32 * 1.333;
        // let xp = (wsize.0 as f32 - ratio) / 2.0;
        gl_ctx.set_viewport(0, 0, wsize.0, wsize.1);
        self
    }

    /// Throw buffer1 at the screen
    pub fn blit(&mut self, sdl_canvas: &mut Canvas<Window>) {
        match self.render_type {
            RenderType::SoftOpenGL => {
                let ogl = unsafe { self.soft_opengl.as_mut().unwrap_unchecked() };
                // shader.shader.clear();
                ogl.copy_softbuf_to_gl_texture(&self.buffer1);
                ogl.screen_shader.draw(&ogl.gl_texture).unwrap();
                sdl_canvas.window().gl_swap_window();
            }
            RenderType::Software => {
                let buf = unsafe { self.software.as_mut().unwrap_unchecked() };
                buf.texture
                    .update(None, &self.buffer1.buffer, self.buffer1.stride)
                    .unwrap();
                sdl_canvas
                    .copy(&buf.texture, None, Some(buf.crop_rect))
                    .unwrap();
                sdl_canvas.present();
            }
            RenderType::OpenGL => todo!(),
            RenderType::Vulkan => todo!(),
        }
    }

    pub fn clear_with_debug(&mut self) {
        self.buffer2.clear_with_colour(&[0, 164, 0, 255]);
    }

    pub fn flip(&mut self) {
        std::mem::swap(&mut self.buffer1, &mut self.buffer2);
    }

    /// Must do a blit after to show the results
    pub fn do_wipe(&mut self) -> bool {
        let done = self
            .wipe
            .do_melt_pixels(&mut self.buffer1, &mut self.buffer2);
        if done {
            self.wipe.reset();
        }
        done
    }
}

pub trait PlayViewRenderer {
    /// Drawing the full player view to the `PixelBuf`.
    ///
    /// Doom function name `R_RenderPlayerView`
    fn render(
        self: &mut Self,
        player: &Player,
        level: &Level,
        pic_data: &mut PicData,
        target: &mut dyn PixelBuffer,
    );

    fn width(&self) -> u32;
    fn height(&self) -> u32;
}
