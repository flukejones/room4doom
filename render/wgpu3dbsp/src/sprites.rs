//! Sprites: world billboards + the weapon psprite overlay.
//!
//! Two passes, both sampling one baked sprite atlas:
//! - World billboards — one camera-facing quad per visible thing, expanded in
//!   the vertex shader. Cutout (1-bit alpha, depth-tested + depth-written) so it
//!   occludes and is occluded by the scene like software3d's masked sprites.
//!   Spectres (Shadow flag) use a second pipeline that RGB-halves the background.
//! - Weapon psprite — screen-space overlay, drawn last with no depth, placed
//!   from Doom's 320×200 layout (see [`SpriteScratch::collect_psprites`]).
//!
//! [`SpriteScratch`] holds the per-frame CPU collection (selection, sort) and
//! the reused upload buffers; [`SpritePipeline`] owns the GPU resources.

use std::cmp::Ordering;
use std::f32::consts::{FRAC_PI_2, TAU};
use std::ptr;
use std::slice;

use bytemuck::cast_slice;
use gameplay::{MapObjFlag, MapObject, SectorExt};
use glam::{Mat4, Vec2, Vec3, Vec4};
use level::Sector;
use math::{FixedT, point_to_angle_2};
use pic_data::{PicData, VoxelManager};
use render_common::RenderView;
use render_common::light::{WEAPON_LIGHT_BOOST, WEAPON_LIGHT_INDEX_MAX, WEAPON_LIGHT_INDEX_SPAN};

use crate::assets::{Atlas, AtlasRect, SpriteAtlas, SpriteMeta};
use crate::camera::MAX_PITCH;
use crate::light::{LIGHT_LEVELS, LightParams};
use crate::scene::{DEPTH_FORMAT, SCENE_FORMAT, upload_atlas_texture, write_atlas_pixels};
use crate::shaders::{
    bind_sampler_entry, bind_storage_entry, bind_tex_array_entry, bind_uniform_entry,
};
use crate::voxel::is_voxel_eligible;

/// 6 vertices per quad (two triangles), pulled from the shader's corner table.
const VERTS_PER_QUAD: u32 = 6;
/// Weapon psprite layers (weapon + muzzle flash); the buffer is sized to this.
const MAX_PSPRITES: usize = 2;
/// `SpriteInstance.flags` bit0: spectre/shadow → fuzz (RGB-halve) pipeline.
const SPRITE_FLAG_FUZZ: u32 = 1;
/// Thing frame flags (Doom): full-bright bit, and the mask for the frame index.
const FF_FULLBRIGHT: u32 = 0x8000;
const FF_FRAMEMASK: u32 = 0x7FFF;
/// Sprite 8-way rotation selection (matches software3d sprites.rs).
const FRAME_ROT_OFFSET: f32 = 9.0 * FRAC_PI_2 / 4.0;
const FRAME_ROT_SELECT: f32 = 8.0 / TAU;
/// Colourmap rows-per-band for the psprite band math. The band ceiling
/// (`LIGHT_LEVELS`) and the row->intensity curve live in [`crate::light`].
const ROWS_PER_BAND: f32 = 4.0;
/// Doom psprite virtual screen: weapons are placed in 320×200 then scaled by
/// `view_height / 200` (matches software3d weapon.rs).
const PSP_VIRTUAL_HEIGHT: f32 = 200.0;
const PSP_HALF_WIDTH: f32 = 160.0;
const PSP_HALF_HEIGHT: f32 = 100.0;
/// OG weapon light boost (+2 light band) so the held weapon reads brighter.
const PSP_LIGHT_BOOST: usize = 2;

/// Sprite camera uniform (group 0). Distinct from the scene `CameraUniform` so
/// adding billboard basis vectors does not perturb the scene/sky layout.
/// std140: mat4 (64) + 3×vec4 (48) + vec4 tail (16) = 128 bytes.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SpriteCam {
    view_proj: [[f32; 4]; 4],
    camera_pos: [f32; 4],
    cam_right: [f32; 4],
    cam_up: [f32; 4],
    /// `[0]` = extralight; `[1..]` pad to 16 bytes.
    extralight: [f32; 4],
}

