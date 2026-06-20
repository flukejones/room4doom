//! Geometry derived from a [`Bsp3dLump`] — pure functions of the lump's flat
//! arrays, recomputed once at load rather than serialized.
//!
//! Per-poly normals, per-leaf AABBs, and the per-sector / per-linedef polygon
//! index tables are deterministic from the lump, so the engine builds them at
//! parse time instead of carrying them on disk.

use glam::{Vec2, Vec3};

use super::lump::{Bsp3dLump, PolyRecord};

/// Axis-aligned bounds of a leaf's geometry, as raw min/max corners. The engine
/// wraps these in its own runtime AABB type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LeafBounds {
    pub min: Vec3,
    pub max: Vec3,
}

impl Bsp3dLump {
    /// Per-polygon winding normal, in `polys` order. Flats face ±Z by their XY
    /// shoelace sign; walls take the right-hand horizontal normal of their first
    /// edge (a degenerate edge yields the zero vector).
    pub fn poly_normals(&self) -> Vec<Vec3> {
        (0..self.polys.len())
            .map(|gi| {
                let p = &self.polys[gi];
                if p.is_flat() {
                    if self.flat_faces_up(p) {
                        Vec3::Z
                    } else {
                        Vec3::NEG_Z
                    }
                } else {
                    let idx = self.poly_vert_indices(gi);
                    let v0 = self.vertices[idx[0] as usize];
                    let v1 = self.vertices[idx[1] as usize];
                    let d = Vec2::new(v1.x - v0.x, v1.y - v0.y).normalize_or_zero();
                    Vec3::new(d.y, -d.x, 0.0)
                }
            })
            .collect()
    }

    /// Per-leaf geometry bounds over own polys plus shared walls.
    pub fn leaf_bounds(&self) -> Vec<LeafBounds> {
        self.leaves
            .iter()
            .map(|leaf| {
                let mut min = Vec3::splat(f32::MAX);
                let mut max = Vec3::splat(f32::MIN);
                let own = leaf.poly_start..leaf.poly_start + leaf.poly_count as u32;
                let shared = &self.shared_walls[leaf.shared_start as usize
                    ..leaf.shared_start as usize + leaf.shared_count as usize];
                for gi in own.chain(shared.iter().copied()) {
                    let p = &self.polys[gi as usize];
                    let s = p.vert_start as usize;
                    for &vi in &self.poly_verts[s..s + p.vert_count as usize] {
                        let v = self.vertices[vi as usize];
                        min = min.min(v);
                        max = max.max(v);
                    }
                }
                LeafBounds {
                    min,
                    max,
                }
            })
            .collect()
    }

    /// Per-sector floor/ceiling/wall polygon lists. A flat with positive XY
    /// shoelace faces +Z (floor), negative faces −Z (ceiling) — matching the
    /// runtime's winding-derived normal. Two-sided walls shared into several
    /// leaves appear once per sector. Returns `(floor, ceiling, wall)`.
    #[allow(clippy::type_complexity)]
    pub fn sector_poly_tables(
        &self,
        subsector_sectors: &[u32],
        num_sectors: usize,
    ) -> (Vec<Vec<u32>>, Vec<Vec<u32>>, Vec<Vec<u32>>) {
        let mut floor = vec![Vec::new(); num_sectors];
        let mut ceiling = vec![Vec::new(); num_sectors];
        let mut wall = vec![Vec::new(); num_sectors];

        // Group leaves by sector, so a sector's shared walls dedup across leaves.
        let mut sector_leaves: Vec<Vec<usize>> = vec![Vec::new(); num_sectors];
        for (leaf_id, leaf) in self.leaves.iter().enumerate() {
            let sector = subsector_sectors.get(leaf.subsector as usize).copied();
            if let Some(s) = sector
                && (s as usize) < num_sectors
            {
                sector_leaves[s as usize].push(leaf_id);
            }
        }

        for (sector_id, leaf_ids) in sector_leaves.iter().enumerate() {
            let mut seen_walls = Vec::new();
            for &leaf_id in leaf_ids {
                let leaf = &self.leaves[leaf_id];
                let own = leaf.poly_start..leaf.poly_start + leaf.poly_count as u32;
                for gi in own {
                    let p = &self.polys[gi as usize];
                    if p.is_flat() {
                        if self.flat_faces_up(p) {
                            floor[sector_id].push(gi);
                        } else {
                            ceiling[sector_id].push(gi);
                        }
                    } else if !seen_walls.contains(&gi) {
                        seen_walls.push(gi);
                        wall[sector_id].push(gi);
                    }
                }
                let shared = &self.shared_walls[leaf.shared_start as usize
                    ..leaf.shared_start as usize + leaf.shared_count as usize];
                for &gi in shared {
                    if !seen_walls.contains(&gi) {
                        seen_walls.push(gi);
                        wall[sector_id].push(gi);
                    }
                }
            }
        }

        (floor, ceiling, wall)
    }

