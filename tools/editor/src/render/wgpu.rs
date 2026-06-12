//! GPU map renderer for the editor canvas.
//!
//! Renders through Slint's `femtovg-wgpu` backend. [`capture`] grabs the
//! device/queue from the window notifier. [`WgpuContext::paint`] draws the
//! cached map mesh + grid into an off-screen `Rgba8Unorm` texture handed back
//! as a `slint::Image` (requires `TEXTURE_BINDING | RENDER_ATTACHMENT`).
//!
//! Layers (draw order):
//! - **surface3d** — sector floors/ceilings/walls at real Z; fill mode drives the shader
//! - **grid / lines / normals** — line instances, constant device-pixel width in VS
//! - **verts / things** — marker and quad instances; things shader draws icon + ring

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::slice;

use bytemuck::{Pod, Zeroable};
use slint::Image;
use slint::wgpu_29::wgpu;
use wgpu::util::{BufferInitDescriptor, DeviceExt as _};

use crate::render::camera3d::{Camera, Mat4};
use crate::render::frame3d::Vert3D;
use crate::render::input::SectorFill;

/// Minimum texture edge; guards against 0-dim texture on empty canvas.
const MIN_DIM: u32 = 1;
/// Depth format: 3D surfaces write, 2D overlays test only.
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const CLEAR: wgpu::Color = wgpu::Color {
    r: 1.0,
    g: 1.0,
    b: 1.0,
    a: 1.0,
};
/// Surface shader `params.x` fill modes. `surface.wgsl` tests `mode < 0.5` (None)
/// and `mode < 1.5` (Colour) — keep in sync.
const FILL_NONE: f32 = 0.0;
const FILL_COLOUR: f32 = 1.0;
const FILL_TEXTURE: f32 = 2.0;
/// Grid fades when the view ray grazes the plane (`|dir.z|`) or the hit recedes
/// past the far threshold, suppressing moiré at oblique angles.
const GRID_GRAZE_FADE_FULL: f32 = 0.05;
const GRID_GRAZE_FADE_START: f32 = 0.005;
const GRID_FAR_FADE_START: f32 = 8192.0;
const GRID_FAR_FADE_END: f32 = 16384.0;
const LINE_WGSL: &str = include_str!("shaders/lines.wgsl");
const THING_WGSL: &str = include_str!("shaders/things.wgsl");
const SURFACE_WGSL: &str = include_str!("shaders/surface.wgsl");
const GRID_WGSL: &str = include_str!("shaders/grid.wgsl");
/// `bytes_per_row` alignment required by wgpu buffer copy.
const COPY_BYTES_PER_ROW_ALIGN: u32 = 256;

// Per-instance vertex layouts; each must match its `#[repr(C)]` struct field order.
/// LineInst: a(2), b(2), half_px(1), az(1), bz(1), rgba(4).
const LINE_VBUF: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![
    0 => Float32x2, 1 => Float32x2, 2 => Float32, 3 => Float32, 4 => Float32, 5 => Float32x4];
/// MarkerInst: centre(2), half_px(1), z(1), rgba(4).
const MARKER_VBUF: [wgpu::VertexAttribute; 4] =
    wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32, 2 => Float32, 3 => Float32x4];
/// ThingInst: centre(2), half(2), uv0(2), uv1(2), rgba(4), radius(1), pad(3).
const THING_VBUF: [wgpu::VertexAttribute; 7] = wgpu::vertex_attr_array![
    0 => Float32x2, 1 => Float32x2, 2 => Float32x2, 3 => Float32x2,
    4 => Float32x4, 5 => Float32, 6 => Float32x3];
/// Vert3D: pos(3), uv(2), atlas_rect(4), sector(1), surface(1), source(1), shade(1), vert(1).
/// Slots 5 (`source`) and 7 (`vert`) are CPU-pick-only — `vs_surface3d` skips them;
/// do not reuse slot 5 or 7.
const SURFACE_VBUF: [wgpu::VertexAttribute; 8] = wgpu::vertex_attr_array![
    0 => Float32x3, 1 => Float32x2, 2 => Float32x4, 3 => Uint32, 4 => Uint32, 5 => Uint32, 6 => Float32, 7 => Uint32];

/// How a pipeline interacts with the depth buffer. 3D surfaces write depth and
/// occlude; the z=0 overlays test against it but never write; the grid ignores
/// depth so it always draws on top of the geometry.
#[derive(Clone, Copy)]
enum DepthMode {
    Surface,
    Opaque,
    Overlay,
    Occluded,
    OnTop,
}

impl DepthMode {
    fn state(self) -> wgpu::DepthStencilState {
        let (compare, write) = match self {
            Self::Surface | Self::Opaque => (wgpu::CompareFunction::Less, true),
            Self::Overlay => (wgpu::CompareFunction::LessEqual, false),
            Self::Occluded => (wgpu::CompareFunction::Greater, false),
            Self::OnTop => (wgpu::CompareFunction::Always, false),
        };
        wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(write),
            depth_compare: Some(compare),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }
    }

    /// Back-cull 3D surfaces only; line quads flip winding with segment direction.
    fn cull_mode(self) -> Option<wgpu::Face> {
        match self {
            Self::Surface => Some(wgpu::Face::Back),
            Self::Opaque | Self::Overlay | Self::Occluded | Self::OnTop => None,
        }
    }
}
/// Per-sector flat attributes indexed by sector. `tile[0] < 0` = no flat → use `fallback`.
/// 16-byte aligned for std430.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct SectorAttr {
    /// Atlas tile origin in texels.
    pub tile: [f32; 2],
    pub _pad: [f32; 2],
    pub fallback: [f32; 4],
}

/// One thing instance: icon quad (`centre` ± `half`), sprite UV rect
/// (`uv0.x < 0` ⇒ colour square), body `radius` for the ring. Fixed stride.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
pub struct ThingInst {
    pub centre: [f32; 2],
    pub half: [f32; 2],
    pub uv0: [f32; 2],
    pub uv1: [f32; 2],
    pub rgba: [f32; 4],
    pub radius: f32,
    /// World Z the billboard's base sits at (sector floor height in 3D, else 0).
    pub z: f32,
    pub _pad: [f32; 2],
}

/// One line instance: world endpoints, device-pixel half-width, colour. The VS
/// expands a shared 6-vertex quad to constant thickness. Fixed stride.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
pub struct LineInst {
    pub a: [f32; 2],
    pub b: [f32; 2],
    pub half_px: f32,
    /// World Z of endpoint `a` (the grid/editing plane, or a wall edge height).
    pub az: f32,
    /// World Z of `b`. Equals `az` for flat lines; spans floor→ceil for wireframe verticals.
    pub bz: f32,
    pub rgba: [f32; 4],
}

