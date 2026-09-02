//! Stereo rendering against an OpenXR runtime.
//!
//! This is a *backend*, in the sense `render/backend` uses the word: it owns
//! how a frame reaches the display. The display happens to be a headset, so
//! the runtime — not a window — decides when a frame starts, and there are
//! two of them per tic.
//!
//! The renderer is untouched: it records into the per-eye texture views
//! this crate hands out, exactly as it records into a backend's scene
//! target today. Controllers arrive through the OpenXR action system as
//! plain state the game folds into the input it already has.
//!
//! `XR_KHR_metal_enable` requires the application's command queue to live on
//! the `MTLDevice` the runtime names, so wgpu cannot create its own device
//! here. [`Session`] builds one from that raw device via `wgpu-hal`, which
//! is an unstable surface — the wgpu version is pinned deliberately.

mod actions;
mod device;
mod hands;
mod session;
mod xr;

pub use actions::{ControllerState, Pose, SnapTurn};
pub use hands::{HandJoints, JOINT_COUNT, Joint};
pub use session::{EyeCamera, EyeTarget, FrameContext, Session, VrError};

/// Renderer view format for already sRGB-encoded pixels. The runtime's sRGB image
/// shares this view's storage, so the renderer must not apply another transfer function.
pub const SWAPCHAIN_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;

/// Views per frame. Fixed by the primary-stereo view configuration.
pub const EYE_COUNT: usize = 2;
