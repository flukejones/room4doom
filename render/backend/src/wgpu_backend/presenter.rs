//! The window-agnostic wgpu present pipeline.
//!
//! Owns the device/queue/surface, the framebuffer texture and the post chain.
//! Hosts both the CPU software renderers (uploads their `[P]` front buffer)
//! and the GPU `wgpu3d` renderer (scene targets + composite/melt/panels).
//! Presentation only — the renderers own nothing here, and neither does the
//! window (the backend does).

#[cfg(feature = "hprof")]
use coarse_prof::profile;
#[cfg(all(not(feature = "display-sdl2"), feature = "cpu-render"))]
use pic_data::PixelFmt;
use wgpu::CurrentSurfaceTexture::{Suboptimal, Success};
#[cfg(feature = "wgpu3d")]
use wgpu3d::{GpuHandle, SceneEffects};

#[cfg(feature = "wgpu3d")]
use render_common::wipe::MeltColumns;

#[cfg(feature = "vr")]
use render_common::{EYE_COUNT, EyeView};

#[cfg(feature = "vr")]
use crate::backend::PanelPlacement;
#[cfg(feature = "wgpu3d")]
use crate::backend::{ScreenEffects, UiPlanes};
use crate::wgpu_backend::FORMAT;
#[cfg(feature = "vr")]
use crate::wgpu_backend::headset::Headset;
#[cfg(feature = "vr")]
use crate::wgpu_backend::headset::{HUD_PANEL, SCREEN_PANEL};
#[cfg(feature = "wgpu3d")]
use crate::wgpu_backend::panel::{self, PanelPipeline};
use crate::wgpu_backend::post::{PostChain, PostEffect};
#[cfg(feature = "wgpu3d")]
use crate::wgpu_backend::scene::{self, GpuTargets, Melt};
#[cfg(all(not(feature = "display-sdl2"), feature = "cpu-render"))]
use crate::wgpu_backend::{PIXEL_BYTES, as_u32_slice, bytemuck_cast};

/// The engine framebuffer texture and its dimensions. The software-present path
/// uploads an externally-owned `[u32]` front buffer into it each frame.
struct FrameTexture {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    w: u32,
    h: u32,
}

impl FrameTexture {
    fn new(device: &wgpu::Device, w: u32, h: u32) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("frame_texture"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            // RENDER_ATTACHMENT lets the GPU composite pass target it; COPY_DST
            // serves the software CPU-upload path; COPY_SRC lets the melt wipe
            // snapshot it as the "old" frame.
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            texture,
            view,
            w,
            h,
        }
    }

    /// Upload `pixels` (tight `w*h`) into the GPU texture (software present path).
    #[cfg(all(not(feature = "display-sdl2"), feature = "cpu-render"))]
    fn upload(&self, queue: &wgpu::Queue, pixels: &[u32]) {
        let bytes: &[u8] = bytemuck_cast(pixels);
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(self.w * PIXEL_BYTES),
                rows_per_image: Some(self.h),
            },
            wgpu::Extent3d {
                width: self.w,
                height: self.h,
                depth_or_array_layers: 1,
            },
        );
    }
}

/// Create the wgpu instance, surface, adapter and device for `target` (a window
/// satisfying `Into<SurfaceTarget>`, e.g. `Arc<winit::Window>`). The surface owns
/// the window target, so it borrows nothing — `'static`. Winit path only; the
/// sdl2 hardware mode uses [`init_gpu_raw`].
#[cfg(not(feature = "display-sdl2"))]
fn init_gpu(
    target: impl Into<wgpu::SurfaceTarget<'static>>,
) -> (wgpu::Surface<'static>, wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::default();
    let surface = instance
        .create_surface(target)
        .expect("failed to create wgpu surface");
    adapter_and_device(&instance, surface)
}

/// Like [`init_gpu`] but from a raw window handle (e.g. an sdl2 window). The
/// surface does NOT keep the window alive, so the caller MUST own the window and
/// keep it alive at least as long as the returned surface (the sdl2 hardware
/// backend owns both, dropping the surface before the window).
///
/// # Safety
/// `window` must remain valid for the lifetime of the returned `Surface`.
#[cfg(feature = "display-sdl2")]
unsafe fn init_gpu_raw(
    window: &(impl wgpu::rwh::HasWindowHandle + wgpu::rwh::HasDisplayHandle),
) -> (wgpu::Surface<'static>, wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::default();
    // SAFETY: the caller guarantees `window` outlives the surface.
    let surface = unsafe {
        instance
            .create_surface_unsafe(
                wgpu::SurfaceTargetUnsafe::from_window(window)
                    .expect("sdl2 window has no raw handle"),
            )
            .expect("failed to create wgpu surface from raw handle")
    };
    adapter_and_device(&instance, surface)
}

