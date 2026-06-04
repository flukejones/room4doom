//! Camera matching `software3d::update_view_matrix`: eye-at-origin view matrix,
//! per-vertex translation by subtracting `camera_pos` (avoids fp cancellation at
//! large Doom coords). The GPU vertex shader computes
//! `clip = view_proj * vec4(pos - camera_pos, 1)`.

use glam::{Mat4, Vec3};
use render_common::RenderView;
use std::f32::consts::PI;

const NEAR_Z: f32 = 4.0;
const FAR_Z: f32 = 10000.0;
/// Max pitch (radians); matches software3d, prevents a degenerate basis.
pub(crate) const MAX_PITCH: f32 = 89.0 * PI / 180.0;

/// Camera uniform uploaded each frame. `view_proj` is eye-at-origin
/// (`projection * look_at_rh(ZERO, fwd, up)`); the shader subtracts `camera_pos`
/// per vertex. 16-byte aligned for WGSL std140/std430.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    view_proj: [[f32; 4]; 4],
    camera_pos: [f32; 3],
    /// Gun-flash extra light (0..~3), added to the sector light band.
    extralight: f32,
}

impl CameraUniform {
    /// Projection matrix for the buffer size, via the shared OG projection.
    pub fn projection(fov: f32, width: f32, view_height: f32) -> Mat4 {
        let (hfov, vfov, _) = render_common::og_projection(fov, width, view_height);
        let aspect = (hfov / 2.0).tan() / (vfov / 2.0).tan();
        Mat4::perspective_rh_gl(vfov, aspect, NEAR_Z, FAR_Z)
    }

    /// The eye-at-origin view_proj as column-major arrays (for sky ray inverse).
    pub fn view_proj(&self) -> [[f32; 4]; 4] {
        self.view_proj
    }

    /// Build from the player view + a prebuilt projection matrix.
    pub fn new(view: &RenderView, projection: Mat4) -> Self {
        let pos = Vec3::new(view.x.into(), view.y.into(), view.viewz.into());
        let angle = view.angle.rad();
        let pitch = view.lookdir.clamp(-MAX_PITCH, MAX_PITCH);
        let forward = Vec3::new(
            angle.cos() * pitch.cos(),
            angle.sin() * pitch.cos(),
            pitch.sin(),
        );
        let view_matrix = Mat4::look_at_rh(Vec3::ZERO, forward, Vec3::Z);
        let view_proj = projection * view_matrix;
        Self {
            view_proj: view_proj.to_cols_array_2d(),
            camera_pos: pos.to_array(),
            extralight: view.extralight as f32,
        }
    }
}