impl SpriteCam {
    /// Build from the player view + the shared projection matrix. `cam_right`
    /// matches software3d (`(sin angle, -cos angle)`); `cam_up` is world Z.
    pub fn new(view: &RenderView, projection: Mat4) -> Self {
        let pos = Vec3::new(view.x.into(), view.y.into(), view.viewz.into());
        let angle = view.angle.rad();
        let pitch = view.lookdir.clamp(-MAX_PITCH, MAX_PITCH);
        let forward = Vec3::new(
            angle.cos() * pitch.cos(),
            angle.sin() * pitch.cos(),
            pitch.sin(),
        );
        let view_proj = projection * Mat4::look_at_rh(Vec3::ZERO, forward, Vec3::Z);
        Self {
            view_proj: view_proj.to_cols_array_2d(),
            camera_pos: [pos.x, pos.y, pos.z, 0.0],
            cam_right: [angle.sin(), -angle.cos(), 0.0, 0.0],
            cam_up: [0.0, 0.0, 1.0, 0.0],
            extralight: [view.extralight as f32, 0.0, 0.0, 0.0],
        }
    }
}

/// One billboard, uploaded per frame. std430: 16-byte-field-aligned, 64 bytes.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SpriteInstance {
    pub center: [f32; 3],
    pub left_dist: f32,
    pub right_dist: f32,
    pub height: f32,
    pub rect_origin: [u32; 2],
    pub rect_size: [u32; 2],
    pub layer: u32,
    pub flip: u32,
    pub brightness: u32,
    pub flags: u32,
    pub _pad: [u32; 2],
}

/// One screen-space weapon psprite quad, uploaded per frame. std430: 12 fields
/// of 4 bytes = 48 bytes (16-aligned).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PspriteInstance {
    pub ndc_min: [f32; 2],
    pub ndc_max: [f32; 2],
    pub rect_origin: [u32; 2],
    pub rect_size: [u32; 2],
    pub layer: u32,
    pub flip: u32,
    pub light: f32,
    pub flags: u32,
}

/// Per-frame CPU sprite state: the per-patch tables (built once with the atlas),
/// the reused sort/upload scratch, and the opaque/fuzz split. `collect`/
/// `collect_psprites` fill the upload buffers each frame; the renderer then
/// hands them to [`SpritePipeline`].
#[derive(Default)]
pub struct SpriteScratch {
    /// Per-patch pivot (left_offset), indexed by patch id (built with the atlas).
    pub meta: Vec<SpriteMeta>,
    /// Per-patch atlas rect, indexed by patch id (parallel to `meta`).
    pub rects: Vec<AtlasRect>,
    /// Sort scratch: (squared XY dist, instance), sorted back-to-front.
    sort: Vec<(f32, SpriteInstance)>,
    /// Contiguous upload scratch: opaque (cutout) instances first, then fuzz
    /// (spectre), each back-to-front. `opaque_count` is the split.
    instances: Vec<SpriteInstance>,
    /// Count of leading opaque instances in `instances` (the rest are fuzz).
    opaque_count: u32,
    /// Weapon psprite upload scratch (≤ [`MAX_PSPRITES`] layers).
    psprites: Vec<PspriteInstance>,
}

/// Per-frame inputs for in-walk sprite collection, computed once before the
/// world walk.
pub struct SpriteCollectCtx<'a> {
    view: &'a RenderView,
    pic_data: &'a PicData,
    voxel_mgr: Option<&'a VoxelManager>,
    view_proj: Mat4,
    player_pos: Vec3,
    player_xy: Vec2,
}

impl<'a> SpriteCollectCtx<'a> {
    pub fn new(
        view: &'a RenderView,
        pic_data: &'a PicData,
        projection: Mat4,
        voxel_mgr: Option<&'a VoxelManager>,
    ) -> Self {
        Self {
            view,
            pic_data,
            voxel_mgr,
            view_proj: projection * Mat4::look_at_rh(Vec3::ZERO, view_forward(view), Vec3::Z),
            player_pos: Vec3::new(view.x.into(), view.y.into(), view.viewz.into()),
            player_xy: Vec2::new(view.x.into(), view.y.into()),
        }
    }
}

impl SpriteScratch {
    /// Store the per-patch tables from the baked atlas (called once at level load).
    pub fn set_atlas(&mut self, atlas: &SpriteAtlas) {
        self.meta = atlas.meta.clone();
        self.rects = atlas.atlas.rects.clone();
    }

