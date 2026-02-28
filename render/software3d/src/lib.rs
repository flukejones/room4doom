#[cfg(feature = "hprof")]
use coarse_prof::profile;
use gameplay::{
    AABB, BSP3D, Level, MapData, PVS, PicData, Player, Sector, SubSector, SurfaceKind, SurfacePolygon, WallTexPin, WallType
};
use glam::{Mat4, Vec2, Vec3, Vec4};
use render_trait::DrawBuffer;

use std::f32::consts::PI;

mod depth_buffer;
mod render;
mod sprites;
#[cfg(test)]
mod tests;
mod weapon;

use depth_buffer::DepthBuffer;

#[derive(Clone, Copy)]
struct VertexCache {
    view_pos: Vec4,
    clip_pos: Vec4,
    valid: bool,
}

const IS_SSECTOR_MASK: u32 = 0x8000_0000;
const CLIP_VERTICES_LEN: usize = 3;
const MAX_CLIPPED_VERTICES: usize = 16;

/// Debug colouring mode for polygons.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum DebugColourMode {
    #[default]
    None,
    /// Flat colour per sector (hashed from sector_id)
    SectorId,
    /// Depth buffer visualisation (near=white, far=black)
    Depth,
    /// Overdraw heatmap (brighter = more polygons drawn to that pixel)
    Overdraw,
}

/// Mutually-exclusive debug overlay mode (colour visualisation or wireframe).
/// Used as an argh enum option via `FromStr`.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum DebugOverlay {
    #[default]
    None,
    SectorId,
    Depth,
    Overdraw,
    Wireframe,
}

impl std::str::FromStr for DebugOverlay {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "sector_id" => Ok(Self::SectorId),
            "depth" => Ok(Self::Depth),
            "overdraw" => Ok(Self::Overdraw),
            "wireframe" => Ok(Self::Wireframe),
            other => Err(format!(
                "unknown overlay '{}'. Expected: sector_id, depth, overdraw, wireframe",
                other
            )),
        }
    }
}

/// Debug rendering options for the 3D software renderer.
#[derive(Debug, Clone, Default)]
pub struct DebugDrawOptions {
    pub outline: bool,
    pub normals: bool,
    pub clear_colour: Option<[u8; 4]>,
    pub alpha: Option<u8>,
    pub no_depth: bool,
    pub colour_mode: DebugColourMode,
    pub wireframe: bool,
}

impl DebugDrawOptions {
    /// Returns true if any debug option is active that affects the rasteriser
    /// hot loop (alpha blend, depth disable, colour mode, wireframe).
    pub fn is_active(&self) -> bool {
        self.alpha.is_some()
            || self.no_depth
            || self.colour_mode != DebugColourMode::None
            || self.wireframe
    }
}

/// A 3D software renderer for Doom levels.
///
/// This renderer displays the level geometry in true 3D space,
/// showing floors, ceilings, walls with different colours.
///
/// Features depth buffer optimization for improved performance by testing
/// polygon visibility before expensive occlusion calculations.
pub struct Software3D {
    width: u32,
    height: u32,
    width_minus_one: f32,
    height_minus_one: f32,
    fov: f32,
    view_matrix: Mat4,
    projection_matrix: Mat4,
    depth_buffer: DepthBuffer,
    near_z: f32,
    far_z: f32,
    // Static arrays to eliminate hot path allocations
    screen_vertices_buffer: [Vec2; MAX_CLIPPED_VERTICES],
    tex_coords_buffer: [Vec2; MAX_CLIPPED_VERTICES],
    inv_w_buffer: [f32; MAX_CLIPPED_VERTICES],
    screen_vertices_len: usize,
    tex_coords_len: usize,
    inv_w_len: usize,
    clip_vertices: [Vec4; CLIP_VERTICES_LEN],
    clipped_vertices_buffer: [Vec4; MAX_CLIPPED_VERTICES],
    clipped_tex_coords_buffer: [Vec3; MAX_CLIPPED_VERTICES],
    clipped_vertices_len: usize,
    // Vertex transformation cache
    vertex_cache: Vec<VertexCache>,
    current_frame_id: u32,
    polygons_submitted_count: u32,
    polygons_frustum_clipped_count: u32,
    polygons_early_culled_count: u32,
    polygons_rendered_count: u32,
    polygons_no_draw_count: u32,
    polygons_depth_rejected_count: u32,
    #[cfg(feature = "render_stats")]
    render_stats_last_print: std::time::Instant,
    debug_draw: DebugDrawOptions,
    has_debug_draw: bool,
    debug_polygon_outlines: Vec<(Vec<Vec2>, Vec<f32>, [u8; 4])>,
    /// (screen_center, screen_tip, depth) for normal direction lines
    debug_normal_lines: Vec<(Vec2, Vec2, f32)>,
}

