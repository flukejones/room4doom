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
//! The CPU framebuffer, double-buffering, melt-wipe and the scene draw target
//! live in [`Frame`], owned by [`RenderStack`]; the plane UI draws into is a
//! [`UiSurface`]. Software streams the `Frame`'s `front` pixels; hardware
//! uploads the UI surfaces as panels and draws them over the recorded scene.
//! Backend files carry zero presentation policy.
//!
//! # UI planes
//!
//! A flat display has one plane, so [`hud_frame`](RenderStack::hud_frame) and
//! [`screen_frame`](RenderStack::screen_frame) hand back the same surface and
//! call order decides what covers what. A headset gives each its own virtual
//! panel: the hud rides the head, the screen holds the player's facing.
//!
//! # Per-frame flow (driven by `game-exe`'s `d_display`)
//!
//! [`RenderStack`] exposes ONE uniform per-step API; the render kind is hidden.
//! UI is view-independent, so it is drawn and uploaded once; the scene and the
//! present run once per eye:
//!
//! 1. [`start_wipe`](RenderStack::start_wipe) — seed the melt on a gamestate change.
//! 2. [`hud_frame`](RenderStack::hud_frame) /
//!    [`screen_frame`](RenderStack::screen_frame) — UI subsystems draw.
//! 3. [`upload_ui`](RenderStack::upload_ui) — hand the planes to the display.
//! 4. [`set_screen_effects`](RenderStack::set_screen_effects) — tint/bleed (GPU only).
//! 5. [`render_player_view`](RenderStack::render_player_view) — CPU draws the
//!    scene into `Frame`; GPU records it into the encoder.
//! 6. [`present`](RenderStack::present) — CPU streams `Frame`; GPU applies the
//!    effects, draws the panels, melts and presents.
//! 7. [`do_wipe`](RenderStack::do_wipe) — CPU melts pixels; GPU steps the offsets.
//!
//! See `examples/minimal.rs` for the presentation lifecycle.

use std::sync::Arc;

use level::LevelData;
#[cfg(feature = "cpu-render")]
use pic_data::{ByteOrder, PalLitCache};
use pic_data::{PicData, PixelFmt, VoxelManager};
#[cfg(feature = "wgpu3d")]
use render_common::HandPose;
use render_common::{BufferSize, EYE_COUNT, EyeView, RenderView};

mod frame;
pub use frame::{Frame, UiSurface};

mod backend;
use backend::Backend as _;
pub use backend::PanelPlacement;
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
#[cfg(all(feature = "vr", not(feature = "display-sdl2")))]
pub use backend::new_vr;
#[cfg(all(feature = "display-wgpu", not(feature = "display-sdl2")))]
pub use backend::new_wgpu;
pub use backend::{ActiveBackend, RenderKind};

#[cfg(feature = "wgpu3d")]
use backend::HardwarePresent;
#[cfg(feature = "wgpu3d")]
pub use backend::ScreenEffects;
#[cfg(feature = "cpu-render")]
use backend::SoftwarePresent;
#[cfg(feature = "wgpu3d")]
use backend::{UiPixels, UiPlanes};
#[cfg(feature = "wgpu3d")]
use wgpu_backend::as_u32_slice;

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

/// The 1.2× pixel aspect OG Doom presents at; on a monitor the buffer width is
/// chosen so the compositor's buffer→window scale reproduces it.
#[cfg(feature = "software25d")]
use render_common::CRT_STRETCH;

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
    /// Hardware GPU rendering with a CPU BSP front-end (wgpu3dbsp): frustum
    /// culling + in-walk entity collection instead of whole-level submission.
    #[cfg(feature = "wgpu3dbsp")]
    Wgpu3DBsp,
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
            #[cfg(feature = "wgpu3dbsp")]
            Self::Wgpu3DBsp => RenderKind::Hardware,
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

/// Index of each UI plane in [`RenderStack::panes`].
const HUD_PLANE: usize = 0;
const SCREEN_PLANE: usize = 1;

/// Doom's virtual UI screen. Every subsystem lays out in 320×200 and centres
/// itself with `(plane_width - 320 * plane_height / 200) / 2`, so a plane
/// narrower than this aspect gives a negative offset and clips both edges.
const UI_VIRTUAL_WIDTH: u32 = 320;
const UI_VIRTUAL_HEIGHT: u32 = 200;

