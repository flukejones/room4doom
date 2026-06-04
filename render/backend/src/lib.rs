//! Render backend — wires a display backend to a renderer behind one
//! consumer-facing [`RenderStack`], so `game-exe` drives every backend×renderer
//! combo through the same API.
//!
//! # The two axes
//!
//! - **Backend** — *how a frame reaches the window*. `SoftbufferBackend` (CPU
//!   blit), `Sdl2Backend` (streaming texture, or wgpu on the SDL2 window), or
//!   `WgpuBackend` (GPU). Compile-time feature choice; exactly one is active per
//!   build, aliased as [`ActiveBackend`].
//! - **Renderer** — *how the player view is drawn*. Runtime choice
//!   ([`RenderType`]) held in [`WorldRenderer`]: the *software* renderers draw
//!   into a CPU `[P]` buffer; the *hardware* renderer (`wgpu3d`) records into GPU
//!   textures.
//!
//! Renderer and backend are mutually agnostic; they meet through the two present
//! traits ([`RenderKind`]) in the `backend` module. A backend implements
//! whichever kinds it hosts (softbuffer: software only; sdl2/wgpu: both). An
//! unsupported pair is reported via [`RenderStack::supports`], not a panic.
//!
//! # The shared middle layer
//!
//! The CPU framebuffer, double-buffering, melt-wipe, and the `DrawBuffer` impl
//! UI/overlays draw through live in [`Frame`], owned by [`RenderStack`]. Both
//! kinds draw their UI into the *same* `Frame`: software streams its `front`
//! pixels; hardware uploads `front` as the UI texture and composites it over the
//! recorded scene. Backend files carry zero presentation policy.
//!
//! # Per-frame flow (driven by `game-exe`'s `d_display`)
//!
//! [`RenderStack`] exposes ONE uniform per-step API; the render kind is hidden:
//!
//! 1. [`start_wipe`](RenderStack::start_wipe) — seed the melt on a gamestate change.
//! 2. [`set_screen_effects`](RenderStack::set_screen_effects) — tint/bleed (GPU only).
//! 3. [`render_player_view`](RenderStack::render_player_view) — CPU draws the
//!    scene into `Frame`; GPU records it into the encoder.
//! 4. [`ui_frame`](RenderStack::ui_frame) — UI subsystems draw into the shared `Frame`.
//! 5. [`do_wipe`](RenderStack::do_wipe) — CPU melts pixels; GPU steps the offsets.
//! 6. [`present`](RenderStack::present) — CPU streams `Frame`; GPU composites +
//!    melts + presents.
//!
//! See `examples/minimal.rs` for the presentation lifecycle.

use std::sync::Arc;

use level::LevelData;
#[cfg(feature = "cpu-render")]
use pic_data::{ByteOrder, PalLitCache};
use pic_data::{PicData, PixelFmt, VoxelManager};
use render_common::{BufferSize, RenderView};

mod frame;
pub use frame::Frame;

mod backend;
use backend::Backend as _;
#[cfg(all(feature = "display-sdl2", feature = "wgpu3d"))]
pub use backend::new_sdl2_hardware;
#[cfg(feature = "display-sdl2")]
pub use backend::new_sdl2_software;
#[cfg(all(
    feature = "display-softbuffer",
    not(feature = "display-wgpu"),
    not(feature = "display-sdl2")
))]
pub use backend::new_softbuffer;
#[cfg(all(feature = "display-wgpu", not(feature = "display-sdl2")))]
pub use backend::new_wgpu;
pub use backend::{ActiveBackend, RenderKind};

#[cfg(feature = "wgpu3d")]
use backend::HardwarePresent;
#[cfg(feature = "wgpu3d")]
pub use backend::ScreenEffects;
#[cfg(feature = "cpu-render")]
use backend::SoftwarePresent;

mod renderer;
pub use renderer::WorldRenderer;

#[cfg(feature = "display-sdl2")]
mod sdl2_backend;
#[cfg(feature = "display-softbuffer")]
mod softbuffer_backend;
#[cfg(feature = "display-wgpu")]
mod wgpu_backend;
#[cfg(feature = "display-wgpu")]
pub use wgpu_backend::PostEffect;

