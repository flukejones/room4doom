//! Pixels display backend — presents pixels via winit + wgpu.
//!
//! Uses a custom `StretchRenderer` that stretches the framebuffer texture to
//! fill the surface exactly (nearest-neighbour), bypassing the pixels crate's
//! default integer-scaling renderer.

use std::sync::Arc;

#[cfg(feature = "hprof")]
use coarse_prof::profile;
use pixels::wgpu;
use winit::window::Window;

use crate::DrawBuffer;

/// Full-screen nearest-neighbour stretch renderer.
///
/// Draws the pixels framebuffer texture to the surface via a single full-screen
/// triangle, stretching it to fill regardless of aspect ratio. The texture is
/// sampled with nearest-neighbour filtering so pixels remain sharp.
struct StretchRenderer {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    sampler: wgpu::Sampler,
}

impl StretchRenderer {
    fn new(px: &pixels::Pixels) -> Self {
        let device = px.device();
        let shader = device.create_shader_module(wgpu::include_wgsl!("stretch.wgsl"));

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("stretch_sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("stretch_bgl"),
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
        });

        let bind_group = Self::make_bind_group(device, &bind_group_layout, px, &sampler);

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("stretch_pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("stretch_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: px.render_texture_format(),
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
        });

        Self {
            pipeline,
            bind_group_layout,
            bind_group,
            sampler,
        }
    }

    /// Rebuild the bind group when the pixels texture is recreated (on resize).
    fn rebind(&mut self, device: &wgpu::Device, px: &pixels::Pixels) {
        self.bind_group = Self::make_bind_group(device, &self.bind_group_layout, px, &self.sampler);
    }

    fn make_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        px: &pixels::Pixels,
        sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        let view = px
            .texture()
            .create_view(&wgpu::TextureViewDescriptor::default());
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("stretch_bg"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }

    fn render(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView) {
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("stretch_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, &self.bind_group, &[]);
        rpass.draw(0..3, 0..1);
    }
}

/// Pixels display: owns a `pixels::Pixels` surface, a custom stretch renderer,
/// and a reference to the window.
pub struct PixelsDisplay {
    pixels: pixels::Pixels<'static>,
    stretch: StretchRenderer,
    window: Arc<Window>,
}

impl PixelsDisplay {
    /// Create from a winit window. The window must be wrapped in `Arc`.
    pub fn new(window: Arc<Window>, vsync: bool) -> Self {
        let size = window.inner_size();
        let surface = pixels::SurfaceTexture::new(size.width, size.height, window.clone());
        let pixels = pixels::PixelsBuilder::new(size.width, size.height, surface)
            // Draw buffer is 0xFFRRGGBB; on little-endian the bytes are [BB,GG,RR,FF] = BGRA,
            // so setting Bgra8UnormSrgb allows a zero-copy bulk upload of the u32 buffer.
            .texture_format(pixels::wgpu::TextureFormat::Bgra8UnormSrgb)
            .enable_vsync(vsync)
            .build()
            .expect("failed to create pixels surface");
        let stretch = StretchRenderer::new(&pixels);
        Self {
            pixels,
            stretch,
            window,
        }
    }

    /// Present the draw buffer to the screen.
    ///
    /// The framebuffer is uploaded to the pixels texture, then the
    /// `StretchRenderer` draws it stretched to fill the window surface
    /// (nearest-neighbour). Width maps 1:1; height is stretched by
    /// `win_h / buf_h` — producing the CRT 1.2× pixel aspect.
    pub(crate) fn blit(&mut self, buffer: &DrawBuffer) {
        #[cfg(feature = "hprof")]
        profile!("pixels_blit");
        let buf_w = buffer.size.width_usize() as u32;
        let buf_h = buffer.size.height_usize() as u32;

        // Resize the internal texture if the framebuffer dimensions changed.
        let tex_changed =
            self.pixels.texture().width() != buf_w || self.pixels.texture().height() != buf_h;
        if tex_changed {
            self.pixels
                .resize_buffer(buf_w, buf_h)
                .expect("failed to resize pixels buffer");
            // Texture was recreated — rebuild the bind group.
            self.stretch.rebind(self.pixels.device(), &self.pixels);
        }

        // Resize the surface to match the current window size.
        let win_size = self.window.inner_size();
        self.pixels
            .resize_surface(win_size.width, win_size.height)
            .expect("failed to resize pixels surface");

        // Upload framebuffer: 0xFFRRGGBB in memory on little-endian is [BB,GG,RR,FF] =
        // BGRA, matching the Bgra8UnormSrgb texture format — bulk copy with no
        // per-pixel work.
        {
            #[cfg(feature = "hprof")]
            profile!("pixels_upload");
            let pixel_count = (buf_w * buf_h) as usize;
            let src = unsafe {
                std::slice::from_raw_parts(buffer.buffer.as_ptr() as *const u8, pixel_count * 4)
            };
            self.pixels.frame_mut()[..src.len()].copy_from_slice(src);
        }

        {
            #[cfg(feature = "hprof")]
            profile!("pixels_render");
            let stretch = &self.stretch;
            self.pixels
                .render_with(|encoder, target, _ctx| {
                    stretch.render(encoder, target);
                    Ok(())
                })
                .expect("pixels render failed");
        }
    }

    /// Window size in logical pixels.
    pub fn window_size(&self) -> (u32, u32) {
        let size = self.window.inner_size();
        (size.width, size.height)
    }

    pub(crate) fn set_fullscreen(&mut self, mode: u8) {
        use winit::window::Fullscreen;
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
}
