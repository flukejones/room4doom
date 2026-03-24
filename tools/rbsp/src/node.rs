//! BSP node construction with clip polygon threading.

use std::time::Instant;

use crate::picknode::{is_convex, ranked_partitions, select_best_partition};
use crate::split::split_segs_and_poly;
use crate::superblock::build_superblock;
use crate::types::*;
use crate::vertex_pool::VertexPool;

/// Mutable builder state passed through the recursion.
pub struct BuildState<'a> {
    pub pool: &'a mut VertexPool,
    pub segs: &'a mut Vec<Seg>,
    pub nodes: &'a mut Vec<Node>,
    pub subsectors: &'a mut Vec<SubSector>,
    pub poly_indices: &'a mut Vec<u32>,
    pub edges: &'a mut Vec<Edge>,
    pub linedefs: &'a [WadLineDef],
    pub sidedefs: &'a [WadSideDef],
    pub wall_tips: &'a mut Vec<Vec<WallTip>>,
    pub options: &'a BspOptions,
    pub start_time: Instant,
}

/// Build the BSP tree recursively.
pub fn build_node(
    seg_indices: Vec<usize>,
    clip_poly: ClipPoly,
    bs: &mut BuildState,
    depth: usize,
) -> u32 {
    if depth > 600 {
        panic!(
            "BSP recursion too deep ({depth}): {} segs, {} poly verts. Likely infinite loop.",
            seg_indices.len(),
            clip_poly.verts.len(),
        );
    }

    // Empty seg list → seg-less subsector
    if seg_indices.is_empty() {
        return create_subsector(&[], clip_poly, bs);
    }

    // Convex → subsector leaf
    if is_convex(&seg_indices, bs.segs, &bs.pool.vertices) {
        return create_subsector(&seg_indices, clip_poly, bs);
    }

    // Build spatial superblock tree for accelerated partition scoring.
    let sblock = build_superblock(&seg_indices, bs.segs, &bs.pool.vertices);

    let Some(best_idx) = select_best_partition(
        &seg_indices,
        bs.segs,
        &bs.pool.vertices,
        bs.options,
        &sblock,
    ) else {
        return create_subsector(&seg_indices, clip_poly, bs);
    };

    let input_count = seg_indices.len();

    let try_partition = |partition_idx: usize, bs: &mut BuildState| -> Option<_> {
        let partition = bs.segs[partition_idx].clone();
        let split = split_segs_and_poly(
            &seg_indices,
            &partition,
            &clip_poly,
            bs.pool,
            bs.segs,
            bs.linedefs,
            bs.sidedefs,
            bs.wall_tips,
        );
        let max_child = split.left_segs.len().max(split.right_segs.len());
        if max_child >= input_count {
            return None;
        }
        if split.right_poly.verts.len() < 3 || split.left_poly.verts.len() < 3 {
            return None;
        }
        Some((partition, split))
    };

    let (partition, split) = if let Some(result) = try_partition(best_idx, bs) {
        result
    } else {
        let candidates = ranked_partitions(
            &seg_indices,
            bs.segs,
            &bs.pool.vertices,
            bs.options,
            &sblock,
        );
        let mut found = None;
        for &partition_idx in &candidates {
            if partition_idx == best_idx {
                continue;
            }
            if let Some(result) = try_partition(partition_idx, bs) {
                found = Some(result);
                break;
            }
        }
        match found {
            Some(result) => result,
            None => return create_subsector(&seg_indices, clip_poly, bs),
        }
    };

    // Recurse
    let right_child = build_node(split.right_segs, split.right_poly, bs, depth + 1);
    let left_child = build_node(split.left_segs, split.left_poly, bs, depth + 1);

    // Compute bboxes
    let right_bbox = compute_child_bbox(
        right_child,
        bs.subsectors,
        bs.nodes,
        bs.poly_indices,
        &bs.pool.vertices,
    );
    let left_bbox = compute_child_bbox(
        left_child,
        bs.subsectors,
        bs.nodes,
        bs.poly_indices,
        &bs.pool.vertices,
    );

    let node = Node {
        x: bs.pool.vertices[partition.start].x,
        y: bs.pool.vertices[partition.start].y,
        dx: partition.dx,
        dy: partition.dy,
        bbox_right: right_bbox,
        bbox_left: left_bbox,
        child_right: right_child,
        child_left: left_child,
    };

    let node_idx = bs.nodes.len() as u32;
    bs.nodes.push(node);
    node_idx
}

