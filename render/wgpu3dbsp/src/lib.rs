//! GPU renderer with a CPU BSP front-end (`wgpu3dbsp`).
//!
//! Forked from `wgpu3d` (which throws the whole level at the GPU every frame).
//! This crate walks the BSP front-to-back on the CPU, frustum-culls node/leaf
//! AABBs, backface-culls polygons, and uploads only the visible corner ids as
//! an index buffer; sprite/voxel instances are collected from each visible
//! leaf's sectors in the same walk. All heavy data stays GPU-resident.
//!
//! A renderer only: records the player view into the scene/depth textures of a
//! borrowed [`GpuHandle`]. The backend owns presentation — UI, composite, and
//! present are all backend-side.

use std::sync::Arc;

use glam::{Mat4, Vec3};
use level::LevelData;
use pic_data::{PicData, VoxelManager};
use render_common::{RenderView, og_projection};

mod assets;
mod camera;
mod cull;
mod geometry;
mod light;
mod scene;
mod screen_effects;
mod shaders;
mod sky;
mod sprites;
mod voxel;

use assets::Atlas;
use camera::{CameraUniform, MAX_PITCH};
use cull::{Frustum, WorldWalk};
use geometry::{CornerAttr, Mesh, Position, corner_attr_of};
use light::LightParams;
pub use light::RenderConfig;
pub use scene::{DEPTH_FORMAT, SCENE_FORMAT};
use scene::{GpuAtlas, GpuMesh, ScenePipeline};
pub use screen_effects::SceneEffects;
use sky::{Sky, SkyMode};
use sprites::{SpriteCam, SpriteCollectCtx, SpritePipeline, SpriteScratch};
use voxel::{VoxelCollectCtx, VoxelPipeline, VoxelScratch};

/// Doom tic rate; converts `game_tic` to seconds for the dynamic sky scroll.
const TICS_PER_SEC: f32 = 35.0;

/// Per-frame GPU handles the renderer draws into. Borrowed from the backend; the
/// scene pass records into `encoder` targeting `scene_view`/`depth_view`.
pub struct GpuHandle<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub encoder: &'a mut wgpu::CommandEncoder,
    pub scene_view: &'a wgpu::TextureView,
    pub depth_view: &'a wgpu::TextureView,
}

/// The level mesh on the GPU, rebuilt on level change.
struct LevelMesh {
    gpu: GpuMesh,
    /// Per-polygon `(first corner, count)` for the world walk (CPU-only).
    poly_corner_range: Vec<(u32, u32)>,
    /// BSP3D vertex count when built, used to detect a level change.
    bsp_vertex_count: usize,
}

/// The GPU renderer. Owns pipelines and GPU resources across frames; lazy-built
/// on the first frame from the borrowed device.
pub struct Wgpu3D {
    projection: Mat4,
    /// Horizontal/vertical frustum FOV (radians) for the world-walk culling,
    /// from the same OG projection the camera matrix uses.
    hfov: f32,
    vfov: f32,
    width: f32,
    view_height: f32,
    scene: Option<ScenePipeline>,
    mesh: Option<LevelMesh>,
    /// Wall + flat atlases, baked once from the WAD textures.
    atlas: Option<GpuAtlas>,
    /// `pic_data.palette_generation()` the atlases were baked at; re-bake on
    /// change (CRT gamma re-tones the palette the atlas pixels resolve through).
    atlas_palette_gen: u64,
    /// Sky (static + dynamic), rebuilt when the episode sky texture changes.
    sky: Option<Sky>,
    /// `pic_data.sky_pic()` the current `sky` was built from (rebuild on change).
    sky_pic: usize,
    sky_mode: SkyMode,
    /// Reused position upload scratch (rebuilt only when geometry is dirty).
    positions: Vec<Position>,
    /// Reused per-sector light scratch, uploaded each frame.
    sector_light: Vec<f32>,
    /// Reused animation translation scratch (wall/flat), uploaded each frame.
    wall_xlat: Vec<u32>,
    flat_xlat: Vec<u32>,
    /// Reused per-corner attr/scroll scratch, re-fanned on texture_dirty.
    corner_attr: Vec<CornerAttr>,
    corner_scroll: Vec<f32>,
    /// Reused per-corner UV scratch, fanned from BSP3D poly_vertex_uv at upload
    /// and re-fanned on geometry_dirty (mover UV re-bake).
    corner_uv: Vec<[f32; 2]>,
    /// Reused visible-corner index scratch, rebuilt by the world walk each frame.
    indices: Vec<u32>,
    /// Per-sector seen flags for in-walk entity collection (reset each frame).
    seen_sectors: Vec<bool>,
    /// Sprite + weapon-psprite pass, built once (atlas baked from the WAD).
    sprite_pipeline: Option<SpritePipeline>,
    /// Per-frame sprite CPU state (selection, sort, upload scratch).
    sprites: SpriteScratch,
    /// Voxel pass: per-model face buffers + pipelines. Built lazily; models are
    /// (re)baked when `voxel_manager` is set or the palette generation changes.
    voxel_pipeline: Option<VoxelPipeline>,
    /// Per-frame voxel CPU state (selection, per-model instance grouping).
    voxels: VoxelScratch,
    /// Active voxel model set (set via `set_voxel_manager`). `None` → the voxels
    /// config flag is off, so all things fall back to sprites.
    voxel_manager: Option<Arc<VoxelManager>>,
    /// Palette generation the voxel face buffers were baked at; re-bake on change.
    voxel_palette_gen: u64,
}

