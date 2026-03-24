pub mod clipper;
pub(crate) mod debug;
pub mod depth_buffer;
pub mod interpolation;
pub(crate) mod polygon;
pub(crate) mod sampling;
pub mod voxel;

use depth_buffer::DepthBuffer;
use glam::{Vec2, Vec3, Vec4};

/// Represents a 2D polygon in screen space
#[derive(Debug, Clone)]
pub struct ScreenPoly<'a>(pub &'a [Vec2]);

impl<'a> ScreenPoly<'a> {
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

pub const MAX_CLIPPED_VERTICES: usize = 64;

const LIGHT_MIN_Z: f32 = 0.001;
const LIGHT_MAX_Z: f32 = 0.055;
const LIGHT_RANGE: f32 = 1.0 / (LIGHT_MAX_Z - LIGHT_MIN_Z);
pub const LIGHT_SCALE: f32 = LIGHT_RANGE * 8.0 * 16.0;

pub struct Rasterizer {
    pub(crate) screen_vertices_buffer: [Vec2; MAX_CLIPPED_VERTICES],
    pub(crate) tex_coords_buffer: [Vec2; MAX_CLIPPED_VERTICES],
    pub(crate) inv_w_buffer: [f32; MAX_CLIPPED_VERTICES],
    pub(crate) clipped_vertices_buffer: [Vec4; MAX_CLIPPED_VERTICES],
    pub(crate) clipped_tex_coords_buffer: [Vec3; MAX_CLIPPED_VERTICES],
    pub(crate) screen_vertices_len: usize,
    pub(crate) tex_coords_len: usize,
    pub(crate) inv_w_len: usize,
    pub(crate) clipped_vertices_len: usize,
    pub(crate) depth_buffer: DepthBuffer,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) view_height: u32,
}

impl Rasterizer {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            screen_vertices_buffer: [Vec2::ZERO; MAX_CLIPPED_VERTICES],
            tex_coords_buffer: [Vec2::ZERO; MAX_CLIPPED_VERTICES],
            inv_w_buffer: [0.0; MAX_CLIPPED_VERTICES],
            clipped_vertices_buffer: [Vec4::ZERO; MAX_CLIPPED_VERTICES],
            clipped_tex_coords_buffer: [Vec3::ZERO; MAX_CLIPPED_VERTICES],
            screen_vertices_len: 0,
            tex_coords_len: 0,
            inv_w_len: 0,
            clipped_vertices_len: 0,
            depth_buffer: DepthBuffer::new(width as usize, height as usize),
            width,
            height,
            view_height: height,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        self.view_height = height;
        self.depth_buffer.resize(width as usize, height as usize);
    }

    pub fn depth_buffer(&self) -> &DepthBuffer {
        &self.depth_buffer
    }

    pub fn depth_buffer_mut(&mut self) -> &mut DepthBuffer {
        &mut self.depth_buffer
    }

    /// Frustum clip + project to screen space. Call once per triangle.
    /// `clip_verts`: 3 clip-space vertices. `tex_coords`: 3 (u, v, w) coords.
    /// Returns the number of screen vertices produced (0 if fully clipped).
    pub fn clip_and_project(&mut self, clip_verts: &[Vec4; 3], tex_coords: &[Vec3; 3]) -> usize {
        self.screen_vertices_len = 0;
        self.tex_coords_len = 0;
        self.inv_w_len = 0;
        self.clipped_vertices_len = 0;

        self.clip_polygon_frustum(clip_verts, tex_coords, 3);

        let w_f32 = self.width as f32;
        let vh_f32 = self.view_height as f32;
        let half_w = 0.5 * w_f32;
        let half_h = 0.5 * vh_f32;
        const SNAP: f32 = 0.01;

        for i in 0..self.clipped_vertices_len {
            let clip_pos = self.clipped_vertices_buffer[i];
            let tex_coord = self.clipped_tex_coords_buffer[i];

            if clip_pos.w > 0.0 {
                let inv_w = 1.0 / clip_pos.w;
                let mut screen_x = (clip_pos.x + clip_pos.w) * half_w * inv_w;
                let mut screen_y = (clip_pos.w - clip_pos.y) * half_h * inv_w;
                if screen_x.abs() < SNAP {
                    screen_x = 0.0;
                } else if (screen_x - w_f32).abs() < SNAP {
                    screen_x = w_f32;
                }
                if screen_y.abs() < SNAP {
                    screen_y = 0.0;
                } else if (screen_y - vh_f32).abs() < SNAP {
                    screen_y = vh_f32;
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

        self.screen_vertices_len
    }
}
