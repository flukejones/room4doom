//! wgpu display backend — presents the engine framebuffer via winit + wgpu.
//!
//! Owns the wgpu device/queue/surface directly (no `pixels` wrapper). The
//! window-facing [`WgpuBackend`] lives here; the pipeline it drives is split by
//! concern: [`presenter`] (frame lifecycle + present), [`post`] (the
//! post-process chain), [`scene`] (targets, composite, melt), [`panel`] (UI
//! quads) and [`headset`] (VR state machine).

#[cfg(feature = "vr")]
mod headset;
#[cfg(feature = "wgpu3d")]
mod panel;
mod post;
mod presenter;
#[cfg(feature = "wgpu3d")]
mod scene;

use std::slice::from_raw_parts;
#[cfg(not(feature = "display-sdl2"))]
use std::sync::Arc;

#[cfg(all(feature = "wgpu3d", not(feature = "display-sdl2")))]
use ::wgpu3d::GpuHandle;
#[cfg(any(not(feature = "display-sdl2"), feature = "wgpu3d"))]
use pic_data::PixelFmt;
pub use post::PostEffect;
pub(crate) use presenter::GpuPresenter;
#[cfg(all(feature = "vr", not(feature = "display-sdl2")))]
use render_common::{EYE_COUNT, EyeView};
#[cfg(not(feature = "display-sdl2"))]
use winit::window::{Fullscreen, Window};

#[cfg(all(feature = "vr", not(feature = "display-sdl2")))]
use crate::backend::PanelPlacement;
#[cfg(not(feature = "display-sdl2"))]
use crate::backend::{Backend, RenderKind, SoftwarePresent};
#[cfg(all(feature = "wgpu3d", not(feature = "display-sdl2")))]
use crate::backend::{HardwarePresent, ScreenEffects, UiPlanes};

/// Engine framebuffer + surface pixel format. The engine writes `0xFFRRGGBB`,
/// whose little-endian bytes are `[BB,GG,RR,FF]` = BGRA, so `Bgra8Unorm` is a
/// straight upload. Non-sRGB: the palette is already gamma-baked.
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;

/// Bytes per framebuffer pixel (`u32`). Used by the software-present upload (the
/// winit wgpu backend; sdl2's hardware mode uses only the GPU present path).
#[cfg(not(feature = "display-sdl2"))]
const PIXEL_BYTES: u32 = size_of::<u32>() as u32;

/// Reinterpret a `&[u32]` as a byte slice for upload (no copy).
#[inline]
fn bytemuck_cast(px: &[u32]) -> &[u8] {
    // SAFETY: u32 is 4 contiguous bytes; the resulting slice covers the same
    // bytes with 4x the length and tighter (byte) alignment.
    unsafe { from_raw_parts(px.as_ptr().cast::<u8>(), size_of_val(px)) }
}

/// Reinterpret the engine `[P]` framebuffer as `[u32]` for the wgpu upload. The
/// wgpu backend is `u32`-only (`Bgra8Unorm`); `P` is always `u32` here (asserted).
#[cfg(any(not(feature = "display-sdl2"), feature = "wgpu3d"))]
#[inline]
pub(crate) fn as_u32_slice<P: PixelFmt>(front: &[P]) -> &[u32] {
    assert_eq!(
        size_of::<P>(),
        size_of::<u32>(),
        "wgpu backend is u32-only; Rgb565 must fall back to Rgb888"
    );
    // SAFETY: P == u32 (asserted); `front` is the tight engine framebuffer.
    unsafe { from_raw_parts(front.as_ptr().cast::<u32>(), front.len()) }
}

/// wgpu display backend: a winit window plus the window-agnostic
/// [`GpuPresenter`]. Implements the [`Backend`] + present traits, sourcing the
/// window size for each present and owning the fullscreen toggle. Hosts both the
/// software renderers (CPU upload) and the GPU `wgpu3d` renderer. `u32`-only; the
/// `P` parameter (always `u32` here) keeps `ActiveBackend<P>` uniform. The winit
/// display backend; when sdl2 is the active backend this is unused.
#[cfg(not(feature = "display-sdl2"))]
pub struct WgpuBackend<P: PixelFmt> {
    window: Arc<Window>,
    presenter: GpuPresenter,
    _p: std::marker::PhantomData<P>,
}

#[cfg(not(feature = "display-sdl2"))]
impl<P: PixelFmt> WgpuBackend<P> {
    /// Create against a VR runtime. The window is still owned — it carries
    /// input, the OS activation the game needs, and a mirror of one eye —
    /// but the headset is what the frame is sized and paced for.
    #[cfg(feature = "vr")]
    pub(crate) fn new_vr(window: Arc<Window>, session: vr::Session, post: Vec<PostEffect>) -> Self {
        let size = window.inner_size();
        let (sw, sh) = (size.width.max(1), size.height.max(1));
        let presenter = GpuPresenter::from_vr(window.clone(), session, post, sw, sh);
        Self {
            window,
            presenter,
            _p: std::marker::PhantomData,
        }
    }