/// One vertex marker instance: world centre, device-pixel half-size, colour.
/// Expanded from the base quad in the shader. Fixed stride.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
pub struct MarkerInst {
    pub centre: [f32; 2],
    pub half_px: f32,
    /// World Z (editing plane).
    pub z: f32,
    pub rgba: [f32; 4],
}

/// Camera uniform: world→clip VP, device viewport px, camera right (`xyz`) +
/// tilted flag (`w`) for billboard expansion.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CameraUniform {
    view_proj: Mat4,
    viewport: [f32; 2],
    _pad: [f32; 2],
    cam_right: [f32; 4],
    /// Selection tint the surface shader blends over selected sectors' floors.
    sel_colour: [f32; 4],
    /// `x` = render fill mode (0 None, 1 Colour, 2 Texture).
    params: [f32; 4],
}

/// Grid uniform for `grid.wgsl`. Layout mirrors the WGSL `Grid` struct exactly.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GridUniform {
    inv_view_proj: Mat4,
    view_proj: Mat4,
    /// `xy` = viewport px, `z` = grid_z plane height, `w` = 1 on-top else 0.
    plane: [f32; 4],
    /// `x` = snap spacing, `y` = tile spacing, `z` = min device-px cutoff,
    /// `w` = line half-width in device px.
    params: [f32; 4],
    grid_rgba: [f32; 4],
    tile_rgba: [f32; 4],
    /// `x,y` = graze-fade |dir.z| start/full, `z,w` = far-fade start/end dist.
    fade: [f32; 4],
    /// Camera eye world position (`xyz`); far fade measures ground distance from `xy`.
    eye: [f32; 4],
}

/// Grid appearance (theme colours, snap/tile spacings, fade cutoff, line width).
#[derive(Clone, Copy)]
pub struct GridStyle {
    pub snap: f32,
    pub tile: f32,
    pub grid_rgba: [f32; 4],
    pub tile_rgba: [f32; 4],
    pub min_px: f32,
    pub half_px: f32,
}

impl Default for GridStyle {
    fn default() -> Self {
        Self {
            snap: 8.0,
            tile: 64.0,
            grid_rgba: [0.0; 4],
            tile_rgba: [0.0; 4],
            min_px: 4.0,
            half_px: 1.0,
        }
    }
}

/// Whole-map geometry (no grid, no camera). Draw order: surface3d → lines/normals → verts → things.
/// Fixed-stride instanced layers; a non-topology edit patches one slot, zoom is camera-only.
#[derive(Default)]
pub struct MapFrame {
    /// Per-sector flat attributes (tile + fallback), indexed by sector.
    pub sector_attrs: Vec<SectorAttr>,
    /// Per-sector 3D attributes (heights + flat tiles), indexed by sector.
    pub sector3d: Vec<Sector3D>,
    /// Line instances, 1:1 with linedefs (slot `i` = line `i`). Patchable.
    pub lines: Vec<LineInst>,
    /// Front-normal indicator instances, parallel to `lines` (one per line).
    pub normals: Vec<LineInst>,
    /// Wireframe extras for None mode: wall band outlines + vertex verticals. Rebuilt on geometry change.
    pub wire: Vec<LineInst>,
    /// Vertex marker instances.
    pub verts: Vec<MarkerInst>,
    /// Thing instances (icon quad + dot + selection box in shader).
    pub things: Vec<ThingInst>,
    /// 3D sector-preview mesh (floors/ceilings/walls), shading driven by fill mode.
    pub surface3d: Vec<Vert3D>,
}

/// Atlas rect for a wall texture: origin in the wall atlas + intrinsic size
/// (for texel-space mod-tiling).
#[derive(Clone, Copy, Default)]
pub struct WallRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// RGBA flat + wall + sprite atlases. Built when map/assets change, handed to [`set_atlases`].
#[derive(Default)]
pub struct AtlasData {
    /// RGBA flat atlas: `flat_atlas_w`×`flat_atlas_h`, 4 bytes per texel.
    pub flat_rgba: Vec<u8>,
    pub flat_atlas_w: u32,
    pub flat_atlas_h: u32,
    /// RGBA wall atlas: `wall_atlas_w`×`wall_atlas_h`, 4 bytes per texel.
    /// Alpha 0 = transparent texel (masked mid-textures).
    pub wall_rgba: Vec<u8>,
    pub wall_atlas_w: u32,
    pub wall_atlas_h: u32,
    /// RGBA sprite atlas: `sprite_w`×`sprite_h`.
    pub sprite_rgba: Vec<u8>,
    pub sprite_w: u32,
    pub sprite_h: u32,
    /// Bumped on change; renderer re-uploads only when this differs.
    pub generation: u64,
}

/// Per-sector 3D attributes: floor/ceil heights + flat tiles. std430 stride 32 bytes.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Sector3D {
    pub floor_h: f32,
    pub ceil_h: f32,
    /// Floor flat tile origin in atlas texels.
    pub floor_tile: [f32; 2],
    /// Ceil flat tile origin in atlas texels.
    pub ceil_tile: [f32; 2],
    /// 1.0 when the sector is selected (the surface shader tints its floor).
    pub selected: f32,
    pub _pad: f32,
}

/// Shared `wgpu` device/queue + pipelines, captured from Slint's renderer.
#[derive(Clone, Default)]
pub struct WgpuContext(Rc<RefCell<Option<WgpuRenderer>>>);

impl WgpuContext {
    /// Upload the whole-map mesh. No-op before device capture.
    pub fn upload_map(&self, frame: &MapFrame) {
        if let Some(r) = self.0.borrow_mut().as_mut() {
            r.upload_map(frame);
        }
    }

    /// Drop the cached mesh; nothing paints until the next `upload_map`. No-op before device.
    pub fn clear_map(&self) {
        if let Some(r) = self.0.borrow_mut().as_mut() {
            r.clear_map();
        }
    }

    /// Set the grid appearance (theme colours + snap step). No-op before device capture.
    pub fn set_grid_style(&self, style: GridStyle) {
        if let Some(r) = self.0.borrow().as_ref() {
            r.set_grid_style(style);
        }
    }

    /// Set the editing-plane height the grid draws on.
    pub fn set_grid_z(&self, z: f32) {
        if let Some(r) = self.0.borrow().as_ref() {
            r.set_grid_z(z);
        }
    }

    /// Upload the transient overlay (BSP anim / edit preview). Empty slices clear it.
    pub fn set_overlay(&self, lines: &[LineInst], markers: &[MarkerInst]) {
        if let Some(r) = self.0.borrow_mut().as_mut() {
            r.set_overlay(lines, markers);
        }
    }

