//! The teleport aim arc and its landing ring.
//!
//! Translucent and depth-tested but never depth-writing, so it reads as an
//! overlay on the world without occluding anything behind it. Drawn last in the
//! world pass for that reason.

use std::slice;

use bytemuck::cast_slice;
use math::{TELEPORT_GRAVITY, TELEPORT_SPEED};
use render_common::TeleportView;

use crate::scene::{DEPTH_FORMAT, SCENE_FORMAT};
use crate::shaders::{TELEPORT_SRC, bind_buf_entry, bind_uniform_entry};
use crate::sprites::SpriteCam;

/// Ribbon strip vertices: a pair per step, plus the closing pair. Must match
/// `ARC_STEPS` in `teleport.wgsl`.
const ARC_STEPS: u32 = 32;
const ARC_VERTS: u32 = (ARC_STEPS + 1) * 2;
/// Quad vertices for the landing ring.
const RING_VERTS: u32 = 4;

/// Half the ribbon's width, map units.
const ARC_HALF_WIDTH: f32 = 1.5;
/// Landing ring radius, map units — the space the player will occupy.
const RING_RADIUS: f32 = 16.0;

/// Arc parameters (group 1). std140: 4×vec4 = 64 bytes.
#[repr(C)]
#[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
struct TeleportUniform {
    origin: [f32; 4],
    dir: [f32; 4],
    landing: [f32; 4],
    shape: [f32; 4],
}

impl From<&TeleportView> for TeleportUniform {
    fn from(view: &TeleportView) -> Self {
        let [ox, oy, oz] = view.origin;
        let [dx, dy, dz] = view.dir;
        let [lx, ly, lz] = view.landing;
        Self {
            origin: [ox, oy, oz, view.flight_time],
            dir: [dx, dy, dz, TELEPORT_SPEED],
            landing: [lx, ly, lz, f32::from(u8::from(view.valid))],
            shape: [TELEPORT_GRAVITY, ARC_HALF_WIDTH, RING_RADIUS, 0.0],
        }
    }
}

/// GPU resources for the aim overlay: its own camera copy and the arc uniform.
pub struct TeleportPipeline {
    arc: wgpu::RenderPipeline,
    ring: wgpu::RenderPipeline,
    cam_buf: wgpu::Buffer,
    cam_bind: wgpu::BindGroup,
    aim_buf: wgpu::Buffer,
    aim_bind: wgpu::BindGroup,
}

impl TeleportPipeline {
    pub fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("teleport_shader"),
            source: wgpu::ShaderSource::Wgsl(TELEPORT_SRC.into()),
        });
        let (cam_buf, cam_layout, cam_bind) =
            uniform_group(device, "teleport_cam", size_of::<SpriteCam>() as u64);
        let (aim_buf, aim_layout, aim_bind) =
            uniform_group(device, "teleport_aim", size_of::<TeleportUniform>() as u64);

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("teleport_pipeline_layout"),
            bind_group_layouts: &[Some(&cam_layout), Some(&aim_layout)],
            immediate_size: 0,
        });
        Self {
            arc: build(device, &layout, &shader, "vs_arc", "fs_arc", "teleport_arc"),
            ring: build(
                device,
                &layout,
                &shader,
                "vs_ring",
                "fs_ring",
                "teleport_ring",
            ),
            cam_buf,
            cam_bind,
            aim_buf,
            aim_bind,
        }
    }

    /// Stage this frame's camera and arc. Call before the world pass opens.
    pub fn upload(&self, queue: &wgpu::Queue, cam: &SpriteCam, view: &TeleportView) {
        queue.write_buffer(&self.cam_buf, 0, cast_slice(slice::from_ref(cam)));
        let aim = TeleportUniform::from(view);
        queue.write_buffer(&self.aim_buf, 0, cast_slice(slice::from_ref(&aim)));
    }

    /// Ribbon then ring, into an open world pass.
    pub fn record(&self, rpass: &mut wgpu::RenderPass<'_>) {
        rpass.set_bind_group(0, &self.cam_bind, &[]);
        rpass.set_bind_group(1, &self.aim_bind, &[]);
        rpass.set_pipeline(&self.arc);
        rpass.draw(0..ARC_VERTS, 0..1);
        rpass.set_pipeline(&self.ring);
        rpass.draw(0..RING_VERTS, 0..1);
    }
}

/// A uniform buffer with its own single-binding layout and bind group.
fn uniform_group(
    device: &wgpu::Device,
    label: &str,
    size: u64,
) -> (wgpu::Buffer, wgpu::BindGroupLayout, wgpu::BindGroup) {
    let buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
        entries: &[bind_uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT)],
    });
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout: &layout,
        entries: &[bind_buf_entry(0, &buf)],
    });
    (buf, layout, bind)
}

/// Alpha-blended, depth-tested, depth-write off: the overlay must not occlude
/// the world it is drawn over.
fn build(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    vs_entry: &str,
    fs_entry: &str,
    label: &str,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(vs_entry),
            buffers: &[],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleStrip,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fs_entry),
            targets: &[Some(wgpu::ColorTargetState {
                format: SCENE_FORMAT,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        multiview_mask: None,
        cache: None,
    })
}