    /// The live headset session, for gaze, controllers and play space. `None`
    /// on a window.
    #[cfg(feature = "vr")]
    pub(crate) fn session(&mut self) -> Option<&mut vr::Session> {
        self.presenter.vr_session_mut()
    }

    /// Create from a winit window. The window must be wrapped in `Arc`; its
    /// clone becomes the wgpu surface target.
    pub(crate) fn new(window: Arc<Window>, vsync: bool, post: Vec<PostEffect>) -> Self {
        let size = window.inner_size();
        let (sw, sh) = (size.width.max(1), size.height.max(1));
        let presenter = GpuPresenter::from_target(window.clone(), vsync, post, sw, sh);
        Self {
            window,
            presenter,
            _p: std::marker::PhantomData,
        }
    }

    /// Window inner size, clamped to a minimum of 1×1 for surface configuration.
    fn window_inner(&self) -> (u32, u32) {
        let size = self.window.inner_size();
        (size.width.max(1), size.height.max(1))
    }
}

#[cfg(not(feature = "display-sdl2"))]
impl<P: PixelFmt> Backend for WgpuBackend<P> {
    /// The display's proportions, which is what the engine buffer is sized
    /// against. In VR that is the headset's field of view, not the window
    /// and not the eye's pixel count: the image has to fill what the lens
    /// shows.
    fn window_size(&self) -> (u32, u32) {
        #[cfg(feature = "vr")]
        if let Some((w, h)) = self.presenter.vr_display_size() {
            return (w, h);
        }
        let size = self.window.inner_size();
        (size.width, size.height)
    }

    fn set_fullscreen(&mut self, mode: u8) {
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

    fn supports(&self, _: RenderKind) -> bool {
        true
    }
}

#[cfg(not(feature = "display-sdl2"))]
impl<P: PixelFmt> SoftwarePresent<P> for WgpuBackend<P> {
    fn present(&mut self, front: &[P], w: u32, h: u32) {
        let (win_w, win_h) = self.window_inner();
        self.presenter.present_software(front, w, h, win_w, win_h);
    }
}

#[cfg(all(feature = "wgpu3d", not(feature = "display-sdl2")))]
impl<P: PixelFmt> HardwarePresent for WgpuBackend<P> {
    fn set_screen_effects(&mut self, effects: ScreenEffects, w: u32, h: u32) {
        self.presenter.set_effects(effects, w, h);
    }

    fn start_wipe(&mut self, w: u32, h: u32) {
        self.presenter.start_wipe(w, h);
    }

    fn is_wiping(&self) -> bool {
        self.presenter.is_wiping()
    }

    fn begin_scene(&mut self, w: u32, h: u32) -> GpuHandle<'_> {
        let (win_w, win_h) = self.window_inner();
        self.presenter.begin_scene(w, h, win_w, win_h)
    }

    fn eye_count(&self) -> usize {
        self.presenter.eye_count()
    }

    #[cfg(feature = "vr")]
    fn begin_frame(&mut self) -> Option<[Option<EyeView>; EYE_COUNT]> {
        self.presenter.vr_begin_frame()
    }

    #[cfg(feature = "vr")]
    fn set_eye(&mut self, eye: usize) {
        self.presenter.set_eye(eye);
    }

    #[cfg(feature = "vr")]
    fn anchor_panels(&mut self) {
        self.presenter.anchor_panels();
    }

    #[cfg(feature = "vr")]
    fn set_panels(&mut self, hud: PanelPlacement, screen: PanelPlacement) {
        self.presenter.set_panels(hud, screen);
    }

    fn advance_wipe(&mut self) -> bool {
        self.presenter.advance_wipe()
    }

    fn upload_ui(&mut self, planes: UiPlanes<'_>) {
        self.presenter.upload_ui(planes);
    }

    fn finish_frame(&mut self, scene_w: u32, scene_h: u32, wiping: bool) {
        let (win_w, win_h) = self.window_inner();
        self.presenter
            .finish_frame(scene_w, scene_h, win_w, win_h, wiping);
    }

    fn reset_health_bleed(&mut self) {
        self.presenter.reset_health_bleed();
    }
}

#[cfg(test)]
mod tests {
    /// Parse + validate the backend WGSL passes offline (no GPU): catches syntax
    /// and the composite uniform layout/type faults before they reach the device.
    #[test]
    fn shaders_validate() {
        for (name, src) in [
            ("composite", include_str!("composite.wgsl")),
            ("panel", include_str!("panel.wgsl")),
            ("melt", include_str!("melt.wgsl")),
            ("stretch", include_str!("stretch.wgsl")),
            ("lottes-crt", include_str!("lottes-crt.wgsl")),
        ] {
            let module = naga::front::wgsl::parse_str(src)
                .unwrap_or_else(|e| panic!("{name} parses: {e:?}"));
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::all(),
            )
            .validate(&module)
            .unwrap_or_else(|e| panic!("{name} validates: {e:?}"));
        }
    }
}