/// Top-down pass: walk all nodes and share boundary vertices between siblings.
/// Called once after the full tree is built so all vertices exist.
pub fn share_all_boundary_vertices(
    nodes: &[Node],
    subsectors: &mut Vec<SubSector>,
    poly_indices: &mut Vec<u32>,
    edges: &mut Vec<Edge>,
    pool: &VertexPool,
) {
    for ni in 0..nodes.len() {
        let node = &nodes[ni];
        let px = node.x;
        let py = node.y;
        let pdx = node.dx;
        let pdy = node.dy;
        let plen = (pdx * pdx + pdy * pdy).sqrt();
        if plen < EPSILON {
            continue;
        }

        let right_leaves = collect_boundary_leaves(
            node.child_right,
            px,
            py,
            pdx,
            pdy,
            plen,
            subsectors,
            nodes,
            poly_indices,
            &pool.vertices,
        );
        let left_leaves = collect_boundary_leaves(
            node.child_left,
            px,
            py,
            pdx,
            pdy,
            plen,
            subsectors,
            nodes,
            poly_indices,
            &pool.vertices,
        );

        let mut right_boundary: Vec<u32> = Vec::new();
        let mut left_boundary: Vec<u32> = Vec::new();

        for &ss_idx in &right_leaves {
            let ss = &subsectors[ss_idx];
            let ps = ss.polygon.first_vertex as usize;
            let pc = ss.polygon.num_vertices as usize;
            for k in 0..pc {
                let vi = poly_indices[ps + k];
                let v = &pool.vertices[vi as usize];
                let dist = (pdx * (v.y - py) - pdy * (v.x - px)).abs() / plen;
                if dist < VERTEX_EPSILON && !right_boundary.contains(&vi) {
                    right_boundary.push(vi);
                }
            }
        }
        for &ss_idx in &left_leaves {
            let ss = &subsectors[ss_idx];
            let ps = ss.polygon.first_vertex as usize;
            let pc = ss.polygon.num_vertices as usize;
            for k in 0..pc {
                let vi = poly_indices[ps + k];
                let v = &pool.vertices[vi as usize];
                let dist = (pdx * (v.y - py) - pdy * (v.x - px)).abs() / plen;
                if dist < VERTEX_EPSILON && !left_boundary.contains(&vi) {
                    left_boundary.push(vi);
                }
            }
        }

        insert_boundary_verts(
            &left_boundary,
            &right_leaves,
            subsectors,
            poly_indices,
            edges,
            &pool.vertices,
        );
        insert_boundary_verts(
            &right_boundary,
            &left_leaves,
            subsectors,
            poly_indices,
            edges,
            &pool.vertices,
        );
    }
}

fn collect_boundary_leaves(
    child: u32,
    px: Float,
    py: Float,
    pdx: Float,
    pdy: Float,
    plen: Float,
    subsectors: &[SubSector],
    nodes: &[Node],
    poly_indices: &[u32],
    vertices: &[Vertex],
) -> Vec<usize> {
    let mut out = Vec::new();
    collect_boundary_inner(
        child,
        px,
        py,
        pdx,
        pdy,
        plen,
        subsectors,
        nodes,
        poly_indices,
        vertices,
        &mut out,
    );
    out
}