/// Request the high-performance adapter + device for `surface`. Shared by both
/// surface-creation paths.
fn adapter_and_device(
    instance: &wgpu::Instance,
    surface: wgpu::Surface<'static>,
) -> (wgpu::Surface<'static>, wgpu::Device, wgpu::Queue) {
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: Some(&surface),
    }))
    .expect("no compatible wgpu adapter");

    // Use the adapter's real limits, not downlevel defaults: a retina surface
    // (e.g. 3024×1898) exceeds the downlevel 2048 max texture size.
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("room4doom_device"),
        required_features: wgpu::Features::empty(),
        required_limits: adapter.limits(),
        memory_hints: wgpu::MemoryHints::Performance,
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
        trace: wgpu::Trace::Off,
    }))
    .expect("failed to request wgpu device");

    (surface, device, queue)
}

/// The swapchain configuration at `w`×`h`.
fn surface_config(w: u32, h: u32, vsync: bool) -> wgpu::SurfaceConfiguration {
    wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: FORMAT,
        width: w,
        height: h,
        present_mode: if vsync {
            wgpu::PresentMode::AutoVsync
        } else {
            wgpu::PresentMode::AutoNoVsync
        },
        // 2 frames in flight: lets the CPU run a frame ahead of the GPU. Dropping
        // to 1 serializes CPU+GPU on acquire (measured ~17% slower).
        desired_maximum_frame_latency: 2,
        alpha_mode: wgpu::CompositeAlphaMode::Auto,
        view_formats: vec![],
    }
}

/// See the module doc; one per backend window (or headset).
pub(crate) struct GpuPresenter {
    /// `None` in VR: the runtime owns presentation, so there is no window
    /// surface and the composite writes straight into an eye image.
    surface: Option<wgpu::Surface<'static>>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    /// Live headset state when this presenter drives one. The device and
    /// queue above are the session's, so every existing target, pipeline and
    /// composite works unchanged.
    #[cfg(feature = "vr")]
    headset: Option<Headset>,
    /// One engine framebuffer per view: a single entry for a window, one
    /// per eye in VR so each eye keeps its own last frame (the melt wipe
    /// snapshots it).
    frames: Vec<FrameTexture>,
    /// Post chain per view, each sampling its own frame in `frames`.
    chains: Vec<PostChain>,
    /// Last configured surface size; reconfigure only on change (Metal
    /// `surface.configure()` is a pipeline stall).
    surface_size: (u32, u32),
    /// GPU renderer display-side resources (scene targets, composite, panels,
    /// melt); built lazily on the GPU path. See [`scene::GpuResources`].
    #[cfg(feature = "wgpu3d")]
    gpu: scene::GpuResources,
    /// Per-column health-bleed shape, resolved into `bleed_cols` for the
    /// composite shader. Owned here — it is GPU presentation state.
    #[cfg(feature = "wgpu3d")]
    bleed: render_common::HealthBleed,
    /// Per-column bleed upload scratch (`vec4<f32>` per column).
    #[cfg(feature = "wgpu3d")]
    bleed_cols: Vec<[f32; 4]>,
    /// Melt-wipe column offsets; `None` until a wipe begins, re-seeded each wipe.
    #[cfg(feature = "wgpu3d")]
    melt_cols: Option<MeltColumns>,
    /// Live command encoder for the frame in flight, `None` between frames.
    /// `wgpu`'s encoder is `'static`, so it can span the scene → UI → present
    /// calls; consumed by `finish_frame`.
    #[cfg(feature = "wgpu3d")]
    encoder: Option<wgpu::CommandEncoder>,
    /// Effects resolved by `set_effects`; reset to default each `finish_frame`.
    #[cfg(feature = "wgpu3d")]
    frame_effects: SceneEffects,
    /// Views whose old frame still needs snapshotting for the melt wipe.
    /// Set for every view by `start_wipe`; each view captures its own
    /// frame once and clears its slot.
    #[cfg(feature = "wgpu3d")]
    wipe_capture_pending: Vec<bool>,
    /// Whether [`Self::begin_scene`] recorded a player view this frame. When
    /// false (non-`Level` states), `finish_frame` clears the stale scene texture
    /// before the composite so the transparent UI borders don't show it.
    #[cfg(feature = "wgpu3d")]
    scene_recorded: bool,
}

