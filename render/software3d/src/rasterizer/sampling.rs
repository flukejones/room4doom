use level::SurfaceKind;
use pic_data::{FlatPic, PicData, WallPic};

use crate::Software3D;
use crate::scene::sky;

/// Sample a single sky pixel from the combined u32 XRGB buffer, returning the
/// colour or `None` for transparent (value = 0).
///
/// Buffer layout (column-major):
///   `0..sky_tex_height`                                — original texture
///   `sky_tex_height..+sky_extended_rows`               — upward extension
///   `sky_tex_height+sky_extended_rows..+sky_down_rows` — downward extension
///
/// `sky_r` mapping:
///   `0 < sky_r < sky_tex_height`  → original row `sky_r`
///   `sky_r <= 0`                  → upward extension row `(-sky_r)`
///   `sky_r >= sky_tex_height`     → downward extension row `(sky_r -
/// sky_tex_height)`
#[inline]
pub(crate) fn sample_sky_pixel(
    sky_col: usize,
    sky_r: i32,
    sky_tex_height: usize,
    sky_combined: &[u32],
) -> Option<u32> {
    const UP: usize = sky::SKY_EXTEND_ROWS;
    const DN: usize = sky::SKY_DOWN_ROWS;
    let total = sky_tex_height + UP + DN;
    let row = if sky_r > 0 && (sky_r as usize) < sky_tex_height {
        sky_r as usize
    } else if sky_r <= 0 {
        sky_tex_height + ((-sky_r) as usize).min(UP - 1)
    } else if sky_r >= sky_tex_height as i32 {
        sky_tex_height + UP + (sky_r as usize - sky_tex_height).min(DN - 1)
    } else if sky_tex_height > 0 {
        sky_tex_height - 1
    } else {
        return None;
    };
    let c = sky_combined[sky_col * total + row];
    if c == 0 { None } else { Some(c) }
}

// TODO: completely change the Texture format to all be one
/// Pre-computed texture sampling strategy to eliminate per-pixel match
/// statements
pub(crate) enum TextureSampler<'a> {
    Vertical {
        texture: &'a WallPic,
        width: f32,
        height: f32,
        width_mask: usize,
        height_mask: usize,
    },
    Horizontal {
        texture: &'a FlatPic,
        width: f32,
        height: f32,
    },
    Sky,
    Untextured,
}

impl<'a> TextureSampler<'a> {
    #[inline(always)]
    pub(crate) fn new(
        surface_kind: &SurfaceKind,
        pic_data: &'a PicData,
        sky_pic: usize,
        sky_num: usize,
    ) -> Self {
        match surface_kind {
            SurfaceKind::Vertical {
                texture: Some(tex_id),
                ..
            } => {
                if *tex_id == sky_pic {
                    TextureSampler::Sky
                } else {
                    let texture = pic_data.wall_pic(*tex_id);
                    let width_f32 = texture.width as f32;
                    let height_f32 = texture.height as f32;
                    TextureSampler::Vertical {
                        texture,
                        width: width_f32,
                        height: height_f32,
                        width_mask: texture.width,
                        height_mask: texture.height,
                    }
                }
            }
            SurfaceKind::Horizontal {
                texture,
                ..
            } => {
                if *texture == sky_num {
                    TextureSampler::Sky
                } else {
                    let texture = pic_data.get_flat(*texture);
                    TextureSampler::Horizontal {
                        texture,
                        width: texture.width as f32,
                        height: texture.height as f32,
                    }
                }
            }
            SurfaceKind::Vertical {
                texture: None,
                ..
            } => TextureSampler::Untextured,
        }
    }

    #[inline(always)]
    pub(crate) fn sample(
        &'a self,
        u: f32,
        v: f32,
        colourmap: &[usize],
        pic_data: &'a PicData,
    ) -> u32 {
        unsafe {
            match self {
                TextureSampler::Vertical {
                    texture,
                    width,
                    height,
                    width_mask,
                    height_mask,
                } => {
                    let u_wrapped = u - u.floor();
                    let v_wrapped = v - v.floor();
                    let tex_x = (u_wrapped * width) as u32 as usize % (*width_mask);
                    let tex_y = (v_wrapped * height) as u32 as usize % (*height_mask);

                    let color_index = *texture.data.get_unchecked(tex_x * texture.height + tex_y);
                    if color_index == usize::MAX {
                        return 0;
                    }
                    let lit_color_index = *colourmap.get_unchecked(color_index);
                    *pic_data.palette().get_unchecked(lit_color_index)
                }
                TextureSampler::Horizontal {
                    texture,
                    width,
                    height,
                } => {
                    let tex_x = ((u.abs() * width) as usize) & 63;
                    let tex_y = ((v.abs() * height) as usize) & 63;
                    let color_index = *texture.data.get_unchecked(tex_x * 64 + tex_y);
                    let lit_color_index = *colourmap.get_unchecked(color_index);
                    *pic_data.palette().get_unchecked(lit_color_index)
                }
                TextureSampler::Sky => 0xFF202020,
                TextureSampler::Untextured => 0xFF202020,
            }
        }
    }
}

impl Software3D {
    pub(crate) fn generate_pseudo_random_colour(id: u32, brightness: usize) -> u32 {
        let mut hash = id.wrapping_mul(0x9E3779B9);
        hash ^= hash >> 15;
        hash = hash.wrapping_mul(0x85EBCA6B);
        hash ^= hash >> 13;

        let hue = (hash % 360) as f32;
        let val = brightness as f32 / 255.0;

        // HSV to RGB (saturation = 1.0)
        let c = val;
        let x = c * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
        let m = val - c;

        let (r1, g1, b1) = match hue as u32 {
            0..=59 => (c, x, 0.0),
            60..=119 => (x, c, 0.0),
            120..=179 => (0.0, c, x),
            180..=239 => (0.0, x, c),
            240..=299 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };

        let r = ((r1 + m) * 255.0).round().min(255.0) as u8;
        let g = ((g1 + m) * 255.0).round().min(255.0) as u8;
        let b = ((b1 + m) * 255.0).round().min(255.0) as u8;

        0xFF_00_00_00 | (r as u32) << 16 | (g as u32) << 8 | b as u32
    }
}
