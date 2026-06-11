//! Scene pass: pulls shared positions + per-corner UV from storage buffers and
//! samples the wall/flat atlases. Non-indexed draw over triangle corners, so
//! positions are stored once (no duplication) yet each corner has its own UV.

use bytemuck::cast_slice;
use wgpu::Face;
use wgpu::util::DeviceExt as _;

use crate::assets::{Atlas, AtlasRect};
use crate::camera::CameraUniform;
use crate::geometry::{CornerAttr, Mesh, Position};
use crate::light::LightParams;
use crate::shaders::{
    bind_buf_entry, bind_sampler_entry, bind_storage_entry, bind_tex_2d_entry,
    bind_tex_array_entry, bind_uniform_entry,
};

/// Scene colour texture format. Matches the surface (`Bgra8Unorm`) so the
/// composite samples it directly.
pub const SCENE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;
/// Scene depth format.
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// RGBA atlas-array texture format (walls/flats/sprites).
pub(crate) const ATLAS_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// GPU pipeline + camera uniform for the scene pass.
pub struct ScenePipeline {
    pipeline: wgpu::RenderPipeline,
    camera_buf: wgpu::Buffer,
    camera_bind: wgpu::BindGroup,
    light_buf: wgpu::Buffer,
    light_bind: wgpu::BindGroup,
    mesh_layout: wgpu::BindGroupLayout,
    atlas_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

/// Bind-group layout (group 3) the scene pipeline uses to sample the shared sky
/// for sky-flagged walls. Structurally identical to [`crate::sky::Sky`]'s layout
/// so the Sky's bind group is compatible.
pub fn sky_bind_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("scene_sky_bgl"),
        entries: &[
            bind_uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT),
            bind_tex_2d_entry(1),
            bind_sampler_entry(2, wgpu::SamplerBindingType::Filtering),
        ],
    })
}

