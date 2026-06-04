//! The active renderer, one boxed implementation behind a bare enum.
//!
//! The CPU renderers ([`software25d`]/[`software3d`]) draw final pixels into a
//! [`PixelTarget`]; the GPU renderer ([`wgpu3d`]) records into a borrowed
//! [`GpuHandle`]. [`RenderStack`](crate::RenderStack) matches this enum once per
//! dispatch — no traits, no object safety to satisfy.

use std::sync::Arc;

use level::LevelData;
use pic_data::{PicData, PixelFmt, VoxelManager};
use render_common::{PixelTarget, RenderView};

#[cfg(feature = "software3d")]
use hud_util::{draw_text_line, hud_scale, measure_text_line};
#[cfg(feature = "software3d")]
use render_common::DrawBuffer as _;
#[cfg(feature = "software3d")]
use software3d::Software3D;
#[cfg(feature = "software25d")]
use software25d::Software25D;
#[cfg(feature = "wgpu3d")]
use wgpu3d::{GpuHandle, RenderConfig, Wgpu3D};

use crate::RenderType;

/// The active renderer. Exactly one variant per the chosen [`RenderType`].
pub enum WorldRenderer {
    #[cfg(feature = "software25d")]
    Software(Box<Software25D>),
    #[cfg(feature = "software3d")]
    Software3D(Box<Software3D>),
    #[cfg(feature = "wgpu3d")]
    Wgpu3D(Box<Wgpu3D>),
}

impl WorldRenderer {
    /// Build the renderer for `render_type` at the given buffer size (px).
    pub(crate) fn new(render_type: RenderType, buf_width: f32, buf_height: f32) -> Self {
        let hfov = 90f32.to_radians();
        match render_type {
            #[cfg(feature = "software25d")]
            RenderType::Software => Self::Software(Box::new(Software25D::new(
                hfov,
                buf_width,
                buf_height,
                buf_height > 200.0,
            ))),
            #[cfg(feature = "software3d")]
            RenderType::Software3D => {
                Self::Software3D(Box::new(Software3D::new(buf_width, buf_height, hfov)))
            }
            #[cfg(feature = "wgpu3d")]
            RenderType::Wgpu3D => Self::Wgpu3D(Box::new(Wgpu3D::new(buf_width, buf_height, hfov))),
        }
    }

    #[allow(clippy::unused_self)]
    pub(crate) fn is_wgpu3d(&self) -> bool {
        #[cfg(feature = "wgpu3d")]
        {
            matches!(self, Self::Wgpu3D(_))
        }
        #[cfg(not(feature = "wgpu3d"))]
        false
    }

    /// Push a new view height (statusbar toggle) to the renderer.
    pub(crate) fn set_view_height(&mut self, vh: i32) {
        match self {
            #[cfg(feature = "software25d")]
            Self::Software(r) => r.set_view_height(vh),
            #[cfg(feature = "software3d")]
            Self::Software3D(r) => r.set_view_height(vh),
            #[cfg(feature = "wgpu3d")]
            Self::Wgpu3D(_) => {}
        }
    }

    /// Set the voxel manager. Only software3d / wgpu3d support voxels.
    pub(crate) fn set_voxel_manager(&mut self, mgr: Arc<VoxelManager>) {
        match self {
            #[cfg(feature = "software3d")]
            Self::Software3D(r) => r.set_voxel_manager(mgr),
            #[cfg(feature = "wgpu3d")]
            Self::Wgpu3D(r) => r.set_voxel_manager(mgr),
            #[allow(unreachable_patterns)]
            _ => drop(mgr),
        }
    }

    pub(crate) fn clear_voxel_manager(&mut self) {
        match self {
            #[cfg(feature = "software3d")]
            Self::Software3D(r) => r.clear_voxel_manager(),
            #[cfg(feature = "wgpu3d")]
            Self::Wgpu3D(r) => r.clear_voxel_manager(),
            #[allow(unreachable_patterns)]
            _ => {}
        }
    }

    #[cfg(feature = "wgpu3d")]
    pub(crate) fn set_dynamic_sky(&mut self, dynamic: bool) {
        if let Self::Wgpu3D(r) = self {
            r.set_dynamic_sky(dynamic);
        }
    }

    /// CPU path: render the player view into `buf`, then draw software3d's debug
    /// overlays + text line on top. No-op on the GPU renderer (it has its own
    /// path; the scene is never drawn into a `[P]` surface).
    #[cfg(feature = "cpu-render")]
    pub(crate) fn draw_view<P: PixelFmt>(
        &mut self,
        view: &RenderView,
        level_data: &LevelData,
        pic_data: &mut PicData,
        buf: &mut PixelTarget<P>,
    ) {
        match self {
            #[cfg(feature = "software25d")]
            Self::Software(r) => {
                r.draw_view(view, level_data, pic_data, buf);
            }
            #[cfg(feature = "software3d")]
            Self::Software3D(r) => {
                r.draw_view(view, level_data, pic_data, buf);
            }
            #[cfg(feature = "wgpu3d")]
            Self::Wgpu3D(_) => {}
        }
    }

    /// CPU path: software3d's debug outline/normal overlays and the upper-right
    /// debug text line, drawn after the scene. No-op on every other renderer.
    #[cfg(feature = "cpu-render")]
    #[cfg_attr(not(feature = "software3d"), allow(unused_variables))]
    pub(crate) fn draw_debug_overlays<P: PixelFmt>(
        &mut self,
        pic_data: &PicData,
        buf: &mut PixelTarget<P>,
    ) {
        match self {
            #[cfg(feature = "software3d")]
            Self::Software3D(r) => {
                let text = r.take_debug_line();
                r.draw_debug_overlays(buf);
                if !text.is_empty() {
                    let (sx, sy) = hud_scale(buf);
                    let palette = pic_data.wad_palette();
                    let width = measure_text_line(&text, sx);
                    let x = buf.size().width_f32() - width - 4.0 * sx;
                    draw_text_line(&text, x, 2.0, sx, sy, palette, buf);
                }
            }
            #[allow(unreachable_patterns)]
            _ => {}
        }
    }

    /// GPU path: record the player view into the borrowed [`GpuHandle`]'s scene
    /// texture. Panics if the active renderer is not the GPU one (a
    /// [`RenderStack`](crate::RenderStack) construction invariant).
    #[cfg(feature = "wgpu3d")]
    pub(crate) fn draw_view_gpu(
        &mut self,
        view: &RenderView,
        level_data: &LevelData,
        pic_data: &PicData,
        light_gamma: f32,
        handle: &mut GpuHandle<'_>,
    ) {
        let Self::Wgpu3D(r) = self else {
            unreachable!("draw_view_gpu requires the wgpu3d renderer");
        };
        let config = RenderConfig {
            light_gamma,
        };
        r.draw_view_gpu(view, level_data, pic_data, &config, handle);
    }
}
