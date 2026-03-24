// Full-screen stretch blit shader.
// Vertex shader generates a clip-space triangle that covers the whole screen
// from vertex index alone — no vertex buffer required.
// Fragment shader samples the framebuffer texture with nearest-neighbour
// filtering, stretching it to fill the surface exactly.

@group(0) @binding(0) var t_frame: texture_2d<f32>;
@group(0) @binding(1) var s_frame: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VertexOutput {
    // Full-screen triangle covering NDC [-1,1]x[-1,1].
    // vi=0: (-1,-1) uv=(0,1)
    // vi=1: ( 3,-1) uv=(2,1)
    // vi=2: (-1, 3) uv=(0,-1)
    let x = f32(i32(vi == 1u)) * 4.0 - 1.0;
    let y = f32(i32(vi == 2u)) * 4.0 - 1.0;
    let u = f32(i32(vi == 1u)) * 2.0;
    let v = 1.0 - f32(i32(vi == 2u)) * 2.0;
    return VertexOutput(vec4<f32>(x, y, 0.0, 1.0), vec2<f32>(u, v));
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(t_frame, s_frame, in.uv);
}