impl GpuPresenter {
    /// Build the present pipeline over an already-created `surface`/`device`/
    /// `queue` (the window lives in the backend). Configures the surface and
    /// builds the framebuffer texture + post chain at the `win_w`×`win_h` window
    /// size.
    pub(crate) fn new(
        surface: Option<wgpu::Surface<'static>>,
        device: wgpu::Device,
        queue: wgpu::Queue,
        config: wgpu::SurfaceConfiguration,
        post: Vec<PostEffect>,
        win_w: u32,
        win_h: u32,
    ) -> Self {
        // The framebuffer texture starts at the window size; `sync_sizes`
        // resizes it to the engine buffer size on the first frame.
        let frame = FrameTexture::new(&device, win_w, win_h);
        let chain = PostChain::new(&device, post, &frame.view, (win_w, win_h));

        Self {
            frames: vec![frame],
            chains: vec![chain],
            surface,
            device,
            queue,
            config,
            #[cfg(feature = "vr")]
            headset: None,
            surface_size: (win_w, win_h),
            #[cfg(feature = "wgpu3d")]
            gpu: scene::GpuResources::default(),
            #[cfg(feature = "wgpu3d")]
            bleed: render_common::HealthBleed::default(),
            #[cfg(feature = "wgpu3d")]
            bleed_cols: Vec::new(),
            #[cfg(feature = "wgpu3d")]
            melt_cols: None,
            #[cfg(feature = "wgpu3d")]
            encoder: None,
            #[cfg(feature = "wgpu3d")]
            frame_effects: SceneEffects::default(),
            #[cfg(feature = "wgpu3d")]
            wipe_capture_pending: Vec::new(),
            #[cfg(feature = "wgpu3d")]
            scene_recorded: false,
        }
    }

    /// Create from any window target (winit `Arc<Window>`, sdl2 window handle, …).
    /// Builds the wgpu instance/surface/device and configures the swapchain at
    /// `win_w`×`win_h`. The single entry point a backend uses to stand up the GPU
    /// present pipeline on its own window.
    #[cfg(not(feature = "display-sdl2"))]
    pub(crate) fn from_target(
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        vsync: bool,
        post: Vec<PostEffect>,
        win_w: u32,
        win_h: u32,
    ) -> Self {
        let (surface, device, queue) = init_gpu(target);
        let config = surface_config(win_w, win_h, vsync);
        surface.configure(&device, &config);
        Self::new(Some(surface), device, queue, config, post, win_w, win_h)
    }

    /// Create against a VR runtime. The session's device and queue become
    /// this presenter's, so every target, pipeline and composite below is
    /// the same code the window path uses — only presentation differs.
    /// The window keeps a surface so one eye can be mirrored into it, but
    /// it must be created on the session's device — a second device could
    /// not sample the eye image.
    #[cfg(feature = "vr")]
    pub(crate) fn from_vr(
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        session: vr::Session,
        post: Vec<PostEffect>,
        win_w: u32,
        win_h: u32,
    ) -> Self {
        let device = session.device().clone();
        let queue = session.queue().clone();
        let config = surface_config(win_w, win_h, true);

        let surface = session
            .wgpu_instance()
            .create_surface(target)
            .inspect_err(|e| log::warn!("vr mirror unavailable: {e}"))
            .ok();
        if let Some(surface) = &surface {
            surface.configure(&device, &config);
        }

        let additional_post = post.clone();
        let mut this = Self::new(surface, device, queue, config, post, win_w, win_h);
        this.headset = Some(Headset::new(session));
        // One frame + chain per eye, so each eye keeps its own last frame.
        while this.frames.len() < vr::EYE_COUNT {
            let frame = FrameTexture::new(&this.device, win_w, win_h);
            let chain = PostChain::new(
                &this.device,
                additional_post.clone(),
                &frame.view,
                (win_w, win_h),
            );
            this.frames.push(frame);
            this.chains.push(chain);
        }
        this
    }

    /// Create from a raw window handle (e.g. an sdl2 window). The caller MUST keep
    /// `window` alive at least as long as this presenter (the sdl2 hardware
    /// backend owns both and drops the presenter first).
    ///
    /// # Safety
    /// `window` must outlive the returned presenter's surface.
    #[cfg(feature = "display-sdl2")]
    pub(crate) unsafe fn from_raw_handle(
        window: &(impl wgpu::rwh::HasWindowHandle + wgpu::rwh::HasDisplayHandle),
        vsync: bool,
        post: Vec<PostEffect>,
        win_w: u32,
        win_h: u32,
    ) -> Self {
        // SAFETY: forwarded — the caller guarantees the window outlives the surface.
        let (surface, device, queue) = unsafe { init_gpu_raw(window) };
        let config = surface_config(win_w, win_h, vsync);
        surface.configure(&device, &config);
        Self::new(Some(surface), device, queue, config, post, win_w, win_h)
    }

