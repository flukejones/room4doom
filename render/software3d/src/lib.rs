#[cfg(feature = "hprof")]
use coarse_prof::profile;
use glam::{Mat4, Vec2, Vec3, Vec4};
use level::{
    AABB, BSP3D, LevelData, Sector, SurfaceKind, SurfacePolygon, WallTexPin, WallType, is_subsector, subsector_index
};
#[cfg(feature = "bench")]
use math::Angle;
use pic_data::{PicData, VoxelManager};
use render_common::{DrawBuffer, RenderView};

use std::f32::consts::PI;
use std::sync::Arc;

pub mod rasterizer;
pub(crate) mod scene;
#[cfg(test)]
mod tests;
pub mod voxel;

use rasterizer::{MAX_CLIPPED_VERTICES, Rasterizer};

enum BBoxCull {
    Outside,
    Occluded,
    Visible,
}
use scene::sprites::SpriteQuad;

#[derive(Clone, Copy)]
struct VertexCache {
    view_pos: Vec4,
    clip_pos: Vec4,
    valid: bool,
}

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
    pub clear_colour: Option<u32>,
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

/// Fake contrast: brightness adjustment for axis-aligned walls.
/// Positive = lighten, negative = darken. Applied to the 0–15 light level.
const FAKE_CONTRAST_LIGHTER: i32 = 1;
const FAKE_CONTRAST_DARKER: i32 = -1;
const FAKE_CONTRAST_NORTH: i32 = FAKE_CONTRAST_DARKER;
const FAKE_CONTRAST_SOUTH: i32 = FAKE_CONTRAST_DARKER;
const FAKE_CONTRAST_EAST: i32 = FAKE_CONTRAST_LIGHTER;
const FAKE_CONTRAST_WEST: i32 = FAKE_CONTRAST_LIGHTER;

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
    /// BSP leaf nodes reached by the traversal.
    subsectors_total: u32,
    /// BSP internal nodes rejected by Hi-Z AABB occlusion.
    nodes_hiz_culled: u32,
    /// Voxel objects matched (things with a voxel model).
    voxel_objects: u32,
    /// Voxel objects fully behind the camera.
    voxel_behind: u32,
    /// Voxel objects rejected by hi-Z occlusion test.
    voxel_hiz_culled: u32,
    /// Total voxel slice quads submitted for rendering.
    voxel_slices_submitted: u32,
    /// Voxel slices culled by distance-based normal test.
    voxel_normal_culled: u32,
    /// Voxel objects that fell back to sprite due to distance.
    voxel_distance_culled: u32,
    /// Voxel slice quads actually rasterised.
    voxel_slices_rendered: u32,
    /// Frames rendered since last stats print.
    #[cfg(feature = "render_stats")]
    frames_since_print: u32,
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
            nodes_hiz_culled: 0,
            voxel_objects: 0,
            voxel_behind: 0,
            voxel_hiz_culled: 0,
            voxel_slices_submitted: 0,
            voxel_normal_culled: 0,
            voxel_distance_culled: 0,
            voxel_slices_rendered: 0,
            #[cfg(feature = "render_stats")]
            frames_since_print: 0,
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
        self.nodes_hiz_culled = 0;
        self.voxel_objects = 0;
        self.voxel_behind = 0;
        self.voxel_hiz_culled = 0;
        self.voxel_slices_submitted = 0;
        self.voxel_normal_culled = 0;
        self.voxel_distance_culled = 0;
        self.voxel_slices_rendered = 0;
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
    pub(crate) extended: Vec<u32>,
    /// Height of the original sky texture.
    pub(crate) tex_height: usize,
    /// Generated rows above the original texture.
    pub(crate) extended_rows: usize,
    /// Generated rows below the original texture.
    pub(crate) down_rows: usize,
    /// Width of the sky texture in columns.
    pub(crate) tex_width: usize,
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
            tex_width: 0,
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
    polygon_outlines: Vec<(Vec<Vec2>, Vec<f32>, u32)>,
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
    view_height: u32,
    width_minus_one: f32,
    height_minus_one: f32,
    fov: f32,
    view_matrix: Mat4,
    camera_pos: Vec3,
    projection_matrix: Mat4,
    rasterizer: Rasterizer,
    near_z: f32,
    far_z: f32,
    // Vertex transformation cache
    vertex_cache: Vec<VertexCache>,
    current_frame_id: u32,
    // Per-frame traversal state — pre-allocated, reset each frame
    seen_sectors: Vec<bool>,
    visible_sectors: Vec<(usize, usize)>,
    sprite_quads: Vec<SpriteQuad>,
    // Sub-structs
    stats: RenderStats,
    sky: SkyRend,
    debug: DebugDraw,
    fuzz_pos: usize,
    // Voxel rendering
    voxel_manager: Option<Arc<VoxelManager>>,
}

