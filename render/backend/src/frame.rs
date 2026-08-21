//! The CPU pixel surfaces the UI and software renderers draw into.
//!
//! [`UiSurface`] is one `[P]` plane and the [`DrawBuffer`] every UI subsystem
//! draws through. [`Frame`] adds what *presenting* needs: a second plane
//! holding the last presented image and the melt-[`Wipe`] that overdraws it.
//! A headset's UI panels are bare `UiSurface`s — never flipped, never melted,
//! since the GPU melt runs on the composited eye image instead.

use pic_data::{ByteOrder, PalLit, PixelFmt};
use render_common::wipe::Wipe;
use render_common::{BufferSize, DrawBuffer, PixelTarget, UiLayout};

/// A single CPU pixel plane that UI draws into.
pub struct UiSurface<P: PixelFmt> {
    buf: Vec<P>,
    w: u32,
    h: u32,
    /// Cached dimensions for the `DrawBuffer::size` borrow; kept in sync with w/h.
    size: BufferSize,
    layout: UiLayout,
    /// Byte order the surface consumes; `set_pixel` converts `0xAARRGGBB` to it.
    order: ByteOrder,
    /// A clean plane costs nothing: no clear, no upload, no draw.
    dirty: bool,
}

impl<P: PixelFmt> UiSurface<P> {
    /// A `w`×`h` plane cleared to opaque black.
    pub fn new(w: u32, h: u32, order: ByteOrder) -> Self {
        let black = P::from_argb(0xFF00_0000, order);
        Self {
            buf: vec![black; (w * h) as usize],
            w,
            h,
            size: BufferSize::new(w as usize, h as usize),
            layout: UiLayout::from_sizes((w, h), (w, h)),
            order,
            dirty: true,
        }
    }

    /// Resize to `w`×`h`, reallocating. No-op when unchanged.
    pub fn resize(&mut self, w: u32, h: u32) {
        if (self.w, self.h) == (w, h) {
            return;
        }
        let black = P::from_argb(0xFF00_0000, self.order);
        self.buf = vec![black; (w * h) as usize];
        self.w = w;
        self.h = h;
        self.size = BufferSize::new(w as usize, h as usize);
        self.set_display_size((w, h));
        self.dirty = true;
    }

    /// Set the final displayed rectangle so UI accounts for presentation stretch.
    pub fn set_display_size(&mut self, display: (u32, u32)) {
        self.layout = UiLayout::from_sizes((self.w, self.h), display);
    }

    /// Scale a finished software scene into this presentation surface.
    #[cfg(any(feature = "cpu-render", test))]
    pub(crate) fn copy_scaled(&mut self, source: &Self) {
        let (w, h) = (self.w as usize, self.h as usize);
        let (sw, sh) = (source.w as usize, source.h as usize);
        for y in 0..h {
            let row = &source.buf[(y * sh / h) * sw..][..sw];
            for (x, pixel) in self.buf[y * w..][..w].iter_mut().enumerate() {
                *pixel = row[x * sw / w];
            }
        }
        self.dirty = true;
    }

    /// Clear to fully transparent (`0x00000000`), so a composite shows the scene
    /// through the unwritten texels. No-op on an already-clean plane.
    pub fn clear_transparent(&mut self) {
        if !self.dirty {
            return;
        }
        let clear = P::from_argb(0x0000_0000, self.order);
        self.buf.fill(clear);
        self.dirty = false;
    }

    /// Whether anything drew since the last clear.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// The plane's pixels, for the backend to stream/blit/upload.
    pub fn pixels(&self) -> &[P] {
        &self.buf
    }

    pub fn width(&self) -> u32 {
        self.w
    }

    pub fn height(&self) -> u32 {
        self.h
    }

    /// A [`PixelTarget`] over the plane for the scene render. `tint` is the
    /// active PLAYPAL palette index; `pal_lit` resolves lit indices.
    pub fn pixel_target<'a>(
        &'a mut self,
        pal_lit: &'a PalLit<P>,
        tint: usize,
    ) -> PixelTarget<'a, P> {
        self.dirty = true;
        PixelTarget::new(
            &mut self.buf,
            BufferSize::new(self.w as usize, self.h as usize),
            self.w as usize,
            pal_lit,
            tint,
        )
    }
}

impl<P: PixelFmt> DrawBuffer for UiSurface<P> {
    type Pixel = P;

    #[inline]
    fn size(&self) -> &BufferSize {
        &self.size
    }

    fn ui_layout(&self) -> UiLayout {
        self.layout
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
        self.dirty = true;
        let pos = y * self.w as usize + x;
        // SAFETY: The drawing caller clips positions to the target dimensions; the backing
        // slice covers every pixel, including its row pitch.
        unsafe {
            *self.buf.get_unchecked_mut(pos) = P::from_argb(colour, self.order);
        }
    }

    #[inline]
    fn buf_mut(&mut self) -> &mut [P] {
        self.dirty = true;
        &mut self.buf
    }
}

/// Double-buffered CPU framebuffer + melt-wipe. One per [`crate::RenderStack`].
///
/// ```no_run
/// use pic_data::ByteOrder;
/// use render_backend::Frame;
/// use render_common::DrawBuffer;
///
/// let mut frame: Frame<u32> = Frame::new(320, 200, ByteOrder::Argb);
/// frame.surface_mut().set_pixel(0, 0, 0xFFFF_0000); // draw UI into `front`
/// frame.flip();                                     // present: front<->back
/// frame.start_wipe();                               // melt the prior frame over the next
/// while !frame.do_wipe() {                          // advance until the melt completes
///     // re-render `front`, then present each frame
/// }
/// ```
pub struct Frame<P: PixelFmt> {
    /// Current frame: scene + UI draw here; presented each frame.
    front: UiSurface<P>,
    /// Last presented frame, kept by [`Self::flip`] on a gameplay present; the
    /// melt-wipe source.
    back: UiSurface<P>,
    wipe: Wipe,
}

