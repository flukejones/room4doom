//! CPU geometry stage: builds a [`MapFrame`] (vertex buffers) from the map + view state. Geometry is in map world units (Y-up); lines, markers, and ticks carry a device-pixel size, the VS expands them to constant screen size so zoom is camera-only.

use std::collections::HashMap;

use bytemuck::Zeroable as _;
use colorous::Gradient;
use editor_core::{
    ArenaKey as _, EditorMap, LineKey, Name8, Sector, SectorKey, Thing, ThingKey, VertKey,
};

use crate::level_editor::transform::TransformHandles;
use crate::render::atlas::AtlasMaps;
use crate::render::editor_camera::CameraMode;
use crate::render::frame3d::{SurfaceSlots, build_surface, wall_bands};
use crate::render::input::{Overlay, SectorFill, SelItem, Selection};
use crate::render::sprites::ThingSpriteCache;
use crate::render::style::{
    CanvasStyle, Color, MIN_GRID_SPACING_PX, NORMAL_TICK_PX, TILE_GRID, VERTEX_DRAW_PX,
};
use crate::render::triangulate::SectorTris;
use crate::render::wgpu::{
    GridStyle, LineInst, MapFrame, MarkerInst, Sector3D, SectorAttr, ThingInst,
};
use crate::render::{FNV_OFFSET, fnv_fold};

const LINE_THICKNESS_PX: f32 = 2.0;
/// Transform handle knob size (device px).
const HANDLE_DRAW_PX: f32 = 9.0;
const SELECTED_THICKNESS_PX: f32 = 4.0;
const SELECTED_VERTEX_BONUS_PX: f32 = 4.0;
pub(crate) const SELECTED_SECTOR_ALPHA: u8 = 64;
/// Grid alpha over fills (blended so geometry shows through).
const GRID_BLEND_ALPHA: u8 = 72;
const ALIGN_GRID_BLEND_ALPHA: u8 = 110;
/// World units per sprite pixel; calibrated so the smallest pickup fits its ring.
const SPRITE_WORLD_PER_PX: f32 = 1.25;
const FALLBACK_THING_HALF_EXTENT: f32 = 20.0;

/// Flat-tile sentinels: `NONE` = no flat (discard); `MISSING` = unresolved (magenta), injected into the shader.
const FLAT_TILE_NONE: [f32; 2] = [-1.0, -1.0];
pub(crate) const FLAT_TILE_MISSING: [f32; 2] = [-2.0, -2.0];

/// World [half-width, half-height] a thing draws at (sprite-pixel scaled, aspect kept); pick and render share it. Sprite-less kinds use a `radius` square.
pub(crate) fn thing_world_half_extent(
    sprites: Option<&ThingSpriteCache>,
    kind: i32,
    radius: f32,
) -> [f32; 2] {
    if let Some(sprite) = sprites.and_then(|s| s.get(kind))
        && sprite.width > 0
        && sprite.height > 0
    {
        let half = SPRITE_WORLD_PER_PX * 0.5;
        [sprite.width as f32 * half, sprite.height as f32 * half]
    } else {
        [radius, radius]
    }
}

/// All inputs the frame builder reads for one canvas frame.
pub struct FrameInput<'a> {
    pub map: &'a EditorMap,
    pub tris: &'a SectorTris,
    /// Pixels per world unit (for normal tick length).
    pub zoom: f32,
    /// Device/logical pixel ratio (HiDPI). Multiplied into screen-px sizes.
    pub pixel_ratio: f32,
    pub style: &'a CanvasStyle,
    pub selection: &'a Selection,
    pub grid: i32,
    pub fill: SectorFill,
    pub selected_sectors: &'a [SectorKey],
    pub thing_visible: &'a dyn Fn(&Thing) -> bool,
    /// Per thing kind; shared with pick.
    pub thing_extents: &'a HashMap<i32, [f32; 2]>,
    pub thing_colors: &'a HashMap<i32, Color>,
    pub atlas: &'a AtlasMaps,
    pub thing_radius: &'a dyn Fn(i32) -> f32,
    pub sector_gradient: Gradient,
    pub highlight_unenclosed: bool,
    pub mode: CameraMode,
    pub grid_z: f32,
    /// Per-vertex floor Z; read only in wireframe fill, other modes ride `grid_z`.
    pub vert_z: &'a [f32],
}