    /// Reset the collection scratch; the world walk then appends per visible
    /// sector via [`Self::collect_in_sector`], and [`Self::finish_collect`]
    /// sorts/partitions the result.
    pub fn begin_collect(&mut self) {
        self.sort.clear();
    }

    /// Resolve every thing in one sector's thinglist to a billboard instance
    /// (frame/rotation replicate software3d). Called from the world walk for
    /// each frustum-visible sector; GPU depth + a behind-camera reject handle
    /// occlusion (over-draw is acceptable).
    pub fn collect_in_sector(&mut self, ctx: &SpriteCollectCtx, sector: &Sector) {
        let Self {
            meta,
            rects,
            sort,
            ..
        } = self;

        <Sector as SectorExt>::run_func_on_thinglist(sector, |thing: &MapObject| {
            if ptr::from_ref(thing) as usize == ctx.view.player_mobj_id {
                return true;
            }
            // Voxel-eligible things are drawn by the voxel pass instead — skip
            // them here so each thing renders once. Same rule both passes use.
            if let Some(mgr) = ctx.voxel_mgr
                && is_voxel_eligible(mgr, thing, ctx.player_xy.x, ctx.player_xy.y)
            {
                return true;
            }
            let sprnum = thing.state.sprite as u32 as usize;
            let sprite_def = ctx.pic_data.sprite_def(sprnum);
            let frame = (thing.frame & FF_FRAMEMASK) as usize;
            if frame >= sprite_def.frames.len() {
                return true;
            }
            let sprite_frame = sprite_def.frames[frame];

            let frac = ctx.view.frac;
            let lerp =
                |prev: FixedT, curr: FixedT| prev.to_f32() + (curr.to_f32() - prev.to_f32()) * frac;
            let base_x = lerp(thing.prev_x, thing.x);
            let base_y = lerp(thing.prev_y, thing.y);
            let base_z = lerp(thing.prev_z, thing.z);

            let (patch_index, flip) = if sprite_frame.rotate == 1 {
                let angle = point_to_angle_2((base_x, base_y), (ctx.player_xy.x, ctx.player_xy.y));
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
            if patch_index >= rects.len() {
                return true;
            }
            let rect = rects[patch_index];
            let width = rect.size[0] as f32;
            let height = rect.size[1] as f32;
            if width < 1.0 {
                return true;
            }

            let left_offset = meta[patch_index].left_offset as f32;
            let (left_dist, right_dist) = if flip {
                (width - left_offset, -left_offset)
            } else {
                (-left_offset, width - left_offset)
            };

            // Behind-camera reject (camera-relative, matches software3d).
            let rel = Vec3::new(
                thing.x.to_f32() - ctx.player_pos.x,
                thing.y.to_f32() - ctx.player_pos.y,
                thing.z.to_f32() - ctx.player_pos.z,
            );
            let clip = ctx.view_proj * Vec4::new(rel.x, rel.y, rel.z, 1.0);
            if clip.w <= 0.0 {
                return true;
            }

            // Sector light band (0..15); extralight is added once in the
            // shader (mirrors scene.wgsl). Full-bright frames pin to max.
            let brightness = if thing.frame & FF_FULLBRIGHT != 0 {
                LIGHT_LEVELS
            } else {
                sector.lightlevel >> 4
            };
            let flags = if thing.flags.contains(MapObjFlag::Shadow) {
                SPRITE_FLAG_FUZZ
            } else {
                0
            };

            let depth = rel.x * rel.x + rel.y * rel.y;
            sort.push((
                depth,
                SpriteInstance {
                    center: [base_x, base_y, base_z],
                    left_dist,
                    right_dist,
                    height,
                    rect_origin: rect.origin,
                    rect_size: rect.size,
                    layer: rect.layer,
                    flip: u32::from(flip),
                    brightness: brightness as u32,
                    flags,
                    _pad: [0; 2],
                },
            ));
            true
        });
    }

    /// Sort the collected instances back-to-front and partition opaque before
    /// fuzz, ready for upload.
    pub fn finish_collect(&mut self) {
        let Self {
            sort,
            instances,
            opaque_count,
            ..
        } = self;
        // Back-to-front (farthest first) painter's order.
        sort.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(Ordering::Equal));
        // Partition opaque (cutout) before fuzz (spectre) so each is one draw,
        // preserving back-to-front order within each class. `flags` bit0 = fuzz.
        instances.clear();
        instances.extend(
            sort.iter()
                .filter(|(_, i)| i.flags & SPRITE_FLAG_FUZZ == 0)
                .map(|(_, inst)| *inst),
        );
        *opaque_count = instances.len() as u32;
        instances.extend(
            sort.iter()
                .filter(|(_, i)| i.flags & SPRITE_FLAG_FUZZ != 0)
                .map(|(_, inst)| *inst),
        );
    }

