#[cfg(feature = "hprof")]
use coarse_prof::profile;
use gameplay::{
    AABB, BSP3D, Level, MapData, PicData, Player, Sector, SubSector, SurfaceKind, SurfacePolygon,
    WallTexPin, WallType,
};
use glam::{Mat4, Vec2, Vec3, Vec4};
use render_trait::DrawBuffer;

use std::f32::consts::PI;

mod depth_buffer;
mod render;
#[cfg(test)]
mod tests;

use depth_buffer::DepthBuffer;

const IS_SSECTOR_MASK: u32 = 0x8000_0000;

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
    x_intersections: [f32; 64],
    near_z: f32,
    far_z: f32,
    // Static arrays to eliminate hot path allocations
    screen_vertices_buffer: [Vec2; 8],
    tex_coords_buffer: [Vec2; 8],
    inv_w_buffer: [f32; 8],
    screen_vertices_len: usize,
    tex_coords_len: usize,
    inv_w_len: usize,
}

impl Software3D {
    pub fn new(width: f32, height: f32, fov: f32) -> Self {
        let near = 0.01;
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
            x_intersections: [0.0f32; 64],
            near_z: near,
            far_z: far,
            screen_vertices_buffer: [Vec2::ZERO; 8],
            tex_coords_buffer: [Vec2::ZERO; 8],
            inv_w_buffer: [0.0; 8],
            screen_vertices_len: 0,
            tex_coords_len: 0,
            inv_w_len: 0,
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
        dbg!(self.projection_matrix);
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
    fn is_bbox_outside_fov(&self, bbox: &AABB) -> bool {
        // Generate all 8 corners of the 3D bbox
        let corners = [
            Vec3::new(bbox.min.x, bbox.min.y, bbox.min.z),
            Vec3::new(bbox.max.x, bbox.min.y, bbox.min.z),
            Vec3::new(bbox.max.x, bbox.max.y, bbox.min.z),
            Vec3::new(bbox.min.x, bbox.max.y, bbox.min.z),
            Vec3::new(bbox.min.x, bbox.min.y, bbox.max.z),
            Vec3::new(bbox.max.x, bbox.min.y, bbox.max.z),
            Vec3::new(bbox.max.x, bbox.max.y, bbox.max.z),
            Vec3::new(bbox.min.x, bbox.max.y, bbox.max.z),
        ];

        // Transform corners to view space
        let view_projection = self.projection_matrix * self.view_matrix;
        let mut clip_corners = Vec::with_capacity(8);

        for corner in corners {
            let clip_pos = view_projection * Vec4::new(corner.x, corner.y, corner.z, 1.0);
            clip_corners.push(clip_pos);
        }

        // Test against frustum planes using clip coordinates
        // Be more conservative - only cull if AABB is completely outside a plane
        // and add small epsilon to avoid precision issues
        const EPSILON: f32 = 0.01;

        // Left plane: x >= -w
        if clip_corners.iter().all(|c| c.x < -c.w - EPSILON) {
            return true;
        }
        // Right plane: x <= w
        if clip_corners.iter().all(|c| c.x > c.w + EPSILON) {
            return true;
        }
        // Bottom plane: y >= -w
        if clip_corners.iter().all(|c| c.y < -c.w - EPSILON) {
            return true;
        }
        // Top plane: y <= w
        if clip_corners.iter().all(|c| c.y > c.w + EPSILON) {
            return true;
        }
        // Near plane: z >= -w
        if clip_corners.iter().all(|c| c.z < -c.w - EPSILON) {
            return true;
        }
        // Far plane: z <= w
        if clip_corners.iter().all(|c| c.z > c.w + EPSILON) {
            return true;
        }

        false
    }

    /// Traverse BSP3D tree and render visible segments in front-to-back order
    fn render_bsp3d_node(
        &mut self,
        node_id: u32,
        bsp3d: &BSP3D,
        sectors: &[Sector],
        player_pos: Vec3,
        player_subsector_id: usize,
        player_light: usize,
        pic_data: &mut PicData,
        rend: &mut impl DrawBuffer,
    ) {
        if node_id & IS_SSECTOR_MASK != 0 {
            // It's a subsector
            let subsector_id = if node_id == u32::MAX {
                0
            } else {
                (node_id & !IS_SSECTOR_MASK) as usize
            };

            // if bsp3d.subsector_visible(player_subsector_id, subsector_id) {
            if let Some(leaf) = bsp3d.get_subsector_leaf(subsector_id) {
                for poly_surface in &leaf.polygons {
                    if poly_surface.is_facing_point(player_pos, &bsp3d.vertices) {
                        self.render_surface_polygon(
                            &poly_surface,
                            bsp3d,
                            sectors,
                            pic_data,
                            player_light,
                            rend,
                        );
                    }
                }
                // }
            }
            return;
        }

        // It's a node
        let Some(node) = bsp3d.nodes().get(node_id as usize).cloned() else {
            return;
        };
        let side = node.point_on_side(Vec2::new(player_pos.x, player_pos.y));

        // Render front side first (closer to player)
        self.render_bsp3d_node(
            node.children[side],
            bsp3d,
            sectors,
            player_pos,
            player_subsector_id,
            player_light,
            pic_data,
            rend,
        );

        // Render back side with 3D frustum check using computed AABB
        let back_child_id = node.children[side ^ 1];
        if let Some(back_aabb) = bsp3d.get_node_aabb(back_child_id) {
            if !self.is_bbox_outside_fov(back_aabb) {
                self.render_bsp3d_node(
                    back_child_id,
                    bsp3d,
                    sectors,
                    player_pos,
                    player_subsector_id,
                    player_light,
                    pic_data,
                    rend,
                );
            }
        }
    }

    /// Render a surface polygon
    fn render_surface_polygon(
        &mut self,
        polygon: &SurfacePolygon,
        bsp3d: &BSP3D,
        sectors: &[Sector],
        pic_data: &mut PicData,
        player_light: usize,
        rend: &mut impl DrawBuffer,
    ) {
        const VERT_COUNT: usize = 3;
        self.screen_vertices_len = 0;
        self.tex_coords_len = 0;
        self.inv_w_len = 0;
        let view_projection = self.projection_matrix * self.view_matrix;

        let mut projected_vertices = [None; VERT_COUNT];
        for (i, &vertex_idx) in polygon.vertices.iter().enumerate() {
            let vertex = bsp3d.vertex_get(vertex_idx);
            let clip_pos = view_projection * Vec4::new(vertex.x, vertex.y, vertex.z, 1.0);
            let polygon_vertices = bsp3d.get_polygon_vertices(polygon);
            let (u, v) = self.calculate_tex_coords(
                vertex,
                &polygon.surface_kind,
                &polygon_vertices,
                pic_data,
            );

            if clip_pos.w > 0.0 {
                let ndc = clip_pos / clip_pos.w;
                let screen_x = (ndc.x + 1.0) * 0.5 * self.width as f32;
                let screen_y = (1.0 - ndc.y) * 0.5 * self.height as f32;
                projected_vertices[i] = Some((
                    Vec2::new(screen_x, screen_y),
                    Vec2::new(u / clip_pos.w, v / clip_pos.w),
                    1.0 / clip_pos.w,
                ));
            }
        }

        for i in 0..VERT_COUNT {
            let v1_idx = i;
            let v2_idx = (i + 1) % VERT_COUNT;
            let v1_world = bsp3d.vertex_get(polygon.vertices[v1_idx]);
            let v2_world = bsp3d.vertex_get(polygon.vertices[v2_idx]);

            let v1_view = self.view_matrix * Vec4::new(v1_world.x, v1_world.y, v1_world.z, 1.0);
            let v2_view = self.view_matrix * Vec4::new(v2_world.x, v2_world.y, v2_world.z, 1.0);

            match (projected_vertices[v1_idx], projected_vertices[v2_idx]) {
                (Some((p1, tex1, w1)), Some(_)) => {
                    if self.screen_vertices_len == 0
                        || self.screen_vertices_buffer[self.screen_vertices_len - 1] != p1
                    {
                        self.screen_vertices_buffer[self.screen_vertices_len] = p1;
                        self.screen_vertices_len += 1;
                        self.tex_coords_buffer[self.tex_coords_len] = tex1;
                        self.tex_coords_len += 1;
                        self.inv_w_buffer[self.inv_w_len] = w1;
                        self.inv_w_len += 1;
                    }
                }
                (Some((p1, tex1, w1)), None) => {
                    if self.screen_vertices_len == 0
                        || self.screen_vertices_buffer[self.screen_vertices_len - 1] != p1
                    {
                        self.screen_vertices_buffer[self.screen_vertices_len] = p1;
                        self.screen_vertices_len += 1;
                        self.tex_coords_buffer[self.tex_coords_len] = tex1;
                        self.tex_coords_len += 1;
                        self.inv_w_buffer[self.inv_w_len] = w1;
                        self.inv_w_len += 1;
                    }
                    if v1_view.z < -self.near_z && v2_view.z > -self.near_z {
                        let t = (-self.near_z - v1_view.z) / (v2_view.z - v1_view.z);
                        let clip_point_view = v1_view + (v2_view - v1_view) * t;
                        let clip_point_world = v1_world + (v2_world - v1_world) * t;

                        let clip_pos = self.projection_matrix * clip_point_view;
                        if clip_pos.w > 0.0 {
                            let ndc = clip_pos / clip_pos.w;
                            let screen_x = (ndc.x + 1.0) * 0.5 * self.width as f32;
                            let screen_y = (1.0 - ndc.y) * 0.5 * self.height as f32;
                            self.screen_vertices_buffer[self.screen_vertices_len] =
                                Vec2::new(screen_x, screen_y);
                            self.screen_vertices_len += 1;

                            // Interpolate texture coordinates for clipped vertex
                            let polygon_vertices = bsp3d.get_polygon_vertices(polygon);
                            let (u_clip, v_clip) = self.calculate_tex_coords(
                                clip_point_world,
                                &polygon.surface_kind,
                                &polygon_vertices,
                                pic_data,
                            );
                            self.tex_coords_buffer[self.tex_coords_len] =
                                Vec2::new(u_clip / clip_pos.w, v_clip / clip_pos.w);
                            self.tex_coords_len += 1;
                            self.inv_w_buffer[self.inv_w_len] = 1.0 / clip_pos.w;
                            self.inv_w_len += 1;
                        }
                    }
                }
                (None, Some((p2, tex2, w2))) => {
                    if v1_view.z > -self.near_z && v2_view.z < -self.near_z {
                        let t = (-self.near_z - v1_view.z) / (v2_view.z - v1_view.z);
                        let clip_point_view = v1_view + (v2_view - v1_view) * t;

                        let clip_pos = self.projection_matrix * clip_point_view;
                        if clip_pos.w > 0.0 {
                            let ndc = clip_pos / clip_pos.w;
                            let screen_x = (ndc.x + 1.0) * 0.5 * self.width as f32;
                            let screen_y = (1.0 - ndc.y) * 0.5 * self.height as f32;
                            self.screen_vertices_buffer[self.screen_vertices_len] =
                                Vec2::new(screen_x, screen_y);
                            self.screen_vertices_len += 1;

                            let clip_point_world = v1_world + (v2_world - v1_world) * t;
                            // Interpolate texture coordinates for clipped vertex
                            let polygon_vertices = bsp3d.get_polygon_vertices(polygon);
                            let (u_clip, v_clip) = self.calculate_tex_coords(
                                clip_point_world,
                                &polygon.surface_kind,
                                &polygon_vertices,
                                pic_data,
                            );
                            self.tex_coords_buffer[self.tex_coords_len] =
                                Vec2::new(u_clip / clip_pos.w, v_clip / clip_pos.w);
                            self.tex_coords_len += 1;
                            self.inv_w_buffer[self.inv_w_len] = 1.0 / clip_pos.w;
                            self.inv_w_len += 1;
                        }
                    }
                    self.screen_vertices_buffer[self.screen_vertices_len] = p2;
                    self.screen_vertices_len += 1;
                    self.tex_coords_buffer[self.tex_coords_len] = tex2;
                    self.tex_coords_len += 1;
                    self.inv_w_buffer[self.inv_w_len] = w2;
                    self.inv_w_len += 1;
                }
                (None, None) => {}
            }
        }

        if self.screen_vertices_len >= 3 {
            let brightness = (sectors[polygon.sector_id].lightlevel >> 4) + player_light;
            self.draw_polygon(
                polygon,
                brightness,
                pic_data,
                rend,
                #[cfg(feature = "debug_draw")]
                bsp3d,
                #[cfg(feature = "debug_draw")]
                {
                    let ptr = (&sectors[polygon.sector_id] as *const Sector as usize) as u32;
                    Some(
                        self.generate_pseudo_random_colour(
                            ptr,
                            sectors[polygon.sector_id].lightlevel,
                        ),
                    )
                },
            );
        }
    }

    fn calculate_tex_coords(
        &self,
        world_pos: Vec3,
        surface_kind: &SurfaceKind,
        original_vertices: &[Vec3],
        pic_data: &PicData,
    ) -> (f32, f32) {
        if original_vertices.len() < 2 {
            return (0.0, 0.0);
        }

        match surface_kind {
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

                // TODO: get rid of this sin/cos by precalculate and store in surface
                let polygon_dir = Vec3::new(texture_direction.cos(), texture_direction.sin(), 0.0);
                let v1 = original_vertices[0];
                let pos_from_start = world_pos - v1;
                let u = pos_from_start.x * polygon_dir.x + pos_from_start.y * polygon_dir.y;

                let wall_bottom_z = original_vertices
                    .iter()
                    .map(|v| v.z)
                    .fold(f32::INFINITY, f32::min);
                let wall_top_z = original_vertices
                    .iter()
                    .map(|v| v.z)
                    .fold(f32::NEG_INFINITY, f32::max);

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
                texture_direction,
            } => {
                let flat = pic_data.get_flat(*texture);
                let tex_width = flat.width as f32;
                let tex_height = flat.height as f32;

                // Step 1: Use world coordinates as base (always vary properly)
                let world_u = world_pos.x;
                let world_v = world_pos.y;

                // Step 2: Apply texture direction transformation
                let cos_angle = texture_direction.cos();
                let sin_angle = texture_direction.sin();
                let final_u = world_u * cos_angle - world_v * sin_angle;
                let final_v = world_u * sin_angle + world_v * cos_angle;

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
        rend: &mut impl DrawBuffer,
    ) {
        #[cfg(feature = "hprof")]
        profile!("render_player_view");
        let MapData {
            sectors,
            subsectors,
            bsp_3d,
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
            // Render using BSP3D traversal for proper front-to-back ordering
            let root_node = bsp_3d.root_node();
            self.render_bsp3d_node(
                root_node,
                bsp_3d,
                sectors,
                player_pos,
                player_subsector_id,
                player.extralight,
                pic_data,
                rend,
            );
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
}
