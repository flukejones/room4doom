// Blends one UI plane over the scene as a quad in the viewer's space. Identity
// makes it the screen; a headset gives it a real distance and each eye's own
// asymmetric frustum.

@group(0) @binding(0) var ui_tex: texture_2d<f32>;
@group(0) @binding(1) var ui_smp: sampler;

struct Placement {
    clip_from_panel: mat4x4<f32>,
};
@group(0) @binding(2) var<uniform> panel: Placement;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VsOut {
    // Unit-square triangle strip; the matrix carries the panel's half-extents.
    let x = f32(vi & 1u) * 2.0 - 1.0;
    let y = f32(vi >> 1u) * 2.0 - 1.0;
    return VsOut(
        panel.clip_from_panel * vec4<f32>(x, y, 0.0, 1.0),
        vec2<f32>(x * 0.5 + 0.5, 0.5 - y * 0.5),
    );
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return textureSample(ui_tex, ui_smp, in.uv);
}
