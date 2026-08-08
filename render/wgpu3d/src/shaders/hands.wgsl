// Tracked hand skeletons: a camera-facing disc per joint and a ribbon per bone.
//
// Positions are submitted relative to the camera, matching every other world
// pass, because the view matrix is eye-at-origin.

struct Camera {
    view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    cam_right: vec4<f32>,
    cam_up: vec4<f32>,
    extralight: vec4<f32>,
};

// xyz position, w radius. A negative radius marks an untracked hand's slot.
struct Joints {
    joint: array<vec4<f32>, 52>,
};

@group(0) @binding(0) var<uniform> cam: Camera;
@group(1) @binding(0) var<uniform> hands: Joints;

const JOINTS_PER_HAND: u32 = 26u;
const BONES_PER_HAND: u32 = 24u;
// Parent of each joint from 2 upward; 0 is the palm and 1 the wrist root.
const PARENT: array<u32, 24> = array<u32, 24>(
    1u, 2u, 3u, 4u,           // thumb
    1u, 6u, 7u, 8u, 9u,       // index
    1u, 11u, 12u, 13u, 14u,   // middle
    1u, 16u, 17u, 18u, 19u,   // ring
    1u, 21u, 22u, 23u, 24u,   // little
);

const HAND_COLOUR: vec3<f32> = vec3<f32>(0.55, 0.78, 1.0);
const JOINT_ALPHA: f32 = 0.9;
const BONE_ALPHA: f32 = 0.75;
// Bones are drawn thinner than the joints they connect.
const BONE_WIDTH_SCALE: f32 = 0.45;
// Below this the bone direction and the eye ray are parallel and their cross
// product carries no direction.
const EDGE_ON: f32 = 1e-6;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) alpha: f32,
};

fn corner_of(vi: u32) -> vec2<f32> {
    return vec2<f32>(
        select(-1.0, 1.0, (vi & 1u) == 1u),
        select(-1.0, 1.0, (vi & 2u) == 2u),
    );
}

fn clip_of(world: vec3<f32>) -> vec4<f32> {
    return cam.view_proj * vec4<f32>(world - cam.camera_pos.xyz, 1.0);
}

@vertex
fn vs_joint(@builtin(vertex_index) vi: u32, @builtin(instance_index) id: u32) -> VsOut {
    let j = hands.joint[id];
    if j.w <= 0.0 {
        // Untracked: collapse to a point so the quad rasterises nothing.
        return VsOut(vec4<f32>(0.0, 0.0, 2.0, 1.0), vec2<f32>(0.0, 0.0), 0.0);
    }
    let corner = corner_of(vi);
    let world = j.xyz + (cam.cam_right.xyz * corner.x + cam.cam_up.xyz * corner.y) * j.w;
    return VsOut(clip_of(world), corner, JOINT_ALPHA);
}

@fragment
fn fs_joint(in: VsOut) -> @location(0) vec4<f32> {
    if length(in.local) > 1.0 {
        discard;
    }
    return vec4<f32>(HAND_COLOUR, in.alpha);
}

@vertex
fn vs_bone(@builtin(vertex_index) vi: u32, @builtin(instance_index) id: u32) -> VsOut {
    let hand = id / BONES_PER_HAND;
    let bone = id % BONES_PER_HAND;
    let child = hand * JOINTS_PER_HAND + bone + 2u;
    let parent = hand * JOINTS_PER_HAND + PARENT[bone];

    let a = hands.joint[parent];
    let b = hands.joint[child];
    if a.w <= 0.0 || b.w <= 0.0 {
        return VsOut(vec4<f32>(0.0, 0.0, 2.0, 1.0), vec2<f32>(0.0, 0.0), 0.0);
    }

    let along = b.xyz - a.xyz;
    let cross_eye = cross(along, a.xyz - cam.camera_pos.xyz);
    let across = select(normalize(cross_eye), cam.cam_right.xyz, length(cross_eye) < EDGE_ON);

    let corner = corner_of(vi);
    let centre = select(a.xyz, b.xyz, corner.y > 0.0);
    let width = select(a.w, b.w, corner.y > 0.0) * BONE_WIDTH_SCALE;
    return VsOut(clip_of(centre + across * corner.x * width), corner, BONE_ALPHA);
}

@fragment
fn fs_bone(in: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(HAND_COLOUR, in.alpha);
}
