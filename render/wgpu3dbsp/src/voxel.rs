//! Voxel pass: instanced exposed faces (the GPU analogue of software3d's voxel
//! rasterizer). Each loaded model bakes its exposed-face list once into a static
//! GPU buffer ([`GpuVoxelModel`]). Per frame [`VoxelScratch::collect`] walks the
//! thinglists, selects voxel-eligible things (a model exists for the sprite/frame
//! and the thing is within distance), resolves each to a [`VoxelInstance`] via
//! the shared [`voxel_transform`], and groups instances by model.
//! [`VoxelPipeline`] then issues one instanced draw per model, reusing the sprite
//! pass's [`SpriteCam`] + [`LightParams`] uniforms.
//!
//! Selection mirrors the sprite pass exactly (the sprite collect skips the same
//! eligible things via [`is_voxel_eligible`]) so each thing draws once.

use std::cmp::Ordering;
use std::ptr;
use std::slice;

use bytemuck::{Zeroable as _, cast_slice};
use gameplay::{MapObjFlag, MapObject, SectorExt};
use level::Sector;
use math::FixedT;
use pic_data::{PicData, VoxelManager};
use render_common::{RenderView, VoxelTransformIn, voxel_transform};
use wgpu::util::DeviceExt as _;

use crate::light::LightParams;
use crate::shaders::{bind_storage_entry, bind_uniform_entry};
use crate::sprites::SpriteCam;

/// 6 vertices per face quad (two triangles).
const VERTS_PER_FACE: u32 = 6;
/// Max XY distance (map units) a thing renders as a voxel; beyond this it falls
/// back to a sprite. Matches software3d's `VOXEL_MAX_DIST`.
const VOXEL_MAX_DIST: f32 = 666.0;
const VOXEL_MAX_DIST_SQ: f32 = VOXEL_MAX_DIST * VOXEL_MAX_DIST;
/// Thing frame flags (Doom): full-bright bit + frame-index mask.
const FF_FULLBRIGHT: u32 = 0x8000;
const FF_FRAMEMASK: u32 = 0x7FFF;
/// `VoxelInstance.flags` bit0: spectre/shadow → fuzz pipeline.
const VOXEL_FLAG_FUZZ: u32 = 1;

/// Is this thing rendered as a voxel (a model exists and it's within distance)?
/// The single source both the voxel collect and the sprite-skip consult, so a
/// thing is never drawn as both.
pub fn is_voxel_eligible(
    mgr: &VoxelManager,
    thing: &MapObject,
    player_x: f32,
    player_y: f32,
) -> bool {
    let sprnum = thing.state.sprite as u32 as usize;
    let frame = (thing.frame & FF_FRAMEMASK) as usize;
    if mgr.get(sprnum, frame).is_none() {
        return false;
    }
    let dx = player_x - thing.x.to_f32();
    let dy = player_y - thing.y.to_f32();
    dx * dx + dy * dy <= VOXEL_MAX_DIST_SQ
}

/// One exposed face's static GPU data. Flat scalars to byte-match the WGSL
/// `VoxelFace` (std430, 32 bytes); see `voxel.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuFace {
    px: u32,
    py: u32,
    pz: u32,
    axis: u32,
    rgba: u32,
    sign: i32,
    _pad: [u32; 2],
}

/// One visible voxel-thing's per-frame transform. Byte-matches the WGSL
/// `VoxelInstance` (std430, 48 bytes); see `voxel.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct VoxelInstance {
    wx: f32,
    wy: f32,
    wz: f32,
    cos_a: f32,
    sin_a: f32,
    pvx: f32,
    pvy: f32,
    pvz: f32,
    brightness: u32,
    flags: u32,
    _pad: [u32; 2],
}

/// A model's baked face buffer (built once at load; re-baked on palette change).
/// The `BindGroup` retains the underlying face buffer, so it is not stored.
pub struct GpuVoxelModel {
    face_bind: wgpu::BindGroup,
    /// Face count (× [`VERTS_PER_FACE`] = vertex count of the instanced draw).
    count: u32,
}

/// Per-model instance grouping for one frame: which model, the instance range in
/// the shared upload buffer, and the opaque/fuzz split within that range.
struct ModelBatch {
    model: usize,
    start: u32,
    opaque_end: u32,
    end: u32,
}