    /// Build the weapon psprite quads (≤ [`MAX_PSPRITES`] layers), converting
    /// Doom's 320×200 placement to NDC (matches software3d weapon.rs).
    /// Screen-space, drawn last with no depth; `view.is_shadow` picks the fuzz
    /// variant at draw time. `screen_w`/`screen_h` are the engine buffer dims.
    pub fn collect_psprites(
        &mut self,
        view: &RenderView,
        pic_data: &PicData,
        light: &LightParams,
        screen_w: f32,
        screen_h: f32,
    ) {
        let base_scale = screen_h / PSP_VIRTUAL_HEIGHT;
        // Weapon light band: sector light + extralight + OG boost, 0..15.
        let base_band =
            ((view.sector_lightlevel >> 4) + view.extralight + PSP_LIGHT_BOOST).min(LIGHT_LEVELS);

        self.psprites.clear();
        for psp in &view.psprites {
            if !psp.active {
                continue;
            }
            let def = pic_data.sprite_def(psp.sprite);
            let frame_index = (psp.frame & FF_FRAMEMASK) as usize;
            if frame_index >= def.frames.len() {
                continue;
            }
            let frame = def.frames[frame_index];
            // Weapon sprites always use rotation 0.
            let patch_index = frame.lump[0] as u32 as usize;
            let flip = frame.flip[0] != 0;
            if patch_index >= self.rects.len() {
                continue;
            }
            let rect = self.rects[patch_index];
            let patch = pic_data.sprite_patch(patch_index);
            let cols = rect.size[0] as f32;
            let rows = rect.size[1] as f32;
            if cols < 1.0 || rows < 1.0 {
                continue;
            }

            // 320×200 placement (weapon.rs): anchor by left/top offset, scale by
            // screen_h/200, then convert pixel rect to NDC.
            let offset_x = (psp.sx - PSP_HALF_WIDTH) - patch.left_offset as f32;
            let x1 = (screen_w * 0.5 + offset_x * base_scale).round();
            let x2 = x1 + cols * base_scale;
            let texture_mid = PSP_HALF_HEIGHT - (psp.sy - patch.top_offset as f32);
            let y1 = (screen_h * 0.5 - texture_mid * base_scale).round();
            let y2 = y1 + rows * base_scale;
            if x2 < 0.0 || x1 >= screen_w || y2 < 0.0 || y1 >= screen_h {
                continue;
            }

            let band = if psp.frame & FF_FULLBRIGHT != 0 {
                LIGHT_LEVELS
            } else {
                base_band
            };
            // Mirror software3d's weapon light (init_light_scales): a band-
            // proportional colourmap index, then row = startmap - index/2.
            let weapon_light_scale = ((band as f32 / LIGHT_LEVELS as f32)
                * WEAPON_LIGHT_INDEX_SPAN
                + WEAPON_LIGHT_BOOST)
                .min(WEAPON_LIGHT_INDEX_MAX);
            let startmap = (LIGHT_LEVELS - band) as f32 * ROWS_PER_BAND;
            let row = (startmap - weapon_light_scale * 0.5).clamp(0.0, light.max_row());
            let intensity = (1.0 - row / light.max_row()).powf(light.light_gamma());

            // Pixel rect -> NDC (x right, y up; pixel y is top-down).
            let ndc = |px: f32, py: f32| [px / screen_w * 2.0 - 1.0, 1.0 - py / screen_h * 2.0];
            self.psprites.push(PspriteInstance {
                ndc_min: ndc(x1, y2),
                ndc_max: ndc(x2, y1),
                rect_origin: rect.origin,
                rect_size: rect.size,
                layer: rect.layer,
                flip: u32::from(flip),
                light: intensity,
                flags: 0,
            });
        }
    }
}

