//! Parse a [`Bsp3dLump`] into the runtime [`BSP3D`]: materialize MapPtrs from
//! lump indices, derive normals from winding, resolve the surface cache,
//! triangulate, compute AABBs, and build the event tables.

#[cfg(feature = "hprof")]
use coarse_prof::profile;

use crate::MapPtr;
use crate::bsp3d::movers::{build_tag_linedef_index, classify_sector_mover};
use crate::bsp3d::runtime::mark_leaf;
use crate::bsp3d::runtime::{AABB, BSP3D, BSPLeaf3D, Node3D, Polygon3D};
use crate::flags::LineDefFlags;
use crate::map_defs::{LineDef, Sector, SubSector};
use glam::{Vec2, Vec3};
use math::FixedT;
use rbsp::bsp3d::{Bsp3dLump, HEIGHT_EPSILON, NO_INDEX, PolyFlags, TreeNode};
use std::collections::HashSet;
use std::iter;

/// Signed shoelace area in XY over `verts` resolved through `indices`.
fn shoelace(indices: &[usize], verts: &[Vec3]) -> f32 {
    let n = indices.len();
    (0..n)
        .map(|i| {
            let a = verts[indices[i]];
            let b = verts[indices[(i + 1) % n]];
            a.x * b.y - b.x * a.y
        })
        .sum()
}