/// The consumer-facing render target: a backend + an active renderer + the shared
/// CPU [`Frame`] + the engine UI-layout state ([`Self::set_statusbar_height`]).
///
/// Generic over the surface pixel type `P` (`u32` ARGB; `u16` RGB565 on sdl2-565).
pub struct RenderStack<P: PixelFmt> {
    backend: ActiveBackend<P>,
    world_renderer: WorldRenderer,
    /// The shared CPU framebuffer + melt-wipe. The software present path draws
    /// scene and UI here; on the hardware path its surface is the hud plane.
    frame: Frame<P>,
    /// Dedicated UI planes, on a display that wants more than one. `None` on a
    /// flat display, where the UI is the frame's own surface and the menu simply
    /// draws over the statusbar. Indexed by [`HUD_PLANE`]/[`SCREEN_PLANE`].
    panes: Option<[UiSurface<P>; 2]>,
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
    /// 400px hi-res buffer.
    ///
    /// software25d draws OG tall pixels, so its buffer is 1.2× wider than
    /// the display and the fill blit stretches it back — the CRT look.
    /// The full-3D renderers draw square pixels at the display's aspect.
    pub fn new(double: bool, backend: ActiveBackend<P>, render_type: RenderType) -> Self {
        let (win_w, win_h) = backend.window_size();
        let buf_height = if double { 400u32 } else { 200u32 };
        #[cfg(feature = "software25d")]
        let pixel_aspect = if render_type == RenderType::Software {
            CRT_STRETCH
        } else {
            1.0
        };
        #[cfg(not(feature = "software25d"))]
        let pixel_aspect = 1.0;
        let buf_width = ((win_w as f32 * buf_height as f32 * pixel_aspect / win_h as f32).round()
            as u32)
            .max(buf_height);
        let (w, h) = (buf_width as usize, buf_height as usize);
        let world_renderer = WorldRenderer::new(render_type, buf_width as f32, buf_height as f32);
        // More than one viewpoint means the UI planes are placed independently
        // in space, so each needs its own surface.
        let mut panes = None;
        #[cfg(feature = "wgpu3d")]
        if world_renderer.is_wgpu3d() && HardwarePresent::eye_count(&backend) > 1 {
            let plane_w = buf_height * UI_VIRTUAL_WIDTH / UI_VIRTUAL_HEIGHT;
            panes = Some([
                UiSurface::new(plane_w, buf_height, byte_order()),
                UiSurface::new(plane_w, buf_height, byte_order()),
            ]);
        }
        Self {
            backend,
            world_renderer,
            frame: Frame::new(buf_width, buf_height, byte_order()),
            panes,
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
    ///
    /// Ignored where the UI has planes of its own: the statusbar is then a
    /// panel floating in front of the player rather than a strip cut out of the
    /// screen, so the world fills the whole view.
    pub fn set_statusbar_height(&mut self, og_height: i32) {
        let og_height = if self.panes.is_some() { 0 } else { og_height };
        let scale = self.size.height() / 200;
        self.size.set_statusbar_height(og_height * scale);
        self.world_renderer.set_view_height(self.size.view_height());
    }

    /// Render the next view through a headset's asymmetric per-eye
    /// frustum (left, right, up, down radians); `None` restores the flat
    /// projection a monitor wants.
    pub fn set_eye_fov(&mut self, fov: Option<[f32; 4]>) {
        self.world_renderer.set_eye_fov(fov);
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

    /// The tracked hands to draw next frame; `None` draws none.
    #[cfg(feature = "wgpu3d")]
    pub fn set_hands(&mut self, hands: Option<HandPose>) {
        self.world_renderer.set_hands(hands);
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

    /// The plane the statusbar and messages draw into; rides the head in a
    /// headset. Reborrow per subsystem: `statusbar.draw(&mut s.hud_frame())`.
    pub fn hud_frame(&mut self) -> &mut UiSurface<P> {
        self.ui_plane(HUD_PLANE)
    }

    /// The plane menus, intermission, finale and the title page draw into; held
    /// at the player's facing in a headset.
    pub fn screen_frame(&mut self) -> &mut UiSurface<P> {
        self.ui_plane(SCREEN_PLANE)
    }

    fn ui_plane(&mut self, plane: usize) -> &mut UiSurface<P> {
        #[cfg(feature = "wgpu3d")]
        if self.is_hardware_renderer() {
            self.gpu_open_frame();
        }
        match &mut self.panes {
            Some(panes) => &mut panes[plane],
            None => self.frame.surface_mut(),
        }
    }

    /// Hand this frame's UI planes to the display. UI is view-independent, so
    /// this runs once per frame however many eyes follow. A plane nothing drew
    /// to is neither uploaded nor drawn. No-op on the software path, which
    /// presents the `Frame` directly.
    pub fn upload_ui(&mut self) {
        #[cfg(feature = "wgpu3d")]
        if self.is_hardware_renderer() {
            // Clears here too, or a frame nothing drew keeps the last one's UI.
            self.gpu_open_frame();
            fn pixels_of<P: PixelFmt>(s: &UiSurface<P>) -> UiPixels<'_> {
                UiPixels {
                    pixels: as_u32_slice(s.pixels()),
                    w: s.width(),
                    h: s.height(),
                }
            }
            let hud = match &self.panes {
                Some(panes) => &panes[HUD_PLANE],
                None => self.frame.surface(),
            };
            let screen = self.panes.as_ref().map(|panes| &panes[SCREEN_PLANE]);
            let planes = UiPlanes {
                hud: hud.is_dirty().then(|| pixels_of(hud)),
                screen: screen.and_then(|s| s.is_dirty().then(|| pixels_of(s))),
            };
            HardwarePresent::upload_ui(&mut self.backend, planes);
            self.gpu_frame_open = false;
        }
    }

    /// Where the display's virtual screens sit. No-op on a flat display, where
    /// the UI covers the window.
    pub fn set_panels(&mut self, hud: PanelPlacement, screen: PanelPlacement) {
        #[cfg(feature = "vr")]
        if self.is_hardware_renderer() {
            HardwarePresent::set_panels(&mut self.backend, hud, screen);
        }
        // Only a headset places panels.
        #[cfg(not(feature = "vr"))]
        let _ = (hud, screen);
    }

    /// Pin the facing-locked panel to where the player is looking now, for the
    /// moment a menu appears.
    pub fn anchor_panels(&mut self) {
        #[cfg(feature = "vr")]
        if self.is_hardware_renderer() {
            HardwarePresent::anchor_panels(&mut self.backend);
        }
    }

    /// The live headset session, for gaze, controllers and play space. `None`
    /// on a window.
    #[cfg(all(feature = "vr", not(feature = "display-sdl2")))]
    pub fn session(&mut self) -> Option<&mut vr::Session> {
        if !self.is_hardware_renderer() {
            return None;
        }
        self.backend.session()
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

    /// Views the display needs per frame: 1 for a window, 2 for a headset.
    pub fn eye_count(&self) -> usize {
        #[cfg(feature = "wgpu3d")]
        if self.is_hardware_renderer() {
            return HardwarePresent::eye_count(&self.backend);
        }
        1
    }

    /// Open the display's frame and locate every eye. The headset runtime paces
    /// the display, so this is where a frame begins; `None` means either there
    /// is no headset or it refused this tic, and no eye should then be drawn.
    pub fn begin_frame(&mut self) -> Option<[Option<EyeView>; EYE_COUNT]> {
        #[cfg(feature = "vr")]
        if self.is_hardware_renderer() {
            return HardwarePresent::begin_frame(&mut self.backend);
        }
        None
    }

    /// Aim the next scene and present at `eye` of the frame already begun.
    /// A display with one viewpoint has nothing to aim.
    pub fn set_eye(&mut self, eye: usize) {
        #[cfg(feature = "vr")]
        if self.is_hardware_renderer() {
            HardwarePresent::set_eye(&mut self.backend, eye);
        }
        // Only a headset has an eye to aim.
        #[cfg(not(feature = "vr"))]
        let _ = eye;
    }

    /// Present the frame in flight. CPU streams the `Frame` (swapping front/back
    /// when not wiping so the just-presented frame seeds the next wipe); GPU
    /// composites the UI (the `Frame`) over the recorded scene, melts at the
    /// current offsets when `wiping`, and presents.
    pub fn present(&mut self, wiping: bool) {
        #[cfg(feature = "wgpu3d")]
        if self.is_hardware_renderer() {
            let (w, h) = (self.size.width() as u32, self.size.height() as u32);
            HardwarePresent::finish_frame(&mut self.backend, w, h, wiping);
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

    /// Clear every UI plane to transparent on the frame's first UI access, so
    /// the panels show the scene through unwritten texels. Idempotent.
    #[cfg(feature = "wgpu3d")]
    fn gpu_open_frame(&mut self) {
        if self.gpu_frame_open {
            return;
        }
        match &mut self.panes {
            Some(panes) => panes.iter_mut().for_each(UiSurface::clear_transparent),
            None => self.frame.clear_transparent(),
        }
        self.gpu_frame_open = true;
    }
}
