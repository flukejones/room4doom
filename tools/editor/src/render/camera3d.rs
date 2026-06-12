//! The canvas camera: eye + orthonormal basis (`right`/`up`/`fwd`), not Euler angles.
//! Orbit rotates the eye and basis rigidly about the 3D pivot so that point stays fixed.
//! Up-locked turntable: yaw about world +Z, pitch about camera right — never rolls.
//! `Mat4` is column-major (`m[col][row]`), matching WGSL; view is eye-at-origin.

use std::f32::consts::PI;

/// Column-major 4×4 (`m[col][row]`), WGSL-compatible.
pub type Mat4 = [[f32; 4]; 4];

/// Column-major identity.
const IDENTITY: Mat4 = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

/// Default perspective vertical field of view.
const DEFAULT_FOV_Y_DEG: f32 = 60.0;
/// Perspective near/far planes (world units).
const NEAR: f32 = 1.0;
const FAR: f32 = 131072.0;
/// Default eye distance / ortho view height (world units).
const DEFAULT_DIST: f32 = 2048.0;
/// Tilt threshold: `fwd[2] > -cos(5°)` → more than ~5° off straight-down.
const TILT_COS: f32 = 0.9962;

/// Parallel or perspective projection.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Projection {
    #[default]
    Ortho,
    Perspective,
}

/// Eye + orthonormal basis (`right`/`up`/`fwd`). Up-locked; never rolls.
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    eye: [f32; 3],
    right: [f32; 3],
    up: [f32; 3],
    fwd: [f32; 3],
    /// World-unit height the viewport spans in ortho (the ortho zoom level).
    ortho_height: f32,
    fov_y_deg: f32,
    projection: Projection,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            eye: [0.0, 0.0, DEFAULT_DIST],
            right: [1.0, 0.0, 0.0],
            up: [0.0, 1.0, 0.0],
            fwd: [0.0, 0.0, -1.0],
            ortho_height: DEFAULT_DIST,
            fov_y_deg: DEFAULT_FOV_Y_DEG,
            projection: Projection::Ortho,
        }
    }
}

impl Camera {
    pub fn eye(&self) -> [f32; 3] {
        self.eye
    }

    pub fn ortho_height(&self) -> f32 {
        self.ortho_height
    }

    pub fn projection(&self) -> Projection {
        self.projection
    }

    /// Reset to straight-down plan view over `point`. Zoom (`ortho_height`) unchanged.
    pub fn look_down_at(&mut self, point: [f32; 3]) {
        self.eye = [point[0], point[1], point[2] + DEFAULT_DIST];
        self.right = [1.0, 0.0, 0.0];
        self.up = [0.0, 1.0, 0.0];
        self.fwd = [0.0, 0.0, -1.0];
    }

    pub fn set_eye(&mut self, eye: [f32; 3]) {
        self.eye = eye;
    }

    pub fn set_ortho_height(&mut self, h: f32) {
        self.ortho_height = h;
    }

    pub fn set_projection(&mut self, projection: Projection) {
        self.projection = projection;
    }

    /// Orbit rigidly about `pivot`: yaw about world +Z, pitch about camera right.
    /// `pivot` keeps its screen position. Positive pitch lifts world height up.
    pub fn orbit(&mut self, pivot: [f32; 3], yaw_deg: f32, pitch_deg: f32) {
        self.rotate_rig([0.0, 0.0, 1.0], yaw_deg, pivot);
        self.rotate_rig(self.right, -pitch_deg, pivot);
    }

    /// Dolly eye toward `point` by `factor` (>1 closer, <1 further).
    pub fn dolly_to(&mut self, point: [f32; 3], factor: f32) {
        let inv = 1.0 / factor.max(1e-3);
        self.eye = [
            point[0] + (self.eye[0] - point[0]) * inv,
            point[1] + (self.eye[1] - point[1]) * inv,
            point[2] + (self.eye[2] - point[2]) * inv,
        ];
    }

