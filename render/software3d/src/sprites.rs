use gameplay::{MapObjFlag, MapObject, PicData, Sector, SectorExt};
use glam::{Vec2, Vec3, Vec4};
use math::point_to_angle_2;
use render_trait::DrawBuffer;
use std::f32::consts::{FRAC_PI_2, TAU};

use crate::Software3D;

const FF_FULLBRIGHT: u32 = 0x8000;
const FF_FRAMEMASK: u32 = 0x7FFF;
const FRAME_ROT_OFFSET: f32 = FRAC_PI_2 / 4.0;
const FRAME_ROT_SELECT: f32 = 8.0 / TAU;

/// Info needed to build and render a sprite billboard quad
pub(crate) struct SpriteQuad {
    /// 4 world-space vertices of the billboard (TL, TR, BR, BL)
    world_verts: [Vec3; 4],
    /// UV coords for each vertex
    uvs: [Vec2; 4],
    /// Sprite patch index for texture lookup
    pub(crate) patch_index: usize,
    /// Light level for colourmapping
    pub(crate) brightness: usize,
    /// Whether this is a shadow/fuzz thing
    pub(crate) is_shadow: bool,
    /// Distance squared for back-to-front sorting
    depth: f32,
}

impl Software3D {
    /// Collect and draw all visible sprites after polygon rendering.
    /// Creates billboard quads and renders them through the polygon pipeline.
    pub(super) fn draw_sprites(
        &mut self,
        visible_sectors: &[(usize, usize)], // (sector_id, light_level)
        sectors: &[Sector],
        player: &gameplay::Player,
        pic_data: &mut PicData,
        buffer: &mut impl DrawBuffer,
    ) {
        let player_mobj = match player.mobj() {
            Some(m) => m as *const MapObject,
            None => return,
        };
        let player_pos =
            unsafe { Vec3::new((*player_mobj).xy.x, (*player_mobj).xy.y, player.viewz) };
        let player_angle = unsafe { (*player_mobj).angle.rad() };

        // Camera right vector in XY plane (perpendicular to view direction)
        // forward = (cos, sin), right = (sin, -cos) in right-handed Z-up
        let cam_right = Vec2::new(player_angle.sin(), -player_angle.cos());
        let view_proj = self.projection_matrix * self.view_matrix;

        let mut quads: Vec<SpriteQuad> = Vec::new();

        for &(sector_id, light_level) in visible_sectors {
            let sector = &sectors[sector_id];
            sector.run_func_on_thinglist(|thing| {
                // Skip the player's own mobj
                if std::ptr::eq(thing, unsafe { &*player_mobj }) {
                    return true;
                }

                if let Some(quad) = Self::build_sprite_quad(
                    thing,
                    player_mobj,
                    &cam_right,
                    &view_proj,
                    player_pos,
                    light_level,
                    player.extralight,
                    pic_data,
                ) {
                    quads.push(quad);
                }
                true
            });
        }

        // Sort back-to-front (farthest first) for painter's algorithm
        quads.sort_by(|a, b| {
            b.depth
                .partial_cmp(&a.depth)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Render each sprite quad through the polygon pipeline
        for quad in &quads {
            self.render_sprite_quad(quad, pic_data, buffer);
        }
    }

    /// Build a billboard quad in world space for a sprite
    fn build_sprite_quad(
        thing: &MapObject,
        player_mobj_ptr: *const MapObject,
        cam_right: &Vec2,
        view_proj: &glam::Mat4,
        player_pos: Vec3,
        light_level: usize,
        player_extralight: usize,
        pic_data: &PicData,
    ) -> Option<SpriteQuad> {
        // Get sprite frame
        let sprnum = thing.state.sprite as u32 as usize;
        let sprite_def = pic_data.sprite_def(sprnum);
        if sprite_def.frames.is_empty() {
            return None;
        }

        let frame = (thing.frame & FF_FRAMEMASK) as usize;
        if frame >= sprite_def.frames.len() {
            return None;
        }
        let sprite_frame = sprite_def.frames[frame];

        // Get patch and flip based on rotation
        let (patch_index, flip) = if sprite_frame.rotate == 1 {
            let player_mobj = unsafe { &*player_mobj_ptr };
            let angle = point_to_angle_2(player_mobj.xy, thing.xy);
            let rot = ((angle - thing.angle + FRAME_ROT_OFFSET).rad()) * FRAME_ROT_SELECT;
            let rot = rot as u32 as usize % 8;
            (
                sprite_frame.lump[rot] as u32 as usize,
                sprite_frame.flip[rot] != 0,
            )
        } else {
            (
                sprite_frame.lump[0] as u32 as usize,
                sprite_frame.flip[0] != 0,
            )
        };

        let patch = pic_data.sprite_patch(patch_index);
        let sprite_width = patch.data.len() as f32;
        if sprite_width < 1.0 {
            return None;
        }
        let sprite_height = if patch.data.is_empty() {
            return None;
        } else {
            patch.data[0].len() as f32
        };

        // Build billboard quad in world space
        // The quad faces the camera (billboarded around Z axis)
        let left_offset = patch.left_offset as f32;

        // Horizontal span: from (center - left_offset) to (center - left_offset +
        // width) in world units along the camera-right direction
        let base_x = thing.xy.x;
        let base_y = thing.xy.y;
        let base_z = thing.z;

        let (left_dist, right_dist) = if flip {
            (sprite_width - left_offset, -left_offset)
        } else {
            (-left_offset, sprite_width - left_offset)
        };

        // The billboard sits on the floor with the full sprite height above it.
        // top_offset positions the texture within the quad but the quad itself
        // always starts at base_z (floor) and extends sprite_height upward.
        let bottom_z = base_z;
        let top_z = base_z + sprite_height;

        // Quad vertices: TL, TR, BR, BL (as seen from camera)
        let tl = Vec3::new(
            base_x + cam_right.x * left_dist,
            base_y + cam_right.y * left_dist,
            top_z,
        );
        let tr = Vec3::new(
            base_x + cam_right.x * right_dist,
            base_y + cam_right.y * right_dist,
            top_z,
        );
        let br = Vec3::new(
            base_x + cam_right.x * right_dist,
            base_y + cam_right.y * right_dist,
            bottom_z,
        );
        let bl = Vec3::new(
            base_x + cam_right.x * left_dist,
            base_y + cam_right.y * left_dist,
            bottom_z,
        );

        // UV coords: map quad corners to texture [0,1] range
        // If flipped, U goes from 1 to 0 instead of 0 to 1
        let (u_left, u_right) = if flip { (1.0, 0.0) } else { (0.0, 1.0) };

        let uvs = [
            Vec2::new(u_left, 0.0),  // TL
            Vec2::new(u_right, 0.0), // TR
            Vec2::new(u_right, 1.0), // BR
            Vec2::new(u_left, 1.0),  // BL
        ];

        let brightness = if thing.frame & FF_FULLBRIGHT != 0 {
            15
        } else {
            (light_level + player_extralight).min(15)
        };

        // Project the thing's center to check if it's in front of the camera
        let clip = *view_proj * Vec4::new(thing.xy.x, thing.xy.y, thing.z, 1.0);
        if clip.w <= 0.0 {
            return None;
        }

        // Distance squared for sorting (farther = larger = drawn first)
        let dx = thing.xy.x - player_pos.x;
        let dy = thing.xy.y - player_pos.y;
        let depth = dx * dx + dy * dy;

        Some(SpriteQuad {
            world_verts: [tl, tr, br, bl],
            patch_index,
            uvs,
            brightness,
            is_shadow: thing.flags & MapObjFlag::Shadow as u32 != 0,
            depth,
        })
    }

    /// Render a sprite quad through the clipping/projection/rasterization
    /// pipeline
    fn render_sprite_quad(
        &mut self,
        quad: &SpriteQuad,
        pic_data: &mut PicData,
        buffer: &mut impl DrawBuffer,
    ) {
        // We need to split the quad into two triangles and render each,
        // because the clipping pipeline works with triangles (3 input vertices)
        // Triangle 1: TL, TR, BR
        // Triangle 2: TL, BR, BL
        let view_proj = self.projection_matrix * self.view_matrix;

        for tri in &[[0, 1, 2], [0, 2, 3]] {
            self.screen_vertices_len = 0;
            self.tex_coords_len = 0;
            self.inv_w_len = 0;
            self.clipped_vertices_len = 0;

            let mut input_vertices = [Vec4::ZERO; 3];
            let mut input_tex_coords = [Vec3::ZERO; 3];

            for (i, &vi) in tri.iter().enumerate() {
                let world = quad.world_verts[vi];
                let clip_pos = view_proj * Vec4::new(world.x, world.y, world.z, 1.0);
                let uv = quad.uvs[vi];
                input_vertices[i] = clip_pos;
                input_tex_coords[i] = Vec3::new(uv.x, uv.y, clip_pos.w);
            }

            // Clip against frustum
            self.clip_polygon_frustum(&input_vertices, &input_tex_coords, 3);

            // Project clipped vertices to screen space
            for i in 0..self.clipped_vertices_len {
                let clip_pos = self.clipped_vertices_buffer[i];
                let tex_coord = self.clipped_tex_coords_buffer[i];

                if clip_pos.w > 0.0 {
                    let inv_w = 1.0 / clip_pos.w;
                    let ndc = clip_pos * inv_w;
                    let mut screen_x = (ndc.x + 1.0) * 0.5 * self.width as f32;
                    let mut screen_y = (1.0 - ndc.y) * 0.5 * self.height as f32;

                    // Screen-edge snap (same as render_surface_polygon)
                    let w_f32 = self.width as f32;
                    let h_f32 = self.height as f32;
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

            if self.screen_vertices_len >= 3 {
                self.draw_sprite_polygon(quad, pic_data, buffer);
            }
        }
    }
}
