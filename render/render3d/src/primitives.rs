#[cfg(feature = "hprof")]
use coarse_prof::profile;

use gameplay::{FlatPic, PicData, SurfaceKind, SurfacePolygon, WallPic};
use glam::Vec2;
use render_trait::{PixelBuffer, RenderTrait};

use crate::Renderer3D;

const LIGHT_MIN_Z: f32 = 0.001;
const LIGHT_MAX_Z: f32 = 0.055;
const LIGHT_SCALE: f32 = 8.0;
const LIGHT_RANGE: f32 = 1.0 / (LIGHT_MAX_Z - LIGHT_MIN_Z);

/// Represents a 2D polygon in screen space
#[derive(Debug, Clone)]
pub struct ScreenPoly {
    pub vertices: Vec<Vec2>,
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
    fn new(surface_kind: &SurfaceKind, pic_data: &'a PicData) -> Self {
        match surface_kind {
            SurfaceKind::Vertical {
                texture: Some(tex_id),
                ..
            } => {
                if *tex_id == pic_data.sky_pic() {
                    TextureSampler::Sky
                } else {
                    let texture = pic_data.wall_pic(*tex_id);
                    TextureSampler::Vertical {
                        texture,
                        width: texture.data.len() as f32,
                        height: texture.data[0].len() as f32,
                        width_mask: texture.data.len() as f32 - 1.0,
                        height_mask: texture.data[0].len() as f32 - 1.0,
                    }
                }
            }
            SurfaceKind::Horizontal { texture, .. } => {
                if *texture == pic_data.sky_num() {
                    TextureSampler::Sky
                } else {
                    let texture = pic_data.get_flat(*texture);
                    TextureSampler::Horizontal {
                        texture,
                        width: texture.data.len() as f32,
                        height: texture.data[0].len() as f32,
                    }
                }
            }
            SurfaceKind::Vertical { texture: None, .. } => TextureSampler::Untextured,
        }
    }

    #[inline(always)]
    fn sample(&'a self, u: f32, v: f32, colourmap: &[usize], pic_data: &'a PicData) -> &'a [u8; 4] {
        // The unsafe unchecked access gains 10fps or more. Depending on level.
        match self {
            TextureSampler::Vertical {
                texture,
                width,
                height,
                width_mask,
                height_mask,
            } => {
                let tex_x = (u.fract().abs() * width).min(*width_mask);
                unsafe {
                    let column = &texture.data.get_unchecked(tex_x as u32 as usize);
                    let tex_y = (v.fract().abs() * height).min(*height_mask);
                    let color_index = column.get_unchecked(tex_y as u32 as usize);
                    // Skip blank pixels in masked textures
                    if *color_index == usize::MAX {
                        return &[0, 0, 0, 0];
                    }
                    let lit_color_index = colourmap.get_unchecked(*color_index);
                    pic_data.palette().get_unchecked(*lit_color_index)
                }
            }
            TextureSampler::Horizontal {
                texture,
                width,
                height,
            } => {
                let tex_x = ((u.abs() * width) as i32 as usize) & 63;
                let tex_y = ((v.abs() * height) as i32 as usize) & 63;
                unsafe {
                    let color_index = texture.data.get_unchecked(tex_y).get_unchecked(tex_x);
                    let lit_color_index = colourmap.get_unchecked(*color_index);
                    pic_data.palette().get_unchecked(*lit_color_index)
                }
            }
            TextureSampler::Sky => &[32, 32, 32, 255],
            TextureSampler::Untextured => &[32, 32, 32, 255],
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
        if self.current_inv_w > 0.00001 {
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
        // Find the best triangle for interpolation
        let mut best_triangle = None;
        let mut best_area = 0.0;

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
            }
        }

        let (i0, i1, i2) = best_triangle?;
        let v0 = screen_verts[i0];
        let v1 = screen_verts[i1];
        let v2 = screen_verts[i2];

        let denom = (v1.y - v2.y) * (v0.x - v2.x) + (v2.x - v1.x) * (v0.y - v2.y);

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

        // Calculate per-pixel increments for texture coordinates
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

impl ScreenPoly {
    /// Get axis-aligned bounding box of polygon
    #[inline(always)]
    pub fn bounds(&self) -> Option<(Vec2, Vec2)> {
        if self.vertices.is_empty() {
            return None;
        }

        let mut min = self.vertices[0];
        let mut max = self.vertices[0];

        for vertex in &self.vertices[1..] {
            min.x = min.x.min(vertex.x);
            min.y = min.y.min(vertex.y);
            max.x = max.x.max(vertex.x);
            max.y = max.y.max(vertex.y);
        }

        Some((min, max))
    }
}

impl Renderer3D {
    pub(super) fn generate_pseudo_random_colour(&self, id: u32, brightness: usize) -> [u8; 4] {
        let mut hash = id.wrapping_mul(2654435761);
        hash ^= hash >> 16;
        hash = hash.wrapping_mul(2654435761);
        hash ^= hash >> 16;

        let t = (hash & 0xFFFF) as f32 / 65535.0;
        let brightness_scale = brightness as f32 / 255.0;

        // Sunset pastel base values (red-orange emphasis)
        let base_r = 230.0 + t * 25.0;
        let base_g = 80.0 + t * 40.0;
        let base_b = 40.0 + t * 20.0;

        let r = (base_r * brightness_scale).min(255.0) as u8;
        let g = (base_g * brightness_scale).min(255.0) as u8;
        let b = (base_b * brightness_scale).min(255.0) as u8;

        [r, g, b, 255]
    }

