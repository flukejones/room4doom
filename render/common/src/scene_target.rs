//! `PixelTarget<T>`: a scene target that stores final `T` pixels directly.
//!
//! Writes `pal_lit.block(tint)[lit]` to the display with no resolve pass.
//! Generic over `T: PixelFmt` (u16/u32); one monomorphised hot store per format.
//! Index-domain effects (bleed) use the index path instead.

use pic_data::{ByteOrder, PalLit, PixelFmt};

use crate::{BufferSize, DrawBuffer};

/// Stores `pal_lit.block(tint)[lit]` straight into the backend `T` surface.
pub struct PixelTarget<'a, T: PixelFmt> {
    surface: &'a mut [T],
    block: &'a [T; 256],
    size: BufferSize,
    pitch: usize,
    order: ByteOrder,
}

impl<'a, T: PixelFmt> PixelTarget<'a, T> {
    /// Wrap a raw surface slice. `tint` is the active PLAYPAL palette index
    /// (0 = normal); the block is hoisted once per frame.
    ///
    /// `pitch` is the surface row stride in `T` elements (may exceed width on a
    /// padded surface). The unchecked pixel stores require every `(x, y)` to
    /// satisfy `y * pitch + x < surface.len()`; callers must clip to the buffer.
    pub fn new(
        surface: &'a mut [T],
        size: BufferSize,
        pitch: usize,
        pal_lit: &'a PalLit<T>,
        tint: usize,
    ) -> Self {
        Self {
            block: pal_lit.block(tint),
            order: pal_lit.order(),
            surface,
            size,
            pitch,
        }
    }

    /// Resolve lit palette index `lit` (0..=255) to a final pixel via the active
    /// block.
    #[inline(always)]
    pub fn lookup(&self, lit: u16) -> T {
        // SAFETY: lit is 0..=255 (caller skips u16::MAX); block is [T; 256].
        unsafe { *self.block.get_unchecked(lit as usize) }
    }

    /// Write a resolved pixel at flat `pos` (`y * pitch() + x`).
    #[inline(always)]
    pub fn write(&mut self, pos: usize, px: T) {
        unsafe {
            *self.surface.get_unchecked_mut(pos) = px;
        }
    }

    /// Resolve lit palette index `lit` (0..=255; caller skips `u16::MAX`) and
    /// write it at flat `pos`. Per-pixel for paths where `lit` varies (walls/
    /// flats/sprites); flat-shaded runs hoist via `lookup`/`write`.
    #[inline(always)]
    pub fn store(&mut self, pos: usize, lit: u16) {
        let px = self.lookup(lit);
        self.write(pos, px);
    }

    /// Fuzz RMW: RGB-halve `src_pos`'s pixel into `dst_pos`.
    #[inline(always)]
    pub fn fuzz(&mut self, dst_pos: usize, src_pos: usize) {
        unsafe {
            let px = *self.surface.get_unchecked(src_pos);
            *self.surface.get_unchecked_mut(dst_pos) = px.darken();
        }
    }
}

impl<T: PixelFmt> DrawBuffer for PixelTarget<'_, T> {
    type Pixel = T;

    #[inline]
    fn size(&self) -> &BufferSize {
        &self.size
    }

    #[inline]
    fn pitch(&self) -> usize {
        self.pitch
    }

    #[inline]
    fn get_buf_index(&self, x: usize, y: usize) -> usize {
        y * self.pitch + x
    }

    #[inline]
    fn set_pixel(&mut self, x: usize, y: usize, colour: u32) {
        let pos = y * self.pitch + x;
        unsafe {
            *self.surface.get_unchecked_mut(pos) = T::from_argb(colour, self.order);
        }
    }

    #[inline]
    fn buf_mut(&mut self) -> &mut [T] {
        self.surface
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pic_data::WadPalette;

    fn pal<T: PixelFmt>() -> PalLit<T> {
        let mut pals: [WadPalette; pic_data::PALETTE_LEN] = Default::default();
        for (t, p) in pals.iter_mut().enumerate() {
            for (i, c) in p.0.iter_mut().enumerate() {
                *c = 0xFF00_0000 | ((i as u32) << 16) | ((t as u32) << 8) | i as u32;
            }
        }
        PalLit::new(&pals, ByteOrder::Argb)
    }

    #[test]
    fn u32_scene_store_writes_block() {
        let lit = pal::<u32>();
        let mut surface = [0u32; 2];
        let mut t = PixelTarget::new(&mut surface, BufferSize::new(2, 1), 2, &lit, 0);
        t.store(0, 5);
        t.store(1, 200);
        assert_eq!(surface[0], 0xFF00_0000 | (5 << 16) | 5);
        assert_eq!(surface[1], 0xFF00_0000 | (200 << 16) | 200);
    }

    #[test]
    fn u32_scene_store_uses_active_tint() {
        let lit = pal::<u32>();
        let mut surface = [0u32; 1];
        let mut t = PixelTarget::new(&mut surface, BufferSize::new(1, 1), 1, &lit, 3);
        t.store(0, 10);
        assert_eq!(surface[0], 0xFF00_0000 | (10 << 16) | (3 << 8) | 10);
    }

    #[test]
    fn u32_fuzz_halves_rgb_keeps_alpha() {
        let lit = pal::<u32>();
        let mut surface = [0xFF80_4020u32, 0];
        let mut t = PixelTarget::new(&mut surface, BufferSize::new(1, 2), 1, &lit, 0);
        t.fuzz(1, 0);
        assert_eq!(surface[1], 0xFF40_2010);
    }

    #[test]
    fn u16_scene_store_writes_565_block() {
        let lit = pal::<u16>();
        let expect = lit.block(0)[200];
        let mut surface = [0u16; 1];
        let mut t = PixelTarget::new(&mut surface, BufferSize::new(1, 1), 1, &lit, 0);
        t.store(0, 200);
        assert_eq!(surface[0], expect);
    }

    #[test]
    fn u16_fuzz_halves_565_channels() {
        let lit = pal::<u16>();
        let mut surface = [0xFFFFu16, 0];
        let mut t = PixelTarget::new(&mut surface, BufferSize::new(1, 2), 1, &lit, 0);
        t.fuzz(1, 0);
        assert_eq!(surface[1], 0x7BEF);
    }
}
