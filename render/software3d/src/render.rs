#[cfg(feature = "hprof")]
use coarse_prof::profile;

use gameplay::{FlatPic, PicData, SurfaceKind, SurfacePolygon, WallPic, WallType};
use glam::Vec2;
use render_trait::{DrawBuffer, SOFT_PIXEL_CHANNELS};

use crate::depth_buffer::SKY_DEPTH;
use crate::{DebugColourMode, Software3D, sky};

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

/// Minimum depth for real geometry. Must exceed `SKY_DEPTH` (f32::EPSILON)
/// so that distant polygons clamped to this value still pass the depth test
/// against sky pixels.
const MIN_GEOMETRY_DEPTH: f32 = 1.0e-6;

const LIGHT_MIN_Z: f32 = 0.001;
const LIGHT_MAX_Z: f32 = 0.055;
const LIGHT_RANGE: f32 = 1.0 / (LIGHT_MAX_Z - LIGHT_MIN_Z);
const LIGHT_SCALE: f32 = LIGHT_RANGE * 8.0 * 16.0;

/// Represents a 2D polygon in screen space
#[derive(Debug, Clone)]
pub struct ScreenPoly<'a>(pub &'a [Vec2]);

impl<'a> ScreenPoly<'a> {
    /// Get axis-aligned bounding box of polygon
    #[inline(always)]
    pub fn bounds(&self) -> Option<(Vec2, Vec2)> {
        if self.0.is_empty() {
            return None;
        }

        let mut min = self.0[0];
        let mut max = self.0[0];

        for vertex in &self.0[1..] {
            min.x = min.x.min(vertex.x);
            min.y = min.y.min(vertex.y);
            max.x = max.x.max(vertex.x);
            max.y = max.y.max(vertex.y);
        }

        Some((min, max))
    }
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

/// Write a pixel, alpha-blending against the existing buffer if alpha is set.
#[inline(always)]
pub(crate) fn write_pixel(
    buffer: &mut impl DrawBuffer,
    x: usize,
    y: usize,
    color: &[u8; 4],
    alpha: Option<u8>,
) {
    if let Some(a) = alpha {
        let dst = buffer.read_pixel(x, y);
        let a = a as u16;
        let inv_a = 255 - a;
        let blended = [
            ((color[0] as u16 * a + dst[0] as u16 * inv_a) >> 8) as u8,
            ((color[1] as u16 * a + dst[1] as u16 * inv_a) >> 8) as u8,
            ((color[2] as u16 * a + dst[2] as u16 * inv_a) >> 8) as u8,
            255,
        ];
        buffer.set_pixel(x, y, &blended);
    } else {
        buffer.set_pixel(x, y, color);
    }
}

impl Software3D {
    /// Fast-path rasteriser: zero debug branches in the inner loop.
    /// Used when no debug draw options are active.
    #[inline(always)]
    pub(super) fn draw_polygon(
        &mut self,
        polygon: &SurfacePolygon,
        brightness: usize,
        bounds: (Vec2, Vec2),
        pic_data: &mut PicData,
        buffer: &mut impl DrawBuffer,
    ) {
        #[cfg(feature = "hprof")]
        profile!("draw_polygon");

        let screen_poly = ScreenPoly(&self.screen_vertices_buffer[..self.screen_vertices_len]);

        let interpolator = match TriangleInterpolator::new(
            &screen_poly.0,
            &self.tex_coords_buffer[..self.tex_coords_len],
            &self.inv_w_buffer[..self.inv_w_len],
        ) {
            Some(interpolator) => interpolator,
            None => {
                self.stats.polygons_early_culled += 1;
                return;
            }
        };

        // Cache frequently used values
        let sky_pic = pic_data.sky_pic();
        let sky_num = pic_data.sky_num();
        let texture_sampler =
            TextureSampler::new(&polygon.surface_kind, pic_data, sky_pic, sky_num);
        let is_masked = matches!(
            &polygon.surface_kind,
            SurfaceKind::Vertical {
                two_sided: true,
                wall_type: WallType::Middle,
                ..
            }
        );
        let is_sky = matches!(texture_sampler, TextureSampler::Sky);
        let vertices = &screen_poly.0;
        let vertex_count = screen_poly.0.len();
        let width_f32 = self.width as f32;
        let height_f32 = self.height as f32;

        // Pre-compute bounds
        let y_start = bounds.0.y.max(0.0).ceil() as u32 as usize;
        let y_end = bounds.1.y.min(height_f32 - 1.0).floor() as u32 as usize;

        let inv_w_slice = &self.inv_w_buffer[..self.inv_w_len];
        let buf_pitch = buffer.pitch();
        let buf = buffer.buf_mut();
        let mut did_draw = false;
        for y in y_start..=y_end {
            let y_f = y as f32;
            let mut x0 = f32::INFINITY;
            let mut x1 = f32::NEG_INFINITY;
            let mut inv_w_at_x0 = 0.0f32;
            let mut inv_w_at_x1 = 0.0f32;
            let mut found = 0;

            // Walk all edges of the polygon, interpolating both x and inv_w at each edge
            for ei in 0..vertex_count {
                let ni = (ei + 1) % vertex_count;
                let start = unsafe { *vertices.get_unchecked(ei) };
                let end = unsafe { *vertices.get_unchecked(ni) };
                let dy = end.y - start.y;
                if dy.abs() < f32::EPSILON {
                    continue;
                }
                // Top-left fill rule: include top edge (min y), exclude bottom edge (max y)
                let (min_y, max_y) = if start.y < end.y {
                    (start.y, end.y)
                } else {
                    (end.y, start.y)
                };
                if y_f >= min_y && y_f < max_y {
                    let t = (y_f - start.y) / dy;
                    let x = start.x + (end.x - start.x) * t;
                    let iw_start = unsafe { *inv_w_slice.get_unchecked(ei) };
                    let iw_end = unsafe { *inv_w_slice.get_unchecked(ni) };
                    let iw = iw_start + (iw_end - iw_start) * t;
                    if found == 0 {
                        x0 = x;
                        inv_w_at_x0 = iw;
                        found += 1;
                    } else {
                        x1 = x;
                        inv_w_at_x1 = iw;
                        found += 1;
                        break;
                    }
                }
            }

            if found < 2 {
                continue;
            }
            if x0 > x1 {
                std::mem::swap(&mut x0, &mut x1);
                std::mem::swap(&mut inv_w_at_x0, &mut inv_w_at_x1);
            }

            let x_f = x0.max(0.0).ceil();
            let x_start = x_f as u32 as usize;
            let x_end = x1.min(width_f32 - 1.0).floor() as u32 as usize;

            // Compute per-pixel depth from edge-interpolated inv_w (consistent across
            // adjacent polygon triangles, unlike barycentric extrapolation)
            let span_width = x1 - x0;
            let (mut edge_inv_w, edge_inv_w_dx) = if span_width > f32::EPSILON {
                let dx = 1.0 / span_width;
                let inv_w_dx = (inv_w_at_x1 - inv_w_at_x0) * dx;
                let start_inv_w = inv_w_at_x0 + (x_f - x0) * inv_w_dx;
                (start_inv_w, inv_w_dx)
            } else {
                (inv_w_at_x0, 0.0)
            };

            // Clamp inv_w to a small positive floor — edge interpolation can
            // drift slightly negative at polygon boundaries due to FP rounding,
            // which would cause the skip loop to eat visible edge pixels.
            if edge_inv_w < MIN_GEOMETRY_DEPTH {
                edge_inv_w = MIN_GEOMETRY_DEPTH;
            }

            if is_sky {
                // Sky polygon: depth-only pass. Write SKY_DEPTH to mark pixels
                // for the full-screen sky fill pass. No pixel drawing here —
                // sky is rendered once after all geometry.
                let mut x = x_start;
                while x <= x_end {
                    self.depth_buffer.set_sky_depth_unchecked(x, y);
                    x += 1;
                }
                did_draw = true;
            } else {
                let mut interp_state = interpolator.init_scanline(x_f, y_f);
                let mut x = x_start;
                while x <= x_end {
                    // Skip occluded pixels quickly using a read-only depth peek
                    while x <= x_end {
                        // Clamp per-pixel: edge interpolation can drift negative
                        // on thin scanlines with large inv_w_dx
                        let test_inv_w = edge_inv_w.max(MIN_GEOMETRY_DEPTH);
                        let peek = self.depth_buffer.peek_depth_unchecked(x, y);
                        if test_inv_w > peek && peek != SKY_DEPTH {
                            break;
                        }
                        interp_state.step_x();
                        edge_inv_w += edge_inv_w_dx;
                        x += 1;
                    }
                    if x > x_end {
                        break;
                    }

                    // Paint visible span starting at x
                    while x <= x_end {
                        #[cfg(feature = "hprof")]
                        profile!("draw_textured_polygon X loop");

                        if is_masked {
                            // Depth test before UV — avoids the perspective divide on misses
                            if edge_inv_w <= self.depth_buffer.peek_depth_unchecked(x, y) {
                                interp_state.step_x();
                                edge_inv_w += edge_inv_w_dx;
                                x += 1;
                                break;
                            }
                            let (u, v) = interp_state.get_current_uv();
                            // Outside texture vertical bounds — no tiling for middle walls
                            if v < 0.0 || v >= 1.0 {
                                interp_state.step_x();
                                edge_inv_w += edge_inv_w_dx;
                                x += 1;
                                continue;
                            }
                            let colourmap =
                                pic_data.base_colourmap(brightness, edge_inv_w * LIGHT_SCALE);
                            let color = texture_sampler.sample(u, v, colourmap, pic_data);
                            if color[3] == 0 {
                                // Transparent pixel — don't write depth or color
                                interp_state.step_x();
                                edge_inv_w += edge_inv_w_dx;
                                x += 1;
                                continue;
                            }
                            self.depth_buffer.set_depth_unchecked(x, y, edge_inv_w);
                            let px = y * buf_pitch + x * SOFT_PIXEL_CHANNELS;
                            buf[px] = color[0];
                            buf[px + 1] = color[1];
                            buf[px + 2] = color[2];
                            buf[px + 3] = color[3];
                        } else {
                            // Depth test before UV — avoids the perspective divide on misses
                            if !self
                                .depth_buffer
                                .test_and_set_depth_unchecked(x, y, edge_inv_w)
                            {
                                // current pixel is occluded; break to resume skipping phase
                                interp_state.step_x();
                                edge_inv_w += edge_inv_w_dx;
                                x += 1;
                                break;
                            }

                            let (u, v) = interp_state.get_current_uv();
                            let colourmap =
                                pic_data.base_colourmap(brightness, edge_inv_w * LIGHT_SCALE);
                            let color = texture_sampler.sample(u, v, colourmap, pic_data);

                            let px = y * buf_pitch + x * SOFT_PIXEL_CHANNELS;
                            buf[px] = color[0];
                            buf[px + 1] = color[1];
                            buf[px + 2] = color[2];
                            buf[px + 3] = color[3];
                        }
                        did_draw = true;

                        interp_state.step_x();
                        edge_inv_w += edge_inv_w_dx;
                        x += 1;
                    }
                }
            }
        }

        if did_draw {
            self.stats.polygons_rendered += 1;
        } else {
            self.stats.polygons_no_draw += 1;
        }
    }