    /// Upload the `buf_w`×`buf_h` front buffer (tight pitch `buf_w`) and run the
    /// post-chain to the window (`win_w`×`win_h`). u32-only (`Bgra8Unorm`): `P`
    /// must be `u32` (asserted), reinterpreted as `[u32]` for upload.
    #[cfg(all(not(feature = "display-sdl2"), feature = "cpu-render"))]
    pub(crate) fn present_software<P: PixelFmt>(
        &mut self,
        front: &[P],
        buf_w: u32,
        buf_h: u32,
        win_w: u32,
        win_h: u32,
    ) {
        #[cfg(feature = "hprof")]
        profile!("wgpu_render");
        self.sync_sizes(win_w, win_h, buf_w, buf_h);
        self.frames[0].upload(&self.queue, as_u32_slice(front));

        let Some(surface_tex) = self.acquire_surface() else {
            return;
        };
        let view = surface_tex
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame_encoder"),
            });
        self.chains[0].render(&mut encoder, &view, [0, 0, win_w, win_h]);
        self.queue.submit(Some(encoder.finish()));
        surface_tex.present();
    }

    /// Resize the framebuffer texture to the engine buffer (`buf_w`×`buf_h`) and
    /// the surface to the window (`win_w`×`win_h`) when either changed, rebuilding
    /// the post chain over the new inputs. Reconfigures the surface only on change
    /// (Metal `surface.configure()` is a pipeline stall).
    fn sync_sizes(&mut self, win_w: u32, win_h: u32, buf_w: u32, buf_h: u32) {
        let frame_changed = self.frames[0].w != buf_w || self.frames[0].h != buf_h;
        if frame_changed {
            for frame in &mut self.frames {
                *frame = FrameTexture::new(&self.device, buf_w, buf_h);
            }
        }
        let win = (win_w.max(1), win_h.max(1));
        let surface_changed = win != self.surface_size;
        if surface_changed {
            self.config.width = win.0;
            self.config.height = win.1;
            if let Some(surface) = &self.surface {
                surface.configure(&self.device, &self.config);
            }
            self.surface_size = win;
        }
        if frame_changed || surface_changed {
            #[cfg(feature = "vr")]
            let post_size = self
                .headset
                .as_ref()
                .map_or(self.surface_size, |h| h.session.view_size());
            #[cfg(not(feature = "vr"))]
            let post_size = self.surface_size;
            for (chain, frame) in self.chains.iter_mut().zip(&self.frames) {
                chain.resize(&self.device, &frame.view, post_size);
            }
        }
    }

    /// Index into `frames`/`chains` for the view being rendered: the
    /// current eye in VR, the single window frame otherwise.
    #[cfg_attr(
        not(feature = "vr"),
        expect(
            clippy::unused_self,
            reason = "the VR build reads the current eye from this presenter"
        )
    )]
    fn cur_view(&self) -> usize {
        #[cfg(feature = "vr")]
        {
            self.headset
                .as_ref()
                .map_or(0, Headset::eye)
                .min(self.frames.len() - 1)
        }
        #[cfg(not(feature = "vr"))]
        {
            0
        }
    }

    /// Acquire the next swapchain texture, reconfiguring and retrying once if the
    /// surface is lost/outdated. `None` drops the frame.
    fn acquire_surface(&self) -> Option<wgpu::SurfaceTexture> {
        // VR has no surface; the runtime's swapchain is the target instead.
        let surface = self.surface.as_ref()?;
        match surface.get_current_texture() {
            Success(t) | Suboptimal(t) => Some(t),
            _ => {
                surface.configure(&self.device, &self.config);
                match surface.get_current_texture() {
                    Success(t) | Suboptimal(t) => Some(t),
                    _ => {
                        log::trace!("wgpu: dropped frame, surface unavailable");
                        None
                    }
                }
            }
        }
    }
}

#[cfg(feature = "wgpu3d")]
impl GpuPresenter {
    /// Reset the health-bleed pattern (new game/level → fresh shape on the next
    /// damaged frame).
    pub(crate) fn reset_health_bleed(&mut self) {
        self.bleed.reset();
    }