/// A thing icon's normalised UV rect within the sprite atlas.
#[derive(Clone, Copy)]
pub struct SpriteSlot {
    pub u0: f32,
    pub v0: f32,
    pub u1: f32,
    pub v1: f32,
}

/// Whole-map geometry + span table: uploaded once on load, patched per slot/span thereafter.
pub fn build_map_geometry(input: &FrameInput<'_>) -> (MapFrame, SurfaceSlots) {
    let mut frame = MapFrame {
        sector_attrs: build_sector_attrs(input),
        sector3d: build_sector_3d(input),
        ..Default::default()
    };
    let (mesh, slots) = build_surface(input.map, input.tris, &input.atlas.wall_rects);
    frame.surface3d = mesh;
    draw_lines(input, &mut frame);
    if input.fill == SectorFill::None {
        frame.wire = build_wire(input);
    }
    draw_vertices(input, &mut frame);
    draw_things(input, &mut frame);
    (frame, slots)
}

/// None-mode wireframe extras: band outlines + merged posts; rebuilt wholesale (posts span lines).
pub fn build_wire(input: &FrameInput<'_>) -> Vec<LineInst> {
    let map = input.map;
    let rects = &input.atlas.wall_rects;
    let mut wire = Vec::new();

    let mut posts: HashMap<VertKey, Vec<(f32, f32)>> = HashMap::new();
    let mut colour: HashMap<VertKey, ([f32; 4], bool)> = HashMap::new();
    let default_colour = rgba_f32(input.style.one_sided);
    let selected = rgba_f32(input.style.selected);

    for (key, line) in map.lines.iter() {
        let (seg, _) = line_instances(input, key);
        let edge_colour = seg.rgba;
        let edge_selected = edge_colour == selected;
        for band in wall_bands(map, line, rects) {
            wire.push(at_height(seg, band.vis_bot));
            wire.push(at_height(seg, band.vis_top));
            for vk in [band.a_vert, band.b_vert] {
                posts
                    .entry(vk)
                    .or_default()
                    .push((band.vis_bot, band.vis_top));
                let c = colour.entry(vk).or_insert((edge_colour, edge_selected));
                if !c.1 {
                    *c = (edge_colour, edge_selected);
                }
            }
        }
    }

    let pen = LINE_THICKNESS_PX * input.pixel_ratio;
    let mut post_keys: Vec<VertKey> = posts.keys().copied().collect();
    post_keys.sort_unstable();
    for vk in post_keys {
        let Some(v) = map.vertices.get(vk) else {
            continue;
        };
        let rgba = colour.get(&vk).map_or(default_colour, |c| c.0);
        let intervals = posts.get_mut(&vk).expect("key from posts");
        for (lo, hi) in merge_intervals(intervals) {
            wire.push(vertical([v.x, v.y], lo, hi, pen, rgba));
        }
    }
    wire
}

/// Sort and union overlapping `(lo, hi)` intervals.
fn merge_intervals(intervals: &mut [(f32, f32)]) -> Vec<(f32, f32)> {
    intervals.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut merged: Vec<(f32, f32)> = Vec::new();
    for &(lo, hi) in intervals.iter() {
        match merged.last_mut() {
            Some(last) if lo <= last.1 => last.1 = last.1.max(hi),
            _ => merged.push((lo, hi)),
        }
    }
    merged
}

fn at_height(mut inst: LineInst, z: f32) -> LineInst {
    inst.az = z;
    inst.bz = z;
    inst
}

fn vertical(p: [f32; 2], z0: f32, z1: f32, pen: f32, rgba: [f32; 4]) -> LineInst {
    LineInst {
        a: p,
        b: p,
        half_px: pen * 0.5,
        az: z0,
        bz: z1,
        rgba,
    }
}

/// Gradient swatch for colour mode and the textured-but-missing fallback.
fn sector_colour(gradient: Gradient, map: &EditorMap, sector: SectorKey) -> Color {
    match map.sectors.get(sector) {
        Some(s) => record_colour(gradient, s),
        None => [0, 0, 0, 0xff],
    }
}