    /// Patch one line instance (segment + normal) in place.
    pub fn patch_line(&self, i: u32, inst: LineInst, normal: LineInst) {
        if let Some(r) = self.0.borrow().as_ref() {
            r.patch_line(i, inst, normal);
        }
    }

    /// Patch one vertex marker in place.
    pub fn patch_vert(&self, i: u32, inst: MarkerInst) {
        if let Some(r) = self.0.borrow().as_ref() {
            r.patch_vert(i, inst);
        }
    }

    /// Patch one thing instance in place.
    pub fn patch_thing(&self, i: u32, inst: ThingInst) {
        if let Some(r) = self.0.borrow().as_ref() {
            r.patch_thing(i, inst);
        }
    }

    /// Patch one sector's flat attrs in place.
    pub fn patch_sector_attr(&self, sector: u32, attr: SectorAttr) {
        if let Some(r) = self.0.borrow().as_ref() {
            r.patch_sector_attr(sector, attr);
        }
    }

    /// Patch one sector's 3D attrs in place.
    pub fn patch_sector_3d(&self, sector: u32, sector3d: Sector3D) {
        if let Some(r) = self.0.borrow().as_ref() {
            r.patch_sector_3d(sector, sector3d);
        }
    }

    /// Paint the cached map mesh + grid. `None` before device capture.
    pub fn paint(&self, camera: Camera, width: u32, height: u32) -> Option<Image> {
        self.0
            .borrow()
            .as_ref()
            .map(|r| r.paint(camera, width, height))
    }

    /// Set the themed canvas background; the next `paint` clears to it.
    pub fn set_clear(&self, rgba: [u8; 4]) {
        if let Some(r) = self.0.borrow().as_ref() {
            r.set_clear(rgba);
        }
    }

    /// Set the themed selection tint for selected sectors' 3D floors.
    pub fn set_sel_colour(&self, rgba: [u8; 4]) {
        if let Some(r) = self.0.borrow().as_ref() {
            r.set_sel_colour(rgba);
        }
    }

    /// Set the render fill mode that drives the surface shader.
    pub fn set_fill_mode(&self, mode: SectorFill) {
        if let Some(r) = self.0.borrow().as_ref() {
            r.set_fill_mode(match mode {
                SectorFill::None => FILL_NONE,
                SectorFill::Colour => FILL_COLOUR,
                SectorFill::Texture => FILL_TEXTURE,
            });
        }
    }

    /// Make the grid overdraw all geometry (an edit gesture is active) or be
    /// depth-tested/occluded (default).
    pub fn set_grid_on_top(&self, on: bool) {
        if let Some(r) = self.0.borrow().as_ref() {
            r.grid_on_top.set(on);
        }
    }

    /// Show/hide the overlay layer (grid + lines + vertices) — the spacebar
    /// toggle. The 3D surface, things, and edit preview are unaffected.
    pub fn set_overlays_visible(&self, on: bool) {
        if let Some(r) = self.0.borrow().as_ref() {
            r.overlays_visible.set(on);
        }
    }

    /// Render to `Rgba8Unorm` bytes (PNG export). `None` before device capture.
    pub fn render_canvas_rgba(&self, camera: Camera, width: u32, height: u32) -> Option<Vec<u8>> {
        self.0
            .borrow()
            .as_ref()
            .map(|r| r.render_canvas_rgba(camera, width, height))
    }

    /// Upload atlas textures if `atlas.generation` changed. No-op before device capture.
    pub fn set_atlases(&self, atlas: &AtlasData) {
        if let Some(r) = self.0.borrow_mut().as_mut() {
            r.set_atlases(atlas);
        }
    }

    /// Build per-sector GPU buffers and upload initial data. No-op before device capture.
    pub fn set_sector_data(&self, brightness: &[f32], attrs: &[SectorAttr], sector3d: &[Sector3D]) {
        if let Some(r) = self.0.borrow_mut().as_mut() {
            r.set_sector_data(brightness, attrs, sector3d);
        }
    }

    /// Rewrite the per-sector brightness buffer (light tic). No-op before `set_sector_data`.
    pub fn update_brightness(&self, brightness: &[f32]) {
        if let Some(r) = self.0.borrow().as_ref() {
            r.update_brightness(brightness);
        }
    }

    /// True when the GPU device is ready.
    pub fn is_ready(&self) -> bool {
        self.0.borrow().is_some()
    }

    fn set(&self, renderer: WgpuRenderer) {
        *self.0.borrow_mut() = Some(renderer);
    }
}

/// Owns the device/queue, bind groups, atlas textures, and render pipelines.
pub struct WgpuRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    camera_buf: wgpu::Buffer,
    camera_bind: wgpu::BindGroup,
    grid_buf: wgpu::Buffer,
    grid_bind: wgpu::BindGroup,
    line_pipeline: wgpu::RenderPipeline,
    line_occluded_pipeline: wgpu::RenderPipeline,
    line_on_top_pipeline: wgpu::RenderPipeline,
    grid_pipeline: wgpu::RenderPipeline,
    grid_occluded_pipeline: wgpu::RenderPipeline,
    marker_on_top_pipeline: wgpu::RenderPipeline,
    marker_pipeline: wgpu::RenderPipeline,
    marker_occluded_pipeline: wgpu::RenderPipeline,
    thing_pipeline: wgpu::RenderPipeline,
    surface_pipeline: wgpu::RenderPipeline,
    sprite_bind_layout: wgpu::BindGroupLayout,
    surface_atlas_layout: wgpu::BindGroupLayout,
    surface_sector_layout: wgpu::BindGroupLayout,
    atlases: Option<Atlases>,
    atlas_generation: Option<u64>,
    /// Per-sector GPU buffers (group 2): brightness + attrs, indexed by `sector`.
    /// Rebuilt when sector count changes; otherwise patched in place.
    sector_bufs: Option<SectorBuffers>,
    sector_count: usize,
    /// Persistent whole-map geometry; reused across pan/zoom, rebuilt on map edit.
    geometry: Option<CachedGeometry>,
    /// Transient overlay (BSP anim / edit preview). Rebuilt per step/drag.
    overlay_lines: Option<(wgpu::Buffer, u32)>,
    overlay_markers: Option<(wgpu::Buffer, u32)>,
    clear: Cell<wgpu::Color>,
    sel_colour: Cell<[f32; 4]>,
    /// 0 None, 1 Colour, 2 Texture — folded into the camera uniform each paint.
    fill_mode: Cell<f32>,
    /// True while an edit gesture is active: grid overdraws geometry.
    grid_on_top: Cell<bool>,
    /// Spacebar toggle: hides grid + lines + verts (wireframe keeps lines).
    overlays_visible: Cell<bool>,
    grid_style: Cell<GridStyle>,
    grid_z: Cell<f32>,
    /// Reused depth texture; recreated only on canvas resize.
    depth: RefCell<Option<(u32, u32, wgpu::Texture)>>,
}

