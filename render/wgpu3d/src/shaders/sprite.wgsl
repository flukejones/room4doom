// World billboard sprites: one instanced quad per visible thing. The vertex
// shader expands a per-instance floor anchor + horizontal extents into a
// camera-facing (cylindrical, yaw-only) quad in world space, then projects it
// eye-at-origin (subtract camera_pos), matching software3d's billboard. The
// fragment samples the sprite atlas array and applies the same smooth
// distance/sector light as the scene pass, discarding transparent texels
// (1-bit cutout, no blending).

struct SpriteCam {
    view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,  // xyz eye, w unused
    cam_right: vec4<f32>,   // (sin angle, -cos angle, 0), w unused
    cam_up: vec4<f32>,      // (0, 0, 1, 0)
    extralight: f32,
    pad0: f32,
    pad1: f32,
    pad2: f32,
};

struct SpriteInstance {
    center: vec3<f32>,      // world floor anchor (interpolated base x,y,z)
    left_dist: f32,         // horizontal extent toward cam_right- side
    right_dist: f32,        // horizontal extent toward cam_right+ side
    height: f32,            // top_z = center.z + height
    rect_origin: vec2<u32>, // atlas pixel origin
    rect_size: vec2<u32>,   // atlas pixel size
    layer: u32,             // atlas array layer
    flip: u32,              // 0/1 horizontal flip (U swap)
    brightness: u32,        // 0..15 sector/full-bright light level
    flags: u32,             // bit0 fuzz/shadow (handled by pipeline variant)
    pad0: u32,
    pad1: u32,
};

// corner_uv comes from sprite_quad_common.wgsl (concatenated ahead).
// LightParams byte-matches the Rust LightParams (light.rs); see scene.wgsl.
struct LightParams {
    light_levels: f32,
    max_row: f32,
    light_gamma: f32,
    dist_scale: f32,
    dist_rows_max: f32,
};

@group(0) @binding(0) var<uniform> cam: SpriteCam;
@group(1) @binding(0) var<storage, read> instances: array<SpriteInstance>;
@group(2) @binding(0) var sprite_atlas: texture_2d_array<f32>;
@group(2) @binding(1) var sprite_samp: sampler;
@group(3) @binding(0) var<uniform> light: LightParams;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) layer: u32,
    @location(2) @interpolate(flat) rect_origin: vec2<u32>,
    @location(3) @interpolate(flat) rect_size: vec2<u32>,
    @location(4) @interpolate(flat) start_row: f32,
    @location(5) view_dist: f32,
};

// Sector baseline colourmap row from the brightness band (mirrors
// level::bsp3d::build::light_band and scene.wgsl start_row).
fn sprite_start_row(brightness: u32) -> f32 {
    let band = min(f32(brightness) + cam.extralight, light.light_levels);
    return (light.light_levels - band) * 4.0;
}

@vertex
fn vs_main(@builtin(vertex_index) corner: u32, @builtin(instance_index) inst: u32) -> VsOut {
    let s = instances[inst];
    let quad = corner_uv(corner);

    // Horizontal world offset along cam_right; vertical along world Z.
    let dist = mix(s.left_dist, s.right_dist, quad.x);
    let top_z = s.center.z + s.height;
    let z = mix(top_z, s.center.z, quad.y);
    let world = vec3<f32>(
        s.center.x + cam.cam_right.x * dist,
        s.center.y + cam.cam_right.y * dist,
        z,
    );

    var out: VsOut;
    out.pos = cam.view_proj * vec4<f32>(world - cam.camera_pos.xyz, 1.0);
    // Flip swaps U so the texture mirrors horizontally.
    let u = select(quad.x, 1.0 - quad.x, s.flip == 1u);
    out.uv = vec2<f32>(u, quad.y);
    out.layer = s.layer;
    out.rect_origin = s.rect_origin;
    out.rect_size = s.rect_size;
    out.start_row = sprite_start_row(s.brightness);
    out.view_dist = out.pos.w;
    return out;
}

// Sample the sprite atlas rect at the interpolated [0,1] UV.
fn sample_rect(rect_origin: vec2<u32>, rect_size: vec2<u32>, uv: vec2<f32>, layer: u32) -> vec4<f32> {
    let px = vec2<f32>(rect_origin) + clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0)) * vec2<f32>(rect_size);
    let dims = vec2<f32>(textureDimensions(sprite_atlas));
    return textureSample(sprite_atlas, sprite_samp, px / dims, layer);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let rgba = sample_rect(in.rect_origin, in.rect_size, in.uv, in.layer);
    if rgba.a < 0.5 {
        discard;
    }
    // Same smooth diminishing-light row as the scene pass.
    let near = clamp((1.0 / max(in.view_dist, 1.0)) * light.dist_scale, 0.0, light.dist_rows_max);
    let row = clamp(in.start_row - near * 0.5, 0.0, light.max_row);
    let intensity = pow(1.0 - row / light.max_row, light.light_gamma);
    return vec4<f32>(rgba.rgb * intensity, 1.0);
}

// Spectre/shadow fuzz: where the sprite covers an opaque texel, halve the
// background (RGB-halve, matching software3d's direct-pixel .darken()). The
// pipeline blends src*0 + dst*(1-srcAlpha); emitting alpha 0.5 → dst*0.5.
// Doom's FUZZ_TABLE vertical jitter is omitted (flat darken, no shimmer):
// framebuffer blend cannot sample a displaced row; parity would need a pass
// that reads the scene texture at an offset.
@fragment
fn fs_fuzz(in: VsOut) -> @location(0) vec4<f32> {
    let rgba = sample_rect(in.rect_origin, in.rect_size, in.uv, in.layer);
    if rgba.a < 0.5 {
        discard;
    }
    return vec4<f32>(0.0, 0.0, 0.0, 0.5);
}