/// Per-frame CPU voxel state: the sort scratch, the contiguous per-model instance
/// upload buffer, and the per-model batch table.
#[derive(Default)]
pub struct VoxelScratch {
    /// (model index, squared XY dist, instance), grouped + sorted before upload.
    sort: Vec<(usize, f32, VoxelInstance)>,
    /// Contiguous upload scratch, ordered by model then opaque-before-fuzz.
    instances: Vec<VoxelInstance>,
    /// One batch per model that has visible instances this frame.
    batches: Vec<ModelBatch>,
}

/// Per-frame inputs for in-walk voxel collection, computed once before the
/// world walk.
pub struct VoxelCollectCtx<'a> {
    view: &'a RenderView,
    mgr: &'a VoxelManager,
    player_x: f32,
    player_y: f32,
}

impl<'a> VoxelCollectCtx<'a> {
    pub fn new(view: &'a RenderView, mgr: &'a VoxelManager) -> Self {
        Self {
            view,
            mgr,
            player_x: view.x.into(),
            player_y: view.y.into(),
        }
    }
}

impl VoxelScratch {
    /// Reset the collection scratch; the world walk then appends per visible
    /// sector via [`Self::collect_in_sector`], and [`Self::finish_collect`]
    /// groups the result by model.
    pub fn begin_collect(&mut self) {
        self.sort.clear();
    }

    /// Select voxel-eligible things in one sector's thinglist and resolve each
    /// to a [`VoxelInstance`]. Called from the world walk for each
    /// frustum-visible sector; GPU depth handles occlusion.
    pub fn collect_in_sector(&mut self, ctx: &VoxelCollectCtx, sector: &Sector) {
        let VoxelCollectCtx {
            view,
            mgr,
            player_x,
            player_y,
        } = *ctx;
        let frac = view.frac;
        let extralight = view.extralight;
        let light_level = sector.lightlevel >> 4;

        <Sector as SectorExt>::run_func_on_thinglist(sector, |thing: &MapObject| {
            if ptr::from_ref(thing) as usize == view.player_mobj_id {
                return true;
            }
            // One model lookup: presence + index in a single call; the
            // distance gate matches is_voxel_eligible (the sprite-skip uses
            // that shared form, this path already holds the lookup result).
            let sprnum = thing.state.sprite as u32 as usize;
            let frame = (thing.frame & FF_FRAMEMASK) as usize;
            let Some((model_idx, vslices)) = mgr.get_indexed(sprnum, frame) else {
                return true;
            };
            let dx = player_x - thing.x.to_f32();
            let dy = player_y - thing.y.to_f32();
            if dx * dx + dy * dy > VOXEL_MAX_DIST_SQ {
                return true;
            }

            let lerp =
                |prev: FixedT, curr: FixedT| prev.to_f32() + (curr.to_f32() - prev.to_f32()) * frac;
            let t = voxel_transform(
                vslices,
                &VoxelTransformIn {
                    base_x: lerp(thing.prev_x, thing.x),
                    base_y: lerp(thing.prev_y, thing.y),
                    base_z: lerp(thing.prev_z, thing.z),
                    thing_angle_rad: thing.angle.rad(),
                    player_x,
                    player_y,
                    game_tic: view.game_tic,
                    frac,
                    dropped: thing.flags.contains(MapObjFlag::Dropped),
                    fullbright: thing.frame & FF_FULLBRIGHT != 0,
                    light_level,
                    extralight,
                },
            );

            let flags = if thing.flags.contains(MapObjFlag::Shadow) {
                VOXEL_FLAG_FUZZ
            } else {
                0
            };
            let dx = player_x - t.pos[0];
            let dy = player_y - t.pos[1];
            self.sort.push((
                model_idx,
                dx * dx + dy * dy,
                VoxelInstance {
                    wx: t.pos[0],
                    wy: t.pos[1],
                    wz: t.pos[2],
                    cos_a: t.angle_rad.cos(),
                    sin_a: t.angle_rad.sin(),
                    pvx: vslices.xpivot,
                    pvy: vslices.ypivot,
                    pvz: vslices.zpivot,
                    brightness: t.brightness as u32,
                    flags,
                    _pad: [0; 2],
                },
            ));
            true
        });
    }

    /// Group the collected instances by model, opaque before fuzz within each
    /// model, ready for upload.
    pub fn finish_collect(&mut self) {
        // Group by model (depth ordering within a model is handled by the GPU
        // depth buffer; the fuzz split must be one contiguous run per model so
        // each draw is a single pipeline). Opaque before fuzz within a model.
        self.sort.sort_unstable_by(|a, b| {
            a.0.cmp(&b.0)
                .then((a.2.flags & VOXEL_FLAG_FUZZ).cmp(&(b.2.flags & VOXEL_FLAG_FUZZ)))
                .then(b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal))
        });