impl<P: PixelFmt> Frame<P> {
    /// A `w`×`h` framebuffer, cleared to opaque black. `order` is the surface
    /// byte order (the `PalLit` is baked in it).
    pub fn new(w: u32, h: u32, order: ByteOrder) -> Self {
        Self {
            front: UiSurface::new(w, h, order),
            back: UiSurface::new(w, h, order),
            wipe: Wipe::new(w as i32, h as i32),
        }
    }

    /// The plane UI and the scene draw into.
    pub fn surface_mut(&mut self) -> &mut UiSurface<P> {
        &mut self.front
    }

    /// The plane UI drew into.
    pub fn surface(&self) -> &UiSurface<P> {
        &self.front
    }

    pub(crate) fn set_display_size(&mut self, display: (u32, u32)) {
        self.front.set_display_size(display);
        self.back.set_display_size(display);
    }

    /// Resize to `w`×`h` (reallocates + re-seeds the wipe geometry).
    pub fn resize(&mut self, w: u32, h: u32) {
        if (self.front.width(), self.front.height()) != (w, h) {
            self.front.resize(w, h);
            self.back.resize(w, h);
            self.wipe = Wipe::new(w as i32, h as i32);
        }
    }

    /// A [`PixelTarget`] over `front` for the scene render.
    pub fn pixel_target<'a>(
        &'a mut self,
        pal_lit: &'a PalLit<P>,
        tint: usize,
    ) -> PixelTarget<'a, P> {
        self.front.pixel_target(pal_lit, tint)
    }

    /// Clear `front` to fully transparent. The GPU path draws UI over a
    /// transparent field so the composite shows the scene through; the software
    /// path never needs this (the scene fills `front`).
    pub fn clear_transparent(&mut self) {
        self.front.clear_transparent();
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
    /// advancing the melt. Returns true when the wipe completes. Both planes are
    /// tight (`w`), so the pitches are equal.
    pub fn do_wipe(&mut self) -> bool {
        let pitch = self.front.width() as usize;
        self.wipe
            .do_melt_pixels(self.front.buf_mut(), pitch, self.back.pixels(), pitch)
    }

    /// Swap front/back so the just-presented frame becomes the next wipe source.
    /// The gameplay present path calls this; the wipe path does not (so `back`
    /// stays the frozen old frame across the melt).
    pub fn flip(&mut self) {
        std::mem::swap(&mut self.front, &mut self.back);
    }

    /// The current frame's pixels, for the backend to stream/blit to the window.
    pub fn front(&self) -> &[P] {
        self.front.pixels()
    }

    pub fn width(&self) -> u32 {
        self.front.width()
    }

    pub fn height(&self) -> u32 {
        self.front.height()
    }
}

#[cfg(test)]
mod tests {
    use hud_util::draw_patch;
    use render_common::DrawBuffer;
    use test_utils::doom1_wad_path;
    use wad::WadData;
    use wad::types::{WadPalette, WadPatch};

    use super::{ByteOrder, PixelFmt, UiSurface};

    #[expect(
        clippy::cast_possible_truncation,
        reason = "test bounds are pixel coordinates inside the surface"
    )]
    fn check_title<P: PixelFmt + PartialEq>() {
        let wad = WadData::new(&doom1_wad_path());
        let patch = WadPatch::from_lump(wad.get_lump("TITLEPIC").expect("shareware title"));
        let palette = wad
            .lump_iter::<WadPalette>("PLAYPAL")
            .next()
            .expect("shareware palette");
        let sentinel = P::from_argb(0xFFFF_00FF, ByteOrder::Argb);
        for (w, h, display) in [
            (320, 200, (640, 480)),
            (640, 480, (640, 480)),
            (1920, 1080, (1920, 1080)),
            (480, 640, (480, 640)),
        ] {
            let mut surface = UiSurface::<P>::new(w, h, ByteOrder::Argb);
            surface.set_display_size(display);
            surface.buf_mut().fill(sentinel);
            let borrowed = &mut surface;
            let layout = DrawBuffer::ui_layout(&borrowed);
            draw_patch(
                &patch,
                layout.origin[0],
                layout.origin[1],
                layout.scale[0],
                layout.scale[1],
                &palette,
                borrowed,
            );
            let left = layout.origin[0].round() as usize;
            let top = layout.origin[1].round() as usize;
            let right = (layout.origin[0] + layout.width()).round() as usize;
            let bottom = (layout.origin[1] + layout.height()).round() as usize;
            for y in 0..h as usize {
                for x in 0..w as usize {
                    let inside = (left..right).contains(&x) && (top..bottom).contains(&y);
                    assert_eq!(
                        surface.pixels()[y * w as usize + x] != sentinel,
                        inside,
                        "title coverage at {x},{y} in {w}×{h}"
                    );
                }
            }
            let mut world = UiSurface::<P>::new(2, 2, ByteOrder::Argb);
            world
                .buf_mut()
                .copy_from_slice(&[sentinel, P::default(), P::default(), sentinel]);
            surface.copy_scaled(&world);
            assert!(surface.pixels()[0] == sentinel);
            assert!(surface.pixels()[w as usize - 1] == P::default());
            assert!(surface.pixels()[(w * h - 1) as usize] == sentinel);
        }
    }

    #[test]
    fn generic_title_drawing_preserves_crt_aspect_and_pixel_formats() {
        check_title::<u32>();
        check_title::<u16>();
    }
}