impl Software3D {
    pub fn new(width: f32, height: f32, fov: f32, debug_draw: DebugDrawOptions) -> Self {
        let near = 4.0;
        let far = 10000.0;

        let mut s = Self {
            width: width as u32,
            height: height as u32,
            width_minus_one: width - 1.0,
            height_minus_one: height - 1.0,
            fov,
            view_matrix: Mat4::IDENTITY,
            projection_matrix: Mat4::IDENTITY,
            depth_buffer: DepthBuffer::new(width as usize, height as usize),
            near_z: near,
            far_z: far,
            screen_vertices_buffer: [Vec2::ZERO; MAX_CLIPPED_VERTICES],
            tex_coords_buffer: [Vec2::ZERO; MAX_CLIPPED_VERTICES],
            inv_w_buffer: [0.0; MAX_CLIPPED_VERTICES],
            screen_vertices_len: 0,
            tex_coords_len: 0,
            inv_w_len: 0,
            clip_vertices: [Vec4::ZERO; 3],
            clipped_vertices_buffer: [Vec4::ZERO; MAX_CLIPPED_VERTICES],
            clipped_tex_coords_buffer: [Vec3::ZERO; MAX_CLIPPED_VERTICES],
            clipped_vertices_len: 0,
            vertex_cache: Vec::new(),
            current_frame_id: 0,
            polygons_submitted_count: 0,
            polygons_frustum_clipped_count: 0,
            polygons_early_culled_count: 0,
            polygons_rendered_count: 0,
            polygons_no_draw_count: 0,
            polygons_depth_rejected_count: 0,
            #[cfg(feature = "render_stats")]
            render_stats_last_print: std::time::Instant::now(),
            has_debug_draw: debug_draw.is_active(),
            debug_draw,
            debug_polygon_outlines: Vec::new(),
            debug_normal_lines: Vec::new(),
        };
        s.set_fov(fov);
        s
    }

    /// Resizes the renderer viewport and updates the projection matrix.
    pub fn resize(&mut self, width: f32, height: f32) {
        self.width = width as u32;
        self.height = height as u32;
        self.width_minus_one = width - 1.0;
        self.height_minus_one = height - 1.0;

        self.set_fov(self.fov);
        self.depth_buffer.resize(width as usize, height as usize);
    }

    /// Sets the field of view and updates the projection matrix.
    pub fn set_fov(&mut self, fov: f32) {
        self.fov = fov;
        let aspect = self.width as f32 / self.height as f32;
        self.projection_matrix =
            Mat4::perspective_rh_gl(fov * 0.75, aspect, self.near_z, self.far_z);
        // CRT stretch: Doom rendered 320x200 but displayed on 4:3 CRT as 320x240,
        // making each pixel 1.2x taller than wide. Scale the projection's Y axis
        // to replicate this.
        self.projection_matrix.y_axis.y *= 240.0 / 200.0;
    }

    fn update_view_matrix(&mut self, player: &Player) {
        if let Some(mobj) = player.mobj() {
            // Use player.viewz which accounts for viewheight (eye level above feet)
            // This is crucial for proper 3D camera positioning in Doom
            let pos = Vec3::new(mobj.xy.x, mobj.xy.y, player.viewz);
            let angle = mobj.angle.rad();
            let pitch = player.lookdir as f32 * PI / 180.0;

            let forward = Vec3::new(
                angle.cos() * pitch.cos(),
                angle.sin() * pitch.cos(),
                pitch.sin(),
            );
            let up = Vec3::Z;

            self.view_matrix = Mat4::look_at_rh(pos, pos + forward, up);
        }
    }

    // ==========================================
    // BSP AND SUBSECTOR RENDERING
    // ==========================================

    /// Check if 3D bounding box is fully outside view frustum
    fn prepare_vertex_cache(&mut self, bsp3d: &BSP3D) {
        let vertex_count = bsp3d.vertices.len();
        if self.vertex_cache.len() != vertex_count {
            self.vertex_cache.resize(
                vertex_count,
                VertexCache {
                    view_pos: Vec4::ZERO,
                    clip_pos: Vec4::ZERO,
                    valid: false,
                },
            );
        } else {
            for cache_entry in &mut self.vertex_cache {
                cache_entry.valid = false;
            }
        }
    }

    #[inline(always)]
    fn get_transformed_vertex(&mut self, vertex_idx: usize, bsp3d: &BSP3D) -> (Vec4, Vec4) {
        unsafe {
            let cache_entry = self.vertex_cache.get_unchecked_mut(vertex_idx);
            if !cache_entry.valid {
                let vertex = bsp3d.vertex_get(vertex_idx);
                let world_pos = Vec4::new(vertex.x, vertex.y, vertex.z, 1.0);
                let view_pos = self.view_matrix * world_pos;
                let clip_pos = self.projection_matrix * view_pos;

                cache_entry.view_pos = view_pos;
                cache_entry.clip_pos = clip_pos;
                cache_entry.valid = true;
            }

            (cache_entry.view_pos, cache_entry.clip_pos)
        }
    }

