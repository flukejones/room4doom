//! wgpu display backend — presents the engine framebuffer via winit + wgpu.
//!
//! Owns the wgpu device/queue/surface directly (no `pixels` wrapper).
//!
//! Each frame uploads the engine `Bgra8Unorm` framebuffer into a texture, then
//! runs the [`PostChain`] — an ordered list of full-screen passes (stretch, CRT,
//! …) where the last targets the surface.

#[cfg(feature = "wgpu3d")]
pub(crate) mod gpu;

use std::slice::from_raw_parts;
#[cfg(not(feature = "display-sdl2"))]
use std::sync::Arc;

#[cfg(all(feature = "wgpu3d", not(feature = "display-sdl2")))]
use ::wgpu3d::GpuHandle;
#[cfg(feature = "wgpu3d")]
use ::wgpu3d::SceneEffects;
#[cfg(feature = "hprof")]
use coarse_prof::profile;
#[cfg(any(not(feature = "display-sdl2"), feature = "wgpu3d"))]
use pic_data::PixelFmt;
use wgpu::CurrentSurfaceTexture::{Suboptimal, Success};
#[cfg(not(feature = "display-sdl2"))]
use winit::window::{Fullscreen, Window};

#[cfg(not(feature = "display-sdl2"))]
use crate::backend::{Backend, RenderKind, SoftwarePresent};
#[cfg(all(feature = "wgpu3d", not(feature = "display-sdl2")))]
use crate::backend::{HardwarePresent, ScreenEffects};

/// Engine framebuffer + surface pixel format. The engine writes `0xFFRRGGBB`,
/// whose little-endian bytes are `[BB,GG,RR,FF]` = BGRA, so `Bgra8Unorm` is a
/// straight upload. Non-sRGB: the palette is already gamma-baked.
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;

/// Bytes per framebuffer pixel (`u32`). Used by the software-present upload (the
/// winit wgpu backend; sdl2's hardware mode uses only the GPU present path).
#[cfg(not(feature = "display-sdl2"))]
const PIXEL_BYTES: u32 = size_of::<u32>() as u32;

/// A full-screen post-process effect.
///
/// Each runs as one pass that samples the previous stage's texture and draws a
/// full-screen triangle into the next. Stackable: the chain runs them in order,
/// the last pass targeting the surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostEffect {
    /// Nearest-neighbour upscale, sharp pixels.
    Stretch,
    /// CRT emulation (crt-lottes): scanlines, mask, warp, bloom.
    Crt,
}

impl PostEffect {
    /// Linear filtering suits the CRT's sub-texel sampling; Stretch wants
    /// nearest for crisp pixels.
    fn linear(self) -> bool {
        matches!(self, Self::Crt)
    }
}

/// One post-process pass: a pipeline plus the bind group for its input texture.
struct PostPass {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
}

/// Ordered post-process chain. Pass 0 samples the engine framebuffer; each
/// later pass samples the previous pass's intermediate texture; the last pass
/// targets the surface. Intermediates are surface-sized and ping-pong.
struct PostChain {
    effects: Vec<PostEffect>,
    bind_group_layout: wgpu::BindGroupLayout,
    nearest: wgpu::Sampler,
    linear: wgpu::Sampler,
    /// One pipeline per [`PostEffect`] kind, built once.
    pipelines: Vec<(PostEffect, wgpu::RenderPipeline)>,
    /// Surface-sized intermediate targets, one fewer than passes (last pass
    /// writes the surface). Recreated on resize. Empty for a single pass.
    intermediates: Vec<TargetTexture>,
    /// Per-pass bind groups, rebuilt when input textures change (resize).
    passes: Vec<PostPass>,
}

/// A renderable + samplable target texture (intermediate stage output).
struct TargetTexture {
    view: wgpu::TextureView,
}

impl TargetTexture {
    fn new(device: &wgpu::Device, w: u32, h: u32) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("post_intermediate"),
            size: wgpu::Extent3d {
                width: w.max(1),
                height: h.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        Self {
            view: texture.create_view(&wgpu::TextureViewDescriptor::default()),
        }
    }
}

