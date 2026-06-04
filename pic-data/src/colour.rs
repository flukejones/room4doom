//! Pixel-format abstraction and the palette-lit table.
//!
//! Folds palette ∘ gamma ∘ byte-order into [`PalLit<T>`] so the direct draw path
//! writes the final pixel directly (`block(tint)[lit_index]`), skipping the u8
//! index plane + `resolve()` pass. Only the palette stage is folded in, never the
//! per-pixel light tables: folding those (768/2048 blocks) would force multi-MB
//! rebuilds on every tint change. `PalLit` is one 14×256 table.

use wad::types::WadColour;
pub use wad::types::WadPalette;

/// Number of PLAYPAL palettes (normal + damage/bonus/radsuit tints).
pub const PALETTE_LEN: usize = 14;

/// Final-pixel byte order, chosen by the backend at table-build time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByteOrder {
    /// `0xAARRGGBB` — softbuffer, sdl2 ARGB8888, engine-native.
    Argb,
    /// `0xAABBGGRR` — the `pixels` crate (wgpu `Bgra8UnormSrgb`) on little-endian.
    Abgr,
}

/// Sealed pixel format. `u8` is the index format (widened at scanout, `PalLit`
/// unused); `u16`/`u32` carry the final pixel.
pub trait PixelFmt: sealed::Sealed + Copy + Default + 'static {
    /// Convert a `0xAARRGGBB` colour to this format in `order`.
    fn from_argb(argb: WadColour, order: ByteOrder) -> Self;

    /// Halve the RGB channels (fuzz darken), preserving alpha/unused bits.
    /// `u8` (index) returns itself — the index path darkens via colourmap 6.
    fn darken(self) -> Self;
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for u8 {}
    impl Sealed for u16 {}
    impl Sealed for u32 {}
}

impl PixelFmt for u8 {
    /// Never called for `u8` on the direct path (the index is stored directly);
    /// total impl returning the low byte.
    #[inline]
    fn from_argb(argb: WadColour, _order: ByteOrder) -> Self {
        argb as Self
    }

    #[inline]
    fn darken(self) -> Self {
        self
    }
}

impl PixelFmt for u16 {
    /// Pack into RGB565; `Abgr` swaps R↔B.
    #[inline]
    fn from_argb(argb: WadColour, order: ByteOrder) -> Self {
        let r = ((argb >> 16) & 0xFF) as Self;
        let g = ((argb >> 8) & 0xFF) as Self;
        let b = (argb & 0xFF) as Self;
        let (c0, c2) = match order {
            ByteOrder::Argb => (r, b),
            ByteOrder::Abgr => (b, r),
        };
        ((c0 >> 3) << 11) | ((g >> 2) << 5) | (c2 >> 3)
    }

    /// Halve each 565 channel: shift right 1, mask off the bits that bled across
    /// channel boundaries (R5/G6/B5 top bits).
    #[inline]
    fn darken(self) -> Self {
        (self >> 1) & 0x7BEF
    }
}

impl PixelFmt for u32 {
    /// `Argb` identity; `Abgr` swizzles R↔B, keeping A and G.
    #[inline]
    fn from_argb(argb: WadColour, order: ByteOrder) -> Self {
        match order {
            ByteOrder::Argb => argb,
            ByteOrder::Abgr => {
                let a = argb & 0xFF00_0000;
                let r = (argb >> 16) & 0xFF;
                let g = argb & 0x0000_FF00;
                let b = argb & 0xFF;
                a | (b << 16) | g | r
            }
        }
    }

    /// Halve RGB, keep alpha.
    #[inline]
    fn darken(self) -> Self {
        (self & 0xFF00_0000) | ((self >> 1) & 0x007F_7F7F)
    }
}

/// Palette ∘ gamma ∘ byte-order folded into one `PALETTE_LEN * 256` table.
/// Built from gamma-baked palettes; rebuild only on palette change, tint select
/// is a free block index.
#[derive(Debug, Clone)]
pub struct PalLit<T> {
    blocks: Box<[T]>,
    order: ByteOrder,
}

impl<T: PixelFmt> PalLit<T> {
    pub fn new(palettes: &[WadPalette; PALETTE_LEN], order: ByteOrder) -> Self {
        let mut s = Self {
            blocks: vec![T::default(); PALETTE_LEN * 256].into_boxed_slice(),
            order,
        };
        s.rebuild(palettes);
        s
    }

    /// Rebuild from the active palettes. Call on palette (gamma) change.
    pub fn rebuild(&mut self, palettes: &[WadPalette; PALETTE_LEN]) {
        for (tint, palette) in palettes.iter().enumerate() {
            let base = tint * 256;
            for (i, &colour) in palette.0.iter().enumerate() {
                self.blocks[base + i] = T::from_argb(colour, self.order);
            }
        }
    }

    /// Block for tint `tint`, indexed by lit palette index.
    #[inline(always)]
    pub fn block(&self, tint: usize) -> &[T; 256] {
        let base = tint * 256;
        // SAFETY: blocks is PALETTE_LEN*256; tint < PALETTE_LEN (clamped at the
        // palette-selection site).
        unsafe { &*(self.blocks.as_ptr().add(base).cast::<[T; 256]>()) }
    }

