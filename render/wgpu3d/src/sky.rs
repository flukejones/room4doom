//! Sky rendering. Two modes (toggled at runtime):
//! - Static: cylindrical SKY1 sample by view direction; fades to the average sky
//!   colour beyond the texture's vertical band.
//! - Dynamic: Quake 1 flattened dome (`EmitSkyPolys`) with procedural fbm clouds
//!   tinted between the SKY1-average dark/bright colours; two scrolling layers.
//!
//! Sky-flagged walls/flats sample the sky by their own `worldpos - eye` direction
//! (interpolated off real geometry) and write real depth, so they occlude. A
//! fullscreen pass clears the background; both share the sky functions.

use crate::camera::CameraUniform;
use crate::shaders::{bind_sampler_entry, bind_tex_2d_entry, bind_uniform_entry};
use bytemuck::{Pod, Zeroable};
use glam::Mat4;
use pic_data::PicData;
use pic_data::sky::{SKY_DOWN_ROWS, SKY_EXTEND_ROWS, SKY_V_STRETCH, build_sky_extended};
use render_common::RenderView;

/// Sky mode, cycled by a key.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum SkyMode {
    #[default]
    Static,
    Dynamic,
}

/// Per-frame sky params. Sky-flagged walls derive their view direction from the
/// interpolated `worldpos - eye`; the fullscreen pass reconstructs it from
/// `inv_view_proj`. Static samples the extended SKY1 texture cylindrically,
/// dynamic is the procedural dome tinted between `sky_dark`/`sky_bright`. vec4
/// colours keep the std140 layout exact (64 + 8 + 8 + 16 + 16 + 16 = 128).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct SkyUniform {
    /// Inverse of the eye-at-origin view_proj: NDC -> world ray (fullscreen pass).
    inv_view_proj: [[f32; 4]; 4],
    /// Buffer dims, to map fragment pixels -> NDC.
    viewport: [f32; 2],
    /// [lo, hi): the v-range the original texture band occupies in the extended
    /// static sky texture. The horizon is placed inside it.
    sky_band: [f32; 2],
    /// Dynamic cloud base colour (rgb, w pad).
    sky_dark: [f32; 4],
    /// Dynamic cloud highlight colour (rgb, w pad).
    sky_bright: [f32; 4],
    /// 0 = static, 1 = dynamic.
    mode: u32,
    /// Seconds, for the dynamic cloud scroll.
    time: f32,
    /// Band-heights of v per tan(pitch); matches software3d's row mapping.
    v_scale: f32,
    _pad: f32,
}

impl SkyUniform {
    fn new(
        view: &RenderView,
        projection: Mat4,
        width: f32,
        view_height: f32,
        sky_band: [f32; 2],
        sky_dark: [f32; 3],
        sky_bright: [f32; 3],
        mode: SkyMode,
        time: f32,
    ) -> Self {
        let camera = CameraUniform::new(view, projection);
        let view_proj = Mat4::from_cols_array_2d(&camera.view_proj());
        Self {
            inv_view_proj: view_proj.inverse().to_cols_array_2d(),
            viewport: [width, view_height],
            sky_band,
            sky_dark: [sky_dark[0], sky_dark[1], sky_dark[2], 0.0],
            sky_bright: [sky_bright[0], sky_bright[1], sky_bright[2], 0.0],
            mode: match mode {
                SkyMode::Static => 0,
                SkyMode::Dynamic => 1,
            },
            time,
            v_scale: projection.y_axis.y / (2.0 * SKY_V_STRETCH),
            _pad: 0.0,
        }
    }
}

/// Sky GPU resources: the static SKY1 texture, per-frame uniform, and the
/// fullscreen background pipeline. Dynamic clouds are procedural (no texture);
/// their tint comes from the SKY1 average colour held here.
pub struct Sky {
    pipeline: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    bind: wgpu::BindGroup,
    sky_band: [f32; 2],
    sky_dark: [f32; 3],
    sky_bright: [f32; 3],
}