    fn is_bbox_outside_fov(&self, bbox: &AABB) -> bool {
        // Generate all 8 corners of the 3D bbox
        let view_projection = self.projection_matrix * self.view_matrix;
        let clip_corners = [
            view_projection * Vec4::new(bbox.min.x, bbox.min.y, bbox.min.z, 1.0),
            view_projection * Vec4::new(bbox.max.x, bbox.min.y, bbox.min.z, 1.0),
            view_projection * Vec4::new(bbox.max.x, bbox.max.y, bbox.min.z, 1.0),
            view_projection * Vec4::new(bbox.min.x, bbox.max.y, bbox.min.z, 1.0),
            view_projection * Vec4::new(bbox.min.x, bbox.min.y, bbox.max.z, 1.0),
            view_projection * Vec4::new(bbox.max.x, bbox.min.y, bbox.max.z, 1.0),
            view_projection * Vec4::new(bbox.max.x, bbox.max.y, bbox.max.z, 1.0),
            view_projection * Vec4::new(bbox.min.x, bbox.max.y, bbox.max.z, 1.0),
        ];

        // If bounding box is fully outside any frustum plane, cull immediately
        if clip_corners.iter().all(|c| c.x < -c.w)
            || clip_corners.iter().all(|c| c.x > c.w)
            || clip_corners.iter().all(|c| c.y < -c.w)
            || clip_corners.iter().all(|c| c.y > c.w)
            || clip_corners.iter().all(|c| c.z < -c.w)
            || clip_corners.iter().all(|c| c.z > c.w)
        {
            return true;
        }

        false
    }

    /// Early screen bounds check to reject polygons with all vertices outside
    /// frustum. Uses separating-axis test against all 6 frustum planes in
    /// clip space.
    fn cull_polygon_bounds(&mut self, polygon: &SurfacePolygon, bsp3d: &BSP3D) -> bool {
        let mut all_outside_left = true;
        let mut all_outside_right = true;
        let mut all_outside_bottom = true;
        let mut all_outside_top = true;
        let mut all_outside_near = true;
        let mut all_outside_far = true;

        for i in 0..CLIP_VERTICES_LEN {
            let vidx = unsafe { *polygon.vertices.get_unchecked(i) };
            let (_, clip_pos) = self.get_transformed_vertex(vidx, bsp3d);
            self.clip_vertices[i] = clip_pos;

            if clip_pos.x >= -clip_pos.w {
                all_outside_left = false;
            }
            if clip_pos.x <= clip_pos.w {
                all_outside_right = false;
            }
            if clip_pos.y >= -clip_pos.w {
                all_outside_bottom = false;
            }
            if clip_pos.y <= clip_pos.w {
                all_outside_top = false;
            }
            if clip_pos.z >= -clip_pos.w {
                all_outside_near = false;
            }
            if clip_pos.z <= clip_pos.w {
                all_outside_far = false;
            }
        }

        if all_outside_left
            || all_outside_right
            || all_outside_bottom
            || all_outside_top
            || all_outside_near
            || all_outside_far
        {
            return true;
        }

        // Early sub-pixel rejection: if all vertices are in front of the camera,
        // estimate projected screen area. Reject if < 1 pixel.
        // This is conservative — polygons clipped by the near plane may be larger
        // than estimated, so we only apply this when all w > 0.
        let c0 = self.clip_vertices[0];
        let c1 = self.clip_vertices[1];
        let c2 = self.clip_vertices[2];
        if c0.w > 0.0 && c1.w > 0.0 && c2.w > 0.0 {
            let hw = self.width as f32 * 0.5;
            let hh = self.height as f32 * 0.5;
            let sx0 = (c0.x / c0.w) * hw;
            let sy0 = (c0.y / c0.w) * hh;
            let sx1 = (c1.x / c1.w) * hw;
            let sy1 = (c1.y / c1.w) * hh;
            let sx2 = (c2.x / c2.w) * hw;
            let sy2 = (c2.y / c2.w) * hh;
            // 2x triangle area via cross product
            let area = ((sx1 - sx0) * (sy2 - sy0) - (sx2 - sx0) * (sy1 - sy0)).abs();
            if area < 2.0 {
                return true;
            }
        }

        false
    }

    /// Calculate screen area of projected polygon vertices
    fn calculate_screen_area(&self, vertices: &[Vec2]) -> f32 {
        if vertices.len() < 3 {
            return 0.0;
        }

        // Shoelace formula for polygon area
        let mut area = 0.0;
        let n = vertices.len();
        for i in 0..n {
            let j = (i + 1) % n;
            area += vertices[i].x * vertices[j].y;
            area -= vertices[j].x * vertices[i].y;
        }
        (area * 0.5).abs()
    }

    /// Check if polygon should be culled based on screen area
    fn should_cull_polygon_area(&self, screen_vertices: &[Vec2]) -> bool {
        let area = self.calculate_screen_area(screen_vertices);
        area < 1.0 // Cull polygons smaller than 1 pixel
    }