fn collect_boundary_inner(
    child: u32,
    px: Float,
    py: Float,
    pdx: Float,
    pdy: Float,
    plen: Float,
    subsectors: &[SubSector],
    nodes: &[Node],
    poly_indices: &[u32],
    vertices: &[Vertex],
    out: &mut Vec<usize>,
) {
    if child & IS_SSECTOR_MASK != 0 {
        let ss_idx = (child & !IS_SSECTOR_MASK) as usize;
        let ss = &subsectors[ss_idx];
        let ps = ss.polygon.first_vertex as usize;
        let pc = ss.polygon.num_vertices as usize;
        for k in 0..pc {
            let vi = poly_indices[ps + k];
            let v = &vertices[vi as usize];
            let dist = (pdx * (v.y - py) - pdy * (v.x - px)).abs() / plen;
            if dist < VERTEX_EPSILON {
                out.push(ss_idx);
                return;
            }
        }
    } else {
        let node = &nodes[child as usize];
        let bb = BBox::union(&node.bbox_right, &node.bbox_left);
        let corners = [
            (bb.min_x, bb.min_y),
            (bb.max_x, bb.min_y),
            (bb.max_x, bb.max_y),
            (bb.min_x, bb.max_y),
        ];
        let mut has_pos = false;
        let mut has_neg = false;
        for &(cx, cy) in &corners {
            let d = pdx * (cy - py) - pdy * (cx - px);
            if d > 0.0 {
                has_pos = true;
            }
            if d < 0.0 {
                has_neg = true;
            }
        }
        if has_pos && has_neg
            || corners
                .iter()
                .any(|&(cx, cy)| (pdx * (cy - py) - pdy * (cx - px)).abs() / plen < VERTEX_EPSILON)
        {
            collect_boundary_inner(
                node.child_right,
                px,
                py,
                pdx,
                pdy,
                plen,
                subsectors,
                nodes,
                poly_indices,
                vertices,
                out,
            );
            collect_boundary_inner(
                node.child_left,
                px,
                py,
                pdx,
                pdy,
                plen,
                subsectors,
                nodes,
                poly_indices,
                vertices,
                out,
            );
        }
    }
}

fn insert_boundary_verts(
    verts_to_insert: &[u32],
    target_leaves: &[usize],
    subsectors: &mut Vec<SubSector>,
    poly_indices: &mut Vec<u32>,
    edges: &mut Vec<Edge>,
    vertices: &[Vertex],
) {
    for &ss_idx in target_leaves {
        let ss = &subsectors[ss_idx];
        let ps = ss.polygon.first_vertex as usize;
        let pc = ss.polygon.num_vertices as usize;
        if pc < 3 {
            continue;
        }

        let mut poly_verts: Vec<u32> = poly_indices[ps..ps + pc].to_vec();
        let mut changed = false;

        for &vi in verts_to_insert {
            if poly_verts.contains(&vi) {
                continue;
            }
            let pv = &vertices[vi as usize];
            let n = poly_verts.len();
            for i in 0..n {
                let a = &vertices[poly_verts[i] as usize];
                let b = &vertices[poly_verts[(i + 1) % n] as usize];
                let edx = b.x - a.x;
                let edy = b.y - a.y;
                let elen_sq = edx * edx + edy * edy;
                if elen_sq < EPSILON * EPSILON {
                    continue;
                }
                let cross = edx * (pv.y - a.y) - edy * (pv.x - a.x);
                if cross.abs() / elen_sq.sqrt() > VERTEX_EPSILON {
                    continue;
                }
                let t = (edx * (pv.x - a.x) + edy * (pv.y - a.y)) / elen_sq;
                if t > 0.001 && t < 0.999 {
                    poly_verts.insert(i + 1, vi);
                    changed = true;
                    break;
                }
            }
        }

        if changed {
            let es = subsectors[ss_idx].polygon.first_edge as usize;
            let old_edges: Vec<Edge> = edges[es..es + pc].to_vec();
            let new_ps = poly_indices.len() as u32;
            let new_es = edges.len() as u32;
            let new_pc = poly_verts.len();
            poly_indices.extend_from_slice(&poly_verts);
            for i in 0..new_pc {
                let sv = poly_verts[i];
                let ev = poly_verts[(i + 1) % new_pc];
                let old_match = old_edges
                    .iter()
                    .find(|e| e.start_vertex == sv && e.end_vertex == ev);
                match old_match {
                    Some(e) => edges.push(*e),
                    None => {
                        let seg_match = old_edges.iter().find(|e| {
                            if e.kind != EdgeKind::Seg {
                                return false;
                            }
                            let es_v = &vertices[e.start_vertex as usize];
                            let ee_v = &vertices[e.end_vertex as usize];
                            let edx = ee_v.x - es_v.x;
                            let edy = ee_v.y - es_v.y;
                            let elen = (edx * edx + edy * edy).sqrt();
                            if elen < EPSILON {
                                return false;
                            }
                            let vs = &vertices[sv as usize];
                            let ve = &vertices[ev as usize];
                            let d1 = (edx * (vs.y - es_v.y) - edy * (vs.x - es_v.x)).abs() / elen;
                            let d2 = (edx * (ve.y - es_v.y) - edy * (ve.x - es_v.x)).abs() / elen;
                            d1 < VERTEX_EPSILON && d2 < VERTEX_EPSILON
                        });
                        match seg_match {
                            Some(e) => edges.push(Edge {
                                start_vertex: sv,
                                end_vertex: ev,
                                ..*e
                            }),
                            None => edges.push(Edge {
                                kind: EdgeKind::Miniseg,
                                start_vertex: sv,
                                end_vertex: ev,
                                seg: Edge::NONE_SEG,
                                partner_leaf: Edge::NONE_PARTNER,
                            }),
                        }
                    }
                }
            }
            subsectors[ss_idx].polygon.first_vertex = new_ps;
            subsectors[ss_idx].polygon.num_vertices = new_pc as u32;
            subsectors[ss_idx].polygon.first_edge = new_es;
        }
    }
}