        self.instances.clear();
        self.batches.clear();
        let mut i = 0;
        while i < self.sort.len() {
            let model = self.sort[i].0;
            let start = self.instances.len() as u32;
            let mut opaque_end = start;
            while i < self.sort.len() && self.sort[i].0 == model {
                let inst = self.sort[i].2;
                if inst.flags & VOXEL_FLAG_FUZZ == 0 {
                    opaque_end += 1;
                }
                self.instances.push(inst);
                i += 1;
            }
            let end = self.instances.len() as u32;
            self.batches.push(ModelBatch {
                model,
                start,
                opaque_end,
                end,
            });
        }
    }

    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }
}

/// GPU resources for the voxel pass: per-model face buffers, the shared
/// per-instance transform buffer, and the opaque + fuzz pipelines. Reuses the
/// sprite pass's `SpriteCam` (group 0) and `LightParams` (group 3) uniforms.
pub struct VoxelPipeline {
    pipeline: wgpu::RenderPipeline,
    fuzz_pipeline: wgpu::RenderPipeline,
    cam_buf: wgpu::Buffer,
    cam_bind: wgpu::BindGroup,
    face_layout: wgpu::BindGroupLayout,
    inst_layout: wgpu::BindGroupLayout,
    inst_buf: wgpu::Buffer,
    inst_bind: wgpu::BindGroup,
    inst_capacity: u32,
    light_buf: wgpu::Buffer,
    light_bind: wgpu::BindGroup,
    /// One per loaded model, parallel to `VoxelManager`'s model order.
    models: Vec<GpuVoxelModel>,
}

