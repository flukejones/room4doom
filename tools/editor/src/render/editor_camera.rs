//! Editor view state: the 3D camera + ease goal, gesture commands, and derived mode.

use super::camera3d::{Camera, Projection};
use super::view::WorldRect;
use crate::state::Damage;

/// Degrees of orbit per screen pixel of shift-drag.
const ORBIT_DEG_PER_PX: f32 = 0.4;
/// Ortho zoom clamp (world-units of view height).
const MIN_ORTHO_H: f32 = 16.0;
const MAX_ORTHO_H: f32 = 262144.0;
/// Per-tic easing fraction toward the camera goal.
const CAM_EASE: f32 = 0.25;
/// Orbit angle within this many degrees of the goal counts as settled.
const CAM_EASE_EPS_DEG: f32 = 0.1;
/// Margin left around the map when fitting it to the viewport.
const FIT_MARGIN: f32 = 1.1;

/// View mode derived from the camera orientation and projection.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CameraMode {
    TopDown,
    Ortho3d,
    Perspective3d,
}

/// Return value of a camera command; maps to [`Damage`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CameraChange {
    None,
    /// View moved (pan, zoom, orbit, projection flip) — geometry unchanged.
    View,
}

impl From<CameraChange> for Damage {
    fn from(change: CameraChange) -> Self {
        match change {
            CameraChange::None => Self::None,
            CameraChange::View => Self::View,
        }
    }
}

/// Editor view state. `cam` renders and picks; eased toward `goal` for programmatic moves.
pub struct EditorCamera {
    cam: Camera,
    goal: Camera,
    viewport: [f32; 2],
    /// Height of the editing/grid plane that `screen_to_world` hits, set from the
    /// last contextual pick (a clicked floor/edge). 0 = the floor grid.
    grid_z: f32,
    /// Sticky orbit centre — the part/selection the world spins about.
    pivot: [f32; 3],
}

impl Default for EditorCamera {
    fn default() -> Self {
        Self {
            cam: Camera::default(),
            goal: Camera::default(),
            viewport: [1024.0, 768.0],
            grid_z: 0.0,
            pivot: [0.0, 0.0, 0.0],
        }
    }
}

impl EditorCamera {
    /// Snap to a fresh top-down ortho view, keeping only the viewport (map load).
    pub fn reset(&mut self) {
        *self = Self {
            viewport: self.viewport,
            ..Self::default()
        };
    }

    /// Viewport aspect (width / height).
    pub fn aspect(&self) -> f32 {
        self.viewport[0] / self.viewport[1].max(1.0)
    }

    /// Pixels per world unit (ortho: viewport height / view height).
    pub fn zoom_level(&self) -> f32 {
        self.viewport[1] / self.cam.ortho_height().max(1e-3)
    }

    pub fn viewport(&self) -> [f32; 2] {
        self.viewport
    }

    /// The contextual editing-plane height (`screen_to_world` hits this in 3D).
    pub fn grid_z(&self) -> f32 {
        self.grid_z
    }

    /// Set the editing/grid-plane height (from a contextual pick).
    pub fn set_grid_z(&mut self, z: f32) {
        self.grid_z = z;
    }

    /// The active 3D projection (for the toolbar toggle state).
    pub fn projection(&self) -> Projection {
        self.cam.projection()
    }

    pub fn set_viewport(&mut self, w: f32, h: f32) {
        self.viewport = [w, h];
    }

    fn ndc(&self, pos: [f32; 2]) -> [f32; 2] {
        let [w, h] = self.viewport;
        [2.0 * pos[0] / w - 1.0, 1.0 - 2.0 * pos[1] / h]
    }

    /// The camera canvas and picking both use — single source of truth.
    pub fn render_camera(&self) -> Camera {
        self.cam
    }

    /// Screen pixel → world XY on the grid plane via 3D ray-cast.
    pub fn screen_to_world(&self, pos: [f32; 2]) -> [f32; 2] {
        self.cam
            .ground_hit_at(self.ndc(pos), self.aspect(), self.grid_z)
            .unwrap_or_else(|| {
                let e = self.cam.eye();
                [e[0], e[1]]
            })
    }

    /// World point under `pos` at the grid plane height (3D ray-cast).
    fn world_at(&self, pos: [f32; 2]) -> [f32; 3] {
        let [x, y] = self.screen_to_world(pos);
        [x, y, self.grid_z]
    }

