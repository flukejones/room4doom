#[cfg(feature = "hprof")]
use coarse_prof::profile;
use gameplay::{
    AABB, BSP3D, Level, MapData, PVS, PicData, Player, Sector, SubSector, SurfaceKind,
    SurfacePolygon, WallTexPin, WallType,
};
use glam::{Mat4, Vec2, Vec3, Vec4};
use render_trait::DrawBuffer;

use std::f32::consts::PI;

mod depth_buffer;
mod render;
#[cfg(test)]
mod tests;

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
}

impl Software3D {
    pub fn new(width: f32, height: f32, fov: f32) -> Self {
        let near = 4.0;
        let far = 10000.0;
        let aspect = width as f32 / height as f32 * 1.33;
        let fov = fov * 0.66;
        let projection_matrix = Mat4::perspective_rh_gl(fov, aspect, near, far);

        Self {
            width: width as u32,
            height: height as u32,
            width_minus_one: width - 1.0,
            height_minus_one: height - 1.0,
            fov,
            view_matrix: Mat4::IDENTITY,
            projection_matrix,
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
        }
    }

    /// Resizes the renderer viewport and updates the projection matrix.
    pub fn resize(&mut self, width: f32, height: f32) {
        self.width = width as u32;
        self.height = height as u32;
        self.width_minus_one = width - 1.0;
        self.height_minus_one = height - 1.0;

        self.set_fov(self.fov);
        self.depth_buffer.resize(width as usize, height as usize);
        self.depth_buffer.set_view_bounds(0.0, width, 0.0, height);
    }