    /// Hi-Z depth rejection: check if the polygon is entirely behind
    /// already-drawn geometry using the tiled depth buffer. Conservative —
    /// only rejects when ALL overlapping tiles are fully covered and the
    /// polygon's closest depth is behind the farthest first-write in every
    /// tile. Never produces false rejections.
    fn is_polygon_depth_occluded(&self) -> bool {
        let verts = &self.screen_vertices_buffer[..self.screen_vertices_len];
        let depths = &self.inv_w_buffer[..self.inv_w_len];

        // Find the polygon's closest depth (max 1/w)
        let mut poly_max_depth: f32 = 0.0;
        for &d in depths {
            if d > poly_max_depth {
                poly_max_depth = d;
            }
        }
        if poly_max_depth <= 0.0 {
            return false;
        }

        // Find screen-space bounding box
        let mut min_x = f32::MAX;
        let mut max_x = f32::MIN;
        let mut min_y = f32::MAX;
        let mut max_y = f32::MIN;
        for v in verts {
            min_x = min_x.min(v.x);
            max_x = max_x.max(v.x);
            min_y = min_y.min(v.y);
            max_y = max_y.max(v.y);
        }

        // Clamp to screen bounds
        let w = self.width as f32;
        let h = self.height as f32;
        let x0 = min_x.max(0.0).min(w - 1.0) as usize;
        let x1 = max_x.max(0.0).min(w - 1.0) as usize;
        let y0 = min_y.max(0.0).min(h - 1.0) as usize;
        let y1 = max_y.max(0.0).min(h - 1.0) as usize;

        self.depth_buffer
            .is_occluded_hiz(x0, y0, x1, y1, poly_max_depth)
    }

    /// Render a surface polygon
    fn render_surface_polygon(
        &mut self,
        polygon: &SurfacePolygon,
        bsp3d: &BSP3D,
        sectors: &[Sector],
        pic_data: &mut PicData,
        player_light: usize,
        buffer: &mut impl DrawBuffer,
    ) {
        self.screen_vertices_len = 0;
        self.tex_coords_len = 0;
        self.inv_w_len = 0;
        self.clipped_vertices_len = 0;

        // Transform vertices to clip space and setup for clipping
        let mut input_vertices = [Vec4::ZERO; 3];
        let mut input_tex_coords = [Vec3::ZERO; 3];

        for (i, &vertex_idx) in polygon.vertices.iter().enumerate() {
            let (_, clip_pos) = self.get_transformed_vertex(vertex_idx, bsp3d);
            let vertex = bsp3d.vertex_get(vertex_idx);
            let (u, v) = self.calculate_tex_coords(vertex, &polygon, bsp3d, pic_data);

            input_vertices[i] = clip_pos;
            input_tex_coords[i] = Vec3::new(u, v, clip_pos.w);
        }

        // Apply Sutherland-Hodgman clipping against all six frustum planes
        self.clip_polygon_frustum(&input_vertices, &input_tex_coords, 3);

        // Project clipped vertices to screen space
        for i in 0..self.clipped_vertices_len {
            let clip_pos = self.clipped_vertices_buffer[i];
            let tex_coord = self.clipped_tex_coords_buffer[i];

            if clip_pos.w > 0.0 {
                let inv_w = 1.0 / clip_pos.w;
                let ndc = clip_pos * inv_w;
                let mut screen_x = (ndc.x + 1.0) * 0.5 * self.width as f32;
                let mut screen_y = (1.0 - ndc.y) * 0.5 * self.height as f32;

                // Snap screen coordinates that are very close to screen boundaries
                // to exact boundary values. Frustum clipping guarantees vertices lie
                // on boundary planes, but the division by w during projection can
                // reintroduce tiny FP drift (e.g. 0.0001). Without snapping, the
                // scanline rasteriser's fill rule and ceil() rounding skip the
                // boundary row/column, producing a 1px gap at screen edges.
                let w_f32 = self.width as f32;
                let h_f32 = self.height as f32;
                const SNAP: f32 = 0.01;
                if screen_x.abs() < SNAP {
                    screen_x = 0.0;
                } else if (screen_x - w_f32).abs() < SNAP {
                    screen_x = w_f32;
                }
                if screen_y.abs() < SNAP {
                    screen_y = 0.0;
                } else if (screen_y - h_f32).abs() < SNAP {
                    screen_y = h_f32;
                }

                self.screen_vertices_buffer[self.screen_vertices_len] =
                    Vec2::new(screen_x, screen_y);
                self.tex_coords_buffer[self.tex_coords_len] =
                    Vec2::new(tex_coord.x * inv_w, tex_coord.y * inv_w);
                self.inv_w_buffer[self.inv_w_len] = inv_w;

                self.screen_vertices_len += 1;
                self.tex_coords_len += 1;
                self.inv_w_len += 1;
            }
        }

        if self.screen_vertices_len < 3 {
            self.polygons_frustum_clipped_count += 1;
            return;
        }

        if self.should_cull_polygon_area(&self.screen_vertices_buffer[..self.screen_vertices_len]) {
            self.polygons_early_culled_count += 1;
            return;
        }

        // Hi-Z depth rejection: skip polygons entirely behind fully-covered tiles
        if self.is_polygon_depth_occluded() {
            self.polygons_depth_rejected_count += 1;
            return;
        }

        let brightness = ((sectors[polygon.sector_id].lightlevel >> 4) + player_light).min(15);

        // Render the polygon: dispatch to debug path only when debug options
        // are active. The fast path has zero debug branches in the inner loop.
        // Wireframe mode skips fill entirely — outlines are drawn as a post-pass.
        if self.debug_draw.wireframe {
            // no fill — outline only
        } else if self.has_debug_draw {
            self.draw_polygon_debug(polygon, brightness, pic_data, buffer);
        } else {
            self.draw_polygon(polygon, brightness, pic_data, buffer);
        }

        if self.debug_draw.outline || self.debug_draw.wireframe {
            let verts = self.screen_vertices_buffer[..self.screen_vertices_len].to_vec();
            let depths = self.inv_w_buffer[..self.inv_w_len].to_vec();
            let color = Self::generate_pseudo_random_colour(
                polygon.sector_id as u32,
                sectors[polygon.sector_id].lightlevel,
            );
            self.debug_polygon_outlines.push((verts, depths, color));
        }

        if self.debug_draw.normals {
            // Compute world-space polygon center
            let mut center = Vec3::ZERO;
            for &vi in &polygon.vertices {
                center += bsp3d.vertex_get(vi);
            }
            center /= polygon.vertices.len() as f32;

            let normal_len = 12.0;
            let tip = center + polygon.normal * normal_len;

            // Project both points to screen
            let vp = self.projection_matrix * self.view_matrix;
            let c_clip = vp * Vec4::new(center.x, center.y, center.z, 1.0);
            let t_clip = vp * Vec4::new(tip.x, tip.y, tip.z, 1.0);

            if c_clip.w > 0.0 && t_clip.w > 0.0 {
                let w = self.width as f32;
                let h = self.height as f32;
                let c_screen = Vec2::new(
                    (c_clip.x / c_clip.w + 1.0) * 0.5 * w,
                    (1.0 - c_clip.y / c_clip.w) * 0.5 * h,
                );
                let t_screen = Vec2::new(
                    (t_clip.x / t_clip.w + 1.0) * 0.5 * w,
                    (1.0 - t_clip.y / t_clip.w) * 0.5 * h,
                );
                let depth = 1.0 / c_clip.w;
                self.debug_normal_lines.push((c_screen, t_screen, depth));
            }
        }
    }

