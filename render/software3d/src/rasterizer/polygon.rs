#[cfg(feature = "hprof")]
use coarse_prof::profile;

use glam::Vec2;
use level::{SurfaceKind, SurfacePolygon, WallType};
use pic_data::PicData;
use render_common::{DrawBuffer, FUZZ_TABLE, fuzz_darken};

use crate::Software3D;

use super::interpolation::TriangleInterpolator;
use super::sampling::{TextureSampler, sample_sky_pixel};
use super::{LIGHT_SCALE, ScreenPoly};

/// Minimum depth for real geometry. Must exceed `SKY_DEPTH` (f32::EPSILON)
/// so that distant polygons clamped to this value still pass the depth test
/// against sky pixels.
pub(crate) const MIN_GEOMETRY_DEPTH: f32 = 1.0e-6;

/// BOOM-style alpha blend: 66% source, 34% destination (ARGB u32 format
/// 0xAARRGGBB)
#[inline(always)]
fn alpha_blend(src: u32, dst: u32) -> u32 {
    let sr = (src >> 16) & 0xFF;
    let sg = (src >> 8) & 0xFF;
    let sb = src & 0xFF;
    let dr = (dst >> 16) & 0xFF;
    let dg = (dst >> 8) & 0xFF;
    let db = dst & 0xFF;
    let r = (sr * 170 + dr * 86) >> 8;
    let g = (sg * 170 + dg * 86) >> 8;
    let b = (sb * 170 + db * 86) >> 8;
    0xFF000000 | (r << 16) | (g << 8) | b
}

impl Software3D {
    /// Fast-path rasteriser: zero debug branches in the inner loop.
    /// Used when no debug draw options are active.
    #[inline(always)]
    pub(crate) fn draw_polygon(
        &mut self,
        polygon: &SurfacePolygon,
        brightness: usize,
        bounds: (Vec2, Vec2),
        pic_data: &mut PicData,
        buffer: &mut impl DrawBuffer,
    ) {
        #[cfg(feature = "hprof")]
        profile!("draw_polygon");

        let screen_poly = ScreenPoly(
            &self.rasterizer.screen_vertices_buffer[..self.rasterizer.screen_vertices_len],
        );

        let interpolator = match TriangleInterpolator::new(
            &screen_poly.0,
            &self.rasterizer.tex_coords_buffer[..self.rasterizer.tex_coords_len],
            &self.rasterizer.inv_w_buffer[..self.rasterizer.inv_w_len],
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
        let is_translucent = matches!(
            &polygon.surface_kind,
            SurfaceKind::Vertical {
                translucent: true,
                ..
            }
        );
        let is_sky = matches!(texture_sampler, TextureSampler::Sky);
        let vertices = &screen_poly.0;
        let vertex_count = screen_poly.0.len();
        let width = self.width as f32;
        let view_height = self.view_height as f32;

        // Pre-compute bounds
        let y_start = bounds.0.y.max(0.0).ceil() as u32 as usize;
        let y_end = bounds.1.y.min(view_height - 1.0).floor() as u32 as usize;

        let inv_w_slice = &self.rasterizer.inv_w_buffer[..self.rasterizer.inv_w_len];
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

            // Walk all edges, collect min/max x intersections with their inv_w.
            // Using min/max instead of first-two-found avoids span corruption
            // when a scanline grazes a short edge near a vertex.
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
                    if x < x0 {
                        x0 = x;
                        inv_w_at_x0 = iw;
                    }
                    if x > x1 {
                        x1 = x;
                        inv_w_at_x1 = iw;
                    }
                    found += 1;
                }
            }

            if found < 2 {
                continue;
            }

