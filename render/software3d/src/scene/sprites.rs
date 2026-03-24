use crate::voxel::collect::{
    CollectResult, VoxelCollectParams, VoxelSliceRef, collect_visible_slices
};
#[cfg(feature = "hprof")]
use coarse_prof::profile;
use gameplay::{MapObjFlag, MapObject, SectorExt};
use glam::{Vec2, Vec3, Vec4};
use level::Sector;
use math::{FixedT, point_to_angle_2};
use pic_data::{PicData, VoxelSlices};
use render_common::{DrawBuffer, RenderView};
use std::f32::consts::{FRAC_PI_2, TAU};

use crate::Software3D;

const FF_FULLBRIGHT: u32 = 0x8000;
const FF_FRAMEMASK: u32 = 0x7FFF;
const FRAME_ROT_OFFSET: f32 = 9.0 * FRAC_PI_2 / 4.0;
const FRAME_ROT_SELECT: f32 = 8.0 / TAU;
const VOXEL_BOB_RANGE: f32 = 6.0;
const VOXEL_MAX_DIST: f32 = 666.0;
const VOXEL_MAX_DIST_SQ: f32 = VOXEL_MAX_DIST * VOXEL_MAX_DIST;

pub(crate) struct SpriteQuad {
    world_verts: [Vec3; 4],
    uvs: [Vec2; 4],
    pub(crate) patch_index: usize,
    pub(crate) brightness: usize,
    pub(crate) is_shadow: bool,
    depth: f32,
}