/// FNV hash of the full sector record → gradient position. Stable across renumbering.
fn record_colour(gradient: Gradient, s: &Sector) -> Color {
    let mut seed = FNV_OFFSET;
    let mut mix = |v: u64| seed = fnv_fold(seed, v);
    for byte in s.floor_flat.to_wad_bytes() {
        mix(byte as u64);
    }
    for byte in s.ceil_flat.to_wad_bytes() {
        mix(byte as u64);
    }
    for v in [
        s.floor_height,
        s.ceil_height,
        s.light_level,
        s.special,
        s.tag,
    ] {
        mix(v as u64);
    }
    let c = gradient.eval_continuous(seed as f64 / u64::MAX as f64);
    [c.r, c.g, c.b, 0xff]
}

/// Per-sector flat attrs by sector slot (free slots default). A property edit patches one slot.
pub fn build_sector_attrs(input: &FrameInput<'_>) -> Vec<SectorAttr> {
    let mut out = vec![SectorAttr::zeroed(); input.map.sectors.slot_count()];
    for key in input.map.sectors.keys() {
        out[key.slot() as usize] = sector_attr(input, key);
    }
    out
}

/// Per-sector 3D attrs (heights, flat tiles, selection) by sector slot.
pub fn build_sector_3d(input: &FrameInput<'_>) -> Vec<Sector3D> {
    let mut out = vec![Sector3D::zeroed(); input.map.sectors.slot_count()];
    for key in input.map.sectors.keys() {
        out[key.slot() as usize] = sector_3d(input, key);
    }
    out
}

/// One sector's 3D attrs: resolved flat → atlas tile, empty name → `[-1,-1]` (discarded), missing → `[-2,-2]` (magenta); selected → surface shader tints the floor.
pub fn sector_3d(input: &FrameInput<'_>, sector: SectorKey) -> Sector3D {
    let Some(s) = input.map.sectors.get(sector) else {
        return Sector3D::zeroed();
    };
    let floor_tile = flat_tile(&s.floor_flat, &input.atlas.sector_tile, sector);
    let ceil_tile = flat_tile(&s.ceil_flat, &input.atlas.sector_ceil_tile, sector);
    Sector3D {
        floor_h: s.floor_height as f32,
        ceil_h: s.ceil_height as f32,
        floor_tile,
        ceil_tile,
        selected: if input.selected_sectors.contains(&sector) {
            1.0
        } else {
            0.0
        },
        _pad: 0.0,
    }
}

/// Resolve flat → atlas tile; empty → [`FLAT_TILE_NONE`]; missing → [`FLAT_TILE_MISSING`].
fn flat_tile(name: &Name8, tiles: &HashMap<SectorKey, [f32; 2]>, key: SectorKey) -> [f32; 2] {
    if name.is_empty() {
        return FLAT_TILE_NONE;
    }
    tiles.get(&key).copied().unwrap_or(FLAT_TILE_MISSING)
}

/// One sector's flat attrs: atlas tile + colour-mode fallback tint.
pub(crate) fn sector_attr(input: &FrameInput<'_>, sector: SectorKey) -> SectorAttr {
    let Some(s) = input.map.sectors.get(sector) else {
        return SectorAttr::zeroed();
    };
    let tile = flat_tile(&s.floor_flat, &input.atlas.sector_tile, sector);
    SectorAttr {
        tile,
        _pad: [0.0, 0.0],
        fallback: rgba_f32(sector_colour(input.sector_gradient, input.map, sector)),
    }
}