/// View forward vector (matches `CameraUniform::new`/software3d), for the
/// behind-camera reject's eye-at-origin clip test.
fn view_forward(view: &RenderView) -> Vec3 {
    let angle = view.angle.rad();
    let pitch = view.lookdir.clamp(-MAX_PITCH, MAX_PITCH);
    Vec3::new(
        angle.cos() * pitch.cos(),
        angle.sin() * pitch.cos(),
        pitch.sin(),
    )
}

/// GPU resources for the sprite pass: pipeline, per-frame camera uniform, the
/// baked sprite atlas + sampler, and a growable instance storage buffer.
pub struct SpritePipeline {
    pipeline: wgpu::RenderPipeline,
    fuzz_pipeline: wgpu::RenderPipeline,
    cam_buf: wgpu::Buffer,
    cam_bind: wgpu::BindGroup,
    inst_layout: wgpu::BindGroupLayout,
    inst_buf: wgpu::Buffer,
    inst_bind: wgpu::BindGroup,
    inst_capacity: u32,
    atlas_bind: wgpu::BindGroup,
    /// Retained so a CRT-gamma change can re-bake the sprite atlas in place.
    atlas_tex: wgpu::Texture,
    /// Light params uniform (group 3), shared model with the scene pass.
    light_buf: wgpu::Buffer,
    light_bind: wgpu::BindGroup,
    /// Weapon psprite overlay: screen-space, no depth, drawn last.
    psp_pipeline: wgpu::RenderPipeline,
    psp_fuzz_pipeline: wgpu::RenderPipeline,
    psp_buf: wgpu::Buffer,
    psp_bind: wgpu::BindGroup,
}

