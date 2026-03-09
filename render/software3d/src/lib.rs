#[cfg(feature = "hprof")]
use coarse_prof::profile;
use gameplay::{
    AABB, BSP3D, Level, MapData, PicData, Player, PvsData, Sector, SubSector, SurfaceKind, SurfacePolygon, WallTexPin, WallType, is_subsector, subsector_index
};
use glam::{Mat4, Vec2, Vec3, Vec4};
use hud_util::{draw_text_line, hud_scale, measure_text_line};
use render_trait::DrawBuffer;

use std::f32::consts::PI;

mod depth_buffer;
mod render;
mod sky;
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

/// Seconds before an unrefreshed debug overlay line is auto-cleared.
const DEBUG_LINE_TIMEOUT_SECS: f32 = 5.0;

// ============================================================================
// SUB-STRUCTS
// ============================================================================

/// Per-frame render counters and stats print rate-limiter.
struct RenderStats {
    /// Polygons in the visible list submitted for rasterisation.
    polygons_submitted: u32,
    /// Polygons that required frustum clipping before rasterisation.
    polygons_frustum_clipped: u32,
    /// Polygons rejected by the early-depth (hi-Z) test.
    polygons_early_culled: u32,
    /// Polygons successfully rasterised to the framebuffer.
    polygons_rendered: u32,
    /// Polygons with no drawable surface (back-face, zero-size, etc.).
    polygons_no_draw: u32,
    /// Polygons rejected after the depth buffer was full.
    polygons_depth_rejected: u32,
    /// BSP leaf nodes reached by the traversal (before PVS test).
    subsectors_total: u32,
    /// Subsectors that passed the PVS visibility test.
    subsectors_pvs_passed: u32,
    /// BSP occlusion fallback events.
    bsp_fallback: u32,
    /// Rate-limiter: only print stats once per second.
    #[cfg(feature = "render_stats")]
    last_print: std::time::Instant,
}

impl RenderStats {
    fn new() -> Self {
        Self {
            polygons_submitted: 0,
            polygons_frustum_clipped: 0,
            polygons_early_culled: 0,
            polygons_rendered: 0,
            polygons_no_draw: 0,
            polygons_depth_rejected: 0,
            subsectors_total: 0,
            subsectors_pvs_passed: 0,
            bsp_fallback: 0,
            #[cfg(feature = "render_stats")]
            last_print: std::time::Instant::now(),
        }
    }

    /// Reset all per-frame counters. Does not reset `last_print`.
    fn reset(&mut self) {
        self.polygons_submitted = 0;
        self.polygons_frustum_clipped = 0;
        self.polygons_early_culled = 0;
        self.polygons_rendered = 0;
        self.polygons_no_draw = 0;
        self.polygons_depth_rejected = 0;
        self.subsectors_total = 0;
        self.subsectors_pvs_passed = 0;
        self.bsp_fallback = 0;
    }
}

/// Sky rendering state. Rebuilt when the sky texture changes.
pub(crate) struct SkyRend {
    /// Sky texture column at screen_x = 0 (wraps into [0, sky_width)).
    pub(crate) x_offset: f32,
    /// Sky texture columns per screen pixel (horizontal pan rate).
    pub(crate) x_step: f32,
    /// Sky texture rows per screen pixel (vertical scale).
    pub(crate) v_scale: f32,
    /// Pitch-based additive offset keeping the sky world-fixed on Y.
    pub(crate) pitch_offset: f32,
    /// Sky texture index last passed to `init_sky`; `usize::MAX` = not built.
    last_pic: usize,
    /// Horizontal FOV in radians, derived from the projection matrix.
    pub(crate) h_fov: f32,
    /// Combined RGBA sky buffer (column-major): original rows + extensions.
    pub(crate) extended: Vec<[u8; 4]>,
    /// Height of the original sky texture.
    pub(crate) tex_height: usize,
    /// Generated rows above the original texture.
    pub(crate) extended_rows: usize,
    /// Generated rows below the original texture.
    pub(crate) down_rows: usize,
}

