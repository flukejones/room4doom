//! SDL2 display backend — presents pixels via SDL2 Canvas + Texture.

use sdl2::pixels::PixelFormatEnum;
use sdl2::rect::Rect;
use sdl2::render::{Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};

use crate::DrawBuffer;

/// SDL2 display: owns the canvas, texture, and texture creator.
pub struct Sdl2Display {
    canvas: Canvas<Window>,
    texture: Option<sdl2::render::Texture>,
    _tc: TextureCreator<WindowContext>,
    crop_rect: Rect,
    tex_size: (u32, u32),
}

impl Sdl2Display {
    /// Create from an SDL2 canvas. The texture is created lazily on first
    /// blit to match the framebuffer dimensions.
    pub fn from_canvas(canvas: Canvas<Window>) -> Self {
        let drawable = canvas.window().drawable_size();
        let tc = canvas.texture_creator();
        Self {
            canvas,
            texture: None,
            _tc: tc,
            crop_rect: Rect::new(0, 0, drawable.0, drawable.1),
            tex_size: (0, 0),
        }
    }

    /// Ensure the streaming texture matches the buffer dimensions.
    fn ensure_texture(&mut self, w: u32, h: u32) {
        if self.tex_size != (w, h) {
            self.texture = Some(
                self._tc
                    .create_texture_streaming(Some(PixelFormatEnum::RGB888), w, h)
                    .expect("failed to create SDL2 streaming texture"),
            );
            self.tex_size = (w, h);
            let drawable = self.canvas.window().drawable_size();
            self.crop_rect = Rect::new(0, 0, drawable.0, drawable.1);
        }
    }

    /// Present the front buffer to the screen.
    pub(crate) fn blit(&mut self, buffer: &DrawBuffer) {
        let w = buffer.size.width() as u32;
        let h = buffer.size.height() as u32;
        self.ensure_texture(w, h);

        let byte_buf = unsafe {
            std::slice::from_raw_parts(buffer.buffer.as_ptr() as *const u8, buffer.buffer.len() * 4)
        };
        let stride = buffer.size.width_usize() * 4;
        let tex = self.texture.as_mut().unwrap();
        tex.update(None, byte_buf, stride).unwrap();
        self.canvas.copy(tex, None, Some(self.crop_rect)).unwrap();
        self.canvas.present();
    }

    /// Window size in screen coordinates.
    pub(crate) fn window_size(&self) -> (u32, u32) {
        self.canvas.window().size()
    }
}