    /// Pan in the camera's view plane: drag left/up moves content left/up.
    pub fn pan(&mut self, dx: f32, dy: f32) -> CameraChange {
        let per_px = self.cam.ortho_height() / self.viewport[1].max(1.0);
        let right = self.cam.billboard_right();
        let up = self.cam.billboard_up();
        let e = self.cam.eye();
        self.cam.set_eye([
            e[0] + (-right[0] * dx + up[0] * dy) * per_px,
            e[1] + (-right[1] * dx + up[1] * dy) * per_px,
            e[2] + (-right[2] * dx + up[2] * dy) * per_px,
        ]);
        self.goal = self.cam;
        CameraChange::View
    }

    /// Zoom by `factor`, keeping the point under `at` fixed on screen.
    pub fn zoom(&mut self, factor: f32, at: [f32; 2]) -> CameraChange {
        let anchor = self.world_at(at);
        match self.cam.projection() {
            Projection::Ortho => {
                let h = (self.cam.ortho_height() / factor).clamp(MIN_ORTHO_H, MAX_ORTHO_H);
                self.cam.set_ortho_height(h);
            }
            Projection::Perspective => self.cam.dolly_to(anchor, factor),
        }
        let after = self.world_at(at);
        let e = self.cam.eye();
        self.cam.set_eye([
            e[0] + (anchor[0] - after[0]),
            e[1] + (anchor[1] - after[1]),
            e[2] + (anchor[2] - after[2]),
        ]);
        self.goal = self.cam;
        CameraChange::View
    }

    pub fn zoom_at_center(&mut self, factor: f32) -> CameraChange {
        let centre = [self.viewport[0] / 2.0, self.viewport[1] / 2.0];
        self.zoom(factor, centre)
    }

    /// Jump to an absolute zoom (pixels per world unit), anchored at the centre.
    pub fn zoom_to(&mut self, zoom: f32) -> CameraChange {
        let factor = zoom / self.zoom_level();
        self.zoom_at_center(factor)
    }

    /// Set the orbit centre (the part/selection the world spins about).
    pub fn set_pivot(&mut self, point: [f32; 3]) {
        self.pivot = point;
    }

    /// Orbit about the sticky pivot. Instant (no easing).
    pub fn orbit(&mut self, dx: f32, dy: f32) -> CameraChange {
        self.cam
            .orbit(self.pivot, -dx * ORBIT_DEG_PER_PX, dy * ORBIT_DEG_PER_PX);
        self.goal = self.cam;
        CameraChange::View
    }

    /// View mode derived from camera orientation + projection.
    pub fn mode(&self) -> CameraMode {
        match (self.cam.is_tilted(), self.cam.projection()) {
            (false, Projection::Ortho) => CameraMode::TopDown,
            (_, Projection::Ortho) => CameraMode::Ortho3d,
            (_, Projection::Perspective) => CameraMode::Perspective3d,
        }
    }

    /// Ease to a straight-down ortho view over `centre` (keep zoom).
    pub fn top_down_to(&mut self, centre: [f32; 3]) {
        self.goal.look_down_at(centre);
        self.goal.set_projection(Projection::Ortho);
        self.pivot = centre;
    }

    pub fn set_projection(&mut self, projection: Projection) -> CameraChange {
        if self.cam.projection() == projection {
            return CameraChange::None;
        }
        self.cam.set_projection(projection);
        self.goal.set_projection(projection);
        CameraChange::View
    }

    /// Ease the camera one tic toward its goal; true while more is needed.
    pub fn ease_tic(&mut self) -> bool {
        self.cam.ease_to(&self.goal, CAM_EASE, CAM_EASE_EPS_DEG)
    }

    /// Jump to the goal instantly (map load; no animated swing).
    pub fn settle(&mut self) {
        self.cam = self.goal;
    }

    pub fn needs_ease(&self) -> bool {
        self.cam.ease_pending(&self.goal, CAM_EASE_EPS_DEG)
    }

    /// Centre on a world rectangle, looking straight down (eased, keep zoom).
    pub fn center_on(&mut self, bounds: WorldRect) {
        let cx = bounds.min_x.midpoint(bounds.max_x);
        let cy = bounds.min_y.midpoint(bounds.max_y);
        self.goal.look_down_at([cx, cy, self.grid_z]);
        self.pivot = [cx, cy, self.grid_z];
    }

