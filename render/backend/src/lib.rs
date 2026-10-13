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
//! `RenderTargets` owns the active renderer's surfaces. Software keeps a
//! [`Frame`] with double-buffering and a melt-wipe, plus a separate world buffer
//! when needed. Hardware keeps [`UiSurface`] panels and uses GPU composition and
//! wipes. Both paths expose the same generic UI drawing interface.
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
//! 2. [`set_screen_effects`](RenderStack::set_screen_effects) — tint/bleed (GPU only).
//! 3. [`render_player_view`](RenderStack::render_player_view) — draw the scene.
//! 4. [`hud_frame`](RenderStack::hud_frame) /
//!    [`screen_frame`](RenderStack::screen_frame) — UI subsystems draw.
//! 5. [`upload_ui`](RenderStack::upload_ui) — hand the planes to the display.
//! 6. [`present`](RenderStack::present) — CPU streams `Frame`; GPU applies the
//!    effects, draws the panels, melts and presents.
//! 7. [`do_wipe`](RenderStack::do_wipe) — report completion; step GPU offsets.
//!
//! See `examples/minimal.rs` for the presentation lifecycle.

use std::sync::Arc;

use level::LevelData;
#[cfg(feature = "cpu-render")]
use pic_data::PalLitCache;
use pic_data::{ByteOrder, PicData, PixelFmt, VoxelManager};
#[cfg(feature = "cpu-render")]
use render_common::DrawBuffer as _;
#[cfg(feature = "wgpu3d")]
use render_common::HandPose;
use render_common::{BufferSize, EYE_COUNT, EyeView, RenderView, UiLayout};
#[cfg(feature = "cpu-render")]
use ui_draw::{draw_text_line, measure_text_line};

mod frame;
pub use frame::{Frame, UiSurface};
mod resolution;
use resolution::Resolution;
pub use resolution::ResolutionOptions;
mod targets;
use targets::RenderTargets;

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
/// The consumer-facing renderer, display, surfaces and shared UI-layout state.
///
/// Generic over the surface pixel type `P` (`u32` ARGB; `u16` RGB565 on sdl2-565).
pub struct RenderStack<P: PixelFmt> {
    backend: ActiveBackend<P>,
    world_renderer: WorldRenderer,
    targets: RenderTargets<P>,
    resolution: Resolution,
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
}

