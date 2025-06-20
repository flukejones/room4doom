use gameplay::{Level, MapData, MapPtr, Node, PicData, Player, Sector, Segment, SubSector};
use glam::{Mat4, Vec2, Vec3, Vec4};
use render_trait::{PixelBuffer, PlayViewRenderer, RenderTrait};

use std::f32::consts::PI;

mod polygon;
use polygon::{Polygon2D, Polygon3D, PolygonType, PortalWindow};

const IS_SSECTOR_MASK: u32 = 0x8000_0000;

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
    /// Tracks occlusion using spans
    occlusion_buffer: OcclusionBuffer,
    /// Stack of portal windows for recursive portal rendering
    portal_stack: Vec<PortalWindow>,
}

/// Tracks occlusion using horizontal spans
#[derive(Clone, Debug)]
struct OcclusionBuffer {
    /// For each X column, track multiple occluded spans to handle portals
    spans: Vec<Vec<(f32, f32)>>, // List of (top_y, bottom_y) spans per column
}

impl OcclusionBuffer {
    fn new(width: usize) -> Self {
        Self {
            spans: vec![Vec::new(); width],
        }
    }

    fn is_column_fully_occluded(&self, x: usize) -> bool {
        if x >= self.spans.len() {
            return false;
        }
        // Check if any span covers the entire screen height
        for (top, bottom) in &self.spans[x] {
            if *top <= 0.0 && *bottom >= self.spans.len() as f32 {
                return true;
            }
        }
        false
    }

    fn update_span(&mut self, x: usize, top: f32, bottom: f32) {
        if x >= self.spans.len() {
            return;
        }

        // Add new span to the list
        self.spans[x].push((top, bottom));

        // Merge overlapping spans
        if self.spans[x].len() > 1 {
            self.spans[x].sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            let mut merged = Vec::new();
            let mut current = self.spans[x][0];

            for &(t, b) in &self.spans[x][1..] {
                if t <= current.1 {
                    // Overlapping or adjacent spans - merge them
                    current.1 = current.1.max(b);
                } else {
                    // Non-overlapping span - save current and start new
                    merged.push(current);
                    current = (t, b);
                }
            }
            merged.push(current);
            self.spans[x] = merged;
        }
    }

    fn is_point_occluded(&self, x: usize, y: f32) -> bool {
        if x >= self.spans.len() {
            return false;
        }
        // Check if point is within any occluded span
        for (top, bottom) in &self.spans[x] {
            if y >= *top && y <= *bottom {
                return true;
            }
        }
        false
    }
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
            occlusion_buffer: OcclusionBuffer::new(width as usize),
            portal_stack: Vec::new(),
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
        // Resize occlusion buffer
        self.occlusion_buffer = OcclusionBuffer::new(width as usize);
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

    /// Convert a segment into 3D polygons based on floor/ceiling heights
    fn segment_to_polygons(&self, seg: &Segment, pic_data: &PicData) -> Vec<Polygon3D> {
        let mut polygons = Vec::new();

        let v1 = seg.v1;
        let v2 = seg.v2;
        let front_floor = seg.frontsector.floorheight;
        let front_ceiling = seg.frontsector.ceilingheight;

        if let Some(back_sector) = &seg.backsector {
            // Two-sided line - may have upper wall, lower wall, and portal
            let back_floor = back_sector.floorheight;
            let back_ceiling = back_sector.ceilingheight;

            // Lower wall (step up) - if back floor is higher than front floor
            if back_floor > front_floor {
                polygons.push(Polygon3D::from_wall_segment(
                    v1,
                    v2,
                    front_floor,
                    back_floor,
                    if let Some(t) = seg.sidedef.bottomtexture {
                        pic_data.get_texture_average_color(t)
                    } else {
                        [128, 128, 128, 255]
                    }, // Gray
                    PolygonType::LowerWall,
                ));
            }

            // Upper wall (overhead) - if back ceiling is lower than front ceiling
            if back_ceiling < front_ceiling {
                polygons.push(Polygon3D::from_wall_segment(
                    v1,
                    v2,
                    back_ceiling,
                    front_ceiling,
                    if let Some(t) = seg.sidedef.toptexture {
                        pic_data.get_texture_average_color(t)
                    } else {
                        [64, 64, 64, 255]
                    }, // Dark gray
                    PolygonType::UpperWall,
                ));
            }

            // Portal opening - no polygon needed, just used for clipping
            // The portal area is defined by the gap between upper and lower walls
        } else {
            // One-sided line - solid wall from floor to ceiling
            polygons.push(Polygon3D::from_wall_segment(
                v1,
                v2,
                front_floor,
                front_ceiling,
                if let Some(t) = seg.sidedef.midtexture {
                    pic_data.get_texture_average_color(t)
                } else {
                    [255, 255, 255, 255]
                }, // White for solid walls
                PolygonType::Wall,
            ));
        }

        polygons
    }

