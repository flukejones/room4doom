//! SDL2 backend — two present modes on an SDL2 window.
//!
//! - **Software**: a streaming texture; the engine's CPU `[P]` frame is uploaded
//!   and blitted to the canvas. `P = u32` → RGB888, `P = u16` → RGB565.
//! - **Hardware** (`wgpu3d`): the bare SDL2 window's raw handle drives a wgpu
//!   [`GpuPresenter`]; SDL2 provides only the window, wgpu owns all GPU work.
//!
//! The shared [`Frame`](crate::frame::Frame) owns the CPU pixels + wipe; this
//! backend only streams `front` (software) or records the scene (hardware).

use pic_data::PixelFmt;
use sdl2::pixels::PixelFormatEnum;
use sdl2::rect::Rect;
use sdl2::render::{Canvas, TextureCreator};
use sdl2::video::{FullscreenType, Window, WindowContext};

#[cfg(feature = "cpu-render")]
use crate::backend::SoftwarePresent;
use crate::backend::{Backend, RenderKind};

/// The software canvas + lazily-sized streaming texture.
struct SoftwareCanvas {
    canvas: Canvas<Window>,
    texture: Option<sdl2::render::Texture>,
    tc: TextureCreator<WindowContext>,
    crop_rect: Rect,
    tex_size: (u32, u32),
}

impl SoftwareCanvas {
    fn new(canvas: Canvas<Window>) -> Self {
        let drawable = canvas.window().drawable_size();
        let tc = canvas.texture_creator();
        Self {
            canvas,
            texture: None,
            tc,
            crop_rect: Rect::new(0, 0, drawable.0, drawable.1),
            tex_size: (0, 0),
        }
    }

    /// Ensure the streaming texture matches `w`×`h` + `format`; recreate on change.
    fn ensure_texture(&mut self, w: u32, h: u32, format: PixelFormatEnum) {
        if self.tex_size != (w, h) {
            self.texture = Some(
                self.tc
                    .create_texture_streaming(Some(format), w, h)
                    .expect("failed to create SDL2 streaming texture"),
            );
            self.tex_size = (w, h);
            let drawable = self.canvas.window().drawable_size();
            self.crop_rect = Rect::new(0, 0, drawable.0, drawable.1);
        }
    }
}

/// SDL2 backend in one of two present modes.
pub struct Sdl2Backend<P: PixelFmt> {
    inner: Mode,
    _p: std::marker::PhantomData<P>,
}

/// `presenter` is declared before `window` so it (and its raw-handle surface)
/// drops first, upholding the safety contract of [`GpuPresenter::from_raw_handle`].
enum Mode {
    Software(SoftwareCanvas),
    #[cfg(feature = "wgpu3d")]
    Hardware {
        presenter: crate::wgpu_backend::GpuPresenter,
        window: Window,
    },
}

impl<P: PixelFmt> Sdl2Backend<P> {
    /// Software mode from a built SDL2 canvas (streaming-texture present).
    pub(crate) fn software(canvas: Canvas<Window>) -> Self {
        Self {
            inner: Mode::Software(SoftwareCanvas::new(canvas)),
            _p: std::marker::PhantomData,
        }
    }

    /// Hardware mode from a bare SDL2 window: wgpu owns the GPU via the window's
    /// raw handle. The window is kept alive by this backend for the surface's
    /// lifetime.
    #[cfg(feature = "wgpu3d")]
    pub(crate) fn hardware(window: Window, vsync: bool, post: Vec<crate::PostEffect>) -> Self {
        let (w, h) = window.size();
        // SAFETY: `window` is owned by this backend and `presenter` is dropped
        // before it (field order in `Mode::Hardware`), so the window outlives the
        // surface.
        let presenter = unsafe {
            crate::wgpu_backend::GpuPresenter::from_raw_handle(
                &window,
                vsync,
                post,
                w.max(1),
                h.max(1),
            )
        };
        Self {
            inner: Mode::Hardware {
                presenter,
                window,
            },
            _p: std::marker::PhantomData,
        }
    }

    /// The active SDL2 window (either mode).
    fn window(&self) -> &Window {
        match &self.inner {
            Mode::Software(s) => s.canvas.window(),
            #[cfg(feature = "wgpu3d")]
            Mode::Hardware {
                window,
                ..
            } => window,
        }
    }

    /// The GPU presenter; only valid in hardware mode.
    #[cfg(feature = "wgpu3d")]
    fn hardware_presenter(&mut self) -> &mut crate::wgpu_backend::GpuPresenter {
        let Mode::Hardware {
            presenter,
            ..
        } = &mut self.inner
        else {
            unreachable!("hardware present on a software sdl2 backend");
        };
        presenter
    }
}

