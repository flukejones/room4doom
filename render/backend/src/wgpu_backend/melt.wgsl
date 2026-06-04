// Vanilla Doom column-melt wipe: the old frame's columns slide down over the new
// frame. Per-column offset `y[x]` (px) grows over the wipe; mirrors
// render_common::wipe (do_melt_pixels): y<0 shows the old column unshifted,
// 0<=y<h shows old shifted down by y (new above it), y>=h shows the new frame.

@group(0) @binding(0) var new_tex: texture_2d<f32>;
@group(0) @binding(1) var smp: sampler;
@group(0) @binding(2) var old_tex: texture_2d<f32>;
@group(0) @binding(3) var<storage, read> col_y: array<i32>;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VsOut {
    var out: VsOut;
    let x = f32((vi << 1u) & 2u);
    let y = f32(vi & 2u);
    out.uv = vec2<f32>(x, y);
    out.pos = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let dims = vec2<f32>(textureDimensions(new_tex));
    let px = vec2<i32>(in.uv * dims);
    let melt_y = col_y[px.x];

    if melt_y < 0 {
        // Column not yet melting — show the old frame unshifted.
        return textureSample(old_tex, smp, in.uv);
    }
    if px.y < melt_y {
        // Above the melt line — the new frame shows through.
        return textureSample(new_tex, smp, in.uv);
    }
    // Old frame shifted down by melt_y: output row py samples old row py-melt_y.
    let src_y = f32(px.y - melt_y) + 0.5;
    let old_uv = vec2<f32>(in.uv.x, src_y / dims.y);
    return textureSample(old_tex, smp, old_uv);
}