    #[inline(always)]
    pub const fn order(&self) -> ByteOrder {
        self.order
    }
}

/// A [`PalLit`] paired with the palette generation it was built from. Rebuilds
/// the table only when the generation changes (gamma/palette change); otherwise
/// returns the cached one. Owned by the renderer, refreshed each frame via
/// [`Self::get`].
#[derive(Debug, Clone, Default)]
pub struct PalLitCache<T> {
    /// `None` until first built.
    entry: Option<(u64, PalLit<T>)>,
}

impl<T: PixelFmt> PalLitCache<T> {
    pub const fn new() -> Self {
        Self {
            entry: None,
        }
    }

    /// Return the table for `generation`, rebuilding from `palettes` (in byte
    /// `order`) only if the generation changed since the last call.
    pub fn get(
        &mut self,
        generation: u64,
        palettes: &[WadPalette; PALETTE_LEN],
        order: ByteOrder,
    ) -> &PalLit<T> {
        match &mut self.entry {
            Some((g, table)) if *g != generation => {
                table.rebuild(palettes);
                *g = generation;
            }
            Some(_) => {}
            None => self.entry = Some((generation, PalLit::new(palettes, order))),
        }
        &self.entry.as_ref().expect("entry built above").1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wad::types::WadPalette;

    /// 14 deterministic palettes, distinct colours per (tint, index).
    fn fake_palettes() -> [WadPalette; PALETTE_LEN] {
        let mut pals: [WadPalette; PALETTE_LEN] = Default::default();
        for (tint, pal) in pals.iter_mut().enumerate() {
            for (i, c) in pal.0.iter_mut().enumerate() {
                let r = (i as u32) & 0xFF;
                let g = ((i as u32 * 2) ^ (tint as u32 * 17)) & 0xFF;
                let b = ((i as u32 * 3) + (tint as u32 * 5)) & 0xFF;
                *c = 0xFF00_0000 | (r << 16) | (g << 8) | b;
            }
        }
        pals
    }

    #[test]
    fn u32_argb_is_identity() {
        let pals = fake_palettes();
        let lit = PalLit::<u32>::new(&pals, ByteOrder::Argb);
        for (tint, pal) in pals.iter().enumerate() {
            let block = lit.block(tint);
            for (i, &src) in pal.0.iter().enumerate() {
                assert_eq!(
                    block[i], src,
                    "tint {tint} idx {i}: u32 ARGB must equal the source palette colour"
                );
            }
        }
    }

    #[test]
    fn u32_abgr_swaps_r_and_b_keeps_a_g() {
        let pals = fake_palettes();
        let lit = PalLit::<u32>::new(&pals, ByteOrder::Abgr);
        for (tint, pal) in pals.iter().enumerate() {
            let block = lit.block(tint);
            for (i, &src) in pal.0.iter().enumerate() {
                let got = block[i];
                assert_eq!(got & 0xFF00_0000, src & 0xFF00_0000, "alpha preserved");
                assert_eq!(got & 0x0000_FF00, src & 0x0000_FF00, "green preserved");
                assert_eq!((got >> 16) & 0xFF, src & 0xFF, "got.R == src.B");
                assert_eq!(got & 0xFF, (src >> 16) & 0xFF, "got.B == src.R");
            }
        }
    }

    #[test]
    fn u16_equals_565_quantized_source() {
        let pals = fake_palettes();
        let lit = PalLit::<u16>::new(&pals, ByteOrder::Argb);
        for (tint, pal) in pals.iter().enumerate() {
            let block = lit.block(tint);
            for (i, &src) in pal.0.iter().enumerate() {
                let r = ((src >> 16) & 0xFF) as u16;
                let g = ((src >> 8) & 0xFF) as u16;
                let b = (src & 0xFF) as u16;
                let expect = ((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3);
                assert_eq!(
                    block[i], expect,
                    "tint {tint} idx {i}: u16 must be 565-quantized source"
                );
            }
        }
    }

    #[test]
    fn u16_abgr_swaps_r_and_b() {
        let pals = fake_palettes();
        let lit = PalLit::<u16>::new(&pals, ByteOrder::Abgr);
        let src = pals[0].0[200];
        let r = ((src >> 16) & 0xFF) as u16;
        let g = ((src >> 8) & 0xFF) as u16;
        let b = (src & 0xFF) as u16;
        let expect = ((b >> 3) << 11) | ((g >> 2) << 5) | (r >> 3);
        assert_eq!(lit.block(0)[200], expect);
    }

    #[test]
    fn rebuild_reflects_new_palettes() {
        let pals = fake_palettes();
        let mut lit = PalLit::<u32>::new(&pals, ByteOrder::Argb);
        let mut pals2 = pals;
        pals2[3].0[42] = 0xFF12_3456;
        lit.rebuild(&pals2);
        assert_eq!(lit.block(3)[42], 0xFF12_3456);
        // Untouched entries still match.
        assert_eq!(lit.block(0)[0], pals[0].0[0]);
    }
}
