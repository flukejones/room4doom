#[cfg(feature = "hprof")]
use coarse_prof::profile;

use gameplay::{PicData, SurfaceKind, SurfacePolygon, WallType};
use glam::Vec2;
use render_trait::{DrawBuffer, SOFT_PIXEL_CHANNELS};

use crate::Software3D;
use crate::depth_buffer::SKY_DEPTH;
use crate::render::{TextureSampler, TriangleInterpolator};

/// Minimum depth for real geometry. Must exceed `SKY_DEPTH` (f32::EPSILON)
/// so that distant polygons clamped to this value still pass the depth test
/// against sky pixels.
pub(crate) const MIN_GEOMETRY_DEPTH: f32 = 1.0e-6;

const LIGHT_MIN_Z: f32 = 0.001;
const LIGHT_MAX_Z: f32 = 0.055;
const LIGHT_RANGE: f32 = 1.0 / (LIGHT_MAX_Z - LIGHT_MIN_Z);
pub(crate) const LIGHT_SCALE: f32 = LIGHT_RANGE * 8.0 * 16.0;

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
}