    /// Draw polygon with span-based occlusion
    fn draw_polygon_with_occlusion(&mut self, buffer: &mut impl PixelBuffer, poly: &Polygon2D) {
        // Draw each edge of the polygon
        for i in 0..poly.vertices.len() {
            let v1 = poly.vertices[i];
            let v2 = poly.vertices[(i + 1) % poly.vertices.len()];

            // Clip line to screen bounds
            if let Some((clipped_v1, clipped_v2)) = self.clip_line(v1, v2) {
                // Draw the line with occlusion checking
                self.draw_line_with_occlusion(buffer, clipped_v1, clipped_v2, poly.color);
            }
        }

        // Update occlusion buffer for solid geometry
        if matches!(
            poly.polygon_type,
            PolygonType::Wall
                | PolygonType::LowerWall
                | PolygonType::UpperWall
                | PolygonType::Floor
                | PolygonType::Ceiling
        ) {
            if let Some((min, max)) = poly.bounds() {
                let x_start = min.x.max(0.0) as i32;
                let x_end = max.x.min(self.width - 1.0) as i32;

                for x in x_start..=x_end {
                    if x >= 0 && (x as usize) < self.occlusion_buffer.spans.len() {
                        // Find the Y range of the polygon at this X
                        let mut y_min = max.y;
                        let mut y_max = min.y;

                        // Check all edges
                        for i in 0..poly.vertices.len() {
                            let v1 = poly.vertices[i];
                            let v2 = poly.vertices[(i + 1) % poly.vertices.len()];

                            if (v1.x <= x as f32 && v2.x >= x as f32)
                                || (v2.x <= x as f32 && v1.x >= x as f32)
                            {
                                // Edge crosses this X column
                                let t = if (v2.x - v1.x).abs() > 0.001 {
                                    (x as f32 - v1.x) / (v2.x - v1.x)
                                } else {
                                    0.5
                                };
                                let y = v1.y + (v2.y - v1.y) * t.clamp(0.0, 1.0);
                                y_min = y_min.min(y);
                                y_max = y_max.max(y);
                            }
                        }

                        self.occlusion_buffer.update_span(x as usize, y_min, y_max);
                    }
                }
            }
        }
    }

