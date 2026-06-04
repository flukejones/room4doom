// Fullscreen sky background pass. Bindings here; sampling in sky_common.wgsl
// (concatenated). Fills the colour target before the scene pass loads over it.

@group(0) @binding(0) var<uniform> sky: Sky;
@group(0) @binding(1) var sky_static_tex: texture_2d<f32>;
@group(0) @binding(2) var sky_samp: sampler;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    let x = f32((vi << 1u) & 2u);
    let y = f32(vi & 2u);
    return vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let c = sky_colour_frag(sky, frag.xy, sky_static_tex, sky_samp);
    return vec4<f32>(c, 1.0);
}
