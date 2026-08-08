//! UI planes as quads in the viewer's space.
//!
//! A flat display draws one quad at identity, covering the screen. A headset
//! draws each plane as a virtual panel at a real distance, projected through
//! that eye's asymmetric frustum — screen-space UI does not fuse in a headset,
//! because the two eyes' frusta are mirrored and the same pixel is a different
//! angle in each.

#[cfg(feature = "vr")]
use glam::{Mat4, Quat, Vec3, Vec4};
#[cfg(feature = "vr")]
use render_common::EyeView;

use super::FORMAT;
use super::scene::{bind_sampler_entry, tex_entry};
#[cfg(feature = "vr")]
use crate::backend::PanelPlacement;

/// Bytes of the panel uniform: one `mat4x4<f32>`.
const PLACEMENT_BYTES: u64 = 64;

/// Clip range the panel projection maps to. `NEAR` exists to cull a panel the
/// viewer has turned their back on — without it the negative-`w` vertices
/// project to a mirrored ghost.
#[cfg(feature = "vr")]
const NEAR: f32 = 0.05;
#[cfg(feature = "vr")]
const FAR: f32 = 100.0;

/// One UI plane's GPU residence: its texture plus the placement uniform.
pub(crate) struct Panel {
    texture: wgpu::Texture,
    placement: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    w: u32,
    h: u32,
}

impl Panel {
    fn new(device: &wgpu::Device, pipeline: &PanelPipeline, w: u32, h: u32) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ui_panel"),
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
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let placement = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui_panel_placement"),
            size: PLACEMENT_BYTES,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui_panel_bg"),
            layout: &pipeline.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&pipeline.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: placement.as_entire_binding(),
                },
            ],
        });
        Self {
            texture,
            placement,
            bind_group,
            w,
            h,
        }
    }

    /// Upload the plane's ARGB pixels.
    pub fn upload(&self, queue: &wgpu::Queue, pixels: &[u32]) {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            super::bytemuck_cast(pixels),
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

    pub fn size(&self) -> (u32, u32) {
        (self.w, self.h)
    }
}

/// The shared quad pipeline plus the per-plane panels it draws.
pub(crate) struct PanelPipeline {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

impl PanelPipeline {
    pub fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui_panel_bgl"),
            entries: &[tex_entry(0), bind_sampler_entry(1), placement_entry(2)],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui_panel_pipeline_layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("panel.wgsl"));
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui_panel_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                // A facing-locked panel is seen from either side as the head
                // turns; the near plane, not backface culling, removes it.
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: FORMAT,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            multiview_mask: None,
            cache: None,
        });
        // Linear so a projected panel does not shimmer. At identity the texture
        // and target match, so samples land on texel centres and nothing blurs.
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ui_panel_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            pipeline,
            layout,
            sampler,
        }
    }

    /// A panel backed by a `w`×`h` texture.
    pub fn panel(&self, device: &wgpu::Device, w: u32, h: u32) -> Panel {
        Panel::new(device, self, w, h)
    }

    /// Blend `panel` over the pass's target at `clip_from_panel`. The quad is
    /// the unit square, so the matrix carries the extents and identity fills
    /// the screen. Records into the caller's pass — no pass of its own.
    pub fn record(
        &self,
        queue: &wgpu::Queue,
        rpass: &mut wgpu::RenderPass<'_>,
        panel: &Panel,
        clip_from_panel: [[f32; 4]; 4],
    ) {
        // A queue write lands before the submit that carries this pass.
        queue.write_buffer(&panel.placement, 0, cast_matrix(&clip_from_panel));
        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, &panel.bind_group, &[]);
        rpass.draw(0..4, 0..1);
    }
}

/// The placement uniform binding. Vertex-visible: the quad's corners are what
/// the matrix moves.
fn placement_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

/// Reinterpret a column-major matrix as bytes for upload (no copy).
#[inline]
fn cast_matrix(m: &[[f32; 4]; 4]) -> &[u8] {
    // SAFETY: 16 contiguous `f32` viewed at byte (tighter) alignment.
    unsafe { std::slice::from_raw_parts(m.as_ptr().cast::<u8>(), size_of_val(m)) }
}