    #[inline(always)]
    pub(super) fn draw_textured_polygon(
        &mut self,
        polygon: &SurfacePolygon,
        screen_poly: &ScreenPoly,
        tex_coords: &[Vec2],
        inv_w: &[f32],
        brightness: usize,
        pic_data: &PicData,
        rend: &mut impl RenderTrait,
        outline_color: Option<[u8; 4]>,
    ) {
        #[cfg(feature = "hprof")]
        profile!("draw_textured_polygon");

        if screen_poly.vertices.len() < 3 || tex_coords.len() < 3 || inv_w.len() < 3 {
            return;
        }
        let bounds = match screen_poly.bounds() {
            Some(bounds) => bounds,
            None => return,
        };

        let interpolator = match TriangleInterpolator::new(&screen_poly.vertices, tex_coords, inv_w)
        {
            Some(interpolator) => interpolator,
            None => return,
        };

        // Cache frequently used values
        let texture_sampler = TextureSampler::new(&polygon.surface_kind, pic_data);
        let vertices = &screen_poly.vertices;
        let vertex_count = vertices.len();
        let width_f32 = self.width as f32;
        let height_f32 = self.height as f32;

        // Pre-compute outline handling
        let has_outline = outline_color.is_some();
        let outline_col = outline_color.unwrap_or([0, 0, 0, 0]);

        // Pre-compute bounds
        let y_min = bounds.0.y.max(0.0);
        let y_max = bounds.1.y.min(height_f32 - 1.0);
        let y_start = y_min as i32;
        let y_end = y_max as i32;

        // Main rendering loops
        for y in y_start..=y_end {
            #[cfg(feature = "hprof")]
            profile!("draw_textured_polygon Y loop");
            let y_float = y as f32;

            let mut intersection_count = 0;
            for i in 0..vertex_count {
                let v1 = vertices[i];
                let v2 = vertices[(i + 1) % vertex_count];

                if (v1.y <= y_float && v2.y >= y_float) || (v2.y <= y_float && v1.y >= y_float) {
                    let t = (y_float - v1.y) / (v2.y - v1.y);
                    if t >= 0.0 && t <= 1.0 {
                        let x = v1.x + (v2.x - v1.x) * t;
                        if intersection_count < 64 {
                            // Insert in sorted order (insertion sort)
                            let mut insert_pos = intersection_count;
                            while insert_pos > 0 && self.x_intersections[insert_pos - 1] > x {
                                self.x_intersections[insert_pos] =
                                    self.x_intersections[insert_pos - 1];
                                insert_pos -= 1;
                            }
                            self.x_intersections[insert_pos] = x;
                            intersection_count += 1;
                        }
                    }
                }
            }

            if intersection_count < 2 {
                continue;
            }

            let mut i = 0;
            while i < intersection_count - 1 {
                let x_start = self.x_intersections[i].max(0.0).ceil() as i32;
                let x_end = self.x_intersections[i + 1].min(width_f32 - 1.0).floor() as i32;

                let mut interp_state = interpolator.init_scanline(x_start as f32, y_float);

                for x in x_start..=x_end {
                    #[cfg(feature = "hprof")]
                    profile!("draw_textured_polygon X loop");
                    let (u, v, inv_z) = interp_state.get_current_uv();

                    let bright_scale = ((inv_z - LIGHT_MIN_Z) * LIGHT_RANGE) * LIGHT_SCALE;
                    let colourmap = pic_data.vert_light_colourmap(brightness, bright_scale);
                    let mut color = texture_sampler.sample(u, v, colourmap, pic_data);
                    if color[3] == 0 {
                        interp_state.step_x();
                        continue;
                    }

                    if has_outline {
                        if self.is_edge_pixel(x as f32, y_float, screen_poly) {
                            color = &outline_col;
                        }
                    }

                    let x = x as usize;
                    let y = y as usize;
                    if self
                        .depth_buffer
                        .test_and_set_depth_unchecked(x, y, 1.0 - inv_z)
                    {
                        rend.draw_buffer().set_pixel(x, y, &color);
                    }

                    interp_state.step_x();
                }
                i += 1;
            }
        }
    }

    #[inline(always)]
    fn is_edge_pixel(&self, x: f32, y: f32, screen_poly: &ScreenPoly) -> bool {
        let threshold = 1.0;

        for i in 0..screen_poly.vertices.len() {
            let v1 = screen_poly.vertices[i];
            let v2 = screen_poly.vertices[(i + 1) % screen_poly.vertices.len()];

            let dist = self.point_to_line_distance(x, y, v1.x, v1.y, v2.x, v2.y);
            if dist <= threshold {
                return true;
            }
        }
        false
    }

    #[inline(always)]
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
