// Scene pass: pull shared positions + per-corner UV from storage buffers, sample
// the wall/flat atlas, apply live sector light + distance falloff. Non-indexed
// draw over triangle corners. Sky-flagged corners sample the shared sky (group 3)
// and write real depth so they occlude geometry behind them (GLQuake/software3d).
// sky_common.wgsl is concatenated ahead of this for the shared sky functions.

struct Camera {
    view_proj: mat4x4<f32>,
    camera_pos: vec3<f32>,
    extralight: f32,        // gun-flash extra light band
};

struct Position {
    pos: vec4<f32>,         // xyz = world pos, w unused
};

struct CornerAttr {
    tex: u32,               // texture id (u32::MAX = untextured)
    is_flat: u32,
    sector: u32,            // sector id, indexes sector_light
    contrast_adjust: i32,   // fake-contrast band delta
    is_sky: u32,            // sky surface: sample sky, write depth (occludes)
    is_masked_mid: u32,     // two-sided middle: discard v outside [0,1), no tile
};

struct AtlasRect {
    origin: vec2<u32>,
    size: vec2<u32>,
    layer: u32,
    pad0: u32,
    pad1: u32,
    pad2: u32,
};

@group(0) @binding(0) var<uniform> camera: Camera;

@group(1) @binding(0) var<storage, read> positions: array<Position>;
@group(1) @binding(1) var<storage, read> corner_index: array<u32>;
@group(1) @binding(2) var<storage, read> corner_attr: array<CornerAttr>;
@group(1) @binding(3) var<storage, read> corner_uv: array<vec2<f32>>;
@group(1) @binding(4) var<storage, read> sector_light: array<f32>;
@group(1) @binding(5) var<storage, read> corner_scroll: array<f32>;

@group(2) @binding(0) var wall_atlas: texture_2d_array<f32>;
@group(2) @binding(1) var flat_atlas: texture_2d_array<f32>;
@group(2) @binding(2) var<storage, read> wall_rects: array<AtlasRect>;
@group(2) @binding(3) var<storage, read> flat_rects: array<AtlasRect>;
@group(2) @binding(4) var atlas_sampler: sampler;
@group(2) @binding(5) var<storage, read> wall_translation: array<u32>;
@group(2) @binding(6) var<storage, read> flat_translation: array<u32>;

@group(3) @binding(0) var<uniform> sky: Sky;
@group(3) @binding(1) var sky_static_tex: texture_2d<f32>;
@group(3) @binding(2) var sky_samp: sampler;

// Doom diminishing light: a colourmap row (0 bright .. max_row dark) from band +
// distance; startmap = (light_levels-band)*4; closer subtracts rows (brighter);
// intensity = (1 - row/max_row)^light_gamma (true black at max_row). Byte-matches
// the Rust LightParams (light.rs); the field names are kept in sync by hand.
struct LightParams {
    light_levels: f32,
    max_row: f32,
    light_gamma: f32,
    dist_scale: f32,
    dist_rows_max: f32,
};
@group(4) @binding(0) var<uniform> light: LightParams;

const NO_TEX: u32 = 0xffffffffu;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) start_row: f32,
    @location(2) @interpolate(flat) tex: u32,
    @location(3) @interpolate(flat) is_flat: u32,
    @location(4) view_dist: f32,
    @location(5) @interpolate(flat) is_sky: u32,
    // Dome direction (worldpos - eye, z flattened) for sky-flagged corners.
    @location(6) sky_dir: vec3<f32>,
    @location(7) @interpolate(flat) is_masked_mid: u32,
};

// Sector baseline colourmap row (0 bright .. 31 dark) from the light band. The
// band math mirrors level::bsp3d::build::light_band (shaders can't call Rust);
// keep the two in sync.
fn start_row(sector: u32, contrast_adjust: i32) -> f32 {
    let raw = sector_light[sector];
    let base = min(floor(raw / 16.0) + camera.extralight, light.light_levels);
    let band = clamp(base + f32(contrast_adjust), 0.0, light.light_levels);
    return (light.light_levels - band) * 4.0;
}

@vertex
fn vs_main(@builtin(vertex_index) corner: u32) -> VsOut {
    let vidx = corner_index[corner];
    let world = positions[vidx].pos.xyz;
    let attr = corner_attr[corner];

    var out: VsOut;
    // Eye-at-origin: subtract camera position per vertex (matches software3d).
    out.pos = camera.view_proj * vec4<f32>(world - camera.camera_pos, 1.0);
    // Texel-space UV + per-corner horizontal scroll (special-48 scrollers).
    out.uv = corner_uv[corner] + vec2<f32>(corner_scroll[corner], 0.0);
    out.start_row = start_row(attr.sector, attr.contrast_adjust);
    out.tex = attr.tex;
    out.is_flat = attr.is_flat;
    out.view_dist = out.pos.w;
    out.is_sky = attr.is_sky;
    // View direction off real geometry for the cylinder sky mapping (no flatten).
    out.sky_dir = world - camera.camera_pos;
    out.is_masked_mid = attr.is_masked_mid;
    return out;
}

// Sample an atlas array: wrap the texel UV within the texture's rect, then
// sample the rect's layer.
fn sample_atlas(
    atlas: texture_2d_array<f32>,
    rect: AtlasRect,
    uv: vec2<f32>,
) -> vec4<f32> {
    let size = vec2<f32>(rect.size);
    // Repeat-wrap within the texture region.
    let wrapped = (fract(uv / size)) * size;
    let px = vec2<f32>(rect.origin) + wrapped;
    let dims = vec2<f32>(textureDimensions(atlas));
    return textureSample(atlas, atlas_sampler, px / dims, rect.layer);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Sky walls: sample the shared sky by the corner's dome direction, keep the
    // fragment (depth writes) so geometry behind is occluded. Always full bright.
    if in.is_sky == 1u {
        let c = sky_colour_dir(sky, in.sky_dir, sky_static_tex, sky_samp);
        return vec4<f32>(c, 1.0);
    }
    // Two-sided middle (masked): drawn once, not tiled. Discard the fragment
    // where the normalised v leaves [0,1) instead of wrapping (matches software3d).
    // Masked middles are always walls, so translate the wall id here.
    if in.is_masked_mid == 1u {
        let v_norm = in.uv.y / f32(wall_rects[wall_translation[in.tex]].size.y);
        if v_norm < 0.0 || v_norm >= 1.0 {
            discard;
        }
    }
    // Single Doom diminishing-light row: baseline minus a near-distance boost.
    let near = clamp((1.0 / max(in.view_dist, 1.0)) * light.dist_scale, 0.0, light.dist_rows_max);
    let row = clamp(in.start_row - near * 0.5, 0.0, light.max_row);
    let intensity = pow(1.0 - row / light.max_row, light.light_gamma);

    if in.tex == NO_TEX {
        return vec4<f32>(vec3<f32>(0.4) * intensity, 1.0);
    }
    // Animation: base id -> current frame id via the per-kind translation table.
    // Indexed only here, so flats never read past the wall table (and vice versa).
    var rgba: vec4<f32>;
    if in.is_flat == 1u {
        rgba = sample_atlas(flat_atlas, flat_rects[flat_translation[in.tex]], in.uv);
    } else {
        rgba = sample_atlas(wall_atlas, wall_rects[wall_translation[in.tex]], in.uv);
    }
    if rgba.a < 0.5 {
        discard;
    }
    return vec4<f32>(rgba.rgb * intensity, 1.0);
}