    /// Resolve the frame's effects into the composite uniform + bleed scratch for
    /// [`Self::finish_frame`]. Level state only; other frames keep the defaults.
    pub(crate) fn set_effects(&mut self, effects: ScreenEffects, buf_w: u32, buf_h: u32) {
        let mut scene_effects = SceneEffects::new(
            effects.damagecount,
            effects.bonuscount,
            effects.radsuit,
            effects.fixedcolormap,
        );
        // Bleed off => health 100 => inactive.
        let health = if effects.bleed_enabled {
            effects.health
        } else {
            100
        };
        self.bleed.update(health, buf_w as usize, buf_h as usize);
        scene_effects.set_bleed_active(self.bleed.is_active());
        if self.bleed.is_active() {
            self.bleed.gpu_columns(&mut self.bleed_cols);
        } else {
            self.bleed_cols.clear();
        }
        self.frame_effects = scene_effects;
    }

    /// Seed the melt offsets at a wipe's start. The next [`Self::finish_frame`]
    /// snapshots the old frame; [`Self::advance_wipe`] steps the offsets.
    pub(crate) fn start_wipe(&mut self, buf_w: u32, buf_h: u32) {
        self.melt_cols = Some(MeltColumns::new(buf_w as i32, buf_h as i32));
        self.wipe_capture_pending = vec![true; self.frames.len()];
    }

    /// True while a GPU melt-wipe is in progress.
    pub(crate) fn is_wiping(&self) -> bool {
        self.melt_cols.is_some()
    }

