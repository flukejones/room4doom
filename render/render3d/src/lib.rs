#[cfg(feature = "hprof")]
use coarse_prof::profile;
use gameplay::{Level, MapData, Node, PicData, Player, Segment, SubSector};
use glam::{Mat4, Vec2, Vec3};
use render_trait::{PixelBuffer, RenderTrait};

use std::f32::consts::PI;

mod bsp_polygon;
mod occlusion;
mod polygon;
use bsp_polygon::{BSPPolygons, Triangle};
use occlusion::OcclusionBuffer;
use polygon::{Polygon2D, Polygon3D};

use crate::polygon::segment_to_polygons;

const IS_SSECTOR_MASK: u32 = 0x8000_0000;

/// A 3D software renderer for Doom levels.
///
/// This renderer displays the level geometry in true 3D space,
/// showing floors, ceilings, walls with different colors.
pub struct Renderer3D {
    width: u32,
    height: u32,
    width_minus_one: f32,
    height_minus_one: f32,
    fov: f32,
    view_matrix: Mat4,
    projection_matrix: Mat4,
    occlusion_buffer: OcclusionBuffer,
    intersection_buffer: Vec<f32>,
    map_name: String,
    bsp_polygons: BSPPolygons,
    render_filled: bool,
}

impl Renderer3D {
    // ==========================================
    // UTILITY/HELPER FUNCTIONS
    // ==========================================

    /// Determines if a segment is front-facing relative to the player position
    /// - Segments have implicit direction from v1 to v2
    /// - Front-facing means the front sector is on the right side when walking v1→v2
    fn is_segment_front_facing(&self, seg: &Segment, player_pos: Vec2) -> bool {
        seg.point_on_side(player_pos) == 1
    }

    /// Cohen-Sutherland line clipping outcode calculation
    ///
    /// Outcodes indicate which side(s) of the viewport a point is on:
    /// - Bit 0: Above (y > ymax)
    /// - Bit 1: Below (y < ymin)
    /// - Bit 2: Right (x > xmax)
    /// - Bit 3: Left (x < xmin)
    fn compute_outcode(&self, x: f32, y: f32) -> u8 {
        let mut code = 0;

        if y > self.height as f32 {
            code |= 1; // Above
        } else if y < 0.0 {
            code |= 2; // Below
        }

        if x > self.width as f32 {
            code |= 4; // Right
        } else if x < 0.0 {
            code |= 8; // Left
        }

        code
    }

    /// Clips a line to the viewport using Cohen-Sutherland algorithm
    ///
    /// Benefits over simple culling:
    /// - More accurate representation of level structure
    /// - Prevents "popping" when segments move in/out of view
    fn clip_line(&self, mut p1: Vec2, mut p2: Vec2) -> Option<(Vec2, Vec2)> {
        #[cfg(feature = "hprof")]
        profile!("clip_line");

        let mut outcode1 = self.compute_outcode(p1.x, p1.y);
        let mut outcode2 = self.compute_outcode(p2.x, p2.y);

        loop {
            // Trivial accept: both points inside viewport
            if outcode1 == 0 && outcode2 == 0 {
                return Some((p1, p2));
            }

            // Trivial reject: both points outside same edge
            if outcode1 & outcode2 != 0 {
                return None;
            }

            // At this point, one point is inside, one is outside
            // We'll move the outside point to the boundary

            // Pick the point with non-zero outcode
            let outcode = if outcode1 != 0 { outcode1 } else { outcode2 };
            let mut x = 0.0;
            let mut y = 0.0;

            // Find intersection with appropriate boundary
            if outcode & 1 != 0 {
                // Above
                x = p1.x + (p2.x - p1.x) * (self.height as f32 - p1.y) / (p2.y - p1.y);
                y = self.height as f32;
            } else if outcode & 2 != 0 {
                // Below
                x = p1.x + (p2.x - p1.x) * (0.0 - p1.y) / (p2.y - p1.y);
                y = 0.0;
            } else if outcode & 4 != 0 {
                // Right
                y = p1.y + (p2.y - p1.y) * (self.width as f32 - p1.x) / (p2.x - p1.x);
                x = self.width as f32;
            } else if outcode & 8 != 0 {
                // Left
                y = p1.y + (p2.y - p1.y) * (0.0 - p1.x) / (p2.x - p1.x);
                x = 0.0;
            }

            // Update the outside point
            if outcode == outcode1 {
                p1.x = x;
                p1.y = y;
                outcode1 = self.compute_outcode(p1.x, p1.y);
            } else {
                p2.x = x;
                p2.y = y;
                outcode2 = self.compute_outcode(p2.x, p2.y);
            }
        }
    }

