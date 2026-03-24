// CRT shader — faithful WGSL port of crt-lottes by Timothy Lottes (public domain).
//
// Original: https://github.com/libretro/glsl-shaders/blob/master/crt/shaders/crt-lottes.glsl
//
// UV (0,0)=top-left, (1,1)=bottom-right.

@group(0) @binding(0) var t_frame: texture_2d<f32>;
@group(0) @binding(1) var s_frame: sampler;

// -- Tunable constants --------------------------------------------------------

// Scanline hardness. More negative = sharper. Lottes default: -8.0
const HARD_SCAN: f32  = -8.0;
// Pixel hardness (horizontal filter). Lottes default: -3.0
const HARD_PIX: f32   = -3.0;
// Screen warp (barrel distortion). Lottes defaults: 0.031, 0.041
const WARP_X: f32     = 0.031;
const WARP_Y: f32     = 0.041;
// Shadow mask dark/light levels. Lottes defaults: 0.5, 1.5
const MASK_DARK: f32  = 0.5;
const MASK_LIGHT: f32 = 1.5;
// 0=none 1=compressed-TV 2=aperture-grille 3=stretched-VGA 4=VGA
const SHADOW_MASK: i32 = 3;
// Bloom horizontal/vertical softness. Lottes defaults: -1.5, -2.0
const HARD_BLOOM_PIX: f32  = -1.5;
const HARD_BLOOM_SCAN: f32 = -2.0;
// Bloom mix amount.
const BLOOM_AMOUNT: f32 = 0.15;
// Brightness boost.
const BRIGHT_BOOST: f32 = 1.0;
// Gaussian kernel shape (power applied to distance). 2.0 = true Gaussian.
const SHAPE: f32 = 2.0;

// -- sRGB / linear conversion -------------------------------------------------

fn to_linear1(c: f32) -> f32 {
    if c <= 0.04045 { return c / 12.92; }
    return pow((c + 0.055) / 1.055, 2.4);
}
fn to_linear(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(to_linear1(c.r), to_linear1(c.g), to_linear1(c.b));
}

fn to_srgb1(c: f32) -> f32 {
    if c < 0.0031308 { return c * 12.92; }
    return 1.055 * pow(c, 0.41666) - 0.055;
}
fn to_srgb(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(to_srgb1(c.r), to_srgb1(c.g), to_srgb1(c.b));
}

// -- Core helpers -------------------------------------------------------------

// Nearest emulated sample at integer texel offset `off`.
fn fetch(pos: vec2<f32>, off: vec2<f32>, tex_size: vec2<f32>) -> vec3<f32> {
    let p = (floor(pos * tex_size + off) + vec2<f32>(0.5)) / tex_size;
    return to_linear(BRIGHT_BOOST * textureSample(t_frame, s_frame, p).rgb);
}

// Distance from pos (in texel space) to the nearest texel centre, per axis.
fn dist(pos: vec2<f32>, tex_size: vec2<f32>) -> vec2<f32> {
    let p = pos * tex_size;
    return -((p - floor(p)) - vec2<f32>(0.5));
}

// 1-D Gaussian with configurable shape exponent.
fn gaus(d: f32, scale: f32) -> f32 {
    return exp2(scale * pow(abs(d), SHAPE));
}

// 3-tap horizontal Gaussian at scanline row offset `off`.
fn horz3(pos: vec2<f32>, off: f32, tex_size: vec2<f32>) -> vec3<f32> {
    let b   = fetch(pos, vec2<f32>(-1.0, off), tex_size);
    let c   = fetch(pos, vec2<f32>( 0.0, off), tex_size);
    let d   = fetch(pos, vec2<f32>( 1.0, off), tex_size);
    let dst = dist(pos, tex_size).x;
    let wb  = gaus(dst - 1.0, HARD_PIX);
    let wc  = gaus(dst + 0.0, HARD_PIX);
    let wd  = gaus(dst + 1.0, HARD_PIX);
    return (b * wb + c * wc + d * wd) / (wb + wc + wd);
}

