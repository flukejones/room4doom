//! Camera matching `software3d::update_view_matrix`: eye-at-origin view matrix,
//! per-vertex translation by subtracting `camera_pos` (avoids fp cancellation at
//! large Doom coords). The GPU vertex shader computes
//! `clip = view_proj * vec4(pos - camera_pos, 1)`.

use glam::{Mat4, Vec3};
use render_common::{CRT_STRETCH, EyeBasis, RenderView};
use std::f32::consts::PI;

const NEAR_Z: f32 = 4.0;
const FAR_Z: f32 = 10000.0;
/// Max pitch (radians); matches software3d, prevents a degenerate basis.
pub(crate) const MAX_PITCH: f32 = 89.0 * PI / 180.0;

/// Doom's tall-pixel look as real world geometry: everything renders 1.2×
/// taller about the camera's horizontal plane. World space, so it holds at
/// any head orientation — an image-space stretch shears on roll.
pub(crate) fn world_stretch() -> Mat4 {
    Mat4::from_scale(Vec3::new(1.0, 1.0, CRT_STRETCH))
}

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

    /// Off-axis projection from a headset's per-eye view angles (left,
    /// right, up, down radians, as OpenXR reports them).
    ///
    /// A headset's eye frustum is asymmetric — the optical axis sits off
    /// centre — so a symmetric perspective renders each eye about the
    /// wrong axis and the two images never fuse.
    pub fn projection_from_angles(fov: [f32; 4]) -> Mat4 {
        let [left, right, up, down] = fov;
        let (l, r) = (left.tan() * NEAR_Z, right.tan() * NEAR_Z);
        let (t, b) = (up.tan() * NEAR_Z, down.tan() * NEAR_Z);
        let (rl, tb, fnr) = (r - l, t - b, FAR_Z - NEAR_Z);
        // Right-handed, OpenGL depth range, matching `perspective_rh_gl`.
        Mat4::from_cols_array_2d(&[
            [2.0 * NEAR_Z / rl, 0.0, 0.0, 0.0],
            [0.0, 2.0 * NEAR_Z / tb, 0.0, 0.0],
            [(r + l) / rl, (t + b) / tb, -(FAR_Z + NEAR_Z) / fnr, -1.0],
            [0.0, 0.0, -2.0 * FAR_Z * NEAR_Z / fnr, 0.0],
        ])
    }

    /// The eye-at-origin view_proj as column-major arrays (for sky ray inverse).
    pub fn view_proj(&self) -> [[f32; 4]; 4] {
        self.view_proj
    }

    /// Build from the player view + a prebuilt projection matrix.
    pub fn new(view: &RenderView, projection: Mat4) -> Self {
        let pos = Vec3::new(view.x.into(), view.y.into(), view.viewz.into());
        let basis = EyeBasis::new(view);
        let view_matrix = Mat4::look_at_rh(
            Vec3::ZERO,
            Vec3::from_array(basis.forward),
            Vec3::from_array(basis.up),
        );
        let view_proj = projection * view_matrix * world_stretch();
        Self {
            view_proj: view_proj.to_cols_array_2d(),
            camera_pos: pos.to_array(),
            extralight: view.extralight as f32,
        }
    }
}