    fn clip_polygon_frustum(
        &mut self,
        vertices: &[Vec4],
        tex_coords: &[Vec3],
        vertex_count: usize,
    ) {
        // Copy input to working buffer
        for i in 0..vertex_count {
            self.clipped_vertices_buffer[i] = vertices[i];
            self.clipped_tex_coords_buffer[i] = tex_coords[i];
        }
        self.clipped_vertices_len = vertex_count;

        // Clip against each frustum plane using Sutherland-Hodgman algorithm
        let frustum_planes = [
            // Left: x >= -w
            (Vec4::new(1.0, 0.0, 0.0, 1.0)),
            // Right: x <= w
            (Vec4::new(-1.0, 0.0, 0.0, 1.0)),
            // Bottom: y >= -w
            (Vec4::new(0.0, 1.0, 0.0, 1.0)),
            // Top: y <= w
            (Vec4::new(0.0, -1.0, 0.0, 1.0)),
            // Near: z >= -w
            (Vec4::new(0.0, 0.0, 1.0, 1.0)),
            // Far: z <= w
            (Vec4::new(0.0, 0.0, -1.0, 1.0)),
        ];

        for plane in frustum_planes {
            if self.clipped_vertices_len == 0 {
                break;
            }
            self.clip_polygon_against_plane(plane);
        }
    }

