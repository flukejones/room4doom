#[cfg(feature = "hprof")]
use coarse_prof::profile;
use gameplay::{Level, MapData, Node, PicData, Player, Segment, SubSector};
use glam::{Mat4, Vec2, Vec3};
use render_trait::{PixelBuffer, RenderTrait};

use std::f32::consts::PI;

mod bsp_polygon;
mod depth_buffer;
mod polygon;
use bsp_polygon::{BSPPolygons, Triangle};
use depth_buffer::DepthBuffer;
use polygon::{Polygon2D, Polygon3D, segment_to_polygons};

const IS_SSECTOR_MASK: u32 = 0x8000_0000;

/// A 3D software renderer for Doom levels.
///
/// This renderer displays the level geometry in true 3D space,
/// showing floors, ceilings, walls with different colors.
///
/// Features depth buffer optimization for improved performance by testing
/// polygon visibility before expensive occlusion calculations.
pub struct Renderer3D {
    width: u32,
    height: u32,
    width_minus_one: f32,
    height_minus_one: f32,
    fov: f32,
    view_matrix: Mat4,
    projection_matrix: Mat4,
    depth_buffer: DepthBuffer,
    intersection_buffer: Vec<f32>,
    map_name: String,
    bsp_polygons: BSPPolygons,
    render_filled: bool,
    near_z: f32,
    far_z: f32,
    vertex_depths: [f32; 3],
}

impl Renderer3D {
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
        let near = 0.01;
        let far = 10000.0;

        Self {
            width: width as u32,
            height: height as u32,
            width_minus_one: width - 1.0,
            height_minus_one: height - 1.0,
            fov,
            view_matrix: Mat4::IDENTITY,
            projection_matrix: Mat4::perspective_rh_gl(fov, aspect, near, far),
            depth_buffer: DepthBuffer::new(width as usize, height as usize),
            intersection_buffer: Vec::with_capacity(256), // Pre-allocate for polygon intersections
            map_name: String::new(),
            bsp_polygons: BSPPolygons::new(),
            render_filled: true,
            near_z: near,
            far_z: far,
            vertex_depths: [0.0; 3],
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
        self.projection_matrix = Mat4::perspective_rh_gl(self.fov, aspect, self.near_z, self.far_z);

        // Resize depth buffer
        self.depth_buffer.resize(width as usize, height as usize);

        // Set view bounds for clipping
        self.depth_buffer.set_view_bounds(0.0, width, 0.0, height);
    }

    /// Sets the field of view and updates the projection matrix.
    pub fn set_fov(&mut self, fov: f32) {
        self.fov = fov;
        let aspect = self.width as f32 / self.height as f32;
        self.projection_matrix = Mat4::perspective_rh_gl(fov, aspect, self.near_z, self.far_z);
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

        // Check if ALL corners are outside any single frustum plane
        // Only cull if the entire bbox is completely outside the frustum

        // Near plane check (behind camera)
        if view_corners.iter().all(|p| p.z > -self.near_z) {
            return false;
        }

        // Far plane check
        if view_corners.iter().all(|p| p.z < -self.far_z) {
            return false;
        }

        // Calculate frustum plane parameters for X/Y culling
        let aspect = self.width as f32 / self.height as f32;
        let half_fov_y = self.fov / 2.0;
        let tan_half_fov_y = half_fov_y.tan();
        let tan_half_fov_x = aspect * tan_half_fov_y;

        // Left plane check - all points outside left edge
        if view_corners.iter().all(|p| p.x < p.z * tan_half_fov_x) {
            return false;
        }
        // Right plane check - all points outside right edge
        if view_corners.iter().all(|p| p.x > -p.z * tan_half_fov_x) {
            return false;
        }
        // Bottom plane check - all points outside bottom edge
        if view_corners.iter().all(|p| p.y < p.z * tan_half_fov_y) {
            return false;
        }
        // Top plane check - all points outside top edge
        if view_corners.iter().all(|p| p.y > -p.z * tan_half_fov_y) {
            return false;
        }

        true
    }

    // ==========================================
    // RENDERING PRIMITIVES
    // ==========================================

