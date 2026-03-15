//! Softbuffer display backend — presents pixels via winit + softbuffer.

use std::num::NonZeroU32;
use std::sync::Arc;

#[cfg(feature = "hprof")]
use coarse_prof::profile;
use softbuffer::{Context, Pixel, Surface};
use winit::window::Window;

use crate::DrawBuffer;

/// Transmute a `&mut [Pixel]` to `&mut [u32]` for bulk pixel operations.
///
/// # Safety
/// `Pixel` is `#[repr(C, align(4))]` with 4 `u8` fields — same size and
/// alignment as `u32`. The softbuffer documentation explicitly endorses this
/// transmute pattern.
#[inline(always)]
fn pixels_as_u32_mut(pixels: &mut [Pixel]) -> &mut [u32] {
    unsafe { std::mem::transmute::<&mut [Pixel], &mut [u32]>(pixels) }
}

/// Softbuffer display: owns the surface and a reference to the window.
///
/// The `Context` is leaked (`Box::leak`) to satisfy `Surface`'s borrow of
/// `&Context`. This is fine — there is exactly one display for the lifetime
/// of the process.
pub struct SoftbufferDisplay {
    surface: Surface<Arc<Window>, Arc<Window>>,
    window: Arc<Window>,
}

impl SoftbufferDisplay {
    /// Create from a winit window. The window must be wrapped in `Arc`.
    pub fn new(window: Arc<Window>) -> Self {
        let ctx: &'static Context<Arc<Window>> = Box::leak(Box::new(
            Context::new(window.clone()).expect("failed to create softbuffer context"),
        ));
        let surface: Surface<Arc<Window>, Arc<Window>> =
            Surface::new(ctx, window.clone()).expect("failed to create softbuffer surface");
        Self {
            surface,
            window,
        }
    }

    /// Present the draw buffer to the screen. The surface is sized to match
    /// the buffer; the compositor scales to fill the window.
    pub(crate) fn blit(&mut self, buffer: &DrawBuffer) {
        #[cfg(feature = "hprof")]
        profile!("softbuffer_blit");
        let buf_w = buffer.size.width_usize() as u32;
        let buf_h = buffer.size.height_usize() as u32;

        self.surface
            .resize(
                NonZeroU32::new(buf_w).unwrap_or(NonZeroU32::new(1).unwrap()),
                NonZeroU32::new(buf_h).unwrap_or(NonZeroU32::new(1).unwrap()),
            )
            .expect("failed to resize softbuffer surface");

        let mut sb = self
            .surface
            .next_buffer()
            .expect("failed to get softbuffer buffer");

        // IOSurface rows may be padded for cache-line alignment, so the
        // surface pitch can exceed the logical width.
        let stride_bytes = sb.byte_stride().get() as usize;
        let stride_px = stride_bytes / size_of::<Pixel>();
        let w = buf_w as usize;
        let dst = pixels_as_u32_mut(sb.pixels());

        if stride_px == w {
            let pixel_count = (buf_w * buf_h) as usize;
            dst[..pixel_count].copy_from_slice(&buffer.buffer[..pixel_count]);
        } else {
            for y in 0..buf_h as usize {
                let dst_row = &mut dst[y * stride_px..y * stride_px + w];
                let src_row = &buffer.buffer[y * w..y * w + w];
                dst_row.copy_from_slice(src_row);
            }
        }

        {
            #[cfg(feature = "hprof")]
            profile!("softbuffer_present");
            sb.present().expect("failed to present softbuffer");
        }
    }

    /// Window size in logical pixels.
    pub fn window_size(&self) -> (u32, u32) {
        let size = self.window.inner_size();
        (size.width, size.height)
    }
}