impl Software3D {
    /// Collect and draw all visible sprites after polygon rendering.
    /// Creates billboard quads and renders them through the polygon pipeline.
    pub(crate) fn draw_sprites(
        &mut self,
        sectors: &[Sector],
        view: &RenderView,
        pic_data: &mut PicData,
        buffer: &mut impl DrawBuffer,
    ) {
        let player_pos = Vec3::new(view.x.into(), view.y.into(), view.viewz.into());
        let player_angle = view.angle.rad();

        // Camera right vector in XY plane (perpendicular to view direction)
        // forward = (cos, sin), right = (sin, -cos) in right-handed Z-up
        let cam_right = Vec2::new(player_angle.sin(), -player_angle.cos());
        let view_proj = self.projection_matrix * self.view_matrix;

        // Clone the Arc so we can borrow it without holding &self
        let voxel_mgr = self.voxel_manager.clone();

        self.sprite_quads.clear();

        // Collect voxel span quads separately — they use the depth buffer
        // directly and don't need painter's sort
        let mut voxel_slices: Vec<VoxelSliceRef> = Vec::new();

        for &(sector_id, light_level) in &self.visible_sectors {
            let sector = &sectors[sector_id];
            <Sector as SectorExt>::run_func_on_thinglist(sector, |thing| {
                // Skip the player's own mobj
                if thing as *const MapObject as usize == view.player_mobj_id {
                    return true;
                }

                let sprnum = thing.state.sprite as u32 as usize;
                let frame = (thing.frame & FF_FRAMEMASK) as usize;

                // Check for voxel replacement (within distance threshold)
                if let Some(ref mgr) = voxel_mgr {
                    if let Some(vslices) = mgr.get(sprnum, frame) {
                        let dx = player_pos.x - thing.x.to_f32();
                        let dy = player_pos.y - thing.y.to_f32();
                        let dist_sq = dx * dx + dy * dy;

                        if dist_sq <= VOXEL_MAX_DIST_SQ {
                            self.stats.voxel_objects += 1;
                            match Self::collect_voxel_slice_quads(
                                thing,
                                vslices,
                                player_pos,
                                &view_proj,
                                light_level,
                                view.extralight,
                                view.frac,
                                view.game_tic,
                                &self.rasterizer.depth_buffer,
                                self.rasterizer.width,
                                self.view_height,
                                &mut voxel_slices,
                            ) {
                                CollectResult::Behind => self.stats.voxel_behind += 1,
                                CollectResult::HizCulled => self.stats.voxel_hiz_culled += 1,
                                CollectResult::Collected(n, culled) => {
                                    self.stats.voxel_slices_submitted += n;
                                    self.stats.voxel_normal_culled += culled;
                                }
                            }
                            return true;
                        }

                        #[cfg(feature = "render_stats")]
                        {
                            self.stats.voxel_distance_culled += 1;
                        }
                    }
                }

                if let Some(quad) = Self::build_sprite_quad(
                    thing,
                    Vec2::new(view.x.into(), view.y.into()),
                    &cam_right,
                    &view_proj,
                    player_pos,
                    light_level,
                    view.extralight,
                    view.frac,
                    pic_data,
                ) {
                    self.sprite_quads.push(quad);
                }
                true
            });
        }

        // Sort regular sprites back-to-front (farthest first) for painter's algorithm
        self.sprite_quads.sort_by(|a, b| {
            b.depth
                .partial_cmp(&a.depth)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        #[cfg(feature = "hprof")]
        profile!("voxel_sort");
        voxel_slices.sort_unstable_by(|a, b| a.depth.total_cmp(&b.depth));

        // Render regular sprite quads
        let quads = std::mem::take(&mut self.sprite_quads);
        for quad in &quads {
            self.render_sprite_quad(quad, pic_data, buffer);
        }
        self.sprite_quads = quads;

        // Render voxel slices — view_proj computed once
        let view_proj = self.projection_matrix * self.view_matrix;
        self.render_voxel_slices(&voxel_slices, &view_proj, pic_data, buffer);
    }

    /// Collect all visible voxel slice quads for a thing.
    fn collect_voxel_slice_quads(
        thing: &MapObject,
        vslices: &VoxelSlices,
        player_pos: Vec3,
        view_proj: &glam::Mat4,
        light_level: usize,
        player_extralight: usize,
        frac: f32,
        game_tic: u32,
        depth_buffer: &crate::rasterizer::depth_buffer::DepthBuffer,
        screen_width: u32,
        screen_height: u32,
        out: &mut Vec<VoxelSliceRef>,
    ) -> CollectResult {
        #[cfg(feature = "hprof")]
        profile!("voxel_collect_slices");

        let lerp =
            |prev: FixedT, curr: FixedT| prev.to_f32() + (curr.to_f32() - prev.to_f32()) * frac;
        let base_x = lerp(thing.prev_x, thing.x);
        let base_y = lerp(thing.prev_y, thing.y);
        let base_z = lerp(thing.prev_z, thing.z);

        let spin_rate = if thing.flags.contains(MapObjFlag::Dropped) {
            vslices.dropped_spin
        } else {
            vslices.placed_spin
        };
        let (angle_rad, spin_bob) = if vslices.face_player {
            let dx = player_pos.x - base_x;
            let dy = player_pos.y - base_y;
            (dy.atan2(dx) + vslices.angle_offset, 0.0)
        } else if spin_rate != 0.0 {
            let spin_angle = spin_rate * (game_tic as f32 + frac);
            let bob = (1.0 - (spin_angle * 3.0).cos()) * 0.5 * VOXEL_BOB_RANGE;
            (spin_angle + vslices.angle_offset, bob)
        } else {
            (thing.angle.rad() + vslices.angle_offset, 0.0)
        };

        let brightness = if thing.frame & FF_FULLBRIGHT != 0 {
            15
        } else {
            (light_level + player_extralight).min(15)
        };

        let params = VoxelCollectParams {
            base_pos: Vec3::new(base_x, base_y, base_z + spin_bob),
            cos_a: angle_rad.cos(),
            sin_a: angle_rad.sin(),
            brightness,
            player_pos,
            view_proj,
            screen_width,
            screen_height,
            is_shadow: thing.flags.contains(MapObjFlag::Shadow),
        };

        collect_visible_slices(vslices, &params, depth_buffer, out)
    }

    /// Build a billboard quad in world space for a sprite
    fn build_sprite_quad(
        thing: &MapObject,
        player_xy: Vec2,
        cam_right: &Vec2,
        view_proj: &glam::Mat4,
        player_pos: Vec3,
        light_level: usize,
        player_extralight: usize,
        frac: f32,
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

        let lerp =
            |prev: FixedT, curr: FixedT| prev.to_f32() + (curr.to_f32() - prev.to_f32()) * frac;
        let base_x = lerp(thing.prev_x, thing.x);
        let base_y = lerp(thing.prev_y, thing.y);
        let base_z = lerp(thing.prev_z, thing.z);

        // Get patch and flip based on rotation
        let (patch_index, flip) = if sprite_frame.rotate == 1 {
            let angle = point_to_angle_2((base_x, base_y), (player_xy.x, player_xy.y));
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
        // (camera-relative to avoid catastrophic cancellation)
        let rel_x = thing.x.to_f32() - player_pos.x;
        let rel_y = thing.y.to_f32() - player_pos.y;
        let rel_z = thing.z.to_f32() - player_pos.z;
        let clip = *view_proj * Vec4::new(rel_x, rel_y, rel_z, 1.0);
        if clip.w <= 0.0 {
            return None;
        }

        // Distance squared for sorting (farther = larger = drawn first)
        let dx = thing.x.to_f32() - player_pos.x;
        let dy = thing.y.to_f32() - player_pos.y;
        let depth = dx * dx + dy * dy;

        Some(SpriteQuad {
            world_verts: [tl, tr, br, bl],
            patch_index,
            uvs,
            brightness,
            is_shadow: thing.flags.contains(MapObjFlag::Shadow),
            depth,
        })
    }

    /// Render a sprite quad through the clipping/projection/rasterization
    /// pipeline.
    ///
    /// - Splits the quad into two triangles (TL-TR-BR and TL-BR-BL)
    /// - Clips each triangle against the view frustum
    /// - Projects clipped vertices to screen space with perspective divide
    /// - Dispatches to `draw_sprite_polygon` or `draw_sprite_fuzz`
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
            self.rasterizer.screen_vertices_len = 0;
            self.rasterizer.tex_coords_len = 0;
            self.rasterizer.inv_w_len = 0;
            self.rasterizer.clipped_vertices_len = 0;

            let mut input_vertices = [Vec4::ZERO; 3];
            let mut input_tex_coords = [Vec3::ZERO; 3];

            let cp = self.camera_pos;
            for (i, &vi) in tri.iter().enumerate() {
                let world = quad.world_verts[vi];
                let rel = world - cp;
                let clip_pos = view_proj * Vec4::new(rel.x, rel.y, rel.z, 1.0);
                let uv = quad.uvs[vi];
                input_vertices[i] = clip_pos;
                input_tex_coords[i] = Vec3::new(uv.x, uv.y, clip_pos.w);
            }

            // Clip against frustum
            self.rasterizer
                .clip_polygon_frustum(&input_vertices, &input_tex_coords, 3);

            // Project clipped vertices to screen space
            for i in 0..self.rasterizer.clipped_vertices_len {
                let clip_pos = self.rasterizer.clipped_vertices_buffer[i];
                let tex_coord = self.rasterizer.clipped_tex_coords_buffer[i];

                if clip_pos.w > 0.0 {
                    let inv_w = 1.0 / clip_pos.w;
                    let w_f32 = self.width as f32;
                    let h_f32 = self.view_height as f32;
                    let half_w = 0.5 * w_f32;
                    let half_h = 0.5 * h_f32;
                    let mut screen_x = (clip_pos.x + clip_pos.w) * half_w * inv_w;
                    let mut screen_y = (clip_pos.w - clip_pos.y) * half_h * inv_w;
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

            if self.rasterizer.screen_vertices_len >= 3 {
                if quad.is_shadow {
                    self.draw_sprite_fuzz(quad, pic_data, buffer);
                } else {
                    self.draw_sprite_polygon(quad, pic_data, buffer);
                }
            }
        }
    }

    /// Render voxel slice quads via per-texel clip-space stepping.
    ///
    /// - Caches colourmaps per brightness band to avoid repeated lookups
    /// - Dispatches to `rasterize_voxel_texels` or `rasterize_voxel_fuzz`
    fn render_voxel_slices(
        &mut self,
        slices: &[VoxelSliceRef],
        view_proj: &glam::Mat4,
        pic_data: &mut PicData,
        buffer: &mut impl DrawBuffer,
    ) {
        #[cfg(feature = "hprof")]
        profile!("voxel_render_all");
        let cp = self.camera_pos;
        let buf_pitch = buffer.pitch();
        let palette = pic_data.palette();

        let mut cached_brightness = usize::MAX;
        let mut colourmaps = [&[][..]; 48];

        self.stats.voxel_slices_rendered += slices.len() as u32;
        for vq in slices {
            if vq.brightness != cached_brightness {
                cached_brightness = vq.brightness;
                for band in 0..48 {
                    colourmaps[band] = pic_data.base_colourmap(vq.brightness, band as f32);
                }
            }

            let columns = unsafe { &*vq.columns };
            let buf = buffer.buf_mut();
            if vq.is_shadow {
                self.rasterizer.rasterize_voxel_fuzz(
                    vq.origin,
                    vq.u_vec,
                    vq.v_vec,
                    columns,
                    vq.width,
                    vq.height,
                    view_proj,
                    cp,
                    buf,
                    buf_pitch,
                    &mut self.fuzz_pos,
                );
            } else {
                self.rasterizer.rasterize_voxel_texels(
                    vq.origin,
                    vq.u_vec,
                    vq.v_vec,
                    columns,
                    vq.width,
                    vq.height,
                    view_proj,
                    cp,
                    &colourmaps,
                    palette,
                    buf,
                    buf_pitch,
                );
            }
        }
    }
}