impl Software3D {
    pub fn new(width: f32, height: f32, fov: f32, debug_draw: DebugDrawOptions) -> Self {
        let near = 4.0;
        let far = 10000.0;

        let mut s = Self {
            width: width as u32,
            height: height as u32,
            view_height: height as u32,
            width_minus_one: width - 1.0,
            height_minus_one: height - 1.0,
            fov,
            view_matrix: Mat4::IDENTITY,
            camera_pos: Vec3::ZERO,
            projection_matrix: Mat4::IDENTITY,
            rasterizer: Rasterizer::new(width as u32, height as u32),
            near_z: near,
            far_z: far,
            vertex_cache: Vec::new(),
            current_frame_id: 0,
            seen_sectors: Vec::new(),
            visible_sectors: Vec::new(),
            sprite_quads: Vec::with_capacity(64),
            stats: RenderStats::new(),
            sky: SkyRend::new(),
            debug: DebugDraw::new(debug_draw),
            fuzz_pos: 0,
            voxel_manager: None,
        };
        s.set_fov(fov);
        s
    }

    /// Resizes the renderer viewport and updates the projection matrix.
    pub fn resize(&mut self, width: f32, height: f32) {
        self.width = width as u32;
        self.height = height as u32;
        self.view_height = height as u32;
        self.width_minus_one = width - 1.0;
        self.height_minus_one = height - 1.0;

        self.set_fov(self.fov);
        self.rasterizer.resize(width as u32, height as u32);
    }

    /// Update the 3D view height (for statusbar toggle). Recomputes projection.
    pub fn set_view_height(&mut self, vh: f32) {
        self.view_height = vh as u32;
        self.rasterizer.view_height = vh as u32;
        self.set_fov(self.fov);
    }

    /// Set the voxel manager for voxel sprite rendering.
    pub fn set_voxel_manager(&mut self, mgr: Arc<VoxelManager>) {
        self.voxel_manager = Some(mgr);
    }

    pub fn clear_voxel_manager(&mut self) {
        self.voxel_manager = None;
    }

    /// Sets the field of view and updates the projection matrix.
    pub fn set_fov(&mut self, fov: f32) {
        self.fov = fov;
        let (hfov, vfov, _) =
            render_common::og_projection(fov, self.width as f32, self.view_height as f32);
        let aspect = (hfov / 2.0).tan() / (vfov / 2.0).tan();
        self.projection_matrix = Mat4::perspective_rh_gl(vfov, aspect, self.near_z, self.far_z);
        self.sky.h_fov = 2.0 * (1.0 / self.projection_matrix.x_axis.x).atan();
    }