    /// Rotate the whole rig (eye + basis) by `deg` about `axis` through `centre`.
    fn rotate_rig(&mut self, axis: [f32; 3], deg: f32, centre: [f32; 3]) {
        if deg.abs() < 1e-6 {
            return;
        }
        let rel = [
            self.eye[0] - centre[0],
            self.eye[1] - centre[1],
            self.eye[2] - centre[2],
        ];
        let r = rotate_about_axis(axis, deg, rel);
        self.eye = [centre[0] + r[0], centre[1] + r[1], centre[2] + r[2]];
        self.right = norm(rotate_about_axis(axis, deg, self.right));
        self.up = norm(rotate_about_axis(axis, deg, self.up));
        self.fwd = norm(rotate_about_axis(axis, deg, self.fwd));
    }

    /// Screen +X world axis.
    pub fn billboard_right(&self) -> [f32; 3] {
        self.right
    }

    /// Screen +Y world axis.
    pub fn billboard_up(&self) -> [f32; 3] {
        self.up
    }

    /// True when tilted enough that heights read and things draw as billboards.
    pub fn is_tilted(&self) -> bool {
        self.fwd[2] > -TILT_COS
    }

    /// World-space ray through NDC `ndc` (x,y in -1..1, +y up): `(origin, dir)`.
    pub fn ray(&self, ndc: [f32; 2], aspect: f32) -> Option<([f32; 3], [f32; 3])> {
        let inv = invert(self.view_proj(aspect))?;
        let near = unproject(&inv, [ndc[0], ndc[1], 0.0]);
        let far = unproject(&inv, [ndc[0], ndc[1], 1.0]);
        Some((near, [far[0] - near[0], far[1] - near[1], far[2] - near[2]]))
    }

    /// World XY where the ray through `ndc` meets the horizontal plane at height
    /// `z`. `None` when the ray is parallel to it.
    pub fn ground_hit_at(&self, ndc: [f32; 2], aspect: f32, z: f32) -> Option<[f32; 2]> {
        let (origin, dir) = self.ray(ndc, aspect)?;
        if dir[2].abs() < 1e-6 {
            return None;
        }
        let t = (z - origin[2]) / dir[2];
        Some([origin[0] + dir[0] * t, origin[1] + dir[1] * t])
    }

    /// World → NDC. `None` if behind the camera. Test-only (GPU projects in production).
    #[cfg(test)]
    pub fn world_to_ndc(&self, p: [f32; 3], aspect: f32) -> Option<[f32; 2]> {
        let m = self.view_proj(aspect);
        let v = [p[0], p[1], p[2], 1.0];
        let mut o = [0.0f32; 4];
        for r in 0..4 {
            o[r] = (0..4).map(|c| m[c][r] * v[c]).sum();
        }
        if o[3] <= 1e-6 {
            return None;
        }
        Some([o[0] / o[3], o[1] / o[3]])
    }

    /// Ease a fraction `t` toward `goal` (eye, zoom, basis). Returns true while more is needed.
    pub fn ease_to(&mut self, goal: &Self, t: f32, eps: f32) -> bool {
        self.eye = lerp3(self.eye, goal.eye, t);
        self.ortho_height = lerp(self.ortho_height, goal.ortho_height, t);
        self.right = norm(lerp3(self.right, goal.right, t));
        self.up = norm(lerp3(self.up, goal.up, t));
        self.fwd = norm(lerp3(self.fwd, goal.fwd, t));
        if !self.ease_pending(goal, eps) {
            *self = *goal;
            false
        } else {
            true
        }
    }