    /// Helper method to render polygon with optional depth buffer testing
    fn render_polygon_with_depth_test(
        &mut self,
        rend: &mut impl RenderTrait,
        view_poly: &Polygon3D,
        screen_poly: &Polygon2D,
    ) {
        self.vertex_depths = [
            view_poly.vertices[0].z,
            view_poly.vertices[1].z,
            view_poly.vertices[2].z,
        ];

        if self
            .depth_buffer
            .is_polygon_potentially_visible(&screen_poly.vertices, &self.vertex_depths)
        {
            // Draw polygon with occlusion
            self.draw_polygon(rend, screen_poly);
            // Update depth buffer with this polygon's depth
            // TODO: this is working like an occlusion buffer for now
            // self.depth_buffer
            //     .update_polygon_depth(&screen_poly.vertices, &depths);
        }
    }

    /// Draw polygon with span-based occlusion
    fn draw_polygon(&mut self, rend: &mut impl RenderTrait, poly: &Polygon2D) {
        #[cfg(feature = "hprof")]
        profile!("draw_polygon");
        // Check if polygon is completely outside screen bounds
        if let Some((min, max)) = poly.bounds() {
            if min.x > self.width_minus_one
                || max.x < 0.0
                || min.y > self.height_minus_one
                || max.y < 0.0
            {
                return; // Skip if entirely outside
            }

            if self.render_filled {
                self.draw_filled(rend, poly);
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
                        self.draw_line(rend, clipped_v1, clipped_v2, poly.color);
                    }
                }
            }
        } else {
            return;
        }
    }

    /// Draw filled polygon using scanline algorithm
    fn draw_filled(&mut self, rend: &mut impl RenderTrait, poly: &Polygon2D) {
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
                        // Draw the entire column segment - depth buffer handles occlusion
                        let y_start = column_y_min.ceil() as i32;
                        let y_end = column_y_max.floor() as i32;

                        for y in y_start..=y_end {
                            if y >= 0 && y < self.height as i32 {
                                let default_depth = 0.0; // Default depth for 2D polygons
                                // Test depth buffer for visibility
                                if self.depth_buffer.is_point_visible(
                                    x as f32,
                                    y as f32,
                                    default_depth,
                                ) {
                                    rend.draw_buffer().set_pixel(
                                        x as usize,
                                        y as usize,
                                        &poly.color,
                                    );

                                    // Update depth buffer
                                    self.depth_buffer.set_depth_unchecked(
                                        x as usize,
                                        y as usize,
                                        default_depth,
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
    fn draw_line(&mut self, rend: &mut impl RenderTrait, p1: Vec2, p2: Vec2, color: [u8; 4]) {
        #[cfg(feature = "hprof")]
        profile!("draw_line");
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

        // Draw pixels based on depth buffer visibility
        let line_depth = 0.0; // Default depth for 2D lines
        for (x, y) in pixels {
            // Check if this pixel is visible in depth buffer
            if self
                .depth_buffer
                .is_point_visible(x as f32, y as f32, line_depth)
            {
                rend.draw_buffer().set_pixel(x as usize, y as usize, &color);
                // Update depth buffer with this pixel
                self.depth_buffer
                    .set_depth_unchecked(x as usize, y as usize, line_depth);
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
        if self.is_segment_front_facing(seg, player_pos) {
            return;
        }
        let polygons = segment_to_polygons(seg, pic_data);

        for poly in polygons {
            let view_poly = poly.transform(&self.view_matrix);

            let mut any_in_front = false;
            for v in &view_poly.vertices {
                if v.z < self.near_z {
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
                self.near_z,
            ) {
                self.render_polygon_with_depth_test(rend, &view_poly, &screen_poly);
            }
        }
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

        let colour = if sector_pic == pic_data.sky_num() {
            [32, 16, 16, 255]
        } else {
            pic_data.get_flat_average_color(light, scale, sector_pic)
        };

        for triangle in triangles {
            // Create 3D vertices at required height height
            let vertices = [
                Vec3::new(
                    triangle.vertices[0].x,
                    triangle.vertices[0].y,
                    sector_height,
                ),
                Vec3::new(
                    triangle.vertices[1].x,
                    triangle.vertices[1].y,
                    sector_height,
                ),
                Vec3::new(
                    triangle.vertices[2].x,
                    triangle.vertices[2].y,
                    sector_height,
                ),
            ];

            let poly = Polygon3D {
                vertices,
                color: colour,
            };

            let view_poly = poly.transform(&self.view_matrix);

            let mut any_in_front = false;
            for v in &view_poly.vertices {
                if v.z < self.near_z {
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
                self.near_z,
            ) {
                self.render_polygon_with_depth_test(rend, &view_poly, &screen_poly);
            }
        }
    }

    fn render_flats(
        &mut self,
        map: &MapData,
        rend: &mut impl RenderTrait,
        subsector: &SubSector,
        pic_data: &mut PicData,
    ) {
        let subsector_idx = map
            .subsectors()
            .iter()
            .position(|s| std::ptr::eq(s, subsector))
            .unwrap_or(0);

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
            self.render_flats(map, rend, subsector, pic_data);
            for seg in segments {
                let front_sector = seg.frontsector.clone();
                if let Some(back_sector) = seg.backsector.clone() {
                    // Doors. Block view
                    if back_sector.ceilingheight <= front_sector.floorheight
                        || back_sector.floorheight >= front_sector.ceilingheight
                        || back_sector.ceilingheight != front_sector.ceilingheight
                        || back_sector.floorheight != front_sector.floorheight
                    {
                        self.render_segment(rend, seg, player_pos, pic_data);
                        continue;
                    }
                    // Reject empty lines used for triggers and special events.
                    // Identical floor and ceiling on both sides, identical light levels
                    // on both sides, and no middle texture.
                    if back_sector.ceilingpic == front_sector.ceilingpic
                        && back_sector.floorpic == front_sector.floorpic
                        && back_sector.lightlevel == front_sector.lightlevel
                        && seg.sidedef.midtexture.is_none()
                    {
                        continue;
                    }
                    // self.render_segment(rend, seg, player_pos, pic_data);
                    continue;
                }
                self.render_segment(rend, seg, player_pos, pic_data);
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
    /// 2. Clear framebuffer (or not)
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
        if !self.render_filled {
            rend.draw_buffer().clear_with_colour(&[0, 0, 0, 255]);
        }

        // Generate BSP polygons for all subsectors (once)
        if self.map_name != level.map_name {
            self.bsp_polygons.generate_polygons(&level.map_data);
            self.map_name = level.map_name.clone();
        }

        self.depth_buffer.reset();

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

    #[test]
    fn test_bbox_culling_tall_sectors() {
        use gameplay::Node;
        use glam::Vec2;

        let mut renderer = Renderer3D::new(640.0, 480.0, 90.0f32.to_radians());

        // Test extremely tall sector (like a skyscraper)
        let node = Node {
            xy: Vec2::new(0.0, 0.0),
            delta: Vec2::new(1.0, 0.0),
            bboxes: [
                [Vec2::new(-100.0, -100.0), Vec2::new(100.0, 100.0)],
                [Vec2::new(-50.0, -50.0), Vec2::new(50.0, 50.0)],
            ],
            children: [0, 1],
            min_z: 0.0,     // Floor
            max_z: 10000.0, // Very tall ceiling
        };

        // Player at ground level looking straight ahead
        renderer.view_matrix = glam::Mat4::look_at_rh(
            glam::Vec3::new(0.0, -200.0, 100.0), // Player position
            glam::Vec3::new(0.0, -100.0, 100.0), // Look at
            glam::Vec3::Z,
        );

        // Should be visible even with extreme height
        assert!(renderer.bbox_in_view(&node, Vec2::new(0.0, -200.0), 0));
    }

    #[test]
    fn test_bbox_culling_looking_down() {
        use gameplay::Node;
        use glam::Vec2;

        let mut renderer = Renderer3D::new(640.0, 480.0, 90.0f32.to_radians());

        // Normal height sector at ground level
        let node = Node {
            xy: Vec2::new(0.0, 0.0),
            delta: Vec2::new(1.0, 0.0),
            bboxes: [
                [Vec2::new(-50.0, -50.0), Vec2::new(50.0, 50.0)],
                [Vec2::new(-25.0, -25.0), Vec2::new(25.0, 25.0)],
            ],
            children: [0, 1],
            min_z: 0.0,   // Floor
            max_z: 100.0, // Ceiling
        };

        // Player high up looking down at 45 degrees
        let eye_pos = glam::Vec3::new(0.0, -100.0, 200.0);
        let look_at = glam::Vec3::new(0.0, 0.0, 0.0); // Looking down at origin
        renderer.view_matrix = glam::Mat4::look_at_rh(eye_pos, look_at, glam::Vec3::Z);

        // Should be visible when looking down
        assert!(renderer.bbox_in_view(&node, Vec2::new(0.0, -100.0), 0));
    }

    #[test]
    fn test_bbox_culling_looking_up() {
        use gameplay::Node;
        use glam::Vec2;

        let mut renderer = Renderer3D::new(640.0, 480.0, 90.0f32.to_radians());

        // High sector (like a floating platform)
        let node = Node {
            xy: Vec2::new(0.0, 0.0),
            delta: Vec2::new(1.0, 0.0),
            bboxes: [
                [Vec2::new(-50.0, -50.0), Vec2::new(50.0, 50.0)],
                [Vec2::new(-25.0, -25.0), Vec2::new(25.0, 25.0)],
            ],
            children: [0, 1],
            min_z: 500.0, // High floor
            max_z: 600.0, // High ceiling
        };

        // Player at ground level looking up
        let eye_pos = glam::Vec3::new(0.0, -100.0, 50.0);
        let look_at = glam::Vec3::new(0.0, 0.0, 550.0); // Looking up at platform
        renderer.view_matrix = glam::Mat4::look_at_rh(eye_pos, look_at, glam::Vec3::Z);

        // Should be visible when looking up
        assert!(renderer.bbox_in_view(&node, Vec2::new(0.0, -100.0), 0));
    }

    #[test]
    fn test_bbox_culling_player_inside_bbox() {
        use gameplay::Node;
        use glam::Vec2;

        let mut renderer = Renderer3D::new(640.0, 480.0, 90.0f32.to_radians());

        // Large sector that contains the player
        let node = Node {
            xy: Vec2::new(0.0, 0.0),
            delta: Vec2::new(1.0, 0.0),
            bboxes: [
                [Vec2::new(-200.0, -200.0), Vec2::new(200.0, 200.0)],
                [Vec2::new(-100.0, -100.0), Vec2::new(100.0, 100.0)],
            ],
            children: [0, 1],
            min_z: 0.0,
            max_z: 300.0,
        };

        // Player inside the bounding box
        renderer.view_matrix = glam::Mat4::look_at_rh(
            glam::Vec3::new(0.0, 0.0, 150.0), // Inside the bbox
            glam::Vec3::new(1.0, 0.0, 150.0), // Looking forward
            glam::Vec3::Z,
        );

        // Should always be visible when player is inside
        assert!(renderer.bbox_in_view(&node, Vec2::new(0.0, 0.0), 0));
    }

    #[test]
    fn test_bbox_culling_extreme_pitch() {
        use gameplay::Node;
        use glam::Vec2;

        let mut renderer = Renderer3D::new(640.0, 480.0, 90.0f32.to_radians());

        // Sector directly below player
        let node = Node {
            xy: Vec2::new(0.0, 0.0),
            delta: Vec2::new(1.0, 0.0),
            bboxes: [
                [Vec2::new(-10.0, -10.0), Vec2::new(10.0, 10.0)],
                [Vec2::new(-5.0, -5.0), Vec2::new(5.0, 5.0)],
            ],
            children: [0, 1],
            min_z: 0.0,
            max_z: 10.0,
        };

        // Player looking straight down (90-degree pitch)
        let eye_pos = glam::Vec3::new(0.0, 0.0, 100.0);
        let look_at = glam::Vec3::new(0.0, 0.0, 0.0); // Straight down
        renderer.view_matrix = glam::Mat4::look_at_rh(eye_pos, look_at, glam::Vec3::Y);

        // Should be visible even with extreme pitch
        assert!(renderer.bbox_in_view(&node, Vec2::new(0.0, 0.0), 0));
    }

    #[test]
    fn test_bbox_culling_near_plane_edge_cases() {
        use gameplay::Node;
        use glam::Vec2;

        let mut renderer = Renderer3D::new(640.0, 480.0, 90.0f32.to_radians());

        // Very close sector
        let node = Node {
            xy: Vec2::new(0.0, 0.0),
            delta: Vec2::new(1.0, 0.0),
            bboxes: [
                [Vec2::new(-5.0, 0.5), Vec2::new(5.0, 2.0)], // Very close to camera
                [Vec2::new(-2.0, 0.5), Vec2::new(2.0, 1.5)],
            ],
            children: [0, 1],
            min_z: 0.0,
            max_z: 100.0,
        };

        // Player looking forward
        renderer.view_matrix = glam::Mat4::look_at_rh(
            glam::Vec3::new(0.0, 0.0, 50.0),
            glam::Vec3::new(0.0, 10.0, 50.0),
            glam::Vec3::Z,
        );

        // Should handle near plane correctly (near_z = 1.0)
        let result = renderer.bbox_in_view(&node, Vec2::new(0.0, 0.0), 0);
        // Result depends on exact near plane handling, but shouldn't crash
        println!("Near plane test result: {}", result);
    }

    #[test]
    fn test_bbox_culling_zero_height_sector() {
        use gameplay::Node;
        use glam::Vec2;

        let mut renderer = Renderer3D::new(640.0, 480.0, 90.0f32.to_radians());

        // Degenerate sector with zero height
        let node = Node {
            xy: Vec2::new(0.0, 0.0),
            delta: Vec2::new(1.0, 0.0),
            bboxes: [
                [Vec2::new(-50.0, -50.0), Vec2::new(50.0, 50.0)],
                [Vec2::new(-25.0, -25.0), Vec2::new(25.0, 25.0)],
            ],
            children: [0, 1],
            min_z: 100.0,
            max_z: 100.0, // Same as min_z - zero height
        };

        renderer.view_matrix = glam::Mat4::look_at_rh(
            glam::Vec3::new(0.0, -100.0, 100.0),
            glam::Vec3::new(0.0, 0.0, 100.0),
            glam::Vec3::Z,
        );

        // Should handle zero-height sectors gracefully
        let result = renderer.bbox_in_view(&node, Vec2::new(0.0, -100.0), 0);
        println!("Zero height sector result: {}", result);
    }

    #[test]
    fn test_bbox_culling_skyscraper_from_ground() {
        use gameplay::Node;
        use glam::Vec2;

        let mut renderer = Renderer3D::new(640.0, 480.0, 90.0f32.to_radians());

        // Extremely tall skyscraper sector
        let node = Node {
            xy: Vec2::new(0.0, 0.0),
            delta: Vec2::new(1.0, 0.0),
            bboxes: [
                [Vec2::new(-50.0, 50.0), Vec2::new(50.0, 150.0)], // Distance ahead
                [Vec2::new(-25.0, 25.0), Vec2::new(25.0, 75.0)],
            ],
            children: [0, 1],
            min_z: 0.0,     // Ground level
            max_z: 50000.0, // Extremely tall
        };

        // Player at ground level looking straight ahead
        renderer.view_matrix = glam::Mat4::look_at_rh(
            glam::Vec3::new(0.0, 0.0, 10.0),   // Player at ground
            glam::Vec3::new(0.0, 100.0, 10.0), // Looking forward
            glam::Vec3::Z,
        );

        // Should be visible - but current logic might fail due to top bbox corners
        let result = renderer.bbox_in_view(&node, Vec2::new(0.0, 0.0), 0);
        println!("Skyscraper visibility: {}", result);
        assert!(result, "Skyscraper should be visible when looking ahead");
    }

    #[test]
    fn test_bbox_culling_deep_pit_looking_down() {
        use gameplay::Node;
        use glam::Vec2;

        let mut renderer = Renderer3D::new(640.0, 480.0, 90.0f32.to_radians());

        // Deep underground sector
        let node = Node {
            xy: Vec2::new(0.0, 0.0),
            delta: Vec2::new(1.0, 0.0),
            bboxes: [
                [Vec2::new(-100.0, -10.0), Vec2::new(100.0, 10.0)], // Right below player
                [Vec2::new(-50.0, -5.0), Vec2::new(50.0, 5.0)],
            ],
            children: [0, 1],
            min_z: -10000.0, // Very deep
            max_z: -100.0,   // Still underground
        };

        // Player high up looking down at 60 degrees
        let eye_pos = glam::Vec3::new(0.0, -50.0, 1000.0);
        let look_at = glam::Vec3::new(0.0, 0.0, -5000.0); // Looking down into pit
        renderer.view_matrix = glam::Mat4::look_at_rh(eye_pos, look_at, glam::Vec3::Z);

        // Should be visible when looking down
        let result = renderer.bbox_in_view(&node, Vec2::new(0.0, -50.0), 0);
        println!("Deep pit visibility: {}", result);
        assert!(result, "Deep pit should be visible when looking down");
    }

    #[test]
    fn test_bbox_culling_tall_sector_partial_view() {
        use gameplay::Node;
        use glam::Vec2;

        let mut renderer = Renderer3D::new(640.0, 480.0, 45.0f32.to_radians()); // Narrower FOV

        // Tall sector where only middle section should be visible
        let node = Node {
            xy: Vec2::new(0.0, 0.0),
            delta: Vec2::new(1.0, 0.0),
            bboxes: [
                [Vec2::new(-20.0, 80.0), Vec2::new(20.0, 120.0)], // Close distance
                [Vec2::new(-10.0, 90.0), Vec2::new(10.0, 110.0)],
            ],
            children: [0, 1],
            min_z: -500.0, // Floor far below view
            max_z: 2000.0, // Ceiling far above view
        };

        // Player at middle height looking forward
        renderer.view_matrix = glam::Mat4::look_at_rh(
            glam::Vec3::new(0.0, 0.0, 100.0),   // Middle height
            glam::Vec3::new(0.0, 100.0, 100.0), // Looking straight ahead
            glam::Vec3::Z,
        );

        // Should be visible even though top/bottom extend beyond view
        let result = renderer.bbox_in_view(&node, Vec2::new(0.0, 0.0), 0);
        println!("Partial tall sector visibility: {}", result);
        assert!(
            result,
            "Tall sector should be visible even if top/bottom are outside view"
        );
    }

    #[test]
    fn test_bbox_culling_extreme_angle_tall_sector() {
        use gameplay::Node;
        use glam::Vec2;

        let mut renderer = Renderer3D::new(640.0, 480.0, 90.0f32.to_radians());

        // Very tall sector at an angle
        let node = Node {
            xy: Vec2::new(0.0, 0.0),
            delta: Vec2::new(1.0, 0.0),
            bboxes: [
                [Vec2::new(80.0, 80.0), Vec2::new(120.0, 120.0)], // Diagonal from player
                [Vec2::new(90.0, 90.0), Vec2::new(110.0, 110.0)],
            ],
            children: [0, 1],
            min_z: 0.0,
            max_z: 8000.0, // Very tall
        };

        // Player looking diagonally up at the sector
        let eye_pos = glam::Vec3::new(0.0, 0.0, 50.0);
        let look_at = glam::Vec3::new(100.0, 100.0, 4000.0); // Looking diagonally up
        renderer.view_matrix = glam::Mat4::look_at_rh(eye_pos, look_at, glam::Vec3::Z);

        // Should be visible at this angle
        let result = renderer.bbox_in_view(&node, Vec2::new(0.0, 0.0), 0);
        println!("Diagonal tall sector visibility: {}", result);
        assert!(
            result,
            "Tall sector should be visible when looking diagonally up"
        );
    }

    #[test]
    fn test_bbox_culling_player_at_different_heights() {
        use gameplay::Node;
        use glam::Vec2;

        let mut renderer = Renderer3D::new(640.0, 480.0, 90.0f32.to_radians());

        // Multi-story building sector
        let node = Node {
            xy: Vec2::new(0.0, 0.0),
            delta: Vec2::new(1.0, 0.0),
            bboxes: [
                [Vec2::new(-30.0, 70.0), Vec2::new(30.0, 130.0)],
                [Vec2::new(-15.0, 85.0), Vec2::new(15.0, 115.0)],
            ],
            children: [0, 1],
            min_z: 0.0,   // Ground floor
            max_z: 800.0, // 8 story building
        };

        // Test from multiple player heights
        let test_heights = [50.0, 200.0, 400.0, 600.0, 750.0];

        for height in test_heights {
            renderer.view_matrix = glam::Mat4::look_at_rh(
                glam::Vec3::new(0.0, 0.0, height),
                glam::Vec3::new(0.0, 100.0, height), // Looking forward at same height
                glam::Vec3::Z,
            );

            let result = renderer.bbox_in_view(&node, Vec2::new(0.0, 0.0), 0);
            println!("Building visibility from height {}: {}", height, result);
            assert!(result, "Building should be visible from height {}", height);
        }
    }
}