impl BSP3D {
    /// Materialize the runtime structure from a lump.
    ///
    /// `linedefs` is mutable only to take [`MapPtr`]s into it; `wall_tex_height`
    /// is indexed by wall texture id (peg anchors need texture heights).
    #[allow(clippy::too_many_arguments)]
    pub fn from_lump(
        lump: Bsp3dLump,
        subsectors: &[SubSector],
        sectors: &[Sector],
        linedefs: &mut [LineDef],
        wall_tex_height: Vec<f32>,
        sky_num: Option<usize>,
    ) -> Self {
        #[cfg(feature = "hprof")]
        profile!("BSP3D::from_lump");

        let Bsp3dLump {
            tree,
            vertices,
            poly_verts,
            polys,
            leaves,
            shared_walls,
        } = lump;
        // The disk format stores u32/u16 indices; widen once here so the
        // runtime indexes without casts.
        let poly_verts: Vec<usize> = poly_verts.iter().map(|&v| v as usize).collect();
        let shared_walls: Vec<usize> = shared_walls.iter().map(|&g| g as usize).collect();

        let root_node = if tree.is_empty() {
            mark_leaf(0)
        } else {
            (tree.len() - 1) as u32
        };
        let mut first_plane_node = tree.len() as u32;
        let mut nodes3d: Vec<Node3D> = Vec::with_capacity(tree.len());
        let mut node_bboxes: Vec<[[Vec2; 2]; 2]> = Vec::with_capacity(tree.len());
        for (i, t) in tree.iter().enumerate() {
            match t {
                TreeNode::Vertical(node) => {
                    let xy = Vec2::new(node.x as f32, node.y as f32);
                    let delta = Vec2::new(node.dx as f32, node.dy as f32);
                    let normal = Vec3::new(delta.y, -delta.x, 0.0);
                    nodes3d.push(Node3D {
                        normal,
                        d: normal.dot(Vec3::new(xy.x, xy.y, 0.0)),
                        xy_fp: [FixedT::from_f32(xy.x), FixedT::from_f32(xy.y)],
                        delta_fp: [FixedT::from_f32(delta.x), FixedT::from_f32(delta.y)],
                        children: [node.child_right, node.child_left],
                    });
                    node_bboxes.push([
                        [
                            Vec2::new(node.bbox_right.min_x as f32, node.bbox_right.max_y as f32),
                            Vec2::new(node.bbox_right.max_x as f32, node.bbox_right.min_y as f32),
                        ],
                        [
                            Vec2::new(node.bbox_left.min_x as f32, node.bbox_left.max_y as f32),
                            Vec2::new(node.bbox_left.max_x as f32, node.bbox_left.min_y as f32),
                        ],
                    ]);
                }
                TreeNode::Plane {
                    normal,
                    d,
                    children,
                } => {
                    first_plane_node = first_plane_node.min(i as u32);
                    nodes3d.push(Node3D {
                        normal: Vec3::from_array(*normal),
                        d: *d,
                        xy_fp: [FixedT::ZERO; 2],
                        delta_fp: [FixedT::ZERO; 2],
                        children: *children,
                    });
                    node_bboxes.push([[Vec2::ZERO; 2]; 2]);
                }
            }
        }
        let node_aabbs: Vec<AABB> = (0..nodes3d.len()).map(|_| AABB::new()).collect();

        let leaves: Vec<BSPLeaf3D> = leaves
            .iter()
            .map(|l| {
                let subsector = l.subsector as usize;
                BSPLeaf3D {
                    subsector,
                    sector: subsectors[subsector].sector.clone(),
                    aabb: AABB::new(),
                    poly_start: l.poly_start as usize,
                    poly_count: l.poly_count as usize,
                    shared_start: l.shared_start as usize,
                    shared_count: l.shared_count as usize,
                }
            })
            .collect();

        // Per-polygon: ranges, base flags, winding-derived normals, MapPtrs.
        let n = polys.len();
        let mut poly_vertex_range: Vec<(usize, usize)> = Vec::with_capacity(n);
        let mut poly_flags: Vec<PolyFlags> = Vec::with_capacity(n);
        let mut polygons: Vec<Polygon3D> = Vec::with_capacity(n);

        for leaf in &leaves {
            let start = leaf.poly_start;
            let end = start + leaf.poly_count;
            debug_assert_eq!(start, polygons.len(), "lump must be leaf-contiguous");
            for rec in &polys[start..end] {
                let vs = rec.vert_start as usize;
                let indices = &poly_verts[vs..vs + rec.vert_count as usize];
                let mut flags = rec.flags & PolyFlags::LUMP_BITS;

                let polygon = if rec.is_flat() {
                    flags |= PolyFlags::IS_FLAT;
                    let area = shoelace(indices, &vertices);
                    debug_assert!(area != 0.0, "flat with zero area survived the builder");
                    let normal = if area > 0.0 { Vec3::Z } else { Vec3::NEG_Z };
                    Polygon3D {
                        normal,
                        sector: leaf.sector.clone(),
                        linedef: None,
                        sidedef: None,
                        back_sidedef: None,
                        seg_offset: 0.0,
                    }
                } else {
                    let ld_ptr = MapPtr::new(&mut linedefs[rec.linedef as usize]);
                    // The linedef side is derived from the quad's traversal
                    // direction (front segs run with the linedef, back segs
                    // against it) — sidedef identity can't discriminate when a
                    // map reuses one sidedef on both sides. Sliver segs whose
                    // endpoints deduped to one vertex (zero-area quads) fall
                    // back to the recorded sidedef index.
                    let ld_dir = (ld_ptr.v2.pos - ld_ptr.v1.pos).normalize_or_zero();
                    let v0 = vertices[indices[0]];
                    let v1 = vertices[indices[1]];
                    let seg_dir = Vec2::new(v1.x - v0.x, v1.y - v0.y).try_normalize();
                    let is_front = match seg_dir {
                        Some(d) => d.dot(ld_dir) >= 0.0,
                        None => rec.sidedef == ld_ptr.sides[0] as u32,
                    };
                    #[cfg(debug_assertions)]
                    {
                        let expected = ld_ptr.sides[usize::from(!is_front)] as u32;
                        debug_assert_eq!(
                            rec.sidedef, expected,
                            "lump sidedef disagrees with the geometric side (linedef {})",
                            rec.linedef,
                        );
                    }
                    let (sidedef, back_sidedef) = if is_front {
                        (ld_ptr.front_sidedef.clone(), ld_ptr.back_sidedef.clone())
                    } else {
                        (
                            ld_ptr
                                .back_sidedef
                                .clone()
                                .expect("wall built from a missing back sidedef"),
                            Some(ld_ptr.front_sidedef.clone()),
                        )
                    };
                    let sector = sidedef.sector.clone();
                    // Winding contract: walls run along the seg direction, so
                    // the right-hand horizontal normal faces the sidedef side.
                    let d = seg_dir.unwrap_or(if is_front { ld_dir } else { -ld_dir });
                    let normal = Vec3::new(d.y, -d.x, 0.0);
                    Polygon3D {
                        normal,
                        sector,
                        linedef: Some(ld_ptr),
                        sidedef: Some(sidedef),
                        back_sidedef,
                        seg_offset: rec.seg_offset,
                    }
                };
                polygons.push(polygon);
                poly_vertex_range.push((vs, vs + rec.vert_count as usize));
                poly_flags.push(flags);
            }
        }
        debug_assert_eq!(polygons.len(), n, "leaf ranges must cover all polys");

        // Fan triangulation over the convex polys.
        let tri_count: usize = poly_vertex_range
            .iter()
            .map(|&(s, e)| (e - s).saturating_sub(2))
            .sum();
        let mut triangles: Vec<[u32; 3]> = Vec::with_capacity(tri_count);
        for &(s, e) in &poly_vertex_range {
            let count = e - s;
            if count < 3 {
                continue;
            }
            // The triangle list is the GPU index buffer — u32 by contract.
            let v0 = poly_verts[s] as u32;
            for i in 1..count - 1 {
                triangles.push([v0, poly_verts[s + i] as u32, poly_verts[s + i + 1] as u32]);
            }
        }

        // Lookup + event tables.
        let mut sector_leaves: Vec<Vec<usize>> = vec![Vec::new(); sectors.len()];
        for (leaf_id, leaf) in leaves.iter().enumerate() {
            let sector_id = leaf.sector.num as usize;
            if sector_id < sectors.len() {
                sector_leaves[sector_id].push(leaf_id);
            }
        }
        let mut sector_floor_polys: Vec<Vec<usize>> = vec![Vec::new(); sectors.len()];
        let mut sector_ceiling_polys: Vec<Vec<usize>> = vec![Vec::new(); sectors.len()];
        let mut sector_wall_polys: Vec<Vec<usize>> = vec![Vec::new(); sectors.len()];
        for (sector_id, leaf_ids) in sector_leaves.iter().enumerate() {
            // A two-sided wall is shared into several leaves; one entry per
            // sector.
            let mut seen: HashSet<usize> = HashSet::new();
            for &leaf_id in leaf_ids {
                let leaf = &leaves[leaf_id];
                let own_end = leaf.poly_start + leaf.poly_count;
                for gi in leaf.poly_start..own_end {
                    if poly_flags[gi].contains(PolyFlags::IS_FLAT) {
                        if polygons[gi].normal.z > 0.0 {
                            sector_floor_polys[sector_id].push(gi);
                        } else {
                            sector_ceiling_polys[sector_id].push(gi);
                        }
                    } else if seen.insert(gi) {
                        sector_wall_polys[sector_id].push(gi);
                    }
                }
                let shared =
                    &shared_walls[leaf.shared_start..leaf.shared_start + leaf.shared_count];
                for &gi in shared {
                    if seen.insert(gi) {
                        sector_wall_polys[sector_id].push(gi);
                    }
                }
            }
        }
        let mut linedef_wall_polys: Vec<Vec<usize>> = vec![Vec::new(); linedefs.len()];
        for (gi, p) in polygons.iter().enumerate() {
            if let Some(ld) = &p.linedef {
                linedef_wall_polys[ld.num].push(gi);
            }
        }

        let uv_len = poly_verts.len();
        let mut bsp = Self {
            nodes: nodes3d,
            node_bboxes,
            node_aabbs,
            root_node,
            first_plane_node,
            leaves,
            shared_walls,
            vertices,
            poly_verts,
            poly_vertex_range,
            poly_vertex_uv: vec![[0.0; 2]; uv_len],
            triangles,
            poly_tex: vec![NO_INDEX; n],
            poly_back_tex: vec![NO_INDEX; n],
            poly_flags,
            poly_scroll: vec![0.0; n],
            polygons,
            sector_leaves,
            sector_floor_polys,
            sector_ceiling_polys,
            sector_wall_polys,
            linedef_wall_polys,
            wall_tex_height,
            sky_num,
            // First frame must upload the initial geometry + textures.
            geometry_dirty: true,
            texture_dirty: true,
            texture_dirty_polys: Vec::new(),
            texture_dirty_full: true,
        };

        // Surface cache: textures, flag bits, UV.
        for gi in 0..n {
            if bsp.poly_flags[gi].contains(PolyFlags::IS_FLAT) {
                bsp.resolve_flat(gi);
                bsp.resolve_flat_uv(gi);
            } else {
                bsp.resolve_wall(gi);
            }
        }

        // AABBs: leaves from geometry, nodes bottom-up, then mover expansion.
        for ss_id in 0..bsp.leaves.len() {
            let aabb = bsp.compute_leaf_aabb(ss_id);
            bsp.leaves[ss_id].aabb = aabb;
        }
        bsp.update_node_aabbs_recursive(bsp.root_node);
        bsp.expand_node_aabbs_for_movers(sectors, linedefs);

        bsp
    }

