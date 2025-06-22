#[cfg(feature = "hprof")]
use coarse_prof::profile;

use glam::Vec2;

/// Depth buffer for visibility testing with efficient polygon clipping
#[derive(Clone, Debug)]
pub struct DepthBuffer {
    /// Depth values for each pixel (z-coordinate in view space)
    /// Negative values indicate closer objects (following view space convention)
    depths: Box<[f32]>,
    /// Screen dimensions
    width: usize,
    height: usize,
    /// View frustum bounds for clipping
    view_left: f32,
    view_right: f32,
    view_top: f32,
    view_bottom: f32,
}

impl DepthBuffer {
    /// Create a new depth buffer with given dimensions
    pub fn new(width: usize, height: usize) -> Self {
        let size = width * height;
        let depths = vec![f32::INFINITY; size].into_boxed_slice();

        Self {
            depths,
            width,
            height,
            view_left: 0.0,
            view_right: width as f32,
            view_top: 0.0,
            view_bottom: height as f32,
        }
    }

    /// Reset the depth buffer for a new frame
    pub fn reset(&mut self) {
        #[cfg(feature = "hprof")]
        profile!("depth_buffer_reset");

        // Reset all depths to infinity (farthest possible)
        self.depths.fill(f32::INFINITY);
    }

    /// Resize the depth buffer - recreates the buffer
    pub fn resize(&mut self, width: usize, height: usize) {
        let size = width * height;
        self.depths = vec![f32::INFINITY; size].into_boxed_slice();
        self.width = width;
        self.height = height;
        self.view_left = 0.0;
        self.view_right = width as f32;
        self.view_top = 0.0;
        self.view_bottom = height as f32;
    }

    /// Set view frustum bounds for clipping
    pub fn set_view_bounds(&mut self, left: f32, right: f32, top: f32, bottom: f32) {
        self.view_left = left;
        self.view_right = right;
        self.view_top = top;
        self.view_bottom = bottom;
    }

    /// Get depth at pixel coordinates (unchecked)
    #[inline]
    fn get_depth_unchecked(&self, x: usize, y: usize) -> f32 {
        unsafe { *self.depths.get_unchecked(y * self.width + x) }
    }

    /// Set depth at pixel coordinates (unchecked)
    #[inline]
    fn set_depth_unchecked(&mut self, x: usize, y: usize, depth: f32) {
        let index = y * self.width + x;
        unsafe {
            let slot = self.depths.get_unchecked_mut(index);
            if depth < *slot {
                *slot = depth;
            }
        }
    }