impl ScenePipeline {
    pub fn new(device: &wgpu::Device) -> Self {
        let camera_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scene_camera"),
            size: size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene_camera_bgl"),
            entries: &[bind_uniform_entry(0, wgpu::ShaderStages::VERTEX)],
        });
        let camera_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene_camera_bg"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buf.as_entire_binding(),
            }],
        });

        // Light params uniform (group 4): runtime gamma/falloff, shared model
        // with the sprite pass. Used in both vertex (start_row) and fragment.
        let light_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scene_light"),
            size: size_of::<LightParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let light_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene_light_bgl"),
            entries: &[bind_uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT)],
        });
        let light_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene_light_bg"),
            layout: &light_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: light_buf.as_entire_binding(),
            }],
        });

        // positions (dyn), corner_index (static), corner_attr (dyn: switches),
        // uv (dyn), sector_light (dyn), corner_scroll (dyn: scrollers).
        let mesh_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene_mesh_bgl"),
            entries: &[
                bind_storage_entry(0, wgpu::ShaderStages::VERTEX),
                bind_storage_entry(1, wgpu::ShaderStages::VERTEX),
                bind_storage_entry(2, wgpu::ShaderStages::VERTEX),
                bind_storage_entry(3, wgpu::ShaderStages::VERTEX),
                bind_storage_entry(4, wgpu::ShaderStages::VERTEX),
                bind_storage_entry(5, wgpu::ShaderStages::VERTEX),
            ],
        });
        let atlas_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene_atlas_bgl"),
            entries: &[
                bind_tex_array_entry(0),
                bind_tex_array_entry(1),
                bind_storage_entry(2, wgpu::ShaderStages::FRAGMENT),
                bind_storage_entry(3, wgpu::ShaderStages::FRAGMENT),
                bind_sampler_entry(4, wgpu::SamplerBindingType::NonFiltering),
                // wall/flat translation tables (animation: base id -> frame id).
                bind_storage_entry(5, wgpu::ShaderStages::FRAGMENT),
                bind_storage_entry(6, wgpu::ShaderStages::FRAGMENT),
            ],
        });

        let sky_layout = sky_bind_layout(device);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scene_shader"),
            source: wgpu::ShaderSource::Wgsl(crate::shaders::SCENE_SRC.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("scene_pipeline_layout"),
            bind_group_layouts: &[
                Some(&camera_layout),
                Some(&mesh_layout),
                Some(&atlas_layout),
                Some(&sky_layout),
                Some(&light_layout),
            ],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("scene_pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: SCENE_FORMAT,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("scene_atlas_sampler"),
            ..Default::default()
        });
        Self {
            pipeline,
            camera_buf,
            camera_bind,
            light_buf,
            light_bind,
            mesh_layout,
            atlas_layout,
            sampler,
        }
    }

    pub fn set_camera(&self, queue: &wgpu::Queue, camera: &CameraUniform) {
        queue.write_buffer(
            &self.camera_buf,
            0,
            cast_slice(std::slice::from_ref(camera)),
        );
    }

    pub fn set_light(&self, queue: &wgpu::Queue, light: &LightParams) {
        queue.write_buffer(&self.light_buf, 0, cast_slice(std::slice::from_ref(light)));
    }

    /// Upload the mesh storage buffers + bind group. `positions`, `corner_uv` and
    /// `sector_light` are `COPY_DST` (dynamic: movers/lighting re-upload them);
    /// `corner_index`/`corner_attr` are static. `corner_uv` is texel UV fanned
    /// from BSP3D `poly_vertex_uv`. `sector_count` sizes the per-sector light buffer.
    pub fn upload_mesh(
        &self,
        device: &wgpu::Device,
        mesh: &Mesh,
        corner_uv: &[[f32; 2]],
        sector_count: usize,
    ) -> GpuMesh {
        let positions = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("scene_positions"),
            contents: cast_slice(&mesh.positions),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        let corner_index =
            storage_buf(device, "scene_corner_index", cast_slice(&mesh.corner_index));
        // corner_attr is dynamic: switches re-fan it (tex changes).
        let corner_attr = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("scene_corner_attr"),
            contents: cast_slice(&mesh.corner_attr),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        let corner_uv_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("scene_corner_uv"),
            contents: cast_slice(corner_uv),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        // Per-corner scroll (texels), re-fanned on texture_dirty (scrollers).
        let corner_scroll = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scene_corner_scroll"),
            size: (mesh.corner_count().max(1) as u64) * size_of::<f32>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sector_light = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scene_sector_light"),
            size: (sector_count.max(1) * size_of::<f32>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // Visible-corner index buffer, worst-case sized (whole level visible).
        // The world walk writes a prefix each frame; never reallocated.
        let visible_index = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scene_visible_index"),
            size: (mesh.corner_count().max(1) as u64) * size_of::<u32>() as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene_mesh_bg"),
            layout: &self.mesh_layout,
            entries: &[
                bind_buf_entry(0, &positions),
                bind_buf_entry(1, &corner_index),
                bind_buf_entry(2, &corner_attr),
                bind_buf_entry(3, &corner_uv_buf),
                bind_buf_entry(4, &sector_light),
                bind_buf_entry(5, &corner_scroll),
            ],
        });
        GpuMesh {
            positions,
            corner_attr,
            corner_uv: corner_uv_buf,
            corner_scroll,
            sector_light,
            visible_index,
            bind,
            corner_count: mesh.corner_count(),
        }
    }

    /// Upload both atlases + rect tables into one bind group.
    pub fn upload_atlases(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        walls: &Atlas,
        flats: &Atlas,
    ) -> GpuAtlas {
        let (wall_tex, wall_view) = upload_atlas_texture(device, queue, walls, "wall_atlas");
        let (flat_tex, flat_view) = upload_atlas_texture(device, queue, flats, "flat_atlas");
        let wall_rects = storage_buf(device, "wall_rects", cast_slice(&pad_rects(&walls.rects)));
        let flat_rects = storage_buf(device, "flat_rects", cast_slice(&pad_rects(&flats.rects)));
        // Animation translation tables (base id -> current frame id), uploaded
        // each frame. Sized to the rect tables (one entry per texture).
        let wall_xlat = translation_buf(device, "wall_translation", walls.rects.len());
        let flat_xlat = translation_buf(device, "flat_translation", flats.rects.len());
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene_atlas_bg"),
            layout: &self.atlas_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&wall_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&flat_view),
                },
                bind_buf_entry(2, &wall_rects),
                bind_buf_entry(3, &flat_rects),
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                bind_buf_entry(5, &wall_xlat),
                bind_buf_entry(6, &flat_xlat),
            ],
        });
        GpuAtlas {
            bind,
            wall_tex,
            flat_tex,
            wall_xlat,
            flat_xlat,
        }
    }

    /// Draw the visible corners (indexed) into the colour + depth views.
    /// `sky_bind` (group 3) lets sky-flagged walls sample the same sky as the
    /// background pass. The pass always runs — it clears the depth buffer for
    /// the entity passes — but the draw is skipped when nothing is visible.
    pub fn draw(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        mesh: &GpuMesh,
        visible_count: u32,
        atlas: &GpuAtlas,
        sky_bind: &wgpu::BindGroup,
        scene_view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
    ) {
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("scene_pass"),
            // Load: the sky background pass already filled the colour target.
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: scene_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        if visible_count == 0 {
            return;
        }
        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, &self.camera_bind, &[]);
        rpass.set_bind_group(1, &mesh.bind, &[]);
        rpass.set_bind_group(2, &atlas.bind, &[]);
        rpass.set_bind_group(3, sky_bind, &[]);
        rpass.set_bind_group(4, &self.light_bind, &[]);
        rpass.set_index_buffer(mesh.visible_index.slice(..), wgpu::IndexFormat::Uint32);
        rpass.draw_indexed(0..visible_count.min(mesh.corner_count), 0, 0..1);
    }
}