/// Uploaded atlas textures + bind groups.
struct Atlases {
    sprite_bind: wgpu::BindGroup,
    /// flat + wall atlas (group 1).
    surface_bind: wgpu::BindGroup,
}

/// Per-sector GPU buffers (group 2) + bind group: brightness + attrs + 3D attrs.
/// Rebuilt when sector count changes; otherwise patched in place.
struct SectorBuffers {
    brightness: wgpu::Buffer,
    attr: wgpu::Buffer,
    sector3d: wgpu::Buffer,
    surface_bind: wgpu::BindGroup,
}

/// Persistent map geometry: surface3d mesh + instanced layers. `*_len` = instance count
/// (vertex count for surface3d). Reused across pan/zoom; patched per element on non-topology edits.
struct CachedGeometry {
    lines: wgpu::Buffer,
    lines_len: u32,
    normals: wgpu::Buffer,
    normals_len: u32,
    wire: wgpu::Buffer,
    wire_len: u32,
    verts: wgpu::Buffer,
    verts_len: u32,
    things: wgpu::Buffer,
    things_len: u32,
    surface3d: wgpu::Buffer,
    surface3d_len: u32,
}

impl WgpuRenderer {
    fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        let line_shader = shader_module(&device, "editor-line-shader", LINE_WGSL);
        let thing_shader = shader_module(&device, "editor-thing-shader", THING_WGSL);
        let surface_shader = shader_module(&device, "editor-surface-shader", SURFACE_WGSL);
        let camera_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("editor-map-camera"),
            size: size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("editor-map-camera-layout"),
            entries: &[uniform_entry(
                0,
                wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
            )],
        });
        let camera_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("editor-map-camera-bind"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buf.as_entire_binding(),
            }],
        });

        let grid_shader = shader_module(&device, "editor-grid-shader", GRID_WGSL);
        let grid_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("editor-grid-uniform"),
            size: size_of::<GridUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let grid_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("editor-grid-layout"),
            entries: &[uniform_entry(0, wgpu::ShaderStages::FRAGMENT)],
        });
        let grid_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("editor-grid-bind"),
            layout: &grid_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: grid_buf.as_entire_binding(),
            }],
        });
        let grid_pl_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("editor-grid-pl-layout"),
            bind_group_layouts: &[Some(&grid_layout)],
            immediate_size: 0,
        });

        let sprite_bind_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("editor-sprite-layout"),
                entries: &[
                    float_tex_entry(0),
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                        count: None,
                    },
                ],
            });
        let surface_atlas_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("editor-surface-atlas-layout"),
                entries: &[float_tex_entry(0), float_tex_entry(1)],
            });
        let surface_sector_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("editor-surface-sector-layout"),
                entries: &[
                    sector_storage_entry(0),
                    sector_storage_entry(1),
                    sector_storage_entry(2),
                ],
            });

        let solid_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("editor-solid-layout"),
            bind_group_layouts: &[Some(&camera_layout)],
            immediate_size: 0,
        });
        let thing_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("editor-thing-pl-layout"),
            bind_group_layouts: &[Some(&camera_layout), Some(&sprite_bind_layout)],
            immediate_size: 0,
        });
        let surface_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("editor-surface-pl-layout"),
            bind_group_layouts: &[
                Some(&camera_layout),
                Some(&surface_atlas_layout),
                Some(&surface_sector_layout),
            ],
            immediate_size: 0,
        });

        let line_pipeline = make_pipeline(
            &device,
            &line_shader,
            &solid_layout,
            "vs_line",
            "fs_solid",
            &LINE_VBUF,
            wgpu::VertexStepMode::Instance,
            DepthMode::Overlay,
            "editor-line",
        );
        let line_occluded_pipeline = make_pipeline(
            &device,
            &line_shader,
            &solid_layout,
            "vs_line",
            "fs_solid_dim",
            &LINE_VBUF,
            wgpu::VertexStepMode::Instance,
            DepthMode::Occluded,
            "editor-line-occluded",
        );
        let line_on_top_pipeline = make_pipeline(
            &device,
            &line_shader,
            &solid_layout,
            "vs_line",
            "fs_solid",
            &LINE_VBUF,
            wgpu::VertexStepMode::Instance,
            DepthMode::OnTop,
            "editor-line-on-top",
        );
        let grid_pipeline = make_fullscreen_pipeline(
            &device,
            &grid_shader,
            &grid_pl_layout,
            DepthMode::OnTop,
            "editor-grid",
        );
        let grid_occluded_pipeline = make_fullscreen_pipeline(
            &device,
            &grid_shader,
            &grid_pl_layout,
            DepthMode::Overlay,
            "editor-grid-occluded",
        );
        let marker_pipeline = make_pipeline(
            &device,
            &line_shader,
            &solid_layout,
            "vs_marker",
            "fs_solid",
            &MARKER_VBUF,
            wgpu::VertexStepMode::Instance,
            DepthMode::Overlay,
            "editor-marker",
        );
        let marker_occluded_pipeline = make_pipeline(
            &device,
            &line_shader,
            &solid_layout,
            "vs_marker",
            "fs_solid_dim",
            &MARKER_VBUF,
            wgpu::VertexStepMode::Instance,
            DepthMode::Occluded,
            "editor-marker-occluded",
        );
        let marker_on_top_pipeline = make_pipeline(
            &device,
            &line_shader,
            &solid_layout,
            "vs_marker",
            "fs_solid",
            &MARKER_VBUF,
            wgpu::VertexStepMode::Instance,
            DepthMode::OnTop,
            "editor-marker-on-top",
        );
        let thing_pipeline = make_pipeline(
            &device,
            &thing_shader,
            &thing_layout,
            "vs_thing",
            "fs_thing",
            &THING_VBUF,
            wgpu::VertexStepMode::Instance,
            DepthMode::Opaque,
            "editor-thing",
        );
        let surface_pipeline = make_pipeline(
            &device,
            &surface_shader,
            &surface_layout,
            "vs_surface3d",
            "fs_surface3d",
            &SURFACE_VBUF,
            wgpu::VertexStepMode::Vertex,
            DepthMode::Surface,
            "editor-surface3d",
        );

        Self {
            device,
            queue,
            camera_buf,
            camera_bind,
            grid_buf,
            grid_bind,
            line_pipeline,
            line_occluded_pipeline,
            line_on_top_pipeline,
            grid_pipeline,
            grid_occluded_pipeline,
            marker_on_top_pipeline,
            marker_pipeline,
            marker_occluded_pipeline,
            thing_pipeline,
            surface_pipeline,
            sprite_bind_layout,
            surface_atlas_layout,
            surface_sector_layout,
            atlases: None,
            atlas_generation: None,
            sector_bufs: None,
            sector_count: 0,
            geometry: None,
            overlay_lines: None,
            overlay_markers: None,
            clear: Cell::new(CLEAR),
            sel_colour: Cell::new([0.0; 4]),
            fill_mode: Cell::new(FILL_TEXTURE),
            grid_on_top: Cell::new(false),
            overlays_visible: Cell::new(true),
            grid_style: Cell::new(GridStyle::default()),
            grid_z: Cell::new(0.0),
            depth: RefCell::new(None),
        }
    }

    /// Build per-sector GPU buffers for `brightness.len()` sectors and upload.
    /// Rebuilds the bind group; light tics + property edits patch in place.
    pub(crate) fn set_sector_data(
        &mut self,
        brightness: &[f32],
        attrs: &[SectorAttr],
        sector3d: &[Sector3D],
    ) {
        let count = brightness.len().max(attrs.len()).max(sector3d.len()).max(1);
        let brightness_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("editor-sector-brightness"),
            size: (count * size_of::<f32>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let attr_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("editor-sector-attr"),
            size: (count * size_of::<SectorAttr>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sector3d_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("editor-sector-3d"),
            size: (count * size_of::<Sector3D>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let surface_bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("editor-surface-sector-bind"),
            layout: &self.surface_sector_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: brightness_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: attr_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: sector3d_buf.as_entire_binding(),
                },
            ],
        });
        if !brightness.is_empty() {
            self.queue
                .write_buffer(&brightness_buf, 0, bytemuck::cast_slice(brightness));
        }
        if !attrs.is_empty() {
            self.queue
                .write_buffer(&attr_buf, 0, bytemuck::cast_slice(attrs));
        }
        if !sector3d.is_empty() {
            self.queue
                .write_buffer(&sector3d_buf, 0, bytemuck::cast_slice(sector3d));
        }
        self.sector_bufs = Some(SectorBuffers {
            brightness: brightness_buf,
            attr: attr_buf,
            sector3d: sector3d_buf,
            surface_bind,
        });
        self.sector_count = count;
    }

    /// Rewrite the brightness buffer (light tic). Requires `set_sector_data` first.
    pub(crate) fn update_brightness(&self, brightness: &[f32]) {
        if let Some(s) = &self.sector_bufs
            && !brightness.is_empty()
        {
            self.queue
                .write_buffer(&s.brightness, 0, bytemuck::cast_slice(brightness));
        }
    }

    /// Upload atlas textures; skips when generation is unchanged.
    pub(crate) fn set_atlases(&mut self, atlas: &AtlasData) {
        if self.atlas_generation == Some(atlas.generation) && self.atlases.is_some() {
            return;
        }
        let flat_tex = self.upload_rgba(
            &atlas.flat_rgba,
            atlas.flat_atlas_w.max(MIN_DIM),
            atlas.flat_atlas_h.max(MIN_DIM),
            "editor-flat-atlas",
        );
        let wall_tex = self.upload_rgba(
            &atlas.wall_rgba,
            atlas.wall_atlas_w.max(MIN_DIM),
            atlas.wall_atlas_h.max(MIN_DIM),
            "editor-wall-atlas",
        );
        let sprite_tex = self.upload_rgba(
            &atlas.sprite_rgba,
            atlas.sprite_w.max(MIN_DIM),
            atlas.sprite_h.max(MIN_DIM),
            "editor-sprite-atlas",
        );
        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("editor-sprite-sampler"),
            ..Default::default()
        });

        let flat_view = flat_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let wall_view = wall_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let surface_bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("editor-surface-atlas-bind"),
            layout: &self.surface_atlas_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&flat_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&wall_view),
                },
            ],
        });
        let sprite_view = sprite_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let sprite_bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("editor-sprite-bind"),
            layout: &self.sprite_bind_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&sprite_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        self.atlases = Some(Atlases {
            sprite_bind,
            surface_bind,
        });
        self.atlas_generation = Some(atlas.generation);
    }

    fn upload_rgba(&self, data: &[u8], w: u32, h: u32, label: &str) -> wgpu::Texture {
        let fallback = [0u8, 0, 0, 0];
        let (data, w, h) = if data.is_empty() {
            (&fallback[..], 1, 1)
        } else {
            (data, w, h)
        };
        self.upload_tex(data, w, h, 4, wgpu::TextureFormat::Rgba8Unorm, label)
    }

    fn upload_tex(
        &self,
        data: &[u8],
        w: u32,
        h: u32,
        bytes_per_texel: u32,
        format: wgpu::TextureFormat,
        label: &str,
    ) -> wgpu::Texture {
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * bytes_per_texel),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        tex
    }

    /// Upload the whole-map mesh, replacing the cache. Instanced buffers are `COPY_DST` for patching.
    fn upload_map(&mut self, frame: &MapFrame) {
        self.geometry = Some(CachedGeometry {
            lines: self.vbuf_patchable(&frame.lines, "editor-lines-vb"),
            lines_len: frame.lines.len() as u32,
            normals: self.vbuf_patchable(&frame.normals, "editor-normals-vb"),
            normals_len: frame.normals.len() as u32,
            wire: self.vbuf(&frame.wire, "editor-wire-vb"),
            wire_len: frame.wire.len() as u32,
            verts: self.vbuf_patchable(&frame.verts, "editor-verts-vb"),
            verts_len: frame.verts.len() as u32,
            things: self.vbuf_patchable(&frame.things, "editor-things-vb"),
            things_len: frame.things.len() as u32,
            surface3d: self.vbuf(&frame.surface3d, "editor-surface3d-vb"),
            surface3d_len: frame.surface3d.len() as u32,
        });
    }

    /// Drop the cached mesh; prevents the previous map flashing on reset.
    fn clear_map(&mut self) {
        self.geometry = None;
    }

    fn set_grid_style(&self, style: GridStyle) {
        self.grid_style.set(style);
    }

    fn set_grid_z(&self, z: f32) {
        self.grid_z.set(z);
    }

    fn set_overlay(&mut self, lines: &[LineInst], markers: &[MarkerInst]) {
        self.overlay_lines = (!lines.is_empty()).then(|| {
            (
                self.vbuf(lines, "editor-overlay-lines-vb"),
                lines.len() as u32,
            )
        });
        self.overlay_markers = (!markers.is_empty()).then(|| {
            (
                self.vbuf(markers, "editor-overlay-markers-vb"),
                markers.len() as u32,
            )
        });
    }

    /// Patch one line instance in place. Out-of-range ids are ignored.
    fn patch_line(&self, i: u32, inst: LineInst, normal: LineInst) {
        if let Some(g) = &self.geometry
            && i < g.lines_len
        {
            let off = i as u64 * size_of::<LineInst>() as u64;
            self.queue
                .write_buffer(&g.lines, off, bytemuck::bytes_of(&inst));
            self.queue
                .write_buffer(&g.normals, off, bytemuck::bytes_of(&normal));
        }
    }

    fn patch_vert(&self, i: u32, inst: MarkerInst) {
        if let Some(g) = &self.geometry
            && i < g.verts_len
        {
            let off = i as u64 * size_of::<MarkerInst>() as u64;
            self.queue
                .write_buffer(&g.verts, off, bytemuck::bytes_of(&inst));
        }
    }

    fn patch_thing(&self, i: u32, inst: ThingInst) {
        if let Some(g) = &self.geometry
            && i < g.things_len
        {
            let off = i as u64 * size_of::<ThingInst>() as u64;
            self.queue
                .write_buffer(&g.things, off, bytemuck::bytes_of(&inst));
        }
    }

    fn patch_sector_attr(&self, sector: u32, attr: SectorAttr) {
        if let Some(s) = &self.sector_bufs
            && (sector as usize) < self.sector_count
        {
            let off = sector as u64 * size_of::<SectorAttr>() as u64;
            self.queue
                .write_buffer(&s.attr, off, bytemuck::bytes_of(&attr));
        }
    }

    fn patch_sector_3d(&self, sector: u32, sector3d: Sector3D) {
        if let Some(s) = &self.sector_bufs
            && (sector as usize) < self.sector_count
        {
            let off = sector as u64 * size_of::<Sector3D>() as u64;
            self.queue
                .write_buffer(&s.sector3d, off, bytemuck::bytes_of(&sector3d));
        }
    }

    fn write_camera(&self, camera: Camera, viewport: [f32; 2]) {
        let r = camera.billboard_right();
        let tilted = if camera.is_tilted() { 1.0 } else { 0.0 };
        let aspect = viewport[0] / viewport[1].max(1.0);
        self.queue.write_buffer(
            &self.camera_buf,
            0,
            bytemuck::bytes_of(&CameraUniform {
                view_proj: camera.view_proj(aspect),
                viewport,
                _pad: [0.0, 0.0],
                cam_right: [r[0], r[1], r[2], tilted],
                sel_colour: self.sel_colour.get(),
                params: [self.fill_mode.get(), 0.0, 0.0, 0.0],
            }),
        );
    }

    /// Write the grid uniform (procedural — no geometry upload).
    fn write_grid(&self, camera: Camera, viewport: [f32; 2]) {
        let aspect = viewport[0] / viewport[1].max(1.0);
        let s = self.grid_style.get();
        let on_top = if self.grid_on_top.get() { 1.0 } else { 0.0 };
        self.queue.write_buffer(
            &self.grid_buf,
            0,
            bytemuck::bytes_of(&GridUniform {
                inv_view_proj: camera.inv_view_proj(aspect),
                view_proj: camera.view_proj(aspect),
                plane: [viewport[0], viewport[1], self.grid_z.get(), on_top],
                params: [s.snap, s.tile, s.min_px, s.half_px],
                grid_rgba: s.grid_rgba,
                tile_rgba: s.tile_rgba,
                fade: [
                    GRID_GRAZE_FADE_START,
                    GRID_GRAZE_FADE_FULL,
                    GRID_FAR_FADE_START,
                    GRID_FAR_FADE_END,
                ],
                eye: {
                    let e = camera.eye();
                    [e[0], e[1], e[2], 0.0]
                },
            }),
        );
    }

    /// Depth-attachment view; reuses the cached texture when size is unchanged.
    fn depth_view(&self, width: u32, height: u32) -> wgpu::TextureView {
        let mut cache = self.depth.borrow_mut();
        let stale = !matches!(*cache, Some((w, h, _)) if w == width && h == height);
        if stale {
            let tex = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("editor-map-depth"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            *cache = Some((width, height, tex));
        }
        let (_, _, tex) = cache.as_ref().expect("just populated");
        tex.create_view(&wgpu::TextureViewDescriptor::default())
    }

    /// Encode the cached geometry into a fresh texture. `extra_usage` adds `COPY_SRC` for readback.
    fn encode_to_texture(
        &self,
        width: u32,
        height: u32,
        camera: Camera,
        extra_usage: wgpu::TextureUsages,
        clear: wgpu::Color,
    ) -> wgpu::Texture {
        self.write_camera(camera, [width as f32, height as f32]);
        self.write_grid(camera, [width as f32, height as f32]);
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("editor-map-wgpu"),
            size: wgpu::Extent3d {
                width: width.max(MIN_DIM),
                height: height.max(MIN_DIM),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | extra_usage,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let depth_view = self.depth_view(width.max(MIN_DIM), height.max(MIN_DIM));
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("editor-map-wgpu-encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("editor-map-wgpu-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.camera_bind, &[]);
            if let Some(geo) = &self.geometry {
                if geo.surface3d_len > 0
                    && let (Some(atlases), Some(sectors)) = (&self.atlases, &self.sector_bufs)
                {
                    pass.set_pipeline(&self.surface_pipeline);
                    pass.set_bind_group(1, &atlases.surface_bind, &[]);
                    pass.set_bind_group(2, &sectors.surface_bind, &[]);
                    pass.set_vertex_buffer(0, geo.surface3d.slice(..));
                    pass.draw(0..geo.surface3d_len, 0..1);
                }
                if geo.things_len > 0
                    && let Some(atlases) = &self.atlases
                {
                    pass.set_pipeline(&self.thing_pipeline);
                    pass.set_bind_group(1, &atlases.sprite_bind, &[]);
                    pass.set_vertex_buffer(0, geo.things.slice(..));
                    pass.draw(0..6, 0..geo.things_len);
                }
                // In wireframe, lines are the geometry; overlay toggle hides only the grid.
                let overlays = self.overlays_visible.get();
                let wireframe = self.fill_mode.get() == FILL_NONE;
                if overlays {
                    // Full-screen triangle uses group 0 = grid uniform; restore camera bind after.
                    let grid_pipeline = if self.grid_on_top.get() {
                        &self.grid_pipeline
                    } else {
                        &self.grid_occluded_pipeline
                    };
                    pass.set_pipeline(grid_pipeline);
                    pass.set_bind_group(0, &self.grid_bind, &[]);
                    pass.draw(0..3, 0..1);
                    pass.set_bind_group(0, &self.camera_bind, &[]);
                }
                if overlays || wireframe {
                    draw_instances(&mut pass, &self.line_pipeline, &geo.lines, geo.lines_len);
                    draw_instances(
                        &mut pass,
                        &self.line_occluded_pipeline,
                        &geo.lines,
                        geo.lines_len,
                    );
                    draw_instances(
                        &mut pass,
                        &self.line_pipeline,
                        &geo.normals,
                        geo.normals_len,
                    );
                    draw_instances(&mut pass, &self.line_pipeline, &geo.wire, geo.wire_len);
                    draw_instances(&mut pass, &self.marker_pipeline, &geo.verts, geo.verts_len);
                    draw_instances(
                        &mut pass,
                        &self.marker_occluded_pipeline,
                        &geo.verts,
                        geo.verts_len,
                    );
                }
                if let Some((buf, len)) = &self.overlay_lines {
                    draw_instances(&mut pass, &self.line_on_top_pipeline, buf, *len);
                }
                if let Some((buf, len)) = &self.overlay_markers {
                    draw_instances(&mut pass, &self.marker_on_top_pipeline, buf, *len);
                }
            }
        }
        self.queue.submit([encoder.finish()]);
        texture
    }

    fn paint(&self, camera: Camera, width: u32, height: u32) -> Image {
        let texture = self.encode_to_texture(
            width,
            height,
            camera,
            wgpu::TextureUsages::empty(),
            self.clear.get(),
        );
        Image::try_from(texture).expect("texture meets Slint's import contract")
    }

    fn set_clear(&self, rgba: [u8; 4]) {
        self.clear.set(wgpu::Color {
            r: rgba[0] as f64 / 255.0,
            g: rgba[1] as f64 / 255.0,
            b: rgba[2] as f64 / 255.0,
            a: rgba[3] as f64 / 255.0,
        });
    }

    fn set_sel_colour(&self, rgba: [u8; 4]) {
        self.sel_colour.set([
            rgba[0] as f32 / 255.0,
            rgba[1] as f32 / 255.0,
            rgba[2] as f32 / 255.0,
            rgba[3] as f32 / 255.0,
        ]);
    }

    fn set_fill_mode(&self, mode: f32) {
        self.fill_mode.set(mode);
    }

    fn render_canvas_rgba(&self, camera: Camera, width: u32, height: u32) -> Vec<u8> {
        let w = width.max(MIN_DIM);
        let h = height.max(MIN_DIM);
        self.grid_on_top.set(false);
        let texture = self.encode_to_texture(
            w,
            h,
            camera,
            wgpu::TextureUsages::COPY_SRC,
            self.clear.get(),
        );
        read_texture_rgba(&self.device, &self.queue, &texture, w, h)
    }

    #[cfg(test)]
    pub fn render_frame_rgba(
        &mut self,
        frame: &MapFrame,
        grid_style: GridStyle,
        camera: Camera,
        width: u32,
        height: u32,
    ) -> Vec<u8> {
        self.upload_map(frame);
        self.set_grid_style(grid_style);
        self.render_canvas_rgba(camera, width, height)
    }

    fn vbuf<T: Pod>(&self, verts: &[T], label: &str) -> wgpu::Buffer {
        self.make_vbuf(verts, label, wgpu::BufferUsages::VERTEX)
    }

    /// Vertex buffer with `COPY_DST` for in-place patching.
    fn vbuf_patchable<T: Pod>(&self, verts: &[T], label: &str) -> wgpu::Buffer {
        self.make_vbuf(
            verts,
            label,
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        )
    }

    fn make_vbuf<T: Pod>(
        &self,
        verts: &[T],
        label: &str,
        usage: wgpu::BufferUsages,
    ) -> wgpu::Buffer {
        // Empty buffer is invalid to bind; pad to one zeroed element (never drawn — len check guards it).
        let zero = [T::zeroed()];
        let bytes: &[u8] = if verts.is_empty() {
            bytemuck::cast_slice(&zero)
        } else {
            bytemuck::cast_slice(verts)
        };
        self.device.create_buffer_init(&BufferInitDescriptor {
            label: Some(label),
            contents: bytes,
            usage,
        })
    }
}

