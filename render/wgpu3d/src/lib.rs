//! GPU renderer (`wgpu3d`).
//!
//! A renderer only: records the player view into the scene/depth textures of a
//! borrowed [`GpuHandle`]. The backend owns presentation — UI, composite, and
//! present are all backend-side.

use std::sync::Arc;

use glam::Mat4;
use level::LevelData;
use pic_data::{PicData, VoxelManager};
use render_common::RenderView;

mod assets;
mod camera;
mod geometry;
mod light;
mod scene;
mod screen_effects;
mod shaders;
mod sky;
mod sprites;
mod voxel;

use assets::Atlas;
use camera::CameraUniform;
use geometry::{CornerAttr, Mesh, Position, corner_attr_of};
use light::LightParams;
pub use light::RenderConfig;
pub use scene::{DEPTH_FORMAT, SCENE_FORMAT};
use scene::{GpuAtlas, GpuMesh, ScenePipeline};
pub use screen_effects::SceneEffects;
use sky::{Sky, SkyMode};
use sprites::{SpriteCam, SpritePipeline, SpriteScratch};
use voxel::{VoxelPipeline, VoxelScratch};

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
    /// BSP3D vertex count when built, used to detect a level change.
    bsp_vertex_count: usize,
}

/// The GPU renderer. Owns pipelines and GPU resources across frames; lazy-built
/// on the first frame from the borrowed device.
pub struct Wgpu3D {
    projection: Mat4,
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
        Self {
            projection: CameraUniform::projection(fov, width, view_height),
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
    pub fn draw_view_gpu(
        &mut self,
        view: &RenderView,
        level_data: &LevelData,
        pic_data: &PicData,
        config: &RenderConfig,
        frame: &mut GpuHandle<'_>,
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

        // Texture: re-fan per-corner tex/scroll only when a switch or scroller
        // changed (or rebuild). Both fan from the same poly data in one pass.
        if rebuild || bsp3d.texture_dirty() {
            bsp3d.fan_corner_attr(&mut self.corner_attr, |p| corner_attr_of(bsp3d, p));
            bsp3d.fan_corner_attr(&mut self.corner_scroll, |p| bsp3d.poly_scroll[p]);
            let mesh = self.mesh.as_ref().expect("mesh built above");
            mesh.gpu.update_corner_attr(frame.queue, &self.corner_attr);
            mesh.gpu
                .update_corner_scroll(frame.queue, &self.corner_scroll);
        }

        // Sector light is live (flicker/specials); upload every frame.
        self.sector_light.clear();
        self.sector_light
            .extend(level_data.sectors.iter().map(|s| s.lightlevel as f32));
        let mesh = self.mesh.as_ref().expect("mesh built above");
        mesh.gpu
            .update_sector_light(frame.queue, &self.sector_light);

        // Animation translation tables are live (per-tic); upload every frame.
        self.wall_xlat.clear();
        self.wall_xlat
            .extend(pic_data.wall_translation().iter().map(|&t| t as u32));
        self.flat_xlat.clear();
        self.flat_xlat
            .extend(pic_data.flat_translation().iter().map(|&t| t as u32));
        let atlas = self.atlas.as_ref().expect("atlas built above");
        atlas.update_translation(frame.queue, &self.wall_xlat, &self.flat_xlat);

        let camera = CameraUniform::new(view, self.projection);
        scene.set_camera(frame.queue, &camera);
        // Light params (gamma/falloff) — one source for scene + sprites + psprite.
        let light = LightParams::new(config);
        scene.set_light(frame.queue, &light);

        // Sky fills the background first; the scene pass loads over it.
        let sky = self.sky.as_ref().expect("sky built above");
        sky.set_params(
            frame.queue,
            view,
            self.projection,
            self.width,
            self.view_height,
            self.sky_mode,
            view.game_tic as f32 / TICS_PER_SEC,
        );
        sky.draw_background(frame.encoder, frame.scene_view);

        let atlas = self.atlas.as_ref().expect("atlas built above");
        scene.draw(
            frame.encoder,
            &mesh.gpu,
            atlas,
            sky.bind(),
            frame.scene_view,
            frame.depth_view,
        );

        // Voxel models: bake per-model face buffers on first use and whenever the
        // model set or palette generation changes (face colours resolve through
        // the base palette). Built before collection so the pass has buffers.
        let voxel_mgr = self.voxel_manager.clone();
        if let Some(mgr) = &voxel_mgr {
            let vp = self
                .voxel_pipeline
                .get_or_insert_with(|| VoxelPipeline::new(frame.device));
            if self.voxel_palette_gen != palette_gen || !vp.has_models() {
                vp.build_models(frame.device, mgr, pic_data);
                self.voxel_palette_gen = palette_gen;
            }
        }

        // Sprites over the scene, then voxels, then the weapon psprite on top.
        // Voxel-eligible things are skipped by the sprite collect (voxel_mgr) and
        // drawn by the voxel pass instead, so each thing renders once.
        self.sprites.collect(
            view,
            level_data,
            pic_data,
            self.projection,
            voxel_mgr.as_deref(),
        );
        self.sprites
            .collect_psprites(view, pic_data, &light, self.width, self.view_height);
        if let Some(mgr) = &voxel_mgr {
            self.voxels.collect(view, level_data, mgr);
        }
        let sprite_cam = SpriteCam::new(view, self.projection);
        let Self {
            sprite_pipeline,
            sprites,
            voxel_pipeline,
            voxels,
            ..
        } = self;
        let sprite_pipeline = sprite_pipeline
            .as_mut()
            .expect("sprite pipeline built above");
        // World billboards first (depth-tested world geometry).
        sprite_pipeline.render_world(
            frame.device,
            frame.queue,
            frame.encoder,
            frame.scene_view,
            frame.depth_view,
            &sprite_cam,
            &light,
            sprites,
        );
        // Voxels next (also depth-tested world geometry) — only when active.
        if voxel_mgr.is_some()
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
                &light,
                voxels,
            );
        }
        // Weapon psprite last (screen-space, on top of everything).
        sprite_pipeline.render_psprites(
            frame.queue,
            frame.encoder,
            frame.scene_view,
            sprites,
            view.is_shadow,
        );
    }
}