    /// Sets the field of view and updates the projection matrix.
    pub fn set_fov(&mut self, fov: f32) {
        self.fov = fov;
        // let aspect = self.width as f32 / self.height as f32;
        let stretched_height = self.height as f32 * (240.0 / 200.0); // 1.2x vertical
        let aspect = self.width as f32 / stretched_height;
        self.projection_matrix =
            Mat4::perspective_rh_gl(fov * 0.75, aspect, self.near_z, self.far_z);
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

    fn overlap(min_v: f32, max_v: f32, w0: f32, w1: f32) -> bool {
        max_v >= -w0 && min_v <= w0 || max_v >= -w1 && min_v <= w1
    }

    /// Early screen bounds check to reject polygons with all vertices outside frustum
    fn cull_polygon_bounds(&mut self, polygon: &SurfacePolygon, bsp3d: &BSP3D) -> bool {
        let mut all_vertices_outside_left = true;
        let mut all_vertices_outside_right = true;
        let mut all_vertices_outside_top = true;
        let mut all_vertices_outside_bottom = true;

        for i in 0..CLIP_VERTICES_LEN {
            let vidx = unsafe { *polygon.vertices.get_unchecked(i) };
            let (_, clip_pos) = self.get_transformed_vertex(vidx, bsp3d);
            self.clip_vertices[i] = clip_pos;

            if clip_pos.x >= -clip_pos.w {
                all_vertices_outside_left = false;
            }
            if clip_pos.x <= clip_pos.w {
                all_vertices_outside_right = false;
            }
            if clip_pos.y >= -clip_pos.w {
                all_vertices_outside_bottom = false;
            }
            if clip_pos.y <= clip_pos.w {
                all_vertices_outside_top = false;
            }
        }

        if all_vertices_outside_left
            || all_vertices_outside_right
            || all_vertices_outside_top
            || all_vertices_outside_bottom
        {
            return true;
        }

        // Test edges of polygon for intersection with frustum by bounding box overlap check in clip space
        for i in 0..CLIP_VERTICES_LEN {
            let v0 = unsafe { self.clip_vertices.get_unchecked(i) };
            let v1 = unsafe {
                self.clip_vertices
                    .get_unchecked((i + 1) % CLIP_VERTICES_LEN)
            };

            let edge_min_x = v0.x.min(v1.x);
            let edge_max_x = v0.x.max(v1.x);
            let edge_min_y = v0.y.min(v1.y);
            let edge_max_y = v0.y.max(v1.y);
            let edge_min_z = v0.z.min(v1.z);
            let edge_max_z = v0.z.max(v1.z);

            let w0 = v0.w;
            let w1 = v1.w;

            let is_inside_frustum = |v: &Vec4| {
                v.x >= -v.w && v.x <= v.w && v.y >= -v.w && v.y <= v.w && v.z >= -v.w && v.z <= v.w
            };

            if is_inside_frustum(&v0) || is_inside_frustum(&v1) {
                return false;
            }

            let overlap_x = Self::overlap(edge_min_x, edge_max_x, w0, w1);
            let overlap_y = Self::overlap(edge_min_y, edge_max_y, w0, w1);
            let overlap_z = Self::overlap(edge_min_z, edge_max_z, w0, w1);

            if overlap_x && overlap_y && overlap_z {
                return false;
            }
        }

        true
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
                let screen_x = (ndc.x + 1.0) * 0.5 * self.width as f32;
                let screen_y = (1.0 - ndc.y) * 0.5 * self.height as f32;

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

        if self.screen_vertices_len >= 3 {
            if self
                .should_cull_polygon_area(&self.screen_vertices_buffer[..self.screen_vertices_len])
            {
                return;
            }

            // conservative occlusion test: compute screen bbox and polygon's closest 1/w
            let mut min_x = f32::INFINITY;
            let mut max_x = f32::NEG_INFINITY;
            let mut min_y = f32::INFINITY;
            let mut max_y = f32::NEG_INFINITY;
            for i in 0..self.screen_vertices_len {
                let v = self.screen_vertices_buffer[i];
                if v.x < min_x {
                    min_x = v.x
                }
                if v.x > max_x {
                    max_x = v.x
                }
                if v.y < min_y {
                    min_y = v.y
                }
                if v.y > max_y {
                    max_y = v.y
                }
            }

            // polygon closest depth (1/w). Larger = closer.
            let mut poly_max_inv_w = f32::NEG_INFINITY;
            for i in 0..self.inv_w_len {
                let d = self.inv_w_buffer[i];
                if d > poly_max_inv_w {
                    poly_max_inv_w = d
                }
            }

            let bbox_w = (max_x - min_x).max(1.0);
            let bbox_h = (max_y - min_y).max(1.0);
            let bbox_area = bbox_w * bbox_h;

            // sample every N pixels (tune sample_step for perf/accuracy)
            let sample_step = if bbox_area <= 64.0 {
                1usize
            } else if bbox_area <= 512.0 {
                2usize
            } else if bbox_area <= 2048.0 {
                4usize
            } else {
                8usize
            };

            if self.depth_buffer.is_bbox_covered(
                min_x,
                max_x,
                min_y,
                max_y,
                sample_step,
                poly_max_inv_w,
            ) {
                return;
            }

            let brightness = (sectors[polygon.sector_id].lightlevel >> 4) + player_light;

            // Triangulate polygon if it has more than 3 vertices
            if self.screen_vertices_len == 3 {
                // Simple triangle - render directly
                self.draw_polygon(
                    polygon,
                    brightness,
                    pic_data,
                    buffer,
                    #[cfg(feature = "debug_draw")]
                    bsp3d,
                    #[cfg(feature = "debug_draw")]
                    {
                        let ptr = (&sectors[polygon.sector_id] as *const Sector as usize) as u32;
                        Some(self.generate_pseudo_random_colour(
                            ptr,
                            sectors[polygon.sector_id].lightlevel,
                        ))
                    },
                );
            } else {
                // Triangulate polygon using fan triangulation
                for i in 1..(self.screen_vertices_len - 1) {
                    // Create triangle from vertex 0, i, i+1
                    let triangle_vertices = [
                        self.screen_vertices_buffer[0],
                        self.screen_vertices_buffer[i],
                        self.screen_vertices_buffer[i + 1],
                    ];
                    let triangle_tex_coords = [
                        self.tex_coords_buffer[0],
                        self.tex_coords_buffer[i],
                        self.tex_coords_buffer[i + 1],
                    ];
                    let triangle_inv_w = [
                        self.inv_w_buffer[0],
                        self.inv_w_buffer[i],
                        self.inv_w_buffer[i + 1],
                    ];

                    // Temporarily store current buffers
                    let orig_screen_len = self.screen_vertices_len;
                    let orig_tex_len = self.tex_coords_len;
                    let orig_inv_w_len = self.inv_w_len;

                    // Set up triangle data
                    self.screen_vertices_buffer[0] = triangle_vertices[0];
                    self.screen_vertices_buffer[1] = triangle_vertices[1];
                    self.screen_vertices_buffer[2] = triangle_vertices[2];
                    self.tex_coords_buffer[0] = triangle_tex_coords[0];
                    self.tex_coords_buffer[1] = triangle_tex_coords[1];
                    self.tex_coords_buffer[2] = triangle_tex_coords[2];
                    self.inv_w_buffer[0] = triangle_inv_w[0];
                    self.inv_w_buffer[1] = triangle_inv_w[1];
                    self.inv_w_buffer[2] = triangle_inv_w[2];
                    self.screen_vertices_len = 3;
                    self.tex_coords_len = 3;
                    self.inv_w_len = 3;

                    // Render triangle
                    self.draw_polygon(
                        polygon,
                        brightness,
                        pic_data,
                        buffer,
                        #[cfg(feature = "debug_draw")]
                        bsp3d,
                        #[cfg(feature = "debug_draw")]
                        {
                            let ptr =
                                (&sectors[polygon.sector_id] as *const Sector as usize) as u32;
                            Some(self.generate_pseudo_random_colour(
                                ptr,
                                sectors[polygon.sector_id].lightlevel,
                            ))
                        },
                    );

                    // Restore original buffer lengths
                    self.screen_vertices_len = orig_screen_len;
                    self.tex_coords_len = orig_tex_len;
                    self.inv_w_len = orig_inv_w_len;
                }
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
                        output_vertices[output_count] =
                            prev_vertex + (current_vertex - prev_vertex) * t;
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
                    output_vertices[output_count] =
                        prev_vertex + (current_vertex - prev_vertex) * t;
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
                    *front_ceiling_z
                } else {
                    match wall_type {
                        WallType::Upper | WallType::Middle => wall_bottom_z + tex_height,
                        WallType::Lower => wall_top_z + tex_height,
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

        self.depth_buffer.reset();

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

            let vis = pvs.get_visible_subsectors(player_subsector_id);
            if !vis.is_empty() {
                // Use PVS-based collection
                for ss in vis.iter() {
                    let Some(leaf) = bsp_3d.get_subsector_leaf(*ss) else {
                        continue;
                    };
                    if self.is_bbox_outside_fov(&leaf.aabb) {
                        continue;
                    }
                    for poly_surface in &leaf.polygons {
                        if poly_surface.is_facing_point(player_pos, &bsp_3d.vertices) {
                            if !self.cull_polygon_bounds(&poly_surface, bsp_3d) {
                                let depth = self.calculate_polygon_depth(poly_surface, bsp_3d);
                                visible_polygons.push((poly_surface, depth));
                            }
                        }
                    }
                }
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
            }

            // Sort polygons front-to-back for optimal Z-rejection (larger 1/w is closer)
            visible_polygons
                .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

            // Render all polygons in optimal depth order
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

    /// Calculate average depth of polygon vertices using 1/w convention
    fn calculate_polygon_depth(&mut self, polygon: &SurfacePolygon, bsp3d: &BSP3D) -> f32 {
        let mut total_depth = 0.0;
        let mut vertex_count = 0;

        for &vertex_idx in &polygon.vertices {
            let (_, clip_pos) = self.get_transformed_vertex(vertex_idx, bsp3d);

            if clip_pos.w > 0.0 {
                total_depth += 1.0 / clip_pos.w;
                vertex_count += 1;
            }
        }

        if vertex_count > 0 {
            total_depth / vertex_count as f32
        } else {
            0.0
        }
    }
}