/// The 1.2× pixel aspect OG Doom presents at; the buffer width is chosen so the
/// compositor's buffer→window scale reproduces it.
const CRT_STRETCH: f32 = 240.0 / 200.0;

/// The active renderer kind. A bare selector — the live renderer lives in
/// [`WorldRenderer`]; this is the user/config-facing choice.
#[derive(Debug, PartialEq, PartialOrd, Clone, Copy)]
pub enum RenderType {
    /// Purely software (software25d). Blits a CPU framebuffer to screen.
    #[cfg(feature = "software25d")]
    Software,
    /// Fully 3D software rendering.
    #[cfg(feature = "software3d")]
    Software3D,
    /// Hardware GPU rendering (wgpu3d). Requires a hardware-capable backend.
    #[cfg(feature = "wgpu3d")]
    Wgpu3D,
}

impl Default for RenderType {
    fn default() -> Self {
        #[cfg(feature = "software3d")]
        return Self::Software3D;
        #[cfg(all(not(feature = "software3d"), feature = "software25d"))]
        return Self::Software;
        #[cfg(all(
            not(feature = "software3d"),
            not(feature = "software25d"),
            feature = "wgpu3d"
        ))]
        return Self::Wgpu3D;
        #[cfg(all(
            not(feature = "software3d"),
            not(feature = "software25d"),
            not(feature = "wgpu3d")
        ))]
        compile_error!("no renderer feature enabled (software25d / software3d / wgpu3d)");
    }
}

impl RenderType {
    /// The render kind this type needs from a backend.
    pub fn kind(self) -> RenderKind {
        match self {
            #[cfg(feature = "wgpu3d")]
            Self::Wgpu3D => RenderKind::Hardware,
            #[allow(unreachable_patterns)]
            _ => RenderKind::Software,
        }
    }
}

/// The byte order the `Frame`/`PalLit` are baked in. Every backend presents
/// `0xAARRGGBB`-derived bytes, so it is always ARGB.
fn byte_order() -> ByteOrder {
    ByteOrder::Argb
}

/// The consumer-facing render target: a backend + an active renderer + the shared
/// CPU [`Frame`] + the engine UI-layout state ([`Self::set_statusbar_height`]).
///
/// Generic over the surface pixel type `P` (`u32` ARGB; `u16` RGB565 on sdl2-565).
pub struct RenderStack<P: PixelFmt> {
    backend: ActiveBackend<P>,
    world_renderer: WorldRenderer,
    /// The shared CPU framebuffer + melt-wipe (the software present path draws
    /// here; the hardware path leaves it idle).
    frame: Frame<P>,
    /// Buffer dimensions the renderer + frame were built for.
    size: BufferSize,
    render_type: RenderType,
    /// Palette block table (`P` pixels) for the CPU direct-write path, rebuilt
    /// only on palette/gamma change.
    #[cfg(feature = "cpu-render")]
    pal_lit: PalLitCache<P>,
    /// The GPU renderer's light-falloff exponent (user config; row→intensity).
    /// Set via [`Self::set_light_gamma`]; the software renderers ignore it.
    #[cfg(feature = "wgpu3d")]
    light_gamma: f32,
    /// Whether the GPU frame's UI plane (the shared [`Frame`]) has been cleared to
    /// transparent for the frame in flight. Cleared once on the first scene/UI
    /// access, reset at [`Self::present`]. CPU path unused (the scene fills it).
    #[cfg(feature = "wgpu3d")]
    gpu_frame_open: bool,
}

impl<P: PixelFmt> RenderStack<P> {
    /// Build a screen for `render_type` over `backend`. `double` selects the
    /// 400px hi-res buffer; the width is chosen for the 1.2× CRT pixel aspect.
    pub fn new(double: bool, backend: ActiveBackend<P>, render_type: RenderType) -> Self {
        let (win_w, win_h) = backend.window_size();
        let buf_height = if double { 400u32 } else { 200u32 };
        let buf_width = ((win_w as f32 * buf_height as f32 * CRT_STRETCH / win_h as f32).round()
            as u32)
            .max(buf_height);
        let (w, h) = (buf_width as usize, buf_height as usize);
        let world_renderer = WorldRenderer::new(render_type, buf_width as f32, buf_height as f32);
        Self {
            backend,
            world_renderer,
            frame: Frame::new(buf_width, buf_height, byte_order()),
            size: BufferSize::new(w, h),
            render_type,
            #[cfg(feature = "cpu-render")]
            pal_lit: PalLitCache::new(),
            #[cfg(feature = "wgpu3d")]
            light_gamma: 1.0,
            #[cfg(feature = "wgpu3d")]
            gpu_frame_open: false,
        }
    }