    /// Check if a bounding box is potentially visible using frustum culling
    /// Check if a bounding box is potentially visible using frustum culling
    fn bbox_in_view(&mut self, node: &Node, _player_pos: Vec2, side: usize) -> bool {
        #[cfg(feature = "hprof")]
        profile!("bbox_in_view");
        let bbox = &node.bboxes[side];
        let min = bbox[0];
        let max = bbox[1];

        // Get all 8 corners of the bounding box (assuming floor and ceiling heights)
        let corners = [
            Vec3::new(min.x, min.y, node.min_z), // bottom corners
            Vec3::new(max.x, min.y, node.min_z),
            Vec3::new(min.x, max.y, node.min_z),
            Vec3::new(max.x, max.y, node.min_z),
            Vec3::new(min.x, min.y, node.max_z), // top corners
            Vec3::new(max.x, min.y, node.max_z),
            Vec3::new(min.x, max.y, node.max_z),
            Vec3::new(max.x, max.y, node.max_z),
        ];

        // Transform corners to view space
        let mut view_corners = Vec::with_capacity(8);
        for corner in &corners {
            let view_pos = self.view_matrix.transform_point3(*corner);
            view_corners.push(view_pos);
        }

        // Check if all points are outside any single frustum plane
        // If all points are on the wrong side of any plane, bbox is outside frustum

        // Near plane check (z > -0.1)
        if view_corners.iter().all(|p| p.z > -0.1) {
            return false;
        }

        // Far plane check (z < -10000.0)
        if view_corners.iter().all(|p| p.z < -10000.0) {
            return false;
        }

        // Calculate frustum plane parameters
        let aspect = self.width as f32 / self.height as f32;
        let half_fov_y = self.fov / 2.0;
        let tan_half_fov_y = half_fov_y.tan();
        let tan_half_fov_x = aspect * tan_half_fov_y;

        // Left plane: x < -z * tan(fov_x/2)
        if view_corners.iter().all(|p| p.x < p.z * tan_half_fov_x) {
            return false;
        }

        // Right plane: x > -z * tan(fov_x/2)
        if view_corners.iter().all(|p| p.x > -p.z * tan_half_fov_x) {
            return false;
        }

        // Bottom plane: y < -z * tan(fov_y/2)
        if view_corners.iter().all(|p| p.y < p.z * tan_half_fov_y) {
            return false;
        }

        // Top plane: y > -z * tan(fov_y/2)
        if view_corners.iter().all(|p| p.y > -p.z * tan_half_fov_y) {
            return false;
        }

        true
    }

    // ==========================================
    // INITIALIZATION AND CONFIGURATION METHODS
    // ==========================================

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