    /// Expand node AABBs to cover the full vertical range of mover sectors.
    ///
    /// A mover's vertical travel is propagated to its own subsector leaves
    /// **and** to the leaves of two-sided neighbours: the opposite-facing
    /// upper/lower wall of a shared linedef lives in the neighbour's
    /// subsector and tracks this sector's floor/ceiling as it moves.
    /// Without the neighbour expansion that wall's leaf AABB stays at the
    /// static opening and gets frustum-culled once the mover travels past
    /// it (e.g. E1M5 ld808: s48 floor drops, lower wall in s50's leaf
    /// vanishes when looking into the pit).
    fn expand_node_aabbs_for_movers(&mut self, sectors: &[Sector], linedefs: &[LineDef]) {
        let tag_linedefs = build_tag_linedef_index(linedefs);
        // Accumulated (min_z, max_z) expansion per subsector leaf.
        let mut expand: Vec<(f32, f32)> = vec![(f32::MAX, f32::MIN); self.leaves.len()];

        for (sector_id, sector) in sectors.iter().enumerate() {
            let is_mover = classify_sector_mover(sector, linedefs, &tag_linedefs).is_some();
            let is_zero_height = (sector.ceilingheight.to_f32() - sector.floorheight.to_f32())
                .abs()
                <= HEIGHT_EPSILON;

            if !is_mover && !is_zero_height {
                continue;
            }

            let mut min_floor = sector.floorheight.to_f32();
            let mut max_ceil = sector.ceilingheight.to_f32();
            let mut neighbours: Vec<usize> = Vec::new();

            for line in &sector.lines {
                if !line.flags.contains(LineDefFlags::TwoSided) {
                    continue;
                }
                let neighbor = if line.frontsector.num == sector.num {
                    line.backsector.as_ref()
                } else {
                    Some(&line.frontsector)
                };
                if let Some(other) = neighbor {
                    min_floor = min_floor.min(other.floorheight.to_f32());
                    max_ceil = max_ceil.max(other.ceilingheight.to_f32());
                    neighbours.push(other.num as usize);
                }
            }

            // Own leaves plus every two-sided neighbour's leaves: the shared
            // wall driven by this mover lives in the neighbour's subsector.
            let targets = iter::once(sector_id).chain(neighbours);
            for tid in targets {
                if tid >= self.sector_leaves.len() {
                    continue;
                }
                for &subsector_id in &self.sector_leaves[tid] {
                    let e = &mut expand[subsector_id];
                    e.0 = e.0.min(min_floor);
                    e.1 = e.1.max(max_ceil);
                }
            }
        }

        for (subsector_id, &(min_z, max_z)) in expand.iter().enumerate() {
            let leaf = &mut self.leaves[subsector_id];
            if min_z < leaf.aabb.min.z {
                leaf.aabb.min.z = min_z;
            }
            if max_z > leaf.aabb.max.z {
                leaf.aabb.max.z = max_z;
            }
        }

        self.update_node_aabbs_recursive(self.root_node);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use math::FixedT;
    use rbsp::bsp3d::{LeafRecord, PolyRecord};

    /// One subsector volume z-split at 64 into two leaves by a plane node.
    fn z_split_lump() -> Bsp3dLump {
        let quad = |z: f32, reversed: bool| -> Vec<Vec3> {
            let mut v = vec![
                Vec3::new(0.0, 0.0, z),
                Vec3::new(64.0, 0.0, z),
                Vec3::new(64.0, 64.0, z),
                Vec3::new(0.0, 64.0, z),
            ];
            if reversed {
                v.reverse();
            }
            v
        };
        let mut vertices = quad(0.0, false);
        vertices.extend(quad(128.0, true));
        let flat = |vert_start: u32| PolyRecord {
            vert_start,
            vert_count: 4,
            flags: PolyFlags::empty(),
            linedef: NO_INDEX,
            sidedef: NO_INDEX,
            seg_offset: 0.0,
        };
        let leaf = |poly_start: u32| LeafRecord {
            subsector: 0,
            poly_start,
            poly_count: 1,
            shared_start: 0,
            shared_count: 0,
        };
        Bsp3dLump {
            tree: vec![TreeNode::Plane {
                normal: [0.0, 0.0, 1.0],
                d: 64.0,
                children: [mark_leaf(1), mark_leaf(0)],
            }],
            vertices,
            poly_verts: vec![0, 1, 2, 3, 4, 5, 6, 7],
            polys: vec![flat(0), flat(4)],
            leaves: vec![leaf(0), leaf(1)],
            shared_walls: Vec::new(),
        }
    }

    #[test]
    fn z_split_leaf_parses_and_resolves() {
        let mut sectors = vec![Sector::new(
            0,
            FixedT::ZERO,
            FixedT::from(128),
            2,
            3,
            160,
            0,
            0,
        )];
        let mut subsectors = vec![SubSector {
            sector: MapPtr::new(&mut sectors[0]),
            seg_count: 0,
            start_seg: 0,
        }];
        subsectors[0].sector = MapPtr::new(&mut sectors[0]);
        let mut linedefs: Vec<LineDef> = Vec::new();

        let bsp = BSP3D::from_lump(
            z_split_lump(),
            &subsectors,
            &sectors,
            &mut linedefs,
            Vec::new(),
            None,
        );

        assert_eq!(bsp.leaves.len(), 2, "two volume leaves");
        assert_eq!(bsp.first_plane_node(), 0, "plane subtree starts at root");
        assert_eq!(bsp.leaves[0].subsector, 0);
        assert_eq!(bsp.leaves[1].subsector, 0);

        let node = &bsp.nodes()[0];
        assert_eq!(node.normal, Vec3::Z);
        assert_eq!(node.d, 64.0);
        let above = node.front_back_children_plane(Vec3::new(32.0, 32.0, 100.0));
        assert_eq!(
            above,
            (mark_leaf(1), mark_leaf(0)),
            "above picks the upper leaf first"
        );
        let below = node.front_back_children_plane(Vec3::new(32.0, 32.0, 10.0));
        assert_eq!(
            below,
            (mark_leaf(0), mark_leaf(1)),
            "below picks the lower leaf first"
        );

        let leaf_id = bsp.point_in_leaf(FixedT::from(32), FixedT::from(32));
        assert_eq!(leaf_id, 1, "stop rule resolves to the front-child leaf");
        assert_eq!(bsp.leaves[leaf_id].subsector, 0);
        assert_eq!(
            bsp.subtree_leaf(0),
            1,
            "front-descend lands in a subtree leaf"
        );

        let aabb = bsp.get_node_aabb(0).expect("root aabb");
        assert_eq!(aabb.min.z, 0.0);
        assert_eq!(aabb.max.z, 128.0);
    }
}
