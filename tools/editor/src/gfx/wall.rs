//! Wall-elevation and flat rendering; no `pic-data` dependency.

use editor_core::Name8;
use slint::{Rgba8Pixel, SharedPixelBuffer};

use super::put_palette_color;
use crate::assets::EditorAssets;

const GAP_FILL: [u8; 4] = [0x20, 0x20, 0x20, 0x90];

/// Vertical slice of a wall elevation: a textured band or a see-through gap.
pub struct WallBand {
    /// `None` → gap fill.
    pub tex: Option<Name8>,
    pub height: f32,
    /// Two-sided masked midtexture; drawn once, not tiled vertically.
    pub masked: bool,
}

/// Render a wall elevation: bands top-to-bottom, textures tile per world unit; caller must [`EditorAssets::ensure_composed`] band textures first.
pub fn render_wall(
    assets: &EditorAssets,
    bands: &[WallBand],
    width_world: f32,
    px_per_unit: f32,
) -> Option<slint::Image> {
    let total: f32 = bands.iter().map(|b| b.height.max(0.0)).sum();
    let width = (width_world * px_per_unit).round().max(1.0) as u32;
    let height = (total * px_per_unit).round().max(1.0) as u32;
    if total <= 0.0 || width_world <= 0.0 {
        return None;
    }

    let mut buf = SharedPixelBuffer::<Rgba8Pixel>::new(width, height);
    let bytes = buf.make_mut_bytes();
    bytes.fill(0);

    let palette = *assets.palette();
    let mut band_top = 0.0f32;
    for band in bands {
        let band_px = band.height.max(0.0) * px_per_unit;
        let y0 = band_top.round() as u32;
        let y1 = ((band_top + band_px).round() as u32).min(height);
        band_top += band_px;

        let pic = band
            .tex
            .filter(|t| !t.is_empty())
            .and_then(|t| assets.composed(&t));
        for y in y0..y1 {
            for x in 0..width {
                let at = ((y * width + x) * 4) as usize;
                let world_x = x as f32 / px_per_unit;
                let world_y = (y as f32 - y0 as f32) / px_per_unit;
                let row = world_y as usize;
                match &pic {
                    Some(pic)
                        if pic.width > 0
                            && pic.height > 0
                            && !(band.masked && row >= pic.height) =>
                    {
                        let tx = (world_x as usize) % pic.width;
                        let ty = row % pic.height;
                        let index = pic.data[tx * pic.height + ty];
                        put_palette_color(&mut bytes[at..at + 4], &palette, index);
                    }
                    _ => bytes[at..at + 4].copy_from_slice(&GAP_FILL),
                }
            }
        }
    }
    Some(slint::Image::from_rgba8_premultiplied(buf))
}

/// Render a flat as a `side`-px square (nearest); missing flat → gap fill.
pub fn render_flat_square(assets: &EditorAssets, name: Name8, side: u32) -> slint::Image {
    let mut buf = SharedPixelBuffer::<Rgba8Pixel>::new(side, side);
    let bytes = buf.make_mut_bytes();
    let palette = *assets.palette();

    let pic = (!name.is_empty())
        .then(|| assets.iwad_flat_num(&name))
        .flatten()
        .map(|num| &assets.iwad_flats()[num].flat);
    for y in 0..side {
        for x in 0..side {
            let at = ((y * side + x) * 4) as usize;
            match pic {
                Some(pic) => {
                    let tx = (x as usize * pic.width) / side as usize;
                    let ty = (y as usize * pic.height) / side as usize;
                    let index = pic.data[ty * pic.width + tx];
                    put_palette_color(&mut bytes[at..at + 4], &palette, index);
                }
                None => bytes[at..at + 4].copy_from_slice(&GAP_FILL),
            }
        }
    }
    slint::Image::from_rgba8_premultiplied(buf)
}
