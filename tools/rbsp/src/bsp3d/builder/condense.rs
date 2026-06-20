//! Condense step: flatten the builder's per-leaf polygon buckets into the
//! leaf-contiguous, serializable [`Bsp3dLump`] — polygons reordered so each
//! leaf's own polys form one range, vertex index lists flattened, shared-wall
//! lists remapped to the new polygon indices.

use crate::bsp3d::lump::{Bsp3dLump, LeafRecord, NO_INDEX, PolyFlags, PolyRecord};
use crate::types::Side;

use super::Bsp3dBuilder;
use super::types::BuildKind;

impl Bsp3dBuilder {
    /// Flatten into a leaf-contiguous [`Bsp3dLump`].
    pub(super) fn condense(self) -> Bsp3dLump {
        let mut remap = vec![NO_INDEX; self.polygons.len()];
        let vert_total: usize = self.polygons.iter().map(|p| p.vertices.len()).sum();
        let mut polys = Vec::with_capacity(self.polygons.len());
        let mut poly_verts = Vec::with_capacity(vert_total);
        let mut leaves = Vec::with_capacity(self.leaves.len());

        for (ss_id, leaf) in self.leaves.iter().enumerate() {
            let poly_start = polys.len() as u32;
            for &gi in &leaf.polys {
                remap[gi] = polys.len() as u32;
                let p = &self.polygons[gi];
                let vert_start = poly_verts.len() as u32;
                poly_verts.extend(p.vertices.iter().map(|&v| v as u32));

                let mut flags = PolyFlags::empty();
                if p.moves {
                    flags |= PolyFlags::MOVES;
                }
                let (linedef, sidedef, linedef_side, seg_offset) = match p.kind {
                    BuildKind::Wall {
                        linedef,
                        sidedef,
                        linedef_side,
                        sky_filler,
                        seg_offset,
                        ..
                    } => {
                        if sky_filler {
                            flags |= PolyFlags::SKY_FILLER;
                        }
                        (linedef, sidedef, linedef_side, seg_offset)
                    }
                    BuildKind::Flat => (NO_INDEX, NO_INDEX, Side::Front, 0.0),
                };
                polys.push(PolyRecord {
                    vert_start,
                    vert_count: p.vertices.len() as u16,
                    flags,
                    linedef,
                    sidedef,
                    linedef_side,
                    seg_offset,
                });
            }
            leaves.push(LeafRecord {
                subsector: ss_id as u32,
                poly_start,
                poly_count: (polys.len() as u32 - poly_start) as u16,
                shared_start: 0,
                shared_count: 0,
            });
        }
        debug_assert!(
            remap.iter().all(|&r| r != NO_INDEX),
            "every polygon must belong to exactly one leaf"
        );

        let mut shared_walls = Vec::new();
        for (li, leaf) in self.leaves.iter().enumerate() {
            leaves[li].shared_start = shared_walls.len() as u32;
            leaves[li].shared_count = leaf.shared.len() as u16;
            shared_walls.extend(leaf.shared.iter().map(|&gi| remap[gi]));
        }

        Bsp3dLump {
            tree: Vec::new(),
            vertices: self.vertices,
            poly_verts,
            polys,
            leaves,
            shared_walls,
        }
    }
}