impl Wgpu3D {
    /// `width`/`view_height` are the engine buffer dims; `fov` in radians.
    pub fn new(width: f32, view_height: f32, fov: f32) -> Self {
        let (hfov, vfov, _) = og_projection(fov, width, view_height);
        Self {
            projection: CameraUniform::projection(fov, width, view_height),
            hfov,
            vfov,
            width,
            view_height,
            scene: None,
            mesh: None,
            atlas: None,
            atlas_palette_gen: u64::MAX,
            sky: None,
            sky_pic: usize::MAX,
            sky_mode: SkyMode::Static,
            positions: Vec::new(),
            sector_light: Vec::new(),
            wall_xlat: Vec::new(),
            flat_xlat: Vec::new(),
            corner_attr: Vec::new(),
            corner_scroll: Vec::new(),
            corner_uv: Vec::new(),
            indices: Vec::new(),
            seen_sectors: Vec::new(),
            sprite_pipeline: None,
            sprites: SpriteScratch::default(),
            voxel_pipeline: None,
            voxels: VoxelScratch::default(),
            voxel_manager: None,
            voxel_palette_gen: u64::MAX,
        }
    }

    /// Set the active voxel model set (the voxels config flag turned on). Models
    /// are baked lazily on the next frame against the current palette.
    pub fn set_voxel_manager(&mut self, mgr: Arc<VoxelManager>) {
        self.voxel_manager = Some(mgr);
        // Force a re-bake on the next frame (the model set changed).
        self.voxel_palette_gen = u64::MAX;
    }

    /// Clear the voxel model set (the voxels config flag turned off). All things
    /// then fall back to sprites.
    pub fn clear_voxel_manager(&mut self) {
        self.voxel_manager = None;
        if let Some(vp) = &mut self.voxel_pipeline {
            vp.clear_models();
        }
    }

    /// Select the procedural cloud sky (true) or the static SKY1 texture.
    pub fn set_dynamic_sky(&mut self, dynamic: bool) {
        self.sky_mode = if dynamic {
            SkyMode::Dynamic
        } else {
            SkyMode::Static
        };
    }

    /// Render the player view into `frame.scene_view`.
    ///
    /// Quake-style staged frame (GL Quake `R_RenderScene`): resources → setup
    /// (camera/frustum/live uploads) → world walk (cull + collect) → world
    /// draw → entities → translucent. PVS leaf-marking would slot between
    /// setup and the walk; culling is frustum-only today.
    pub fn draw_view_gpu(
        &mut self,
        view: &RenderView,
        level_data: &LevelData,
        pic_data: &PicData,
        config: &RenderConfig,
        frame: &mut GpuHandle<'_>,
    ) {
        self.ensure_resources(level_data, pic_data, frame);
        let (light, frustum) = self.frame_setup(view, level_data, pic_data, config, frame);
        let visible = self.walk_world(view, level_data, pic_data, &frustum);
        self.draw_world(visible, frame);
        self.draw_entities(view, &light, frame);
        self.draw_translucent(view, pic_data, &light, frame);
    }