    /// Draw line with occlusion checking
    fn draw_line_with_occlusion(
        &self,
        buffer: &mut impl PixelBuffer,
        p1: Vec2,
        p2: Vec2,
        color: [u8; 4],
    ) {
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

        loop {
            // Check occlusion before drawing
            if x >= 0 && y >= 0 && x < self.width as i32 && y < self.height as i32 {
                let x_idx = x as usize;
                if x_idx < self.occlusion_buffer.spans.len() {
                    // Only draw if pixel is not occluded
                    if !self.occlusion_buffer.is_point_occluded(x_idx, y as f32) {
                        buffer.set_pixel(x as usize, y as usize, &color);
                    }
                }
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
    }

    /// Tests if a line segment is facing towards the player
    ///
    /// Uses the cross product to determine line orientation:
    /// - Cross product of line direction and view direction
    /// - Positive result = line faces right relative to view
    /// - Negative result = line faces left relative to view
    ///
    /// In Doom's right-handed coordinate system:
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

        if y > self.height {
            code |= 1; // Above
        } else if y < 0.0 {
            code |= 2; // Below
        }

        if x > self.width {
            code |= 4; // Right
        } else if x < 0.0 {
            code |= 8; // Left
        }

        code
    }

    /// Cohen-Sutherland line clipping algorithm
    ///
    /// Clips line segment to viewport boundaries using outcodes:
    /// 1. Calculate outcodes for both endpoints
    /// 2. If both outcodes are 0, line is completely inside
    /// 3. If outcodes AND together != 0, line is completely outside
    /// 4. Otherwise, clip against viewport edges iteratively
    ///
    /// Benefits over vertex rejection:
    /// - Renders partial lines that cross viewport boundaries
    /// - Shows geometry that extends beyond screen edges
    /// - More accurate representation of level structure
    /// - Prevents "popping" when segments move in/out of view
    fn clip_line(&self, mut p1: Vec2, mut p2: Vec2) -> Option<(Vec2, Vec2)> {
        let mut outcode1 = self.compute_outcode(p1.x, p1.y);
        let mut outcode2 = self.compute_outcode(p2.x, p2.y);

        loop {
            if (outcode1 | outcode2) == 0 {
                // Both points inside viewport
                return Some((p1, p2));
            }

            if (outcode1 & outcode2) != 0 {
                // Both points on same side outside viewport
                return None;
            }

            // At least one point is outside; clip against viewport edges
            let outcode_out = if outcode1 != 0 { outcode1 } else { outcode2 };
            let mut x = 0.0;
            let mut y = 0.0;

            // Find intersection point with viewport boundary
            if (outcode_out & 1) != 0 {
                // Point is above viewport (y > height)
                x = p1.x + (p2.x - p1.x) * (self.height - p1.y) / (p2.y - p1.y);
                y = self.height;
            } else if (outcode_out & 2) != 0 {
                // Point is below viewport (y < 0)
                x = p1.x + (p2.x - p1.x) * (0.0 - p1.y) / (p2.y - p1.y);
                y = 0.0;
            } else if (outcode_out & 4) != 0 {
                // Point is to the right of viewport (x > width)
                y = p1.y + (p2.y - p1.y) * (self.width - p1.x) / (p2.x - p1.x);
                x = self.width;
            } else if (outcode_out & 8) != 0 {
                // Point is to the left of viewport (x < 0)
                y = p1.y + (p2.y - p1.y) * (0.0 - p1.x) / (p2.x - p1.x);
                x = 0.0;
            }

            // Replace the outside point with the intersection point
            if outcode_out == outcode1 {
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

    /// Renders a single line segment with portal height differences
    ///
    /// Doom portal rendering concept:
    /// - Each segment separates two sectors (front and back)
    /// - When sectors have different heights, we draw the height difference
    /// - This creates the illusion of steps, windows, doors, etc.
    ///
    /// Visual representation:
    /// ```text
    /// Back Sector    Front Sector
    ///     +-----+         +-----+
    ///     |  B  | <-seg-> |  F  |  Player
    ///     |     |         |     |     v
    ///     +-----+         +-----+
    /// ```
    ///
    /// If back.floor > front.floor, draw a step up
    /// If back.ceiling < front.ceiling, draw overhead geometry
    /// Returns (has_solid_wall, optional_portal_data)
    fn render_segment(
        &mut self,
        buffer: &mut impl PixelBuffer,
        seg: &Segment,
        player_pos: Vec2,
        pic_data: &mut PicData,
    ) -> (bool, Option<(Polygon2D, MapPtr<Sector>)>) {
        // Skip back-facing segments for performance
        if self.is_segment_front_facing(seg, player_pos) {
            return (false, None);
        }

        // Temporarily disable frustum culling for debugging
        /*
        if !self.is_segment_in_frustum(seg) {
            self.culling_stats.segments_culled_frustum += 1;
            return (false, None);
        }
        */

        // Convert segment to 3D polygons
        let polygons = self.segment_to_polygons(seg, pic_data);

        let mut has_solid_wall = false;

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
            if let Some(screen_poly) =
                view_poly.project(&self.projection_matrix, self.width, self.height)
            {
                // Apply portal clipping if we're looking through portals
                let clipped_poly = if !self.portal_stack.is_empty() {
                    let mut current = screen_poly.clone();
                    let mut fully_clipped = false;

                    for portal in &self.portal_stack {
                        if let Some(clipped) = portal.clip_polygon(&current) {
                            current = clipped;
                        } else {
                            // Completely clipped away by portal
                            fully_clipped = true;
                            break;
                        }
                    }

                    if fully_clipped {
                        continue;
                    }
                    current
                } else {
                    screen_poly
                };

                // Draw polygon with occlusion
                self.draw_polygon_with_occlusion(buffer, &clipped_poly);

                if matches!(
                    clipped_poly.polygon_type,
                    PolygonType::Wall | PolygonType::LowerWall | PolygonType::UpperWall
                ) {
                    has_solid_wall = true;
                }
            }
        }

        // Second pass: compute portal windows for recursive rendering
        let mut portal_window_data = None;

        if let Some(back_sector) = &seg.backsector {
            let front_floor = seg.frontsector.floorheight;
            let front_ceiling = seg.frontsector.ceilingheight;
            let back_floor = back_sector.floorheight;
            let back_ceiling = back_sector.ceilingheight;

            let portal_bottom = front_floor.max(back_floor);
            let portal_top = front_ceiling.min(back_ceiling);

            if portal_top > portal_bottom && self.portal_stack.len() < 8 {
                // Create portal window polygon for clipping only
                let portal_poly = Polygon3D::from_wall_segment(
                    seg.v1,
                    seg.v2,
                    portal_bottom,
                    portal_top,
                    [0, 0, 0, 0], // Invisible - not for drawing
                    PolygonType::Portal,
                );

                // Transform and project to screen
                let view_poly = portal_poly.transform(&self.view_matrix);
                if let Some(mut screen_poly) =
                    view_poly.project(&self.projection_matrix, self.width, self.height)
                {
                    // Clip portal window against existing portal stack
                    let mut clipped = true;
                    for existing_portal in &self.portal_stack {
                        if let Some(clipped_poly) = existing_portal.clip_polygon(&screen_poly) {
                            screen_poly = clipped_poly;
                        } else {
                            // Completely clipped away
                            clipped = false;
                            break;
                        }
                    }

                    if clipped {
                        portal_window_data = Some((screen_poly, back_sector.clone()));
                    }
                }
            }
        }

        (has_solid_wall, portal_window_data)
    }

    /// Triangulate a subsector's floor and ceiling using simple convex polygon triangulation
    fn triangulate_subsector_floor_ceiling(&self, segments: &[Segment]) -> Vec<Vec<Vec2>> {
        if segments.is_empty() {
            return Vec::new();
        }

        // Collect all unique vertices from segments
        let mut vertices: Vec<Vec2> = Vec::new();
        for seg in segments {
            // Only add vertex if it's not already very close to an existing one
            let mut found_v1 = false;
            let mut found_v2 = false;

            for &existing in &vertices {
                if (seg.v1 - existing).length() < 1.0 {
                    found_v1 = true;
                }
                if (seg.v2 - existing).length() < 1.0 {
                    found_v2 = true;
                }
            }

            if !found_v1 {
                vertices.push(seg.v1);
            }
            if !found_v2 {
                vertices.push(seg.v2);
            }
        }

        if vertices.len() < 3 {
            return Vec::new();
        }

        // Calculate centroid
        let centroid = vertices.iter().fold(Vec2::ZERO, |acc, &v| acc + v) / vertices.len() as f32;

        // Sort vertices by angle from centroid to ensure proper winding order
        vertices.sort_by(|&a, &b| {
            let angle_a = (a - centroid).y.atan2((a - centroid).x);
            let angle_b = (b - centroid).y.atan2((b - centroid).x);
            angle_a
                .partial_cmp(&angle_b)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Create triangles using simple fan triangulation from first vertex
        let mut triangles = Vec::new();
        if vertices.len() >= 3 {
            for i in 1..vertices.len() - 1 {
                triangles.push(vec![vertices[0], vertices[i], vertices[i + 1]]);
            }
        }

        triangles
    }

    /// Main rendering function
    ///
    /// Rendering pipeline:
    /// 1. Update view matrix based on player position/orientation
    /// 2. Clear framebuffer
    /// 3. Iterate through all level segments
    /// 4. Cull back-facing segments
    /// 5. Project and render visible segments
    pub fn render(
        &mut self,
        player: &Player,
        level: &Level,
        pic_data: &mut PicData,
        buffer: &mut impl PixelBuffer,
    ) {
        // Update camera transformation matrix
        self.update_view_matrix(player);

        // Clear screen to black
        buffer.clear_with_colour(&[0, 0, 0, 255]);

        // Reset occlusion buffer, portal stack and stats
        self.occlusion_buffer = OcclusionBuffer::new(self.width as usize);
        self.portal_stack.clear();

        // Get player position for front-face culling
        let player_pos = if let Some(mobj) = player.mobj() {
            mobj.xy
        } else {
            return; // No player object, can't render
        };

        // Render using BSP traversal for proper front-to-back ordering
        self.render_bsp_node(
            &level.map_data,
            buffer,
            level.map_data.start_node(),
            player_pos,
            pic_data,
        );
    }

    /// Render subsector with portal support
    fn render_subsector_with_portals(
        &mut self,
        map: &MapData,
        buffer: &mut impl PixelBuffer,
        subsector: &SubSector,
        player_pos: Vec2,
        pic_data: &mut PicData,
    ) {
        let start_seg = subsector.start_seg as usize;
        let end_seg = start_seg + subsector.seg_count as usize;

        if let Some(segments) = map.segments().get(start_seg..end_seg) {
            // Triangulate subsector floor and ceiling
            let triangles = self.triangulate_subsector_floor_ceiling(segments);
            let sector = &subsector.sector;

            // Render floor triangles (always occlude)
            let floor_color = pic_data.get_flat_average_color(sector.floorpic);
            for triangle in &triangles {
                let floor_poly = Polygon3D::from_horizontal_polygon(
                    triangle.clone(),
                    sector.floorheight,
                    floor_color,
                    PolygonType::Floor,
                );

                if let Some(view_poly) = floor_poly.transform(&self.view_matrix).project(
                    &self.projection_matrix,
                    self.width,
                    self.height,
                ) {
                    // Clip to portal stack if needed
                    let clipped_poly = if self.portal_stack.is_empty() {
                        view_poly
                    } else {
                        let mut current = view_poly;
                        for portal in &self.portal_stack {
                            if let Some(clipped) = portal.clip_polygon(&current) {
                                current = clipped;
                            } else {
                                break;
                            }
                        }
                        current
                    };

                    self.draw_polygon_with_occlusion(buffer, &clipped_poly);
                }
            }

            // Render ceiling triangles (occlude unless it's sky)
            if sector.ceilingpic != pic_data.sky_num() {
                let ceiling_color = pic_data.get_flat_average_color(sector.ceilingpic);
                for triangle in &triangles {
                    let ceiling_poly = Polygon3D::from_horizontal_polygon(
                        triangle.clone(),
                        sector.ceilingheight,
                        ceiling_color,
                        PolygonType::Ceiling,
                    );

                    if let Some(view_poly) = ceiling_poly.transform(&self.view_matrix).project(
                        &self.projection_matrix,
                        self.width,
                        self.height,
                    ) {
                        // Clip to portal stack if needed
                        let clipped_poly = if self.portal_stack.is_empty() {
                            view_poly
                        } else {
                            let mut current = view_poly;
                            for portal in &self.portal_stack {
                                if let Some(clipped) = portal.clip_polygon(&current) {
                                    current = clipped;
                                } else {
                                    break;
                                }
                            }
                            current
                        };

                        self.draw_polygon_with_occlusion(buffer, &clipped_poly);
                    }
                }
            }

            // First pass: render all segments and collect portal windows
            // let mut portal_data_list = Vec::new();

            for seg in segments {
                // TODO: Do we still need this portal stack stuff?
                // Render the segment (solid walls only) and get portal window if any
                let (_, _portal_data) = self.render_segment(buffer, seg, player_pos, pic_data);

                // If this segment has a portal window, save it for recursive rendering
                // if let Some((portal_poly, back_sector)) = portal_data {
                //     portal_data_list.push((portal_poly, back_sector));
                // }
            }

            // Second pass: recursively render through portals
            // for (portal_poly, _back_sector) in portal_data_list {
            //     if self.portal_stack.len() < 4 {
            //         // Limit recursion depth
            //         // Add this portal to the stack
            //         let portal_window = PortalWindow::from_polygon(&portal_poly);
            //         self.portal_stack.push(portal_window);

            //         // Recursively render through the portal using BSP traversal
            //         // This ensures proper depth ordering
            //         self.render_bsp_node(map, buffer, 0, player_pos);

            //         // Remove this portal from the stack
            //         self.portal_stack.pop();
            //     }
            // }
        }
    }

    /// Traverse BSP tree and render visible segments in front-to-back order
    fn render_bsp_node(
        &mut self,
        map: &MapData,
        buffer: &mut impl PixelBuffer,
        node_id: u32,
        player_pos: Vec2,
        pic_data: &mut PicData,
    ) {
        // Early exit if all columns are fully occluded
        let mut all_occluded = true;
        for x in 0..self.occlusion_buffer.spans.len() {
            if !self.occlusion_buffer.is_column_fully_occluded(x) {
                all_occluded = false;
                break;
            }
        }
        if all_occluded {
            return;
        }

        if node_id & IS_SSECTOR_MASK != 0 {
            // It's a subsector
            let subsector_id = if node_id == u32::MAX {
                0
            } else {
                (node_id & !IS_SSECTOR_MASK) as usize
            };

            if subsector_id < map.subsectors().len() {
                let subsector = &map.subsectors()[subsector_id];
                self.render_subsector_with_portals(map, buffer, subsector, player_pos, pic_data);
            }
            return;
        }

        // It's a node - determine which side the player is on
        if let Some(node) = map.get_nodes().get(node_id as usize) {
            let side = node.point_on_side(&player_pos);

            // Render front side first (closer to player)
            self.render_bsp_node(map, buffer, node.children[side], player_pos, pic_data);

            // Check if back side bounding box is in view
            if self.bbox_in_view(node, player_pos, side ^ 1) {
                // Also check if bbox is visible through current portal stack
                // TODO: add a max and min height to the nodes
                let mut bbox_visible = true;
                if !self.portal_stack.is_empty() {
                    // Project bbox corners to screen and check against portal windows
                    let bbox = &node.bboxes[side ^ 1];
                    let min = map.get_map_extents().min_floor;
                    let max = map.get_map_extents().max_ceiling;
                    let corners = [
                        Vec3::new(bbox[0].x, bbox[0].y, min),
                        Vec3::new(bbox[1].x, bbox[0].y, min),
                        Vec3::new(bbox[0].x, bbox[1].y, min),
                        Vec3::new(bbox[1].x, bbox[1].y, min),
                        Vec3::new(bbox[0].x, bbox[0].y, max),
                        Vec3::new(bbox[1].x, bbox[0].y, max),
                        Vec3::new(bbox[0].x, bbox[1].y, max),
                        Vec3::new(bbox[1].x, bbox[1].y, max),
                    ];

                    // Check if any corner is visible through portal stack
                    bbox_visible = false;
                    for corner in &corners {
                        let view_pos = self.view_matrix.transform_point3(*corner);
                        if view_pos.z < -0.1 {
                            // Project to screen space
                            let clip = self.projection_matrix
                                * Vec4::new(view_pos.x, view_pos.y, view_pos.z, 1.0);
                            if clip.w > 0.0 {
                                let ndc =
                                    Vec3::new(clip.x / clip.w, clip.y / clip.w, clip.z / clip.w);
                                let screen_pos = Vec2::new(
                                    (ndc.x + 1.0) * 0.5 * self.width,
                                    (1.0 - ndc.y) * 0.5 * self.height,
                                );
                                let mut point_visible = true;
                                for portal in &self.portal_stack {
                                    // Simple point-in-polygon test for portal window
                                    if !portal.contains_point(screen_pos) {
                                        point_visible = false;
                                        break;
                                    }
                                }
                                if point_visible {
                                    bbox_visible = true;
                                    break;
                                }
                            }
                        }
                    }
                }

                if bbox_visible {
                    // Render back side
                    self.render_bsp_node(
                        map,
                        buffer,
                        node.children[side ^ 1],
                        player_pos,
                        pic_data,
                    );
                }
            }
        }
    }

    /// Check if a bounding box is potentially visible using frustum culling
    fn bbox_in_view(&mut self, node: &Node, _player_pos: Vec2, side: usize) -> bool {
        let bbox = &node.bboxes[side];
        let min = bbox[0];
        let max = bbox[1];

        // Get all 8 corners of the bounding box (assuming floor and ceiling heights)
        let corners = [
            Vec3::new(min.x, min.y, -1000.0), // bottom corners
            Vec3::new(max.x, min.y, -1000.0),
            Vec3::new(min.x, max.y, -1000.0),
            Vec3::new(max.x, max.y, -1000.0),
            Vec3::new(min.x, min.y, 1000.0), // top corners
            Vec3::new(max.x, min.y, 1000.0),
            Vec3::new(min.x, max.y, 1000.0),
            Vec3::new(max.x, max.y, 1000.0),
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
        let aspect = self.width / self.height;
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
}

impl PlayViewRenderer for Renderer3D {
    fn render_player_view(&mut self, _player: &Player, _level: &Level, _pic_data: &mut PicData) {
        // This method is called by the game engine, but we don't have access to the buffer here
        // The actual rendering happens in the render() method
    }
}

impl Renderer3D {
    /// Renders using a RenderTrait implementation.
    pub fn render_with_trait(
        &mut self,
        player: &Player,
        level: &Level,
        pic_data: &mut PicData,
        renderer: &mut impl RenderTrait,
    ) {
        self.render(player, level, pic_data, renderer.draw_buffer());
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
        assert_eq!(renderer.width, 640.0);
        assert_eq!(renderer.height, 480.0);
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

        // Point to the right of viewport
        assert_eq!(renderer.compute_outcode(700.0, 240.0), 4);

        // Point to the left of viewport
        assert_eq!(renderer.compute_outcode(-10.0, 240.0), 8);
    }
}
