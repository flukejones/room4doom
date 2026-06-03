//! World-ray surface picking for the 3D view.
//!
//! Builds a ray from the camera through the clicked pixel using the same basis
//! and projection as the software3d renderer, then brute-forces ray-vs-polygon
//! over the BSP3D polygons and returns the nearest hit's sector.

use glam::Vec3;
use level::{BSP3D, SurfaceKind};

use super::render3d::{Camera3D, FOV};

const NEAR: f32 = 1.0;

/// A picked surface: the sector its polygon belongs to, plus the linedef id if
/// it was a wall (so the caller can also consider the wall's other sector — a
/// door's moving sector is the one *behind* its visible wall).
pub struct PickHit {
    pub sector_id: usize,
    pub linedef_id: Option<usize>,
}

/// Cast a ray from `cam` through the cursor (in pixels relative to the viewport
/// of size `w`×`h`) and return the nearest polygon hit.
pub fn pick_sector(
    bsp3d: &BSP3D,
    cam: &Camera3D,
    cursor_x: f32,
    cursor_y: f32,
    w: usize,
    h: usize,
) -> Option<PickHit> {
    let (origin, dir) = ray_from_cursor(cam, cursor_x, cursor_y, w, h);
    let mut best: Option<(f32, usize, Option<usize>)> = None;
    for poly in bsp3d.polygons.iter() {
        if poly.vertices.len() < 3 {
            continue;
        }
        // No AABB pre-reject: `poly.aabb` is not refreshed by `move_surface`,
        // so a moved surface would be wrongly culled. Ray-vs-polygon directly.
        let verts: Vec<Vec3> = poly.vertices.iter().map(|&i| bsp3d.vertex_get(i)).collect();
        if let Some(t) = ray_hits_polygon(origin, dir, &verts, poly.normal) {
            if best.is_none_or(|(bt, ..)| t < bt) {
                let linedef_id = match &poly.surface_kind {
                    SurfaceKind::Vertical { linedef_id, .. } => Some(*linedef_id),
                    SurfaceKind::Horizontal { .. } => None,
                };
                best = Some((t, poly.sector_id, linedef_id));
            }
        }
    }
    best.map(|(_, sector_id, linedef_id)| PickHit {
        sector_id,
        linedef_id,
    })
}

/// Build the world-space ray for a cursor position. The basis matches
/// software3d's `update_view_matrix` (forward from yaw/pitch, world up = Z),
/// so the pick ray agrees with the rendered image.
fn ray_from_cursor(cam: &Camera3D, px: f32, py: f32, w: usize, h: usize) -> (Vec3, Vec3) {
    let forward = cam.forward();
    let right = forward.cross(Vec3::Z).normalize();
    let up = right.cross(forward).normalize();

    let (hfov, vfov, _) = render_common::og_projection(FOV, w as f32, h as f32);
    let tan_half_h = (hfov * 0.5).tan();
    let tan_half_v = (vfov * 0.5).tan();

    let ndc_x = 2.0 * px / w as f32 - 1.0;
    let ndc_y = 1.0 - 2.0 * py / h as f32;

    let dir = (forward + right * (ndc_x * tan_half_h) + up * (ndc_y * tan_half_v)).normalize();
    (cam.pos, dir)
}

/// Ray vs planar convex polygon. Double-sided (no facing rejection). Returns
/// the positive parameter `t` of the hit, or `None`.
fn ray_hits_polygon(origin: Vec3, dir: Vec3, verts: &[Vec3], normal: Vec3) -> Option<f32> {
    let denom = normal.dot(dir);
    if denom.abs() < 1e-6 {
        return None;
    }
    let t = normal.dot(verts[0] - origin) / denom;
    if t <= NEAR {
        return None;
    }
    let hit = origin + dir * t;
    // Consistent edge-cross sign => point is inside the convex polygon.
    let mut sign = 0i32;
    let n = verts.len();
    for i in 0..n {
        let a = verts[i];
        let b = verts[(i + 1) % n];
        let c = (b - a).cross(hit - a).dot(normal);
        let s = if c > 1e-4 {
            1
        } else if c < -1e-4 {
            -1
        } else {
            0
        };
        if s != 0 {
            if sign == 0 {
                sign = s;
            } else if s != sign {
                return None;
            }
        }
    }
    Some(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad() -> ([Vec3; 4], Vec3) {
        // Unit quad in the plane x = 10, facing -X.
        let v = [
            Vec3::new(10.0, -1.0, -1.0),
            Vec3::new(10.0, 1.0, -1.0),
            Vec3::new(10.0, 1.0, 1.0),
            Vec3::new(10.0, -1.0, 1.0),
        ];
        (v, Vec3::new(-1.0, 0.0, 0.0))
    }

    #[test]
    fn ray_hits_front() {
        let (v, n) = quad();
        let t = ray_hits_polygon(Vec3::ZERO, Vec3::X, &v, n);
        assert!(t.is_some());
        assert!((t.unwrap() - 10.0).abs() < 1e-3);
    }

    #[test]
    fn ray_misses() {
        let (v, n) = quad();
        let dir = Vec3::new(1.0, 5.0, 0.0).normalize();
        assert!(ray_hits_polygon(Vec3::ZERO, dir, &v, n).is_none());
    }

    #[test]
    fn ray_hits_back_face() {
        // Ray from behind the quad (+X side) travelling -X still hits it
        // (double-sided picking).
        let (v, n) = quad();
        let t = ray_hits_polygon(Vec3::new(20.0, 0.0, 0.0), -Vec3::X, &v, n);
        assert!(t.is_some());
        assert!((t.unwrap() - 10.0).abs() < 1e-3);
    }
}
