//! GPU canvas rendering: [`frame`] builds geometry, [`atlas`] packs textures, [`wgpu`] owns
//! pipelines and [`sync`] reconciles editing state. Native view publication and animation
//! scheduling live with the map tab.

pub mod atlas;
pub(crate) mod cache;
pub mod camera3d;
pub mod editor_camera;
pub mod frame;
pub mod frame3d;
pub mod input;
#[cfg(target_os = "macos")]
mod native_surface;
pub mod sprites;
pub mod style;
pub(crate) mod sync;
pub mod triangulate;
pub mod view;
pub mod wgpu;

pub(crate) use sync::export_camera;

/// FNV-1a seed + prime, shared by the sector-colour hash and the atlas content key.
pub(crate) const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
pub(crate) const FNV_PRIME: u64 = 0x100_0000_01b3;

/// One FNV-1a fold step.
pub(crate) fn fnv_fold(h: u64, v: u64) -> u64 {
    (h ^ v).wrapping_mul(FNV_PRIME)
}