impl VoxelPipeline {
    pub fn new(device: &wgpu::Device) -> Self {
        let cam_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("voxel_camera"),
            size: size_of::<SpriteCam>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let cam_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("voxel_camera_bgl"),
            entries: &[bind_uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT)],
        });
        let cam_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("voxel_camera_bg"),
            layout: &cam_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: cam_buf.as_entire_binding(),
            }],
        });

        let face_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("voxel_face_bgl"),
            entries: &[bind_storage_entry(0, wgpu::ShaderStages::VERTEX)],
        });
        let inst_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("voxel_inst_bgl"),
            entries: &[bind_storage_entry(0, wgpu::ShaderStages::VERTEX)],
        });
        let (inst_buf, inst_bind, inst_capacity) = make_inst_buffer(device, &inst_layout, 1);

        let light_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("voxel_light"),
            size: size_of::<LightParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let light_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("voxel_light_bgl"),
            entries: &[bind_uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT)],
        });
        let light_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("voxel_light_bg"),
            layout: &light_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: light_buf.as_entire_binding(),
            }],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("voxel_shader"),
            source: wgpu::ShaderSource::Wgsl(crate::shaders::VOXEL_SRC.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("voxel_pipeline_layout"),
            bind_group_layouts: &[
                Some(&cam_layout),
                Some(&face_layout),
                Some(&inst_layout),
                Some(&light_layout),
            ],
            immediate_size: 0,
        });
        let pipeline = make_pipeline(
            device,
            &layout,
            &shader,
            "fs_main",
            wgpu::BlendState::REPLACE,
            "voxel_pipeline",
        );
        // Spectre fuzz: RGB-halve the background, depth-write on (nearest spectre
        // wins per pixel), matching the sprite fuzz pass.
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
            "voxel_fuzz_pipeline",
        );

        Self {
            pipeline,
            fuzz_pipeline,
            cam_buf,
            cam_bind,
            face_layout,
            inst_layout,
            inst_buf,
            inst_bind,
            inst_capacity,
            light_buf,
            light_bind,
            models: Vec::new(),
        }
    }

    /// Bake (or re-bake) the per-model face buffers from the manager. Called on
    /// first build and on a palette-generation change (face colours are resolved
    /// through the base palette). `palette` is `pic_data.palettes()[0]`.
    pub fn build_models(&mut self, device: &wgpu::Device, mgr: &VoxelManager, pic_data: &PicData) {
        let palette = &pic_data.palettes()[0];
        self.models.clear();
        for face_list in mgr.faces() {
            let faces: Vec<GpuFace> = face_list
                .iter()
                .map(|f| GpuFace {
                    px: f.pos[0] as u32,
                    py: f.pos[1] as u32,
                    pz: f.pos[2] as u32,
                    axis: f.axis as u32,
                    rgba: palette.0[f.pal_idx as usize],
                    sign: f.sign as i32,
                    _pad: [0; 2],
                })
                .collect();
            // An empty face list can't bind a zero-size storage buffer; pad to a
            // single zeroed face drawn with instance count 0 (count stays 0).
            let upload: &[GpuFace] = if faces.is_empty() {
                &[GpuFace::zeroed()]
            } else {
                &faces
            };
            let face_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("voxel_faces"),
                contents: cast_slice(upload),
                usage: wgpu::BufferUsages::STORAGE,
            });
            let face_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("voxel_face_bg"),
                layout: &self.face_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: face_buf.as_entire_binding(),
                }],
            });
            self.models.push(GpuVoxelModel {
                face_bind,
                count: faces.len() as u32,
            });
        }
    }

    pub fn has_models(&self) -> bool {
        !self.models.is_empty()
    }

    /// Drop all baked model face buffers (voxels toggled off).
    pub fn clear_models(&mut self) {
        self.models.clear();
    }

    /// Upload the frame's instances + camera/light, then draw one instanced batch
    /// per model. Depth loads (scene + sprites wrote it) and writes.
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        scene_view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
        cam: &SpriteCam,
        light: &LightParams,
        scratch: &VoxelScratch,
    ) {
        queue.write_buffer(&self.cam_buf, 0, cast_slice(slice::from_ref(cam)));
        queue.write_buffer(&self.light_buf, 0, cast_slice(slice::from_ref(light)));
        self.upload_instances(device, queue, &scratch.instances);

        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("voxel_pass"),
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
        if scratch.instances.is_empty() {
            return;
        }
        rpass.set_bind_group(0, &self.cam_bind, &[]);
        rpass.set_bind_group(2, &self.inst_bind, &[]);
        rpass.set_bind_group(3, &self.light_bind, &[]);
        for batch in &scratch.batches {
            let Some(model) = self.models.get(batch.model) else {
                continue;
            };
            if model.count == 0 {
                continue;
            }
            rpass.set_bind_group(1, &model.face_bind, &[]);
            let verts = 0..model.count * VERTS_PER_FACE;
            if batch.opaque_end > batch.start {
                rpass.set_pipeline(&self.pipeline);
                rpass.draw(verts.clone(), batch.start..batch.opaque_end);
            }
            if batch.end > batch.opaque_end {
                rpass.set_pipeline(&self.fuzz_pipeline);
                rpass.draw(verts, batch.opaque_end..batch.end);
            }
        }
    }

    /// Grow + rebind the instance buffer when the count exceeds capacity, then
    /// upload. Empty frames are a no-op.
    fn upload_instances(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        insts: &[VoxelInstance],
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
}

/// Allocate an instance storage buffer (≥1 entry) + its bind group.
fn make_inst_buffer(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    capacity: u32,
) -> (wgpu::Buffer, wgpu::BindGroup, u32) {
    let capacity = capacity.max(1);
    let buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("voxel_instances"),
        size: capacity as u64 * size_of::<VoxelInstance>() as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("voxel_inst_bg"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buf.as_entire_binding(),
        }],
    });
    (buf, bind, capacity)
}

/// Build a voxel render pipeline. Shared `vs_main`, back-face cull off (faces are
/// single-sided by construction but yaw can flip winding); depth-tested
/// `LessEqual`, depth-write on (opaque + fuzz both write, like the sprite pass).
fn make_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    fs_entry: &str,
    blend: wgpu::BlendState,
    label: &str,
) -> wgpu::RenderPipeline {
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
        depth_stencil: Some(wgpu::DepthStencilState {
            format: crate::scene::DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fs_entry),
            targets: &[Some(wgpu::ColorTargetState {
                format: crate::scene::SCENE_FORMAT,
                blend: Some(blend),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        multiview_mask: None,
        cache: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voxel_face_layout() {
        // Must byte-match the WGSL std430 VoxelFace (32 bytes).
        assert_eq!(size_of::<GpuFace>(), 32);
    }

    #[test]
    fn voxel_instance_layout() {
        // Must byte-match the WGSL std430 VoxelInstance (48 bytes).
        assert_eq!(size_of::<VoxelInstance>(), 48);
    }
}
