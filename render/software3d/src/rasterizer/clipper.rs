use glam::{Vec3, Vec4};

use super::{MAX_CLIPPED_VERTICES, Rasterizer};

impl Rasterizer {
    /// Clip a polygon against all 6 frustum planes using Sutherland-Hodgman.
    ///
    /// - Copies input vertices/tex_coords into working buffers
    /// - Clips sequentially against left, right, bottom, top, near, far planes
    /// - Result left in `clipped_vertices_buffer` / `clipped_tex_coords_buffer`
    pub fn clip_polygon_frustum(
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

    /// Clip the working polygon against a single half-space plane.
    /// Vertices on the positive dot-product side are kept; entering/exiting
    /// edges produce interpolated intersection vertices.
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
}