    /// Build/refresh long-lived GPU resources: pipelines, atlases, sky, the
    /// level mesh, the voxel model bake, and the mover/switch dirty re-uploads.
    fn ensure_resources(
        &mut self,
        level_data: &LevelData,
        pic_data: &PicData,
        frame: &GpuHandle<'_>,
    ) {
        let scene = self
            .scene
            .get_or_insert_with(|| ScenePipeline::new(frame.device));

        // Atlases are static per WAD; bake once. Per-layer height caps at the
        // device 2D limit; large WADs spill across array layers.
        // CRT gamma re-tones the palette the atlas pixels resolve through; re-bake
        // the atlases in place when the palette generation changes (else only on
        // first build). Dimensions are palette-independent, so textures are reused.
        let palette_gen = pic_data.palette_generation();
        let palette_changed = self.atlas_palette_gen != palette_gen;
        if self.atlas.is_none() || palette_changed {
            let max_dim = frame.device.limits().max_texture_dimension_2d;
            let walls = Atlas::walls(pic_data, max_dim);
            let flats = Atlas::flats(pic_data, max_dim);
            match &self.atlas {
                Some(atlas) if palette_changed => atlas.reupload(frame.queue, &walls, &flats),
                _ => {
                    self.atlas =
                        Some(scene.upload_atlases(frame.device, frame.queue, &walls, &flats));
                }
            }
        }
        // Sprite atlas + pipeline are static per WAD; bake once, re-bake pixels on
        // a palette change. The scratch keeps the per-patch pivot/rect tables.
        if self.sprite_pipeline.is_none() || palette_changed {
            let max_dim = frame.device.limits().max_texture_dimension_2d;
            let sprites = Atlas::sprites(pic_data, max_dim);
            match &self.sprite_pipeline {
                Some(sp) if palette_changed => sp.reupload_atlas(frame.queue, &sprites.atlas),
                _ => {
                    self.sprite_pipeline =
                        Some(SpritePipeline::new(frame.device, frame.queue, &sprites));
                    self.sprites.set_atlas(&sprites);
                }
            }
        }
        self.atlas_palette_gen = palette_gen;
        // Sky texture is per-episode (set_sky_pic on level load); rebuild on change.
        if self.sky.is_none() || self.sky_pic != pic_data.sky_pic() {
            self.sky = Some(Sky::new(frame.device, frame.queue, pic_data));
            self.sky_pic = pic_data.sky_pic();
        }

        let bsp3d = level_data.bsp_3d();
        let rebuild = self
            .mesh
            .as_ref()
            .is_none_or(|m| m.bsp_vertex_count != bsp3d.vertices.len());
        if rebuild {
            let mesh = Mesh::build(bsp3d);
            bsp3d.fan_corner_uv(&mut self.corner_uv);
            self.mesh = Some(LevelMesh {
                gpu: scene.upload_mesh(
                    frame.device,
                    &mesh,
                    &self.corner_uv,
                    level_data.sectors.len(),
                ),
                poly_corner_range: mesh.poly_corner_range,
                bsp_vertex_count: bsp3d.vertices.len(),
            });
        }

        // Geometry: re-upload positions/UV only when a surface moved (or rebuild).
        if rebuild || bsp3d.geometry_dirty() {
            self.positions.clear();
            self.positions
                .extend(bsp3d.vertices.iter().map(|p| Position {
                    pos: [p.x, p.y, p.z, 1.0],
                }));
            bsp3d.fan_corner_uv(&mut self.corner_uv);
            let mesh = self.mesh.as_ref().expect("mesh built above");
            mesh.gpu.update_positions(frame.queue, &self.positions);
            mesh.gpu.update_corner_uv(frame.queue, &self.corner_uv);
        }

        // Switch/scroll re-fan, scoped to the dirty polygons' corner spans;
        // rebuild or cap spill falls back to the whole map.
        if rebuild || bsp3d.texture_dirty() {
            let mesh = self.mesh.as_ref().expect("mesh built above");
            if let (false, Some(dirty)) = (rebuild, bsp3d.texture_dirty_polys()) {
                for &gi in dirty {
                    let (start, count) = mesh.poly_corner_range[gi];
                    if count == 0 {
                        continue;
                    }
                    let attr = corner_attr_of(bsp3d, gi);
                    self.corner_attr.clear();
                    self.corner_attr.resize(count as usize, attr);
                    mesh.gpu
                        .update_corner_attr_range(frame.queue, start, &self.corner_attr);
                    self.corner_scroll.clear();
                    self.corner_scroll
                        .resize(count as usize, bsp3d.poly_scroll[gi]);
                    mesh.gpu
                        .update_corner_scroll_range(frame.queue, start, &self.corner_scroll);
                }
            } else {
                bsp3d.fan_corner_attr(&mut self.corner_attr, |p| corner_attr_of(bsp3d, p));
                bsp3d.fan_corner_attr(&mut self.corner_scroll, |p| bsp3d.poly_scroll[p]);
                mesh.gpu.update_corner_attr(frame.queue, &self.corner_attr);
                mesh.gpu
                    .update_corner_scroll(frame.queue, &self.corner_scroll);
            }
        }

        // Voxel models: bake per-model face buffers on first use and whenever the
        // model set or palette generation changes (face colours resolve through
        // the base palette). Built before collection so the pass has buffers.
        if let Some(mgr) = &self.voxel_manager {
            let vp = self
                .voxel_pipeline
                .get_or_insert_with(|| VoxelPipeline::new(frame.device));
            if self.voxel_palette_gen != palette_gen || !vp.has_models() {
                vp.build_models(frame.device, mgr, pic_data);
                self.voxel_palette_gen = palette_gen;
            }
        }
    }

