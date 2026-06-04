// Weapon psprite overlay: screen-space quads (no world projection, no depth),
// drawn on top of everything. The CPU computes each layer's NDC rect from
// Doom's 320x200 placement (weapon.rs); the vertex shader just expands it. Light
// is a precomputed multiplier (psprite has no world depth). Cutout discard;
// the fuzz entry halves the background (spectre player), matching the world
// sprite fuzz.

struct PspriteInstance {
    ndc_min: vec2<f32>,     // bottom-left in NDC (x right, y up)
    ndc_max: vec2<f32>,     // top-right in NDC
    rect_origin: vec2<u32>, // atlas pixel origin
    rect_size: vec2<u32>,   // atlas pixel size
    layer: u32,             // atlas array layer
    flip: u32,              // 0/1 horizontal flip
    light: f32,             // precomputed brightness multiplier 0..1
    flags: u32,             // reserved
};

@group(0) @binding(0) var<storage, read> instances: array<PspriteInstance>;
@group(1) @binding(0) var sprite_atlas: texture_2d_array<f32>;
@group(1) @binding(1) var sprite_samp: sampler;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) layer: u32,
    @location(2) @interpolate(flat) rect_origin: vec2<u32>,
    @location(3) @interpolate(flat) rect_size: vec2<u32>,
    @location(4) @interpolate(flat) light: f32,
};

// corner_uv comes from sprite_quad_common.wgsl (concatenated ahead).
@vertex
fn vs_main(@builtin(vertex_index) corner: u32, @builtin(instance_index) inst: u32) -> VsOut {
    let s = instances[inst];
    let quad = corner_uv(corner);
    // u maps to NDC x (min..max); v=0 (top) -> ndc_max.y, v=1 (bottom) -> ndc_min.y.
    let ndc_x = mix(s.ndc_min.x, s.ndc_max.x, quad.x);
    let ndc_y = mix(s.ndc_max.y, s.ndc_min.y, quad.y);

    var out: VsOut;
    out.pos = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    let u = select(quad.x, 1.0 - quad.x, s.flip == 1u);
    out.uv = vec2<f32>(u, quad.y);
    out.layer = s.layer;
    out.rect_origin = s.rect_origin;
    out.rect_size = s.rect_size;
    out.light = s.light;
    return out;
}

fn sample_rect(in: VsOut) -> vec4<f32> {
    let size = vec2<f32>(in.rect_size);
    let px = vec2<f32>(in.rect_origin) + clamp(in.uv, vec2<f32>(0.0), vec2<f32>(1.0)) * size;
    let dims = vec2<f32>(textureDimensions(sprite_atlas));
    return textureSample(sprite_atlas, sprite_samp, px / dims, in.layer);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let rgba = sample_rect(in);
    if rgba.a < 0.5 {
        discard;
    }
    return vec4<f32>(rgba.rgb * in.light, 1.0);
}

// Spectre-player weapon fuzz: halve the background where the weapon covers.
@fragment
fn fs_fuzz(in: VsOut) -> @location(0) vec4<f32> {
    let rgba = sample_rect(in);
    if rgba.a < 0.5 {
        discard;
    }
    return vec4<f32>(0.0, 0.0, 0.0, 0.5);
}
