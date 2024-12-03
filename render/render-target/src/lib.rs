//! A generic `PixelBuf` that can be drawn to and is blitted to screen by the
//! game, and a generic `PlayRenderer` for rendering the players view of the
//! level.

pub mod shaders;
pub mod wipe;

use gameplay::{Level, PicData, Player};
use golem::{ColorFormat, Context, GolemError, Texture, TextureFilter};
use render_soft::SoftwareRenderer;
use render_trait::{BufferSize, PixelBuffer, PlayViewRenderer, RenderTrait};
use sdl2::rect::Rect;
use sdl2::render::{Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};
use shaders::basic::Basic;
use shaders::lottes_crt::LottesCRT;
use shaders::{ShaderDraw, Shaders};
use wipe::Wipe;

/// channels should match pixel format
const SOFT_PIXEL_CHANNELS: usize = 4;

#[derive(Debug, Default, PartialEq, PartialOrd, Clone, Copy)]
pub enum RenderApiType {
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

struct Buffer {
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
    #[inline(always)]
    fn size(&self) -> &BufferSize {
        &self.size
    }

    #[inline(always)]
    fn clear(&mut self) {
        self.buffer
            .chunks_mut(4)
            .for_each(|n| n.copy_from_slice(&[0, 0, 0, 255]));
    }

    #[inline(always)]
    fn clear_with_colour(&mut self, colour: &[u8; SOFT_PIXEL_CHANNELS]) {
        self.buffer
            .chunks_mut(4)
            .for_each(|n| n.copy_from_slice(colour));
    }

    #[inline(always)]
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
    #[inline]
    fn read_pixel(&self, x: usize, y: usize) -> [u8; SOFT_PIXEL_CHANNELS] {
        let pos = y * self.stride + x * SOFT_PIXEL_CHANNELS;
        let mut slice = [0u8; SOFT_PIXEL_CHANNELS];
        slice.copy_from_slice(&self.buffer[pos..pos + SOFT_PIXEL_CHANNELS]);
        slice
    }

    /// Read the full buffer
    #[inline(always)]
    fn buf_mut(&mut self) -> &mut [u8] {
        &mut self.buffer
    }

    #[inline(always)]
    fn pitch(&self) -> usize {
        self.size.width_usize() * SOFT_PIXEL_CHANNELS
    }

    #[inline(always)]
    fn channels(&self) -> usize {
        SOFT_PIXEL_CHANNELS
    }

    #[inline(always)]
    fn get_buf_index(&self, x: usize, y: usize) -> usize {
        y * self.size.width_usize() * SOFT_PIXEL_CHANNELS + x * SOFT_PIXEL_CHANNELS
    }
}

/// A structure holding display data
struct SoftFramebuffer {
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

/// A structure holding display data
struct SoftGLBuffer {
    gl_texture: Texture,
    screen_shader: Box<dyn ShaderDraw>,
}

impl SoftGLBuffer {
    fn new(width: usize, height: usize, gl_ctx: &Context, screen_shader: Shaders) -> Self {
        let mut gl_texture = Texture::new(gl_ctx).unwrap();
        gl_texture.set_image(None, width as u32, height as u32, ColorFormat::RGB);

        Self {
            gl_texture,
            screen_shader: match screen_shader {
                Shaders::Basic => Box::new(Basic::new(gl_ctx)),
                Shaders::Lottes => Box::new(LottesCRT::new(gl_ctx)),
                Shaders::LottesBasic => Box::new(shaders::lottes_reduced::LottesCRT::new(gl_ctx)),
            },
        }
    }

    #[inline]
    fn set_gl_filter(&self) -> Result<(), GolemError> {
        self.gl_texture.set_minification(TextureFilter::Linear)?;
        self.gl_texture.set_magnification(TextureFilter::Linear)
    }