/// The full-screen placement: the quad's unit square already spans clip space.
pub(crate) const FULLSCREEN: [[f32; 4]; 4] = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

/// What a panel's orientation follows. Fixed per plane: the hud rides the head
/// so it is always to hand, the screen holds a heading so a menu stays put while
/// the player looks around it.
#[cfg(feature = "vr")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Lock {
    /// Rides head orientation exactly — always centred in view.
    Head,
    /// Holds the yaw captured when the panel became visible.
    Facing,
}

/// Where a panel lands in one eye's clip space. It hangs in front of the *head*
/// centre, not the eye: that offset is the disparity that makes it fuse at
/// `distance_m` rather than at infinity. `aspect` is the surface's width:height.
#[cfg(feature = "vr")]
pub(crate) fn clip_from_panel(
    eye: &EyeView,
    head_position: [f32; 3],
    placement: PanelPlacement,
    lock: Lock,
    anchor_yaw: f32,
    aspect: f32,
) -> [[f32; 4]; 4] {
    let orientation = Quat::from_xyzw(
        eye.orientation[0],
        eye.orientation[1],
        eye.orientation[2],
        eye.orientation[3],
    );
    let eye_from_play =
        Mat4::from_quat(orientation.inverse()) * Mat4::from_translation(-Vec3::from(eye.position));
    let facing = match lock {
        Lock::Head => Mat4::from_quat(orientation),
        Lock::Facing => Mat4::from_rotation_y(anchor_yaw),
    };
    let half_width = placement.distance_m * (placement.width_deg.to_radians() * 0.5).tan();
    let play_from_panel = Mat4::from_translation(Vec3::from(head_position))
        * facing
        * Mat4::from_rotation_x(placement.pitch_deg.to_radians())
        * Mat4::from_translation(Vec3::new(0.0, 0.0, -placement.distance_m))
        * Mat4::from_scale(Vec3::new(half_width, half_width / aspect, 1.0));
    (projection(eye.fov) * eye_from_play * play_from_panel).to_cols_array_2d()
}

/// The eye's asymmetric frustum from its tangent-space left/right/up/down,
/// mapping -Z forward to a 0..1 depth range.
#[cfg(feature = "vr")]
fn projection(fov: [f32; 4]) -> Mat4 {
    let [left, right, up, down] = fov;
    let (left, right) = (left.tan(), right.tan());
    let (up, down) = (up.tan(), down.tan());
    let (width, height) = (right - left, up - down);
    Mat4::from_cols(
        Vec4::new(2.0 / width, 0.0, 0.0, 0.0),
        Vec4::new(0.0, 2.0 / height, 0.0, 0.0),
        Vec4::new(
            (right + left) / width,
            (up + down) / height,
            -FAR / (FAR - NEAR),
            -1.0,
        ),
        Vec4::new(0.0, 0.0, -FAR * NEAR / (FAR - NEAR), 0.0),
    )
}

#[cfg(all(test, feature = "vr"))]
mod tests {
    use super::*;

    /// A symmetric 90° eye at the origin, looking down -Z.
    fn eye(position: [f32; 3], fov: [f32; 4]) -> EyeView {
        EyeView {
            playspace_yaw: 0.0,
            position,
            orientation: [0.0, 0.0, 0.0, 1.0],
            fov,
        }
    }

    const SYMMETRIC: [f32; 4] = [-0.7853982, 0.7853982, 0.7853982, -0.7853982];
    /// Left eye of a typical headset: the frustum leans outward.
    const ASYMMETRIC: [f32; 4] = [-0.9, 0.7853982, 0.7853982, -0.7853982];

    const HEAD: PanelPlacement = PanelPlacement {
        distance_m: 2.0,
        width_deg: 60.0,
        pitch_deg: 0.0,
    };

