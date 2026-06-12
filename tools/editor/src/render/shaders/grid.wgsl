// Procedural editing-plane grid. A full-screen triangle; each pixel is
// unprojected through `inv_view_proj` onto the z=`grid_z` plane, then the snap
// grid and the 64u alignment grid are drawn from the world XY with a constant
// device-pixel line width (fwidth AA). Replaces the CPU-built line instances:
// no geometry, no per-frame rebuild, immune to grazing-ray coordinate blowup.

struct Grid {
    inv_view_proj: mat4x4<f32>,
    view_proj: mat4x4<f32>,
    // x,y = viewport px; z = grid_z plane height; w = 1 on-top else 0.
    plane: vec4<f32>,
    // x = snap spacing, y = tile spacing (64), z = min device-px before a grid
    //   fades out (zoom-out cutoff), w = line half-width in device px.
    params: vec4<f32>,
    grid_rgba: vec4<f32>,
    tile_rgba: vec4<f32>,
    // x = graze-fade start |dir.z|, y = graze-fade full |dir.z|,
    // z = far-fade start dist, w = far-fade end dist (ground units from eye).
    fade: vec4<f32>,
    // Camera eye world position (xyz); the far fade measures ground distance from xy.
    eye: vec4<f32>,
};
@group(0) @binding(0) var<uniform> g: Grid;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

// Oversized clip-space triangle covering the whole viewport.
@vertex
fn vs_grid(@builtin(vertex_index) vi: u32) -> VsOut {
    var c = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    var o: VsOut;
    o.pos = vec4<f32>(c[vi], 0.0, 1.0);
    o.ndc = c[vi];
    return o;
}

// Unproject NDC at depth `z_ndc` (0=near,1=far) to a world point.
fn unproject(ndc: vec2<f32>, z_ndc: f32) -> vec3<f32> {
    let h = g.inv_view_proj * vec4<f32>(ndc, z_ndc, 1.0);
    return h.xyz / h.w;
}

// Coverage of one grid: 1 on a line, 0 between. `spacing` in world units,
// `wpp` = world units per device pixel (from screen-space derivatives).
fn grid_coverage(world: vec2<f32>, spacing: f32, wpp: vec2<f32>) -> f32 {
    // Distance to the nearest line on each axis, in world units.
    let to_line = abs(world / spacing - round(world / spacing)) * spacing;
    // Convert to device px and take the closer of the two axes.
    let px = min(to_line.x / max(wpp.x, 1e-12), to_line.y / max(wpp.y, 1e-12));
    let half = g.params.w;
    return 1.0 - smoothstep(half - 0.5, half + 0.5, px);
}

struct FsOut {
    @location(0) rgba: vec4<f32>,
    @builtin(frag_depth) depth: f32,
};

@fragment
fn fs_grid(in: VsOut) -> FsOut {
    var out: FsOut;
    // Two points along the pixel's view ray → world ray.
    let near = unproject(in.ndc, 0.0);
    let far = unproject(in.ndc, 1.0);
    let dir = far - near;
    let plane_z = g.plane.z;
    // Parallel to the plane → nothing to draw.
    if (abs(dir.z) < 1e-7) { discard; }
    let t = (plane_z - near.z) / dir.z;
    // Hit must be in front of the near plane.
    if (t < 0.0) { discard; }
    let world = near + dir * t;

    // World units per device pixel from the on-plane world derivatives.
    let wpp = max(fwidth(world.xy), vec2<f32>(1e-12));

    // Per-grid zoom-out cutoff: fade a grid as its on-screen spacing shrinks
    // below `min_px` device px (mirrors the old `spacing*zoom < MIN` skip).
    let min_px = g.params.z;
    let wppm = max(wpp.x, wpp.y);
    let snap_fade = smoothstep(min_px * 0.5, min_px, g.params.x / wppm);
    let tile_fade = smoothstep(min_px * 0.5, min_px, g.params.y / wppm);

    let tile_c = grid_coverage(world.xy, g.params.y, wpp) * g.tile_rgba.a * tile_fade;
    let snap_c = grid_coverage(world.xy, g.params.x, wpp) * g.grid_rgba.a * snap_fade;

    // Composite tile under the finer snap grid.
    var col = g.tile_rgba.rgb;
    var a = tile_c;
    a = snap_c + a * (1.0 - snap_c);
    col = mix(col, g.grid_rgba.rgb, snap_c / max(a, 1e-6));

    // Horizon fade: grazing rays (|dir.z| small) and far hits fade to nothing,
    // so the near-edge-on plane never moirés. Distance is measured on the ground
    // from the camera eye's XY — in top-down ortho every visible cell is near the
    // eye (no fade); only a tilted view reaches far enough to fade.
    let ndir = normalize(dir);
    let graze = smoothstep(g.fade.x, g.fade.y, abs(ndir.z));
    let ground_dist = length(world.xy - g.eye.xy);
    let farf = 1.0 - smoothstep(g.fade.z, g.fade.w, ground_dist);
    a = a * graze * farf;
    if (a <= 0.0) { discard; }

    out.rgba = vec4<f32>(col, a);
    // Real plane depth so the grid depth-tests against the 3D surface. When
    // on-top, the pipeline's depth_compare=Always ignores this.
    let clip = g.view_proj * vec4<f32>(world, 1.0);
    out.depth = clip.z / clip.w;
    return out;
}