impl SkyRend {
    fn new() -> Self {
        Self {
            x_offset: 0.0,
            x_step: 0.0,
            v_scale: 0.0,
            pitch_offset: 0.0,
            last_pic: usize::MAX,
            h_fov: 0.0,
            extended: Vec::new(),
            tex_height: 0,
            extended_rows: 0,
            down_rows: 0,
        }
    }
}

/// Debug draw options and per-frame scratch buffers for debug overlays.
struct DebugDraw {
    /// Active debug rendering options (outline, normals, colour mode, etc.).
    options: DebugDrawOptions,
    /// Cache of whether any option is active (avoids repeated `is_active()`
    /// calls).
    has_active: bool,
    /// Per-frame polygon outline scratch (vertices, depths, colour).
    polygon_outlines: Vec<(Vec<Vec2>, Vec<f32>, [u8; 4])>,
    /// Per-frame normal line scratch (screen_center, screen_tip, depth).
    normal_lines: Vec<(Vec2, Vec2, f32)>,
    /// Upper-right overlay text. Empty = hidden.
    line: String,
    /// `Instant` of the last `set_line` call; for `DEBUG_LINE_TIMEOUT_SECS`
    /// auto-clear.
    line_set_at: std::time::Instant,
}

impl DebugDraw {
    fn new(options: DebugDrawOptions) -> Self {
        let has_active = options.is_active();
        Self {
            options,
            has_active,
            polygon_outlines: Vec::new(),
            normal_lines: Vec::new(),
            line: String::new(),
            line_set_at: std::time::Instant::now(),
        }
    }

    /// Replace the debug overlay text and reset the auto-clear timer.
    fn set_line(&mut self, s: String) {
        self.line = s;
        self.line_set_at = std::time::Instant::now();
    }

    /// Return the current line text if within `DEBUG_LINE_TIMEOUT_SECS`;
    /// clears and returns `""` after timeout.
    fn current_line(&mut self) -> &str {
        if !self.line.is_empty()
            && self.line_set_at.elapsed().as_secs_f32() >= DEBUG_LINE_TIMEOUT_SECS
        {
            self.line.clear();
        }
        &self.line
    }
}

