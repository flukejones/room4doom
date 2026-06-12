// 3D sector surfaces: floors/ceilings sample the flat atlas, walls the wall atlas,
// shaded by per-sector brightness. None mode draws only a selected sector's
// floor/ceil tint; colour mode flat-shades the sector colour with per-wall angle
// contrast; texture mode samples. A selected sector's floor + ceil are tinted.

struct Camera { view_proj: mat4x4<f32>, viewport: vec2<f32>, pad: vec2<f32>, cam_right: vec4<f32>, sel_colour: vec4<f32>, params: vec4<f32> };
@group(0) @binding(0) var<uniform> cam: Camera;

fn to_clip(pos: vec3<f32>) -> vec4<f32> {
    return cam.view_proj * vec4<f32>(pos, 1.0);
}

@group(1) @binding(0) var flat_atlas: texture_2d<f32>;
@group(1) @binding(1) var wall_atlas: texture_2d<f32>;
struct SectorAttr { tile: vec2<f32>, pad: vec2<f32>, fallback: vec4<f32> };
@group(2) @binding(0) var<storage, read> brightness: array<f32>;
@group(2) @binding(1) var<storage, read> sector_attr: array<SectorAttr>;
struct Sector3D {
    floor_h: f32, ceil_h: f32,
    floor_tile: vec2<f32>, ceil_tile: vec2<f32>,
    selected: f32, pad: f32,
};
@group(2) @binding(2) var<storage, read> sector3d: array<Sector3D>;

const SURFACE_FLOOR: u32 = 0u;
const SURFACE_CEIL: u32 = 1u;
// Two-sided masked midtexture: drawn once, clipped to the opening (no vertical
// tiling). Mirrors frame3d::SURFACE_WALL_MASKED.
const SURFACE_WALL_MASKED: u32 = 5u;

// Fill-mode thresholds on cam.params.x (mirrors wgpu.rs FILL_NONE/COLOUR/TEXTURE):
// < FILL_NONE_MAX → None, < FILL_COLOUR_MAX → Colour, else Texture.
const FILL_NONE_MAX: f32 = 0.5;
const FILL_COLOUR_MAX: f32 = 1.5;

// Missing-resource marker (Texture mode only): walls signal it via a negative
// `rect.x`, flats via FLAT_TILE_MISSING (a -1 tile is "no flat" → discard).
const MAGENTA: vec3<f32> = vec3<f32>(1.0, 0.0, 1.0);
const FLAT_TILE_MISSING: f32 = -2.0;

// World-aligned 64-unit flat lookup: atlas RGB at `world` for the flat at `tile`.
fn sample_flat(world: vec2<f32>, tile: vec2<f32>) -> vec3<f32> {
    let lx = (i32(floor(world.x)) % 64 + 64) % 64;
    let ly = (i32(floor(-world.y)) % 64 + 64) % 64;
    let tx = u32(lx) + u32(tile.x);
    let ty = u32(ly) + u32(tile.y);
    return textureLoad(flat_atlas, vec2<u32>(tx, ty), 0).rgb;
}

// Atlas texel for wall UV `uv` within `rect` (x,y origin; z,w size). U always
// wraps; V wraps when `wrap_v` (solid wall), else V outside the texture height
// returns alpha 0 (a masked midtexture's see-through part).
fn wall_texel(uv: vec2<f32>, rect: vec4<f32>, wrap_v: bool) -> vec4<f32> {
    let lx = (i32(floor(uv.x)) % i32(rect.z) + i32(rect.z)) % i32(rect.z);
    let vy = i32(floor(uv.y));
    if (!wrap_v && (vy < 0 || vy >= i32(rect.w))) {
        return vec4<f32>(0.0);
    }
    let ly = (vy % i32(rect.w) + i32(rect.w)) % i32(rect.w);
    let tx = u32(lx) + u32(rect.x);
    let ty = u32(ly) + u32(rect.y);
    return textureLoad(wall_atlas, vec2<u32>(tx, ty), 0);
}

struct SurfaceOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) world: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) @interpolate(flat) rect: vec4<f32>,
    @location(3) @interpolate(flat) sector: u32,
    @location(4) @interpolate(flat) surface: u32,
    @location(5) @interpolate(flat) shade: f32,
};
@vertex
fn vs_surface3d(
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) rect: vec4<f32>,
    @location(3) sector: u32,
    @location(4) surface: u32,
    @location(6) shade: f32,
) -> SurfaceOut {
    var o: SurfaceOut;
    o.pos = to_clip(pos);
    o.world = pos.xy;
    o.uv = uv;
    o.rect = rect;
    o.sector = sector;
    o.surface = surface;
    o.shade = shade;
    return o;
}
@fragment
fn fs_surface3d(in: SurfaceOut) -> @location(0) vec4<f32> {
    let mode = cam.params.x;
    let is_flat = in.surface == SURFACE_FLOOR || in.surface == SURFACE_CEIL;
    let selected = sector3d[in.sector].selected > 0.5;

    // None mode: surfaces aren't drawn, except a selected sector's floor/ceil tint.
    if (mode < FILL_NONE_MAX) {
        if (!is_flat || !selected) { discard; }
        return cam.sel_colour;
    }

    let b = brightness[in.sector];

    // Texture mode only: in Colour mode a missing texture/flat falls through to
    // the sector's flat colour rather than drowning the view in magenta.
    if (mode >= FILL_COLOUR_MAX) {
        if (in.rect.x < 0.0) {
            return vec4<f32>(MAGENTA, 1.0);
        }
        if (is_flat) {
            let flat_tile = select(sector3d[in.sector].ceil_tile, sector3d[in.sector].floor_tile, in.surface == SURFACE_FLOOR);
            if (flat_tile.x == FLAT_TILE_MISSING) {
                return vec4<f32>(MAGENTA, 1.0);
            }
        }
    }

    // A masked midtexture keeps its transparency in every fill mode (its alpha
    // is its shape), unlike a solid wall, which renders flat colour in Colour mode.
    if (in.surface == SURFACE_WALL_MASKED) {
        let c = wall_texel(in.uv, in.rect, false);
        if (c.a <= 0.0) { discard; }
        let tinted = select(c.rgb, sector_attr[in.sector].fallback.rgb, mode < FILL_COLOUR_MAX);
        return vec4<f32>(tinted * b * in.shade, 1.0);
    }
    var rgb: vec3<f32>;
    if (mode < FILL_COLOUR_MAX) {
        rgb = sector_attr[in.sector].fallback.rgb * b * in.shade;
    } else if (is_flat) {
        let tile = select(sector3d[in.sector].ceil_tile, sector3d[in.sector].floor_tile, in.surface == SURFACE_FLOOR);
        if (tile.x < 0.0) { discard; }
        rgb = sample_flat(in.world, tile) * b;
    } else {
        let c = wall_texel(in.uv, in.rect, true);
        if (c.a <= 0.0) { discard; }
        rgb = c.rgb * b * in.shade;
    }
    if (is_flat && selected) {
        rgb = mix(rgb, cam.sel_colour.rgb, cam.sel_colour.a);
    }
    return vec4<f32>(rgb, 1.0);
}