impl SpritePipeline {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, sprites: &SpriteAtlas) -> Self {
        let cam_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sprite_camera"),
            size: size_of::<SpriteCam>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let cam_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sprite_camera_bgl"),
            entries: &[bind_uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT)],
        });
        let cam_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sprite_camera_bg"),
            layout: &cam_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: cam_buf.as_entire_binding(),
            }],
        });

        let inst_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sprite_inst_bgl"),
            entries: &[bind_storage_entry(0, wgpu::ShaderStages::VERTEX)],
        });
        let (inst_buf, inst_bind, inst_capacity) = make_inst_buffer(device, &inst_layout, 1);

        let atlas_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sprite_atlas_bgl"),
            entries: &[
                bind_tex_array_entry(0),
                bind_sampler_entry(1, wgpu::SamplerBindingType::NonFiltering),
            ],
        });
        let (atlas_tex, atlas_view) =
            upload_atlas_texture(device, queue, &sprites.atlas, "sprite_atlas");
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sprite_atlas_sampler"),
            ..Default::default()
        });
        let atlas_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sprite_atlas_bg"),
            layout: &atlas_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        // Light params uniform (group 3): runtime gamma/falloff shared with the
        // scene pass. Built once; uploaded each frame via `set_light`.
        let light_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sprite_light"),
            size: size_of::<LightParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let light_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sprite_light_bgl"),
            entries: &[bind_uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT)],
        });
        let light_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sprite_light_bg"),
            layout: &light_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: light_buf.as_entire_binding(),
            }],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sprite_shader"),
            source: wgpu::ShaderSource::Wgsl(crate::shaders::SPRITE_SRC.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sprite_pipeline_layout"),
            bind_group_layouts: &[
                Some(&cam_layout),
                Some(&inst_layout),
                Some(&atlas_layout),
                Some(&light_layout),
            ],
            immediate_size: 0,
        });
        // Cutout: opaque-masked, depth test + write so sprites occlude correctly.
        let pipeline = make_pipeline(
            device,
            &layout,
            &shader,
            "fs_main",
            wgpu::BlendState::REPLACE,
            Some(true),
            "sprite_pipeline",
        );
        // Spectre fuzz: RGB-halve the background (src*0 + dst*(1-srcAlpha)).
        // Depth-write on (like software3d's fuzz path) so the nearest spectre
        // wins per pixel — overlapping spectres darken once, not 0.5^N.
        let fuzz_blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent::REPLACE,
        };
        let fuzz_pipeline = make_pipeline(
            device,
            &layout,
            &shader,
            "fs_fuzz",
            fuzz_blend,
            Some(true),
            "sprite_fuzz_pipeline",
        );

        // Weapon psprite: screen-space, no depth. Own instance buffer (≤2 layers)
        // + a layout binding the same atlas at group 1.
        let psp_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("psprite_shader"),
            source: wgpu::ShaderSource::Wgsl(crate::shaders::PSPRITE_SRC.into()),
        });
        let psp_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("psprite_instances"),
            size: MAX_PSPRITES as u64 * size_of::<PspriteInstance>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let psp_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("psprite_inst_bg"),
            layout: &inst_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: psp_buf.as_entire_binding(),
            }],
        });
        let psp_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("psprite_pipeline_layout"),
            bind_group_layouts: &[Some(&inst_layout), Some(&atlas_layout)],
            immediate_size: 0,
        });
        let psp_pipeline = make_pipeline(
            device,
            &psp_layout,
            &psp_shader,
            "fs_main",
            wgpu::BlendState::REPLACE,
            None,
            "psprite_pipeline",
        );
        let psp_fuzz_pipeline = make_pipeline(
            device,
            &psp_layout,
            &psp_shader,
            "fs_fuzz",
            fuzz_blend,
            None,
            "psprite_fuzz_pipeline",
        );

        Self {
            pipeline,
            fuzz_pipeline,
            cam_buf,
            cam_bind,
            inst_layout,
            inst_buf,
            inst_bind,
            inst_capacity,
            atlas_bind,
            atlas_tex,
            light_buf,
            light_bind,
            psp_pipeline,
            psp_fuzz_pipeline,
            psp_buf,
            psp_bind,
        }
    }

    /// Upload and draw the frame's world billboards (over the scene, with depth).
    /// Split from the psprite draw so the voxel pass can run between them (voxels
    /// are world geometry; the psprite is screen-space and must stay on top).
    pub fn render_world(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        scene_view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
        cam: &SpriteCam,
        light: &LightParams,
        scratch: &SpriteScratch,
    ) {
        self.set_camera(queue, cam);
        self.set_light(queue, light);
        self.upload_instances(device, queue, &scratch.instances);
        self.draw(
            encoder,
            scene_view,
            depth_view,
            scratch.opaque_count,
            scratch.instances.len() as u32,
        );
    }

    /// Draw the weapon psprite (screen-space, no depth, on top). Called last, after
    /// world billboards and the voxel pass. `fuzz_player` picks the spectre variant.
    pub fn render_psprites(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        scene_view: &wgpu::TextureView,
        scratch: &SpriteScratch,
        fuzz_player: bool,
    ) {
        self.set_psprites(queue, &scratch.psprites);
        self.draw_psprites(
            encoder,
            scene_view,
            scratch.psprites.len() as u32,
            fuzz_player,
        );
    }

    fn set_camera(&self, queue: &wgpu::Queue, cam: &SpriteCam) {
        queue.write_buffer(&self.cam_buf, 0, cast_slice(slice::from_ref(cam)));
    }

    fn set_light(&self, queue: &wgpu::Queue, light: &LightParams) {
        queue.write_buffer(&self.light_buf, 0, cast_slice(slice::from_ref(light)));
    }

    /// Re-bake the sprite atlas pixels into the existing texture (CRT gamma
    /// changed the palette). Dimensions are palette-independent.
    pub fn reupload_atlas(&self, queue: &wgpu::Queue, atlas: &Atlas) {
        write_atlas_pixels(queue, &self.atlas_tex, atlas);
    }

    /// Upload the frame's instances, growing the GPU buffer + rebinding when the
    /// count exceeds capacity. Empty frames are a no-op (drawn count 0).
    fn upload_instances(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        insts: &[SpriteInstance],
    ) {
        if insts.len() as u32 > self.inst_capacity {
            let cap = (insts.len() as u32).next_power_of_two();
            let (buf, bind, capacity) = make_inst_buffer(device, &self.inst_layout, cap);
            self.inst_buf = buf;
            self.inst_bind = bind;
            self.inst_capacity = capacity;
        }
        if !insts.is_empty() {
            queue.write_buffer(&self.inst_buf, 0, cast_slice(insts));
        }
    }

    /// Record the sprite pass into the scene colour + depth views. The instance
    /// buffer is partitioned `[0, opaque_count)` opaque (cutout) and
    /// `[opaque_count, count)` fuzz (spectre); each is drawn with its pipeline.
    /// Depth loads (the scene pass wrote it); colour loads too.
    fn draw(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        scene_view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
        opaque_count: u32,
        count: u32,
    ) {
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("sprite_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: scene_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        if count == 0 {
            return;
        }
        rpass.set_bind_group(0, &self.cam_bind, &[]);
        rpass.set_bind_group(1, &self.inst_bind, &[]);
        rpass.set_bind_group(2, &self.atlas_bind, &[]);
        rpass.set_bind_group(3, &self.light_bind, &[]);
        if opaque_count > 0 {
            rpass.set_pipeline(&self.pipeline);
            rpass.draw(0..VERTS_PER_QUAD, 0..opaque_count);
        }
        if count > opaque_count {
            rpass.set_pipeline(&self.fuzz_pipeline);
            rpass.draw(0..VERTS_PER_QUAD, opaque_count..count);
        }
    }

    /// Upload the weapon psprite layers (≤ [`MAX_PSPRITES`]).
    fn set_psprites(&self, queue: &wgpu::Queue, psprites: &[PspriteInstance]) {
        if !psprites.is_empty() {
            queue.write_buffer(&self.psp_buf, 0, cast_slice(psprites));
        }
    }

    /// Record the weapon psprite pass: screen-space, no depth, on top. `fuzz`
    /// selects the spectre-player (RGB-halve) variant for all layers.
    fn draw_psprites(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        scene_view: &wgpu::TextureView,
        count: u32,
        fuzz: bool,
    ) {
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("psprite_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: scene_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        if count == 0 {
            return;
        }
        let pipeline = if fuzz {
            &self.psp_fuzz_pipeline
        } else {
            &self.psp_pipeline
        };
        rpass.set_pipeline(pipeline);
        rpass.set_bind_group(0, &self.psp_bind, &[]);
        rpass.set_bind_group(1, &self.atlas_bind, &[]);
        rpass.draw(0..VERTS_PER_QUAD, 0..count);
    }
}

