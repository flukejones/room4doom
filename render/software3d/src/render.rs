use gameplay::{FlatPic, PicData, SurfaceKind, WallPic};
use glam::Vec2;

use crate::{Software3D, sky};

/// Sample a single sky pixel from the combined RGBA buffer, returning the
/// colour or `None` for transparent (alpha = 0).
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
    sky_combined: &[[u8; 4]],
) -> Option<[u8; 4]> {
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
    if c[3] == 0 { None } else { Some(c) }
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
    ) -> &'a [u8; 4] {
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
                        return &[0, 0, 0, 0];
                    }
                    let lit_color_index = *colourmap.get_unchecked(color_index);
                    pic_data.palette().get_unchecked(lit_color_index)
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
                    pic_data.palette().get_unchecked(lit_color_index)
                }
                TextureSampler::Sky => &[32, 32, 32, 255],
                TextureSampler::Untextured => &[32, 32, 32, 255],
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct InterpolationState {
    current_tex: Vec2,
    current_inv_w: f32,
    tex_dx: Vec2,
    inv_w_dx: f32,
    inv_w_min: f32,
    inv_w_max: f32,
}

impl InterpolationState {
    #[inline(always)]
    pub(crate) fn get_current_uv(&self) -> (f32, f32) {
        // Clamp inv_w to the polygon's vertex range to prevent barycentric
        // extrapolation from producing incorrect depth values at screen edges
        let clamped_inv_w = self.current_inv_w.clamp(self.inv_w_min, self.inv_w_max);
        if clamped_inv_w > 0.0 {
            let w = 1.0 / clamped_inv_w;
            let corrected_tex = self.current_tex * w;
            (corrected_tex.x, corrected_tex.y)
        } else {
            (self.current_tex.x, self.current_tex.y)
        }
    }

    #[inline(always)]
    pub(crate) fn step_x(&mut self) {
        self.current_tex += self.tex_dx;
        self.current_inv_w += self.inv_w_dx;
    }
}

/// Pre-computed triangle interpolation data for efficient per-pixel texture
/// coordinate calculation
#[derive(Debug, Clone)]
pub(crate) struct TriangleInterpolator {
    v0: Vec2,
    v1: Vec2,
    v2: Vec2,
    tex0: Vec2,
    tex1: Vec2,
    tex2: Vec2,
    inv_w0: f32,
    inv_w1: f32,
    inv_w2: f32,
    denom: f32,
    da_dx: f32,
    db_dx: f32,
    /// Min/max inv_w across all polygon vertices, used to clamp extrapolated
    /// depth
    inv_w_min: f32,
    inv_w_max: f32,
}

impl TriangleInterpolator {
    #[inline(always)]
    pub(crate) fn new(screen_verts: &[Vec2], tex_coords: &[Vec2], inv_w: &[f32]) -> Option<Self> {
        // Compute min/max inv_w across all polygon vertices to clamp extrapolation
        let mut inv_w_min = f32::INFINITY;
        let mut inv_w_max = f32::NEG_INFINITY;
        for &w in inv_w.iter() {
            if w < inv_w_min {
                inv_w_min = w;
            }
            if w > inv_w_max {
                inv_w_max = w;
            }
        }

        // Fast path for triangles - no need to search for best triangle
        if screen_verts.len() == 3 {
            let v0 = screen_verts[0];
            let v1 = screen_verts[1];
            let v2 = screen_verts[2];

            let denom = (v1.y - v2.y) * (v0.x - v2.x) + (v2.x - v1.x) * (v0.y - v2.y);
            if denom.abs() < f32::EPSILON {
                return None;
            }
            let da_dx = (v1.y - v2.y) / denom;
            let db_dx = (v2.y - v0.y) / denom;

            return Some(TriangleInterpolator {
                v0,
                v1,
                v2,
                tex0: tex_coords[0],
                tex1: tex_coords[1],
                tex2: tex_coords[2],
                inv_w0: inv_w[0],
                inv_w1: inv_w[1],
                inv_w2: inv_w[2],
                denom,
                da_dx,
                db_dx,
                inv_w_min,
                inv_w_max,
            });
        }

        // For polygons with more than 3 vertices, find the best triangle
        let mut best_triangle = None;
        let mut best_area = 0.0;
        let mut best_denom = 0.0;

        for i in 1..screen_verts.len() - 1 {
            let v0 = screen_verts[0];
            let v1 = screen_verts[i];
            let v2 = screen_verts[i + 1];

            let denom = (v1.y - v2.y) * (v0.x - v2.x) + (v2.x - v1.x) * (v0.y - v2.y);
            if denom.abs() < f32::EPSILON {
                continue;
            }

            let area = denom.abs();
            if area > best_area {
                best_area = area;
                best_triangle = Some((0, i, i + 1));
                best_denom = denom;
            }
        }

        let (i0, i1, i2) = best_triangle?;
        let v0 = screen_verts[i0];
        let v1 = screen_verts[i1];
        let v2 = screen_verts[i2];

        let denom = best_denom;

        // Pre-compute barycentric derivatives
        let da_dx = (v1.y - v2.y) / denom;
        let db_dx = (v2.y - v0.y) / denom;

        Some(TriangleInterpolator {
            v0,
            v1,
            v2,
            tex0: tex_coords[i0],
            tex1: tex_coords[i1],
            tex2: tex_coords[i2],
            inv_w0: inv_w[i0],
            inv_w1: inv_w[i1],
            inv_w2: inv_w[i2],
            denom,
            da_dx,
            db_dx,
            inv_w_min,
            inv_w_max,
        })
    }

    /// Initialize interpolation state for a scanline
    #[inline(always)]
    pub(crate) fn init_scanline(&self, start_x: f32, y: f32) -> InterpolationState {
        let p = Vec2::new(start_x, y);

        // Calculate initial barycentric coordinates
        let a = ((self.v1.y - self.v2.y) * (p.x - self.v2.x)
            + (self.v2.x - self.v1.x) * (p.y - self.v2.y))
            / self.denom;
        let b = ((self.v2.y - self.v0.y) * (p.x - self.v2.x)
            + (self.v0.x - self.v2.x) * (p.y - self.v2.y))
            / self.denom;
        let c = 1.0 - a - b;

        // Calculate initial interpolated values
        let interp_tex = self.tex0 * a + self.tex1 * b + self.tex2 * c;
        let interp_inv_w = self.inv_w0 * a + self.inv_w1 * b + self.inv_w2 * c;

        // Calculate per-pixel increments for X direction
        let tex_dx = self.tex0 * self.da_dx
            + self.tex1 * self.db_dx
            + self.tex2 * (-self.da_dx - self.db_dx);
        let inv_w_dx = self.inv_w0 * self.da_dx
            + self.inv_w1 * self.db_dx
            + self.inv_w2 * (-self.da_dx - self.db_dx);

        InterpolationState {
            current_tex: interp_tex,
            current_inv_w: interp_inv_w,
            tex_dx,
            inv_w_dx,
            inv_w_min: self.inv_w_min,
            inv_w_max: self.inv_w_max,
        }
    }
}

impl Software3D {
    pub(super) fn generate_pseudo_random_colour(id: u32, brightness: usize) -> [u8; 4] {
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

        [r, g, b, 255]
    }
}