/// Grid appearance for the procedural GPU grid: spacings, colours (alpha reduced over fills in 3D), zoom cutoff, and line half-width.
pub fn grid_style(input: &FrameInput<'_>) -> GridStyle {
    let tilted = input.mode != CameraMode::TopDown;
    let blend = tilted || input.fill != SectorFill::None;
    let pr = input.pixel_ratio;
    let colour = |base: Color, alpha: u8| -> [f32; 4] {
        if blend {
            rgba_f32([base[0], base[1], base[2], alpha])
        } else {
            rgba_f32(base)
        }
    };
    GridStyle {
        snap: input.grid.max(1) as f32,
        tile: TILE_GRID as f32,
        grid_rgba: colour(input.style.grid, GRID_BLEND_ALPHA),
        tile_rgba: colour(input.style.tile, ALIGN_GRID_BLEND_ALPHA),
        min_px: MIN_GRID_SPACING_PX * pr,
        half_px: (LINE_THICKNESS_PX * 0.5 * pr).max(0.5),
    }
}

/// One segment + one front-normal per line slot (free slots tombstone). Degenerate → zero normal.
fn draw_lines(input: &FrameInput<'_>, frame: &mut MapFrame) {
    (frame.lines, frame.normals) = build_line_instances(input);
}

/// All line + normal instances by slot (full build and slot-capacity growth).
pub fn build_line_instances(input: &FrameInput<'_>) -> (Vec<LineInst>, Vec<LineInst>) {
    let mut lines = vec![LineInst::default(); input.map.lines.slot_count()];
    let mut normals = vec![LineInst::default(); input.map.lines.slot_count()];
    for key in input.map.lines.keys() {
        let (seg, normal) = line_instances(input, key);
        lines[key.slot() as usize] = seg;
        normals[key.slot() as usize] = normal;
    }
    (lines, normals)
}

/// Segment + front-normal instances for a line; rides the front floor in wireframe, else the grid plane.
pub(crate) fn line_instances(input: &FrameInput<'_>, key: LineKey) -> (LineInst, LineInst) {
    let Some(line) = input.map.lines.get(key) else {
        return (LineInst::default(), LineInst::default());
    };
    let pr = input.pixel_ratio;
    let (Some(p1), Some(p2)) = (
        input.map.vertices.get(line.v1),
        input.map.vertices.get(line.v2),
    ) else {
        return (LineInst::default(), LineInst::default());
    };
    let selected = input.selection.contains(&SelItem::Line(key));
    let unenclosed = line.front.sector.is_none() || line.back.is_some_and(|b| b.sector.is_none());
    let colour = if selected {
        input.style.selected
    } else if input.highlight_unenclosed && unenclosed {
        input.style.warning
    } else if line.special != 0 {
        input.style.special
    } else if line.back.is_some() {
        input.style.two_sided
    } else {
        input.style.one_sided
    };
    let c = rgba_f32(colour);
    let pen = if selected {
        SELECTED_THICKNESS_PX
    } else {
        LINE_THICKNESS_PX
    } * pr;
    let a = [p1.x, p1.y];
    let b = [p2.x, p2.y];
    let z = if input.fill == SectorFill::None {
        line.front
            .sector
            .and_then(|s| input.map.sectors.get(s))
            .map_or(0.0, |s| s.floor_height as f32)
    } else {
        input.grid_z
    };
    let seg = line_at(a, b, pen, c, z);

    // Front normal: midpoint → right of v1→v2, i.e. (dy,-dx). Length = fixed tick in world units.
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = (dx * dx + dy * dy).sqrt();
    let normal = if len > 1.0 {
        let (nx, ny) = (dy / len, -dx / len);
        let tick = NORMAL_TICK_PX / input.zoom;
        let mid = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
        line_at(
            mid,
            [mid[0] + nx * tick, mid[1] + ny * tick],
            LINE_THICKNESS_PX * pr,
            c,
            z,
        )
    } else {
        LineInst::default()
    };
    (seg, normal)
}

fn draw_vertices(input: &FrameInput<'_>, frame: &mut MapFrame) {
    frame.verts = build_vert_instances(input);
}

/// All vertex markers by slot (full build and slot-capacity growth).
pub fn build_vert_instances(input: &FrameInput<'_>) -> Vec<MarkerInst> {
    let mut verts = vec![MarkerInst::default(); input.map.vertices.slot_count()];
    for key in input.map.vertices.keys() {
        verts[key.slot() as usize] = vert_instance(input, key);
    }
    verts
}

