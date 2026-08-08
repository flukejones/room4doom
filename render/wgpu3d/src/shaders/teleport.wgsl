// Teleport aim: a camera-facing ribbon along the ballistic curve, and a ring on
// the floor where the player would land. The curve is regenerated here rather
// than uploaded as points, so it is smooth at screen resolution.
//
// The view matrix is eye-at-origin, so every world position is submitted
// relative to the camera, matching the sprite and voxel passes.

struct Camera {
    view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    cam_right: vec4<f32>,
    cam_up: vec4<f32>,
    extralight: vec4<f32>,
};

struct Teleport {
    // xyz launch point, w flight seconds drawn.
    origin: vec4<f32>,
    // xyz unit launch direction, w launch speed.
    dir: vec4<f32>,
    // xyz landing point, w non-zero when the spot is legal.
    landing: vec4<f32>,
    // x gravity, y ribbon half-width, z ring radius.
    shape: vec4<f32>,
};

@group(0) @binding(0) var<uniform> cam: Camera;
@group(1) @binding(0) var<uniform> tp: Teleport;

const ARC_STEPS: u32 = 32u;
const VALID_COLOUR: vec3<f32> = vec3<f32>(0.25, 1.0, 0.35);
const INVALID_COLOUR: vec3<f32> = vec3<f32>(1.0, 0.15, 0.1);
const ARC_ALPHA: f32 = 0.55;
const ARC_TAIL_ALPHA: f32 = 0.2;
const RING_ALPHA: f32 = 0.5;
const RING_INNER: f32 = 0.72;
// Lift off the floor, map units — enough to clear depth quantisation.
const RING_LIFT: f32 = 1.0;
// Below this the curve tangent and the eye ray are parallel and their cross
// product carries no direction.
const EDGE_ON: f32 = 1e-4;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) along: f32,
    @location(1) local: vec2<f32>,
};

fn arc_at(t: f32) -> vec3<f32> {
    let fall = 0.5 * tp.shape.x * t * t;
    return tp.origin.xyz + tp.dir.xyz * tp.dir.w * t - vec3<f32>(0.0, 0.0, fall);
}

fn aim_colour() -> vec3<f32> {
    return select(INVALID_COLOUR, VALID_COLOUR, tp.landing.w > 0.5);
}

// The ribbon faces the eye: offset each pair of vertices along the curve's
// tangent crossed with the eye ray.
fn ribbon_offset(centre: vec3<f32>, tangent: vec3<f32>, side: f32) -> vec3<f32> {
    let across = cross(tangent, centre - cam.camera_pos.xyz);
    let facing = select(normalize(across), cam.cam_right.xyz, length(across) < EDGE_ON);
    return facing * side * tp.shape.y;
}

@vertex
fn vs_arc(@builtin(vertex_index) vi: u32) -> VsOut {
    let step = tp.origin.w / f32(ARC_STEPS);
    let along = f32(vi >> 1u) / f32(ARC_STEPS);
    let t = tp.origin.w * along;
    let centre = arc_at(t);
    let tangent = normalize(arc_at(t + step * 0.5) - centre);
    let side = select(-1.0, 1.0, (vi & 1u) == 1u);
    let world = centre + ribbon_offset(centre, tangent, side) - cam.camera_pos.xyz;
    return VsOut(cam.view_proj * vec4<f32>(world, 1.0), along, vec2<f32>(0.0, 0.0));
}

@fragment
fn fs_arc(in: VsOut) -> @location(0) vec4<f32> {
    // Faint at the hand, full at the landing, so the arc reads directionally.
    return vec4<f32>(aim_colour(), mix(ARC_TAIL_ALPHA, ARC_ALPHA, in.along));
}

@vertex
fn vs_ring(@builtin(vertex_index) vi: u32) -> VsOut {
    let corner = vec2<f32>(
        select(-1.0, 1.0, (vi & 1u) == 1u),
        select(-1.0, 1.0, (vi & 2u) == 2u),
    );
    let world =
        tp.landing.xyz + vec3<f32>(corner * tp.shape.z, RING_LIFT) - cam.camera_pos.xyz;
    return VsOut(cam.view_proj * vec4<f32>(world, 1.0), 1.0, corner);
}

@fragment
fn fs_ring(in: VsOut) -> @location(0) vec4<f32> {
    let radius = length(in.local);
    if radius > 1.0 {
        discard;
    }
    return vec4<f32>(aim_colour(), smoothstep(RING_INNER, 1.0, radius) * RING_ALPHA);
}
