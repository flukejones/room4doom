use std::slice::from_raw_parts;

use ::wgpu3d::{GpuHandle, SceneEffects};
#[cfg(feature = "hprof")]
use coarse_prof::profile;
use render_common::wipe::MeltColumns;

use crate::backend::ScreenEffects;
use crate::wgpu_backend::{FORMAT, GpuPresenter, bytemuck_cast};

/// Bytes per health-bleed column (`vec4<f32>`).
const BLEED_COL_BYTES: u64 = 16;

/// Size of the `wgpu3d::SceneEffects` uniform (32 bytes; layout-tested wgpu3d-side).
const SCENE_EFFECTS_BYTES: u64 = 32;

/// The GPU renderer's display-side resources, built lazily on the first GPU
/// frame and recreated on engine-buffer resize. Held by [`GpuPresenter`] on the
/// `wgpu3d` path.
#[derive(Default)]
pub(crate) struct GpuResources {
    /// Scene/depth/UI textures. `None` until the first GPU frame.
    pub targets: Option<GpuTargets>,
    /// Composite pipeline (scene + UI over). Built once with the device.
    pub composite: Option<Composite>,
    /// Melt-wipe pass. `None` until the first wipe frame.
    pub melt: Option<Melt>,
}

/// Scene colour + depth + UI textures for the GPU renderer, all at the engine
/// buffer size. Recreated on resize.
pub(crate) struct GpuTargets {
    pub scene_view: wgpu::TextureView,
    pub depth_view: wgpu::TextureView,
    ui_texture: wgpu::Texture,
    pub ui_view: wgpu::TextureView,
    pub w: u32,
    pub h: u32,
}

impl GpuTargets {
    pub fn new(device: &wgpu::Device, w: u32, h: u32) -> Self {
        let make = |label, format, usage| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: w,
                        height: h,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
                .create_view(&wgpu::TextureViewDescriptor::default())
        };
        let scene_view = make(
            "gpu_scene",
            wgpu3d::SCENE_FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let depth_view = make(
            "gpu_depth",
            wgpu3d::DEPTH_FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        );
        let ui_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gpu_ui"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let ui_view = ui_texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            scene_view,
            depth_view,
            ui_texture,
            ui_view,
            w,
            h,
        }
    }

    /// Upload the UI ARGB pixels into the UI texture.
    pub fn upload_ui(&self, queue: &wgpu::Queue, pixels: &[u32]) {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.ui_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck_cast(pixels),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(self.w * 4),
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

/// Composite pass: applies the scene colour effects (player tint + invuln) to
/// the scene texture, then blends the UI texture over it, into the frame texture
/// the [`PostChain`] presents.
pub(crate) struct Composite {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// `wgpu3d::SceneEffects` uniform, uploaded each frame before the pass.
    effects: wgpu::Buffer,
    /// Per-column health-bleed `vec4<f32>` (`[shown, bound0, bound1, _]`, px),
    /// re-uploaded each frame; one element per scene-texture column. Grown on
    /// resize.
    bleed: wgpu::Buffer,
    bleed_cols: u32,
}

impl Composite {
    pub fn new(device: &wgpu::Device, width: u32) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("composite_bgl"),
            entries: &[
                tex_entry(0),
                bind_sampler_entry(1),
                tex_entry(2),
                bind_sampler_entry(3),
                uniform_entry(4),
                storage_entry(5),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("composite_pipeline_layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("composite.wgsl"));
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("composite_pipeline"),
            layout: Some(&pipeline_layout),
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
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("composite_sampler"),
            ..Default::default()
        });
        let effects = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("composite_effects"),
            size: SCENE_EFFECTS_BYTES,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bleed_cols = width.max(1);
        let bleed = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("composite_bleed"),
            size: bleed_cols as u64 * BLEED_COL_BYTES,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline,
            layout,
            sampler,
            effects,
            bleed,
            bleed_cols,
        }
    }

    /// Upload the scene-effect parameters for the next pass.
    pub fn set_effects(&self, queue: &wgpu::Queue, effects: &SceneEffects) {
        queue.write_buffer(&self.effects, 0, effects.as_bytes());
    }

    /// Upload the per-column health-bleed geometry; grow the buffer if the
    /// scene width changed. `cols` is `[shown, bound0, bound1, _]` (px) per
    /// column, sized to the scene width.
    pub fn set_bleed(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, cols: &[[f32; 4]]) {
        let need = cols.len() as u32;
        if need > self.bleed_cols {
            self.bleed_cols = need;
            self.bleed = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("composite_bleed"),
                size: need as u64 * BLEED_COL_BYTES,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        if !cols.is_empty() {
            queue.write_buffer(&self.bleed, 0, bytemuck_cast_f32x4(cols));
        }
    }

    /// Apply the scene effects and blend `ui` over `scene` into `out`.
    pub fn render(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        scene: &wgpu::TextureView,
        ui: &wgpu::TextureView,
        out: &wgpu::TextureView,
    ) {
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("composite_bg"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(scene),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(ui),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: self.effects.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: self.bleed.as_entire_binding(),
                },
            ],
        });
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("composite_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: out,
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
        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, &bind_group, &[]);
        rpass.draw(0..3, 0..1);
    }
}