            let x_f = x0.max(0.0).ceil();
            let x_start = x_f as u32 as usize;
            let x_end = x1.min(width - 1.0).floor() as u32 as usize;

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
                let sky_combined = &self.sky.extended;
                let sky_tex_height = self.sky.tex_height;
                let sky_w = self.sky.tex_width;
                let sky_r = (y_f * self.sky.v_scale + self.sky.pitch_offset) as i32;
                let mut x = x_start;
                while x <= x_end {
                    if self
                        .rasterizer
                        .depth_buffer
                        .test_and_set_depth_unchecked(x, y, edge_inv_w)
                    {
                        let sky_col = (self.sky.x_offset + x as f32 * self.sky.x_step)
                            .rem_euclid(sky_w as f32)
                            as usize;
                        if let Some(color) =
                            sample_sky_pixel(sky_col, sky_r, sky_tex_height, sky_combined)
                        {
                            buf[y * buf_pitch + x] = color;
                        }
                    }
                    edge_inv_w += edge_inv_w_dx;
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
                        let peek = self.rasterizer.depth_buffer.peek_depth_unchecked(x, y);
                        if test_inv_w > peek {
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
                            if edge_inv_w <= self.rasterizer.depth_buffer.peek_depth_unchecked(x, y)
                            {
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
                            if color == 0 {
                                // Transparent pixel — don't write depth or color
                                interp_state.step_x();
                                edge_inv_w += edge_inv_w_dx;
                                x += 1;
                                continue;
                            }
                            if is_translucent {
                                // Alpha blend: 66% source, 34% dest (BOOM default)
                                let dst = buf[y * buf_pitch + x];
                                buf[y * buf_pitch + x] = alpha_blend(color, dst);
                                // No depth write — geometry behind shows
                                // through
                            } else {
                                self.rasterizer
                                    .depth_buffer
                                    .set_depth_unchecked(x, y, edge_inv_w);
                                buf[y * buf_pitch + x] = color;
                            }
                        } else {
                            // Depth test before UV — avoids the perspective divide on misses
                            if !self
                                .rasterizer
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

                            buf[y * buf_pitch + x] = color;
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
    pub(crate) fn draw_sprite_polygon(
        &mut self,
        quad: &crate::scene::sprites::SpriteQuad,
        pic_data: &mut PicData,
        buffer: &mut impl DrawBuffer,
    ) {
        let setup = match self.sprite_setup(quad, pic_data) {
            Some(s) => s,
            None => return,
        };
        let patch = pic_data.sprite_patch(quad.patch_index);

        for y in setup.y_start..=setup.y_end {
            let span = match self.sprite_scanline(&setup, y) {
                Some(s) => s,
                None => continue,
            };

            let mut edge_inv_w = span.edge_inv_w;
            let mut interp_state = setup.interpolator.init_scanline(span.x_f, y as f32);

            for x in span.x_start..=span.x_end {
                if edge_inv_w <= 0.0
                    || edge_inv_w <= self.rasterizer.depth_buffer.peek_depth_unchecked(x, y)
                {
                    interp_state.step_x();
                    edge_inv_w += span.edge_inv_w_dx;
                    continue;
                }

                let (u, v) = interp_state.get_current_uv();
                let tex_col = (u * setup.sprite_width_f) as i32;
                let tex_row = (v * setup.sprite_height_f) as i32;

                if tex_col < 0
                    || tex_col >= setup.sprite_cols as i32
                    || tex_row < 0
                    || tex_row >= setup.sprite_rows as i32
                {
                    interp_state.step_x();
                    edge_inv_w += span.edge_inv_w_dx;
                    continue;
                }

                let color_index = patch.data[tex_col as usize][tex_row as usize];
                if color_index == usize::MAX {
                    interp_state.step_x();
                    edge_inv_w += span.edge_inv_w_dx;
                    continue;
                }

                let colourmap = pic_data.base_colourmap(quad.brightness, edge_inv_w * LIGHT_SCALE);
                let lit_index = colourmap[color_index];
                let color = pic_data
                    .palette()
                    .get(lit_index)
                    .copied()
                    .unwrap_or(0xFFFF00FF);
                buffer.set_pixel(x, y, color);

                self.rasterizer
                    .depth_buffer
                    .set_depth_unchecked(x, y, edge_inv_w);

                interp_state.step_x();
                edge_inv_w += span.edge_inv_w_dx;
            }
        }
    }

    /// Fuzz variant of sprite rendering — reads existing framebuffer pixels
    /// at Y-offset and darkens them for the spectre shimmer effect.
    pub(crate) fn draw_sprite_fuzz(
        &mut self,
        quad: &crate::scene::sprites::SpriteQuad,
        pic_data: &mut PicData,
        buffer: &mut impl DrawBuffer,
    ) {
        let setup = match self.sprite_setup(quad, pic_data) {
            Some(s) => s,
            None => return,
        };
        let patch = pic_data.sprite_patch(quad.patch_index);
        let pitch = buffer.pitch();
        let h_clamp = setup.height_f32 as i32 - 1;

        for y in setup.y_start..=setup.y_end {
            let span = match self.sprite_scanline(&setup, y) {
                Some(s) => s,
                None => continue,
            };

            let mut edge_inv_w = span.edge_inv_w;
            let mut interp_state = setup.interpolator.init_scanline(span.x_f, y as f32);

            for x in span.x_start..=span.x_end {
                if edge_inv_w <= 0.0
                    || edge_inv_w <= self.rasterizer.depth_buffer.peek_depth_unchecked(x, y)
                {
                    interp_state.step_x();
                    edge_inv_w += span.edge_inv_w_dx;
                    continue;
                }

                let (u, v) = interp_state.get_current_uv();
                let tex_col = (u * setup.sprite_width_f) as i32;
                let tex_row = (v * setup.sprite_height_f) as i32;

                if tex_col < 0
                    || tex_col >= setup.sprite_cols as i32
                    || tex_row < 0
                    || tex_row >= setup.sprite_rows as i32
                {
                    interp_state.step_x();
                    edge_inv_w += span.edge_inv_w_dx;
                    continue;
                }

                let color_index = patch.data[tex_col as usize][tex_row as usize];
                if color_index == usize::MAX {
                    interp_state.step_x();
                    edge_inv_w += span.edge_inv_w_dx;
                    continue;
                }

                let buf = buffer.buf_mut();
                let offset = FUZZ_TABLE[self.fuzz_pos % FUZZ_TABLE.len()];
                let src_y = (y as i32 + offset).clamp(0, h_clamp) as usize;
                buf[y * pitch + x] = fuzz_darken(buf[src_y * pitch + x]);
                self.fuzz_pos += 1;

                self.rasterizer
                    .depth_buffer
                    .set_depth_unchecked(x, y, edge_inv_w);

                interp_state.step_x();
                edge_inv_w += span.edge_inv_w_dx;
            }
        }
    }

    /// Shared setup for sprite polygon rendering.
    fn sprite_setup(
        &self,
        quad: &crate::scene::sprites::SpriteQuad,
        pic_data: &PicData,
    ) -> Option<SpriteSetup> {
        let screen_poly = ScreenPoly(
            &self.rasterizer.screen_vertices_buffer[..self.rasterizer.screen_vertices_len],
        );

        let bounds = screen_poly.bounds()?;

        let interpolator = TriangleInterpolator::new(
            &screen_poly.0,
            &self.rasterizer.tex_coords_buffer[..self.rasterizer.tex_coords_len],
            &self.rasterizer.inv_w_buffer[..self.rasterizer.inv_w_len],
        )?;

        let patch = pic_data.sprite_patch(quad.patch_index);
        let sprite_cols = patch.data.len();
        let sprite_rows = if sprite_cols > 0 {
            patch.data[0].len()
        } else {
            return None;
        };

        let width_f32 = self.width as f32;
        let height_f32 = self.view_height as f32;

        Some(SpriteSetup {
            vertices: screen_poly.0.to_vec(),
            inv_w: self.rasterizer.inv_w_buffer[..self.rasterizer.inv_w_len].to_vec(),
            interpolator,
            sprite_cols,
            sprite_rows,
            sprite_width_f: sprite_cols as f32,
            sprite_height_f: sprite_rows as f32,
            width_f32,
            height_f32,
            y_start: bounds.0.y.max(0.0).ceil() as u32 as usize,
            y_end: bounds.1.y.min(height_f32 - 1.0).floor() as u32 as usize,
        })
    }

    /// Compute scanline span for a given Y in a sprite polygon.
    fn sprite_scanline(&self, setup: &SpriteSetup, y: usize) -> Option<SpriteScanline> {
        let y_f = y as f32;
        let mut x0 = f32::INFINITY;
        let mut x1 = f32::NEG_INFINITY;
        let mut inv_w_at_x0 = 0.0f32;
        let mut inv_w_at_x1 = 0.0f32;
        let mut found = 0;
        let vertex_count = setup.vertices.len();

        for ei in 0..vertex_count {
            let ni = (ei + 1) % vertex_count;
            let start = setup.vertices[ei];
            let end = setup.vertices[ni];
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
                let iw = setup.inv_w[ei] + (setup.inv_w[ni] - setup.inv_w[ei]) * t;
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
            return None;
        }
        if x0 > x1 {
            std::mem::swap(&mut x0, &mut x1);
            std::mem::swap(&mut inv_w_at_x0, &mut inv_w_at_x1);
        }

        let x_f = x0.max(0.0).ceil();
        let x_start = x_f as u32 as usize;
        let x_end = x1.min(setup.width_f32 - 1.0).floor() as u32 as usize;

        let span_width = x1 - x0;
        let (edge_inv_w, edge_inv_w_dx) = if span_width > f32::EPSILON {
            let dx = 1.0 / span_width;
            let inv_w_dx = (inv_w_at_x1 - inv_w_at_x0) * dx;
            let start_inv_w = inv_w_at_x0 + (x_f - x0) * inv_w_dx;
            (start_inv_w, inv_w_dx)
        } else {
            (inv_w_at_x0, 0.0)
        };

        Some(SpriteScanline {
            x_f,
            x_start,
            x_end,
            edge_inv_w,
            edge_inv_w_dx,
        })
    }
}

struct SpriteSetup {
    vertices: Vec<Vec2>,
    inv_w: Vec<f32>,
    interpolator: TriangleInterpolator,
    sprite_cols: usize,
    sprite_rows: usize,
    sprite_width_f: f32,
    sprite_height_f: f32,
    width_f32: f32,
    height_f32: f32,
    y_start: usize,
    y_end: usize,
}

struct SpriteScanline {
    x_f: f32,
    x_start: usize,
    x_end: usize,
    edge_inv_w: f32,
    edge_inv_w_dx: f32,
}
