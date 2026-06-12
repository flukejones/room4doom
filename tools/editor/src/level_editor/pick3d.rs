//! Ray-vs-mesh picking against the retained 3D surface mesh.

use editor_core::{LineKey, SectorKey, ThingKey, VertKey};

use crate::boundary::SelectMode;
use crate::render::frame3d::Vert3D;
use crate::state::SelItem;

/// Ray-triangle parallel/edge-on epsilon.
const RAY_EPS: f32 = 1e-6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PickKind {
    Linedef(LineKey),
    Sector(SectorKey),
    Vertex(VertKey),
    Thing(ThingKey),
}

impl PickKind {
    pub(crate) fn matches_mode(self, mode: SelectMode) -> bool {
        match mode {
            SelectMode::All => true,
            SelectMode::Vertex => matches!(self, Self::Vertex(_)),
            SelectMode::Line => matches!(self, Self::Linedef(_)),
            SelectMode::Sector => matches!(self, Self::Sector(_)),
            SelectMode::Thing => matches!(self, Self::Thing(_)),
        }
    }

    /// `None` for Sector (not draggable).
    pub(crate) fn as_item(self) -> Option<SelItem> {
        match self {
            Self::Vertex(k) => Some(SelItem::Vertex(k)),
            Self::Linedef(k) => Some(SelItem::Line(k)),
            Self::Thing(k) => Some(SelItem::Thing(k)),
            Self::Sector(_) => None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PickHit {
    pub kind: PickKind,
    pub world: [f32; 3],
    pub grid_z: f32,
}

/// Nearest mesh triangle hit.
pub(crate) struct MeshHit {
    pub tri: usize,
    pub world: [f32; 3],
    pub t: f32,
}

#[inline]
fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
#[inline]
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
#[inline]
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Möller–Trumbore, front-facing only (matches GPU CCW back-cull); `det > 0` ↔ camera-facing. Returns `t > 0` or `None`.
pub(crate) fn ray_hits_tri(
    origin: [f32; 3],
    dir: [f32; 3],
    a: [f32; 3],
    b: [f32; 3],
    c: [f32; 3],
) -> Option<f32> {
    let e1 = sub(b, a);
    let e2 = sub(c, a);
    let p = cross(dir, e2);
    let det = dot(e1, p);
    if det < RAY_EPS {
        return None;
    }
    let inv = 1.0 / det;
    let tvec = sub(origin, a);
    let u = dot(tvec, p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = cross(tvec, e1);
    let v = dot(dir, q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = dot(e2, q) * inv;
    (t > RAY_EPS).then_some(t)
}

pub(crate) fn pick_mesh(mesh: &[Vert3D], origin: [f32; 3], dir: [f32; 3]) -> Option<MeshHit> {
    let mut best: Option<MeshHit> = None;
    let mut i = 0;
    while i + 3 <= mesh.len() {
        let (a, b, c) = (mesh[i].pos, mesh[i + 1].pos, mesh[i + 2].pos);
        if let Some(t) = ray_hits_tri(origin, dir, a, b, c)
            && best.as_ref().is_none_or(|h| t < h.t)
        {
            best = Some(MeshHit {
                tri: i,
                world: [
                    origin[0] + dir[0] * t,
                    origin[1] + dir[1] * t,
                    origin[2] + dir[2] * t,
                ],
                t,
            });
        }
        i += 3;
    }
    best
}

/// Ray vs quad `[bl, br, tr, tl]`, double-sided. Returns nearest `t`.
pub(crate) fn ray_hits_quad(origin: [f32; 3], dir: [f32; 3], quad: [[f32; 3]; 4]) -> Option<f32> {
    let two_sided =
        |a, b, c| ray_hits_tri(origin, dir, a, b, c).or_else(|| ray_hits_tri(origin, dir, a, c, b));
    let t1 = two_sided(quad[0], quad[1], quad[2]);
    let t2 = two_sided(quad[0], quad[2], quad[3]);
    match (t1, t2) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// Squared distance from `p` to the ray and `t ≥ 0`.
pub(crate) fn point_ray_dist_sq(p: [f32; 3], origin: [f32; 3], dir: [f32; 3]) -> (f32, f32) {
    let dd = dot(dir, dir);
    let t = if dd < RAY_EPS {
        0.0
    } else {
        (dot(sub(p, origin), dir) / dd).max(0.0)
    };
    let foot = [
        origin[0] + dir[0] * t,
        origin[1] + dir[1] * t,
        origin[2] + dir[2] * t,
    ];
    let d = sub(p, foot);
    (dot(d, d), t)
}

/// Squared closest distance between segment `a`–`b` and the ray, and `t ≥ 0`; clamped segment–ray closest points (Ericson, Real-Time Collision Detection §5.1.9).
pub(crate) fn seg_ray_dist_sq(
    a: [f32; 3],
    b: [f32; 3],
    origin: [f32; 3],
    dir: [f32; 3],
) -> (f32, f32) {
    let u = sub(b, a);
    let w0 = sub(a, origin);
    let (uu, ud, dd) = (dot(u, u), dot(u, dir), dot(dir, dir));
    let (uw, dw) = (dot(u, w0), dot(dir, w0));
    let denom = uu * dd - ud * ud;
    let mut s = if denom > RAY_EPS {
        ((ud * dw - dd * uw) / denom).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let mut t = if dd > RAY_EPS {
        (ud * s + dw) / dd
    } else {
        0.0
    };
    if t < 0.0 {
        t = 0.0;
        s = if uu > RAY_EPS {
            (-uw / uu).clamp(0.0, 1.0)
        } else {
            0.0
        };
    }
    let pa = [a[0] + u[0] * s, a[1] + u[1] * s, a[2] + u[2] * s];
    let pr = [
        origin[0] + dir[0] * t,
        origin[1] + dir[1] * t,
        origin[2] + dir[2] * t,
    ];
    let d = sub(pa, pr);
    (dot(d, d), t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::frame3d::{SURFACE_CEIL, SURFACE_FLOOR, build_surface};
    use crate::render::triangulate::build_sector_tris;
    use editor_core::geom::sector_at;
    use std::collections::HashMap;

    /// Ray down from above: hits floor top face; ceiling is back-facing (culled).
    #[test]
    fn ray_down_from_above_hits_floor_through_culled_ceiling() {
        let wad = wad::WadData::new(&test_utils::doom1_wad_path());
        let map = editor_core::import_wad_map(&wad, "E1M1").expect("E1M1 imports");
        let tris = build_sector_tris(&map);
        let mesh = build_surface(&map, &tris, &HashMap::new()).0;

        let start = map
            .things
            .values()
            .find(|t| t.kind == 1)
            .expect("player start");
        let at = [start.x as f32, start.y as f32];
        let s = sector_at(&map, at).expect("start is inside a sector");
        let floor_h = map.sectors[s].floor_height as f32;
        let origin = [at[0], at[1], 1.0e5];
        let hit = pick_mesh(&mesh, origin, [0.0, 0.0, -1.0]).expect("ray hits the floor");
        assert_eq!(
            mesh[hit.tri].surface, SURFACE_FLOOR,
            "from above, the camera-facing surface is the floor (ceiling culled)"
        );
        assert!(
            (hit.world[2] - floor_h).abs() < 1.0,
            "hit z is the floor height"
        );
    }

    /// Ray up from below: floor back-face culled; ceiling front-face is the hit.
    #[test]
    fn ray_up_from_below_culls_floor_backface() {
        let wad = wad::WadData::new(&test_utils::doom1_wad_path());
        let map = editor_core::import_wad_map(&wad, "E1M1").expect("E1M1 imports");
        let tris = build_sector_tris(&map);
        let mesh = build_surface(&map, &tris, &HashMap::new()).0;

        let start = map
            .things
            .values()
            .find(|t| t.kind == 1)
            .expect("player start");
        let at = [start.x as f32, start.y as f32];
        let s = sector_at(&map, at).expect("start is inside a sector");
        let floor_h = map.sectors[s].floor_height as f32;
        let origin = [at[0], at[1], floor_h - 16.0];
        let hit = pick_mesh(&mesh, origin, [0.0, 0.0, 1.0]).expect("ray hits the ceiling");
        assert_eq!(
            mesh[hit.tri].surface, SURFACE_CEIL,
            "the floor's back face is culled; the ceiling is the front-facing hit"
        );
    }

    #[test]
    fn ray_into_void_misses() {
        let wad = wad::WadData::new(&test_utils::doom1_wad_path());
        let map = editor_core::import_wad_map(&wad, "E1M1").expect("E1M1 imports");
        let tris = build_sector_tris(&map);
        let mesh = build_surface(&map, &tris, &HashMap::new()).0;
        let hit = pick_mesh(&mesh, [1.0e6, 1.0e6, 1.0e5], [0.0, 0.0, 1.0]);
        assert!(hit.is_none(), "a ray in the void hits nothing");
    }
}