/// Create a subsector from the clip polygon and seg indices.
///
/// The clip polygon already has seg vertices inserted (done during the
/// combined split_segs_and_poly pass). Seg vertices ARE polygon vertices.
fn create_subsector(seg_indices: &[usize], clip_poly: ClipPoly, bs: &mut BuildState) -> u32 {
    if bs.subsectors.len() % 200 == 0 {
        let elapsed = bs.start_time.elapsed();
        print!(
            "\r  rBSP: {} ss, {} segs, {} nodes, {} verts [{:.1}s]",
            bs.subsectors.len(),
            bs.segs.len(),
            bs.nodes.len(),
            bs.pool.vertices.len(),
            elapsed.as_secs_f64(),
        );
    }

    let mut poly = clip_poly;

    // Ensure winding is CCW (positive signed area)
    if poly.verts.len() >= 3
        && crate::polygon::signed_area(&poly.verts, &bs.pool.vertices) < -EPSILON
    {
        poly.verts.reverse();
    }

    // Clip polygon against one-sided (solid) wall segs so it doesn't extend
    // past solid walls into void. Two-sided linedefs are skipped — the floor
    // continues through them at potentially different heights.
    for &si in seg_indices {
        if poly.verts.len() < 3 {
            break;
        }
        let seg = &bs.segs[si];
        if bs.linedefs[seg.linedef].back_sidedef_idx().is_some() {
            continue;
        }
        let (right, left) = crate::polygon::clip_convex_poly(&poly, seg, bs.pool);
        let keep = match seg.side {
            Side::Front => right,
            Side::Back => left,
        };
        if keep.verts.len() >= 3 {
            poly = keep;
        }
    }

    let poly_start = bs.poly_indices.len() as u32;
    let edge_start = bs.edges.len() as u32;
    let n = poly.verts.len() as u32;

    // Append polygon vertex indices
    bs.poly_indices.extend_from_slice(&poly.verts);

    // Walk polygon edges, classify as Seg or Miniseg
    for i in 0..poly.verts.len() {
        let v_start = poly.verts[i];
        let v_end = poly.verts[(i + 1) % poly.verts.len()];

        let matching_seg =
            find_seg_on_edge(seg_indices, v_start, v_end, bs.segs, &bs.pool.vertices);

        match matching_seg {
            Some(seg_idx) => bs.edges.push(Edge {
                kind: EdgeKind::Seg,
                start_vertex: v_start,
                end_vertex: v_end,
                seg: seg_idx as u32,
                partner_leaf: Edge::NONE_PARTNER,
            }),
            None => bs.edges.push(Edge {
                kind: EdgeKind::Miniseg,
                start_vertex: v_start,
                end_vertex: v_end,
                seg: Edge::NONE_SEG,
                partner_leaf: Edge::NONE_PARTNER,
            }),
        }
    }

    // Determine sector from seg sector field (set from linedef sidedef)
    let sector = if seg_indices.is_empty() {
        SubSector::UNASSIGNED_SECTOR
    } else {
        bs.segs[seg_indices[0]].sector as u32
    };

    let first_seg = seg_indices.first().copied().unwrap_or(0) as u32;

    let ss = SubSector {
        sector,
        polygon: ConvexPoly {
            first_vertex: poly_start,
            num_vertices: n,
            first_edge: edge_start,
        },
        first_seg,
        num_segs: seg_indices.len() as u32,
        seg_indices: seg_indices.iter().map(|&i| i as u32).collect(),
    };

    let ss_idx = bs.subsectors.len() as u32;
    bs.subsectors.push(ss);
    ss_idx | IS_SSECTOR_MASK
}

