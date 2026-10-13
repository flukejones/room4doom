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
//!   [`RenderStack`](crate::RenderStack) drives it per step (set effects →
//!   start wipe → upload UI → begin scene → advance wipe → finish frame),
//!   mirroring the software per-step path. UI is drawn into CPU
//!   [`UiSurface`](crate::UiSurface)s and uploaded once per frame; the scene and
//!   the present run once per eye.
//!
//! The traits hand out a borrowed [`GpuHandle`], so they are static bounds only,
//! never `dyn`.

#[cfg(all(
    any(feature = "display-softbuffer", feature = "display-wgpu"),
    not(feature = "display-sdl2")
))]
use std::sync::Arc;

use pic_data::PixelFmt;
#[cfg(feature = "vr")]
use render_common::{EYE_COUNT, EyeView};
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
    fn display_size(&self) -> (u32, u32) {
        self.window_size()
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "physical target sizes are converted to rendering coordinates"
    )]
    fn render_aspect(&self) -> f32 {
        let (w, h) = self.display_size();
        w.max(1) as f32 / h.max(1) as f32
    }
    /// 0=windowed, 1=borderless, 2=exclusive.
    fn set_fullscreen(&mut self, mode: u8);
    fn supports(&self, kind: RenderKind) -> bool;
}

/// Show the engine's CPU `[P]` frame. The [`Frame`](crate::Frame) is owned by
/// [`RenderStack`](crate::RenderStack); the backend only shows `front`.
#[cfg(feature = "cpu-render")]
pub(crate) trait SoftwarePresent<P: PixelFmt>: Backend {
    /// Present the `w`×`h` front buffer (tight pitch `w`).
    fn present(&mut self, front: &[P], w: u32, h: u32);
}

/// Own the GPU device + present pipeline, holding the live encoder across one
/// frame's steps. `w`×`h` is the engine buffer size; the backend sources the
/// window size itself.
/// The GPU path is `Bgra8Unorm` throughout, so this is not generic over the
/// surface pixel type the way [`SoftwarePresent`] is.
#[cfg(feature = "wgpu3d")]
pub(crate) trait HardwarePresent: Backend {
    /// Resolve tint/bleed into the composite uniform (Level state only).
    fn set_screen_effects(&mut self, effects: ScreenEffects, w: u32, h: u32);
    /// Seed the melt offsets at a wipe's start.
    fn start_wipe(&mut self, w: u32, h: u32);
    fn is_wiping(&self) -> bool;
    /// Begin recording the scene; the returned handle borrows the held encoder.
    fn begin_scene(&mut self, w: u32, h: u32, output: (u32, u32)) -> GpuHandle<'_>;
    /// Views this backend needs per frame: 1 for a window, 2 for a headset.
    fn eye_count(&self) -> usize {
        1
    }
    /// Open the display's frame and report where each eye looks from. `None`
    /// on a display with a single fixed viewpoint, or when the runtime refused
    /// the frame — in which case no eye should be drawn.
    #[cfg(feature = "vr")]
    fn begin_frame(&mut self) -> Option<[Option<EyeView>; EYE_COUNT]> {
        None
    }
    /// Target the next scene and present at `eye`, of the frame already begun.
    #[cfg(feature = "vr")]
    fn set_eye(&mut self, _eye: usize) {}
    /// Pin the facing-locked panel to where the player is looking now.
    #[cfg(feature = "vr")]
    fn anchor_panels(&mut self) {}
    /// Panel geometry for the display's virtual screens.
    #[cfg(feature = "vr")]
    fn set_panels(&mut self, _hud: PanelPlacement, _screen: PanelPlacement) {}
    /// Step the melt offsets (no GPU work); `true` once complete.
    fn advance_wipe(&mut self) -> bool;
    /// Upload the UI planes for the frame in flight, once per frame.
    fn upload_ui(&mut self, planes: UiPlanes<'_>);
    /// Consume the encoder for one eye: scene effects, UI panels over the
    /// result, melt at the current offsets when `wiping`, post-chain, present.
    fn finish_frame(&mut self, scene_w: u32, scene_h: u32, output: (u32, u32), wiping: bool);
    fn reset_health_bleed(&mut self);
}

/// One UI plane's pixels for [`HardwarePresent::upload_ui`].
#[cfg(feature = "wgpu3d")]
pub(crate) struct UiPixels<'a> {
    pub pixels: &'a [u32],
    pub w: u32,
    pub h: u32,
}

/// The UI planes for one frame. `None` = nothing drew on that plane, so it is
/// neither uploaded nor drawn. On a flat display the whole UI is `hud` and
/// `screen` is always `None`.
#[cfg(feature = "wgpu3d")]
pub(crate) struct UiPlanes<'a> {
    pub hud: Option<UiPixels<'a>>,
    pub screen: Option<UiPixels<'a>>,
}

/// Where a headset's virtual screen sits and how big it is. What it is anchored
/// to is fixed per plane and belongs to the display, not here.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanelPlacement {
    /// Distance in front of the viewer, metres.
    pub distance_m: f32,
    /// Angle the panel subtends, degrees.
    pub width_deg: f32,
    /// Panel centre relative to eye level, degrees; negative is below.
    pub pitch_deg: f32,
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

/// wgpu backend presenting to a VR runtime instead of the window surface.
/// The window is kept for input and OS activation.
#[cfg(all(feature = "vr", not(feature = "display-sdl2")))]
pub fn new_vr<P: PixelFmt>(
    window: Arc<winit::window::Window>,
    session: vr::Session,
    post: Vec<PostEffect>,
) -> ActiveBackend<P> {
    crate::wgpu_backend::WgpuBackend::new_vr(window, session, post)
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
