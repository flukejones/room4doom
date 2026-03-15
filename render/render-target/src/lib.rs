#[cfg(feature = "hprof")]
use coarse_prof::profile;

pub mod wipe;

use gameplay::{Level, PicData, Player};
use render_trait::{BufferSize, GameRenderer};
use software3d::{DebugDrawOptions, Software3D};
use software25d::Software25D;
use wipe::Wipe;

#[cfg(feature = "display-sdl2")]
mod sdl2_backend;

#[cfg(feature = "display-softbuffer")]
mod softbuffer_backend;

#[cfg(feature = "display-pixels")]
mod pixels_backend;

#[derive(Debug, Default, PartialEq, PartialOrd, Clone, Copy)]
pub enum RenderType {
    /// Purely software. Typically used with blitting a framebuffer maintained
    /// in memory directly to screen using SDL2
    #[default]
    Software,
    /// Fully 3D software rendering.
    Software3D,
}

pub enum Renderer {
    /// Purely software. Typically used with blitting a framebuffer maintained
    /// in memory directly to screen using SDL2
    Software(Software25D),
    /// Fully 3D software rendering.
    Software3D(Software3D),
}

/// Backend-agnostic display presentation.
pub enum DisplayBackend {
    #[cfg(feature = "display-sdl2")]
    Sdl2(sdl2_backend::Sdl2Display),
    #[cfg(feature = "display-softbuffer")]
    Softbuffer(softbuffer_backend::SoftbufferDisplay),
    #[cfg(feature = "display-pixels")]
    Pixels(pixels_backend::PixelsDisplay),
}

impl DisplayBackend {
    /// Create an SDL2 display backend from a canvas.
    #[cfg(feature = "display-sdl2")]
    pub fn new_sdl2(canvas: sdl2::render::Canvas<sdl2::video::Window>) -> Self {
        DisplayBackend::Sdl2(sdl2_backend::Sdl2Display::from_canvas(canvas))
    }

    /// Create a softbuffer display backend from a winit window.
    #[cfg(feature = "display-softbuffer")]
    pub fn new_softbuffer(window: std::sync::Arc<winit::window::Window>) -> Self {
        DisplayBackend::Softbuffer(softbuffer_backend::SoftbufferDisplay::new(window))
    }

    /// Create a pixels (wgpu) display backend from a winit window.
    #[cfg(feature = "display-pixels")]
    pub fn new_pixels(window: std::sync::Arc<winit::window::Window>, vsync: bool) -> Self {
        DisplayBackend::Pixels(pixels_backend::PixelsDisplay::new(window, vsync))
    }

    /// Present the buffer to the screen.
    fn blit(&mut self, buffer: &DrawBuffer) {
        match self {
            #[cfg(feature = "display-sdl2")]
            DisplayBackend::Sdl2(d) => d.blit(buffer),
            #[cfg(feature = "display-softbuffer")]
            DisplayBackend::Softbuffer(d) => d.blit(buffer),
            #[cfg(feature = "display-pixels")]
            DisplayBackend::Pixels(d) => d.blit(buffer),
        }
    }

    /// Query the window/drawable size for buffer sizing.
    fn window_size(&self) -> (u32, u32) {
        match self {
            #[cfg(feature = "display-sdl2")]
            DisplayBackend::Sdl2(d) => d.window_size(),
            #[cfg(feature = "display-softbuffer")]
            DisplayBackend::Softbuffer(d) => d.window_size(),
            #[cfg(feature = "display-pixels")]
            DisplayBackend::Pixels(d) => d.window_size(),
        }
    }
}

/// A structure holding display data
pub struct RenderTarget {
    renderer: Renderer,
    framebuffer: FrameBuffer,
}

impl RenderTarget {
    pub fn new(
        double: bool,
        debug: bool,
        debug_draw: &DebugDrawOptions,
        display: DisplayBackend,
        render_type: RenderType,
    ) -> RenderTarget {
        let size = display.window_size();
        // Buffer height is fixed at 200 (or 400 hi-res). Buffer width is chosen
        // so that when the blit scales buf_width→win_width and buf_height→win_height,
        // pixels appear 1.2× taller than wide (CRT aspect):
        //   (win_h / buf_h) / (win_w / buf_w) = 1.2
        //   buf_w = win_w * buf_h * 1.2 / win_h
        const CRT_STRETCH: f32 = 240.0 / 200.0;
        let buf_height = if double { 400u32 } else { 200u32 };
        let buf_width = ((size.0 as f32 * buf_height as f32 * CRT_STRETCH / size.1 as f32).round()
            as u32)
            .max(buf_height);

        Self {
            framebuffer: FrameBuffer {
                wipe: Wipe::new(buf_width as i32, buf_height as i32),
                buffer: DrawBuffer::new(buf_width as usize, buf_height as usize),
                display,
            },
            renderer: match render_type {
                RenderType::Software => Renderer::Software(Software25D::new(
                    90f32.to_radians(),
                    buf_width as f32,
                    buf_height as f32,
                    double,
                    debug,
                )),
                RenderType::Software3D => Renderer::Software3D(Software3D::new(
                    buf_width as f32,
                    buf_height as f32,
                    90.0_f32.to_radians(),
                    debug_draw.clone(),
                )),
            },
        }
    }