/// Find a seg that lies on (overlaps) a polygon edge.
/// The seg and edge must be collinear and their extents must overlap.
fn find_seg_on_edge(
    seg_indices: &[usize],
    v_start: u32,
    v_end: u32,
    segs: &[Seg],
    vertices: &[Vertex],
) -> Option<usize> {
    let vs = v_start as usize;
    let ve = v_end as usize;

    // Case 1: exact vertex index match (fast path)
    for &seg_idx in seg_indices {
        let seg = &segs[seg_idx];
        if seg.start == vs && seg.end == ve {
            return Some(seg_idx);
        }
    }

    // Case 2: seg overlaps edge — collinear + overlapping extent.
    // This handles the common case where the polygon edge is a fragment of the
    // seg (or vice versa) with different vertex indices.
    let e_sx = vertices[vs].x;
    let e_sy = vertices[vs].y;
    let edx = vertices[ve].x - e_sx;
    let edy = vertices[ve].y - e_sy;
    let e_len = (edx * edx + edy * edy).sqrt();
    if e_len < EPSILON {
        return None;
    }

    for &seg_idx in seg_indices {
        let seg = &segs[seg_idx];

        // Check collinearity: perpendicular distance from seg endpoints to edge line
        let d1 =
            (edx * (vertices[seg.start].y - e_sy) - edy * (vertices[seg.start].x - e_sx)) / e_len;
        let d2 = (edx * (vertices[seg.end].y - e_sy) - edy * (vertices[seg.end].x - e_sx)) / e_len;
        if d1.abs() > VERTEX_EPSILON || d2.abs() > VERTEX_EPSILON {
            continue; // not collinear
        }

        // Check overlap: project both seg endpoints onto the edge direction
        let t1 = (edx * (vertices[seg.start].x - e_sx) + edy * (vertices[seg.start].y - e_sy))
            / (e_len * e_len);
        let t2 = (edx * (vertices[seg.end].x - e_sx) + edy * (vertices[seg.end].y - e_sy))
            / (e_len * e_len);
        let seg_min = t1.min(t2);
        let seg_max = t1.max(t2);

        // Edge runs from t=0 to t=1. Check if seg overlaps this range.
        if seg_max > EPSILON && seg_min < 1.0 - EPSILON {
            return Some(seg_idx);
        }
    }

    None
}

/// Compute bounding box for a child node reference.
fn compute_child_bbox(
    child: u32,
    subsectors: &[SubSector],
    nodes: &[Node],
    poly_indices: &[u32],
    vertices: &[Vertex],
) -> BBox {
    if child & IS_SSECTOR_MASK != 0 {
        let ss_idx = (child & !IS_SSECTOR_MASK) as usize;
        let ss = &subsectors[ss_idx];
        bbox_from_poly(&ss.polygon, poly_indices, vertices)
    } else {
        let node = &nodes[child as usize];
        BBox::union(&node.bbox_right, &node.bbox_left)
    }
}

/// Compute bounding box from a polygon's vertex indices.
fn bbox_from_poly(poly: &ConvexPoly, poly_indices: &[u32], vertices: &[Vertex]) -> BBox {
    let start = poly.first_vertex as usize;
    let end = start + poly.num_vertices as usize;
    let mut bbox = BBox::EMPTY;
    for &vi in &poly_indices[start..end] {
        let v = &vertices[vi as usize];
        bbox.min_x = bbox.min_x.min(v.x);
        bbox.min_y = bbox.min_y.min(v.y);
        bbox.max_x = bbox.max_x.max(v.x);
        bbox.max_y = bbox.max_y.max(v.y);
    }
    bbox
}