/// Per-vertex-slot floor Z for wireframe mode: lowest front-sector floor among touching lines; vertices on no sectored line → 0. Stale/empty in Colour/Texture mode.
pub fn build_vertex_floor_z(map: &EditorMap) -> Vec<f32> {
    let mut z = vec![f32::INFINITY; map.vertices.slot_count()];
    for line in map.lines.values() {
        let floor = line
            .front
            .sector
            .and_then(|s| map.sectors.get(s))
            .map(|s| s.floor_height as f32);
        if let Some(f) = floor {
            for vk in [line.v1, line.v2] {
                let slot = &mut z[vk.slot() as usize];
                *slot = slot.min(f);
            }
        }
    }
    for v in &mut z {
        if v.is_infinite() {
            *v = 0.0;
        }
    }
    z
}

/// The marker instance for a vertex (used by build + patch).
pub(crate) fn vert_instance(input: &FrameInput<'_>, key: VertKey) -> MarkerInst {
    let Some(v) = input.map.vertices.get(key) else {
        return MarkerInst::default();
    };
    let selected = input.selection.contains(&SelItem::Vertex(key));
    let colour = if selected {
        input.style.selected
    } else {
        input.style.point
    };
    let edge = if selected {
        VERTEX_DRAW_PX + SELECTED_VERTEX_BONUS_PX
    } else {
        VERTEX_DRAW_PX
    };
    let z = if input.fill == SectorFill::None {
        input
            .vert_z
            .get(key.slot() as usize)
            .copied()
            .unwrap_or(input.grid_z)
    } else {
        input.grid_z
    };
    MarkerInst {
        centre: [v.x, v.y],
        half_px: edge * input.pixel_ratio * 0.5,
        z,
        rgba: rgba_f32(colour),
    }
}

fn draw_things(input: &FrameInput<'_>, frame: &mut MapFrame) {
    frame.things = build_thing_instances(input);
}

/// All thing instances by slot (full build and slot-capacity growth).
pub fn build_thing_instances(input: &FrameInput<'_>) -> Vec<ThingInst> {
    let mut things = vec![ThingInst::default(); input.map.things.slot_count()];
    for key in input.map.things.keys() {
        things[key.slot() as usize] = thing_instance(input, key);
    }
    things
}

/// The instance for a thing; removed or skill-filtered-out things emit a zero-size slot.
pub(crate) fn thing_instance(input: &FrameInput<'_>, key: ThingKey) -> ThingInst {
    let Some(thing) = input.map.things.get(key) else {
        return ThingInst::default();
    };
    if !(input.thing_visible)(thing) {
        return ThingInst::default();
    }
    let centre = [thing.x as f32, thing.y as f32];
    let [hw, hh] = input
        .thing_extents
        .get(&thing.kind)
        .copied()
        .unwrap_or([FALLBACK_THING_HALF_EXTENT, FALLBACK_THING_HALF_EXTENT]);
    let radius = (input.thing_radius)(thing.kind);
    let z = thing.z as f32;
    if let Some(slot) = input.atlas.sprite_slots.get(&thing.kind) {
        return ThingInst {
            centre,
            half: [hw, hh],
            uv0: [slot.u0, slot.v0],
            uv1: [slot.u1, slot.v1],
            rgba: [0.0; 4],
            radius,
            z,
            _pad: [0.0; 2],
        };
    }
    // No sprite: colour square (uv0.x < 0 signals the shader).
    let colour = if input.selection.contains(&SelItem::Thing(key)) {
        input.style.selected
    } else {
        input
            .thing_colors
            .get(&thing.kind)
            .copied()
            .unwrap_or(input.style.thing)
    };
    ThingInst {
        centre,
        half: [radius, radius],
        uv0: [-1.0, -1.0],
        uv1: [-1.0, -1.0],
        rgba: rgba_f32(colour),
        radius,
        z,
        _pad: [0.0; 2],
    }
}

pub(crate) fn line_inst(a: [f32; 2], b: [f32; 2], pen: f32, rgba: [f32; 4]) -> LineInst {
    line_at(a, b, pen, rgba, 0.0)
}

