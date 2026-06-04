//! `Frame<P>`: the CPU framebuffer shared by every software-present backend.
//!
//! Owns a double-buffered `[P]` store (`front` drawn + presented, `back` holding
//! the last presented frame) plus the melt-[`Wipe`] state. It IS a [`DrawBuffer`]
//! (UI/overlays draw straight into `front`) and yields a [`PixelTarget`] for the
//! scene render. The melt-wipe overdraws the last frame (`back`) onto the freshly
//! rendered `front`. Backends never touch any of this — they only stream
//! [`Frame::front`] to the window.

use pic_data::{ByteOrder, PalLit, PixelFmt};
use render_common::wipe::Wipe;
use render_common::{BufferSize, DrawBuffer, PixelTarget};

/// Double-buffered CPU framebuffer + melt-wipe. One per [`crate::RenderStack`].
///
/// ```no_run
/// use pic_data::ByteOrder;
/// use render_backend::Frame;
/// use render_common::DrawBuffer;
///
/// let mut frame: Frame<u32> = Frame::new(320, 200, ByteOrder::Argb);
/// frame.set_pixel(0, 0, 0xFFFF_0000); // draw UI into `front`
/// frame.flip();                       // present: front<->back
/// frame.start_wipe();                 // melt the prior frame over the next
/// while !frame.do_wipe() {            // advance until the melt completes
///     // re-render `front`, then present each frame
/// }
/// ```
pub struct Frame<P: PixelFmt> {
    /// Current frame: scene + UI draw here; presented each frame.
    front: Vec<P>,
    /// Last presented frame, kept by [`Self::flip`] on a gameplay present; the
    /// melt-wipe source.
    back: Vec<P>,
    wipe: Wipe,
    w: u32,
    h: u32,
    /// Cached dimensions for the `DrawBuffer::size` borrow; kept in sync with w/h.
    size: BufferSize,
    /// Byte order the surface consumes; UI `set_pixel` converts `0xAARRGGBB` to it.
    order: ByteOrder,
}

impl<P: PixelFmt> Frame<P> {
    /// A `w`×`h` framebuffer, cleared to opaque black. `order` is the surface
    /// byte order (the `PalLit` is baked in it).
    pub fn new(w: u32, h: u32, order: ByteOrder) -> Self {
        let len = (w * h) as usize;
        let black = P::from_argb(0xFF00_0000, order);
        Self {
            front: vec![black; len],
            back: vec![black; len],
            wipe: Wipe::new(w as i32, h as i32),
            w,
            h,
            size: BufferSize::new(w as usize, h as usize),
            order,
        }
    }

    /// Resize to `w`×`h` (reallocates + re-seeds the wipe geometry).
    pub fn resize(&mut self, w: u32, h: u32) {
        if (self.w, self.h) != (w, h) {
            let len = (w * h) as usize;
            let black = P::from_argb(0xFF00_0000, self.order);
            self.front = vec![black; len];
            self.back = vec![black; len];
            self.wipe = Wipe::new(w as i32, h as i32);
            self.w = w;
            self.h = h;
            self.size = BufferSize::new(w as usize, h as usize);
        }
    }

    /// A [`PixelTarget`] over `front` for the scene render. `tint` is the active
    /// PLAYPAL palette index; `pal_lit` resolves lit indices. Tight pitch (`w`).
    pub fn pixel_target<'a>(
        &'a mut self,
        pal_lit: &'a PalLit<P>,
        tint: usize,
    ) -> PixelTarget<'a, P> {
        PixelTarget::new(
            &mut self.front,
            BufferSize::new(self.w as usize, self.h as usize),
            self.w as usize,
            pal_lit,
            tint,
        )
    }

    /// Clear `front` to fully transparent (`0x00000000`). The GPU path draws UI
    /// into `front` over a transparent field so the composite shows the scene
    /// through; the software path never needs this (the scene fills `front`).
    pub fn clear_transparent(&mut self) {
        let clear = P::from_argb(0x0000_0000, self.order);
        self.front.fill(clear);
    }

    /// Begin a melt-wipe. `back` already holds the previous frame (kept by the
    /// last gameplay [`Self::flip`]); just re-seed the column offsets.
    pub fn start_wipe(&mut self) {
        self.wipe.start();
    }

    /// True while a melt-wipe is in progress.
    pub fn is_wiping(&self) -> bool {
        self.wipe.is_wiping()
    }

    /// Overdraw the last frame (`back`) onto the freshly rendered `front`,
    /// advancing the melt. Returns true when the wipe completes. Both buffers are
    /// tight (`w`), so the pitches are equal.
    pub fn do_wipe(&mut self) -> bool {
        let pitch = self.w as usize;
        self.wipe
            .do_melt_pixels(&mut self.front, pitch, &self.back, pitch)
    }

    /// Swap front/back so the just-presented frame becomes the next wipe source.
    /// The gameplay present path calls this; the wipe path does not (so `back`
    /// stays the frozen old frame across the melt).
    pub fn flip(&mut self) {
        std::mem::swap(&mut self.front, &mut self.back);
    }

    /// The current frame's pixels, for the backend to stream/blit to the window.
    pub fn front(&self) -> &[P] {
        &self.front
    }

    pub fn width(&self) -> u32 {
        self.w
    }

    pub fn height(&self) -> u32 {
        self.h
    }
}

impl<P: PixelFmt> DrawBuffer for Frame<P> {
    type Pixel = P;

    #[inline]
    fn size(&self) -> &BufferSize {
        // BufferSize is Copy and cheap, but the trait returns a reference; hold a
        // cached one so callers get a stable borrow.
        &self.size
    }

    #[inline]
    fn pitch(&self) -> usize {
        self.w as usize
    }

    #[inline]
    fn get_buf_index(&self, x: usize, y: usize) -> usize {
        y * self.w as usize + x
    }

    #[inline]
    fn set_pixel(&mut self, x: usize, y: usize, colour: u32) {
        let pos = y * self.w as usize + x;
        unsafe {
            *self.front.get_unchecked_mut(pos) = P::from_argb(colour, self.order);
        }
    }

    #[inline]
    fn buf_mut(&mut self) -> &mut [P] {
        &mut self.front
    }
}