    #[inline]
    fn copy_softbuf_to_gl_texture(&mut self, buffer: &Buffer) {
        self.gl_texture.set_image(
            Some(&buffer.buffer),
            buffer.size.width() as u32,
            buffer.size.height() as u32,
            ColorFormat::RGBA,
        );
    }
}

/// A structure holding display data
pub struct RenderTarget {
    width: usize,
    height: usize,
    renderer: SoftwareRenderer,
    pub framebuffer: FrameBuffer,
}

impl RenderTarget {
    pub fn new(
        double: bool,
        debug: bool,
        canvas: Canvas<Window>,
        gl_ctx: &golem::Context,
        render_type: RenderApiType,
        shader: Shaders,
    ) -> RenderTarget {
        let render_target = match render_type {
            RenderApiType::Software => {
                let mut r = RenderTarget::build_soft(double, debug, canvas);
                if r.framebuffer.soft_opengl.is_some() {
                    panic!("Rendering already set up for software-opengl");
                }
                let width = r.renderer.buf_width;
                let height = r.renderer.buf_height;
                r.framebuffer.software = Some(SoftFramebuffer::new(
                    &r.framebuffer.canvas,
                    width as u32,
                    height as u32,
                ));
                r.framebuffer.api_type = RenderApiType::Software;
                r
            }
            RenderApiType::SoftOpenGL => {
                let wsize = canvas.window().drawable_size();
                let mut r = RenderTarget::build_soft(double, debug, canvas);
                if r.framebuffer.software.is_some() {
                    panic!("Rendering already set up for software");
                }
                let gl = SoftGLBuffer::new(r.width, r.height, gl_ctx, shader);
                gl.set_gl_filter().unwrap();
                r.framebuffer.soft_opengl = Some(gl);
                r.framebuffer.api_type = RenderApiType::SoftOpenGL;
                // let ratio = wsize.1 as f32 * 1.333;
                // let xp = (wsize.0 as f32 - ratio) / 2.0;
                gl_ctx.set_viewport(0, 0, wsize.0, wsize.1);
                r
            }
            RenderApiType::OpenGL => todo!(),
            RenderApiType::Vulkan => todo!(),
        };

        render_target
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
                api_type: RenderApiType::Software,
                buffer1: Buffer::new(width, height),
                buffer2: Buffer::new(width, height),
                software: None,
                soft_opengl: None,
                canvas,
            },
            width,
            height,
            renderer: soft,
        }
    }
}

impl PlayViewRenderer for RenderTarget {
    fn render_player_view(&mut self, player: &Player, level: &Level, pic_data: &mut PicData) {
        let r = &mut self.framebuffer;
        match r.api_type {
            RenderApiType::Software | RenderApiType::SoftOpenGL => {
                self.renderer.render_player_view(player, level, pic_data, r)
            }
            RenderApiType::OpenGL => todo!(),
            RenderApiType::Vulkan => todo!(),
        }
    }
}

impl RenderTrait for RenderTarget {
    fn draw_buffer(&mut self) -> &mut impl PixelBuffer {
        self.framebuffer.draw_buffer()
    }

    fn blit_buffer(&mut self) -> &mut impl PixelBuffer {
        self.framebuffer.blit_buffer()
    }

    fn blit(&mut self) {
        self.framebuffer.blit();
    }

    fn debug_blit_draw_buffer(&mut self) {
        self.framebuffer.debug_blit_draw_buffer();
    }

    fn debug_clear(&mut self) {
        self.framebuffer.debug_clear();
    }

    fn clear(&mut self) {
        self.framebuffer.clear();
    }

    fn flip(&mut self) {
        self.framebuffer.flip();
    }

    fn do_wipe(&mut self) -> bool {
        self.framebuffer.do_wipe()
    }
}

pub struct FrameBuffer {
    wipe: Wipe,
    api_type: RenderApiType,
    /// Software rendering draws to the software buffer. If OpenGL or Vulkan are
    /// used then the menus and HUD are drawn to this and blitted on top of the
    /// player view
    buffer1: Buffer,
    buffer2: Buffer,
    /// Total length is width * height * CHANNELS, where CHANNELS is RGB bytes
    software: Option<SoftFramebuffer>,
    soft_opengl: Option<SoftGLBuffer>,
    pub canvas: Canvas<Window>,
}

impl RenderTrait for FrameBuffer {
    /// Get the buffer currently being drawn to
    fn draw_buffer(&mut self) -> &mut impl PixelBuffer {
        &mut self.buffer2
    }

    /// Get the buffer that will be blitted to screen
    fn blit_buffer(&mut self) -> &mut impl PixelBuffer {
        &mut self.buffer1
    }

    /// Throw buffer1 at the screen
    fn blit(&mut self) {
        match self.api_type {
            RenderApiType::SoftOpenGL => {
                let ogl = unsafe { self.soft_opengl.as_mut().unwrap_unchecked() };
                // shader.shader.clear();
                ogl.copy_softbuf_to_gl_texture(&self.buffer1);
                ogl.screen_shader.draw(&ogl.gl_texture).unwrap();
                self.canvas.window().gl_swap_window();
            }
            RenderApiType::Software => {
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

    /// for debug
    fn debug_blit_draw_buffer(&mut self) {
        match self.api_type {
            RenderApiType::SoftOpenGL => {
                let ogl = unsafe { self.soft_opengl.as_mut().unwrap_unchecked() };
                // shader.shader.clear();
                ogl.copy_softbuf_to_gl_texture(&self.buffer2);
                ogl.screen_shader.draw(&ogl.gl_texture).unwrap();
                self.canvas.window().gl_swap_window();
            }
            RenderApiType::Software => {
                let buf = unsafe { self.software.as_mut().unwrap_unchecked() };
                buf.texture
                    .update(None, &self.buffer2.buffer, self.buffer2.stride)
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

    fn debug_clear(&mut self) {
        self.buffer2.clear_with_colour(&[0, 164, 0, 255]);
    }

    fn clear(&mut self) {
        self.buffer2.clear_with_colour(&[0, 0, 0, 255]);
    }

    fn flip(&mut self) {
        std::mem::swap(&mut self.buffer1, &mut self.buffer2);
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
