// Apply gameplay scene effects (player tint + invuln inverse) to the scene
// colour, then composite the UI texture over the result into the frame texture.
// Effects hit the scene only — the UI mixes over the tinted scene, so the HUD
// stays untinted (matching vanilla).

@group(0) @binding(0) var scene_tex: texture_2d<f32>;
@group(0) @binding(1) var scene_smp: sampler;
@group(0) @binding(2) var ui_tex: texture_2d<f32>;
@group(0) @binding(3) var ui_smp: sampler;

// Byte-matches the Rust `wgpu3d::SceneEffects`.
struct SceneEffects {
    tint_rgb: vec3<f32>,
    tint_pct: f32,
    invert: f32,
    bleed_active: f32,
};
@group(0) @binding(4) var<uniform> fx: SceneEffects;

// Per-column health-bleed geometry: [shown, bound0, bound1, _] in pixels.
// Mirrors render_common::HealthBleed (one element per scene column).
@group(0) @binding(5) var<storage, read> bleed_cols: array<vec4<f32>>;

// Red ramp the 3 bleed bands darken through (top band darkest), the GPU analogue
// of the PLAYPAL red palettes the CPU path remaps through.
const BLEED_BAND0: vec3<f32> = vec3<f32>(0.78, 0.0, 0.0);
const BLEED_BAND1: vec3<f32> = vec3<f32>(0.6, 0.0, 0.0);
const BLEED_BAND2: vec3<f32> = vec3<f32>(0.46, 0.0, 0.0);
// Blend strength of the red over the scene (the scene stays visible through it).
const BLEED_ALPHA: f32 = 0.4;

// Bleed colour at pixel (x,y), or the unchanged scene where no column covers it.
// Reproduces HealthBleed::palette_offset's band select, blended over the scene.
fn apply_bleed(scene: vec3<f32>, px: vec2<i32>) -> vec3<f32> {
    let col = bleed_cols[px.x];
    let shown = col.x;
    if f32(px.y) >= shown {
        return scene;
    }
    let from_edge = shown - 1.0 - f32(px.y);
    var band = 0;
    if from_edge >= col.y { band = 1; }
    if from_edge >= col.z { band = 2; }
    var red = BLEED_BAND0;
    if band == 1 { red = BLEED_BAND1; }
    if band == 2 { red = BLEED_BAND2; }
    return mix(scene, red, BLEED_ALPHA);
}

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VsOut {
    // Full-screen triangle from the vertex index alone.
    var out: VsOut;
    let x = f32((vi << 1u) & 2u);
    let y = f32(vi & 2u);
    out.uv = vec2<f32>(x, y);
    out.pos = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    var scene = textureSample(scene_tex, scene_smp, in.uv).rgb;
    // Invuln inverse map first (fullbright inversion), then the damage/bonus/
    // radsuit colour wash, then the health-bleed columns over the top.
    scene = mix(scene, vec3<f32>(1.0) - scene, fx.invert);
    scene = mix(scene, fx.tint_rgb, fx.tint_pct);
    if fx.bleed_active > 0.5 {
        let dims = vec2<f32>(textureDimensions(scene_tex));
        let px = vec2<i32>(in.uv * dims);
        scene = apply_bleed(scene, px);
    }

    let ui = textureSample(ui_tex, ui_smp, in.uv);
    // UI alpha selects UI over scene (transparent UI shows the scene).
    let rgb = mix(scene, ui.rgb, ui.a);
    return vec4<f32>(rgb, 1.0);
}