    /// Project a panel-local corner and return its NDC.
    fn ndc(m: &[[f32; 4]; 4], x: f32, y: f32) -> (f32, f32) {
        let m = Mat4::from_cols_array_2d(m);
        let clip = m * Vec4::new(x, y, 0.0, 1.0);
        (clip.x / clip.w, clip.y / clip.w)
    }

    #[test]
    fn panel_width_matches_the_angle_it_subtends() {
        // A 90° panel in a 90° frustum fills the eye exactly.
        let m = clip_from_panel(
            &eye([0.0; 3], SYMMETRIC),
            [0.0; 3],
            PanelPlacement {
                width_deg: 90.0,
                ..HEAD
            },
            Lock::Head,
            0.0,
            1.0,
        );
        let (nx, ny) = ndc(&m, 1.0, 1.0);
        assert!((nx - 1.0).abs() < 1e-5, "right edge at {nx}");
        assert!((ny - 1.0).abs() < 1e-5, "top edge at {ny}");
    }

    #[test]
    fn an_asymmetric_eye_offsets_the_panel_centre() {
        let symmetric = clip_from_panel(
            &eye([0.0; 3], SYMMETRIC),
            [0.0; 3],
            HEAD,
            Lock::Head,
            0.0,
            1.0,
        );
        let asymmetric = clip_from_panel(
            &eye([0.0; 3], ASYMMETRIC),
            [0.0; 3],
            HEAD,
            Lock::Head,
            0.0,
            1.0,
        );
        assert!(ndc(&symmetric, 0.0, 0.0).0.abs() < 1e-6);
        // tan(0.9) > tan(pi/4), so the frustum's centre line sits left of the
        // eye's forward axis and a panel dead ahead reads right of centre.
        assert!(
            ndc(&asymmetric, 0.0, 0.0).0 > 0.05,
            "asymmetric centre at {}",
            ndc(&asymmetric, 0.0, 0.0).0
        );
    }

    #[test]
    fn each_eye_sees_the_panel_from_its_own_side() {
        // 64 mm IPD, head centred between the eyes.
        let (left, right) = ([-0.032, 0.0, 0.0], [0.032, 0.0, 0.0]);
        let head = [0.0; 3];
        let l = clip_from_panel(&eye(left, SYMMETRIC), head, HEAD, Lock::Head, 0.0, 1.0);
        let r = clip_from_panel(&eye(right, SYMMETRIC), head, HEAD, Lock::Head, 0.0, 1.0);
        let (lx, rx) = (ndc(&l, 0.0, 0.0).0, ndc(&r, 0.0, 0.0).0);
        // An object ahead of the head appears right of centre to the left eye.
        assert!(lx > 0.0, "left eye sees centre at {lx}");
        assert!(rx < 0.0, "right eye sees centre at {rx}");
        assert!((lx + rx).abs() < 1e-6, "disparity is not symmetric");
    }

    #[test]
    fn a_panel_behind_the_viewer_is_clipped_away() {
        // Facing-locked panel anchored behind: every corner must fail the
        // near-plane test rather than project to a mirrored ghost.
        let m = Mat4::from_cols_array_2d(&clip_from_panel(
            &eye([0.0; 3], SYMMETRIC),
            [0.0; 3],
            HEAD,
            Lock::Facing,
            std::f32::consts::PI,
            1.0,
        ));
        for (x, y) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
            let clip = m * Vec4::new(x, y, 0.0, 1.0);
            assert!(
                clip.w <= 0.0 || clip.z < 0.0,
                "corner ({x},{y}) not clipped"
            );
        }
    }

    #[test]
    fn pitch_drops_the_panel_below_the_eye_line() {
        let m = clip_from_panel(
            &eye([0.0; 3], SYMMETRIC),
            [0.0; 3],
            PanelPlacement {
                pitch_deg: -15.0,
                ..HEAD
            },
            Lock::Head,
            0.0,
            1.0,
        );
        assert!(ndc(&m, 0.0, 0.0).1 < -0.1, "panel centre is not below");
    }
}