    /// Per-linedef wall polygon lists.
    pub fn linedef_wall_polys(&self, num_linedefs: usize) -> Vec<Vec<u32>> {
        let mut table = vec![Vec::new(); num_linedefs];
        for (gi, p) in self.polys.iter().enumerate() {
            if !p.is_flat() && (p.linedef as usize) < num_linedefs {
                table[p.linedef as usize].push(gi as u32);
            }
        }
        table
    }

    /// Whether a flat polygon faces up (+Z): positive XY shoelace area.
    fn flat_faces_up(&self, p: &PolyRecord) -> bool {
        let s = p.vert_start as usize;
        let idx = &self.poly_verts[s..s + p.vert_count as usize];
        let mut area = 0.0;
        let n = idx.len();
        for i in 0..n {
            let a = self.vertices[idx[i] as usize];
            let b = self.vertices[idx[(i + 1) % n] as usize];
            area += a.x * b.y - b.x * a.y;
        }
        area > 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bsp3d::lump::{LeafRecord, NO_INDEX, PolyFlags, PolyRecord};
    use crate::types::Side;

    /// One subsector (sector 0): a floor quad (CCW, +Z), a ceiling quad (CW, −Z),
    /// and one wall quad on linedef 0.
    fn one_sector_lump() -> Bsp3dLump {
        let vertices = vec![
            // floor (CCW in XY)
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(4.0, 0.0, 0.0),
            Vec3::new(4.0, 4.0, 0.0),
            Vec3::new(0.0, 4.0, 0.0),
            // ceiling (CW in XY)
            Vec3::new(0.0, 0.0, 8.0),
            Vec3::new(0.0, 4.0, 8.0),
            Vec3::new(4.0, 4.0, 8.0),
            Vec3::new(4.0, 0.0, 8.0),
            // wall quad
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(4.0, 0.0, 0.0),
            Vec3::new(4.0, 0.0, 8.0),
            Vec3::new(0.0, 0.0, 8.0),
        ];
        let flat = |vert_start: u32| PolyRecord {
            vert_start,
            vert_count: 4,
            flags: PolyFlags::empty(),
            linedef: NO_INDEX,
            sidedef: NO_INDEX,
            linedef_side: Side::Front,
            seg_offset: 0.0,
        };
        let wall = PolyRecord {
            vert_start: 8,
            vert_count: 4,
            flags: PolyFlags::empty(),
            linedef: 0,
            sidedef: 0,
            linedef_side: Side::Front,
            seg_offset: 0.0,
        };
        Bsp3dLump {
            tree: Vec::new(),
            vertices,
            poly_verts: (0..12).collect(),
            polys: vec![flat(0), flat(4), wall],
            leaves: vec![LeafRecord {
                subsector: 0,
                poly_start: 0,
                poly_count: 3,
                shared_start: 0,
                shared_count: 0,
            }],
            shared_walls: Vec::new(),
        }
    }

    #[test]
    fn classifies_floor_ceiling_wall() {
        let lump = one_sector_lump();
        let (floor, ceiling, wall) = lump.sector_poly_tables(&[0], 1);

        assert_eq!(floor, vec![vec![0]], "floor is poly 0 (+Z)");
        assert_eq!(ceiling, vec![vec![1]], "ceiling is poly 1 (−Z)");
        assert_eq!(wall, vec![vec![2]], "wall is poly 2");
        assert_eq!(
            lump.linedef_wall_polys(1),
            vec![vec![2]],
            "linedef 0 owns poly 2"
        );
    }

    #[test]
    fn fan_triangulates_in_poly_order() {
        let lump = one_sector_lump();
        let triangles = lump.triangles();

        // Each quad → 2 triangles; 3 quads → 6.
        assert_eq!(triangles.len(), 6);
        // Poly 0 fan is (v0, v1, v2), (v0, v2, v3).
        assert_eq!(triangles[0], [0, 1, 2]);
        assert_eq!(triangles[1], [0, 2, 3]);
        // Whole-map triangles equal the per-leaf primitives concatenated.
        let leaf: Vec<_> = lump.leaf_triangles(0).collect();
        assert_eq!(triangles, leaf);
    }

    #[test]
    fn poly_normals_match_winding() {
        let lump = one_sector_lump();
        let normals = lump.poly_normals();

        assert_eq!(normals[0], Vec3::Z, "floor faces +Z");
        assert_eq!(normals[1], Vec3::NEG_Z, "ceiling faces −Z");
        // Wall first edge runs +X, so the right-hand normal faces −Y.
        assert_eq!(normals[2], Vec3::new(0.0, -1.0, 0.0));
    }

    #[test]
    fn leaf_bounds_cover_geometry() {
        let lump = one_sector_lump();
        let bounds = lump.leaf_bounds();

        assert_eq!(bounds.len(), 1);
        assert_eq!(bounds[0].min, Vec3::new(0.0, 0.0, 0.0));
        assert_eq!(bounds[0].max, Vec3::new(4.0, 4.0, 8.0));
    }
}
