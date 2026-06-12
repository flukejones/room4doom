//! Palette-indexed picture → RGBA8 conversion for thumbnails.

use slint::{Rgba8Pixel, SharedPixelBuffer};
use wad::types::WadPalette;

use super::put_palette_color;
use crate::assets::FlatPic;

/// Row-major 64×64 flat to RGBA8.
pub fn flat_to_rgba(pic: &FlatPic, palette: &WadPalette) -> SharedPixelBuffer<Rgba8Pixel> {
    let mut buf = SharedPixelBuffer::<Rgba8Pixel>::new(pic.width as u32, pic.height as u32);
    let bytes = buf.make_mut_bytes();
    for (i, &index) in pic.data.iter().enumerate() {
        put_palette_color(&mut bytes[i * 4..i * 4 + 4], palette, index);
    }
    buf
}
