//! GPU canvas rendering: [`frame`] builds vertex buffers, [`atlas`] packs textures,
//! [`wgpu`] owns the device + pipelines, [`sync`] is the sole app-coupled bridge.

pub mod atlas;
pub mod camera3d;
pub mod editor_camera;
pub mod frame;
pub mod frame3d;
pub mod input;
pub mod sprites;
pub mod style;
mod sync;
pub mod triangulate;
pub mod view;
pub mod wgpu;

pub(crate) use sync::{
    apply_damage, export_camera, push_wgpu_frame, regrid_and_paint, repaint_canvas,
    stop_light_timer,
};

/// FNV-1a seed + prime, shared by the sector-colour hash and the atlas content key.
pub(crate) const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
pub(crate) const FNV_PRIME: u64 = 0x100_0000_01b3;

/// One FNV-1a fold step.
pub(crate) fn fnv_fold(h: u64, v: u64) -> u64 {
    (h ^ v).wrapping_mul(FNV_PRIME)
}