/// Build a sprite render pipeline. Shared `vs_main`, no cull; the fragment
/// entry and colour blend vary. `depth = Some(write)` → depth-tested `LessEqual`
/// (world billboards); `None` → no depth attachment (screen-space psprites).
fn make_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    fs_entry: &str,
    blend: wgpu::BlendState,
    depth: Option<bool>,
    label: &str,
) -> wgpu::RenderPipeline {
    let depth_stencil = depth.map(|write| wgpu::DepthStencilState {
        format: DEPTH_FORMAT,
        depth_write_enabled: Some(write),
        depth_compare: Some(wgpu::CompareFunction::LessEqual),
        stencil: wgpu::StencilState::default(),
        bias: wgpu::DepthBiasState::default(),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        primitive: wgpu::PrimitiveState {
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fs_entry),
            targets: &[Some(wgpu::ColorTargetState {
                format: SCENE_FORMAT,
                blend: Some(blend),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// Allocate an instance storage buffer (≥1 entry) + its bind group.
fn make_inst_buffer(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    capacity: u32,
) -> (wgpu::Buffer, wgpu::BindGroup, u32) {
    let capacity = capacity.max(1);
    let buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("sprite_instances"),
        size: capacity as u64 * size_of::<SpriteInstance>() as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("sprite_inst_bg"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buf.as_entire_binding(),
        }],
    });
    (buf, bind, capacity)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprite_cam_layout() {
        // Must byte-match the WGSL std140 SpriteCam (128 bytes).
        assert_eq!(size_of::<SpriteCam>(), 128);
    }

    #[test]
    fn sprite_instance_layout() {
        // Must byte-match the WGSL std430 SpriteInstance (64 bytes).
        assert_eq!(size_of::<SpriteInstance>(), 64);
    }

    #[test]
    fn psprite_instance_layout() {
        // Must byte-match the WGSL std430 PspriteInstance (48 bytes).
        assert_eq!(size_of::<PspriteInstance>(), 48);
    }

    #[test]
    fn atlas_rect_layout() {
        // Must byte-match the WGSL std430 AtlasRect (32 bytes) in scene.wgsl.
        assert_eq!(size_of::<AtlasRect>(), 32);
    }
}
