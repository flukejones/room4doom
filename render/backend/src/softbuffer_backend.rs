//! Softbuffer backend — shows the engine's CPU frame via winit + softbuffer.
//!
//! Software-present only (softbuffer is a CPU-framebuffer→window blitter; it has
//! no GPU device, so it cannot host the hardware renderer). The shared
//! [`Frame`](crate::frame::Frame) owns the pixels + wipe; this backend copies the
//! frame's `front` into softbuffer's persistent buffer and presents. `u32`-only.

use std::num::NonZeroU32;
use std::sync::Arc;

use pic_data::PixelFmt;
use softbuffer::{AlphaMode, Context, Pixel, Surface};
use winit::window::{Fullscreen, Window};

use crate::backend::{Backend, RenderKind, SoftwarePresent};

#[inline(always)]
fn nz(v: u32) -> NonZeroU32 {
    NonZeroU32::new(v).unwrap_or(NonZeroU32::new(1).unwrap())
}

/// Softbuffer backend. The `Context` is leaked (`Box::leak`) to satisfy
/// `Surface`'s `&Context` borrow — one backend for the process lifetime. `u32`-
/// only; the `P` parameter (always `u32` here) keeps `ActiveBackend<P>` uniform.
pub struct SoftbufferBackend<P: PixelFmt> {
    surface: Surface<Arc<Window>, Arc<Window>>,
    window: Arc<Window>,
    /// Buffer dimensions the surface is configured for.
    size: (u32, u32),
    _p: std::marker::PhantomData<P>,
}

impl<P: PixelFmt> SoftbufferBackend<P> {
    pub(crate) fn new(window: Arc<Window>) -> Self {
        let ctx: &'static Context<Arc<Window>> = Box::leak(Box::new(
            Context::new(window.clone()).expect("failed to create softbuffer context"),
        ));
        let surface =
            Surface::new(ctx, window.clone()).expect("failed to create softbuffer surface");
        Self {
            surface,
            window,
            size: (0, 0),
            _p: std::marker::PhantomData,
        }
    }

    /// Configure the surface to `w`×`h`. `AlphaMode::Ignored` is preferred so
    /// undrawn alpha bytes don't trip softbuffer's `Opaque` debug check; falls
    /// back to `Opaque` where `Ignored` is unsupported (e.g. Core Graphics).
    fn configure_surface(&mut self, w: u32, h: u32) {
        if self.size == (w, h) {
            return;
        }
        let mode = if self.surface.supports_alpha_mode(AlphaMode::Ignored) {
            AlphaMode::Ignored
        } else {
            AlphaMode::Opaque
        };
        self.surface
            .configure(nz(w), nz(h), mode)
            .expect("failed to configure softbuffer surface");
        self.size = (w, h);
    }
}

impl<P: PixelFmt> Backend for SoftbufferBackend<P> {
    fn window_size(&self) -> (u32, u32) {
        let s = self.window.inner_size();
        (s.width, s.height)
    }

    fn set_fullscreen(&mut self, mode: u8) {
        let fs = match mode {
            1 => Some(Fullscreen::Borderless(None)),
            2 => self
                .window
                .current_monitor()
                .or_else(|| self.window.primary_monitor())
                .and_then(|m| m.video_modes().next())
                .map(Fullscreen::Exclusive),
            _ => None,
        };
        self.window.set_fullscreen(fs);
    }

    fn supports(&self, kind: RenderKind) -> bool {
        kind == RenderKind::Software
    }
}

impl<P: PixelFmt> SoftwarePresent<P> for SoftbufferBackend<P> {
    fn present(&mut self, front: &[P], w: u32, h: u32) {
        debug_assert_eq!(size_of::<P>(), 4, "softbuffer is u32-only");
        self.configure_surface(w, h);
        let mut buf = self.surface.next_buffer().expect("softbuffer next_buffer");
        let stride = buf.byte_stride().get() as usize / size_of::<Pixel>();
        let dst = as_u32_mut(buf.pixels());
        // SAFETY: P == u32 (asserted); `front` is tight `w*h`, the surface may be
        // padded (stride >= w), so copy row by row.
        let src: &[u32] =
            unsafe { std::slice::from_raw_parts(front.as_ptr().cast::<u32>(), front.len()) };
        let w = w as usize;
        for y in 0..h as usize {
            dst[y * stride..y * stride + w].copy_from_slice(&src[y * w..y * w + w]);
        }
        buf.present().expect("failed to present softbuffer");
    }
}

/// Reinterpret a `&mut [Pixel]` as `&mut [u32]`.
///
/// # Safety
/// `Pixel` is `#[repr(C, align(4))]` (four `u8`s) — size/align identical to
/// `u32`, length and layout preserved.
#[inline(always)]
fn as_u32_mut(pixels: &mut [Pixel]) -> &mut [u32] {
    let len = pixels.len();
    let ptr = pixels.as_mut_ptr().cast::<u32>();
    // SAFETY: layout-identical (see above).
    unsafe { std::slice::from_raw_parts_mut(ptr, len) }
}