    /// Ensure targets + a live encoder exist, then return a [`GpuHandle`]
    /// borrowing the held encoder + scene/depth views. The renderer records
    /// through it; the encoder persists for [`Self::finish_frame`].
    pub(crate) fn begin_scene(
        &mut self,
        buf_w: u32,
        buf_h: u32,
        win_w: u32,
        win_h: u32,
        output: (u32, u32),
    ) -> GpuHandle<'_> {
        self.ensure_frame(buf_w, buf_h, win_w, win_h, output);
        self.scene_recorded = true;
        let gpu = self.gpu.targets.as_mut().expect("gpu targets built above");
        GpuHandle {
            device: &self.device,
            queue: &self.queue,
            encoder: self.encoder.as_mut().expect("encoder built above"),
            scene_view: &gpu.scene_view,
            depth_view: &gpu.depth_view,
        }
    }

    /// Step the melt offsets one frame (no GPU work); `true` once complete. The
    /// pixel melt runs in [`Self::finish_frame`] at the current offsets.
    pub(crate) fn advance_wipe(&mut self) -> bool {
        let done = self.melt_cols.as_mut().is_some_and(|m| m.advance());
        if done {
            self.melt_cols = None;
        }
        done
    }

    /// Build or resize the panel each live UI plane uploads into, then upload
    /// it. A `None` plane received nothing this frame and is neither uploaded
    /// nor drawn.
    pub(crate) fn upload_ui(&mut self, planes: UiPlanes<'_>) {
        #[cfg(feature = "vr")]
        let projected = self.headset.is_some();
        #[cfg(not(feature = "vr"))]
        let projected = false;
        let pipeline = self
            .gpu
            .panels
            .get_or_insert_with(|| PanelPipeline::new(&self.device, projected));
        for (slot, live, plane) in [
            (&mut self.gpu.hud, &mut self.gpu.hud_live, planes.hud),
            (
                &mut self.gpu.screen,
                &mut self.gpu.screen_live,
                planes.screen,
            ),
        ] {
            *live = plane.is_some();
            let Some(plane) = plane else {
                continue;
            };
            if slot.as_ref().is_none_or(|p| p.size() != (plane.w, plane.h)) {
                *slot = Some(pipeline.panel(&self.device, plane.w, plane.h));
            }
            slot.as_ref()
                .expect("panel built above")
                .upload(&self.queue, plane.pixels);
        }
    }

    /// Record the live UI panels into the pass, farthest first: nothing
    /// depth-tests here, so paint order is the only thing making occlusion
    /// agree with the disparity the eyes already see.
    fn record_panels(&self, rpass: &mut wgpu::RenderPass<'_>) {
        let Some(pipeline) = self.gpu.panels.as_ref() else {
            return;
        };
        #[cfg(feature = "vr")]
        if let Some(headset) = self.headset.as_ref() {
            let mut order = [
                (HUD_PANEL, panel::Lock::Head),
                (SCREEN_PANEL, panel::Lock::Facing),
            ];
            if headset.panels[order[0].0].distance_m < headset.panels[order[1].0].distance_m {
                order.swap(0, 1);
            }
            for (plane, lock) in order {
                let (slot, live) = match plane {
                    HUD_PANEL => (&self.gpu.hud, self.gpu.hud_live),
                    _ => (&self.gpu.screen, self.gpu.screen_live),
                };
                let Some(target_panel) = slot.as_ref() else {
                    continue;
                };
                if !live {
                    continue;
                }
                let place = self.placement(target_panel, headset.panels[plane], lock);
                pipeline.record(&self.queue, rpass, target_panel, place);
            }
            return;
        }
        if self.gpu.hud_live
            && let Some(hud) = self.gpu.hud.as_ref()
        {
            pipeline.record(&self.queue, rpass, hud, panel::FULLSCREEN);
        }
    }

    /// Where `panel` lands in the eye being drawn. Falls back to full-screen
    /// with no pose, so the UI is never lost.
    #[cfg(feature = "vr")]
    #[expect(
        clippy::cast_precision_loss,
        reason = "panel pixel dimensions intentionally enter the f32 projection aspect ratio"
    )]
    fn placement(
        &self,
        panel: &panel::Panel,
        placement: PanelPlacement,
        lock: panel::Lock,
    ) -> [[f32; 4]; 4] {
        let Some(headset) = self.headset.as_ref() else {
            return panel::FULLSCREEN;
        };
        let Some(camera) = headset.camera.as_ref() else {
            return panel::FULLSCREEN;
        };
        let (w, h) = panel.size();
        panel::clip_from_panel(
            camera,
            headset.session.head_position(),
            placement,
            lock,
            headset.panel_yaw,
            w as f32 / h as f32,
        )
    }

    /// Consume the held encoder for one eye: one pass applying the scene
    /// effects and drawing the UI panels over the result, melt at the current
    /// offsets when `wiping`, post-chain, present. Offsets are stepped by
    /// [`Self::advance_wipe`].
    pub(crate) fn finish_frame(
        &mut self,
        buf_w: u32,
        buf_h: u32,
        win_w: u32,
        win_h: u32,
        output: (u32, u32),
        wiping: bool,
    ) {
        #[cfg(feature = "hprof")]
        profile!("wgpu_gpu_frame");

        // A UI-only frame (no player view) still needs targets/encoder.
        self.ensure_frame(buf_w, buf_h, win_w, win_h, output);
        let scene_recorded = self.scene_recorded;
        let cur = self.cur_view();
        let mut encoder = self.encoder.take().expect("encoder built above");

        // Buffer writes and the (cached) bind group, before the pass.
        {
            let composite = self.gpu.composite.as_mut().expect("composite built above");
            composite.set_effects(&self.queue, &self.frame_effects);
            composite.set_bleed(&self.device, &self.queue, &self.bleed_cols);
        }
        let gpu = self.gpu.targets.as_ref().expect("gpu targets built above");
        self.gpu
            .composite
            .as_mut()
            .expect("composite built above")
            .rebind(&self.device, &gpu.scene_view);
        let composite = self.gpu.composite.as_ref().expect("composite built above");

        // Non-Level frames record no scene; clear the stale one or it shows
        // around the panels.
        if !scene_recorded {
            scene::clear_view(&mut encoder, &gpu.scene_view);
        }

        // Wiping: draw into the melt scratch, then slide the snapshot over it
        // into the view's frame. Otherwise draw straight into the frame.
        let melt = if wiping {
            self.melt_cols
                .as_ref()
                .and_then(|_| self.gpu.melts.get(cur))
        } else {
            None
        };
        let target = match melt {
            Some(m) => &m.scratch_view,
            None => &self.frames[cur].view,
        };
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("composite_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            composite.record(&mut rpass);
            self.record_panels(&mut rpass);
        }
        if let Some(m) = melt {
            let offsets = self.melt_cols.as_ref().expect("melt active").offsets();
            m.render(&self.queue, &mut encoder, &self.frames[cur].view, offsets);
        }

        #[cfg(feature = "vr")]
        let presented_to_eye = self.present_eye(&mut encoder);
        #[cfg(not(feature = "vr"))]
        let presented_to_eye = false;

        if !presented_to_eye && let Some(surface_tex) = self.acquire_surface() {
            let view = surface_tex
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            self.chains[cur].render(&mut encoder, &view, [0, 0, win_w, win_h]);
            self.queue.submit(Some(encoder.finish()));
            surface_tex.present();
        } else if !presented_to_eye {
            self.queue.submit(Some(encoder.finish()));
        }

        self.frame_effects = SceneEffects::default();
        self.scene_recorded = false;
    }

    /// Runs the existing post chain into the current eye's swapchain image
    /// and, once both eyes are filled, submits the runtime frame. Returns
    /// false when there is no VR frame open, so the window path runs instead.
    #[cfg(feature = "vr")]
    fn present_eye(&mut self, encoder: &mut wgpu::CommandEncoder) -> bool {
        let Some(headset) = self.headset.as_ref() else {
            return false;
        };
        if !headset.frame_open() {
            return false;
        }
        let eye = headset.eye();
        if let Some(target) = headset.eye_target() {
            self.chains[eye].render(encoder, target.colour, target.rect);
        }

        // Left-eye mirror into the window: same chain, second target.
        let mirror = if eye == 0 {
            self.acquire_surface()
        } else {
            None
        };
        if let Some(surface_tex) = &mirror {
            let view = surface_tex
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            self.chains[eye].render(
                encoder,
                &view,
                [0, 0, self.config.width, self.config.height],
            );
        }

        let done = std::mem::replace(
            encoder,
            self.device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("vr_eye_encoder"),
                }),
        );
        self.queue.submit(Some(done.finish()));
        if let Some(surface_tex) = mirror {
            surface_tex.present();
        }

        self.headset.as_mut().expect("checked above").finish_eye();
        true
    }

    /// Views needed per frame: both eyes in VR, one otherwise.
    #[cfg_attr(
        not(feature = "vr"),
        expect(
            clippy::unused_self,
            reason = "the VR build queries this presenter's headset"
        )
    )]
    #[cfg(not(feature = "display-sdl2"))]
    pub(crate) fn eye_count(&self) -> usize {
        #[cfg(feature = "vr")]
        if self.headset.is_some() {
            return vr::EYE_COUNT;
        }
        1
    }

    /// Ensure targets, composite, melt (when wiping), and a live encoder exist,
    /// snapshotting the previous frame on a wipe's first frame. Idempotent.
    fn ensure_frame(&mut self, buf_w: u32, buf_h: u32, win_w: u32, win_h: u32, output: (u32, u32)) {
        self.sync_sizes(win_w, win_h, output.0, output.1);
        if self
            .gpu
            .targets
            .as_ref()
            .is_none_or(|g| g.w != buf_w || g.h != buf_h)
        {
            self.gpu.targets = Some(GpuTargets::new(&self.device, buf_w, buf_h));
            if let Some(composite) = self.gpu.composite.as_mut() {
                composite.invalidate();
            }
        }
        if self.gpu.composite.is_none() {
            self.gpu.composite = Some(scene::Composite::new(&self.device, buf_w));
        }
        let wiping = self.melt_cols.is_some();
        if wiping {
            if self
                .gpu
                .melts
                .first()
                .is_some_and(|m| m.w != output.0 || m.h != output.1)
            {
                self.gpu.melts.clear();
            }
            while self.gpu.melts.len() < self.frames.len() {
                self.gpu
                    .melts
                    .push(Melt::new(&self.device, output.0, output.1));
            }
        }
        if self.encoder.is_none() {
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("gpu_frame_encoder"),
                });
            // Snapshot this view's previous frame before the composite
            // overwrites it — each view melts its own last image.
            let cur = self.cur_view();
            if self.wipe_capture_pending.get(cur).copied().unwrap_or(false)
                && let Some(melt) = self.gpu.melts.get(cur)
            {
                melt.capture(&mut encoder, &self.frames[cur].texture);
                self.wipe_capture_pending[cur] = false;
            }
            self.encoder = Some(encoder);
        }
    }
}