/// Line instance on the plane at height `z`.
pub(crate) fn line_at(a: [f32; 2], b: [f32; 2], pen: f32, rgba: [f32; 4], z: f32) -> LineInst {
    LineInst {
        a,
        b,
        half_px: pen * 0.5,
        az: z,
        bz: z,
        rgba,
    }
}

pub(crate) fn push_line(
    buf: &mut Vec<LineInst>,
    a: [f32; 2],
    b: [f32; 2],
    pen: f32,
    rgba: [f32; 4],
) {
    buf.push(line_inst(a, b, pen, rgba));
}

pub(crate) fn push_marker(buf: &mut Vec<MarkerInst>, c: [f32; 2], half_px: f32, rgba: [f32; 4]) {
    buf.push(MarkerInst {
        centre: c,
        half_px,
        z: 0.0,
        rgba,
    });
}

/// Build the edit preview overlay: rubber-band, polyline, or move ghosts on the grid plane.
pub(crate) fn build_preview(
    overlay: &Overlay,
    style: &CanvasStyle,
    pixel_ratio: f32,
    z: f32,
) -> (Vec<LineInst>, Vec<MarkerInst>) {
    let mut lines = Vec::new();
    let mut markers = Vec::new();
    let pen = LINE_THICKNESS_PX * pixel_ratio;
    let accent = rgba_f32(style.selected);
    let mut polyline = |pts: &[[f32; 2]], close: bool| {
        for w in pts.windows(2) {
            lines.push(line_at(w[0], w[1], pen, accent, z));
        }
        if close && pts.len() >= 3 {
            lines.push(line_at(pts[pts.len() - 1], pts[0], pen, accent, z));
        }
    };
    match overlay {
        Overlay::None => {}
        Overlay::Rubber {
            a,
            b,
        } => {
            let box_pts = [*a, [b[0], a[1]], *b, [a[0], b[1]]];
            polyline(&box_pts, true);
        }
        Overlay::Poly {
            pts,
        } => polyline(pts, true),
        Overlay::Chain {
            pts,
            rubber,
        } => {
            polyline(pts, false);
            if let (Some(last), Some(r)) = (pts.last(), rubber) {
                lines.push(line_at(*last, *r, pen, accent, z));
            }
        }
        Overlay::Move {
            segments,
            points,
        } => {
            for [a, b] in segments {
                lines.push(line_at(*a, *b, pen, accent, z));
            }
            let half = VERTEX_DRAW_PX * pixel_ratio * 0.5;
            for p in points {
                markers.push(MarkerInst {
                    centre: *p,
                    half_px: half,
                    z,
                    rgba: accent,
                });
            }
        }
    }
    (lines, markers)
}

/// Transform-handle chrome: bbox outline, corner scale markers, rotate stem + knob.
pub(crate) fn push_handles(
    lines: &mut Vec<LineInst>,
    markers: &mut Vec<MarkerInst>,
    h: &TransformHandles,
    style: &CanvasStyle,
    pixel_ratio: f32,
    z: f32,
) {
    let pen = LINE_THICKNESS_PX * pixel_ratio;
    let accent = rgba_f32(style.selected);
    let corners = h.corners();
    for i in 0..4 {
        lines.push(line_at(corners[i], corners[(i + 1) % 4], pen, accent, z));
    }
    lines.push(line_at([h.pivot[0], h.max[1]], h.rotate, pen, accent, z));
    let half = HANDLE_DRAW_PX * pixel_ratio * 0.5;
    for c in corners.into_iter().chain([h.rotate]) {
        markers.push(MarkerInst {
            centre: c,
            half_px: half,
            z,
            rgba: accent,
        });
    }
}