    /// Debug-path rasteriser: supports alpha blending, depth disable,
    /// debug colour modes (sector_id, depth, normals, overdraw), and wireframe.
    /// Only called when `DebugDrawOptions::is_active()` is true.
    pub(super) fn draw_polygon_debug(
        &mut self,
        polygon: &SurfacePolygon,
        brightness: usize,
        bounds: (Vec2, Vec2),
        pic_data: &mut PicData,
        buffer: &mut impl DrawBuffer,
    ) {
        let screen_poly = ScreenPoly(&self.screen_vertices_buffer[..self.screen_vertices_len]);

        let interpolator = match TriangleInterpolator::new(
            &screen_poly.0,
            &self.tex_coords_buffer[..self.tex_coords_len],
            &self.inv_w_buffer[..self.inv_w_len],
        ) {
            Some(interpolator) => interpolator,
            None => {
                self.stats.polygons_early_culled += 1;
                return;
            }
        };

        let sky_pic = pic_data.sky_pic();
        let sky_num = pic_data.sky_num();
        let texture_sampler =
            TextureSampler::new(&polygon.surface_kind, pic_data, sky_pic, sky_num);
        let is_masked = matches!(
            &polygon.surface_kind,
            SurfaceKind::Vertical {
                two_sided: true,
                wall_type: WallType::Middle,
                ..
            }
        );
        let is_sky = matches!(texture_sampler, TextureSampler::Sky);
        let vertices = &screen_poly.0;
        let vertex_count = screen_poly.0.len();
        let width_f32 = self.width as f32;
        let height_f32 = self.height as f32;

        let y_start = bounds.0.y.max(0.0).ceil() as u32 as usize;
        let y_end = bounds.1.y.min(height_f32 - 1.0).floor() as u32 as usize;

        let inv_w_slice = &self.inv_w_buffer[..self.inv_w_len];
        let alpha = self.debug.options.alpha;
        let no_depth = self.debug.options.no_depth;
        let colour_mode = &self.debug.options.colour_mode;

        // Pre-compute debug colour for the whole polygon if using a flat mode
        let debug_flat_colour = match colour_mode {
            DebugColourMode::SectorId => Some(Self::generate_pseudo_random_colour(
                polygon.sector_id as u32,
                132,
            )),
            _ => None,
        };

        let mut did_draw = false;
        for y in y_start..=y_end {
            let y_f = y as f32;
            let mut x0 = f32::INFINITY;
            let mut x1 = f32::NEG_INFINITY;
            let mut inv_w_at_x0 = 0.0f32;
            let mut inv_w_at_x1 = 0.0f32;
            let mut found = 0;

            for ei in 0..vertex_count {
                let ni = (ei + 1) % vertex_count;
                let start = unsafe { *vertices.get_unchecked(ei) };
                let end = unsafe { *vertices.get_unchecked(ni) };
                let dy = end.y - start.y;
                if dy.abs() < f32::EPSILON {
                    continue;
                }
                let (min_y, max_y) = if start.y < end.y {
                    (start.y, end.y)
                } else {
                    (end.y, start.y)
                };
                if y_f >= min_y && y_f < max_y {
                    let t = (y_f - start.y) / dy;
                    let x = start.x + (end.x - start.x) * t;
                    let iw_start = unsafe { *inv_w_slice.get_unchecked(ei) };
                    let iw_end = unsafe { *inv_w_slice.get_unchecked(ni) };
                    let iw = iw_start + (iw_end - iw_start) * t;
                    if found == 0 {
                        x0 = x;
                        inv_w_at_x0 = iw;
                        found += 1;
                    } else {
                        x1 = x;
                        inv_w_at_x1 = iw;
                        found += 1;
                        break;
                    }
                }
            }

            if found < 2 {
                continue;
            }
            if x0 > x1 {
                std::mem::swap(&mut x0, &mut x1);
                std::mem::swap(&mut inv_w_at_x0, &mut inv_w_at_x1);
            }

            let x_f = x0.max(0.0).ceil();
            let x_start = x_f as u32 as usize;
            let x_end = x1.min(width_f32 - 1.0).floor() as u32 as usize;

            let span_width = x1 - x0;
            let (mut edge_inv_w, edge_inv_w_dx) = if span_width > f32::EPSILON {
                let dx = 1.0 / span_width;
                let inv_w_dx = (inv_w_at_x1 - inv_w_at_x0) * dx;
                let start_inv_w = inv_w_at_x0 + (x_f - x0) * inv_w_dx;
                (start_inv_w, inv_w_dx)
            } else {
                (inv_w_at_x0, 0.0)
            };

            if edge_inv_w < MIN_GEOMETRY_DEPTH {
                edge_inv_w = MIN_GEOMETRY_DEPTH;
            }

            if is_sky {
                // Sky polygon: depth-only pass. Write SKY_DEPTH to mark pixels
                // for the full-screen sky fill pass.
                let mut x = x_start;
                while x <= x_end {
                    self.depth_buffer.set_sky_depth_unchecked(x, y);
                    x += 1;
                }
                did_draw = true;
            } else {
                let mut interp_state = interpolator.init_scanline(x_f, y_f);
                let mut x = x_start;
                while x <= x_end {
                    // Skip occluded pixels (unless depth is disabled)
                    if !no_depth {
                        while x <= x_end {
                            let test_inv_w = edge_inv_w.max(MIN_GEOMETRY_DEPTH);
                            if test_inv_w > self.depth_buffer.peek_depth_unchecked(x, y) {
                                break;
                            }
                            interp_state.step_x();
                            edge_inv_w += edge_inv_w_dx;
                            x += 1;
                        }
                        if x > x_end {
                            break;
                        }
                    }

                    while x <= x_end {
                        let (u, v) = interp_state.get_current_uv();
                        if edge_inv_w <= 0.0 {
                            interp_state.step_x();
                            edge_inv_w += edge_inv_w_dx;
                            x += 1;
                            continue;
                        }

                        if is_masked {
                            if !no_depth
                                && edge_inv_w <= self.depth_buffer.peek_depth_unchecked(x, y)
                            {
                                interp_state.step_x();
                                edge_inv_w += edge_inv_w_dx;
                                x += 1;
                                break;
                            }
                            if v < 0.0 || v >= 1.0 {
                                interp_state.step_x();
                                edge_inv_w += edge_inv_w_dx;
                                x += 1;
                                continue;
                            }
                            let colourmap =
                                pic_data.base_colourmap(brightness, edge_inv_w * LIGHT_SCALE);
                            let color = texture_sampler.sample(u, v, colourmap, pic_data);
                            if color[3] == 0 {
                                interp_state.step_x();
                                edge_inv_w += edge_inv_w_dx;
                                x += 1;
                                continue;
                            }
                            if !no_depth {
                                self.depth_buffer.set_depth_unchecked(x, y, edge_inv_w);
                            }

                            let final_color = self.apply_debug_colour(
                                color,
                                edge_inv_w,
                                colour_mode,
                                debug_flat_colour.as_ref(),
                            );
                            write_pixel(buffer, x, y, &final_color, alpha);
                        } else {
                            if !no_depth
                                && !self
                                    .depth_buffer
                                    .test_and_set_depth_unchecked(x, y, edge_inv_w)
                            {
                                interp_state.step_x();
                                edge_inv_w += edge_inv_w_dx;
                                x += 1;
                                break;
                            }

                            let colourmap =
                                pic_data.base_colourmap(brightness, edge_inv_w * LIGHT_SCALE);
                            let color = texture_sampler.sample(u, v, colourmap, pic_data);

                            let final_color = self.apply_debug_colour(
                                color,
                                edge_inv_w,
                                colour_mode,
                                debug_flat_colour.as_ref(),
                            );
                            write_pixel(buffer, x, y, &final_color, alpha);
                        }
                        did_draw = true;

                        interp_state.step_x();
                        edge_inv_w += edge_inv_w_dx;
                        x += 1;
                    }
                }
            } // end else (non-sky)
        }

        if did_draw {
            self.stats.polygons_rendered += 1;
        } else {
            self.stats.polygons_no_draw += 1;
        }
    }