    /// Forward a debug overlay line to the active renderer. No-op for non-3D.
    pub fn set_debug_line(&mut self, s: String) {
        if let Renderer::Software3D(r) = &mut self.renderer {
            r.set_debug_line(s);
        }
    }

    /// Rebuild the render target, reusing the display backend.
    pub fn resize(
        self,
        double: bool,
        debug: bool,
        debug_draw: &DebugDrawOptions,
        render_type: RenderType,
    ) -> Self {
        let display = self.framebuffer.display;
        Self::new(double, debug, debug_draw, display, render_type)
    }
}

impl GameRenderer for RenderTarget {
    fn render_player_view(&mut self, player: &Player, level: &mut Level, pic_data: &mut PicData) {
        let f = &mut self.framebuffer;
        match &mut self.renderer {
            Renderer::Software(r) => r.draw_view(player, level, pic_data, f),
            Renderer::Software3D(r) => r.draw_view(player, level, pic_data, f),
        }
    }

    fn frame_buffer(&mut self) -> &mut impl render_trait::DrawBuffer {
        &mut self.framebuffer
    }

    fn flip_and_present(&mut self) {
        #[cfg(feature = "hprof")]
        profile!("flip_and_present");
        self.framebuffer.blit();
    }

    fn start_wipe(&mut self) {
        if self.framebuffer.wipe.is_active() {
            return;
        }
        let pixel_count = self.framebuffer.buffer.size.width_usize()
            * self.framebuffer.buffer.size.height_usize();
        self.framebuffer
            .wipe
            .start(&self.framebuffer.buffer.buffer[..pixel_count]);
    }

    fn do_wipe(&mut self) -> bool {
        self.framebuffer.do_wipe()
    }

    fn buffer_size(&self) -> &BufferSize {
        &self.framebuffer.buffer.size
    }
}

pub(crate) struct DrawBuffer {
    pub(crate) size: BufferSize,
    /// Pixel buffer in `0xFFRRGGBB` format (ARGB, fully opaque).
    pub(crate) buffer: Vec<u32>,
}

impl DrawBuffer {
    fn new(width: usize, height: usize) -> Self {
        Self {
            size: BufferSize::new(width, height),
            buffer: vec![0xFF_00_00_00; width * height + 1],
        }
    }
}

impl DrawBuffer {
    /// Read the colour of a single pixel at X|Y
    #[inline(always)]
    pub fn read_pixel(&self, x: usize, y: usize) -> u32 {
        let pos = y * self.size.width_usize() + x;
        self.buffer[pos]
    }

    #[inline(always)]
    pub fn set_pixel(&mut self, x: usize, y: usize, colour: u32) {
        #[cfg(feature = "hprof")]
        profile!("set_pixel");
        #[cfg(feature = "safety_check")]
        if x >= self.size.width_usize() || y >= self.size.height_usize() {
            dbg!(x, self.size.width_usize(), y, self.size.height_usize());
            panic!();
        }

        let pos = y * self.size.width_usize() + x;
        #[cfg(not(feature = "safety_check"))]
        unsafe {
            *self.buffer.get_unchecked_mut(pos) = colour;
        }
        #[cfg(feature = "safety_check")]
        {
            self.buffer[pos] = colour;
        }
    }

    #[inline(always)]
    pub fn pitch(&self) -> usize {
        self.size.width_usize()
    }

    #[inline(always)]
    pub fn get_buf_index(&self, x: usize, y: usize) -> usize {
        y * self.size.width_usize() + x
    }
}

pub struct FrameBuffer {
    wipe: Wipe,
    buffer: DrawBuffer,
    display: DisplayBackend,
}

impl FrameBuffer {
    /// Present the buffer to the screen via the display backend.
    fn blit(&mut self) {
        self.display.blit(&self.buffer);
    }

    /// Overdraw old-frame columns on top of the current buffer, then present.
    /// Returns true when the melt is complete.
    fn do_wipe(&mut self) -> bool {
        let pitch = self.buffer.pitch();
        let done = self.wipe.do_melt_pixels(&mut self.buffer.buffer, pitch);
        if done {
            self.wipe.reset();
        }
        done
    }
}

impl render_trait::DrawBuffer for FrameBuffer {
    #[inline]
    fn buf_mut(&mut self) -> &mut [u32] {
        &mut self.buffer.buffer
    }

    #[inline]
    fn get_buf_index(&self, x: usize, y: usize) -> usize {
        self.buffer.get_buf_index(x, y)
    }

    #[inline]
    fn pitch(&self) -> usize {
        self.buffer.pitch()
    }

    #[inline]
    fn size(&self) -> &BufferSize {
        &self.buffer.size
    }

    #[inline]
    fn set_pixel(&mut self, x: usize, y: usize, colour: u32) {
        self.buffer.set_pixel(x, y, colour);
    }

    #[inline]
    fn read_pixel(&self, x: usize, y: usize) -> u32 {
        self.buffer.read_pixel(x, y)
    }

    fn debug_flip_and_present(&mut self) {
        self.blit();
    }
}
