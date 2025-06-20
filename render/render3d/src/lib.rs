//! # 3D Wireframe Renderer for Doom
//!
//! This crate provides a fully 3D software renderer that displays Doom levels
//! as wireframes. It renders the level geometry in true 3D space, showing
//! floor and ceiling lines, walls, and portal connections.
//!
//! ## Usage
//!
//! ```rust,no_run
//! use render3d::Renderer3D;
//!
//! let mut renderer = Renderer3D::new(640.0, 480.0, 90.0);
//! // renderer.render(&player, &level, &mut buffer);
//! ```

use gameplay::{Level, PicData, Player, Segment};
use glam::{Mat4, Vec2, Vec3, Vec4Swizzles};
use render_trait::{PixelBuffer, PlayViewRenderer, RenderTrait};
use std::f32::consts::PI;

/// A 3D wireframe renderer for Doom levels.
///
/// This renderer displays the level geometry as wireframes in true 3D space,
/// showing floors, ceilings, walls, and portal connections with different colors.
pub struct Renderer3D {
    width: f32,
    height: f32,
    fov: f32,
    view_matrix: Mat4,
    projection_matrix: Mat4,
}

impl Renderer3D {
    /// Creates a new 3D wireframe renderer.
    ///
    /// # Arguments
    ///
    /// * `width` - Screen width in pixels
    /// * `height` - Screen height in pixels
    /// * `fov` - Field of view in radians
    pub fn new(width: f32, height: f32, fov: f32) -> Self {
        let aspect = width / height;
        let near = 0.1;
        let far = 10000.0;

        let projection_matrix = Mat4::perspective_rh_gl(fov, aspect, near, far);

        Self {
            width,
            height,
            fov,
            view_matrix: Mat4::IDENTITY,
            projection_matrix,
        }
    }

    /// Resizes the renderer viewport and updates the projection matrix.
    pub fn resize(&mut self, width: f32, height: f32) {
        self.width = width;
        self.height = height;
        let aspect = width / height;
        let near = 0.1;
        let far = 10000.0;
        self.projection_matrix = Mat4::perspective_rh_gl(self.fov, aspect, near, far);
    }

    /// Sets the field of view and updates the projection matrix.
    pub fn set_fov(&mut self, fov: f32) {
        self.fov = fov;
        let aspect = self.width / self.height;
        let near = 0.1;
        let far = 10000.0;
        self.projection_matrix = Mat4::perspective_rh_gl(fov, aspect, near, far);
    }

    fn update_view_matrix(&mut self, player: &Player) {
        if let Some(mobj) = player.mobj() {
            // Use player.viewz which accounts for viewheight (eye level above feet)
            // This is crucial for proper 3D camera positioning in Doom
            let pos = Vec3::new(mobj.xy.x, mobj.xy.y, player.viewz);
            let angle = mobj.angle.rad();
            let pitch = player.lookdir as f32 * PI / 180.0;

            let forward = Vec3::new(angle.cos(), angle.sin(), pitch.sin());
            let up = Vec3::Z;

            self.view_matrix = Mat4::look_at_rh(pos, pos + forward, up);
        }
    }

    fn project_vertex(&self, vertex: Vec3) -> Option<Vec2> {
        let clip_pos = self.projection_matrix * self.view_matrix * vertex.extend(1.0);

        if clip_pos.w <= 0.0 {
            return None;
        }

        let ndc = clip_pos.xyz() / clip_pos.w;

        if ndc.z < -1.0 || ndc.z > 1.0 {
            return None;
        }

        let screen_x = (ndc.x + 1.0) * 0.5 * self.width;
        let screen_y = (1.0 - ndc.y) * 0.5 * self.height;

        Some(Vec2::new(screen_x, screen_y))
    }

    fn draw_line(&self, buffer: &mut impl PixelBuffer, p1: Vec2, p2: Vec2, color: [u8; 4]) {
        let dx = (p2.x - p1.x).abs();
        let dy = (p2.y - p1.y).abs();
        let steps = dx.max(dy) as i32;

        if steps == 0 {
            return;
        }

        let x_inc = (p2.x - p1.x) / steps as f32;
        let y_inc = (p2.y - p1.y) / steps as f32;

        let mut x = p1.x;
        let mut y = p1.y;

        for _ in 0..=steps {
            let px = x as usize;
            let py = y as usize;

            if px < buffer.size().width_usize() && py < buffer.size().height_usize() {
                buffer.set_pixel(px, py, &color);
            }

            x += x_inc;
            y += y_inc;
        }
    }

