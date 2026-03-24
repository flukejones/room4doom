//! Quake-style frustum (`R_SetFrustum`/`R_CullBox`): 4 inward side planes,
//! tri-state AABB test. Near/far stay with the per-polygon cull.

use glam::Vec3;
use level::AABB;

/// AABB vs frustum; `Inside` subtrees need no further frustum tests.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum AabbCull {
    Outside,
    Intersect,
    Inside,
}

/// 4 inward side planes, unnormalised (sign test only).
pub(crate) struct Frustum {
    planes: [(Vec3, f32); 4],
}

impl Frustum {
    /// Z-up forward from yaw+pitch. FOVs in radians.
    pub(crate) fn new(
        camera_pos: Vec3,
        angle_rad: f32,
        pitch_rad: f32,
        hfov: f32,
        vfov: f32,
    ) -> Self {
        let forward = Vec3::new(
            angle_rad.cos() * pitch_rad.cos(),
            angle_rad.sin() * pitch_rad.cos(),
            pitch_rad.sin(),
        );
        // Pitch is clamped below 90° (MAX_PITCH), so forward is never Z.
        let right = forward.cross(Vec3::Z).normalize();
        let up = right.cross(forward);
        let (ha, va) = (hfov * 0.5, vfov * 0.5);
        // Inward normal = forward*sin(half_fov) ± basis*cos(half_fov).
        let normals = [
            forward * ha.sin() + right * ha.cos(),
            forward * ha.sin() - right * ha.cos(),
            forward * va.sin() + up * va.cos(),
            forward * va.sin() - up * va.cos(),
        ];
        Self {
            planes: normals.map(|n| (n, n.dot(camera_pos))),
        }
    }

    /// True if the AABB's p-vertex is behind any side plane (`R_CullBox`).
    pub(crate) fn cull_aabb(&self, aabb: &AABB) -> bool {
        self.planes
            .iter()
            .any(|&(n, dist)| n.dot(p_vertex(n, aabb)) < dist)
    }

    /// `Outside` = p-vertex behind any plane; `Inside` = n-vertex clear of all.
    pub(crate) fn classify_aabb(&self, aabb: &AABB) -> AabbCull {
        let mut inside = true;
        for &(n, dist) in &self.planes {
            if n.dot(p_vertex(n, aabb)) < dist {
                return AabbCull::Outside;
            }
            if n.dot(n_vertex(n, aabb)) < dist {
                inside = false;
            }
        }
        if inside {
            AabbCull::Inside
        } else {
            AabbCull::Intersect
        }
    }
}

/// AABB corner maximising the dot with `n`.
fn p_vertex(n: Vec3, aabb: &AABB) -> Vec3 {
    Vec3::new(
        if n.x >= 0.0 { aabb.max.x } else { aabb.min.x },
        if n.y >= 0.0 { aabb.max.y } else { aabb.min.y },
        if n.z >= 0.0 { aabb.max.z } else { aabb.min.z },
    )
}

/// AABB corner minimising the dot with `n`.
fn n_vertex(n: Vec3, aabb: &AABB) -> Vec3 {
    Vec3::new(
        if n.x >= 0.0 { aabb.min.x } else { aabb.max.x },
        if n.y >= 0.0 { aabb.min.y } else { aabb.max.y },
        if n.z >= 0.0 { aabb.min.z } else { aabb.max.z },
    )
}