/// RGBA8 → normalised f32.
pub fn rgba_f32(c: Color) -> [f32; 4] {
    [
        c[0] as f32 / 255.0,
        c[1] as f32 / 255.0,
        c[2] as f32 / 255.0,
        c[3] as f32 / 255.0,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::defaults;
    use crate::render::camera3d::Camera;
    use crate::render::triangulate::build_sector_tris;

    fn fit_view(map: &EditorMap) -> (Camera, f32, [f32; 2]) {
        let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
        for v in map.vertices.values() {
            lo[0] = lo[0].min(v.x);
            lo[1] = lo[1].min(v.y);
            hi[0] = hi[0].max(v.x);
            hi[1] = hi[1].max(v.y);
        }
        let viewport = [hi[0] - lo[0] + 256.0, hi[1] - lo[1] + 256.0];
        let mut cam = Camera::default();
        cam.look_down_at([(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5, 0.0]);
        cam.set_ortho_height(viewport[1]);
        (cam, 1.0, viewport)
    }

    #[allow(clippy::too_many_arguments)]
    fn input<'a>(
        map: &'a EditorMap,
        tris: &'a SectorTris,
        zoom: f32,
        style: &'a CanvasStyle,
        sel: &'a Selection,
        atlas: &'a AtlasMaps,
        extents: &'a HashMap<i32, [f32; 2]>,
        colors: &'a HashMap<i32, Color>,
        fill: SectorFill,
    ) -> FrameInput<'a> {
        FrameInput {
            map,
            tris,
            zoom,
            pixel_ratio: 1.0,
            style,
            selection: sel,
            grid: 8,
            fill,
            selected_sectors: &[],
            thing_visible: &|_| true,
            thing_extents: extents,
            thing_colors: colors,
            atlas,
            thing_radius: &defaults::thing_radius,
            sector_gradient: colorous::PLASMA,
            highlight_unenclosed: false,
            mode: CameraMode::TopDown,
            grid_z: 0.0,
            vert_z: &[],
        }
    }

    #[test]
    fn e1m1_frame_has_lines_grid_and_vertices() {
        let wad = wad::WadData::new(&test_utils::doom1_wad_path());
        let map = editor_core::import_wad_map(&wad, "E1M1").expect("E1M1 imports");
        let tris = build_sector_tris(&map);
        let zoom = fit_view(&map).1;
        let style = CanvasStyle::default();
        let (sel, atlas, extents, colors) = (
            Selection::default(),
            AtlasMaps::default(),
            HashMap::new(),
            HashMap::new(),
        );
        let inp = input(
            &map,
            &tris,
            zoom,
            &style,
            &sel,
            &atlas,
            &extents,
            &colors,
            SectorFill::None,
        );
        let (frame, slots) = build_map_geometry(&inp);
        let grid = grid_style(&inp);

        assert_eq!(frame.lines.len(), map.lines.len(), "one slot per line");
        assert_eq!(frame.normals.len(), map.lines.len(), "one normal per line");
        assert_eq!(
            frame.verts.len(),
            map.vertices.slot_count(),
            "one marker per vertex slot (import pruning leaves free slots)"
        );
        assert!(grid.snap >= 1.0 && grid.tile == 64.0, "grid spacings set");
        assert_eq!(frame.sector_attrs.len(), map.sectors.len());
        assert_eq!(slots.sector.len(), map.sectors.len(), "one span per sector");
        assert_eq!(slots.line.len(), map.lines.len(), "one span per line");
        assert_eq!(slots.used as usize, frame.surface3d.len());
        assert!(!frame.wire.is_empty(), "None mode builds the 3D wireframe");
    }

    #[test]
    fn vertex_floor_z_is_lowest_bordering_floor() {
        let wad = wad::WadData::new(&test_utils::doom1_wad_path());
        let map = editor_core::import_wad_map(&wad, "E1M1").expect("E1M1 imports");
        let z = build_vertex_floor_z(&map);
        assert_eq!(z.len(), map.vertices.slot_count(), "one Z per vertex slot");

        let keys: Vec<VertKey> = map.vertices.keys().collect();
        for &vk in &[keys[0], keys[keys.len() / 2], keys[keys.len() - 1]] {
            let want = map
                .lines
                .values()
                .filter(|l| l.v1 == vk || l.v2 == vk)
                .filter_map(|l| map.sectors.get(l.front.sector?))
                .map(|s| s.floor_height as f32)
                .fold(f32::INFINITY, f32::min);
            let want = if want.is_infinite() { 0.0 } else { want };
            assert_eq!(
                z[vk.slot() as usize],
                want,
                "vertex {vk:?} sits on its lowest bordering floor"
            );
        }
    }
}