    /// Per-frame setup: live buffer uploads (sector light, animation
    /// translation), camera + light uniforms, sky params, and the view frustum
    /// for the world walk.
    fn frame_setup(
        &mut self,
        view: &RenderView,
        level_data: &LevelData,
        pic_data: &PicData,
        config: &RenderConfig,
        frame: &GpuHandle<'_>,
    ) -> (LightParams, Frustum) {
        // Sector light is live (flicker/specials); upload every frame.
        self.sector_light.clear();
        self.sector_light
            .extend(level_data.sectors.iter().map(|s| s.lightlevel as f32));
        let mesh = self.mesh.as_ref().expect("mesh built in ensure_resources");
        mesh.gpu
            .update_sector_light(frame.queue, &self.sector_light);

        // Animation translation tables are live (per-tic); upload every frame.
        self.wall_xlat.clear();
        self.wall_xlat
            .extend(pic_data.wall_translation().iter().map(|&t| t as u32));
        self.flat_xlat.clear();
        self.flat_xlat
            .extend(pic_data.flat_translation().iter().map(|&t| t as u32));
        let atlas = self
            .atlas
            .as_ref()
            .expect("atlas built in ensure_resources");
        atlas.update_translation(frame.queue, &self.wall_xlat, &self.flat_xlat);

        let scene = self
            .scene
            .as_ref()
            .expect("scene built in ensure_resources");
        let camera = CameraUniform::new(view, self.projection);
        scene.set_camera(frame.queue, &camera);
        // Light params (gamma/falloff) — one source for scene + sprites + psprite.
        let light = LightParams::new(config);
        scene.set_light(frame.queue, &light);

        let sky = self.sky.as_ref().expect("sky built in ensure_resources");
        sky.set_params(
            frame.queue,
            view,
            self.projection,
            self.width,
            self.view_height,
            self.sky_mode,
            view.game_tic as f32 / TICS_PER_SEC,
        );

        // World-space side planes from the same view basis the camera uses.
        let camera_pos = Vec3::new(view.x.into(), view.y.into(), view.viewz.into());
        let pitch = view.lookdir.clamp(-MAX_PITCH, MAX_PITCH);
        let frustum = Frustum::new(camera_pos, view.angle.rad(), pitch, self.hfov, self.vfov);
        (light, frustum)
    }