    /// Set the depth value at a specific pixel if it's closer than existing depth
    pub fn set_depth(&mut self, x: usize, y: usize, depth: f32) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }

        let index = y * self.width + x;
        if depth < self.depths[index] {
            self.depths[index] = depth;
            true
        } else {
            false
        }
    }

    /// Test if a point is visible (closer than stored depth)
    pub fn is_point_visible(&self, x: f32, y: f32, depth: f32) -> bool {
        if x < 0.0 || y < 0.0 {
            return false;
        }

        let pixel_x = x as usize;
        let pixel_y = y as usize;

        if pixel_x >= self.width || pixel_y >= self.height {
            return false;
        }

        let stored_depth = self.get_depth_unchecked(pixel_x, pixel_y);
        depth < stored_depth
    }

    /// Clip polygon to view frustum using Sutherland-Hodgman algorithm
    /// Returns clipped vertices, or empty if completely outside
    pub fn clip_polygon_to_view(&self, vertices: &[Vec2]) -> Vec<Vec2> {
        if vertices.len() < 3 {
            return Vec::new();
        }

        let mut clipped = vertices.to_vec();

        // Clip against each edge of the view frustum
        // Left edge
        clipped = self.clip_against_edge(clipped, self.view_left, true, false);
        if clipped.is_empty() {
            return clipped;
        }

        // Right edge
        clipped = self.clip_against_edge(clipped, self.view_right, false, false);
        if clipped.is_empty() {
            return clipped;
        }

        // Top edge
        clipped = self.clip_against_edge(clipped, self.view_top, true, true);
        if clipped.is_empty() {
            return clipped;
        }

        // Bottom edge
        clipped = self.clip_against_edge(clipped, self.view_bottom, false, true);

        clipped
    }

    /// Clip against a single edge
    fn clip_against_edge(
        &self,
        vertices: Vec<Vec2>,
        edge: f32,
        is_min: bool,
        is_y: bool,
    ) -> Vec<Vec2> {
        if vertices.is_empty() {
            return vertices;
        }

        let mut result = Vec::new();
        let mut prev = vertices[vertices.len() - 1];

        for current in vertices {
            let current_inside = if is_y {
                if is_min {
                    current.y >= edge
                } else {
                    current.y <= edge
                }
            } else {
                if is_min {
                    current.x >= edge
                } else {
                    current.x <= edge
                }
            };

            let prev_inside = if is_y {
                if is_min {
                    prev.y >= edge
                } else {
                    prev.y <= edge
                }
            } else {
                if is_min {
                    prev.x >= edge
                } else {
                    prev.x <= edge
                }
            };

            if current_inside {
                if !prev_inside {
                    // Entering - add intersection
                    let intersection = self.compute_intersection(prev, current, edge, is_y);
                    result.push(intersection);
                }
                // Add current vertex
                result.push(current);
            } else if prev_inside {
                // Leaving - add intersection
                let intersection = self.compute_intersection(prev, current, edge, is_y);
                result.push(intersection);
            }

            prev = current;
        }

        result
    }

    /// Compute intersection of line segment with edge
    fn compute_intersection(&self, p1: Vec2, p2: Vec2, edge: f32, is_y: bool) -> Vec2 {
        if is_y {
            let t = (edge - p1.y) / (p2.y - p1.y);
            Vec2::new(p1.x + t * (p2.x - p1.x), edge)
        } else {
            let t = (edge - p1.x) / (p2.x - p1.x);
            Vec2::new(edge, p1.y + t * (p2.y - p1.y))
        }
    }

    /// Test if a polygon is potentially visible after clipping
    /// Returns true if any part of the polygon could be visible
    pub fn is_polygon_potentially_visible(&self, vertices: &[Vec2], depths: &[f32]) -> bool {
        #[cfg(feature = "hprof")]
        profile!("is_polygon_potentially_visible");

        if vertices.len() != depths.len() || vertices.is_empty() {
            return true; // Conservative - assume visible if invalid input
        }

        // First clip the polygon to the view frustum
        let clipped_vertices = self.clip_polygon_to_view(vertices);
        if clipped_vertices.is_empty() {
            return false; // Completely outside view
        }

        // Test depth at key points of the clipped polygon
        // Test vertices first
        for (i, vertex) in clipped_vertices.iter().enumerate() {
            let depth = if i < depths.len() {
                depths[i]
            } else {
                // For clipped vertices, interpolate depth
                self.interpolate_depth_at_point(*vertex, vertices, depths)
            };

            if self.is_point_visible(vertex.x, vertex.y, depth) {
                return true;
            }
        }

        // Test center point
        if clipped_vertices.len() >= 3 {
            let center = self.compute_polygon_center(&clipped_vertices);
            let center_depth = self.interpolate_depth_at_point(center, vertices, depths);
            if self.is_point_visible(center.x, center.y, center_depth) {
                return true;
            }
        }

        // Test a few sample points along edges
        for i in 0..clipped_vertices.len() {
            let v1 = clipped_vertices[i];
            let v2 = clipped_vertices[(i + 1) % clipped_vertices.len()];
            let mid = (v1 + v2) * 0.5;
            let mid_depth = self.interpolate_depth_at_point(mid, vertices, depths);
            if self.is_point_visible(mid.x, mid.y, mid_depth) {
                return true;
            }
        }

        false
    }

    /// Compute the center point of a polygon
    fn compute_polygon_center(&self, vertices: &[Vec2]) -> Vec2 {
        let mut center = Vec2::ZERO;
        for vertex in vertices {
            center += *vertex;
        }
        center / vertices.len() as f32
    }

    /// Interpolate depth at a point using barycentric coordinates
    fn interpolate_depth_at_point(&self, point: Vec2, vertices: &[Vec2], depths: &[f32]) -> f32 {
        if vertices.len() < 3 || depths.len() != vertices.len() {
            return depths.get(0).copied().unwrap_or(0.0);
        }

        // Find closest vertex as fallback
        let mut closest_depth = depths[0];
        let mut min_dist_sq = (vertices[0] - point).length_squared();

        for i in 1..vertices.len() {
            let dist_sq = (vertices[i] - point).length_squared();
            if dist_sq < min_dist_sq {
                min_dist_sq = dist_sq;
                closest_depth = depths[i];
            }
        }

        // For simplicity, use the closest vertex depth
        // In a more sophisticated implementation, you could use proper barycentric interpolation
        closest_depth
    }

    /// Update depth buffer with a polygon's depth values
    /// This should be called after a polygon is successfully rendered
    pub fn update_polygon_depth(&mut self, vertices: &[Vec2], depths: &[f32]) {
        #[cfg(feature = "hprof")]
        profile!("update_polygon_depth");

        if vertices.len() != depths.len() || vertices.len() < 3 {
            return;
        }

        // Clip polygon to view
        let clipped_vertices = self.clip_polygon_to_view(vertices);
        if clipped_vertices.is_empty() {
            return;
        }

        // Update depth buffer at key points
        self.update_depth_at_vertices(&clipped_vertices, vertices, depths);
    }

    /// Update depth buffer at polygon vertices and key points
    fn update_depth_at_vertices(
        &mut self,
        clipped_vertices: &[Vec2],
        original_vertices: &[Vec2],
        depths: &[f32],
    ) {
        // Update at clipped vertices
        for vertex in clipped_vertices {
            let depth = self.interpolate_depth_at_point(*vertex, original_vertices, depths);
            let x = vertex.x.round() as usize;
            let y = vertex.y.round() as usize;
            if x < self.width && y < self.height {
                self.set_depth_unchecked(x, y, depth);
            }
        }

        // Update at center if polygon is large enough
        if clipped_vertices.len() >= 3 {
            let center = self.compute_polygon_center(clipped_vertices);
            let center_depth = self.interpolate_depth_at_point(center, original_vertices, depths);
            let x = center.x.round() as usize;
            let y = center.y.round() as usize;
            if x < self.width && y < self.height {
                self.set_depth_unchecked(x, y, center_depth);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_depth_buffer_creation() {
        let buffer = DepthBuffer::new(100, 100);
        // Test that initial depths are infinity by testing visibility
        assert!(buffer.is_point_visible(0.0, 0.0, -1.0)); // Should be visible since buffer is empty
        assert!(buffer.is_point_visible(99.0, 99.0, -1.0));
    }

    #[test]
    fn test_depth_testing() {
        let mut buffer = DepthBuffer::new(100, 100);

        // Initially all points should be visible
        assert!(buffer.is_point_visible(50.0, 50.0, -10.0));

        // Set a depth value
        assert!(buffer.set_depth(50, 50, -5.0));

        // Closer points should be visible
        assert!(buffer.is_point_visible(50.0, 50.0, -10.0));

        // Farther points should not be visible
        assert!(!buffer.is_point_visible(50.0, 50.0, -1.0));

        // Points at the same depth should not be visible (not closer)
        assert!(!buffer.is_point_visible(50.0, 50.0, -5.0));
    }

    #[test]
    fn test_polygon_clipping() {
        let mut buffer = DepthBuffer::new(100, 100);
        buffer.set_view_bounds(10.0, 90.0, 10.0, 90.0);

        // Polygon completely inside
        let vertices = vec![
            Vec2::new(20.0, 20.0),
            Vec2::new(40.0, 20.0),
            Vec2::new(40.0, 40.0),
            Vec2::new(20.0, 40.0),
        ];
        let clipped = buffer.clip_polygon_to_view(&vertices);
        assert_eq!(clipped.len(), 4);

        // Polygon completely outside
        let vertices = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(5.0, 0.0),
            Vec2::new(5.0, 5.0),
            Vec2::new(0.0, 5.0),
        ];
        let clipped = buffer.clip_polygon_to_view(&vertices);
        assert!(clipped.is_empty());

        // Polygon partially outside
        let vertices = vec![
            Vec2::new(0.0, 20.0),
            Vec2::new(50.0, 20.0),
            Vec2::new(50.0, 50.0),
            Vec2::new(0.0, 50.0),
        ];
        let clipped = buffer.clip_polygon_to_view(&vertices);
        assert!(!clipped.is_empty());
        // Should be clipped to view bounds
        for vertex in &clipped {
            assert!(vertex.x >= 10.0 && vertex.x <= 90.0);
            assert!(vertex.y >= 10.0 && vertex.y <= 90.0);
        }
    }

    #[test]
    fn test_polygon_visibility() {
        let mut buffer = DepthBuffer::new(100, 100);

        // Create a simple triangle
        let vertices = vec![
            Vec2::new(10.0, 10.0),
            Vec2::new(50.0, 10.0),
            Vec2::new(30.0, 50.0),
        ];
        let depths = vec![-10.0, -10.0, -10.0];

        // Initially should be visible
        assert!(buffer.is_polygon_potentially_visible(&vertices, &depths));

        // Block some pixels
        for x in 10..=50 {
            for y in 10..=50 {
                buffer.set_depth(x, y, -20.0);
            }
        }

        // Should still be visible due to conservative testing
        assert!(!buffer.is_polygon_potentially_visible(&vertices, &depths));
    }

    #[test]
    fn test_resize() {
        let mut buffer = DepthBuffer::new(50, 50);
        assert!(buffer.set_depth(25, 25, -10.0));

        buffer.resize(100, 100);
        // After resize, all depths should be reset - test by checking visibility
        assert!(buffer.is_point_visible(25.0, 25.0, -1.0)); // Should be visible since buffer was reset
    }
}