    /// Set the GPU renderer's light-falloff exponent (user config). No-op for the
    /// software renderers. Call after [`Self::new`]/[`Self::resize`] and on a
    /// config change (the value is not preserved across a rebuild).
    #[cfg(feature = "wgpu3d")]
    pub fn set_light_gamma(&mut self, light_gamma: f32) {
        self.light_gamma = light_gamma;
    }

    /// Rebuild for a new buffer mode / renderer, reusing the backend.
    pub fn resize(self, double: bool, render_type: RenderType) -> Self {
        Self::new(double, self.backend, render_type)
    }

    /// The active renderer kind.
    pub fn render_type(&self) -> RenderType {
        self.render_type
    }

    /// True if the GPU (`wgpu3d`) renderer is active.
    pub fn is_hardware_renderer(&self) -> bool {
        self.world_renderer.is_wgpu3d()
    }

    /// Whether the backend can host the given renderer kind.
    pub fn supports(&self, kind: RenderKind) -> bool {
        self.backend.supports(kind)
    }

    /// Buffer dimensions (the renderer's draw resolution, = the present size).
    pub fn buffer_size(&self) -> &BufferSize {
        &self.size
    }

    /// Window size in physical pixels (for config persistence / fullscreen).
    pub fn window_size(&self) -> (u32, u32) {
        self.backend.window_size()
    }

    /// Set fullscreen mode: 0=windowed, 1=borderless, 2=exclusive.
    pub fn set_fullscreen(&mut self, mode: u8) {
        self.backend.set_fullscreen(mode);
    }

    /// Update the statusbar height (OG 200px-space pixels); recompute the view
    /// height and push it to the renderer.
    pub fn set_statusbar_height(&mut self, og_height: i32) {
        let scale = self.size.height() / 200;
        self.size.set_statusbar_height(og_height * scale);
        self.world_renderer.set_view_height(self.size.view_height());
    }

    /// Set the voxel manager on the active renderer (software3d / wgpu3d only).
    pub fn set_voxel_manager(&mut self, mgr: Arc<VoxelManager>) {
        self.world_renderer.set_voxel_manager(mgr);
    }

    pub fn clear_voxel_manager(&mut self) {
        self.world_renderer.clear_voxel_manager();
    }

    #[cfg(feature = "wgpu3d")]
    pub fn set_dynamic_sky(&mut self, dynamic: bool) {
        self.world_renderer.set_dynamic_sky(dynamic);
    }

    /// Whether a melt-wipe is in progress (CPU frame wipe, or the GPU melt). One
    /// uniform query regardless of render kind.
    pub fn is_wiping(&self) -> bool {
        #[cfg(feature = "wgpu3d")]
        if self.is_hardware_renderer() {
            return HardwarePresent::is_wiping(&self.backend);
        }
        #[cfg(feature = "cpu-render")]
        {
            return self.frame.is_wiping();
        }
        #[allow(unreachable_code)]
        false
    }

    /// Begin a melt-wipe over the last presented frame. CPU re-seeds the frame's
    /// column offsets; GPU seeds the melt-shader offsets and snapshots the last
    /// frame on the next present.
    pub fn start_wipe(&mut self) {
        #[cfg(feature = "wgpu3d")]
        if self.is_hardware_renderer() {
            let (w, h) = (self.size.width() as u32, self.size.height() as u32);
            HardwarePresent::start_wipe(&mut self.backend, w, h);
            return;
        }
        #[cfg(feature = "cpu-render")]
        self.frame.start_wipe();
    }

    /// Reset the health-bleed pattern (new game/level). GPU only; CPU has none.
    pub fn reset_health_bleed(&mut self) {
        #[cfg(feature = "wgpu3d")]
        self.backend.reset_health_bleed();
    }