    /// Fit a world rectangle to the viewport with [`FIT_MARGIN`] padding, centred.
    pub fn fit(&mut self, bounds: WorldRect) {
        let bw = (bounds.max_x - bounds.min_x).max(1.0);
        let bh = (bounds.max_y - bounds.min_y).max(1.0);
        let h = (bh.max(bw / self.aspect()) * FIT_MARGIN).clamp(MIN_ORTHO_H, MAX_ORTHO_H);
        self.goal.set_ortho_height(h);
        self.center_on(bounds);
    }
}

#[cfg(test)]
impl EditorCamera {
    /// World point → screen pixels. Behind camera → clamped off-screen. Test-only.
    pub fn world_to_screen(&self, p: [f32; 2]) -> [f32; 2] {
        let [w, h] = self.viewport;
        match self
            .cam
            .world_to_ndc([p[0], p[1], self.grid_z], self.aspect())
        {
            Some(n) => [(n[0] * 0.5 + 0.5) * w, (0.5 - n[1] * 0.5) * h],
            None => [-1e6, -1e6],
        }
    }

    /// Set ortho zoom (pixels per world unit) directly. Test-only.
    pub fn set_zoom(&mut self, zoom: f32) {
        let h = (self.viewport[1] / zoom.max(1e-3)).clamp(MIN_ORTHO_H, MAX_ORTHO_H);
        self.cam.set_ortho_height(h);
        self.goal.set_ortho_height(h);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orbit_enters_3d_from_top_down() {
        let mut c = EditorCamera::default();
        assert_eq!(c.mode(), CameraMode::TopDown);
        assert_eq!(c.orbit(20.0, 15.0), CameraChange::View);
        assert_eq!(c.mode(), CameraMode::Ortho3d);
        assert_eq!(c.orbit(5.0, 5.0), CameraChange::View);
    }

    #[test]
    fn toggle_off_returns_to_top_down() {
        let mut c = EditorCamera::default();
        c.orbit(40.0, 30.0);
        assert!(c.render_camera().is_tilted(), "orbit tilted the view");
        c.top_down_to([0.0, 0.0, 0.0]);
        while c.ease_tic() {}
        assert!(
            !c.render_camera().is_tilted(),
            "eases back to a straight-down plan view"
        );
    }

    #[test]
    fn reset_snaps_to_top_down() {
        let mut c = EditorCamera::default();
        c.set_viewport(800.0, 600.0);
        c.orbit(40.0, 30.0);
        c.set_projection(Projection::Perspective);
        c.reset();
        assert_eq!(c.viewport(), [800.0, 600.0], "viewport survives reset");
    }

    #[test]
    fn ortho_zoom_anchors_cursor() {
        let mut c = EditorCamera::default();
        c.set_viewport(800.0, 600.0);
        let at = [200.0, 150.0];
        let before = c.screen_to_world(at);
        c.zoom(2.0, at);
        let after = c.screen_to_world(at);
        assert!(
            (before[0] - after[0]).abs() < 1.0,
            "x anchored: {before:?} {after:?}"
        );
        assert!(
            (before[1] - after[1]).abs() < 1.0,
            "y anchored: {before:?} {after:?}"
        );
    }

    /// Regression: too-loose singularity cutoff in `invert` froze the anchor at deep zoom-out.
    #[test]
    fn ortho_zoom_anchors_when_far_out() {
        let mut c = EditorCamera::default();
        c.set_viewport(1024.0, 768.0);
        let at = [800.0, 200.0];
        // Zoom out well past the failure point (zoom_level ~0.15).
        for _ in 0..40 {
            c.zoom(1.0 / 1.1, at);
        }
        assert!(c.zoom_level() < 0.05, "deep zoom-out reached");
        let before = c.screen_to_world(at);
        c.zoom(1.0 / 1.1, at);
        let after = c.screen_to_world(at);
        assert!(
            (before[0] - after[0]).abs() < 5.0 && (before[1] - after[1]).abs() < 5.0,
            "anchor holds far out: {before:?} {after:?}"
        );
    }

    #[test]
    fn fit_centres_the_rect() {
        let mut c = EditorCamera::default();
        c.set_viewport(800.0, 600.0);
        let rect = WorldRect {
            min_x: 100.0,
            min_y: -400.0,
            max_x: 900.0,
            max_y: 200.0,
        };
        c.fit(rect);
        c.settle();
        let centre = c.screen_to_world([400.0, 300.0]);
        assert!((centre[0] - 500.0).abs() < 1.0, "x centred: {centre:?}");
        assert!((centre[1] + 100.0).abs() < 1.0, "y centred: {centre:?}");
    }
}