// 5-tap horizontal Gaussian.
fn horz5(pos: vec2<f32>, off: f32, tex_size: vec2<f32>) -> vec3<f32> {
    let a   = fetch(pos, vec2<f32>(-2.0, off), tex_size);
    let b   = fetch(pos, vec2<f32>(-1.0, off), tex_size);
    let c   = fetch(pos, vec2<f32>( 0.0, off), tex_size);
    let d   = fetch(pos, vec2<f32>( 1.0, off), tex_size);
    let e   = fetch(pos, vec2<f32>( 2.0, off), tex_size);
    let dst = dist(pos, tex_size).x;
    let wa  = gaus(dst - 2.0, HARD_PIX);
    let wb  = gaus(dst - 1.0, HARD_PIX);
    let wc  = gaus(dst + 0.0, HARD_PIX);
    let wd  = gaus(dst + 1.0, HARD_PIX);
    let we  = gaus(dst + 2.0, HARD_PIX);
    return (a * wa + b * wb + c * wc + d * wd + e * we) / (wa + wb + wc + wd + we);
}

// 7-tap horizontal Gaussian (bloom, softer kernel).
fn horz7(pos: vec2<f32>, off: f32, tex_size: vec2<f32>) -> vec3<f32> {
    let a   = fetch(pos, vec2<f32>(-3.0, off), tex_size);
    let b   = fetch(pos, vec2<f32>(-2.0, off), tex_size);
    let c   = fetch(pos, vec2<f32>(-1.0, off), tex_size);
    let d   = fetch(pos, vec2<f32>( 0.0, off), tex_size);
    let e   = fetch(pos, vec2<f32>( 1.0, off), tex_size);
    let f   = fetch(pos, vec2<f32>( 2.0, off), tex_size);
    let g   = fetch(pos, vec2<f32>( 3.0, off), tex_size);
    let dst = dist(pos, tex_size).x;
    let wa  = gaus(dst - 3.0, HARD_BLOOM_PIX);
    let wb  = gaus(dst - 2.0, HARD_BLOOM_PIX);
    let wc  = gaus(dst - 1.0, HARD_BLOOM_PIX);
    let wd  = gaus(dst + 0.0, HARD_BLOOM_PIX);
    let we  = gaus(dst + 1.0, HARD_BLOOM_PIX);
    let wf  = gaus(dst + 2.0, HARD_BLOOM_PIX);
    let wg  = gaus(dst + 3.0, HARD_BLOOM_PIX);
    return (a*wa + b*wb + c*wc + d*wd + e*we + f*wf + g*wg) / (wa+wb+wc+wd+we+wf+wg);
}

// Scanline weight at row offset `off`.
fn scan(pos: vec2<f32>, off: f32, tex_size: vec2<f32>) -> f32 {
    return gaus(dist(pos, tex_size).y + off, HARD_SCAN);
}

// Bloom scanline weight.
fn bloom_scan(pos: vec2<f32>, off: f32, tex_size: vec2<f32>) -> f32 {
    return gaus(dist(pos, tex_size).y + off, HARD_BLOOM_SCAN);
}

// Tri-scanline sample (3 rows, each 3–5 tap filtered horizontally).
fn tri(pos: vec2<f32>, tex_size: vec2<f32>) -> vec3<f32> {
    let a  = horz3(pos, -1.0, tex_size);
    let b  = horz5(pos,  0.0, tex_size);
    let c  = horz3(pos,  1.0, tex_size);
    let wa = scan(pos, -1.0, tex_size);
    let wb = scan(pos,  0.0, tex_size);
    let wc = scan(pos,  1.0, tex_size);
    return a * wa + b * wb + c * wc;
}

