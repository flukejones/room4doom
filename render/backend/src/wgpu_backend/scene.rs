//! Scene-side GPU resources for the `wgpu3d` renderer: the scene/depth
//! targets, the effects composite, and the melt wipe.

use std::slice::from_raw_parts;

use ::wgpu3d::SceneEffects;

use crate::wgpu_backend::FORMAT;
use crate::wgpu_backend::panel::{Panel, PanelPipeline};

/// Bytes per health-bleed column (`vec4<f32>`).
const BLEED_COL_BYTES: u64 = 16;

/// Size of the `wgpu3d::SceneEffects` uniform (32 bytes; layout-tested wgpu3d-side).
const SCENE_EFFECTS_BYTES: u64 = 32;

/// The GPU renderer's display-side resources, built lazily on the first GPU
/// frame and recreated on engine-buffer resize. Held by
/// [`GpuPresenter`](super::GpuPresenter) on the `wgpu3d` path.
#[derive(Default)]
pub(crate) struct GpuResources {
    /// Scene + depth textures. `None` until the first GPU frame.
    pub targets: Option<GpuTargets>,
    /// Scene-effects pipeline (tint, invuln, bleed). Built once with the device.
    pub composite: Option<Composite>,
    /// UI panel quad pipeline. Built once with the device.
    pub panels: Option<PanelPipeline>,
    /// `hud` is the whole UI on a flat display; `screen` exists only where the
    /// display wants a second plane.
    pub hud: Option<Panel>,
    pub screen: Option<Panel>,
    /// Whether each plane received content this frame; a clean plane is
    /// neither uploaded nor drawn.
    pub hud_live: bool,
    pub screen_live: bool,
    /// Melt-wipe pass per view (one for a window, one per eye in VR).
    /// Empty until the first wipe frame.
    pub melts: Vec<Melt>,
}

/// Scene colour + depth textures for the GPU renderer, both at the engine
/// buffer size. Recreated on resize.
pub(crate) struct GpuTargets {
    pub scene_view: wgpu::TextureView,
    pub depth_view: wgpu::TextureView,
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
        Self {
            scene_view,
            depth_view,
            w,
            h,
        }
    }
}

/// Composite draw: applies the scene colour effects (player tint, invuln,
/// health bleed) to the scene texture, into the frame texture the post chain
/// presents. The UI follows as panel quads in the same pass, so the effects
/// never touch it — matching vanilla, where the HUD stays untinted.
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
    /// Cached over the current scene view; dropped when the scene texture or
    /// the bleed buffer is recreated, rebuilt via [`Self::rebind`].
    bind_group: Option<wgpu::BindGroup>,
}

impl Composite {
    pub fn new(device: &wgpu::Device, width: u32) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("composite_bgl"),
            entries: &[
                tex_entry(0),
                bind_sampler_entry(1),
                uniform_entry(2),
                storage_entry(3),
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
            bind_group: None,
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
            self.bind_group = None;
        }
        if !cols.is_empty() {
            queue.write_buffer(&self.bleed, 0, bytemuck_cast_f32x4(cols));
        }
    }

    /// Drop the cached bind group; the next [`Self::rebind`] rebuilds it. Call
    /// when the scene texture is recreated.
    pub fn invalidate(&mut self) {
        self.bind_group = None;
    }

    /// Rebuild the cached bind group over `scene` if it was invalidated.
    pub fn rebind(&mut self, device: &wgpu::Device, scene: &wgpu::TextureView) {
        if self.bind_group.is_some() {
            return;
        }
        self.bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
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
                    resource: self.effects.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.bleed.as_entire_binding(),
                },
            ],
        }));
    }

    /// Record the full-screen effects draw into the caller's pass.
    pub fn record(&self, rpass: &mut wgpu::RenderPass<'_>) {
        let bind_group = self.bind_group.as_ref().expect("rebound before record");
        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, bind_group, &[]);
        rpass.draw(0..3, 0..1);
    }
}

/// GPU column-melt wipe. Snapshots the composited frame at wipe start, then each
/// frame slides the snapshot's columns down (per `col_y`) over the new frame.
/// `scratch_view` holds the new composited frame during a wipe; the melt slides
/// the snapshot over it into the frame texture the post chain presents.
pub(crate) struct Melt {
    pipeline: wgpu::RenderPipeline,
    snapshot: wgpu::Texture,
    pub scratch_view: wgpu::TextureView,
    /// Per-column melt offset (`i32` px), re-uploaded each wiping frame.
    offsets: wgpu::Buffer,
    /// Samples scratch + snapshot + offsets, all owned here, so it is built
    /// once with them.
    bind_group: wgpu::BindGroup,
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
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("melt_bg"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&scratch_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&snapshot_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: offsets.as_entire_binding(),
                },
            ],
        });
        Self {
            pipeline,
            snapshot,
            scratch_view,
            offsets,
            bind_group,
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

    /// Slide the snapshot columns down by `offsets` over the scratch into
    /// `out`. `out` must differ from the scratch (no read-while-write).
    pub fn render(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        out: &wgpu::TextureView,
        offsets: &[i32],
    ) {
        queue.write_buffer(&self.offsets, 0, melt_offset_bytes(offsets));
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
        rpass.set_bind_group(0, &self.bind_group, &[]);
        rpass.draw(0..3, 0..1);
    }
}

/// Clear a texture view to opaque black via a load-clear render pass (no draws).
pub(crate) fn clear_view(encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
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

pub(crate) fn tex_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
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

pub(crate) fn bind_sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}