impl<P: PixelFmt> RenderStack<P> {
    /// Build independent world and UI targets, reusing the display's presentation path.
    pub fn new(
        options: impl Into<ResolutionOptions>,
        backend: ActiveBackend<P>,
        render_type: RenderType,
    ) -> Self {
        let resolution = Self::resolve(&backend, options.into(), render_type);
        let (w, h) = resolution.world;
        let size = BufferSize::new(w as usize, h as usize);
        let world_renderer = WorldRenderer::new(render_type, size.width_f32(), size.height_f32());
        let targets = RenderTargets::new(resolution, render_type.kind(), byte_order());
        Self {
            backend,
            world_renderer,
            targets,
            resolution,
            size,
            render_type,
            #[cfg(feature = "cpu-render")]
            pal_lit: PalLitCache::new(),
            #[cfg(feature = "wgpu3d")]
            light_gamma: 1.0,
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
    pub fn resize(
        mut self,
        options: impl Into<ResolutionOptions>,
        render_type: RenderType,
    ) -> Self {
        self.reconfigure(options.into(), render_type);
        self
    }

    /// Update dimensions in place, deferring changes until a wipe finishes.
    pub fn reconfigure(&mut self, options: ResolutionOptions, render_type: RenderType) -> bool {
        if self.is_wiping() {
            return false;
        }
        let resolution = Self::resolve(&self.backend, options, render_type);
        if self.resolution == resolution && self.render_type == render_type {
            return false;
        }
        let (w, h) = resolution.world;
        if self.resolution.world != resolution.world || self.render_type != render_type {
            self.size = BufferSize::new(w as usize, h as usize);
            self.world_renderer =
                WorldRenderer::new(render_type, self.size.width_f32(), self.size.height_f32());
        }
        self.targets
            .reconfigure(resolution, render_type.kind(), byte_order());
        self.render_type = render_type;
        self.resolution = resolution;
        true
    }

    fn resolve(
        backend: &ActiveBackend<P>,
        options: ResolutionOptions,
        render_type: RenderType,
    ) -> Resolution {
        #[cfg(feature = "wgpu3d")]
        let stereo =
            render_type.kind() == RenderKind::Hardware && HardwarePresent::eye_count(backend) > 1;
        #[cfg(not(feature = "wgpu3d"))]
        let stereo = false;
        Resolution::new(
            options,
            render_type,
            backend.display_size(),
            backend.render_aspect(),
            stereo,
        )
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

    /// World-rendering dimensions, independent of UI and presentation resolution.
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
    #[expect(
        clippy::cast_possible_truncation,
        reason = "statusbar boundaries quantize UI rows"
    )]
    pub fn set_statusbar_height(&mut self, og_height: i32) {
        let og_height = if self.resolution.stereo {
            0
        } else {
            og_height.clamp(0, 200)
        };
        let og_height = u16::try_from(og_height).expect("clamped statusbar height");
        let layout = UiLayout::from_sizes(self.resolution.world, self.resolution.display);
        let height = (f32::from(og_height) * layout.scale[1]).round() as i32;
        self.size.set_statusbar_height(height);
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
        self.world_renderer.set_hands(hands.as_ref());
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
            return self.targets.software_frame().is_wiping();
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
            let (w, h) = self.resolution.output;
            HardwarePresent::start_wipe(&mut self.backend, w, h);
            return;
        }
        #[cfg(feature = "cpu-render")]
        self.targets.software_mut().0.start_wipe();
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
    /// encoder. UI draws follow via [`Self::hud_frame`] and [`Self::screen_frame`].
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
            let mut handle =
                HardwarePresent::begin_scene(&mut self.backend, w, h, self.resolution.output);
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
            let (frame, scene) = self.targets.software_mut();
            let target = scene.as_mut().unwrap_or_else(|| frame.surface_mut());
            if self.size.statusbar_height() > 0 {
                let start = self.size.view_height_usize() * self.size.width_usize();
                target.buf_mut()[start..].fill(P::from_argb(0xFF00_0000, ByteOrder::Argb));
            }
            let mut buf = target.pixel_target(pal_lit, tint);
            self.world_renderer
                .draw_view(view, level_data, pic_data, &mut buf);
            self.world_renderer.draw_debug_overlays(&mut buf);
            if let Some(scene) = scene {
                frame.surface_mut().copy_scaled(scene);
            }
            let text = self.world_renderer.debug_line();
            if !text.is_empty() {
                let ui = frame.surface_mut();
                let [sx, sy] = ui.ui_layout().scale;
                let x = ui.size().width_f32() - measure_text_line(&text, sx) - 4.0 * sx;
                draw_text_line(&text, x, 2.0 * sy, sx, sy, pic_data.wad_palette(), ui);
            }
        }
    }

    /// The plane the statusbar and messages draw into; rides the head in a
    /// headset. Reborrow per subsystem: `statusbar.draw(&mut s.hud_frame())`.
    pub fn hud_frame(&mut self) -> &mut UiSurface<P> {
        self.targets.hud_mut()
    }

    /// The plane menus, intermission, finale and the title page draw into.
    pub fn screen_frame(&mut self) -> &mut UiSurface<P> {
        self.targets.screen_mut()
    }

    /// Hand this frame's UI planes to the display. UI is view-independent, so
    /// this runs once per frame however many eyes follow. A plane nothing drew
    /// to is neither uploaded nor drawn. No-op on the software path, which
    /// presents the `Frame` directly.
    pub fn upload_ui(&mut self) {
        #[cfg(feature = "wgpu3d")]
        if self.is_hardware_renderer() {
            let planes = self.targets.ui_planes();
            HardwarePresent::upload_ui(&mut self.backend, planes);
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

    /// Finish the presented wipe step; GPU offsets advance here, CPU melt runs in `present`.
    pub fn do_wipe(&mut self) -> bool {
        #[cfg(feature = "wgpu3d")]
        if self.is_hardware_renderer() {
            return HardwarePresent::advance_wipe(&mut self.backend);
        }
        #[cfg(feature = "cpu-render")]
        {
            return !self.targets.software_frame().is_wiping();
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
    /// composites the UI surfaces over the recorded scene, melts at the
    /// current offsets when `wiping`, and presents.
    pub fn present(&mut self, wiping: bool) {
        #[cfg(feature = "wgpu3d")]
        if self.is_hardware_renderer() {
            let (w, h) = (self.size.width() as u32, self.size.height() as u32);
            HardwarePresent::finish_frame(&mut self.backend, w, h, self.resolution.output, wiping);
            return;
        }
        #[cfg(feature = "cpu-render")]
        {
            let (frame, _) = self.targets.software_mut();
            if wiping {
                frame.do_wipe();
            }
            let (w, h) = (frame.width(), frame.height());
            SoftwarePresent::present(&mut self.backend, frame.front(), w, h);
            if !wiping {
                frame.flip();
            }
        }
    }
}