    fn clip_polygon_against_plane(&mut self, plane: Vec4) {
        if self.clipped_vertices_len < 3 {
            return;
        }

        let mut output_vertices = [Vec4::ZERO; MAX_CLIPPED_VERTICES];
        let mut output_tex_coords = [Vec3::ZERO; MAX_CLIPPED_VERTICES];
        let mut output_count = 0;

        let mut prev_vertex = self.clipped_vertices_buffer[self.clipped_vertices_len - 1];
        let mut prev_tex = self.clipped_tex_coords_buffer[self.clipped_vertices_len - 1];
        let mut prev_inside = plane.dot(prev_vertex) >= 0.0;

        for i in 0..self.clipped_vertices_len {
            let current_vertex = self.clipped_vertices_buffer[i];
            let current_tex = self.clipped_tex_coords_buffer[i];
            let current_inside = plane.dot(current_vertex) >= 0.0;

            if current_inside {
                if !prev_inside {
                    // Entering: add intersection point
                    let prev_distance = plane.dot(prev_vertex);
                    let current_distance = plane.dot(current_vertex);
                    let t = prev_distance / (prev_distance - current_distance);
                    if output_count < MAX_CLIPPED_VERTICES {
                        let v = prev_vertex + (current_vertex - prev_vertex) * t;
                        output_vertices[output_count] = v;
                        output_tex_coords[output_count] = prev_tex + (current_tex - prev_tex) * t;
                        output_count += 1;
                    }
                }
                // Add current vertex (it's inside)
                if output_count < MAX_CLIPPED_VERTICES {
                    output_vertices[output_count] = current_vertex;
                    output_tex_coords[output_count] = current_tex;
                    output_count += 1;
                }
            } else if prev_inside {
                // Exiting: add intersection point
                let prev_distance = plane.dot(prev_vertex);
                let current_distance = plane.dot(current_vertex);
                let t = prev_distance / (prev_distance - current_distance);
                if output_count < MAX_CLIPPED_VERTICES {
                    let v = prev_vertex + (current_vertex - prev_vertex) * t;
                    output_vertices[output_count] = v;
                    output_tex_coords[output_count] = prev_tex + (current_tex - prev_tex) * t;
                    output_count += 1;
                }
            }

            prev_vertex = current_vertex;
            prev_tex = current_tex;
            prev_inside = current_inside;
        }

        // Copy results back to working buffer
        for i in 0..output_count.min(MAX_CLIPPED_VERTICES) {
            self.clipped_vertices_buffer[i] = output_vertices[i];
            self.clipped_tex_coords_buffer[i] = output_tex_coords[i];
        }
        self.clipped_vertices_len = output_count.min(MAX_CLIPPED_VERTICES);
    }

    fn calculate_tex_coords(
        &self,
        world_pos: Vec3,
        surface: &SurfacePolygon,
        bsp3d: &BSP3D,
        pic_data: &PicData,
    ) -> (f32, f32) {
        if surface.vertices.len() < 2 {
            return (0.0, 0.0);
        }

        match &surface.surface_kind {
            SurfaceKind::Vertical {
                texture: Some(tex_id),
                tex_x_offset,
                tex_y_offset,
                texture_direction,
                wall_tex_pin,
                wall_type,
                front_ceiling_z,
                ..
            } => {
                let texture = pic_data.get_texture(*tex_id);
                let tex_width = texture.width as f32;
                let tex_height = texture.height as f32;

                let v1 = bsp3d.vertex_get(surface.vertices[0]);
                let pos_from_start = world_pos - v1;
                let u =
                    pos_from_start.x * texture_direction.x + pos_from_start.y * texture_direction.y;

                let (wall_bottom_z, wall_top_z) = surface.vertices.iter().fold(
                    (f32::INFINITY, f32::NEG_INFINITY),
                    |(min_z, max_z), v| {
                        let z = bsp3d.vertex_get(*v).z;
                        (min_z.min(z), max_z.max(z))
                    },
                );

                let unpeg_condition = match wall_type {
                    WallType::Upper => {
                        matches!(wall_tex_pin, WallTexPin::UnpegTop | WallTexPin::UnpegBoth)
                    }
                    WallType::Middle => !matches!(
                        wall_tex_pin,
                        WallTexPin::UnpegBottom | WallTexPin::UnpegBoth
                    ),
                    WallType::Lower => matches!(
                        wall_tex_pin,
                        WallTexPin::UnpegBottom | WallTexPin::UnpegBoth
                    ),
                };

                let anchor_z = if unpeg_condition {
                    match wall_type {
                        // Middle walls anchor at the polygon's actual top, which
                        // for two-sided walls is min(front_ceil, back_ceil), not
                        // always front_ceiling_z.
                        WallType::Middle => wall_top_z,
                        _ => *front_ceiling_z,
                    }
                } else {
                    match wall_type {
                        WallType::Upper | WallType::Middle => wall_bottom_z + tex_height,
                        WallType::Lower => wall_top_z,
                    }
                };

                let v = -world_pos.z + anchor_z;

                (
                    (u + tex_x_offset) / tex_width,
                    (v + tex_y_offset) / tex_height,
                )
            }
            SurfaceKind::Horizontal {
                texture,
                tex_cos,
                tex_sin,
            } => {
                let flat = pic_data.get_flat(*texture);
                let tex_width = flat.width as f32;
                let tex_height = flat.height as f32;

                // Step 1: Use world coordinates as base (always vary properly)
                let world_u = world_pos.x;
                let world_v = world_pos.y;

                // Step 2: Apply texture direction transformation
                let final_u = world_u * tex_cos - world_v * tex_sin;
                let final_v = world_u * tex_sin + world_v * tex_cos;

                (final_u / tex_width, final_v / tex_height)
            }

            SurfaceKind::Vertical { texture: None, .. } => (0.0, 0.0),
        }
    }

