#[cfg(feature = "hprof")]
use coarse_prof::profile;

#[cfg(feature = "debug_draw")]
use gameplay::BSP3D;
use gameplay::{FlatPic, PicData, SurfaceKind, SurfacePolygon, WallPic};
#[cfg(not(feature = "debug_draw"))]
use glam::Vec2;
#[cfg(feature = "debug_draw")]
use glam::{Vec2, Vec3, Vec4};
use render_trait::DrawBuffer;

use crate::Software3D;

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
enum TextureSampler<'a> {
    Vertical {
        texture: &'a WallPic,
        width: f32,
        height: f32,
        width_mask: f32,
        height_mask: f32,
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
    fn new(
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
                        width_mask: width_f32 - 1.0,
                        height_mask: height_f32 - 1.0,
                    }
                }
            }
            SurfaceKind::Horizontal { texture, .. } => {
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
            SurfaceKind::Vertical { texture: None, .. } => TextureSampler::Untextured,
        }
    }

    #[inline(always)]
    fn sample(&'a self, u: f32, v: f32, colourmap: &[usize], pic_data: &'a PicData) -> &'a [u8; 4] {
        unsafe {
            match self {
                TextureSampler::Vertical {
                    texture,
                    width,
                    height,
                    width_mask,
                    height_mask,
                } => {
                    let tex_x = (u.fract().abs() * width).min(*width_mask).floor() as u32 as usize;
                    let tex_y =
                        (v.fract().abs() * height).min(*height_mask).floor() as u32 as usize;
                    let color_index = *texture.data.get_unchecked(tex_x * texture.height + tex_y);
                    if color_index == usize::MAX {
                        &[0, 0, 0, 0]
                    } else {
                        let lit_color_index = *colourmap.get_unchecked(color_index);
                        pic_data.palette().get_unchecked(lit_color_index)
                    }
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
struct InterpolationState {
    current_tex: Vec2,
    current_inv_w: f32,
    tex_dx: Vec2,
    inv_w_dx: f32,
}

impl InterpolationState {
    #[inline(always)]
    fn get_current_uv(&self) -> (f32, f32, f32) {
        if self.current_inv_w > 0.0 {
            let w = 1.0 / self.current_inv_w;
            let corrected_tex = self.current_tex * w;
            (corrected_tex.x, corrected_tex.y, self.current_inv_w)
        } else {
            (self.current_tex.x, self.current_tex.y, self.current_inv_w)
        }
    }

    #[inline(always)]
    fn step_x(&mut self) {
        self.current_tex += self.tex_dx;
        self.current_inv_w += self.inv_w_dx;
    }
}

/// Pre-computed triangle interpolation data for efficient per-pixel texture
/// coordinate calculation
#[derive(Debug, Clone)]
struct TriangleInterpolator {
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
}

impl TriangleInterpolator {
    #[inline(always)]
    fn new(screen_verts: &[Vec2], tex_coords: &[Vec2], inv_w: &[f32]) -> Option<Self> {
        // Fast path for triangles - no need to search for best triangle
        if screen_verts.len() == 3 {
            let v0 = screen_verts[0];
            let v1 = screen_verts[1];
            let v2 = screen_verts[2];

            let denom = (v1.y - v2.y) * (v0.x - v2.x) + (v2.x - v1.x) * (v0.y - v2.y);
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
            if denom.abs() < 0.001 {
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
        })
    }

    /// Initialize interpolation state for a scanline
    #[inline(always)]
    fn init_scanline(&self, start_x: f32, y: f32) -> InterpolationState {
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
        }
    }
}

impl Software3D {
    #[inline(always)]
    pub(super) fn draw_polygon(
        &mut self,
        polygon: &SurfacePolygon,
        brightness: usize,
        pic_data: &mut PicData,
        buffer: &mut impl DrawBuffer,
        #[cfg(feature = "debug_draw")] bsp3d: &BSP3D,
        #[cfg(feature = "debug_draw")] outline_color: Option<[u8; 4]>,
    ) {
        #[cfg(feature = "hprof")]
        profile!("draw_polygon");

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

        // Cache frequently used values
        let sky_pic = pic_data.sky_pic();
        let sky_num = pic_data.sky_num();
        let texture_sampler =
            TextureSampler::new(&polygon.surface_kind, pic_data, sky_pic, sky_num);
        let vertices = &screen_poly.0;
        let vertex_count = screen_poly.0.len();
        let width_f32 = self.width as f32;
        let height_f32 = self.height as f32;

        // Pre-compute bounds
        let y_min = bounds.0.y.max(0.0);
        let y_max = bounds.1.y.min(height_f32 - 1.0);
        let y_start = y_min as u32 as usize;
        let y_end = y_max as u32 as usize;
        let mut span_drawn = false;

        // Instead of doing the scanline stuff here, could do it all in a fast loop for
        // all polygons before calling draw_polygon(). Then just slam the scanlines
        // out all in one go.
        for y in y_start..=y_end {
            if self.screen_occlusion.completed(y) {
                continue;
            }
            let y_f = y as f32;
            let mut x0 = f32::INFINITY;
            let mut x1 = f32::NEG_INFINITY;
            let mut found = 0;
            if vertex_count == 3 {
                // Faster if vertices count == 3
                let v0 = unsafe { *vertices.get_unchecked(0) };
                let v1 = unsafe { *vertices.get_unchecked(1) };
                let v2 = unsafe { *vertices.get_unchecked(2) };

                let edges = [(v0, v1), (v1, v2), (v2, v0)];
                for &(start, end) in &edges {
                    if (start.y <= y_f && end.y >= y_f) || (end.y <= y_f && start.y >= y_f) {
                        let t = (y_f - start.y) / (end.y - start.y);
                        if t >= 0.0 && t <= 1.0 {
                            let x = start.x + (end.x - start.x) * t;
                            if found == 0 {
                                x0 = x;
                                found += 1;
                            } else {
                                x1 = x;
                                found += 1;
                                break;
                            }
                        }
                    }
                }
            } else {
                let v0 = unsafe { *vertices.get_unchecked(0) };
                let v1 = unsafe { *vertices.get_unchecked(1) };
                let v2 = unsafe { *vertices.get_unchecked(2) };
                let v3 = unsafe { *vertices.get_unchecked(3) };

                let edges = [(v0, v1), (v1, v2), (v2, v3), (v3, v0)];
                for &(start, end) in &edges {
                    if (start.y <= y_f && end.y >= y_f) || (end.y <= y_f && start.y >= y_f) {
                        let t = (y_f - start.y) / (end.y - start.y);
                        if t >= 0.0 && t <= 1.0 {
                            let x = start.x + (end.x - start.x) * t;
                            if found == 0 {
                                x0 = x;
                                found += 1;
                            } else {
                                x1 = x;
                                found += 1;
                                break;
                            }
                        }
                    }
                }
            }

            if found < 2 {
                continue;
            }

            if x0 > x1 {
                std::mem::swap(&mut x0, &mut x1);
            }

            let x_f = x0.max(0.0).ceil();
            let x_start = x_f as u32 as usize;
            let x_end = x1.min(width_f32 - 1.0).floor() as u32 as usize;

            if self.screen_occlusion.is_range_occluded(y, x_start, x_end) {
                continue;
            }

            let mut interp_state = interpolator.init_scanline(x_f, y_f);
            for x in x_start..=x_end {
                #[cfg(feature = "hprof")]
                profile!("draw_textured_polygon X loop");
                let (u, v, inv_z) = interp_state.get_current_uv();
                // TODO: this part of loop costs 100fps~~
                if self.depth_buffer.test_and_set_depth_unchecked(x, y, inv_z) {
                    span_drawn = true;
                    // TODO: colourmap lookup is 30fps in X loop
                    let colourmap = pic_data.base_colourmap(brightness, inv_z * LIGHT_SCALE);
                    let color = texture_sampler.sample(u, v, colourmap, pic_data);
                    // TODO: need a separate masked texture draw
                    // This conditional causes a 15fps loss
                    // if color[3] == 0 {
                    //     interp_state.step_x();
                    //     continue;
                    // }
                    #[cfg(not(feature = "debug_draw"))]
                    buffer.set_pixel(x, y, &color);
                    #[cfg(feature = "debug_draw")]
                    let mut color = color;
                    #[cfg(feature = "debug_draw")]
                    if outline_color.is_some() {
                        if self.is_edge_pixel(x as f32, y_f, vertices) {
                            buffer.set_pixel(x, y, &outline_color.unwrap_or([0, 0, 0, 0]));
                        } else {
                            buffer.set_pixel(x, y, &color);
                        }
                    }
                }
                interp_state.step_x();
            }

            if span_drawn {
                self.screen_occlusion.add_span(y, x_start, x_end);
            }

            // buffer.debug_flip_and_present();
        }

        // Draw polygon normals after the main polygon rendering (if enabled)
        // #[cfg(feature = "debug_draw")]
        // self.draw_polygon_normals(polygon, bsp3d, screen_poly, inv_w, rend);
    }

    #[cfg(feature = "debug_draw")]
    pub(super) fn generate_pseudo_random_colour(&self, id: u32, brightness: usize) -> [u8; 4] {
        // Hash mix
        let mut hash = id.wrapping_mul(0x9E3779B9);
        hash ^= hash >> 15;
        hash = hash.wrapping_mul(0x85EBCA6B);
        hash ^= hash >> 13;

        // Generate pseudo-random hue in range [0, 360)
        let hue = (hash % 360) as f32;
        let sat = 1.0; // Full saturation
        let val = brightness as f32 / 255.0; // Brightness scale

        // HSV to RGB
        let c = val * sat;
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

    /// Draw polygon normals as short lines perpendicular to the polygon face
    #[inline(always)]
    #[cfg(feature = "debug_draw")]
    pub(super) fn draw_polygon_normals(
        &mut self,
        polygon: &SurfacePolygon,
        bsp3d: &BSP3D,
        screen_poly: &ScreenPoly,
        inv_w: &[f32],
        rend: &mut impl DrawBuffer,
    ) {
        if screen_poly.0.len() < 3 || inv_w.len() < 3 {
            return;
        }

        // Calculate the center of the polygon in world space
        let center = polygon
            .vertices
            .iter()
            .fold(Vec3::ZERO, |acc, &vertex_idx| {
                acc + bsp3d.vertex_get(vertex_idx)
            })
            / polygon.vertices.len() as f32;

        // Calculate normal endpoint (short line outward from face)
        let normal_length = 8.0; // Length of normal line in world units - adjust for visibility
        let normal_end = center + polygon.normal * normal_length;

        // Project both center and normal endpoint to screen space
        let view_projection = self.projection_matrix * self.view_matrix;

        let center_clip = view_projection * Vec4::new(center.x, center.y, center.z, 1.0);
        let normal_end_clip =
            view_projection * Vec4::new(normal_end.x, normal_end.y, normal_end.z, 1.0);

        if center_clip.w > 0.0 && normal_end_clip.w > 0.0 {
            let center_ndc = center_clip / center_clip.w;
            let normal_end_ndc = normal_end_clip / normal_end_clip.w;

            let center_screen = Vec2::new(
                (center_ndc.x + 1.0) * 0.5 * self.width as f32,
                (1.0 - center_ndc.y) * 0.5 * self.height as f32,
            );
            let normal_end_screen = Vec2::new(
                (normal_end_ndc.x + 1.0) * 0.5 * self.width as f32,
                (1.0 - normal_end_ndc.y) * 0.5 * self.height as f32,
            );

            let center_depth = 1.0 - (1.0 / center_clip.w);
            let normal_end_depth = 1.0 - (1.0 / normal_end_clip.w);

            // Draw line from center to normal endpoint
            self.draw_line(
                center_screen,
                normal_end_screen,
                center_depth,
                normal_end_depth,
                &[255, 0, 255, 255], // Magenta color for high visibility against textures
                rend,
            );
        }
    }

    /// Draw a line between two screen points with depth testing
    #[inline(always)]
    #[cfg(feature = "debug_draw")]
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

        for i in 0..=steps {
            let x = (start.x + x_step * i as f32) as u32 as usize;
            let y = (start.y + y_step * i as f32) as u32 as usize;
            let depth = start_depth + depth_step * i as f32;

            if x < self.width as u32 as usize && y < self.height as u32 as usize {
                if self.depth_buffer.test_and_set_depth_unchecked(x, y, depth) {
                    rend.set_pixel(x, y, color);
                }
            }
        }
    }

    #[inline(always)]
    #[cfg(feature = "debug_draw")]
    fn is_edge_pixel(&self, x: f32, y: f32, screen_poly: &[Vec2]) -> bool {
        let threshold = 1.0;

        for i in 0..screen_poly.len() {
            let v1 = screen_poly[i];
            let v2 = screen_poly[(i + 1) % screen_poly.len()];

            let dist = self.point_to_line_distance(x, y, v1.x, v1.y, v2.x, v2.y);
            if dist <= threshold {
                return true;
            }
        }
        false
    }

    #[inline(always)]
    #[cfg(feature = "debug_draw")]
    fn point_to_line_distance(&self, px: f32, py: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
        let a = px - x1;
        let b = py - y1;
        let c = x2 - x1;
        let d = y2 - y1;

        let dot = a * c + b * d;
        let len_sq = c * c + d * d;

        if len_sq == 0.0 {
            return (a * a + b * b).sqrt();
        }

        let param = dot / len_sq;

        let (xx, yy) = if param < 0.0 {
            (x1, y1)
        } else if param > 1.0 {
            (x2, y2)
        } else {
            (x1 + param * c, y1 + param * d)
        };

        let dx = px - xx;
        let dy = py - yy;
        (dx * dx + dy * dy).sqrt()
    }
}