    /// True while meaningfully far from `goal` (`fwd` weighted ×1000 — it is a unit vector).
    pub fn ease_pending(&self, goal: &Self, eps: f32) -> bool {
        let d = (goal.eye[0] - self.eye[0])
            .abs()
            .max((goal.eye[1] - self.eye[1]).abs())
            .max((goal.eye[2] - self.eye[2]).abs())
            .max((goal.fwd[0] - self.fwd[0]).abs() * 1000.0)
            .max((goal.fwd[1] - self.fwd[1]).abs() * 1000.0)
            .max((goal.fwd[2] - self.fwd[2]).abs() * 1000.0);
        d > eps
    }

    /// World→clip. Eye-at-origin: rotate world by basis, subtract the eye.
    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let eye = self.eye;
        let view = mul(self.rot(), translate(-eye[0], -eye[1], -eye[2]));
        let proj = match self.projection {
            Projection::Ortho => ortho(self.ortho_height, aspect),
            Projection::Perspective => perspective(self.fov_y_deg, aspect, NEAR, FAR),
        };
        mul(proj, view)
    }

    /// Clip→world inverse of [`view_proj`]. Identity if the matrix is singular
    /// (degenerate camera) — callers tolerate it; the grid shader discards.
    pub fn inv_view_proj(&self, aspect: f32) -> Mat4 {
        invert(self.view_proj(aspect)).unwrap_or(IDENTITY)
    }

    /// Basis rotation: rows = right / up / −fwd (camera looks down −Z), column-major.
    fn rot(&self) -> Mat4 {
        let (r, u, f) = (self.right, self.up, self.fwd);
        [
            [r[0], u[0], -f[0], 0.0],
            [r[1], u[1], -f[1], 0.0],
            [r[2], u[2], -f[2], 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ]
    }
}

fn norm(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if l < 1e-9 {
        v
    } else {
        [v[0] / l, v[1] / l, v[2] / l]
    }
}

/// Ortho projection. Depth spans `[-FAR, +FAR]` symmetrically so geometry above
/// or below the eye is never clipped — ortho orders depth, not occludes it.
fn ortho(height: f32, aspect: f32) -> Mat4 {
    let h = height.max(1e-3);
    let w = h * aspect.max(1e-3);
    [
        [2.0 / w, 0.0, 0.0, 0.0],
        [0.0, 2.0 / h, 0.0, 0.0],
        [0.0, 0.0, -1.0 / FAR, 0.0],
        [0.0, 0.0, 0.5, 1.0],
    ]
}

fn perspective(fov_y_deg: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / (fov_y_deg * PI / 360.0).tan();
    [
        [f / aspect.max(1e-3), 0.0, 0.0, 0.0],
        [0.0, f, 0.0, 0.0],
        [0.0, 0.0, far / (near - far), -1.0],
        [0.0, 0.0, near * far / (near - far), 0.0],
    ]
}

/// Rodrigues rotation of `v` by `deg` about `axis`.
fn rotate_about_axis(axis: [f32; 3], deg: f32, v: [f32; 3]) -> [f32; 3] {
    let len = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
    if len < 1e-6 {
        return v;
    }
    let k = [axis[0] / len, axis[1] / len, axis[2] / len];
    let (s, c) = (deg * PI / 180.0).sin_cos();
    let kv = [
        k[1] * v[2] - k[2] * v[1],
        k[2] * v[0] - k[0] * v[2],
        k[0] * v[1] - k[1] * v[0],
    ];
    let kd = k[0] * v[0] + k[1] * v[1] + k[2] * v[2];
    [
        v[0] * c + kv[0] * s + k[0] * kd * (1.0 - c),
        v[1] * c + kv[1] * s + k[1] * kd * (1.0 - c),
        v[2] * c + kv[2] * s + k[2] * kd * (1.0 - c),
    ]
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        lerp(a[0], b[0], t),
        lerp(a[1], b[1], t),
        lerp(a[2], b[2], t),
    ]
}

fn translate(x: f32, y: f32, z: f32) -> Mat4 {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [x, y, z, 1.0],
    ]
}