// ============================================================================
// MAIN RENDERER
// ============================================================================

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
    // Per-frame traversal state — pre-allocated, reset each frame
    seen_sectors: Vec<bool>,
    visible_sectors: Vec<(usize, usize)>,
    visible_polygons: Vec<(*const SurfacePolygon, f32)>,
    // Sub-structs
    stats: RenderStats,
    sky: SkyRend,
    debug: DebugDraw,
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
            seen_sectors: Vec::new(),
            visible_sectors: Vec::new(),
            visible_polygons: Vec::new(),
            stats: RenderStats::new(),
            sky: SkyRend::new(),
            debug: DebugDraw::new(debug_draw),
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
        // Horizontal FOV: derived from projection x_axis (1/tan(hfov/2))
        self.sky.h_fov = 2.0 * (1.0 / self.projection_matrix.x_axis.x).atan();
    }

    /// One-time sky setup: precompute static scale factors and per-column edge
    /// colours. Called when the sky texture changes (e.g. new map).
    fn init_sky(&mut self, sky_pic: usize, pic_data: &PicData) {
        let sky = pic_data.wall_pic(sky_pic);
        let sky_w = sky.width;
        let sky_h = sky.height;
        let screen_w = self.width as f32;
        let screen_h = self.height as f32;

        // Horizontal step: sky tiles SKY_TILES times per full 360°, columns
        // decrease left-to-right (matches 2.5d screen_to_angle convention).
        self.sky.x_step =
            -(self.sky.h_fov * sky_w as f32 * sky::SKY_TILES) / (screen_w * std::f32::consts::TAU);

        // Vertical scale: texture is SKY_V_STRETCH times taller than the screen.
        self.sky.v_scale = sky_h as f32 / (screen_h * sky::SKY_V_STRETCH);

        // Build combined RGBA buffer: original texture rows + generated extension.
        self.sky.extended = sky::build_sky_combined(
            &sky.data,
            sky_w,
            sky_h,
            pic_data.colourmap(0),
            pic_data.palette(),
        );
        self.sky.tex_height = sky_h;
        self.sky.extended_rows = sky::SKY_EXTEND_ROWS;
        self.sky.down_rows = sky::SKY_DOWN_ROWS;

        self.sky.last_pic = sky_pic;
    }

    /// Per-frame sky update: recompute only the values that depend on player
    /// angle and pitch. Calls `init_sky` first if the sky texture changed.
    fn update_sky_params(&mut self, angle_rad: f32, pitch_rad: f32, pic_data: &PicData) {
        let sky_pic = pic_data.sky_pic();
        if sky_pic != self.sky.last_pic {
            self.init_sky(sky_pic, pic_data);
        }

        let sky_h = pic_data.wall_pic(sky_pic).height as f32;
        let screen_h = self.height as f32;
        let sky_w = pic_data.wall_pic(sky_pic).width as f32;

        // Horizontal offset: left edge of screen = angle + hfov/2 (decreasing
        // rightward).
        self.sky.x_offset =
            (angle_rad + self.sky.h_fov * 0.5) * sky_w * sky::SKY_TILES / std::f32::consts::TAU;

        // Vertical center + pitch offset: sky_h/2 sits at screen center when
        // pitch = 0; positive pitch (looking up) shifts rows toward the zenith.
        let half_h = screen_h * 0.5;
        let sky_center_base = sky_h * 0.5 - half_h * self.sky.v_scale;
        let proj_y = half_h * self.projection_matrix.y_axis.y;
        self.sky.pitch_offset = sky_center_base - pitch_rad * proj_y * self.sky.v_scale;
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
    /// Returns `None` if the polygon should be culled, or `Some(max_inv_w)` if
    /// it passes — with the closest-vertex depth pre-computed for free.
    fn cull_polygon_bounds(&mut self, polygon: &SurfacePolygon, bsp3d: &BSP3D) -> Option<f32> {
        let mut all_outside_left = true;
        let mut all_outside_right = true;
        let mut all_outside_bottom = true;
        let mut all_outside_top = true;
        let mut all_outside_near = true;
        let mut all_outside_far = true;
        let mut max_inv_w: f32 = 0.0;

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
            if clip_pos.w > 0.0 {
                let inv_w = 1.0 / clip_pos.w;
                if inv_w > max_inv_w {
                    max_inv_w = inv_w;
                }
            }
        }

        if all_outside_left
            || all_outside_right
            || all_outside_bottom
            || all_outside_top
            || all_outside_near
            || all_outside_far
        {
            return None;
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
                return None;
            }

            // Hi-Z pre-check: reject if entirely behind existing geometry.
            // Convert NDC-space coords to screen pixels (Y flipped), take vertex
            // AABB (conservative overapproximation), then query the tiled depth buffer.
            let sx0s = sx0 + hw;
            let sy0s = hh - sy0;
            let sx1s = sx1 + hw;
            let sy1s = hh - sy1;
            let sx2s = sx2 + hw;
            let sy2s = hh - sy2;
            let min_x = sx0s.min(sx1s).min(sx2s).max(0.0) as usize;
            let min_y = sy0s.min(sy1s).min(sy2s).max(0.0) as usize;
            let max_x = (sx0s.max(sx1s).max(sx2s) as usize).min(self.width as usize - 1);
            let max_y = (sy0s.max(sy1s).max(sy2s) as usize).min(self.height as usize - 1);
            if self
                .depth_buffer
                .is_occluded_hiz(min_x, min_y, max_x, max_y, max_inv_w)
            {
                return None;
            }
        }

        Some(max_inv_w)
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

        // Pre-compute wall z-range once — used by all 3 vertices in
        // calculate_tex_coords
        let wall_z_range = match &polygon.surface_kind {
            SurfaceKind::Vertical {
                texture: Some(_),
                ..
            } => polygon.vertices.iter().fold(
                (f32::INFINITY, f32::NEG_INFINITY),
                |(min_z, max_z), &v| {
                    let z = bsp3d.vertex_get(v).z;
                    (min_z.min(z), max_z.max(z))
                },
            ),
            _ => (0.0, 0.0),
        };

        for (i, &vertex_idx) in polygon.vertices.iter().enumerate() {
            let (_, clip_pos) = self.get_transformed_vertex(vertex_idx, bsp3d);
            let vertex = bsp3d.vertex_get(vertex_idx);
            let (u, v) = self.calculate_tex_coords(vertex, &polygon, bsp3d, pic_data, wall_z_range);

            input_vertices[i] = clip_pos;
            input_tex_coords[i] = Vec3::new(u, v, clip_pos.w);
        }

        // Apply Sutherland-Hodgman clipping against all six frustum planes
        self.clip_polygon_frustum(&input_vertices, &input_tex_coords, 3);

        // Project clipped vertices to screen space, tracking AABB and max depth inline
        // to avoid rescanning the buffers later.
        let w_f32 = self.width as f32;
        let h_f32 = self.height as f32;
        let mut max_inv_w: f32 = 0.0;
        let mut scr_min_x = f32::MAX;
        let mut scr_min_y = f32::MAX;
        let mut scr_max_x = f32::MIN;
        let mut scr_max_y = f32::MIN;

        for i in 0..self.clipped_vertices_len {
            let clip_pos = self.clipped_vertices_buffer[i];
            let tex_coord = self.clipped_tex_coords_buffer[i];

            if clip_pos.w > 0.0 {
                let inv_w = 1.0 / clip_pos.w;
                if inv_w > max_inv_w {
                    max_inv_w = inv_w;
                }
                let ndc = clip_pos * inv_w;
                let mut screen_x = (ndc.x + 1.0) * 0.5 * w_f32;
                let mut screen_y = (1.0 - ndc.y) * 0.5 * h_f32;

                // Snap screen coordinates that are very close to screen boundaries
                // to exact boundary values. Frustum clipping guarantees vertices lie
                // on boundary planes, but the division by w during projection can
                // reintroduce tiny FP drift (e.g. 0.0001). Without snapping, the
                // scanline rasteriser's fill rule and ceil() rounding skip the
                // boundary row/column, producing a 1px gap at screen edges.
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

                if screen_x < scr_min_x {
                    scr_min_x = screen_x;
                }
                if screen_x > scr_max_x {
                    scr_max_x = screen_x;
                }
                if screen_y < scr_min_y {
                    scr_min_y = screen_y;
                }
                if screen_y > scr_max_y {
                    scr_max_y = screen_y;
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
            self.stats.polygons_frustum_clipped += 1;
            return;
        }

        // Cull sub-pixel polygons using the pre-computed AABB — O(1) vs O(N) shoelace.
        // AABB area overestimates thin slivers, but those would produce no drawn pixels
        // and be caught by the no-draw counter anyway.
        if (scr_max_x - scr_min_x) * (scr_max_y - scr_min_y) < 1.0 {
            self.stats.polygons_early_culled += 1;
            return;
        }

        // Hi-Z depth rejection using pre-computed AABB and max depth — no rescan
        // needed.
        if max_inv_w > 0.0 {
            let x0 = scr_min_x.max(0.0).min(w_f32 - 1.0) as usize;
            let x1 = scr_max_x.max(0.0).min(w_f32 - 1.0) as usize;
            let y0 = scr_min_y.max(0.0).min(h_f32 - 1.0) as usize;
            let y1 = scr_max_y.max(0.0).min(h_f32 - 1.0) as usize;
            if self.depth_buffer.is_occluded_hiz(x0, y0, x1, y1, max_inv_w) {
                self.stats.polygons_depth_rejected += 1;
                return;
            }
        }

        let brightness = ((sectors[polygon.sector_id].lightlevel >> 4) + player_light).min(15);
        let bounds = (
            Vec2::new(scr_min_x, scr_min_y),
            Vec2::new(scr_max_x, scr_max_y),
        );

        // Render the polygon: dispatch to debug path only when debug options
        // are active. The fast path has zero debug branches in the inner loop.
        // Wireframe mode skips fill entirely — outlines are drawn as a post-pass.
        if self.debug.options.wireframe {
            // no fill — outline only
        } else if self.debug.has_active {
            self.draw_polygon_debug(polygon, brightness, bounds, pic_data, buffer);
        } else {
            self.draw_polygon(polygon, brightness, bounds, pic_data, buffer);
        }

        if self.debug.options.outline || self.debug.options.wireframe {
            let verts = self.screen_vertices_buffer[..self.screen_vertices_len].to_vec();
            let depths = self.inv_w_buffer[..self.inv_w_len].to_vec();
            let color = Self::generate_pseudo_random_colour(
                polygon.sector_id as u32,
                sectors[polygon.sector_id].lightlevel,
            );
            self.debug.polygon_outlines.push((verts, depths, color));
        }

        if self.debug.options.normals {
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
                self.debug.normal_lines.push((c_screen, t_screen, depth));
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
        wall_z_range: (f32, f32),
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

                let (wall_bottom_z, wall_top_z) = wall_z_range;

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

            SurfaceKind::Vertical {
                texture: None,
                ..
            } => (0.0, 0.0),
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

        let clear = if self.debug.options.wireframe && self.debug.options.clear_colour.is_none() {
            Some([30, 30, 30, 255])
        } else {
            self.debug.options.clear_colour
        };
        if let Some(colour) = clear {
            let buf = buffer.buf_mut();
            for pixel in buf.chunks_exact_mut(4) {
                pixel.copy_from_slice(&colour);
            }
        }

        self.stats.reset();
        self.depth_buffer.reset();
        self.debug.polygon_outlines.clear();
        self.debug.normal_lines.clear();

        let player_pos = if let Some(mobj) = player.mobj() {
            Vec3::new(mobj.xy.x, mobj.xy.y, mobj.z + player.viewheight)
        } else {
            return; // No player object, can't render
        };

        if let Some(player_subsector_id) = player
            .mobj()
            .and_then(|m| self.find_player_subsector_id(subsectors, &m.subsector))
        {
            let sector_count = sectors.len();
            // Reset pre-allocated per-frame state
            self.seen_sectors.resize(sector_count, false);
            self.seen_sectors.fill(false);
            self.visible_sectors.clear();
            self.visible_polygons.clear();

            let player_angle_rad = player.mobj().unwrap().angle.rad();
            let player_pitch_rad = player.lookdir as f32 * PI / 180.0;
            self.update_sky_params(player_angle_rad, player_pitch_rad, pic_data);

            if pvs.is_visible(player_subsector_id, player_subsector_id) {
                // Use PVS + hierarchical BSP traversal
                self.collect_pvs_visible_polygons(
                    bsp_3d.root_node(),
                    bsp_3d,
                    pvs,
                    sectors,
                    player_pos,
                    player_subsector_id,
                );
            } else {
                // Single-pass: render immediately during BSP traversal (front-to-back),
                // enabling depth_buffer.is_full() early-out and inline sector tracking.
                self.render_visible_polygons(
                    bsp_3d.root_node(),
                    bsp_3d,
                    sectors,
                    player_pos,
                    player.extralight,
                    pic_data,
                    buffer,
                );
            }

            if !self.visible_polygons.is_empty() {
                // Sort polygons front-to-back for optimal Z-rejection (larger 1/w = closer)
                self.visible_polygons
                    .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

                // Render all polygons in optimal depth order
                self.stats.polygons_submitted = self.visible_polygons.len() as u32;
                for i in 0..self.visible_polygons.len() {
                    let poly_surface = unsafe { &*self.visible_polygons[i].0 };
                    self.render_surface_polygon(
                        poly_surface,
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

            // Draw sprites after all geometry
            self.draw_sprites(sectors, player, pic_data, buffer);

            // Draw player weapon overlay on top of everything
            self.draw_player_weapons(player, pic_data, buffer);

            // Debug: draw polygon outlines / wireframe as post-render overlay
            if self.debug.options.outline || self.debug.options.wireframe {
                self.draw_debug_polygon_outlines(buffer);
            }

            // Debug: draw normal direction lines
            if self.debug.options.normals {
                self.draw_debug_normal_lines(buffer);
            }

            self.draw_debug_line(pic_data, buffer);

            #[cfg(feature = "render_stats")]
            if self.stats.last_print.elapsed().as_secs_f32() >= 1.0 {
                println!(
                    "polys: {} submitted, {} frustum-clipped, {} culled, {} early-depth, {} no-draw, {} rendered, {} fallback | subsectors: {} passed / {} total",
                    self.stats.polygons_submitted,
                    self.stats.polygons_frustum_clipped,
                    self.stats.polygons_early_culled,
                    self.stats.polygons_depth_rejected,
                    self.stats.polygons_no_draw,
                    self.stats.polygons_rendered,
                    self.stats.bsp_fallback,
                    self.stats.subsectors_pvs_passed,
                    self.stats.subsectors_total,
                );
                self.stats.last_print = std::time::Instant::now();
            }
        }
    }

    /// Draw the debug overlay text line in the upper-right corner, if set.
    fn draw_debug_line(&mut self, pic_data: &PicData, pixels: &mut impl DrawBuffer) {
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

    /// Headless render entry point for benchmarks. Bypasses Player/Level in
    /// favour of raw camera parameters and a pre-known subsector ID.
    /// Skips sprite and weapon overlay rendering.
    #[cfg(feature = "bench")]
    pub fn draw_view_bench(
        &mut self,
        pos: Vec3,
        angle_rad: f32,
        pitch_rad: f32,
        subsector_id: usize,
        map_data: &MapData,
        pic_data: &mut PicData,
        buffer: &mut impl DrawBuffer,
    ) {
        let MapData {
            sectors,
            subsectors: _,
            bsp_3d,
            pvs,
            ..
        } = map_data;

        self.prepare_vertex_cache(bsp_3d);
        self.current_frame_id = self.current_frame_id.wrapping_add(1);

        let forward = Vec3::new(
            angle_rad.cos() * pitch_rad.cos(),
            angle_rad.sin() * pitch_rad.cos(),
            pitch_rad.sin(),
        );
        self.view_matrix = Mat4::look_at_rh(pos, pos + forward, Vec3::Z);

        self.stats.reset();
        self.depth_buffer.reset();

        self.seen_sectors.resize(sectors.len(), false);
        self.seen_sectors.fill(false);
        self.visible_sectors.clear();
        self.visible_polygons.clear();

        self.update_sky_params(angle_rad, pitch_rad, pic_data);
        if pvs.is_visible(subsector_id, subsector_id) {
            self.collect_pvs_visible_polygons(
                bsp_3d.root_node(),
                bsp_3d,
                pvs,
                sectors,
                pos,
                subsector_id,
            );
        } else {
            self.render_visible_polygons(
                bsp_3d.root_node(),
                bsp_3d,
                sectors,
                pos,
                0,
                pic_data,
                buffer,
            );
        }

        if !self.visible_polygons.is_empty() {
            self.visible_polygons
                .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            self.stats.polygons_submitted = self.visible_polygons.len() as u32;
            for i in 0..self.visible_polygons.len() {
                let poly_surface = unsafe { &*self.visible_polygons[i].0 };
                self.render_surface_polygon(poly_surface, bsp_3d, sectors, pic_data, 0, buffer);
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

    /// Single-pass BSP traversal: render polygons immediately front-to-back.
    /// Depth buffer fills as we go, enabling is_full() early-out. Sector
    /// visibility is tracked inline for sprite rendering.
    fn render_visible_polygons(
        &mut self,
        node_id: u32,
        bsp3d: &BSP3D,
        sectors: &[Sector],
        player_pos: Vec3,
        player_light: usize,
        pic_data: &mut PicData,
        buffer: &mut impl DrawBuffer,
    ) {
        if self.depth_buffer.is_full() {
            return;
        }

        if is_subsector(node_id) {
            let subsector_id = if node_id == u32::MAX {
                0
            } else {
                subsector_index(node_id)
            };

            if let Some(leaf) = bsp3d.get_subsector_leaf(subsector_id) {
                if self.is_bbox_outside_fov(&leaf.aabb) {
                    return;
                }

                for poly_surface in &leaf.polygons {
                    if poly_surface.is_facing_point(player_pos, &bsp3d.vertices) {
                        if self.cull_polygon_bounds(poly_surface, bsp3d).is_some() {
                            self.stats.polygons_submitted += 1;
                            self.render_surface_polygon(
                                poly_surface,
                                bsp3d,
                                sectors,
                                pic_data,
                                player_light,
                                buffer,
                            );
                            let sid = poly_surface.sector_id;
                            if !self.seen_sectors[sid] {
                                self.seen_sectors[sid] = true;
                                self.visible_sectors
                                    .push((sid, sectors[sid].lightlevel >> 4));
                            }
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
        let (front, back) = node.front_back_children(Vec2::new(player_pos.x, player_pos.y));

        // Front side first — skip if node AABB is outside frustum (None = leaf, always
        // enter).
        if bsp3d
            .get_node_aabb(front)
            .map_or(true, |a| !self.is_bbox_outside_fov(a))
        {
            self.render_visible_polygons(
                front,
                bsp3d,
                sectors,
                player_pos,
                player_light,
                pic_data,
                buffer,
            );
        }

        // Back side — same AABB guard.
        if !self.depth_buffer.is_full()
            && bsp3d
                .get_node_aabb(back)
                .map_or(false, |a| !self.is_bbox_outside_fov(a))
        {
            self.render_visible_polygons(
                back,
                bsp3d,
                sectors,
                player_pos,
                player_light,
                pic_data,
                buffer,
            );
        }
    }

    /// Collect visible polygons using PVS + hierarchical BSP node AABB culling.
    /// Walks the BSP tree, skipping subtrees whose AABB is outside the camera
    /// frustum. At leaf nodes, checks PVS visibility before processing
    /// polygons.
    fn collect_pvs_visible_polygons(
        &mut self,
        node_id: u32,
        bsp3d: &BSP3D,
        pvs: &impl PvsData,
        sectors: &[Sector],
        player_pos: Vec3,
        player_subsector_id: usize,
    ) {
        if is_subsector(node_id) {
            // Leaf: subsector
            let subsector_id = if node_id == u32::MAX {
                0
            } else {
                subsector_index(node_id)
            };

            // PVS check
            self.stats.subsectors_total += 1;
            if !pvs.is_visible(player_subsector_id, subsector_id) {
                return;
            }
            self.stats.subsectors_pvs_passed += 1;

            let Some(leaf) = bsp3d.get_subsector_leaf(subsector_id) else {
                return;
            };
            if self.is_bbox_outside_fov(&leaf.aabb) {
                return;
            }

            for poly_surface in &leaf.polygons {
                let sid = poly_surface.sector_id;
                if !self.seen_sectors[sid] {
                    self.seen_sectors[sid] = true;
                    self.visible_sectors
                        .push((sid, sectors[sid].lightlevel >> 4));
                }
                if poly_surface.is_facing_point(player_pos, &bsp3d.vertices) {
                    if let Some(depth) = self.cull_polygon_bounds(poly_surface, bsp3d) {
                        self.visible_polygons
                            .push((poly_surface as *const _, depth));
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
        let (front, back) = node.front_back_children(Vec2::new(player_pos.x, player_pos.y));
        self.collect_pvs_visible_polygons(
            front,
            bsp3d,
            pvs,
            sectors,
            player_pos,
            player_subsector_id,
        );
        self.collect_pvs_visible_polygons(
            back,
            bsp3d,
            pvs,
            sectors,
            player_pos,
            player_subsector_id,
        );
    }
}