/// Draw `count` instances from `vb`. Group 0 = camera, already bound.
fn draw_instances(
    pass: &mut wgpu::RenderPass<'_>,
    pipeline: &wgpu::RenderPipeline,
    vb: &wgpu::Buffer,
    count: u32,
) {
    if count == 0 {
        return;
    }
    pass.set_pipeline(pipeline);
    pass.set_vertex_buffer(0, vb.slice(..));
    pass.draw(0..6, 0..count);
}

/// Read back a `Rgba8Unorm` texture (needs `COPY_SRC`) to row-tight RGBA bytes via a blocking poll.
fn read_texture_rgba(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    w: u32,
    h: u32,
) -> Vec<u8> {
    let unpadded = w * 4;
    let padded = unpadded.div_ceil(COPY_BYTES_PER_ROW_ALIGN) * COPY_BYTES_PER_ROW_ALIGN;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("editor-readback"),
        size: (padded * h) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("editor-readback-encoder"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(h),
            },
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);

    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |r| {
        if let Err(e) = r {
            log::error!("readback buffer map failed: {e}");
        }
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("poll readback");
    let mapped = slice.get_mapped_range();
    let mut out = Vec::with_capacity((unpadded * h) as usize);
    for row in 0..h {
        let start = (row * padded) as usize;
        out.extend_from_slice(&mapped[start..start + unpadded as usize]);
    }
    drop(mapped);
    buffer.unmap();
    out
}