        Self {
            width: width as u32,
            height: height as u32,
            width_minus_one: width - 1.0,
            height_minus_one: height - 1.0,
            fov,
            view_matrix: Mat4::IDENTITY,
            projection_matrix: Mat4::perspective_rh_gl(fov, aspect, near, far),
            occlusion_buffer: OcclusionBuffer::new(width as usize, height as usize),
            intersection_buffer: Vec::with_capacity(256), // Pre-allocate for polygon intersections
            map_name: String::new(),
            bsp_polygons: BSPPolygons::new(),
            render_filled: true, // Default to filled mode
        }
    }

    /// Resizes the renderer viewport and updates the projection matrix.
    pub fn resize(&mut self, width: f32, height: f32) {
        self.width = width as u32;
        self.height = height as u32;
        self.width_minus_one = width - 1.0;
        self.height_minus_one = height - 1.0;

        // Update projection matrix with new aspect ratio
        let aspect = width / height;
        let near = 0.1;
        let far = 10000.0;
        self.projection_matrix = Mat4::perspective_rh_gl(self.fov, aspect, near, far);

        // Resize occlusion buffer
        self.occlusion_buffer = OcclusionBuffer::new(width as usize, height as usize);
    }

    /// Sets the field of view and updates the projection matrix.
    pub fn set_fov(&mut self, fov: f32) {
        self.fov = fov;
        let aspect = self.width as f32 / self.height as f32;
        let near = 0.1;
        let far = 10000.0;
        self.projection_matrix = Mat4::perspective_rh_gl(fov, aspect, near, far);
    }

    /// Set whether to render filled polygons or wireframes
    pub fn set_render_filled(&mut self, filled: bool) {
        self.render_filled = filled;
    }

    /// Get current rendering mode
    pub fn is_render_filled(&self) -> bool {
        self.render_filled
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

    // ==========================================
    // RENDERING PRIMITIVES
    // ==========================================

    /// Draw polygon with span-based occlusion
    fn draw_polygon_with_occlusion(&mut self, rend: &mut impl RenderTrait, poly: &Polygon2D) {
        #[cfg(feature = "hprof")]
        profile!("draw_polygon_with_occlusion");
        // Check if polygon is completely outside screen bounds
        if let Some((min, max)) = poly.bounds() {
            if min.x > self.width_minus_one
                || max.x < 0.0
                || min.y > self.height_minus_one
                || max.y < 0.0
            {
                return; // Skip if entirely outside
            }

            // Draw the polygon
            if self.render_filled {
                self.draw_filled_polygon(rend, poly);
            } else {
                // Draw polygon as wireframe (edge-only)
                let vertices = &poly.vertices;
                let vertex_count = vertices.len();

                for i in 0..vertex_count {
                    let v1 = vertices[i];
                    let v2 = vertices[(i + 1) % vertex_count];

                    // Quick reject for lines outside screen
                    if (v1.x < 0.0 && v2.x < 0.0)
                        || (v1.x > self.width_minus_one && v2.x > self.width_minus_one)
                        || (v1.y < 0.0 && v2.y < 0.0)
                        || (v1.y > self.height_minus_one && v2.y > self.height_minus_one)
                    {
                        continue;
                    }

                    // Clip line to screen bounds
                    if let Some((clipped_v1, clipped_v2)) = self.clip_line(v1, v2) {
                        // Draw the line with occlusion checking
                        self.draw_line_with_occlusion(rend, clipped_v1, clipped_v2, poly.color);
                    }
                }
            }

            // Update occlusion buffer if polygon is large enough
            if max.x - min.x > 4.0 || max.y - min.y > 4.0 {
                self.occlusion_buffer.update_polygon_occlusion(poly);
            }
        } else {
            return; // Skip degenerate polygons
        }
    }

    /// Draw filled polygon using scanline algorithm
    fn draw_filled_polygon(&mut self, rend: &mut impl RenderTrait, poly: &Polygon2D) {
        #[cfg(feature = "hprof")]
        profile!("draw_filled_polygon");
        if poly.vertices.len() < 3 {
            return;
        }

        // Get bounding box
        if let Some((min, max)) = poly.bounds() {
            let x_start = min.x.max(0.0) as i32;
            let x_end = max.x.min(self.width as f32 - 1.0) as i32;
            let y_min = min.y.max(0.0);
            let y_max = max.y.min(self.height as f32 - 1.0);

            // Process each column
            for x in x_start..=x_end {
                if x < 0 || x >= self.width as i32 {
                    continue;
                }

                let x_float = x as f32;
                self.intersection_buffer.clear();

                // Find all Y intersections at this X column
                for i in 0..poly.vertices.len() {
                    let v1 = poly.vertices[i];
                    let v2 = poly.vertices[(i + 1) % poly.vertices.len()];

                    // Check if edge crosses this X column
                    if (v1.x <= x_float && v2.x >= x_float) || (v2.x <= x_float && v1.x >= x_float)
                    {
                        // Skip vertical edges
                        if (v2.x - v1.x).abs() > 0.001 {
                            let t = (x_float - v1.x) / (v2.x - v1.x);
                            if t >= 0.0 && t <= 1.0 {
                                let y = v1.y + (v2.y - v1.y) * t;
                                self.intersection_buffer.push(y);
                            }
                        }
                    }
                }

                // Skip if no intersections or odd number (invalid polygon)
                if self.intersection_buffer.is_empty() {
                    continue;
                }

                // Sort intersections
                self.intersection_buffer
                    .sort_by(|a, b| a.partial_cmp(b).unwrap());

                // Process pairs of intersections (fill between them)
                let mut i = 0;
                while i < self.intersection_buffer.len() - 1 {
                    let mut column_y_min = self.intersection_buffer[i];
                    let mut column_y_max = self.intersection_buffer[i + 1];

                    // Clamp to screen bounds
                    column_y_min = column_y_min.max(y_min).max(0.0);
                    column_y_max = column_y_max.min(y_max).min(self.height as f32 - 1.0);

                    if column_y_min <= column_y_max {
                        // Get visible spans for this column segment
                        let visible_spans = self.occlusion_buffer.get_visible_spans(
                            x as usize,
                            column_y_min,
                            column_y_max,
                        );

                        // Draw each visible span
                        for (span_top, span_bottom) in visible_spans {
                            let y_start = span_top.ceil() as i32;
                            let y_end = span_bottom.floor() as i32;

                            for y in y_start..=y_end {
                                if y >= 0 && y <= self.height as i32 {
                                    rend.draw_buffer().set_pixel(
                                        x as usize,
                                        y as usize,
                                        &poly.color,
                                    );
                                }
                            }
                        }
                    }

                    i += 1;
                }
            }
        }
    }

    /// Draw line with occlusion checking
    fn draw_line_with_occlusion(
        &self,
        rend: &mut impl RenderTrait,
        p1: Vec2,
        p2: Vec2,
        color: [u8; 4],
    ) {
        #[cfg(feature = "hprof")]
        profile!("draw_line_with_occlusion");
        let x1 = p1.x as i32;
        let y1 = p1.y as i32;
        let x2 = p2.x as i32;
        let y2 = p2.y as i32;

        let dx = (x2 - x1).abs();
        let dy = (y2 - y1).abs();
        let sx = if x1 < x2 { 1 } else { -1 };
        let sy = if y1 < y2 { 1 } else { -1 };
        let mut err = dx - dy;

        let mut x = x1;
        let mut y = y1;

        // Collect pixels in a buffer before checking occlusion
        let mut pixels = Vec::new();

        loop {
            if x >= 0 && y >= 0 && x < self.width as i32 && y < self.height as i32 {
                pixels.push((x, y));
            }

            if x == x2 && y == y2 {
                break;
            }

            let e2 = 2 * err;
            if e2 > -dy {
                err -= dy;
                x += sx;
            }
            if e2 < dx {
                err += dx;
                y += sy;
            }
        }

        // Draw pixels in runs based on visibility
        let mut i = 0;
        while i < pixels.len() {
            let (x, y) = pixels[i];

            // Check if this pixel is visible
            if !self
                .occlusion_buffer
                .is_point_occluded(x as usize, y as f32)
            {
                // Find run of visible pixels
                let mut j = i;
                while j < pixels.len() {
                    let (px, py) = pixels[j];
                    if self
                        .occlusion_buffer
                        .is_point_occluded(px as usize, py as f32)
                    {
                        break;
                    }
                    rend.draw_buffer()
                        .set_pixel(px as usize, py as usize, &color);
                    j += 1;
                }
                i = j;
            } else {
                i += 1;
            }
        }
    }

    // ==========================================
    // BSP AND SUBSECTOR RENDERING
    // ==========================================

    /// Get pre-triangulated subsector floor/ceiling data
    fn get_subsector_triangles(&self, subsector_idx: usize) -> Vec<Triangle> {
        #[cfg(feature = "hprof")]
        profile!("get_subsector_triangles");
        self.bsp_polygons
            .get_subsector_triangles(subsector_idx)
            .map(|triangles| triangles.to_vec())
            .unwrap_or_default()
    }

    /// Renders a single line segment with portal height differences
    ///
    /// Doom portal rendering concept:
    /// - Each segment separates two sectors (front and back)
    /// - When sectors have different heights, we draw the height difference
    /// - This creates the illusion of steps, windows, doors, etc.
    fn render_segment(
        &mut self,
        rend: &mut impl RenderTrait,
        seg: &Segment,
        player_pos: Vec2,
        pic_data: &mut PicData,
    ) {
        #[cfg(feature = "hprof")]
        profile!("render_segment");
        // Skip back-facing segments for performance
        if self.is_segment_front_facing(seg, player_pos) {
            return;
        }
        // Convert segment to 3D polygons
        let polygons = segment_to_polygons(seg, pic_data);

        // First pass: render solid polygons
        for poly in polygons {
            // Transform polygon to view space
            let view_poly = poly.transform(&self.view_matrix);

            // Simple check - at least one vertex should be in front
            let mut any_in_front = false;
            for v in &view_poly.vertices {
                if v.z < -0.1 {
                    any_in_front = true;
                    break;
                }
            }
            if !any_in_front {
                continue;
            }

            // Project to screen space
            if let Some(screen_poly) = view_poly.project(
                &self.projection_matrix,
                self.width as f32,
                self.height as f32,
            ) {
                // Draw polygon with occlusion
                self.draw_polygon_with_occlusion(rend, &screen_poly);
            }
        }
    }

    fn render_subsector(
        &mut self,
        map: &MapData,
        rend: &mut impl RenderTrait,
        subsector: &SubSector,
        player_pos: Vec2,
        pic_data: &mut PicData,
    ) {
        #[cfg(feature = "hprof")]
        profile!("render_subsector");
        let start_seg = subsector.start_seg as usize;
        let end_seg = start_seg + subsector.seg_count as usize;

        if let Some(segments) = map.segments().get(start_seg..end_seg) {
            for seg in segments {
                self.render_segment(rend, seg, player_pos, pic_data);
            }
        }

        // Get subsector index for BSP polygon lookup
        let subsector_idx = map
            .subsectors()
            .iter()
            .position(|s| std::ptr::eq(s, subsector))
            .unwrap_or(0);

        // Get pre-triangulated floor and ceiling data
        let triangles = self.get_subsector_triangles(subsector_idx);
        let sector = &subsector.sector;

        let light = subsector.sector.lightlevel >> 4;
        let scale = 5;

        self.render_flat(
            light,
            scale,
            &triangles,
            sector.floorheight,
            sector.floorpic,
            pic_data,
            rend,
        );

        self.render_flat(
            light,
            scale,
            &triangles,
            sector.ceilingheight,
            sector.ceilingpic,
            pic_data,
            rend,
        );
    }

    fn render_flat(
        &mut self,
        light: usize,
        scale: usize,
        triangles: &[Triangle],
        sector_height: f32,
        sector_pic: usize,
        pic_data: &mut PicData,
        rend: &mut impl RenderTrait,
    ) {
        #[cfg(feature = "hprof")]
        profile!("render_flat");
        if sector_pic != pic_data.sky_num() {
            let colour = pic_data.get_flat_average_color(light, scale, sector_pic);
            for triangle in triangles {
                // TODO: prebuild this
                // Create 3D vertices at required height height
                let vertices = triangle
                    .vertices
                    .iter()
                    .map(|v| Vec3::new(v.x, v.y, sector_height))
                    .collect();

                let poly = Polygon3D {
                    vertices,
                    color: colour,
                };

                if let Some(view_poly) = poly.transform(&self.view_matrix).project(
                    &self.projection_matrix,
                    self.width as f32,
                    self.height as f32,
                ) {
                    self.draw_polygon_with_occlusion(rend, &view_poly);
                }
            }
        }
    }

    /// Traverse BSP tree and render visible segments in front-to-back order
    fn render_bsp_node(
        &mut self,
        map: &MapData,
        rend: &mut impl RenderTrait,
        node_id: u32,
        player_pos: Vec2,
        player_sector: &SubSector,
        pic_data: &mut PicData,
    ) {
        if node_id & IS_SSECTOR_MASK != 0 {
            // It's a subsector
            let subsector_id = if node_id == u32::MAX {
                0
            } else {
                (node_id & !IS_SSECTOR_MASK) as usize
            };

            if subsector_id < map.subsectors().len() {
                let subsector = &map.subsectors()[subsector_id];

                // TODO: we need to rebuild this for maps at map load. It works but not well
                // let s1 = player_sector.sector.num;
                // let s2 = subsector.sector.num;
                // // self.level().
                // let pnum = s1 * 1 + s2;
                // let bytenum = pnum >> 3;
                // let bitnum = 1 << (pnum & 7);

                // if !map.get_devils_rejects().is_empty() {
                //     if map.get_devils_rejects()[bytenum as usize] & bitnum != 0 {
                //         // println!("REJECTED");
                //         return;
                //     }
                // }

                self.render_subsector(map, rend, subsector, player_pos, pic_data);
            }
            return;
        }

        // It's a node - determine which side the player is on
        if let Some(node) = map.get_nodes().get(node_id as usize) {
            let side = node.point_on_side(&player_pos);

            // Render front side first (closer to player)
            self.render_bsp_node(
                map,
                rend,
                node.children[side],
                player_pos,
                player_sector,
                pic_data,
            );

            // Check if back side bounding box is in view
            if self.bbox_in_view(node, player_pos, side ^ 1) {
                // Render back side
                self.render_bsp_node(
                    map,
                    rend,
                    node.children[side ^ 1],
                    player_pos,
                    player_sector,
                    pic_data,
                );
            }
        }
    }

    /// Main rendering function
    ///
    /// Rendering pipeline:
    /// 1. Update view matrix based on player position/orientation
    /// 2. Clear framebuffer
    /// 3. Iterate through all level segments
    /// 4. Cull back-facing segments
    /// 5. Project and render visible segments
    pub fn render_player_view(
        &mut self,
        player: &Player,
        level: &Level,
        pic_data: &mut PicData,
        rend: &mut impl RenderTrait,
    ) {
        #[cfg(feature = "hprof")]
        profile!("render_player_view");
        self.update_view_matrix(player);
        // TODO: make this an option
        rend.draw_buffer().clear_with_colour(&[0, 0, 0, 255]);

        // Generate BSP polygons for all subsectors (once)
        if self.map_name != level.map_name {
            self.bsp_polygons.generate_polygons(&level.map_data);
            self.map_name = level.map_name.clone();
        }

        self.occlusion_buffer.reset();

        let player_pos = if let Some(mobj) = player.mobj() {
            mobj.xy
        } else {
            return; // No player object, can't render
        };

        let player_sector = player.mobj().unwrap().subsector.clone();
        // Render using BSP traversal for proper front-to-back ordering
        self.render_bsp_node(
            &level.map_data,
            rend,
            level.map_data.start_node(),
            player_pos,
            &player_sector,
            pic_data,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gameplay::*;
    use glam::Vec2;

    #[test]
    fn test_renderer_creation() {
        let renderer = Renderer3D::new(640.0, 480.0, 90.0);
        assert_eq!(renderer.width, 640);
        assert_eq!(renderer.height, 480);
    }

    #[test]
    fn test_viewheight_integration() {
        let renderer = Renderer3D::new(640.0, 480.0, 90.0);
        let initial_view = renderer.view_matrix;
        assert_eq!(initial_view, Mat4::IDENTITY);
    }

    #[test]
    fn test_line_clipping() {
        let renderer = Renderer3D::new(640.0, 480.0, 90.0);

        // Test line completely inside viewport
        let p1 = Vec2::new(100.0, 100.0);
        let p2 = Vec2::new(200.0, 200.0);
        let result = renderer.clip_line(p1, p2);
        assert!(result.is_some());
        let (clipped1, clipped2) = result.unwrap();
        assert_eq!(clipped1, p1);
        assert_eq!(clipped2, p2);

        // Test line completely outside viewport
        let p1 = Vec2::new(-100.0, -100.0);
        let p2 = Vec2::new(-50.0, -50.0);
        let result = renderer.clip_line(p1, p2);
        assert!(result.is_none());

        // Test line crossing viewport boundary
        let p1 = Vec2::new(-50.0, 100.0);
        let p2 = Vec2::new(100.0, 100.0);
        let result = renderer.clip_line(p1, p2);
        assert!(result.is_some());
        let (clipped1, clipped2) = result.unwrap();
        assert_eq!(clipped1.x, 0.0);
        assert_eq!(clipped1.y, 100.0);
        assert_eq!(clipped2, p2);
    }

    #[test]
    fn test_outcode_computation() {
        let renderer = Renderer3D::new(640.0, 480.0, 90.0);
        // Point inside viewport
        assert_eq!(renderer.compute_outcode(320.0, 240.0), 0);
        // Point above viewport
        assert_eq!(renderer.compute_outcode(320.0, 500.0), 1);
        // Point below viewport
        assert_eq!(renderer.compute_outcode(320.0, -10.0), 2);
    }

    #[test]
    fn test_render_mode() {
        let mut renderer = Renderer3D::new(640.0, 480.0, 90.0);
        // Default should be filled mode
        assert!(renderer.is_render_filled());
        // Test setting wireframe mode
        renderer.set_render_filled(false);
        assert!(!renderer.is_render_filled());
        // Test setting back to filled
        renderer.set_render_filled(true);
        assert!(renderer.is_render_filled());
    }

    #[test]
    fn test_outcode_computation_extended() {
        let renderer = Renderer3D::new(640.0, 480.0, 90.0);
        // Point to the right of viewport
        assert_eq!(renderer.compute_outcode(700.0, 240.0), 4);
        // Point to the left of viewport
        assert_eq!(renderer.compute_outcode(-10.0, 240.0), 8);
    }

    #[test]
    fn test_triangulation_integration() {
        let renderer = Renderer3D::new(640.0, 480.0, 90.0);
        // Test that triangulation data is empty initially
        let triangles = renderer.get_subsector_triangles(0);
        assert!(triangles.is_empty());
        // Test that the method handles invalid indices gracefully
        let triangles = renderer.get_subsector_triangles(999);
        assert!(triangles.is_empty());
    }
}