impl PostChain {
    fn new(
        device: &wgpu::Device,
        effects: Vec<PostEffect>,
        frame_view: &wgpu::TextureView,
        surface: (u32, u32),
    ) -> Self {
        let effects = if effects.is_empty() {
            vec![PostEffect::Stretch]
        } else {
            effects
        };

        let bind_group_layout = post_bind_group_layout(device);

        let make_sampler = |linear: bool| {
            let f = if linear {
                wgpu::FilterMode::Linear
            } else {
                wgpu::FilterMode::Nearest
            };
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("post_sampler"),
                mag_filter: f,
                min_filter: f,
                mipmap_filter: wgpu::MipmapFilterMode::Nearest,
                ..Default::default()
            })
        };
        let nearest = make_sampler(false);
        let linear = make_sampler(true);

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("post_pipeline_layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let mut pipelines: Vec<(PostEffect, wgpu::RenderPipeline)> = Vec::new();
        for &e in &effects {
            if pipelines.iter().any(|(k, _)| *k == e) {
                continue;
            }
            pipelines.push((e, build_pipeline(device, &layout, e)));
        }

        let mut chain = Self {
            effects,
            bind_group_layout,
            nearest,
            linear,
            pipelines,
            intermediates: Vec::new(),
            passes: Vec::new(),
        };
        chain.resize(device, frame_view, surface);
        chain
    }

    /// Recreate intermediate targets at the surface size and rebuild every
    /// pass's bind group over its input texture. Called on init and resize.
    fn resize(
        &mut self,
        device: &wgpu::Device,
        frame_view: &wgpu::TextureView,
        surface: (u32, u32),
    ) {
        let n = self.effects.len();
        self.intermediates = (0..n.saturating_sub(1))
            .map(|_| TargetTexture::new(device, surface.0, surface.1))
            .collect();

        self.passes = (0..n)
            .map(|i| {
                let effect = self.effects[i];
                let input = if i == 0 {
                    frame_view
                } else {
                    &self.intermediates[i - 1].view
                };
                let sampler = if effect.linear() {
                    &self.linear
                } else {
                    &self.nearest
                };
                let pipeline = self
                    .pipelines
                    .iter()
                    .find(|(k, _)| *k == effect)
                    .map(|(_, p)| p.clone())
                    .expect("pipeline built for effect");
                let bind_group =
                    texture_bind_group(device, &self.bind_group_layout, input, sampler);
                PostPass {
                    pipeline,
                    bind_group,
                }
            })
            .collect();
    }

    /// Run the chain: each pass draws into the next intermediate; the last into
    /// `surface`.
    fn render(&self, encoder: &mut wgpu::CommandEncoder, surface: &wgpu::TextureView) {
        let last = self.passes.len() - 1;
        for (i, pass) in self.passes.iter().enumerate() {
            let target = if i == last {
                surface
            } else {
                &self.intermediates[i].view
            };
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("post_pass"),
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
            rpass.set_pipeline(&pass.pipeline);
            rpass.set_bind_group(0, &pass.bind_group, &[]);
            rpass.draw(0..3, 0..1);
        }
    }
}

/// Bind-group layout shared by every post pass: one sampled texture (the
/// previous stage's output) plus its sampler, both fragment-visible.
fn post_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("post_bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float {
                        filterable: true,
                    },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

/// Bind a sampled texture `view` + `sampler` against [`post_bind_group_layout`].
fn texture_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("post_bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

/// Build a full-screen-triangle pipeline for `effect`. All effects share the
/// `vs_main`/`fs_main` entry points and the single texture+sampler bind layout.
fn build_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    effect: PostEffect,
) -> wgpu::RenderPipeline {
    let shader = match effect {
        PostEffect::Stretch => device.create_shader_module(wgpu::include_wgsl!("stretch.wgsl")),
        PostEffect::Crt => device.create_shader_module(wgpu::include_wgsl!("lottes-crt.wgsl")),
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("post_pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: FORMAT,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        multiview_mask: None,
        cache: None,
    })
}

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
    #[cfg(not(feature = "display-sdl2"))]
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

/// Reinterpret a `&[u32]` as a byte slice for upload (no copy).
#[inline]
fn bytemuck_cast(px: &[u32]) -> &[u8] {
    // SAFETY: u32 is 4 contiguous bytes; the resulting slice covers the same
    // bytes with 4x the length and tighter (byte) alignment.
    unsafe { from_raw_parts(px.as_ptr().cast::<u8>(), size_of_val(px)) }
}

