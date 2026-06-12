// World-space colour fills + instanced screen-space lines and markers (constant device-pixel width, so thickness never scales with zoom).

struct Camera { view_proj: mat4x4<f32>, viewport: vec2<f32>, pad: vec2<f32>, cam_right: vec4<f32>, sel_colour: vec4<f32>, params: vec4<f32> };
@group(0) @binding(0) var<uniform> cam: Camera;

fn to_clip(pos: vec3<f32>) -> vec4<f32> {
    return cam.view_proj * vec4<f32>(pos, 1.0);
}
fn ndc_to_px(ndc: vec2<f32>) -> vec2<f32> {
    return ndc * 0.5 * cam.viewport;
}
fn px_to_ndc(px: vec2<f32>) -> vec2<f32> {
    return px / (0.5 * cam.viewport);
}
fn quad_corner(vi: u32) -> vec2<f32> {
    var c = array<vec2<f32>, 6>(
        vec2<f32>(0.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0));
    return c[vi];
}

struct SolidOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) rgba: vec4<f32>,
};
@fragment
fn fs_solid(in: SolidOut) -> @location(0) vec4<f32> {
    return in.rgba;
}
@fragment
fn fs_solid_dim(in: SolidOut) -> @location(0) vec4<f32> {
    return vec4<f32>(in.rgba.rgb * (1.0 / 4.0), in.rgba.a);
}

// One instance per line: two endpoints, a px half-width, colour. The base quad is expanded in pixel space so thickness never scales with zoom.
@vertex
fn vs_line(
    @builtin(vertex_index) vi: u32,
    @location(0) a_pt: vec2<f32>,
    @location(1) b_pt: vec2<f32>,
    @location(2) half_px: f32,
    @location(3) az: f32,
    @location(4) bz: f32,
    @location(5) rgba: vec4<f32>,
) -> SolidOut {
    let q = quad_corner(vi);
    let ca = to_clip(vec3<f32>(a_pt, az));
    let cb = to_clip(vec3<f32>(b_pt, bz));
    let sa = ndc_to_px(ca.xy / ca.w);
    let sb = ndc_to_px(cb.xy / cb.w);
    var dir = sb - sa;
    let len = length(dir);
    if (len > 0.0001) { dir = dir / len; } else { dir = vec2<f32>(1.0, 0.0); }
    let perp = vec2<f32>(-dir.y, dir.x);
    let c = mix(ca, cb, q.x);
    let off = px_to_ndc(perp * (q.y * half_px)) * c.w;
    var o: SolidOut;
    o.pos = vec4<f32>(c.x + off.x, c.y + off.y, c.z, c.w);
    o.rgba = rgba;
    return o;
}

@vertex
fn vs_marker(
    @builtin(vertex_index) vi: u32,
    @location(0) centre: vec2<f32>,
    @location(1) half_px: f32,
    @location(2) z: f32,
    @location(3) rgba: vec4<f32>,
) -> SolidOut {
    let q = quad_corner(vi);
    // Map (along 0|1, side -1|1) → (±1, ±1) square corners.
    let corner = vec2<f32>(q.x * 2.0 - 1.0, q.y);
    let cc = to_clip(vec3<f32>(centre, z));
    let off = px_to_ndc(corner * half_px) * cc.w;
    var o: SolidOut;
    o.pos = vec4<f32>(cc.x + off.x, cc.y + off.y, cc.z, cc.w);
    o.rgba = rgba;
    return o;
}
