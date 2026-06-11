//! World walk: Quake-style frustum cull (`R_SetFrustum`/`R_CullBox`) over a
//! front-to-back BSP traverse. One pass emits the visible corner-id list and
//! collects sprite/voxel instances per visible leaf. No occlusion culling.

use glam::Vec3;
use level::{AABB, BSP3D, Sector, is_leaf, leaf_index};

use crate::sprites::{SpriteCollectCtx, SpriteScratch};
use crate::voxel::{VoxelCollectCtx, VoxelScratch};

/// AABB vs frustum; `Inside` subtrees need no further frustum tests.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AabbCull {
    Outside,
    Intersect,
    Inside,
}

/// 4 inward side planes, unnormalised (sign test only); GPU clips near/far.
pub struct Frustum {
    planes: [(Vec3, f32); 4],
}

impl Frustum {
    /// Z-up forward from yaw+pitch, matching `CameraUniform`. FOVs in radians.
    pub fn new(camera_pos: Vec3, angle_rad: f32, pitch_rad: f32, hfov: f32, vfov: f32) -> Self {
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
    pub fn cull_aabb(&self, aabb: &AABB) -> bool {
        self.planes
            .iter()
            .any(|&(n, dist)| n.dot(p_vertex(n, aabb)) < dist)
    }

    /// `Outside` = p-vertex behind any plane; `Inside` = n-vertex clear of all.
    pub fn classify_aabb(&self, aabb: &AABB) -> AabbCull {
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

/// Per-frame walk state: one traverse fills `indices` + entity instances.
pub struct WorldWalk<'a> {
    pub bsp3d: &'a BSP3D,
    pub sectors: &'a [Sector],
    pub frustum: &'a Frustum,
    pub camera_pos: Vec3,
    /// Per-polygon `(first corner, count)` — `Mesh::poly_corner_range`.
    pub poly_corner_range: &'a [(u32, u32)],
    /// Out: visible corner ids in front-to-back leaf order.
    pub indices: &'a mut Vec<u32>,
    /// Per-sector dedup for entity collection (sized to the sector count).
    pub seen_sectors: &'a mut [bool],
    pub sprites: &'a mut SpriteScratch,
    pub sprite_ctx: &'a SpriteCollectCtx<'a>,
    /// Voxel collection, only when the voxel pass is active.
    pub voxels: Option<(&'a mut VoxelScratch, &'a VoxelCollectCtx<'a>)>,
}

impl WorldWalk<'_> {
    /// Front-to-back BSP traverse with frustum-culled node/leaf AABBs;
    /// `inside` subtrees skip all further frustum tests.
    pub fn walk(&mut self, node_id: u32, inside: bool) {
        if is_leaf(node_id) {
            let leaf_id = leaf_index(node_id);
            let bsp3d = self.bsp3d;
            let Some(leaf) = bsp3d.get_leaf(leaf_id) else {
                return;
            };
            if !inside && self.frustum.cull_aabb(&leaf.aabb) {
                return;
            }
            self.visit_leaf(leaf_id);
            return;
        }

        let Some(node) = self.bsp3d.nodes().get(node_id as usize) else {
            return;
        };
        let children: [u32; 2] = node.front_back_children_plane(self.camera_pos).into();
        for child in children {
            if inside {
                self.walk(child, true);
                continue;
            }
            match self
                .bsp3d
                .get_node_aabb(child)
                .map(|a| self.frustum.classify_aabb(a))
            {
                Some(AabbCull::Outside) => {}
                Some(AabbCull::Inside) => self.walk(child, true),
                _ => self.walk(child, false),
            }
        }
    }

    fn visit_leaf(&mut self, leaf_id: usize) {
        let bsp3d = self.bsp3d;
        // Sectors before facing cull: back-facing geometry, visible things.
        for gi in bsp3d.leaf_poly_indices(leaf_id) {
            let sid = bsp3d.polygons[gi].sector.num as usize;
            if !self.seen_sectors[sid] {
                self.seen_sectors[sid] = true;
                let sector = &self.sectors[sid];
                self.sprites.collect_in_sector(self.sprite_ctx, sector);
                if let Some((voxels, ctx)) = &mut self.voxels {
                    voxels.collect_in_sector(ctx, sector);
                }
            }
        }
        for gi in bsp3d.leaf_poly_indices(leaf_id) {
            if bsp3d.is_facing_point(gi, self.camera_pos) {
                let (start, count) = self.poly_corner_range[gi];
                self.indices.extend(start..start + count);
            }
        }
    }
}