    pub fn draw_view(
        &mut self,
        player: &Player,
        level: &Level,
        pic_data: &mut PicData,
        buffer: &mut impl DrawBuffer,
    ) {
        self.prepare_vertex_cache(&level.map_data.bsp_3d);
        self.current_frame_id = self.current_frame_id.wrapping_add(1);
        #[cfg(feature = "hprof")]
        profile!("render_player_view");
        let MapData {
            sectors,
            subsectors,
            bsp_3d,
            pvs,
            ..
        } = &level.map_data;

        self.update_view_matrix(player);

        let clear = if self.debug_draw.wireframe && self.debug_draw.clear_colour.is_none() {
            Some([30, 30, 30, 255])
        } else {
            self.debug_draw.clear_colour
        };
        if let Some(colour) = clear {
            let buf = buffer.buf_mut();
            for pixel in buf.chunks_exact_mut(4) {
                pixel.copy_from_slice(&colour);
            }
        }

        self.polygons_submitted_count = 0;
        self.polygons_frustum_clipped_count = 0;
        self.polygons_no_draw_count = 0;
        self.polygons_early_culled_count = 0;
        self.polygons_rendered_count = 0;
        self.polygons_depth_rejected_count = 0;
        self.depth_buffer.reset();
        self.debug_polygon_outlines.clear();
        self.debug_normal_lines.clear();

        let player_pos = if let Some(mobj) = player.mobj() {
            Vec3::new(mobj.xy.x, mobj.xy.y, mobj.z + player.viewheight)
        } else {
            return; // No player object, can't render
        };

        let player_sector = player.mobj().unwrap().subsector.clone();
        if let Some(player_subsector_id) = self.find_player_subsector_id(subsectors, &player_sector)
        {
            // Two-pass rendering: collect all visible polygons, then sort and render
            let mut visible_polygons = Vec::new();
            let mut visible_sectors: Vec<(usize, usize)> = Vec::new();
            let mut seen_sectors = vec![false; sectors.len()];

            if pvs.is_visible(player_subsector_id, player_subsector_id) {
                // Use PVS + hierarchical BSP traversal
                self.collect_pvs_visible_polygons(
                    bsp_3d.root_node(),
                    bsp_3d,
                    pvs,
                    sectors,
                    player_pos,
                    player_subsector_id,
                    &mut visible_polygons,
                    &mut visible_sectors,
                    &mut seen_sectors,
                );
            } else {
                // Use BSP traversal for collection
                let root_node = bsp_3d.root_node();
                self.collect_visible_polygons(
                    root_node,
                    bsp_3d,
                    pvs,
                    sectors,
                    player_pos,
                    player_subsector_id,
                    player.extralight,
                    pic_data,
                    &mut visible_polygons,
                );
                // Collect visible sectors from the polygons found
                for (poly, _) in &visible_polygons {
                    let sid = poly.sector_id;
                    if !seen_sectors[sid] {
                        seen_sectors[sid] = true;
                        visible_sectors.push((sid, sectors[sid].lightlevel >> 4));
                    }
                }
            }

            // Sort polygons front-to-back for optimal Z-rejection (larger 1/w is closer)
            visible_polygons
                .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

            // Render all polygons in optimal depth order
            self.polygons_submitted_count = visible_polygons.len() as u32;
            for (poly_surface, _) in visible_polygons {
                self.render_surface_polygon(
                    &poly_surface,
                    bsp_3d,
                    sectors,
                    pic_data,
                    player.extralight,
                    buffer,
                );

                if self.depth_buffer.is_full() {
                    break;
                }
            }

            // Draw sprites after all geometry
            self.draw_sprites(&visible_sectors, sectors, player, pic_data, buffer);

            // Draw player weapon overlay on top of everything
            self.draw_player_weapons(player, pic_data, buffer);

            // Debug: draw polygon outlines / wireframe as post-render overlay
            if self.debug_draw.outline || self.debug_draw.wireframe {
                self.draw_debug_polygon_outlines(buffer);
            }

            // Debug: draw normal direction lines
            if self.debug_draw.normals {
                self.draw_debug_normal_lines(buffer);
            }

            #[cfg(feature = "render_stats")]
            if self.render_stats_last_print.elapsed().as_secs_f32() >= 1.0 {
                println!(
                    "polys: {} submitted, {} frustum-clipped, {} culled, {} early-depth, {} no-draw, {} rendered",
                    self.polygons_submitted_count,
                    self.polygons_frustum_clipped_count,
                    self.polygons_early_culled_count,
                    self.polygons_depth_rejected_count,
                    self.polygons_no_draw_count,
                    self.polygons_rendered_count,
                );
                self.render_stats_last_print = std::time::Instant::now();
            }
        }
    }

    /// Find the subsector ID that matches the given player subsector
    fn find_player_subsector_id(
        &self,
        subsectors: &[SubSector],
        player_sector: &SubSector,
    ) -> Option<usize> {
        for (i, subsector) in subsectors.iter().enumerate() {
            if *subsector == *player_sector {
                return Some(i);
            }
        }
        None
    }