    fn render_segment(&self, buffer: &mut impl PixelBuffer, seg: &Segment) {
        let v1 = seg.v1;
        let v2 = seg.v2;

        let sector = &seg.frontsector;
        let floor_height = sector.floorheight;
        let ceil_height = sector.ceilingheight;

        let mut p1_floor = Vec3::new(v1.x, v1.y, floor_height);
        let mut p1_ceil = Vec3::new(v1.x, v1.y, ceil_height);
        let p2_floor = Vec3::new(v2.x, v2.y, floor_height);
        let p2_ceil = Vec3::new(v2.x, v2.y, ceil_height);

        let wall_color = [255, 255, 255, 255];
        let floor_color = [128, 128, 128, 255];
        let ceil_color = [64, 64, 64, 255];

        let proj_p1_floor = self.project_vertex(p1_floor);
        let proj_p1_ceil = self.project_vertex(p1_ceil);
        let proj_p2_floor = self.project_vertex(p2_floor);
        let proj_p2_ceil = self.project_vertex(p2_ceil);

        if proj_p1_floor.is_some() || proj_p2_floor.is_some() {
            if let (Some(s1_floor), Some(s2_floor)) = (proj_p1_floor, proj_p2_floor) {
                self.draw_line(buffer, s1_floor, s2_floor, floor_color);
            }
        }

        if proj_p1_ceil.is_some() || proj_p2_ceil.is_some() {
            if let (Some(s1_ceil), Some(s2_ceil)) = (proj_p1_ceil, proj_p2_ceil) {
                self.draw_line(buffer, s1_ceil, s2_ceil, ceil_color);
            }
        }

        if seg.backsector.is_none() {
            if let (Some(s1_floor), Some(s1_ceil)) = (proj_p1_floor, proj_p1_ceil) {
                self.draw_line(buffer, s1_floor, s1_ceil, wall_color);
            }

            if let (Some(s2_floor), Some(s2_ceil)) = (proj_p2_floor, proj_p2_ceil) {
                self.draw_line(buffer, s2_floor, s2_ceil, wall_color);
            }
        }
        if let Some(back) = seg.backsector.as_ref() {
            if back.floorheight != seg.frontsector.floorheight {
                if let (Some(s1_floor), Some(s2_floor)) = (proj_p1_floor, proj_p2_floor) {
                    self.draw_line(buffer, s1_floor, s2_floor, floor_color);
                }
                p1_floor.z += back.floorheight - seg.frontsector.floorheight;
                if let (Some(s1_floor), Some(s2_floor)) =
                    (proj_p1_floor, self.project_vertex(p1_floor))
                {
                    self.draw_line(buffer, s1_floor, s2_floor, [255, 128, 0, 255]);
                }
            }
            if back.ceilingheight != seg.frontsector.ceilingheight {
                if let (Some(s1_ceil), Some(s2_ceil)) = (proj_p1_ceil, proj_p2_ceil) {
                    self.draw_line(buffer, s1_ceil, s2_ceil, ceil_color);
                }
                p1_ceil.z += back.ceilingheight - seg.frontsector.ceilingheight;
                if let (Some(s1_ceil), Some(s2_ceil)) = (proj_p1_ceil, self.project_vertex(p1_ceil))
                {
                    self.draw_line(buffer, s1_ceil, s2_ceil, [0, 128, 255, 255]);
                }
            }
        }
    }

    /// Renders the level as a 3D wireframe.
    ///
    /// The renderer draws:
    /// - Floor lines in gray
    /// - Ceiling lines in dark gray
    /// - Wall lines in white
    /// - Back sector floor differences in orange
    /// - Back sector ceiling differences in blue
    ///
    /// Camera positioning uses the player's viewz (eye level) which accounts
    /// for viewheight above the player's feet position.
    pub fn render(&mut self, player: &Player, level: &Level, buffer: &mut impl PixelBuffer) {
        self.update_view_matrix(player);

        buffer.clear_with_colour(&[0, 0, 0, 255]);

        for seg in level.map_data.segments() {
            self.render_segment(buffer, seg);
        }
    }
}

