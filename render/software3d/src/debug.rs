use gameplay::{PicData, SurfaceKind, SurfacePolygon, WallType};
use glam::Vec2;
use hud_util::{draw_text_line, hud_scale, measure_text_line};
use render_trait::DrawBuffer;

use crate::poly_occluder::{LIGHT_SCALE, MIN_GEOMETRY_DEPTH, ScreenPoly};
use crate::render::{TextureSampler, TriangleInterpolator};
use crate::{DebugColourMode, Software3D};

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

    /// Draw the debug overlay text line in the upper-right corner, if set.
    pub(super) fn draw_debug_line(&mut self, pic_data: &PicData, pixels: &mut impl DrawBuffer) {
        let text = self.debug.current_line().to_ascii_uppercase();
        if text.is_empty() {
            return;
        }
        let (sx, sy) = hud_scale(pixels);
        let palette = pic_data.wad_palette();
        let width = measure_text_line(&text, sx);
        let x = pixels.size().width_f32() - width - 4.0 * sx;
        draw_text_line(&text, x, 2.0, sx, sy, palette, pixels);
    }

    /// Set the upper-right debug text overlay line, resetting the 5-second
    /// auto-clear timer.
    pub fn set_debug_line(&mut self, s: String) {
        self.debug.set_line(s);
    }
}