/// Mesh storage buffers + bind group. The static `corner_index` is kept alive by
/// `bind`; the dynamic buffers (`positions`, `corner_attr`, `corner_uv`,
/// `corner_scroll`, `sector_light`, `visible_index`) are retained for re-upload.
pub struct GpuMesh {
    positions: wgpu::Buffer,
    corner_attr: wgpu::Buffer,
    corner_uv: wgpu::Buffer,
    corner_scroll: wgpu::Buffer,
    sector_light: wgpu::Buffer,
    /// Worst-case-sized index buffer of visible corner ids; the world walk
    /// writes a prefix each frame and `draw` reads `visible_count` of it.
    visible_index: wgpu::Buffer,
    bind: wgpu::BindGroup,
    corner_count: u32,
}

impl GpuMesh {
    /// Re-upload vertex positions (movers mutate vertex z). Only when a surface
    /// moved (caller gates on `BSP3D::geometry_dirty`).
    pub fn update_positions(&self, queue: &wgpu::Queue, positions: &[Position]) {
        queue.write_buffer(&self.positions, 0, cast_slice(positions));
    }

    /// Re-upload per-corner attrs (switch changes a poly's texture id). Caller
    /// gates on `BSP3D::texture_dirty`.
    pub fn update_corner_attr(&self, queue: &wgpu::Queue, attr: &[CornerAttr]) {
        queue.write_buffer(&self.corner_attr, 0, cast_slice(attr));
    }

    /// Re-upload per-corner texture scroll (scrollers). Caller gates on
    /// `BSP3D::texture_dirty`.
    pub fn update_corner_scroll(&self, queue: &wgpu::Queue, scroll: &[f32]) {
        queue.write_buffer(&self.corner_scroll, 0, cast_slice(scroll));
    }

    /// Re-upload one polygon's corner-attr span (scoped dirty re-fan).
    pub fn update_corner_attr_range(
        &self,
        queue: &wgpu::Queue,
        first_corner: u32,
        attr: &[CornerAttr],
    ) {
        let offset = first_corner as u64 * size_of::<CornerAttr>() as u64;
        queue.write_buffer(&self.corner_attr, offset, cast_slice(attr));
    }

    /// Re-upload one polygon's corner-scroll span (scoped dirty re-fan).
    pub fn update_corner_scroll_range(
        &self,
        queue: &wgpu::Queue,
        first_corner: u32,
        scroll: &[f32],
    ) {
        let offset = first_corner as u64 * size_of::<f32>() as u64;
        queue.write_buffer(&self.corner_scroll, offset, cast_slice(scroll));
    }