/// GPU column-melt wipe. Snapshots the composited frame at wipe start, then each
/// frame slides the snapshot's columns down (per `col_y`) over the new frame.
/// `scratch_view` holds the new composited frame during a wipe; the melt slides
/// the snapshot over it into the frame texture the [`PostChain`] presents.
pub(crate) struct Melt {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    snapshot: wgpu::Texture,
    snapshot_view: wgpu::TextureView,
    pub scratch_view: wgpu::TextureView,
    /// Per-column melt offset (`i32` px), re-uploaded each wiping frame.
    offsets: wgpu::Buffer,
    pub w: u32,
    pub h: u32,
}

impl Melt {
    pub fn new(device: &wgpu::Device, w: u32, h: u32) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("melt_bgl"),
            entries: &[
                tex_entry(0),
                bind_sampler_entry(1),
                tex_entry(2),
                storage_entry(3),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("melt_pipeline_layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("melt.wgsl"));
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("melt_pipeline"),
            layout: Some(&pipeline_layout),
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
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("melt_sampler"),
            ..Default::default()
        });
        let make_tex = |label| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
        };
        let snapshot = make_tex("melt_snapshot");
        let snapshot_view = snapshot.create_view(&wgpu::TextureViewDescriptor::default());
        let scratch_view =
            make_tex("melt_scratch").create_view(&wgpu::TextureViewDescriptor::default());
        let offsets = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("melt_offsets"),
            size: w.max(1) as u64 * size_of::<i32>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline,
            layout,
            sampler,
            snapshot,
            snapshot_view,
            scratch_view,
            offsets,
            w,
            h,
        }
    }

    /// Copy the just-composited frame into the snapshot as the wipe's old frame.
    pub fn capture(&self, encoder: &mut wgpu::CommandEncoder, frame: &wgpu::Texture) {
        encoder.copy_texture_to_texture(
            frame.as_image_copy(),
            self.snapshot.as_image_copy(),
            wgpu::Extent3d {
                width: self.w,
                height: self.h,
                depth_or_array_layers: 1,
            },
        );
    }

    /// Slide the snapshot columns down by `offsets` over `new` into `out`.
    /// `new` must differ from `out` (no read-while-write).
    pub fn render(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        new: &wgpu::TextureView,
        out: &wgpu::TextureView,
        offsets: &[i32],
    ) {
        queue.write_buffer(&self.offsets, 0, melt_offset_bytes(offsets));
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("melt_bg"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(new),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&self.snapshot_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.offsets.as_entire_binding(),
                },
            ],
        });
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("melt_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: out,
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
        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, &bind_group, &[]);
        rpass.draw(0..3, 0..1);
    }
}

/// Clear a texture view to opaque black via a load-clear render pass (no draws).
/// Used to wipe the stale scene texture on a UI-only (non-`Level`) frame.
fn clear_view(encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("scene_clear_pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
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
}

/// Reinterpret `&[i32]` as bytes for upload (no copy).
#[inline]
fn melt_offset_bytes(offsets: &[i32]) -> &[u8] {
    // SAFETY: i32 is 4 contiguous bytes; the slice covers the same bytes at byte
    // (tighter) alignment.
    unsafe { from_raw_parts(offsets.as_ptr().cast::<u8>(), size_of_val(offsets)) }
}

fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn storage_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage {
                read_only: true,
            },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

/// Reinterpret `&[[f32; 4]]` as bytes for upload (no copy).
#[inline]
fn bytemuck_cast_f32x4(cols: &[[f32; 4]]) -> &[u8] {
    // SAFETY: `[f32; 4]` is 16 contiguous bytes; the slice covers the same bytes
    // at byte (tighter) alignment.
    unsafe { from_raw_parts(cols.as_ptr().cast::<u8>(), size_of_val(cols)) }
}

fn tex_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float {
                filterable: true,
            },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn bind_sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}