    /// Apply debug colour mode transformation to a sampled texel.
    #[inline(always)]
    fn apply_debug_colour(
        &self,
        original: &[u8; 4],
        inv_w: f32,
        mode: &DebugColourMode,
        flat_colour: Option<&[u8; 4]>,
    ) -> [u8; 4] {
        match mode {
            DebugColourMode::None => *original,
            DebugColourMode::SectorId => *flat_colour.unwrap_or(original),
            DebugColourMode::Depth => {
                // Full projection range with sqrt curve
                let inv_near = 1.0 / self.far_z;
                let inv_far = 1.0 / self.near_z;
                let t = ((inv_w - inv_near) / (inv_far - inv_near)).clamp(0.0, 1.0);
                let v = (t.sqrt() * 255.0) as u8;
                [v, v, v, 255]
            }
            DebugColourMode::Overdraw => {
                // Additive: read current pixel and brighten
                [
                    original[0].saturating_add(32),
                    original[1].saturating_add(8),
                    original[2].saturating_add(8),
                    255,
                ]
            }
        }
    }

    /// Draw a sprite polygon (billboard quad triangle).
    /// Uses masked rendering: peeks depth, skips transparent pixels, doesn't
    /// write depth.
    pub(super) fn draw_sprite_polygon(
        &mut self,
        quad: &super::sprites::SpriteQuad,
        pic_data: &mut PicData,
        buffer: &mut impl DrawBuffer,
    ) {
        let screen_poly = ScreenPoly(&self.screen_vertices_buffer[..self.screen_vertices_len]);

        let bounds = match screen_poly.bounds() {
            Some(bounds) => bounds,
            None => return,
        };

        let interpolator = match TriangleInterpolator::new(
            &screen_poly.0,
            &self.tex_coords_buffer[..self.tex_coords_len],
            &self.inv_w_buffer[..self.inv_w_len],
        ) {
            Some(interpolator) => interpolator,
            None => return,
        };

        let patch = pic_data.sprite_patch(quad.patch_index);
        let sprite_cols = patch.data.len();
        let sprite_rows = if sprite_cols > 0 {
            patch.data[0].len()
        } else {
            return;
        };
        let sprite_width_f = sprite_cols as f32;
        let sprite_height_f = sprite_rows as f32;

        let vertices = &screen_poly.0;
        let vertex_count = screen_poly.0.len();
        let width_f32 = self.width as f32;
        let height_f32 = self.height as f32;

        let y_start = bounds.0.y.max(0.0).ceil() as u32 as usize;
        let y_end = bounds.1.y.min(height_f32 - 1.0).floor() as u32 as usize;

        let inv_w_slice = &self.inv_w_buffer[..self.inv_w_len];

        for y in y_start..=y_end {
            let y_f = y as f32;
            let mut x0 = f32::INFINITY;
            let mut x1 = f32::NEG_INFINITY;
            let mut inv_w_at_x0 = 0.0f32;
            let mut inv_w_at_x1 = 0.0f32;
            let mut found = 0;

            for ei in 0..vertex_count {
                let ni = (ei + 1) % vertex_count;
                let start = unsafe { *vertices.get_unchecked(ei) };
                let end = unsafe { *vertices.get_unchecked(ni) };
                let dy = end.y - start.y;
                if dy.abs() < f32::EPSILON {
                    continue;
                }
                let (min_y, max_y) = if start.y < end.y {
                    (start.y, end.y)
                } else {
                    (end.y, start.y)
                };
                if y_f >= min_y && y_f < max_y {
                    let t = (y_f - start.y) / dy;
                    let x = start.x + (end.x - start.x) * t;
                    let iw_start = unsafe { *inv_w_slice.get_unchecked(ei) };
                    let iw_end = unsafe { *inv_w_slice.get_unchecked(ni) };
                    let iw = iw_start + (iw_end - iw_start) * t;
                    if found == 0 {
                        x0 = x;
                        inv_w_at_x0 = iw;
                        found += 1;
                    } else {
                        x1 = x;
                        inv_w_at_x1 = iw;
                        found += 1;
                        break;
                    }
                }
            }

            if found < 2 {
                continue;
            }
            if x0 > x1 {
                std::mem::swap(&mut x0, &mut x1);
                std::mem::swap(&mut inv_w_at_x0, &mut inv_w_at_x1);
            }

            let x_f = x0.max(0.0).ceil();
            let x_start = x_f as u32 as usize;
            let x_end = x1.min(width_f32 - 1.0).floor() as u32 as usize;

            let span_width = x1 - x0;
            let (mut edge_inv_w, edge_inv_w_dx) = if span_width > f32::EPSILON {
                let dx = 1.0 / span_width;
                let inv_w_dx = (inv_w_at_x1 - inv_w_at_x0) * dx;
                let start_inv_w = inv_w_at_x0 + (x_f - x0) * inv_w_dx;
                (start_inv_w, inv_w_dx)
            } else {
                (inv_w_at_x0, 0.0)
            };

            let mut interp_state = interpolator.init_scanline(x_f, y_f);

            for x in x_start..=x_end {
                if edge_inv_w <= 0.0 || edge_inv_w <= self.depth_buffer.peek_depth_unchecked(x, y) {
                    interp_state.step_x();
                    edge_inv_w += edge_inv_w_dx;
                    continue;
                }

                let (u, v) = interp_state.get_current_uv();

                // Map UV [0,1] to sprite texture coordinates
                let tex_col = (u * sprite_width_f) as i32;
                let tex_row = (v * sprite_height_f) as i32;

                if tex_col < 0
                    || tex_col >= sprite_cols as i32
                    || tex_row < 0
                    || tex_row >= sprite_rows as i32
                {
                    interp_state.step_x();
                    edge_inv_w += edge_inv_w_dx;
                    continue;
                }

                let color_index = patch.data[tex_col as usize][tex_row as usize];
                if color_index == usize::MAX {
                    // Transparent pixel
                    interp_state.step_x();
                    edge_inv_w += edge_inv_w_dx;
                    continue;
                }

                let colourmap = if quad.is_shadow {
                    pic_data.colourmap(33)
                } else {
                    pic_data.base_colourmap(quad.brightness, edge_inv_w * LIGHT_SCALE)
                };
                let lit_index = colourmap[color_index];
                let color = pic_data
                    .palette()
                    .get(lit_index)
                    .unwrap_or(&[255, 0, 255, 255]);

                buffer.set_pixel(x, y, color);
                // Write depth so the sky fill pass does not overwrite drawn sprite pixels
                self.depth_buffer.set_depth_unchecked(x, y, edge_inv_w);

                interp_state.step_x();
                edge_inv_w += edge_inv_w_dx;
            }
        }
    }

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

