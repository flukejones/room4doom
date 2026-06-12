//! Texture-editor composite preview (Slint image). Patch codec + composer live in
//! [`crate::assets::texture_compose`]; this adds highlight stroke and light-map.

use editor_core::{ImportedPatch, TextureDef};
use slint::{Rgba8Pixel, SharedPixelBuffer};
use wad::WadData;
use wad::types::WadPalette;

use super::{MISSING_PATCH_INDEX, TRANSPARENT_INDEX, put_palette_color};
use crate::assets::{compose_texture_indices, patch_dims};

const PATCH_HIGHLIGHT: [u8; 4] = [0xff, 0xe0, 0x40, 0xff];

/// Composite a texture preview: reuses `compose_texture_indices` so preview matches
/// canvas. Missing patch → magenta; genuine transparency → clear. Optionally strokes
/// `highlight` patch bounds and applies a COLORMAP light level.
pub fn compose_texture_highlight(
    def: &TextureDef,
    imported: &[ImportedPatch],
    wad: &WadData,
    palette: &WadPalette,
    highlight: Option<usize>,
    colormap: Option<&[u8]>,
) -> SharedPixelBuffer<Rgba8Pixel> {
    let (w, h) = (def.width.max(1) as usize, def.height.max(1) as usize);
    let pic = compose_texture_indices(def, imported, wad);

    let mut buf = SharedPixelBuffer::<Rgba8Pixel>::new(w as u32, h as u32);
    let bytes = buf.make_mut_bytes();
    // pic.data is column-major (data[x*h + y]); buf is row-major.
    let real = |i: u16| i != TRANSPARENT_INDEX && i != MISSING_PATCH_INDEX;
    for y in 0..h {
        for x in 0..w {
            let index = pic.data[x * h + y];
            let shaded = match colormap {
                Some(cm) if real(index) => cm[index as usize & 0xff] as u16,
                _ => index,
            };
            let at = (y * w + x) * 4;
            put_palette_color(&mut bytes[at..at + 4], palette, shaded);
        }
    }

    if let Some(hi) = highlight
        && let Some(placement) = def.patches.get(hi)
        && let Some((pw, ph)) = patch_dims(imported, wad, placement.patch.as_str())
    {
        let x0 = placement.origin_x.clamp(0, w as i32);
        let y0 = placement.origin_y.clamp(0, h as i32);
        let x1 = (placement.origin_x + pw as i32).clamp(0, w as i32);
        let y1 = (placement.origin_y + ph as i32).clamp(0, h as i32);
        let plot = |bytes: &mut [u8], x: i32, y: i32| {
            if x >= 0 && (x as usize) < w && y >= 0 && (y as usize) < h {
                let at = (y as usize * w + x as usize) * 4;
                bytes[at..at + 4].copy_from_slice(&PATCH_HIGHLIGHT);
            }
        };
        for x in x0..x1 {
            plot(bytes, x, y0);
            plot(bytes, x, y1 - 1);
        }
        for y in y0..y1 {
            plot(bytes, x0, y);
            plot(bytes, x1 - 1, y);
        }
    }
    buf
}
