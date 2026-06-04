// Voxel pass: instanced exposed faces. One instanced quad per exposed voxel
// face; the vertex shader expands the face's unit quad at its grid position,
// applies the model pivot + Z-flip + per-instance yaw (replicating
// software3d/src/voxel/collect.rs), and projects eye-at-origin (subtract
// camera_pos) like the sprite pass. The fragment applies the same Doom
// diminishing-light row as the scene/sprite passes to the face's pre-resolved
// colour. A fuzz variant RGB-halves the background for spectre things.
//
// Reuses SpriteCam (group 0) and LightParams (group 3) byte-for-byte from the
// sprite pass so the camera/light plumbing is shared.

struct SpriteCam {
    view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,  // xyz eye, w unused
    cam_right: vec4<f32>,   // unused here (kept for layout parity with sprites)
    cam_up: vec4<f32>,      // unused here
    extralight: f32,
    pad0: f32,
    pad1: f32,
    pad2: f32,
};

// One exposed face's static geometry/colour (baked once per model). Flat scalar
// fields (no vec3) so the Rust #[repr(C)] layout is unambiguous; 32 bytes.
struct VoxelFace {
    px: u32, py: u32, pz: u32, // voxel grid coord
    axis: u32,                 // 0=X 1=Y 2=Z (face normal axis)
    rgba: u32,                 // 0xAARRGGBB, pre-resolved base palette
    sign: i32,                 // -1 / +1 (side along axis)
    pad0: u32, pad1: u32,
};

// One visible voxel-thing's per-frame transform. Flat scalars; 48 bytes.
struct VoxelInstance {
    wx: f32, wy: f32, wz: f32, // base world position (bob folded into wz)
    cos_a: f32,
    sin_a: f32,
    pvx: f32, pvy: f32, pvz: f32, // xpivot, ypivot, zpivot
    brightness: u32,           // 0..15 light band
    flags: u32,                // bit0 fuzz/shadow (pipeline variant)
    pad0: u32, pad1: u32,
};

struct LightParams {
    light_levels: f32,
    max_row: f32,
    light_gamma: f32,
    dist_scale: f32,
    dist_rows_max: f32,
};

// 6 verts per face quad (two triangles); must match VERTS_PER_FACE in voxel.rs.
const VERTS_PER_FACE: u32 = 6u;

@group(0) @binding(0) var<uniform> cam: SpriteCam;
@group(1) @binding(0) var<storage, read> faces: array<VoxelFace>;
@group(2) @binding(0) var<storage, read> instances: array<VoxelInstance>;
@group(3) @binding(0) var<uniform> light: LightParams;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) @interpolate(flat) rgba: u32,
    @location(1) @interpolate(flat) start_row: f32,
    @location(2) view_dist: f32,
};

// Sector baseline colourmap row from the brightness band (mirrors the sprite
// pass: (light_levels - (band+extralight)) * 4).
fn voxel_start_row(brightness: u32) -> f32 {
    let band = min(f32(brightness) + cam.extralight, light.light_levels);
    return (light.light_levels - band) * 4.0;
}

// Quad corner (a,b) in {0,1}^2 for the two non-axis grid dimensions, 6 verts.
fn face_corner(vertex: u32) -> vec2<f32> {
    switch vertex {
        case 0u: { return vec2<f32>(0.0, 0.0); }
        case 1u: { return vec2<f32>(1.0, 0.0); }
        case 2u: { return vec2<f32>(1.0, 1.0); }
        case 3u: { return vec2<f32>(0.0, 0.0); }
        case 4u: { return vec2<f32>(1.0, 1.0); }
        default: { return vec2<f32>(0.0, 1.0); }
    }
}

@vertex
fn vs_main(@builtin(vertex_index) corner: u32, @builtin(instance_index) inst: u32) -> VsOut {
    let f = faces[corner / VERTS_PER_FACE];
    let s = instances[inst];
    let q = face_corner(corner % VERTS_PER_FACE);

    // Axis fixed at the voxel side (sign>0 -> +1 cell, else +0); the other two
    // dims span the unit quad via (q.x, q.y).
    let axis_step = select(0.0, 1.0, f.sign > 0);
    let base = vec3<f32>(f32(f.px), f32(f.py), f32(f.pz));
    var cell: vec3<f32>;
    switch f.axis {
        case 0u: { cell = base + vec3<f32>(axis_step, q.x, q.y); }      // X face: span Y,Z
        case 1u: { cell = base + vec3<f32>(q.x, axis_step, q.y); }      // Y face: span X,Z
        default: { cell = base + vec3<f32>(q.x, q.y, axis_step); }      // Z face: span X,Y
    }

    // collect.rs world transform: pivot offset, Z flipped, XY rotated by yaw.
    let ox = cell.x - s.pvx;
    let oy = cell.y - s.pvy;
    let oz = -(cell.z - s.pvz);
    let rx = ox * s.cos_a - oy * s.sin_a;
    let ry = ox * s.sin_a + oy * s.cos_a;
    let world = vec3<f32>(s.wx + rx, s.wy + ry, s.wz + oz);

    var out: VsOut;
    out.pos = cam.view_proj * vec4<f32>(world - cam.camera_pos.xyz, 1.0);
    out.rgba = f.rgba;
    out.start_row = voxel_start_row(s.brightness);
    out.view_dist = out.pos.w;
    return out;
}

// 0xAARRGGBB -> linear-ish vec3 (same byte order assets.rs resolves to).
fn unpack_rgb(argb: u32) -> vec3<f32> {
    let r = f32((argb >> 16u) & 0xFFu) / 255.0;
    let g = f32((argb >> 8u) & 0xFFu) / 255.0;
    let b = f32(argb & 0xFFu) / 255.0;
    return vec3<f32>(r, g, b);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Same smooth diminishing-light row as the scene/sprite passes.
    let near = clamp((1.0 / max(in.view_dist, 1.0)) * light.dist_scale, 0.0, light.dist_rows_max);
    let row = clamp(in.start_row - near * 0.5, 0.0, light.max_row);
    let intensity = pow(1.0 - row / light.max_row, light.light_gamma);
    return vec4<f32>(unpack_rgb(in.rgba) * intensity, 1.0);
}

// Spectre/shadow fuzz: RGB-halve the background (matches sprite.wgsl fs_fuzz).
@fragment
fn fs_fuzz(in: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(0.0, 0.0, 0.0, 0.5);
}