impl<P: PixelFmt> Backend for Sdl2Backend<P> {
    fn window_size(&self) -> (u32, u32) {
        self.window().size()
    }

    fn set_fullscreen(&mut self, mode: u8) {
        let fs = match mode {
            1 => FullscreenType::Desktop,
            2 => FullscreenType::True,
            _ => FullscreenType::Off,
        };
        let win: &mut Window = match &mut self.inner {
            Mode::Software(s) => s.canvas.window_mut(),
            #[cfg(feature = "wgpu3d")]
            Mode::Hardware {
                window,
                ..
            } => window,
        };
        let _ = win.set_fullscreen(fs);
    }

    fn supports(&self, kind: RenderKind) -> bool {
        // SDL2 hosts software always, and hardware when wgpu3d is compiled.
        match kind {
            RenderKind::Software => true,
            #[cfg(feature = "wgpu3d")]
            RenderKind::Hardware => true,
            #[cfg(not(feature = "wgpu3d"))]
            RenderKind::Hardware => false,
        }
    }
}

#[cfg(feature = "cpu-render")]
impl<P: PixelFmt> SoftwarePresent<P> for Sdl2Backend<P> {
    fn present(&mut self, front: &[P], w: u32, h: u32) {
        // Irrefutable when `wgpu3d` is off (no `Hardware` variant); refutable
        // when both modes are compiled.
        #[allow(irrefutable_let_patterns)]
        let Mode::Software(s) = &mut self.inner else {
            unreachable!("software present on a hardware sdl2 backend");
        };
        let format = match size_of::<P>() {
            4 => PixelFormatEnum::RGB888,
            2 => PixelFormatEnum::RGB565,
            n => panic!("SDL2 backend: unsupported pixel size {n}"),
        };
        s.ensure_texture(w, h, format);
        let tex = s
            .texture
            .as_mut()
            .expect("texture created by ensure_texture");
        let wb = w as usize;
        tex.with_lock(None, |bytes, byte_pitch| {
            let dst_pitch = byte_pitch / size_of::<P>();
            for y in 0..h as usize {
                let src = &front[y * wb..y * wb + wb];
                // SAFETY: the lock buffer is `dst_pitch` `P`-elements per row.
                let dst = unsafe {
                    std::slice::from_raw_parts_mut(
                        bytes.as_mut_ptr().cast::<P>().add(y * dst_pitch),
                        wb,
                    )
                };
                dst.copy_from_slice(src);
            }
        })
        .expect("failed to lock SDL2 texture");
        s.canvas
            .copy(tex, None, Some(s.crop_rect))
            .expect("failed to copy SDL2 texture to canvas");
        s.canvas.present();
    }
}

#[cfg(feature = "wgpu3d")]
impl<P: PixelFmt> crate::backend::HardwarePresent<P> for Sdl2Backend<P> {
    fn set_screen_effects(&mut self, effects: crate::backend::ScreenEffects, w: u32, h: u32) {
        self.hardware_presenter().set_effects(effects, w, h);
    }

    fn start_wipe(&mut self, w: u32, h: u32) {
        self.hardware_presenter().start_wipe(w, h);
    }

    fn is_wiping(&self) -> bool {
        match &self.inner {
            Mode::Hardware {
                presenter,
                ..
            } => presenter.is_wiping(),
            Mode::Software(_) => false,
        }
    }

    fn begin_scene(&mut self, w: u32, h: u32) -> ::wgpu3d::GpuHandle<'_> {
        let (win_w, win_h) = self.window().size();
        let (win_w, win_h) = (win_w.max(1), win_h.max(1));
        self.hardware_presenter().begin_scene(w, h, win_w, win_h)
    }

    fn advance_wipe(&mut self) -> bool {
        self.hardware_presenter().advance_wipe()
    }

    fn finish_frame(&mut self, front: &[P], w: u32, h: u32, wiping: bool) {
        let (win_w, win_h) = self.window().size();
        let (win_w, win_h) = (win_w.max(1), win_h.max(1));
        let ui = crate::wgpu_backend::as_u32_slice(front);
        self.hardware_presenter()
            .finish_frame(ui, w, h, win_w, win_h, wiping);
    }

    fn reset_health_bleed(&mut self) {
        if let Mode::Hardware {
            presenter,
            ..
        } = &mut self.inner
        {
            presenter.reset_health_bleed();
        }
    }
}