fn uniform_entry(binding: u32, visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

/// Read-only storage-buffer binding in the fragment shader.
fn sector_storage_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage {
                read_only: true,
            },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn float_tex_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float {
                filterable: false,
            },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn shader_module(device: &wgpu::Device, label: &str, src: &str) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(src.into()),
    })
}

fn make_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    vs: &str,
    fs: &str,
    attrs: &'static [wgpu::VertexAttribute],
    step_mode: wgpu::VertexStepMode,
    depth: DepthMode,
    label: &str,
) -> wgpu::RenderPipeline {
    let stride: u64 = attrs.iter().map(|a| vertex_format_size(a.format)).sum();
    let vbuf_layout = wgpu::VertexBufferLayout {
        array_stride: stride,
        step_mode,
        attributes: attrs,
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(vs),
            buffers: slice::from_ref(&vbuf_layout),
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fs),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8Unorm,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: depth.cull_mode(),
            ..Default::default()
        },
        depth_stencil: Some(depth.state()),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

/// Pipeline with no vertex buffer; shader synthesises a full-screen triangle from `vertex_index`.
fn make_fullscreen_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    depth: DepthMode,
    label: &str,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_grid"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_grid"),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8Unorm,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(depth.state()),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

fn vertex_format_size(f: wgpu::VertexFormat) -> u64 {
    match f {
        wgpu::VertexFormat::Float32 | wgpu::VertexFormat::Uint32 => 4,
        wgpu::VertexFormat::Float32x2 | wgpu::VertexFormat::Uint32x2 => 8,
        wgpu::VertexFormat::Float32x3 => 12,
        wgpu::VertexFormat::Float32x4 => 16,
        _ => unreachable!("unsupported vertex format"),
    }
}

