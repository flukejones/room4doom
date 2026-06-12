// Instanced things: a billboard quad (upright in 3D, flat in top-down) with the sprite, plus a top-down centre dot + radius ring editing aid.

struct Camera { view_proj: mat4x4<f32>, viewport: vec2<f32>, pad: vec2<f32>, cam_right: vec4<f32>, sel_colour: vec4<f32>, params: vec4<f32> };
@group(0) @binding(0) var<uniform> cam: Camera;
@group(1) @binding(0) var sprite_atlas: texture_2d<f32>;
@group(1) @binding(1) var sprite_samp: sampler;

const THING_DOT_PX: f32 = 2.5;
const THING_RING_PX: f32 = 1.0;
const THING_QUAD_PAD: f32 = 1.15;

fn to_clip(pos: vec3<f32>) -> vec4<f32> {
    return cam.view_proj * vec4<f32>(pos, 1.0);
}
fn to_ndc(pos: vec2<f32>) -> vec2<f32> {
    let c = to_clip(vec3<f32>(pos, 0.0));
    return c.xy / c.w;
}
fn quad_corner(vi: u32) -> vec2<f32> {
    var c = array<vec2<f32>, 6>(
        vec2<f32>(0.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0));
    return c[vi];
}

struct ThingOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) quad: vec2<f32>,                       // -1..1 across the quad
    @location(1) @interpolate(flat) uv0: vec2<f32>,
    @location(2) @interpolate(flat) uv1: vec2<f32>,
    @location(3) @interpolate(flat) rgba: vec4<f32>,
    @location(4) @interpolate(flat) has_sprite: u32,
    @location(5) @interpolate(flat) fit: vec2<f32>,     // sprite half-extent / quad extent
    @location(6) @interpolate(flat) extent: f32,        // quad half-extent, world units
    @location(7) @interpolate(flat) radius: f32,        // body radius, world units
};
@vertex
fn vs_thing(
    @builtin(vertex_index) vi: u32,
    @location(0) centre: vec2<f32>,
    @location(1) half: vec2<f32>,
    @location(2) uv0: vec2<f32>,
    @location(3) uv1: vec2<f32>,
    @location(4) rgba: vec4<f32>,
    @location(5) radius: f32,
    @location(6) z_pad: vec3<f32>,
) -> ThingOut {
    let q = quad_corner(vi);
    let corner = vec2<f32>(q.x * 2.0 - 1.0, q.y);
    let extent = max(max(half.x, half.y), radius * THING_QUAD_PAD);
    var o: ThingOut;
    if (cam.cam_right.w > 0.5) {
        // Upright quad facing the camera: width along the camera right axis, height along world +Z, bottom edge on the floor at z.
        let right = cam.cam_right.xyz;
        let up = vec3<f32>(0.0, 0.0, 1.0);
        let base = vec3<f32>(centre, z_pad.x);
        let world = base + right * (corner.x * extent) + up * ((corner.y * 0.5 + 0.5) * 2.0 * extent);
        o.pos = to_clip(world);
    } else {
        let world = centre + corner * extent;
        o.pos = vec4<f32>(to_ndc(world), 0.0, 1.0);
    }
    o.quad = corner;
    o.uv0 = uv0;
    o.uv1 = uv1;
    o.rgba = rgba;
    o.has_sprite = select(0u, 1u, uv0.x >= 0.0);
    o.fit = select(half / extent, vec2<f32>(1.0, 1.0), extent <= 0.0);
    o.extent = extent;
    o.radius = radius;
    return o;
}
@fragment
fn fs_thing(in: ThingOut) -> @location(0) vec4<f32> {
    // Centre dot + radius ring are top-down editing aids only.
    if (cam.cam_right.w < 0.5) {
        let per_world = cam.view_proj[0][0] * 0.5 * cam.viewport.x;
        let px = length(in.quad * in.extent) * per_world;
        if (abs(px - in.radius * per_world) < THING_RING_PX) {
            return vec4<f32>(1.0, 0.13, 0.13, 0.85);
        }
        if (px < THING_DOT_PX) {
            return vec4<f32>(1.0, 0.13, 0.13, 1.0);
        }
    }
    if (abs(in.quad.x) <= in.fit.x && abs(in.quad.y) <= in.fit.y) {
        let s = (in.quad / in.fit) * 0.5 + 0.5;
        let uv = vec2<f32>(mix(in.uv0.x, in.uv1.x, s.x), mix(in.uv1.y, in.uv0.y, s.y));
        if (in.has_sprite != 0u) {
            let dims = vec2<f32>(textureDimensions(sprite_atlas));
            let c = textureLoad(sprite_atlas, vec2<u32>(uv * dims), 0);
            if (c.a <= 0.0) { discard; }
            return c;
        }
        return in.rgba;
    }
    discard;
}