    /// Draw a line between two screen points with depth testing
    #[inline(always)]
    fn draw_line(
        &mut self,
        start: Vec2,
        end: Vec2,
        start_depth: f32,
        end_depth: f32,
        color: &[u8; 4],
        rend: &mut impl DrawBuffer,
    ) {
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let distance = (dx * dx + dy * dy).sqrt();

        if distance < 1.0 {
            return;
        }

        let steps = distance.ceil() as i32;
        let x_step = dx / steps as f32;
        let y_step = dy / steps as f32;
        let depth_step = (end_depth - start_depth) / steps as f32;

        let w = self.width as usize;
        let h = self.height as usize;
        for i in 0..=steps {
            let cx = (start.x + x_step * i as f32) as u32 as usize;
            let cy = (start.y + y_step * i as f32) as u32 as usize;
            let depth = start_depth + depth_step * i as f32;

            // Draw a 2px thick line by writing the pixel and its neighbour below
            for y in cy..=(cy + 1).min(h - 1) {
                if cx < w && y < h {
                    if self.depth_buffer.test_and_set_depth_unchecked(cx, y, depth) {
                        rend.set_pixel(cx, y, color);
                    }
                }
            }
        }
    }

    /// Draw all collected polygon outlines as a post-render overlay.
    /// Called once per frame after all geometry, sprites, and weapons are
    /// drawn.
    pub(super) fn draw_debug_polygon_outlines(&mut self, buffer: &mut impl DrawBuffer) {
        let outlines = std::mem::take(&mut self.debug.polygon_outlines);
        for (verts, depths, color) in &outlines {
            if verts.len() < 3 {
                continue;
            }
            for j in 0..verts.len() {
                let k = (j + 1) % verts.len();
                self.draw_line(verts[j], verts[k], depths[j], depths[k], color, buffer);
            }
        }
    }

    /// Draw normal direction lines as a post-render overlay.
    pub(super) fn draw_debug_normal_lines(&mut self, buffer: &mut impl DrawBuffer) {
        let lines = std::mem::take(&mut self.debug.normal_lines);
        let base = [200, 60, 10, 255]; // deep ember
        let tip_color = [255, 220, 50, 255]; // bright flame tip
        for (center, tip, depth) in &lines {
            self.draw_line(*center, *tip, *depth, *depth, &base, buffer);
            // Bright dot at the tip
            let tx = tip.x as u32 as usize;
            let ty = tip.y as u32 as usize;
            if tx < self.width as usize && ty < self.height as usize {
                buffer.set_pixel(tx, ty, &tip_color);
            }
        }
    }
}