    /// Collect all visible polygons with their depths for global sorting
    fn collect_visible_polygons<'a>(
        &mut self,
        node_id: u32,
        bsp3d: &'a BSP3D,
        pvs: &PVS,
        sectors: &[Sector],
        player_pos: Vec3,
        player_subsector_id: usize,
        player_light: usize,
        pic_data: &mut PicData,
        polygons: &mut Vec<(&'a SurfacePolygon, f32)>,
    ) {
        if node_id & IS_SSECTOR_MASK != 0 {
            // It's a subsector
            let subsector_id = if node_id == u32::MAX {
                0
            } else {
                (node_id & !IS_SSECTOR_MASK) as usize
            };

            if let Some(leaf) = bsp3d.get_subsector_leaf(subsector_id) {
                if self.is_bbox_outside_fov(&leaf.aabb) {
                    return;
                }
                for poly_surface in &leaf.polygons {
                    if poly_surface.is_facing_point(player_pos, &bsp3d.vertices) {
                        if !self.cull_polygon_bounds(&poly_surface, bsp3d) {
                            let depth = self.calculate_polygon_depth(poly_surface, bsp3d);
                            polygons.push((poly_surface, depth));
                        }
                    }
                }
            }
            return;
        }

        // It's a node
        let Some(node) = bsp3d.nodes().get(node_id as usize) else {
            return;
        };
        let side = node.point_on_side(Vec2::new(player_pos.x, player_pos.y));

        // Collect from front side first (closer to player)
        self.collect_visible_polygons(
            node.children[side],
            bsp3d,
            pvs,
            sectors,
            player_pos,
            player_subsector_id,
            player_light,
            pic_data,
            polygons,
        );

        // Collect from back side with 3D frustum check using computed AABB
        let back_child_id = node.children[side ^ 1];
        if let Some(back_aabb) = bsp3d.get_node_aabb(back_child_id) {
            if !self.is_bbox_outside_fov(back_aabb) {
                self.collect_visible_polygons(
                    back_child_id,
                    bsp3d,
                    pvs,
                    sectors,
                    player_pos,
                    player_subsector_id,
                    player_light,
                    pic_data,
                    polygons,
                );
            }
        }
    }

    /// Calculate closest depth of polygon vertices using 1/w convention
    fn calculate_polygon_depth(&mut self, polygon: &SurfacePolygon, bsp3d: &BSP3D) -> f32 {
        let mut max_inv_w = 0.0;

        for &vertex_idx in &polygon.vertices {
            let (_, clip_pos) = self.get_transformed_vertex(vertex_idx, bsp3d);

            if clip_pos.w > 0.0 {
                let inv_w = 1.0 / clip_pos.w;
                if inv_w > max_inv_w {
                    max_inv_w = inv_w;
                }
            }
        }

        max_inv_w
    }

    /// Collect visible polygons using PVS + hierarchical BSP node AABB culling.
    /// Walks the BSP tree, skipping subtrees whose AABB is outside the camera
    /// frustum. At leaf nodes, checks PVS visibility before processing
    /// polygons.
    fn collect_pvs_visible_polygons<'a>(
        &mut self,
        node_id: u32,
        bsp3d: &'a BSP3D,
        pvs: &PVS,
        sectors: &[Sector],
        player_pos: Vec3,
        player_subsector_id: usize,
        polygons: &mut Vec<(&'a SurfacePolygon, f32)>,
        visible_sectors: &mut Vec<(usize, usize)>,
        seen_sectors: &mut Vec<bool>,
    ) {
        if node_id & IS_SSECTOR_MASK != 0 {
            // Leaf: subsector
            let subsector_id = if node_id == u32::MAX {
                0
            } else {
                (node_id & !IS_SSECTOR_MASK) as usize
            };

            // PVS check
            if !pvs.is_visible(player_subsector_id, subsector_id) {
                return;
            }

            let Some(leaf) = bsp3d.get_subsector_leaf(subsector_id) else {
                return;
            };
            if self.is_bbox_outside_fov(&leaf.aabb) {
                return;
            }

            for poly_surface in &leaf.polygons {
                let sid = poly_surface.sector_id;
                if !seen_sectors[sid] {
                    seen_sectors[sid] = true;
                    visible_sectors.push((sid, sectors[sid].lightlevel >> 4));
                }
                if poly_surface.is_facing_point(player_pos, &bsp3d.vertices) {
                    if !self.cull_polygon_bounds(&poly_surface, bsp3d) {
                        let depth = self.calculate_polygon_depth(poly_surface, bsp3d);
                        polygons.push((poly_surface, depth));
                    }
                }
            }
            return;
        }

        // Internal node: check AABB then recurse both children
        let Some(node) = bsp3d.nodes().get(node_id as usize) else {
            return;
        };

        // Check node AABB against camera frustum
        if self.is_bbox_outside_fov(&node.aabb) {
            return;
        }

        // Visit front side first (closer to player)
        let side = node.point_on_side(Vec2::new(player_pos.x, player_pos.y));
        self.collect_pvs_visible_polygons(
            node.children[side],
            bsp3d,
            pvs,
            sectors,
            player_pos,
            player_subsector_id,
            polygons,
            visible_sectors,
            seen_sectors,
        );
        self.collect_pvs_visible_polygons(
            node.children[side ^ 1],
            bsp3d,
            pvs,
            sectors,
            player_pos,
            player_subsector_id,
            polygons,
            visible_sectors,
            seen_sectors,
        );
    }
}
