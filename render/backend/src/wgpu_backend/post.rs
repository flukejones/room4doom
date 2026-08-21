//! The post-process chain: an ordered list of full-screen passes (stretch,
//! CRT, …) where the last pass targets the surface.

use super::FORMAT;

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
pub(crate) struct PostChain {
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
    pub(super) fn new(
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
    pub(super) fn resize(
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

    /// Run the chain, placing its final pass in `rect` within `surface`.
    #[expect(
        clippy::cast_precision_loss,
        reason = "wgpu viewports use f32 coordinates"
    )]
    pub(crate) fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        surface: &wgpu::TextureView,
        rect: [u32; 4],
    ) {
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
                        load: if i == last {
                            wgpu::LoadOp::Load
                        } else {
                            wgpu::LoadOp::Clear(wgpu::Color::BLACK)
                        },
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if i == last {
                rpass.set_viewport(
                    rect[0] as f32,
                    rect[1] as f32,
                    rect[2] as f32,
                    rect[3] as f32,
                    0.0,
                    1.0,
                );
                rpass.set_scissor_rect(rect[0], rect[1], rect[2], rect[3]);
            }
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
