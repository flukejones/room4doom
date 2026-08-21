//! Tracked hand skeletons and hand/controller aim pointers drawn in the world.
//!
//! Depth-tested but never depth-writing, like the teleport aim: the hands sit
//! over the world without occluding anything behind them.

use std::slice;

use bytemuck::cast_slice;
use render_common::{HAND_JOINT_COUNT, HandPose};

use crate::scene::{DEPTH_FORMAT, SCENE_FORMAT};
use crate::shaders::{HANDS_SRC, bind_buf_entry, bind_uniform_entry};
use crate::sprites::SpriteCam;

const HANDS: usize = 2;
const JOINT_SLOTS: usize = HAND_JOINT_COUNT * HANDS;
const POINTER_SLOTS: usize = 2 * HANDS;
/// The ribbon uses 45% of this radius, matching the hand bones.
const POINTER_RADIUS: f32 = 0.25;
/// Bones per hand: every joint above the palm and wrist has a parent.
const BONES: u32 = 24;
const QUAD_VERTS: u32 = 4;

/// Joint slots (group 1). `w` is the radius; negative marks an untracked hand.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct JointsUniform {
    joint: [[f32; 4]; JOINT_SLOTS + POINTER_SLOTS],
}

impl From<&HandPose> for JointsUniform {
    fn from(pose: &HandPose) -> Self {
        let mut joint = [[0.0, 0.0, 0.0, -1.0]; JOINT_SLOTS + POINTER_SLOTS];
        for (hand, tracked) in pose.joints.iter().enumerate() {
            let Some(tracked) = tracked else {
                continue;
            };
            for (i, j) in tracked.iter().enumerate() {
                let [x, y, z] = j.position;
                joint[hand * HAND_JOINT_COUNT + i] = [x, y, z, j.radius];
            }
        }
        for (hand, pointer) in pose.pointers.iter().enumerate() {
            if let Some(pointer) = pointer {
                for (end, [x, y, z]) in [pointer.origin, pointer.tip].into_iter().enumerate() {
                    joint[JOINT_SLOTS + hand * 2 + end] = [x, y, z, POINTER_RADIUS];
                }
            }
        }
        Self {
            joint,
        }
    }
}

/// GPU resources for the hand overlay: its own camera copy and the joint table.
pub struct HandsPipeline {
    joints_pipeline: wgpu::RenderPipeline,
    bones_pipeline: wgpu::RenderPipeline,
    pointers_pipeline: wgpu::RenderPipeline,
    cam_buf: wgpu::Buffer,
    cam_bind: wgpu::BindGroup,
    joints_buf: wgpu::Buffer,
    joints_bind: wgpu::BindGroup,
}

impl HandsPipeline {
    pub fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("hands_shader"),
            source: wgpu::ShaderSource::Wgsl(HANDS_SRC.into()),
        });
        let (cam_buf, cam_layout, cam_bind) =
            uniform_group(device, "hands_cam", size_of::<SpriteCam>() as u64);
        let (joints_buf, joints_layout, joints_bind) =
            uniform_group(device, "hands_joints", size_of::<JointsUniform>() as u64);

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("hands_pipeline_layout"),
            bind_group_layouts: &[Some(&cam_layout), Some(&joints_layout)],
            immediate_size: 0,
        });
        Self {
            joints_pipeline: build(
                device,
                &layout,
                &shader,
                "vs_joint",
                "fs_joint",
                "hands_joints",
            ),
            bones_pipeline: build(
                device,
                &layout,
                &shader,
                "vs_bone",
                "fs_bone",
                "hands_bones",
            ),
            pointers_pipeline: build(
                device,
                &layout,
                &shader,
                "vs_pointer",
                "fs_bone",
                "hands_pointers",
            ),
            cam_buf,
            cam_bind,
            joints_buf,
            joints_bind,
        }
    }

    /// Stage this frame's camera and joints. Call before the world pass opens.
    pub fn upload(&self, queue: &wgpu::Queue, cam: &SpriteCam, pose: &HandPose) {
        queue.write_buffer(&self.cam_buf, 0, cast_slice(slice::from_ref(cam)));
        let joints = JointsUniform::from(pose);
        queue.write_buffer(&self.joints_buf, 0, cast_slice(slice::from_ref(&joints)));
    }

    /// Pointers, bones, then joints, into an open world pass.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "rendering intentionally quantizes float coordinates and packs pixel channels and bounded buffer indices"
    )]
    pub fn record(&self, rpass: &mut wgpu::RenderPass<'_>) {
        rpass.set_bind_group(0, &self.cam_bind, &[]);
        rpass.set_bind_group(1, &self.joints_bind, &[]);
        rpass.set_pipeline(&self.pointers_pipeline);
        rpass.draw(0..QUAD_VERTS, 0..HANDS as u32);
        rpass.set_pipeline(&self.bones_pipeline);
        rpass.draw(0..QUAD_VERTS, 0..BONES * HANDS as u32);
        rpass.set_pipeline(&self.joints_pipeline);
        rpass.draw(0..QUAD_VERTS, 0..JOINT_SLOTS as u32);
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

/// Alpha-blended, depth-tested, depth-write off: an overlay on the world.
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