/// Reinterpret the engine `[P]` framebuffer as `[u32]` for the wgpu upload. The
/// wgpu backend is `u32`-only (`Bgra8Unorm`); `P` is always `u32` here (asserted).
#[cfg(any(not(feature = "display-sdl2"), feature = "wgpu3d"))]
#[inline]
pub(crate) fn as_u32_slice<P: PixelFmt>(front: &[P]) -> &[u32] {
    assert_eq!(
        size_of::<P>(),
        size_of::<u32>(),
        "wgpu backend is u32-only; Rgb565 must fall back to Rgb888"
    );
    // SAFETY: P == u32 (asserted); `front` is the tight engine framebuffer.
    unsafe { from_raw_parts(front.as_ptr().cast::<u32>(), front.len()) }
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

/// Window-agnostic wgpu present pipeline: owns the device/queue/surface, the
/// framebuffer texture, and the post-process chain. Hosts both the CPU software
/// renderers (uploads their `[P]` front buffer) and the GPU `wgpu3d` renderer
/// (scene/UI textures + composite/melt shaders). Presentation only — the
/// renderers own nothing here, and neither does the window (the backend does).
pub(crate) struct GpuPresenter {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    frame: FrameTexture,
    chain: PostChain,
    /// Last configured surface size; reconfigure only on change (Metal
    /// `surface.configure()` is a pipeline stall).
    surface_size: (u32, u32),
    /// GPU renderer display-side resources (scene/UI targets, composite, melt);
    /// built lazily on the GPU path. See [`gpu::GpuResources`].
    #[cfg(feature = "wgpu3d")]
    gpu: gpu::GpuResources,
    /// Per-column health-bleed shape, resolved into `bleed_cols` for the
    /// composite shader. Owned here — it is GPU presentation state.
    #[cfg(feature = "wgpu3d")]
    bleed: render_common::HealthBleed,
    /// Per-column bleed upload scratch (`vec4<f32>` per column).
    #[cfg(feature = "wgpu3d")]
    bleed_cols: Vec<[f32; 4]>,
    /// Melt-wipe column offsets; `None` until a wipe begins, re-seeded each wipe.
    #[cfg(feature = "wgpu3d")]
    melt_cols: Option<render_common::wipe::MeltColumns>,
    /// Live command encoder for the frame in flight, `None` between frames.
    /// `wgpu`'s encoder is `'static`, so it can span the scene → UI → present
    /// calls; consumed by `finish_frame`.
    #[cfg(feature = "wgpu3d")]
    encoder: Option<wgpu::CommandEncoder>,
    /// Effects resolved by `set_effects`; reset to default each `finish_frame`.
    #[cfg(feature = "wgpu3d")]
    frame_effects: SceneEffects,
    /// Wipe began this frame → snapshot the old frame. Set by `start_wipe`.
    #[cfg(feature = "wgpu3d")]
    wipe_just_started: bool,
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
        surface: wgpu::Surface<'static>,
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
            surface,
            device,
            queue,
            config,
            frame,
            chain,
            surface_size: (win_w, win_h),
            #[cfg(feature = "wgpu3d")]
            gpu: gpu::GpuResources::default(),
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
            wipe_just_started: false,
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
        Self::new(surface, device, queue, config, post, win_w, win_h)
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
        Self::new(surface, device, queue, config, post, win_w, win_h)
    }

    /// Reset the health-bleed pattern (new game/level → fresh shape on the next
    /// damaged frame).
    #[cfg(feature = "wgpu3d")]
    pub(crate) fn reset_health_bleed(&mut self) {
        self.bleed.reset();
    }

    /// Upload the `buf_w`×`buf_h` front buffer (tight pitch `buf_w`) and run the
    /// post-chain to the window (`win_w`×`win_h`). u32-only (`Bgra8Unorm`): `P`
    /// must be `u32` (asserted), reinterpreted as `[u32]` for upload.
    #[cfg(not(feature = "display-sdl2"))]
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
        self.frame.upload(&self.queue, as_u32_slice(front));

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
        self.chain.render(&mut encoder, &view);
        self.queue.submit(Some(encoder.finish()));
        surface_tex.present();
    }

    /// Resize the framebuffer texture to the engine buffer (`buf_w`×`buf_h`) and
    /// the surface to the window (`win_w`×`win_h`) when either changed, rebuilding
    /// the post chain over the new inputs. Reconfigures the surface only on change
    /// (Metal `surface.configure()` is a pipeline stall).
    fn sync_sizes(&mut self, win_w: u32, win_h: u32, buf_w: u32, buf_h: u32) {
        let frame_changed = self.frame.w != buf_w || self.frame.h != buf_h;
        if frame_changed {
            self.frame = FrameTexture::new(&self.device, buf_w, buf_h);
        }
        let win = (win_w.max(1), win_h.max(1));
        let surface_changed = win != self.surface_size;
        if surface_changed {
            self.config.width = win.0;
            self.config.height = win.1;
            self.surface.configure(&self.device, &self.config);
            self.surface_size = win;
        }
        if frame_changed || surface_changed {
            self.chain
                .resize(&self.device, &self.frame.view, self.surface_size);
        }
    }

    /// Acquire the next swapchain texture, reconfiguring and retrying once if the
    /// surface is lost/outdated. `None` drops the frame.
    fn acquire_surface(&self) -> Option<wgpu::SurfaceTexture> {
        match self.surface.get_current_texture() {
            Success(t) | Suboptimal(t) => Some(t),
            _ => {
                self.surface.configure(&self.device, &self.config);
                match self.surface.get_current_texture() {
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

/// wgpu display backend: a winit window plus the window-agnostic
/// [`GpuPresenter`]. Implements the [`Backend`] + present traits, sourcing the
/// window size for each present and owning the fullscreen toggle. Hosts both the
/// software renderers (CPU upload) and the GPU `wgpu3d` renderer. `u32`-only; the
/// `P` parameter (always `u32` here) keeps `ActiveBackend<P>` uniform. The winit
/// display backend; when sdl2 is the active backend this is unused.
#[cfg(not(feature = "display-sdl2"))]
pub struct WgpuBackend<P: PixelFmt> {
    window: Arc<Window>,
    presenter: GpuPresenter,
    _p: std::marker::PhantomData<P>,
}

#[cfg(not(feature = "display-sdl2"))]
impl<P: PixelFmt> WgpuBackend<P> {
    /// Create from a winit window. The window must be wrapped in `Arc`; its clone
    /// becomes the wgpu surface target.
    pub(crate) fn new(window: Arc<Window>, vsync: bool, post: Vec<PostEffect>) -> Self {
        let size = window.inner_size();
        let (sw, sh) = (size.width.max(1), size.height.max(1));
        let presenter = GpuPresenter::from_target(window.clone(), vsync, post, sw, sh);
        Self {
            window,
            presenter,
            _p: std::marker::PhantomData,
        }
    }

    /// Window inner size, clamped to a minimum of 1×1 for surface configuration.
    fn window_inner(&self) -> (u32, u32) {
        let size = self.window.inner_size();
        (size.width.max(1), size.height.max(1))
    }
}

#[cfg(not(feature = "display-sdl2"))]
impl<P: PixelFmt> Backend for WgpuBackend<P> {
    fn window_size(&self) -> (u32, u32) {
        let size = self.window.inner_size();
        (size.width, size.height)
    }

    fn set_fullscreen(&mut self, mode: u8) {
        let fs = match mode {
            1 => Some(Fullscreen::Borderless(None)),
            2 => {
                let monitor = self
                    .window
                    .current_monitor()
                    .or_else(|| self.window.primary_monitor());
                monitor
                    .and_then(|m| m.video_modes().next())
                    .map(Fullscreen::Exclusive)
            }
            _ => None,
        };
        self.window.set_fullscreen(fs);
    }

    fn supports(&self, _: RenderKind) -> bool {
        true
    }
}

#[cfg(not(feature = "display-sdl2"))]
impl<P: PixelFmt> SoftwarePresent<P> for WgpuBackend<P> {
    fn present(&mut self, front: &[P], w: u32, h: u32) {
        let (win_w, win_h) = self.window_inner();
        self.presenter.present_software(front, w, h, win_w, win_h);
    }
}

#[cfg(all(feature = "wgpu3d", not(feature = "display-sdl2")))]
impl<P: PixelFmt> HardwarePresent<P> for WgpuBackend<P> {
    fn set_screen_effects(&mut self, effects: ScreenEffects, w: u32, h: u32) {
        self.presenter.set_effects(effects, w, h);
    }

    fn start_wipe(&mut self, w: u32, h: u32) {
        self.presenter.start_wipe(w, h);
    }

    fn is_wiping(&self) -> bool {
        self.presenter.is_wiping()
    }

    fn begin_scene(&mut self, w: u32, h: u32) -> GpuHandle<'_> {
        let (win_w, win_h) = self.window_inner();
        self.presenter.begin_scene(w, h, win_w, win_h)
    }

    fn advance_wipe(&mut self) -> bool {
        self.presenter.advance_wipe()
    }

    fn finish_frame(&mut self, front: &[P], w: u32, h: u32, wiping: bool) {
        let (win_w, win_h) = self.window_inner();
        let ui = as_u32_slice(front);
        self.presenter.finish_frame(ui, w, h, win_w, win_h, wiping);
    }

    fn reset_health_bleed(&mut self) {
        self.presenter.reset_health_bleed();
    }
}

#[cfg(test)]
mod tests {
    /// Parse + validate the backend WGSL passes offline (no GPU): catches syntax
    /// and the composite uniform layout/type faults before they reach the device.
    #[test]
    fn shaders_validate() {
        for (name, src) in [
            ("composite", include_str!("composite.wgsl")),
            ("melt", include_str!("melt.wgsl")),
            ("stretch", include_str!("stretch.wgsl")),
            ("lottes-crt", include_str!("lottes-crt.wgsl")),
        ] {
            let module = naga::front::wgsl::parse_str(src)
                .unwrap_or_else(|e| panic!("{name} parses: {e:?}"));
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::all(),
            )
            .validate(&module)
            .unwrap_or_else(|e| panic!("{name} validates: {e:?}"));
        }
    }
}
