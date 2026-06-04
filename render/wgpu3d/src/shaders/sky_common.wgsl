// Shared sky sampling. Included by both the fullscreen sky pass and the scene
// shader (sky-flagged walls) via concat, so they declare their own bindings but
// sample identically. The view direction is `worldpos - eye` (raw, no flatten),
// off real geometry (scene) or reconstructed from the inverse view_proj
// (fullscreen). Both modes map by a CYLINDER (yaw->u, pitch->v): no pole, so no
// zenith convergence. Static samples SKY1; dynamic is fully procedural fbm cloud.

struct Sky {
    inv_view_proj: mat4x4<f32>,
    viewport: vec2<f32>,
    sky_band: vec2<f32>,    // [lo, hi): texture band's v-range in the extended tex
    sky_dark: vec4<f32>,    // dynamic cloud base rgb (SKY1 avg * 0.55), w pad
    sky_bright: vec4<f32>,  // dynamic cloud highlight rgb (SKY1 avg * 1.6), w pad
    mode: u32,              // 0 static, 1 dynamic
    time: f32,
    v_scale: f32,           // band-heights of v per tan(pitch), matches software3d
};

const SKY_TILES: f32 = 4.0;
const TAU: f32 = 6.2831853;
// Dynamic dome cloud scale (Quake 1: 6*63/128 = 189/64) + per-layer scroll.
// Quake scrolls the two layers at different rates (8 vs 16) for parallax.
const CLOUD_SCALE: f32 = 2.953125;
const CLOUD_SPEED_A: f32 = 0.15;
const CLOUD_SPEED_B: f32 = 0.30;
// Virtual texels per uv unit; snapping the noise uv to this grid gives the
// clouds SKY1-like texel grain instead of smooth gradients.
const CLOUD_TEXEL_DENSITY: f32 = 48.0;

// Cylindrical sample of the extended SKY1 texture. u from yaw (wraps). v from
// tan(pitch) = dir.z/horiz, mapped so the horizon sits at the band centre; past
// the band, v runs into the generated extension rows (smooth fade to zenith/
// nadir, baked by pic_data::sky::build_sky_extended). No edge-row repeat.
fn sky_static(sky: Sky, dir: vec3<f32>, tex: texture_2d<f32>, samp: sampler) -> vec3<f32> {
    let yaw = atan2(dir.y, dir.x);
    let u = fract(yaw / TAU * SKY_TILES);
    let horiz = max(length(dir.xy), 0.0001);
    // Horizon at the band centre; far up/down runs into the extension rows.
    let band_mid = (sky.sky_band.x + sky.sky_band.y) * 0.5;
    let band_h = sky.sky_band.y - sky.sky_band.x;
    let v = clamp(band_mid - (dir.z / horiz) * sky.v_scale * band_h, 0.0, 1.0);
    return textureSample(tex, samp, vec2<f32>(u, v)).rgb;
}

// Hash + value noise + fbm (procedural clouds, no texture).
fn hash2(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn value_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash2(i);
    let b = hash2(i + vec2<f32>(1.0, 0.0));
    let c = hash2(i + vec2<f32>(0.0, 1.0));
    let d = hash2(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn fbm(p: vec2<f32>) -> f32 {
    var v = 0.0;
    var amp = 0.5;
    var pp = p;
    for (var i = 0; i < 4; i = i + 1) {
        v = v + amp * value_noise(pp);
        pp = pp * 2.0;
        amp = amp * 0.5;
    }
    return v;
}

// Procedural dynamic clouds on a flattened sphere (Quake 1 EmitSkyPolys):
// dir.z *= 3, project onto the dome via normalize().xy. Two scrolling fbm layers,
// the second masked into drifting clouds over the first. Tinted dark..bright.
// The dome converges to a point overhead (authentic Quake zenith pinch).
fn snap_texel(p: vec2<f32>) -> vec2<f32> {
    return floor(p * CLOUD_TEXEL_DENSITY) / CLOUD_TEXEL_DENSITY;
}

fn sky_dynamic(sky: Sky, dir: vec3<f32>) -> vec3<f32> {
    var d = dir;
    d.z *= 3.0;                            // flatten the sphere
    let uv = normalize(d).xy * CLOUD_SCALE;
    let base = fbm(snap_texel(uv + vec2<f32>(sky.time * CLOUD_SPEED_A, 0.0)));
    let over = fbm(snap_texel(
        uv * 1.7 + vec2<f32>(sky.time * CLOUD_SPEED_B, sky.time * CLOUD_SPEED_B * 0.3),
    ));
    let sky_body = mix(sky.sky_dark.rgb, sky.sky_bright.rgb, base);
    let cloud = clamp((over - 0.5) * 2.0, 0.0, 1.0);
    return mix(sky_body, sky.sky_bright.rgb, cloud);
}

fn sky_colour_dir(
    sky: Sky,
    dir: vec3<f32>,
    static_tex: texture_2d<f32>,
    samp: sampler,
) -> vec3<f32> {
    if sky.mode == 1u {
        return sky_dynamic(sky, dir);
    }
    return sky_static(sky, dir, static_tex, samp);
}

// Fullscreen-pass helper: reconstruct the world view ray from a fragment's
// buffer-pixel position (eye-at-origin inverse view_proj).
fn sky_colour_frag(
    sky: Sky,
    frag: vec2<f32>,
    static_tex: texture_2d<f32>,
    samp: sampler,
) -> vec3<f32> {
    let ndc = vec2<f32>(
        frag.x / sky.viewport.x * 2.0 - 1.0,
        1.0 - frag.y / sky.viewport.y * 2.0,
    );
    let world = sky.inv_view_proj * vec4<f32>(ndc, 1.0, 1.0);
    let dir = world.xyz / world.w;
    return sky_colour_dir(sky, dir, static_tex, samp);
}