// Bloom: 5 rows, 5–7 tap filtered, soft scanline weights.
fn bloom(pos: vec2<f32>, tex_size: vec2<f32>) -> vec3<f32> {
    let a  = horz5(pos, -2.0, tex_size);
    let b  = horz7(pos, -1.0, tex_size);
    let c  = horz7(pos,  0.0, tex_size);
    let d  = horz7(pos,  1.0, tex_size);
    let e  = horz5(pos,  2.0, tex_size);
    let wa = bloom_scan(pos, -2.0, tex_size);
    let wb = bloom_scan(pos, -1.0, tex_size);
    let wc = bloom_scan(pos,  0.0, tex_size);
    let wd = bloom_scan(pos,  1.0, tex_size);
    let we = bloom_scan(pos,  2.0, tex_size);
    return a * wa + b * wb + c * wc + d * wd + e * we;
}

// Barrel distortion (screen warp).
fn warp(uv: vec2<f32>) -> vec2<f32> {
    var p = uv * 2.0 - 1.0;
    p *= vec2<f32>(1.0 + (p.y * p.y) * WARP_X, 1.0 + (p.x * p.x) * WARP_Y);
    return p * 0.5 + 0.5;
}

// Shadow / aperture-grille mask.
fn mask(pos: vec2<f32>) -> vec3<f32> {
    var m = vec3<f32>(MASK_DARK);

    if SHADOW_MASK == 1 {
        // Compressed TV shadow mask.
        var odd = 0.0;
        if fract(pos.x * 0.16666666) < 0.5 { odd = 1.0; }
        var line = MASK_LIGHT;
        if fract((pos.y + odd) * 0.5) < 0.5 { line = MASK_DARK; }
        let fx = fract(pos.x * 0.33333333);
        if      fx < 0.333 { m.r = MASK_LIGHT; }
        else if fx < 0.666 { m.g = MASK_LIGHT; }
        else               { m.b = MASK_LIGHT; }
        m *= line;
    } else if SHADOW_MASK == 2 {
        // Aperture-grille.
        let fx = fract(pos.x * 0.33333333);
        if      fx < 0.333 { m.r = MASK_LIGHT; }
        else if fx < 0.666 { m.g = MASK_LIGHT; }
        else               { m.b = MASK_LIGHT; }
    } else if SHADOW_MASK == 3 {
        // Stretched VGA.
        let fx = fract((pos.x + pos.y * 3.0) * 0.16666666);
        if      fx < 0.333 { m.r = MASK_LIGHT; }
        else if fx < 0.666 { m.g = MASK_LIGHT; }
        else               { m.b = MASK_LIGHT; }
    } else if SHADOW_MASK == 4 {
        // VGA.
        let p2 = vec2<f32>(floor(pos.x), floor(pos.y * 0.5));
        let fx = fract((p2.x + p2.y * 3.0) * 0.16666666);
        if      fx < 0.333 { m.r = MASK_LIGHT; }
        else if fx < 0.666 { m.g = MASK_LIGHT; }
        else               { m.b = MASK_LIGHT; }
    }

    return m;
}

// -- Vertex -------------------------------------------------------------------

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VertexOutput {
    let x  = f32(i32(vi == 1u)) * 4.0 - 1.0;
    let y  = f32(i32(vi == 2u)) * 4.0 - 1.0;
    let pu = f32(i32(vi == 1u)) * 2.0;
    let pv = 1.0 - f32(i32(vi == 2u)) * 2.0;
    return VertexOutput(vec4<f32>(x, y, 0.0, 1.0), vec2<f32>(pu, pv));
}

// -- Fragment -----------------------------------------------------------------

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let tex_size = vec2<f32>(textureDimensions(t_frame));

    let pos = warp(in.uv);

    // Hard black outside warped screen boundary.
    if pos.x < 0.0 || pos.x > 1.0 || pos.y < 0.0 || pos.y > 1.0 {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }

    var col = tri(pos, tex_size);
    col += bloom(pos, tex_size) * BLOOM_AMOUNT;

    if SHADOW_MASK > 0 {
        col *= mask(in.position.xy);
    }

    return vec4<f32>(to_srgb(col), 1.0);
}