fn mul(a: Mat4, b: Mat4) -> Mat4 {
    let mut out = [[0.0f32; 4]; 4];
    for c in 0..4 {
        for r in 0..4 {
            out[c][r] = (0..4).map(|k| a[k][r] * b[c][k]).sum();
        }
    }
    out
}

fn unproject(m: &Mat4, p: [f32; 3]) -> [f32; 3] {
    let v = [p[0], p[1], p[2], 1.0];
    let mut o = [0.0f32; 4];
    for r in 0..4 {
        o[r] = (0..4).map(|c| m[c][r] * v[c]).sum();
    }
    let w = if o[3].abs() < 1e-9 { 1.0 } else { o[3] };
    [o[0] / w, o[1] / w, o[2] / w]
}

/// Cofactor inverse of a column-major 4×4; `None` if singular.
fn invert(m: Mat4) -> Option<Mat4> {
    let a = [
        m[0][0], m[0][1], m[0][2], m[0][3], m[1][0], m[1][1], m[1][2], m[1][3], m[2][0], m[2][1],
        m[2][2], m[2][3], m[3][0], m[3][1], m[3][2], m[3][3],
    ];
    let mut inv = [0.0f32; 16];
    inv[0] = a[5] * a[10] * a[15] - a[5] * a[11] * a[14] - a[9] * a[6] * a[15]
        + a[9] * a[7] * a[14]
        + a[13] * a[6] * a[11]
        - a[13] * a[7] * a[10];
    inv[4] = -a[4] * a[10] * a[15] + a[4] * a[11] * a[14] + a[8] * a[6] * a[15]
        - a[8] * a[7] * a[14]
        - a[12] * a[6] * a[11]
        + a[12] * a[7] * a[10];
    inv[8] = a[4] * a[9] * a[15] - a[4] * a[11] * a[13] - a[8] * a[5] * a[15]
        + a[8] * a[7] * a[13]
        + a[12] * a[5] * a[11]
        - a[12] * a[7] * a[9];
    inv[12] = -a[4] * a[9] * a[14] + a[4] * a[10] * a[13] + a[8] * a[5] * a[14]
        - a[8] * a[6] * a[13]
        - a[12] * a[5] * a[10]
        + a[12] * a[6] * a[9];
    inv[1] = -a[1] * a[10] * a[15] + a[1] * a[11] * a[14] + a[9] * a[2] * a[15]
        - a[9] * a[3] * a[14]
        - a[13] * a[2] * a[11]
        + a[13] * a[3] * a[10];
    inv[5] = a[0] * a[10] * a[15] - a[0] * a[11] * a[14] - a[8] * a[2] * a[15]
        + a[8] * a[3] * a[14]
        + a[12] * a[2] * a[11]
        - a[12] * a[3] * a[10];
    inv[9] = -a[0] * a[9] * a[15] + a[0] * a[11] * a[13] + a[8] * a[1] * a[15]
        - a[8] * a[3] * a[13]
        - a[12] * a[1] * a[11]
        + a[12] * a[3] * a[9];
    inv[13] = a[0] * a[9] * a[14] - a[0] * a[10] * a[13] - a[8] * a[1] * a[14]
        + a[8] * a[2] * a[13]
        + a[12] * a[1] * a[10]
        - a[12] * a[2] * a[9];
    inv[2] = a[1] * a[6] * a[15] - a[1] * a[7] * a[14] - a[5] * a[2] * a[15]
        + a[5] * a[3] * a[14]
        + a[13] * a[2] * a[7]
        - a[13] * a[3] * a[6];
    inv[6] = -a[0] * a[6] * a[15] + a[0] * a[7] * a[14] + a[4] * a[2] * a[15]
        - a[4] * a[3] * a[14]
        - a[12] * a[2] * a[7]
        + a[12] * a[3] * a[6];
    inv[10] = a[0] * a[5] * a[15] - a[0] * a[7] * a[13] - a[4] * a[1] * a[15]
        + a[4] * a[3] * a[13]
        + a[12] * a[1] * a[7]
        - a[12] * a[3] * a[5];
    inv[14] = -a[0] * a[5] * a[14] + a[0] * a[6] * a[13] + a[4] * a[1] * a[14]
        - a[4] * a[2] * a[13]
        - a[12] * a[1] * a[6]
        + a[12] * a[2] * a[5];
    inv[3] = -a[1] * a[6] * a[11] + a[1] * a[7] * a[10] + a[5] * a[2] * a[11]
        - a[5] * a[3] * a[10]
        - a[9] * a[2] * a[7]
        + a[9] * a[3] * a[6];
    inv[7] = a[0] * a[6] * a[11] - a[0] * a[7] * a[10] - a[4] * a[2] * a[11]
        + a[4] * a[3] * a[10]
        + a[8] * a[2] * a[7]
        - a[8] * a[3] * a[6];
    inv[11] = -a[0] * a[5] * a[11] + a[0] * a[7] * a[9] + a[4] * a[1] * a[11]
        - a[4] * a[3] * a[9]
        - a[8] * a[1] * a[7]
        + a[8] * a[3] * a[5];
    inv[15] = a[0] * a[5] * a[10] - a[0] * a[6] * a[9] - a[4] * a[1] * a[10]
        + a[4] * a[2] * a[9]
        + a[8] * a[1] * a[6]
        - a[8] * a[2] * a[5];
    let det = a[0] * inv[0] + a[1] * inv[4] + a[2] * inv[8] + a[3] * inv[12];
    // Ortho determinant is tiny (1/FAR factor) but invertible; reject only det=0.
    if !det.is_finite() || det == 0.0 {
        return None;
    }
    let d = 1.0 / det;
    Some([
        [inv[0] * d, inv[1] * d, inv[2] * d, inv[3] * d],
        [inv[4] * d, inv[5] * d, inv[6] * d, inv[7] * d],
        [inv[8] * d, inv[9] * d, inv[10] * d, inv[11] * d],
        [inv[12] * d, inv[13] * d, inv[14] * d, inv[15] * d],
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    const ASPECT: f32 = 16.0 / 9.0;

    fn ndc(cam: &Camera, p: [f32; 3]) -> [f32; 2] {
        cam.world_to_ndc(p, ASPECT).expect("in front of camera")
    }

    #[test]
    fn plan_view_projects_xy_linearly() {
        let mut cam = Camera::default();
        cam.look_down_at([200.0, -100.0, 0.0]);
        let o = ndc(&cam, [200.0, -100.0, 0.0]);
        let px = ndc(&cam, [400.0, -100.0, 0.0]);
        let py = ndc(&cam, [200.0, 100.0, 0.0]);
        let raised = ndc(&cam, [200.0, -100.0, 256.0]);
        assert!(
            px[0] > o[0] && (px[1] - o[1]).abs() < 1e-4,
            "+X is screen right"
        );
        assert!(
            py[1] > o[1] && (py[0] - o[0]).abs() < 1e-4,
            "+Y is screen up"
        );
        assert!(
            (raised[0] - o[0]).abs() < 1e-4 && (raised[1] - o[1]).abs() < 1e-4,
            "straight down, height does not shift a point"
        );
    }

    /// Replicates the grid shader's unproject path. Regression: solid grid when `world` was constant → zero screen-space derivative.
    #[test]
    fn inv_view_proj_unprojects_distinct_ground_points() {
        let mut cam = Camera::default();
        cam.look_down_at([0.0, 0.0, 0.0]);
        let inv = cam.inv_view_proj(ASPECT);
        let ground = |ndc: [f32; 2]| -> [f32; 3] {
            let near = unproject(&inv, [ndc[0], ndc[1], 0.0]);
            let far = unproject(&inv, [ndc[0], ndc[1], 1.0]);
            let dir = [far[0] - near[0], far[1] - near[1], far[2] - near[2]];
            let t = -near[2] / dir[2];
            [
                near[0] + dir[0] * t,
                near[1] + dir[1] * t,
                near[2] + dir[2] * t,
            ]
        };
        let centre = ground([0.0, 0.0]);
        let right = ground([0.5, 0.0]);
        assert!(centre[2].abs() < 1e-2, "centre lands on z=0: {centre:?}");
        assert!(right[2].abs() < 1e-2, "right lands on z=0: {right:?}");
        assert!(
            (right[0] - centre[0]).abs() > 1.0,
            "distinct NDCs give distinct ground X: {centre:?} vs {right:?}"
        );
    }

    #[test]
    fn pivot_stays_fixed_on_screen_under_orbit() {
        for proj in [Projection::Ortho, Projection::Perspective] {
            let pivot = [512.0, -256.0, 64.0];
            let mut cam = Camera::default();
            cam.look_down_at([pivot[0], pivot[1], 0.0]);
            cam.set_projection(proj);
            let before = ndc(&cam, pivot);
            cam.orbit(pivot, 50.0, -35.0);
            let after = ndc(&cam, pivot);
            assert!(
                (after[0] - before[0]).abs() < 1e-2 && (after[1] - before[1]).abs() < 1e-2,
                "{proj:?}: pivot moved on screen under orbit: {before:?} -> {after:?}"
            );
        }
    }

    #[test]
    fn higher_z_reads_higher_when_tilted() {
        for proj in [Projection::Ortho, Projection::Perspective] {
            let mut cam = Camera::default();
            cam.look_down_at([0.0, 0.0, 0.0]);
            cam.set_projection(proj);
            cam.orbit([0.0, 0.0, 0.0], 0.0, -60.0);
            let floor = ndc(&cam, [0.0, 0.0, 0.0]);
            let ceil = ndc(&cam, [0.0, 0.0, 256.0]);
            assert!(ceil[1] > floor[1], "{proj:?}: ceiling reads above floor");
        }
    }

    #[test]
    fn ground_hit_round_trips() {
        let cases: &[(f32, f32, Projection)] = &[
            (0.0, 0.0, Projection::Ortho),
            (50.0, -40.0, Projection::Ortho),
            (50.0, -45.0, Projection::Perspective),
        ];
        for &(yaw, pitch, proj) in cases {
            let mut cam = Camera::default();
            cam.look_down_at([400.0, -250.0, 0.0]);
            cam.set_projection(proj);
            cam.orbit([400.0, -250.0, 0.0], yaw, pitch);
            let world = [420.0, -210.0];
            let n = ndc(&cam, [world[0], world[1], 0.0]);
            let hit = cam.ground_hit_at(n, ASPECT, 0.0).expect("hits the plane");
            // Perspective precision loss over NEAR..FAR; sub-2-unit error is within pick radius.
            let tol = if proj == Projection::Perspective {
                2.0
            } else {
                1.0
            };
            assert!(
                (hit[0] - world[0]).abs() < tol && (hit[1] - world[1]).abs() < tol,
                "{proj:?} y{yaw} p{pitch}: {hit:?} != {world:?}"
            );
        }
    }

    #[test]
    fn billboard_right_is_screen_horizontal() {
        for yaw in [0.0, 30.0, 90.0, 145.0, 220.0] {
            let mut cam = Camera::default();
            cam.look_down_at([0.0, 0.0, 0.0]);
            cam.orbit([0.0, 0.0, 0.0], yaw, -50.0);
            let r = cam.billboard_right();
            let o = ndc(&cam, [0.0, 0.0, 0.0]);
            let p = ndc(&cam, [r[0] * 100.0, r[1] * 100.0, r[2] * 100.0]);
            assert!(
                (p[0] - o[0]).abs() > 1e-3 && (p[1] - o[1]).abs() < 1e-2,
                "yaw {yaw}: right must be horizontal on screen ({}, {})",
                p[0] - o[0],
                p[1] - o[1]
            );
        }
    }
}