#[cfg(feature = "vr")]
impl GpuPresenter {
    /// Open the display's frame and locate both eyes; see
    /// [`Headset::begin_frame`].
    pub(crate) fn vr_begin_frame(&mut self) -> Option<[Option<EyeView>; EYE_COUNT]> {
        self.headset.as_mut()?.begin_frame()
    }

    pub(crate) fn vr_target_size(&self) -> Option<(u32, u32)> {
        self.headset.as_ref().map(|h| h.session.view_size())
    }

    /// Aim the next scene and present at `eye` of the frame already begun.
    pub(crate) fn set_eye(&mut self, eye: usize) {
        if let Some(headset) = self.headset.as_mut() {
            headset.set_eye(eye);
        }
    }

    /// Panel geometry from the user config.
    pub(crate) fn set_panels(&mut self, hud: PanelPlacement, screen: PanelPlacement) {
        if let Some(headset) = self.headset.as_mut() {
            headset.panels = [hud, screen];
        }
    }

    /// Pin the facing-locked panel to where the player is looking now.
    pub(crate) fn anchor_panels(&mut self) {
        if let Some(headset) = self.headset.as_mut() {
            headset.anchor_panels();
        }
    }

    /// The headset's field of view expressed as pixel proportions, so the
    /// caller's aspect maths needs no VR special case. `None` on a window.
    #[expect(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        reason = "the headset aspect ratio intentionally rounds to integer display pixels"
    )]
    pub(crate) fn vr_display_size(&self) -> Option<(u32, u32)> {
        let session = &self.headset.as_ref()?.session;
        let (_, eye_h) = session.view_size();
        let width = (eye_h as f32 * session.fov_aspect()).round() as u32;
        Some((width.max(1), eye_h.max(1)))
    }

    /// The live VR session, for callers that need more than a frame.
    pub(crate) fn vr_session_mut(&mut self) -> Option<&mut vr::Session> {
        self.headset.as_mut().map(|h| &mut h.session)
    }
}