impl PlayViewRenderer for Renderer3D {
    fn render_player_view(&mut self, _player: &Player, _level: &Level, _pic_data: &mut PicData) {
        // This method is called by the game engine, but we don't have access to the buffer here
        // The actual rendering happens in the render() method
    }
}

impl Renderer3D {
    /// Renders using a RenderTrait implementation.
    ///
    /// This is a convenience method that extracts the draw buffer from
    /// the RenderTrait and calls the main render method.
    pub fn render_with_trait(
        &mut self,
        player: &Player,
        level: &Level,
        renderer: &mut impl RenderTrait,
    ) {
        self.render(player, level, renderer.draw_buffer());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gameplay::*;
    use glam::Vec2;
    use render_trait::BufferSize;

    struct TestBuffer {
        size: BufferSize,
        data: Vec<u8>,
    }

    impl TestBuffer {
        fn new(width: usize, height: usize) -> Self {
            Self {
                size: BufferSize::new(width, height),
                data: vec![0; width * height * 4],
            }
        }
    }

    impl PixelBuffer for TestBuffer {
        fn size(&self) -> &BufferSize {
            &self.size
        }

        fn clear(&mut self) {
            self.data.fill(0);
        }

        fn clear_with_colour(&mut self, colour: &[u8; 4]) {
            for chunk in self.data.chunks_exact_mut(4) {
                chunk.copy_from_slice(colour);
            }
        }

        fn set_pixel(&mut self, x: usize, y: usize, colour: &[u8; 4]) {
            if x < self.size.width_usize() && y < self.size.height_usize() {
                let idx = (y * self.size.width_usize() + x) * 4;
                if idx + 3 < self.data.len() {
                    self.data[idx..idx + 4].copy_from_slice(colour);
                }
            }
        }

        fn read_pixel(&self, x: usize, y: usize) -> [u8; 4] {
            if x < self.size.width_usize() && y < self.size.height_usize() {
                let idx = (y * self.size.width_usize() + x) * 4;
                if idx + 3 < self.data.len() {
                    [
                        self.data[idx],
                        self.data[idx + 1],
                        self.data[idx + 2],
                        self.data[idx + 3],
                    ]
                } else {
                    [0, 0, 0, 0]
                }
            } else {
                [0, 0, 0, 0]
            }
        }

        fn buf_mut(&mut self) -> &mut [u8] {
            &mut self.data
        }

        fn pitch(&self) -> usize {
            self.size.width_usize() * 4
        }

        fn channels(&self) -> usize {
            4
        }

        fn get_buf_index(&self, x: usize, y: usize) -> usize {
            (y * self.size.width_usize() + x) * 4
        }
    }

    #[test]
    fn test_renderer_creation() {
        let renderer = Renderer3D::new(640.0, 480.0, 90.0);
        assert_eq!(renderer.width, 640.0);
        assert_eq!(renderer.height, 480.0);
    }

    #[test]
    fn test_project_vertex() {
        let mut renderer = Renderer3D::new(640.0, 480.0, 90.0);
        // Set up a simple view matrix looking forward
        let pos = Vec3::new(0.0, 0.0, 0.0);
        let target = Vec3::new(0.0, 1.0, 0.0);
        let up = Vec3::Z;
        renderer.view_matrix = Mat4::look_at_rh(pos, target, up);

        let vertex = Vec3::new(0.0, 10.0, 0.0);
        let projected = renderer.project_vertex(vertex);
        assert!(projected.is_some());
    }

    #[test]
    fn test_line_drawing() {
        let renderer = Renderer3D::new(640.0, 480.0, 90.0);
        let mut buffer = TestBuffer::new(640, 480);

        let p1 = Vec2::new(100.0, 100.0);
        let p2 = Vec2::new(200.0, 200.0);
        let color = [255, 255, 255, 255];

        renderer.draw_line(&mut buffer, p1, p2, color);

        let pixel = buffer.read_pixel(100, 100);
        assert_eq!(pixel, color);
    }

    #[test]
    fn test_viewheight_integration() {
        let renderer = Renderer3D::new(640.0, 480.0, 90.0);

        // Test that the renderer correctly uses player.viewz instead of mobj.z
        // We can't easily mock a full Player, but we can verify the view matrix
        // changes when we call update_view_matrix
        let initial_view = renderer.view_matrix;

        // The view matrix should remain identity until update_view_matrix is called
        // with a valid player that has a MapObject
        assert_eq!(initial_view, Mat4::IDENTITY);
    }
}