impl Sky {
    /// Build from the WAD sky texture (`pic_data.sky_pic()`).
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, pic_data: &PicData) -> Self {
        let sky_pic = pic_data.sky_pic();
        // Dynamic cloud tint from the sky average.
        let avg = sky_average_color(pic_data, sky_pic);
        let avg_f = avg.map(|c| c as f32 / 255.0);
        let sky_dark = avg_f.map(|c| c * 0.55);
        let sky_bright = avg_f.map(|c| (c * 1.6).min(1.0));

        let (static_view, sky_band) = upload_sky_texture(device, queue, pic_data, sky_pic);

        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sky_uniform"),
            size: size_of::<SkyUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sky_sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sky_bgl"),
            entries: &[
                bind_uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT),
                bind_tex_2d_entry(1),
                bind_sampler_entry(2, wgpu::SamplerBindingType::Filtering),
            ],
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sky_bg"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&static_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sky_shader"),
            source: wgpu::ShaderSource::Wgsl(crate::shaders::SKY_SRC.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sky_pipeline_layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sky_pipeline"),
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
                    format: crate::SCENE_FORMAT,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            multiview_mask: None,
            cache: None,
        });

        Self {
            pipeline,
            uniform,
            bind,
            sky_band,
            sky_dark,
            sky_bright,
        }
    }

    /// Update the per-frame sky uniform.
    pub fn set_params(
        &self,
        queue: &wgpu::Queue,
        view: &RenderView,
        projection: Mat4,
        width: f32,
        view_height: f32,
        mode: SkyMode,
        time: f32,
    ) {
        let u = SkyUniform::new(
            view,
            projection,
            width,
            view_height,
            self.sky_band,
            self.sky_dark,
            self.sky_bright,
            mode,
            time,
        );
        queue.write_buffer(&self.uniform, 0, bytemuck::cast_slice(&[u]));
    }

    /// The sky bind group, so the scene pipeline can sample the same sky for
    /// sky-flagged walls. Its layout matches the scene's group-3 layout.
    pub fn bind(&self) -> &wgpu::BindGroup {
        &self.bind
    }

    /// Fullscreen background sky pass (fills where no geometry is drawn).
    pub fn draw_background(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView) {
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("sky_background"),
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
        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, &self.bind, &[]);
        rpass.draw(0..3, 0..1);
    }
}

/// Upload the extended sky as RGBA, laid out top->bottom: up-extension (zenith
/// at row 0), original texture, down-extension (nadir at the last row). Returns
/// the view and the [lo, hi) v-range the original texture band occupies, so the
/// shader can place the horizon inside it. The extension fades the texture into
/// the averaged zenith/nadir colours (matches software3d's smooth sky).
fn upload_sky_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pic_data: &PicData,
    sky_pic: usize,
) -> (wgpu::TextureView, [f32; 2]) {
    let tex = pic_data.wall_pic(sky_pic);
    let (w, h) = (tex.width, tex.height);
    let palette = pic_data.palette();

    // build_sky_extended returns column-major [texture(h) | up(UP) | down(DN)].
    let extended = build_sky_extended(&tex.data, w, h, pic_data.colourmap(0), palette, |c| c);

    let up = SKY_EXTEND_ROWS;
    let dn = SKY_DOWN_ROWS;
    let src_total = h + up + dn;
    let out_h = up + h + dn;

    // Reassemble into natural top->bottom order, row-major RGBA.
    let mut rgba = vec![0u8; w * out_h * 4];
    let mut put = |out_row: usize, col: usize, argb: u32| {
        let di = (out_row * w + col) * 4;
        rgba[di] = (argb >> 16) as u8;
        rgba[di + 1] = (argb >> 8) as u8;
        rgba[di + 2] = argb as u8;
        rgba[di + 3] = 255;
    };
    for col in 0..w {
        let base = col * src_total;
        // up-extension: src row (h + e) sits e rows above the texture top, so it
        // goes to natural row (up - 1 - e) — zenith (largest e) at row 0.
        for e in 0..up {
            put(up - 1 - e, col, extended[base + h + e]);
        }
        // original texture
        for row in 0..h {
            put(up + row, col, extended[base + row]);
        }
        // down-extension: src row (h + up + d) goes below the texture.
        for d in 0..dn {
            put(up + h + d, col, extended[base + h + up + d]);
        }
    }

    let lo = up as f32 / out_h as f32;
    let hi = (up + h) as f32 / out_h as f32;
    let view = upload_rgba(device, queue, "sky_static", w as u32, out_h as u32, &rgba);
    (view, [lo, hi])
}

/// Average RGB of the sky texture's opaque texels through the base palette.
fn sky_average_color(pic_data: &PicData, sky_pic: usize) -> [u8; 3] {
    let tex = pic_data.wall_pic(sky_pic);
    let palette = &pic_data.palettes()[0];
    let (mut r, mut g, mut b, mut n) = (0u64, 0u64, 0u64, 0u64);
    for &texel in &tex.data {
        if texel == u16::MAX {
            continue;
        }
        let argb = palette.0[texel as usize];
        r += ((argb >> 16) & 0xFF) as u64;
        g += ((argb >> 8) & 0xFF) as u64;
        b += (argb & 0xFF) as u64;
        n += 1;
    }
    if n == 0 {
        return [0, 0, 0];
    }
    [(r / n) as u8, (g / n) as u8, (b / n) as u8]
}

fn upload_rgba(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    w: u32,
    h: u32,
    rgba: &[u8],
) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(w * 4),
            rows_per_image: Some(h),
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}