    /// Resolve the frame's screen effects (player tint, health bleed) for the GPU
    /// composite. No-op on the software path (the renderer applies them per pixel).
    /// Call in the Level state before [`Self::render_player_view`].
    #[cfg(feature = "wgpu3d")]
    pub fn set_screen_effects(&mut self, effects: ScreenEffects) {
        if self.is_hardware_renderer() {
            let (w, h) = (self.size.width() as u32, self.size.height() as u32);
            HardwarePresent::set_screen_effects(&mut self.backend, effects, w, h);
        }
    }

    /// Render the player view (and software3d debug overlays) for the frame in
    /// flight. CPU draws into the `Frame`; GPU records the scene into the held
    /// encoder. UI draws follow via [`Self::ui_frame`].
    pub fn render_player_view(
        &mut self,
        view: &RenderView,
        level_data: &LevelData,
        pic_data: &mut PicData,
    ) {
        #[cfg(feature = "wgpu3d")]
        if self.is_hardware_renderer() {
            self.gpu_open_frame();
            let (w, h) = (self.size.width() as u32, self.size.height() as u32);
            let light_gamma = self.light_gamma;
            let mut handle = HardwarePresent::begin_scene(&mut self.backend, w, h);
            self.world_renderer
                .draw_view_gpu(view, level_data, pic_data, light_gamma, &mut handle);
            return;
        }
        #[cfg(feature = "cpu-render")]
        {
            let pal_lit = self.pal_lit.get(
                pic_data.palette_generation(),
                pic_data.palettes(),
                ByteOrder::Argb,
            );
            let tint = pic_data.use_palette();
            let mut buf = self.frame.pixel_target(pal_lit, tint);
            self.world_renderer
                .draw_view(view, level_data, pic_data, &mut buf);
            self.world_renderer.draw_debug_overlays(pic_data, &mut buf);
        }
    }

    /// The shared [`Frame`] as a `DrawBuffer` for ONE UI subsystem draw — the
    /// same target for both render kinds (the GPU path uploads it as the UI
    /// texture). Reborrow per subsystem: `statusbar.draw(&mut screen.ui_frame())`.
    pub fn ui_frame(&mut self) -> &mut Frame<P> {
        #[cfg(feature = "wgpu3d")]
        if self.is_hardware_renderer() {
            self.gpu_open_frame();
        }
        &mut self.frame
    }

    /// Advance the melt-wipe one step. CPU melts the previous frame over the
    /// just-rendered front; GPU advances the melt-shader offsets (the pixel melt
    /// runs at [`Self::present`]). Returns `true` once the melt completes.
    pub fn do_wipe(&mut self) -> bool {
        #[cfg(feature = "wgpu3d")]
        if self.is_hardware_renderer() {
            return HardwarePresent::advance_wipe(&mut self.backend);
        }
        #[cfg(feature = "cpu-render")]
        {
            return self.frame.do_wipe();
        }
        #[allow(unreachable_code)]
        false
    }

    /// Present the frame in flight. CPU streams the `Frame` (swapping front/back
    /// when not wiping so the just-presented frame seeds the next wipe); GPU
    /// composites the UI (the `Frame`) over the recorded scene, melts at the
    /// current offsets when `wiping`, and presents.
    pub fn present(&mut self, wiping: bool) {
        #[cfg(feature = "wgpu3d")]
        if self.is_hardware_renderer() {
            self.gpu_open_frame();
            let (w, h) = (self.frame.width(), self.frame.height());
            HardwarePresent::finish_frame(&mut self.backend, self.frame.front(), w, h, wiping);
            self.gpu_frame_open = false;
            return;
        }
        #[cfg(feature = "cpu-render")]
        {
            if !wiping {
                self.frame.flip();
            }
            let (w, h) = (self.frame.width(), self.frame.height());
            SoftwarePresent::present(&mut self.backend, self.frame.front(), w, h);
        }
    }

    /// Clear the shared [`Frame`]'s UI plane to transparent on the first scene/UI
    /// access of a GPU frame, so the composite shows the recorded scene through
    /// the unwritten UI texels. Idempotent within a frame.
    #[cfg(feature = "wgpu3d")]
    fn gpu_open_frame(&mut self) {
        if !self.gpu_frame_open {
            self.frame.clear_transparent();
            self.gpu_frame_open = true;
        }
    }
}