/// Install a rendering notifier that captures the wgpu device/queue and runs `on_ready` once.
pub fn capture(window: &slint::Window, on_ready: impl Fn() + 'static) -> WgpuContext {
    let ctx = WgpuContext::default();
    let captured = ctx.clone();
    let result = window.set_rendering_notifier(move |state, api| {
        if !matches!(state, slint::RenderingState::RenderingSetup) {
            return;
        }
        if let slint::GraphicsAPI::WGPU29 {
            device,
            queue,
            ..
        } = api
        {
            captured.set(WgpuRenderer::new(device.clone(), queue.clone()));
            on_ready();
        }
    });
    if let Err(e) = result {
        log::warn!("wgpu rendering notifier not installed: {e:?}");
    }
    ctx
}

/// Headless wgpu renderer for tests. `None` when no adapter is available (CI).
#[cfg(test)]
pub(crate) fn headless_renderer() -> Option<WgpuRenderer> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::default(),
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .ok()?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("editor-headless"),
        ..Default::default()
    }))
    .ok()?;
    Some(WgpuRenderer::new(device, queue))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::EditorAssets;
    use crate::render::atlas;
    use crate::render::editor_camera::CameraMode;
    use crate::render::export_camera;
    use crate::render::frame::{self, FrameInput};
    use crate::render::input::{SectorFill, Selection};
    use crate::render::sprites::ThingSpriteCache;
    use crate::render::style::CanvasStyle;
    use crate::render::triangulate::build_sector_tris;
    use std::collections::HashMap;

    fn px(buf: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * w + x) * 4) as usize;
        [buf[at], buf[at + 1], buf[at + 2], buf[at + 3]]
    }

    #[test]
    fn surface3d_renders_with_depth() {
        let Some(mut r) = headless_renderer() else {
            eprintln!("no wgpu adapter; skipping GPU render test");
            return;
        };
        let wad = wad::WadData::new(&test_utils::doom1_wad_path());
        let map = editor_core::import_wad_map(&wad, "E1M1").expect("E1M1 imports");
        let mut assets = EditorAssets::load(&[test_utils::doom1_wad_path()], &wad, None);
        assets.set_map_wad("doom1.wad");
        let names = atlas::collect_wall_names(&assets, &map);
        assets.ensure_composed(&names, &wad);
        let tris = build_sector_tris(&map);
        let sprites = ThingSpriteCache::default();
        let (data, maps) = atlas::build(&assets, &map, &sprites, 1);
        r.set_atlases(&data);

        let (w, h) = (640u32, 480u32);
        let zoom = 0.15f32;
        let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
        for v in &map.vertices {
            lo[0] = lo[0].min(v.x);
            lo[1] = lo[1].min(v.y);
            hi[0] = hi[0].max(v.x);
            hi[1] = hi[1].max(v.y);
        }
        let centre = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5];
        let camera = export_camera(centre, zoom, w as f32, h as f32);

        let style = CanvasStyle::default();
        let sel = Selection::default();
        let extents = HashMap::new();
        let colors = HashMap::new();
        let input = FrameInput {
            map: &map,
            tris: &tris,
            zoom,
            pixel_ratio: 1.0,
            style: &style,
            selection: &sel,
            grid: 64,
            fill: SectorFill::Texture,
            selected_sectors: &[],
            thing_visible: &|_| true,
            thing_extents: &extents,
            thing_colors: &colors,
            atlas: &maps,
            thing_radius: &|_| 20.0,
            sector_gradient: colorous::PLASMA,
            highlight_unenclosed: false,
            mode: CameraMode::Ortho3d,
            grid_z: 0.0,
            vert_z: &[],
        };
        let f = frame::build_map_geometry(&input);
        let grid = frame::grid_style(&input);
        assert!(!f.surface3d.is_empty(), "the surface mesh is always built");

        let brightness: Vec<f32> = map
            .sectors
            .iter()
            .map(|s| s.light_level.clamp(0, 255) as f32 / 255.0)
            .collect();
        r.set_sector_data(&brightness, &f.sector_attrs, &f.sector3d);
        let buf = r.render_frame_rgba(&f, grid, camera, w, h);

        let non_bg = (0..w * h)
            .filter(|i| {
                let p = px(&buf, w, i % w, i / w);
                p[0] < 250 || p[1] < 250 || p[2] < 250
            })
            .count();
        assert!(
            non_bg > (w * h / 20) as usize,
            "3D surfaces cover the canvas"
        );
    }

    /// Grid shader paints lines over an empty canvas; regression for the "I see no grid" bug.
    #[test]
    fn grid_shader_paints_lines_top_down() {
        let Some(mut r) = headless_renderer() else {
            eprintln!("no wgpu adapter; skipping GPU render test");
            return;
        };
        let (w, h) = (256u32, 256u32);
        let camera = export_camera([0.0, 0.0], 1.0, w as f32, h as f32);
        let style = GridStyle {
            snap: 64.0,
            tile: 64.0,
            grid_rgba: [1.0, 0.0, 0.0, 1.0],
            tile_rgba: [1.0, 0.0, 0.0, 1.0],
            min_px: 4.0,
            half_px: 1.0,
        };
        r.set_clear([0, 0, 0, 255]);
        let buf = r.render_frame_rgba(&MapFrame::default(), style, camera, w, h);

        let red = (0..w * h)
            .filter(|i| {
                let p = px(&buf, w, i % w, i / w);
                p[0] > 150 && p[1] < 100 && p[2] < 100
            })
            .count();
        assert!(
            red > 100,
            "grid lines paint over the empty canvas (got {red})"
        );
        assert!(
            red < (w * h / 2) as usize,
            "grid is lines, not a fill (got {red})"
        );
    }
}