    /// One-time sky setup: precompute static scale factors and per-column edge
    /// colours. Called when the sky texture changes (e.g. new map).
    fn init_sky(&mut self, sky_pic: usize, pic_data: &PicData) {
        let sky = pic_data.wall_pic(sky_pic);
        let sky_w = sky.width;
        let sky_h = sky.height;
        let screen_w = self.width as f32;
        let view_h = self.view_height as f32;

        // Horizontal step: sky tiles SKY_TILES times per full 360°, columns
        // decrease left-to-right (matches 2.5d screen_to_angle convention).
        self.sky.x_step = -(self.sky.h_fov * sky_w as f32 * scene::sky::SKY_TILES)
            / (screen_w * std::f32::consts::TAU);

        // Vertical scale: texture is SKY_V_STRETCH times taller than the view.
        self.sky.v_scale = sky_h as f32 / (view_h * scene::sky::SKY_V_STRETCH);

        // Build combined RGBA buffer: original texture rows + generated extension.
        self.sky.extended = scene::sky::build_sky_combined(
            &sky.data,
            sky_w,
            sky_h,
            pic_data.colourmap(0),
            pic_data.palette(),
        );
        self.sky.tex_height = sky_h;
        self.sky.tex_width = sky_w;
        self.sky.extended_rows = scene::sky::SKY_EXTEND_ROWS;
        self.sky.down_rows = scene::sky::SKY_DOWN_ROWS;

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
        let view_h = self.view_height as f32;
        let sky_w = pic_data.wall_pic(sky_pic).width as f32;

        // Horizontal offset: left edge of screen = angle + hfov/2 (decreasing
        // rightward).
        self.sky.x_offset = (angle_rad + self.sky.h_fov * 0.5) * sky_w * scene::sky::SKY_TILES
            / std::f32::consts::TAU;

        // Vertical center + pitch offset: sky_h/2 sits at view center when
        // pitch = 0; positive pitch (looking up) shifts rows toward the zenith.
        let half_h = view_h * 0.5;
        let sky_center_base = sky_h * 0.5 - half_h * self.sky.v_scale;
        let proj_y = half_h * self.projection_matrix.y_axis.y;
        self.sky.pitch_offset = sky_center_base - pitch_rad * proj_y * self.sky.v_scale;
    }

    /// Fill all pixels that have no solid geometry with the sky texture.
    /// Runs after all polygons and sprites are rendered. Pixels at depth
    /// <= SKY_DEPTH (sky-marked walls or never-written -1.0) get the sky
    /// sampled at screen coordinates.
    fn draw_sky_fill(&self, _pic_data: &PicData, buffer: &mut impl DrawBuffer) {
        use crate::rasterizer::depth_buffer::SKY_DEPTH;
        use crate::rasterizer::sampling::sample_sky_pixel;

        if self.sky.extended.is_empty() {
            return;
        }

        let sky_w = self.sky.tex_width;
        let sky_combined = &self.sky.extended;
        let sky_tex_height = self.sky.tex_height;

        let w = self.width as usize;
        let vh = self.view_height as usize;
        for y in 0..vh {
            let sky_r = (y as f32 * self.sky.v_scale + self.sky.pitch_offset) as i32;
            for x in 0..w {
                if self.rasterizer.depth_buffer.peek_depth_unchecked(x, y) <= SKY_DEPTH {
                    let sky_col = (self.sky.x_offset + x as f32 * self.sky.x_step)
                        .rem_euclid(sky_w as f32) as usize;
                    if let Some(color) =
                        sample_sky_pixel(sky_col, sky_r, sky_tex_height, sky_combined)
                    {
                        buffer.set_pixel(x, y, color);
                    }
                }
            }
        }
    }

    fn update_view_matrix(&mut self, view: &RenderView) {
        let pos = Vec3::new(view.x.into(), view.y.into(), view.viewz.into());
        let angle = view.angle.rad();
        const MAX_PITCH: f32 = 89.0 * PI / 180.0;
        let pitch = view.lookdir.clamp(-MAX_PITCH, MAX_PITCH);

        let forward = Vec3::new(
            angle.cos() * pitch.cos(),
            angle.sin() * pitch.cos(),
            pitch.sin(),
        );
        let up = Vec3::Z;

        // Build rotation-only view matrix (camera at origin). Translation
        // is applied separately in get_transformed_vertex by subtracting
        // camera_pos before rotation. This avoids catastrophic cancellation
        // when large world coords are baked into the matrix.
        self.camera_pos = pos;
        self.view_matrix = Mat4::look_at_rh(Vec3::ZERO, forward, up);
    }

    // ==========================================
    // BSP AND SUBSECTOR RENDERING
    // ==========================================

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
                // Subtract camera position first to keep values small,
                // avoiding catastrophic cancellation in the matrix multiply
                let rel = vertex - self.camera_pos;
                let world_pos = Vec4::new(rel.x, rel.y, rel.z, 1.0);
                let view_pos = self.view_matrix * world_pos;
                let clip_pos = self.projection_matrix * view_pos;