    /// Re-upload per-sector light levels (flicker/specials change them).
    pub fn update_sector_light(&self, queue: &wgpu::Queue, light: &[f32]) {
        queue.write_buffer(&self.sector_light, 0, cast_slice(light));
    }

    /// Re-upload per-corner UV straight from BSP3D (movers re-derive wall UV).
    pub fn update_corner_uv(&self, queue: &wgpu::Queue, corner_uv: &[[f32; 2]]) {
        queue.write_buffer(&self.corner_uv, 0, cast_slice(corner_uv));
    }

    /// Upload this frame's visible corner ids (a prefix of the worst-case
    /// buffer). The world walk rebuilds the list every frame.
    pub fn update_visible_indices(&self, queue: &wgpu::Queue, indices: &[u32]) {
        queue.write_buffer(&self.visible_index, 0, cast_slice(indices));
    }
}

/// GPU atlas textures + rect tables. Static resources are kept alive by `bind`;
/// the translation tables are dynamic (animation re-uploads them each frame).
/// The atlas textures are retained so a CRT-gamma change can re-bake the
/// palette-resolved pixels in place (same dimensions, new colours).
pub struct GpuAtlas {
    bind: wgpu::BindGroup,
    wall_tex: wgpu::Texture,
    flat_tex: wgpu::Texture,
    wall_xlat: wgpu::Buffer,
    flat_xlat: wgpu::Buffer,
}

impl GpuAtlas {
    /// Re-upload the wall/flat animation translation tables (base id -> current
    /// frame id). `wall`/`flat` are `u32` per texture id.
    pub fn update_translation(&self, queue: &wgpu::Queue, wall: &[u32], flat: &[u32]) {
        queue.write_buffer(&self.wall_xlat, 0, cast_slice(wall));
        queue.write_buffer(&self.flat_xlat, 0, cast_slice(flat));
    }

    /// Re-bake the atlas pixels into the existing textures (CRT gamma changed the
    /// palette). Dimensions are palette-independent, so the textures are reused.
    pub fn reupload(&self, queue: &wgpu::Queue, walls: &Atlas, flats: &Atlas) {
        write_atlas_pixels(queue, &self.wall_tex, walls);
        write_atlas_pixels(queue, &self.flat_tex, flats);
    }
}

/// A `u32`-per-texture translation buffer (STORAGE|COPY_DST), at least one entry.
fn translation_buf(device: &wgpu::Device, label: &str, count: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: (count.max(1) * size_of::<u32>()) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// At least one rect so the storage buffer is never zero-sized.
fn pad_rects(rects: &[AtlasRect]) -> Vec<AtlasRect> {
    if rects.is_empty() {
        vec![AtlasRect {
            origin: [0, 0],
            size: [1, 1],
            layer: 0,
            _pad: [0; 3],
        }]
    } else {
        rects.to_vec()
    }
}

/// Create an array texture, bake the atlas pixels into it, and return the
/// texture (retained for re-bake) + its `D2Array` view (for the bind group).
pub(crate) fn upload_atlas_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    atlas: &Atlas,
    label: &str,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: atlas.width,
            height: atlas.height,
            depth_or_array_layers: atlas.layers,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: ATLAS_FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    write_atlas_pixels(queue, &texture, atlas);
    let view = texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    (texture, view)
}

/// Write an atlas's layer-major RGBA pixels into `texture` (one contiguous copy
/// covering all layers). Dimensions must match the texture.
pub(crate) fn write_atlas_pixels(queue: &wgpu::Queue, texture: &wgpu::Texture, atlas: &Atlas) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &atlas.pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(atlas.width * 4),
            rows_per_image: Some(atlas.height),
        },
        wgpu::Extent3d {
            width: atlas.width,
            height: atlas.height,
            depth_or_array_layers: atlas.layers,
        },
    );
}

fn storage_buf(device: &wgpu::Device, label: &str, contents: &[u8]) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents,
        usage: wgpu::BufferUsages::STORAGE,
    })
}