    /// The world walk: front-to-back BSP traverse with frustum + backface
    /// culling, emitting visible corner ids and collecting sprite/voxel
    /// instances from each visible leaf's sectors. Returns the visible corner
    /// count.
    fn walk_world(
        &mut self,
        view: &RenderView,
        level_data: &LevelData,
        pic_data: &PicData,
        frustum: &Frustum,
    ) -> u32 {
        let bsp3d = level_data.bsp_3d();
        let Self {
            mesh,
            indices,
            seen_sectors,
            sprites,
            voxels,
            voxel_manager,
            projection,
            ..
        } = self;
        let mesh = mesh.as_ref().expect("mesh built in ensure_resources");
        indices.clear();
        seen_sectors.clear();
        seen_sectors.resize(level_data.sectors.len(), false);

        // Voxel-eligible things are skipped by the sprite collect (voxel_mgr)
        // and collected for the voxel pass instead, so each thing renders once.
        let voxel_mgr = voxel_manager.as_deref();
        let sprite_ctx = SpriteCollectCtx::new(view, pic_data, *projection, voxel_mgr);
        sprites.begin_collect();
        let voxel_ctx = voxel_mgr.map(|mgr| VoxelCollectCtx::new(view, mgr));
        if voxel_ctx.is_some() {
            voxels.begin_collect();
        }

        let camera_pos = Vec3::new(view.x.into(), view.y.into(), view.viewz.into());
        let mut walk = WorldWalk {
            bsp3d,
            sectors: &level_data.sectors,
            frustum,
            camera_pos,
            poly_corner_range: &mesh.poly_corner_range,
            indices: &mut *indices,
            seen_sectors: seen_sectors.as_mut_slice(),
            sprites: &mut *sprites,
            sprite_ctx: &sprite_ctx,
            voxels: voxel_ctx.as_ref().map(|ctx| (&mut *voxels, ctx)),
        };
        walk.walk(bsp3d.root_node(), false);

        sprites.finish_collect();
        if voxel_ctx.is_some() {
            voxels.finish_collect();
        }
        indices.len() as u32
    }

    /// Upload the frame's visible index prefix, fill the sky background, then
    /// draw the world in one indexed pass (which also clears depth for the
    /// entity passes).
    #[expect(
        clippy::needless_pass_by_ref_mut,
        reason = "frame.encoder is reborrowed mutably; &GpuHandle fails E0596"
    )]
    fn draw_world(&mut self, visible: u32, frame: &mut GpuHandle<'_>) {
        let mesh = self.mesh.as_ref().expect("mesh built in ensure_resources");
        if visible > 0 {
            mesh.gpu.update_visible_indices(frame.queue, &self.indices);
        }
        let sky = self.sky.as_ref().expect("sky built in ensure_resources");
        sky.draw_background(frame.encoder, frame.scene_view);
        let atlas = self
            .atlas
            .as_ref()
            .expect("atlas built in ensure_resources");
        let scene = self
            .scene
            .as_ref()
            .expect("scene built in ensure_resources");
        scene.draw(
            frame.encoder,
            &mesh.gpu,
            visible,
            atlas,
            sky.bind(),
            frame.scene_view,
            frame.depth_view,
        );
    }

    /// Sprite billboards then voxels — the depth-tested world entities the walk
    /// collected.
    fn draw_entities(&mut self, view: &RenderView, light: &LightParams, frame: &mut GpuHandle<'_>) {
        let sprite_cam = SpriteCam::new(view, self.projection);
        let voxel_active = self.voxel_manager.is_some();
        let Self {
            sprite_pipeline,
            sprites,
            voxel_pipeline,
            voxels,
            ..
        } = self;
        let sprite_pipeline = sprite_pipeline
            .as_mut()
            .expect("sprite pipeline built in ensure_resources");
        // World billboards first (depth-tested world geometry).
        sprite_pipeline.render_world(
            frame.device,
            frame.queue,
            frame.encoder,
            frame.scene_view,
            frame.depth_view,
            &sprite_cam,
            light,
            sprites,
        );
        // Voxels next (also depth-tested world geometry) — only when active.
        if voxel_active
            && let Some(vp) = voxel_pipeline.as_mut()
            && !voxels.is_empty()
        {
            vp.render(
                frame.device,
                frame.queue,
                frame.encoder,
                frame.scene_view,
                frame.depth_view,
                &sprite_cam,
                light,
                voxels,
            );
        }
    }

    /// Weapon psprite overlay — screen-space, drawn last over everything (fuzz
    /// variant when the player is shadowed).
    fn draw_translucent(
        &mut self,
        view: &RenderView,
        pic_data: &PicData,
        light: &LightParams,
        frame: &mut GpuHandle<'_>,
    ) {
        self.sprites
            .collect_psprites(view, pic_data, light, self.width, self.view_height);
        let Self {
            sprite_pipeline,
            sprites,
            ..
        } = self;
        let sprite_pipeline = sprite_pipeline
            .as_mut()
            .expect("sprite pipeline built in ensure_resources");
        sprite_pipeline.render_psprites(
            frame.queue,
            frame.encoder,
            frame.scene_view,
            sprites,
            view.is_shadow,
        );
    }
}