                cache_entry.view_pos = view_pos;
                cache_entry.clip_pos = clip_pos;
                cache_entry.valid = true;
            }

            (cache_entry.view_pos, cache_entry.clip_pos)
        }
    }

    /// Check if 3D bounding box is fully outside the view frustum.
    fn is_bbox_outside_fov(&self, bbox: &AABB) -> bool {
        // Generate all 8 corners of the 3D bbox (camera-relative)
        let view_projection = self.projection_matrix * self.view_matrix;
        let cp = self.camera_pos;
        let clip_corners = [
            view_projection
                * Vec4::new(bbox.min.x - cp.x, bbox.min.y - cp.y, bbox.min.z - cp.z, 1.0),
            view_projection
                * Vec4::new(bbox.max.x - cp.x, bbox.min.y - cp.y, bbox.min.z - cp.z, 1.0),
            view_projection
                * Vec4::new(bbox.max.x - cp.x, bbox.max.y - cp.y, bbox.min.z - cp.z, 1.0),
            view_projection
                * Vec4::new(bbox.min.x - cp.x, bbox.max.y - cp.y, bbox.min.z - cp.z, 1.0),
            view_projection
                * Vec4::new(bbox.min.x - cp.x, bbox.min.y - cp.y, bbox.max.z - cp.z, 1.0),
            view_projection
                * Vec4::new(bbox.max.x - cp.x, bbox.min.y - cp.y, bbox.max.z - cp.z, 1.0),
            view_projection
                * Vec4::new(bbox.max.x - cp.x, bbox.max.y - cp.y, bbox.max.z - cp.z, 1.0),
            view_projection
                * Vec4::new(bbox.min.x - cp.x, bbox.max.y - cp.y, bbox.max.z - cp.z, 1.0),
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

    /// Frustum + Hi-Z AABB occlusion test for BSP node bounding boxes.
    /// Returns `Outside` if fully outside frustum, `Occluded` if inside
    /// frustum but fully behind existing depth, `Visible` otherwise.
    fn cull_bbox(&self, bbox: &AABB) -> BBoxCull {
        let view_projection = self.projection_matrix * self.view_matrix;
        let cp = self.camera_pos;
        let clip_corners = [
            view_projection
                * Vec4::new(bbox.min.x - cp.x, bbox.min.y - cp.y, bbox.min.z - cp.z, 1.0),
            view_projection
                * Vec4::new(bbox.max.x - cp.x, bbox.min.y - cp.y, bbox.min.z - cp.z, 1.0),
            view_projection
                * Vec4::new(bbox.max.x - cp.x, bbox.max.y - cp.y, bbox.min.z - cp.z, 1.0),
            view_projection
                * Vec4::new(bbox.min.x - cp.x, bbox.max.y - cp.y, bbox.min.z - cp.z, 1.0),
            view_projection
                * Vec4::new(bbox.min.x - cp.x, bbox.min.y - cp.y, bbox.max.z - cp.z, 1.0),
            view_projection
                * Vec4::new(bbox.max.x - cp.x, bbox.min.y - cp.y, bbox.max.z - cp.z, 1.0),
            view_projection
                * Vec4::new(bbox.max.x - cp.x, bbox.max.y - cp.y, bbox.max.z - cp.z, 1.0),
            view_projection
                * Vec4::new(bbox.min.x - cp.x, bbox.max.y - cp.y, bbox.max.z - cp.z, 1.0),
        ];

        if clip_corners.iter().all(|c| c.x < -c.w)
            || clip_corners.iter().all(|c| c.x > c.w)
            || clip_corners.iter().all(|c| c.y < -c.w)
            || clip_corners.iter().all(|c| c.y > c.w)
            || clip_corners.iter().all(|c| c.z < -c.w)
            || clip_corners.iter().all(|c| c.z > c.w)
        {
            return BBoxCull::Outside;
        }

        // Hi-Z test: project to screen AABB if all corners are in front
        let hw = self.width as f32 * 0.5;
        let hh = self.view_height as f32 * 0.5;
        let mut all_in_front = true;
        let mut max_inv_w: f32 = 0.0;
        let mut scr_min_x = f32::MAX;
        let mut scr_min_y = f32::MAX;
        let mut scr_max_x = f32::MIN;
        let mut scr_max_y = f32::MIN;

        for c in &clip_corners {
            if c.w <= 0.0 {
                all_in_front = false;
                break;
            }
            let inv_w = 1.0 / c.w;
            if inv_w > max_inv_w {
                max_inv_w = inv_w;
            }
            let sx = (c.x * inv_w) * hw + hw;
            let sy = hh - (c.y * inv_w) * hh;
            scr_min_x = scr_min_x.min(sx);
            scr_min_y = scr_min_y.min(sy);
            scr_max_x = scr_max_x.max(sx);
            scr_max_y = scr_max_y.max(sy);
        }

        if all_in_front {
            let min_x = scr_min_x.max(0.0) as usize;
            let min_y = scr_min_y.max(0.0) as usize;
            let max_x = (scr_max_x as usize).min(self.width as usize - 1);
            let max_y = (scr_max_y as usize).min(self.view_height as usize - 1);
            if self
                .rasterizer
                .depth_buffer
                .is_occluded_hiz(min_x, min_y, max_x, max_y, max_inv_w)
            {
                return BBoxCull::Occluded;
            }
        }

        BBoxCull::Visible
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
        let mut all_in_front = true;

        let hw = self.width as f32 * 0.5;
        let hh = self.view_height as f32 * 0.5;
        let mut scr_min_x = f32::MAX;
        let mut scr_min_y = f32::MAX;
        let mut scr_max_x = f32::MIN;
        let mut scr_max_y = f32::MIN;

        for &vidx in &polygon.vertices {
            let (_, clip_pos) = self.get_transformed_vertex(vidx, bsp3d);

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
                let sx = (clip_pos.x / clip_pos.w) * hw + hw;
                let sy = hh - (clip_pos.y / clip_pos.w) * hh;
                scr_min_x = scr_min_x.min(sx);
                scr_min_y = scr_min_y.min(sy);
                scr_max_x = scr_max_x.max(sx);
                scr_max_y = scr_max_y.max(sy);
            } else {
                all_in_front = false;
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

        // Hi-Z pre-check: if all vertices in front, use projected AABB.
        if all_in_front {
            let min_x = scr_min_x.max(0.0) as usize;
            let min_y = scr_min_y.max(0.0) as usize;
            let max_x = (scr_max_x as usize).min(self.width as usize - 1);
            let max_y = (scr_max_y as usize).min(self.view_height as usize - 1);
            if self
                .rasterizer
                .depth_buffer
                .is_occluded_hiz(min_x, min_y, max_x, max_y, max_inv_w)
            {
                return None;
            }
        }

        Some(max_inv_w)
    }

    /// Transform, clip, and rasterize a surface polygon. Clip-space frustum
    /// cull, Sutherland-Hodgman clip, perspective divide, hi-Z test, then
    /// draw_polygon dispatch.
    fn render_surface_polygon(
        &mut self,
        polygon: &SurfacePolygon,
        bsp3d: &BSP3D,
        sectors: &[Sector],
        pic_data: &mut PicData,
        player_light: usize,
        buffer: &mut impl DrawBuffer,
    ) {
        self.rasterizer.screen_vertices_len = 0;
        self.rasterizer.tex_coords_len = 0;
        self.rasterizer.inv_w_len = 0;
        self.rasterizer.clipped_vertices_len = 0;

        // Transform vertices to clip space and setup for clipping
        let vert_count = polygon.vertices.len();
        assert!(vert_count <= MAX_CLIPPED_VERTICES);
        let mut input_vertices = [Vec4::ZERO; MAX_CLIPPED_VERTICES];
        let mut input_tex_coords = [Vec3::ZERO; MAX_CLIPPED_VERTICES];

        // Pre-compute wall z-range once — used by all vertices in
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
        self.rasterizer
            .clip_polygon_frustum(&input_vertices, &input_tex_coords, vert_count);

        // Project clipped vertices to screen space, tracking AABB and max depth inline
        // to avoid rescanning the buffers later.
        let w_f32 = self.width as f32;
        let vh_f32 = self.view_height as f32;
        let mut max_inv_w: f32 = 0.0;
        let mut scr_min_x = f32::MAX;
        let mut scr_min_y = f32::MAX;
        let mut scr_max_x = f32::MIN;
        let mut scr_max_y = f32::MIN;

        for i in 0..self.rasterizer.clipped_vertices_len {
            let clip_pos = self.rasterizer.clipped_vertices_buffer[i];
            let tex_coord = self.rasterizer.clipped_tex_coords_buffer[i];

            if clip_pos.w > 0.0 {
                let inv_w = 1.0 / clip_pos.w;
                if inv_w > max_inv_w {
                    max_inv_w = inv_w;
                }
                // Compute screen coords directly from clip-space to avoid
                // catastrophic cancellation in (1.0 - ndc.y) when ndc.y ≈ 1.0
                // (distant horizontal polygons near the horizon).
                let half_w = 0.5 * w_f32;
                let half_h = 0.5 * vh_f32;
                let mut screen_x = (clip_pos.x + clip_pos.w) * half_w * inv_w;
                let mut screen_y = (clip_pos.w - clip_pos.y) * half_h * inv_w;

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
                } else if (screen_y - vh_f32).abs() < SNAP {
                    screen_y = vh_f32;
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

                self.rasterizer.screen_vertices_buffer[self.rasterizer.screen_vertices_len] =
                    Vec2::new(screen_x, screen_y);
                self.rasterizer.tex_coords_buffer[self.rasterizer.tex_coords_len] =
                    Vec2::new(tex_coord.x * inv_w, tex_coord.y * inv_w);
                self.rasterizer.inv_w_buffer[self.rasterizer.inv_w_len] = inv_w;

                self.rasterizer.screen_vertices_len += 1;
                self.rasterizer.tex_coords_len += 1;
                self.rasterizer.inv_w_len += 1;
            }
        }

        if self.rasterizer.screen_vertices_len < 3 {
            self.stats.polygons_frustum_clipped += 1;
            return;
        }

        if (scr_max_x - scr_min_x) < 1.0 && (scr_max_y - scr_min_y) < 1.0 {
            self.stats.polygons_early_culled += 1;
            return;
        }

        // Hi-Z depth rejection using pre-computed AABB and max depth — no rescan
        // needed.
        if max_inv_w > 0.0 {
            let x0 = scr_min_x.max(0.0).min(w_f32 - 1.0) as usize;
            let x1 = scr_max_x.max(0.0).min(w_f32 - 1.0) as usize;
            let y0 = scr_min_y.max(0.0).min(vh_f32 - 1.0) as usize;
            let y1 = scr_max_y.max(0.0).min(vh_f32 - 1.0) as usize;
            if self
                .rasterizer
                .depth_buffer
                .is_occluded_hiz(x0, y0, x1, y1, max_inv_w)
            {
                self.stats.polygons_depth_rejected += 1;
                return;
            }
        }

        let mut brightness = ((sectors[polygon.sector_id].lightlevel >> 4) + player_light).min(15);
        // Fake contrast: axis-aligned walls get a brightness nudge.
        // Normal along +Y = north-facing, -Y = south, +X = east, -X = west.
        if polygon.normal.z.abs() < 0.01 {
            let adjust = if polygon.normal.x.abs() < 0.001 {
                if polygon.normal.y > 0.0 {
                    FAKE_CONTRAST_NORTH
                } else {
                    FAKE_CONTRAST_SOUTH
                }
            } else if polygon.normal.y.abs() < 0.001 {
                if polygon.normal.x > 0.0 {
                    FAKE_CONTRAST_EAST
                } else {
                    FAKE_CONTRAST_WEST
                }
            } else {
                0
            };
            brightness = (brightness as i32 + adjust).clamp(0, 15) as usize;
        }
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
            let verts = self.rasterizer.screen_vertices_buffer
                [..self.rasterizer.screen_vertices_len]
                .to_vec();
            let depths = self.rasterizer.inv_w_buffer[..self.rasterizer.inv_w_len].to_vec();
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

            // Project both points to screen (camera-relative)
            let vp = self.projection_matrix * self.view_matrix;
            let cp = self.camera_pos;
            let c_rel = center - cp;
            let t_rel = tip - cp;
            let c_clip = vp * Vec4::new(c_rel.x, c_rel.y, c_rel.z, 1.0);
            let t_clip = vp * Vec4::new(t_rel.x, t_rel.y, t_rel.z, 1.0);

            if c_clip.w > 0.0 && t_clip.w > 0.0 {
                let w = self.width as f32;
                let vh = self.view_height as f32;
                let c_screen = Vec2::new(
                    (c_clip.x / c_clip.w + 1.0) * 0.5 * w,
                    (1.0 - c_clip.y / c_clip.w) * 0.5 * vh,
                );
                let t_screen = Vec2::new(
                    (t_clip.x / t_clip.w + 1.0) * 0.5 * w,
                    (1.0 - t_clip.y / t_clip.w) * 0.5 * vh,
                );
                let depth = 1.0 / c_clip.w;
                self.debug.normal_lines.push((c_screen, t_screen, depth));
            }
        }
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
        view: &RenderView,
        level_data: &LevelData,
        pic_data: &mut PicData,
        buffer: &mut impl DrawBuffer,
    ) {
        self.prepare_vertex_cache(&level_data.bsp_3d);
        self.current_frame_id = self.current_frame_id.wrapping_add(1);
        #[cfg(feature = "hprof")]
        profile!("render_player_view");
        let LevelData {
            sectors,
            bsp_3d,
            ..
        } = level_data;

        self.update_view_matrix(view);

        if let Some(colour) = self.debug.options.clear_colour {
            buffer.buf_mut().fill(colour);
        }

        self.stats.reset();
        self.rasterizer.depth_buffer.reset();
        self.debug.polygon_outlines.clear();
        self.debug.normal_lines.clear();

        let player_pos = Vec3::new(view.x.into(), view.y.into(), view.viewz.into());

        {
            let sector_count = sectors.len();
            self.seen_sectors.resize(sector_count, false);
            self.seen_sectors.fill(false);
            self.visible_sectors.clear();

            let player_angle_rad = view.angle.rad();
            let player_pitch_rad = view.lookdir;
            self.update_sky_params(player_angle_rad, player_pitch_rad, pic_data);

            self.render_bsp(
                bsp_3d.root_node(),
                bsp_3d,
                sectors,
                player_pos,
                view.extralight,
                pic_data,
                buffer,
            );

            // Draw sprites after all geometry
            self.draw_sprites(sectors, view, pic_data, buffer);

            // Fill remaining empty/sky pixels with the sky backdrop. Any pixel
            // at depth <= SKY_DEPTH (written by sky-textured walls or left at
            // -1.0) gets the sky texture sampled at screen coordinates.
            self.draw_sky_fill(pic_data, buffer);

            // Draw player weapon overlay on top of everything
            self.draw_player_weapons(view, pic_data, buffer);

            // Debug: draw polygon outlines / wireframe as post-render overlay
            if self.debug.options.outline || self.debug.options.wireframe {
                self.draw_debug_polygon_outlines(buffer);
            }

            // Debug: draw normal direction lines
            if self.debug.options.normals {
                self.draw_debug_normal_lines(buffer);
            }

            #[cfg(feature = "render_stats")]
            {
                self.stats.frames_since_print += 1;
                let elapsed = self.stats.last_print.elapsed().as_secs_f32();
                if elapsed >= 1.0 {
                    let fps = self.stats.frames_since_print as f32 / elapsed;
                    println!(
                        "polys: {} submitted, {} frustum-clipped, {} culled, {} early-depth, {} no-draw, {} rendered | subsectors: {} | nodes hiz-culled: {} | fps: {:.0}",
                        self.stats.polygons_submitted,
                        self.stats.polygons_frustum_clipped,
                        self.stats.polygons_early_culled,
                        self.stats.polygons_depth_rejected,
                        self.stats.polygons_no_draw,
                        self.stats.polygons_rendered,
                        self.stats.subsectors_total,
                        self.stats.nodes_hiz_culled,
                        fps,
                    );
                    if self.stats.voxel_objects > 0 || self.voxel_manager.is_some() {
                        println!(
                            "voxels: {} objects, {} behind, {} hiz-culled, {} normal-culled, {} dist-culled, {} slices submitted, {} rendered",
                            self.stats.voxel_objects,
                            self.stats.voxel_behind,
                            self.stats.voxel_hiz_culled,
                            self.stats.voxel_normal_culled,
                            self.stats.voxel_distance_culled,
                            self.stats.voxel_slices_submitted,
                            self.stats.voxel_slices_rendered,
                        );
                    }
                    self.stats.frames_since_print = 0;
                    self.stats.last_print = std::time::Instant::now();
                }
            }
        }
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
        level_data: &LevelData,
        pic_data: &mut PicData,
        buffer: &mut impl DrawBuffer,
    ) {
        let LevelData {
            sectors,
            bsp_3d,
            ..
        } = level_data;

        self.prepare_vertex_cache(bsp_3d);
        self.current_frame_id = self.current_frame_id.wrapping_add(1);

        let forward = Vec3::new(
            angle_rad.cos() * pitch_rad.cos(),
            angle_rad.sin() * pitch_rad.cos(),
            pitch_rad.sin(),
        );
        self.camera_pos = pos;
        self.view_matrix = Mat4::look_at_rh(Vec3::ZERO, forward, Vec3::Z);

        self.stats.reset();
        self.rasterizer.depth_buffer.reset();

        self.seen_sectors.resize(sectors.len(), false);
        self.seen_sectors.fill(false);
        self.visible_sectors.clear();

        self.update_sky_params(angle_rad, pitch_rad, pic_data);

        self.render_bsp(
            bsp_3d.root_node(),
            bsp_3d,
            sectors,
            pos,
            0,
            pic_data,
            buffer,
        );
    }

    /// Front-to-back BSP traversal with immediate rendering and Hi-Z AABB
    /// node rejection. Depth buffer fills as we go, enabling `is_full()`
    /// early-out and Hi-Z rejection of entire subtrees.
    fn render_bsp(
        &mut self,
        node_id: u32,
        bsp3d: &BSP3D,
        sectors: &[Sector],
        player_pos: Vec3,
        player_light: usize,
        pic_data: &mut PicData,
        buffer: &mut impl DrawBuffer,
    ) {
        if self.rasterizer.depth_buffer.is_full() {
            return;
        }

        if is_subsector(node_id) {
            let subsector_id = if node_id == u32::MAX {
                0
            } else {
                subsector_index(node_id)
            };

            self.stats.subsectors_total += 1;

            let Some(leaf) = bsp3d.get_subsector_leaf(subsector_id) else {
                return;
            };
            if self.is_bbox_outside_fov(&leaf.aabb) {
                return;
            }

            // Mark all sectors in this leaf as visible for sprite/voxel
            // rendering BEFORE per-polygon culling. A sector's geometry may
            // be fully occluded while its sprites are still visible.
            for poly_surface in &leaf.polygons {
                let sid = poly_surface.sector_id;
                if !self.seen_sectors[sid] {
                    self.seen_sectors[sid] = true;
                    self.visible_sectors
                        .push((sid, sectors[sid].lightlevel >> 4));
                }
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
                    }
                }
            }
            return;
        }

        let Some(node) = bsp3d.nodes().get(node_id as usize) else {
            return;
        };
        let (front, back) = node.front_back_children(Vec2::new(player_pos.x, player_pos.y));

        // Front side — cull by frustum + Hi-Z. Leaves always enter (no node AABB).
        let front_cull = bsp3d.get_node_aabb(front).map(|a| self.cull_bbox(a));
        match front_cull {
            Some(BBoxCull::Outside) => {}
            Some(BBoxCull::Occluded) => {
                self.stats.nodes_hiz_culled += 1;
            }
            _ => {
                self.render_bsp(
                    front,
                    bsp3d,
                    sectors,
                    player_pos,
                    player_light,
                    pic_data,
                    buffer,
                );
            }
        }

        // Back side — skip if depth buffer full, cull by frustum + Hi-Z.
        if !self.rasterizer.depth_buffer.is_full() {
            let back_cull = bsp3d.get_node_aabb(back).map(|a| self.cull_bbox(a));
            match back_cull {
                Some(BBoxCull::Outside) => {}
                Some(BBoxCull::Occluded) => {
                    self.stats.nodes_hiz_culled += 1;
                }
                _ => {
                    self.render_bsp(
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
        }
    }
}