impl GpuPresenter {
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
        self.wipe_just_started = true;
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
    ) -> GpuHandle<'_> {
        self.ensure_frame(buf_w, buf_h, win_w, win_h);
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

    /// Consume the held encoder: upload `ui_pixels` as the UI texture, composite
    /// over the scene, melt at the current offsets when `wiping`, post-chain, and
    /// present. Resets per-frame state; offsets are stepped by [`Self::advance_wipe`].
    pub(crate) fn finish_frame(
        &mut self,
        ui_pixels: &[u32],
        buf_w: u32,
        buf_h: u32,
        win_w: u32,
        win_h: u32,
        wiping: bool,
    ) {
        #[cfg(feature = "hprof")]
        profile!("wgpu_gpu_frame");

        // A UI-only frame (no player view) still needs targets/encoder.
        self.ensure_frame(buf_w, buf_h, win_w, win_h);
        let scene_recorded = self.scene_recorded;
        let mut encoder = self.encoder.take().expect("encoder built above");

        let gpu = self.gpu.targets.as_mut().expect("gpu targets built above");
        let composite = self.gpu.composite.as_mut().expect("composite built above");

        // Non-Level frames record no scene; clear the stale one so it doesn't show
        // through the transparent UI borders.
        if !scene_recorded {
            clear_view(&mut encoder, &gpu.scene_view);
        }

        gpu.upload_ui(&self.queue, ui_pixels);
        composite.set_effects(&self.queue, &self.frame_effects);
        composite.set_bleed(&self.device, &self.queue, &self.bleed_cols);

        // Wiping: composite into the melt scratch, then slide the snapshot over it
        // into `self.frame`. Otherwise composite straight into `self.frame`.
        let melt = if wiping {
            self.melt_cols.as_ref().and(self.gpu.melt.as_ref())
        } else {
            None
        };
        let composite_target = match melt {
            Some(m) => &m.scratch_view,
            None => &self.frame.view,
        };
        composite.render(
            &self.device,
            &mut encoder,
            &gpu.scene_view,
            &gpu.ui_view,
            composite_target,
        );
        if let Some(m) = melt {
            let offsets = self.melt_cols.as_ref().expect("melt active").offsets();
            m.render(
                &self.device,
                &self.queue,
                &mut encoder,
                &m.scratch_view,
                &self.frame.view,
                offsets,
            );
        }

        if let Some(surface_tex) = self.acquire_surface() {
            let view = surface_tex
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            self.chain.render(&mut encoder, &view);
            self.queue.submit(Some(encoder.finish()));
            surface_tex.present();
        }

        self.frame_effects = SceneEffects::default();
        self.wipe_just_started = false;
        self.scene_recorded = false;
    }

    /// Ensure targets, composite, melt (when wiping), and a live encoder exist,
    /// snapshotting the previous frame on a wipe's first frame. Idempotent.
    fn ensure_frame(&mut self, buf_w: u32, buf_h: u32, win_w: u32, win_h: u32) {
        self.sync_sizes(win_w, win_h, buf_w, buf_h);
        if self
            .gpu
            .targets
            .as_ref()
            .is_none_or(|g| g.w != buf_w || g.h != buf_h)
        {
            self.gpu.targets = Some(GpuTargets::new(&self.device, buf_w, buf_h));
        }
        if self.gpu.composite.is_none() {
            self.gpu.composite = Some(Composite::new(&self.device, buf_w));
        }
        let wiping = self.melt_cols.is_some();
        if wiping
            && self
                .gpu
                .melt
                .as_ref()
                .is_none_or(|m| m.w != buf_w || m.h != buf_h)
        {
            self.gpu.melt = Some(Melt::new(&self.device, buf_w, buf_h));
        }
        if self.encoder.is_none() {
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("gpu_frame_encoder"),
                });
            // Snapshot the previous frame before this frame's composite overwrites it.
            if self.wipe_just_started
                && let Some(melt) = &self.gpu.melt
            {
                melt.capture(&mut encoder, &self.frame.texture);
            }
            self.encoder = Some(encoder);
        }
    }
}