#[cfg(all(test, feature = "wgpu3d"))]
mod tests {
    use super::*;
    use crate::backend::UiPixels;

    #[test]
    fn native_ui_detail_survives_low_resolution_scene_and_post_chain() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let Ok(adapter) =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        else {
            eprintln!("native UI GPU test skipped: no adapter");
            return;
        };
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                .expect("test device");
        let mut presenter = GpuPresenter::new(
            None,
            device,
            queue,
            surface_config(64, 48, false),
            vec![PostEffect::Stretch, PostEffect::Stretch],
            64,
            48,
        );
        for (scene, ui_size) in [
            ((4, 3), (64, 48)),
            ((64, 48), (32, 24)),
            ((64, 48), (64, 48)),
        ] {
            let pixels: Vec<u32> = (0u32..ui_size.0 * ui_size.1)
                .map(|i| {
                    if (i % ui_size.0).is_multiple_of(2) {
                        0xFFFF_FFFF
                    } else {
                        0xFF00_0000
                    }
                })
                .collect();
            presenter.upload_ui(UiPlanes {
                hud: Some(UiPixels {
                    pixels: &pixels,
                    w: ui_size.0,
                    h: ui_size.1,
                }),
                screen: None,
            });
            presenter.finish_frame(scene.0, scene.1, 64, 48, (64, 48), false);
            let targets = presenter.gpu.targets.as_ref().expect("scene targets");
            assert_eq!((targets.w, targets.h), scene);
            assert_eq!((presenter.frames[0].w, presenter.frames[0].h), (64, 48));
            let post = FrameTexture::new(&presenter.device, 64, 48);
            let mut encoder = presenter
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            presenter.chains[0].render(&mut encoder, &post.view, [0, 0, 64, 48]);
            presenter.queue.submit([encoder.finish()]);
            let bytes = readback(&presenter, &post.texture);
            for row in bytes.as_chunks::<256>().0 {
                for (x, pixel) in row.as_chunks::<4>().0.iter().enumerate() {
                    let expected = if (x * ui_size.0 as usize / 64).is_multiple_of(2) {
                        255
                    } else {
                        0
                    };
                    assert_eq!(
                        *pixel,
                        [expected, expected, expected, 255],
                        "UI texels at output column {x}"
                    );
                }
            }
        }
        presenter.upload_ui(UiPlanes {
            hud: None,
            screen: None,
        });
        presenter.finish_frame(4, 3, 64, 48, (64, 48), false);
        let bytes = readback(&presenter, &presenter.frames[0].texture);
        assert!(
            bytes
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| *p == [0, 0, 0, 255]),
            "stale UI cleared"
        );
    }

    fn readback(presenter: &GpuPresenter, texture: &wgpu::Texture) -> Vec<u8> {
        let buffer = presenter.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("native-ui-readback"),
            size: 256 * 48,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = presenter
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(48),
                },
            },
            wgpu::Extent3d {
                width: 64,
                height: 48,
                depth_or_array_layers: 1,
            },
        );
        presenter.queue.submit([encoder.finish()]);
        let slice = buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |result| result.expect("readback map"));
        presenter
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("readback poll");
        let bytes = slice.get_mapped_range().to_vec();
        buffer.unmap();
        bytes
    }
}
