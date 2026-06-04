//! Display backends and the present contracts they fulfil.
//!
//! A backend is *how a frame reaches the window*. Exactly one is compiled per
//! build, aliased as [`ActiveBackend`] and constructed via the `new_*` fns. Each
//! implements the present trait(s) for the render *kinds* it hosts — softbuffer:
//! software only; sdl2 / wgpu: both:
//!
//! - [`SoftwarePresent`] — show the engine's finished CPU `[P]` frame.
//! - [`HardwarePresent`] — own the GPU device + present pipeline (composite,
//!   melt, post-chain) and hold the live encoder across one frame's steps.
//!   [`RenderStack`](crate::RenderStack) drives it per step (set effects → start
//!   wipe → begin scene → advance wipe → finish frame), mirroring the software
//!   per-step path; UI is drawn into the shared CPU [`Frame`](crate::Frame).
//!
//! The traits hand out a borrowed [`GpuHandle`], so they are static bounds only,
//! never `dyn`.

#[cfg(all(
    any(feature = "display-softbuffer", feature = "display-wgpu"),
    not(feature = "display-sdl2")
))]
use std::sync::Arc;

use pic_data::PixelFmt;
#[cfg(feature = "wgpu3d")]
use wgpu3d::GpuHandle;

#[cfg(feature = "display-wgpu")]
use crate::PostEffect;

/// The single active backend for this build, chosen by feature (exactly one
/// display backend is compiled-and-selected). `P` is the surface pixel type
/// (`u32` ARGB; `u16` RGB565 on sdl2).
#[cfg(feature = "display-sdl2")]
pub type ActiveBackend<P> = crate::sdl2_backend::Sdl2Backend<P>;
#[cfg(all(feature = "display-wgpu", not(feature = "display-sdl2")))]
pub type ActiveBackend<P> = crate::wgpu_backend::WgpuBackend<P>;
#[cfg(all(
    feature = "display-softbuffer",
    not(feature = "display-wgpu"),
    not(feature = "display-sdl2")
))]
pub type ActiveBackend<P> = crate::softbuffer_backend::SoftbufferBackend<P>;

/// The two renderer kinds a backend may host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderKind {
    /// CPU renderer (software25d/software3d) — draws into a `[P]` frame.
    Software,
    /// GPU renderer (wgpu3d) — records into GPU textures. Needs a GPU device.
    Hardware,
}

/// Every backend: window queries plus a capability check.
pub(crate) trait Backend {
    fn window_size(&self) -> (u32, u32);
    /// 0=windowed, 1=borderless, 2=exclusive.
    fn set_fullscreen(&mut self, mode: u8);
    fn supports(&self, kind: RenderKind) -> bool;
}

/// Show the engine's CPU `[P]` frame. The [`Frame`](crate::Frame) is owned by
/// [`RenderStack`](crate::RenderStack); the backend only shows `front`.
pub(crate) trait SoftwarePresent<P: PixelFmt>: Backend {
    /// Present the `w`×`h` front buffer (tight pitch `w`).
    fn present(&mut self, front: &[P], w: u32, h: u32);
}

/// Own the GPU device + present pipeline, holding the live encoder across one
/// frame's steps. `w`×`h` is the engine buffer size; the backend sources the
/// window size itself.
#[cfg(feature = "wgpu3d")]
pub(crate) trait HardwarePresent<P: PixelFmt>: Backend {
    /// Resolve tint/bleed into the composite uniform (Level state only).
    fn set_screen_effects(&mut self, effects: ScreenEffects, w: u32, h: u32);
    /// Seed the melt offsets at a wipe's start.
    fn start_wipe(&mut self, w: u32, h: u32);
    fn is_wiping(&self) -> bool;
    /// Begin recording the scene; the returned handle borrows the held encoder.
    fn begin_scene(&mut self, w: u32, h: u32) -> GpuHandle<'_>;
    /// Step the melt offsets (no GPU work); `true` once complete.
    fn advance_wipe(&mut self) -> bool;
    /// Consume the encoder: upload `front` as the UI texture, composite over the
    /// scene, melt at the current offsets when `wiping`, post-chain, present.
    fn finish_frame(&mut self, front: &[P], w: u32, h: u32, wiping: bool);
    fn reset_health_bleed(&mut self);
}

/// Per-frame screen-effect inputs passed by value; the hardware present resolves
/// them into the GPU composite uniform. Plain data so `game-exe` stays free of
/// wgpu3d types.
#[cfg(feature = "wgpu3d")]
#[derive(Clone, Copy)]
pub struct ScreenEffects {
    pub damagecount: i32,
    pub bonuscount: i32,
    pub radsuit: bool,
    pub fixedcolormap: usize,
    /// Player health (0..100) driving the bleed columns.
    pub health: i32,
    /// `HealthBleed` config toggle; off forces the bleed inactive.
    pub bleed_enabled: bool,
}

#[cfg(feature = "wgpu3d")]
impl Default for ScreenEffects {
    fn default() -> Self {
        Self {
            damagecount: 0,
            bonuscount: 0,
            radsuit: false,
            fixedcolormap: 0,
            health: 100,
            bleed_enabled: false,
        }
    }
}

/// Softbuffer backend from a winit window. Gated to match [`ActiveBackend`]
/// (softbuffer is active only when neither wgpu nor sdl2 is compiled).
#[cfg(all(
    feature = "display-softbuffer",
    not(feature = "display-wgpu"),
    not(feature = "display-sdl2")
))]
pub fn new_softbuffer<P: PixelFmt>(window: Arc<winit::window::Window>) -> ActiveBackend<P> {
    crate::softbuffer_backend::SoftbufferBackend::new(window)
}

/// wgpu backend from a winit window. Empty `post` = nearest-neighbour stretch.
#[cfg(all(feature = "display-wgpu", not(feature = "display-sdl2")))]
pub fn new_wgpu<P: PixelFmt>(
    window: Arc<winit::window::Window>,
    vsync: bool,
    post: Vec<PostEffect>,
) -> ActiveBackend<P> {
    crate::wgpu_backend::WgpuBackend::new(window, vsync, post)
}

/// SDL2 software backend from a built canvas.
#[cfg(feature = "display-sdl2")]
pub fn new_sdl2_software<P: PixelFmt>(
    canvas: sdl2::render::Canvas<sdl2::video::Window>,
) -> ActiveBackend<P> {
    crate::sdl2_backend::Sdl2Backend::software(canvas)
}

/// SDL2 hardware backend (wgpu on the SDL2 window) from a bare window.
#[cfg(all(feature = "display-sdl2", feature = "wgpu3d"))]
pub fn new_sdl2_hardware<P: PixelFmt>(
    window: sdl2::video::Window,
    vsync: bool,
    post: Vec<PostEffect>,
) -> ActiveBackend<P> {
    crate::sdl2_backend::Sdl2Backend::hardware(window, vsync, post)
}
