//! Seg splitting against partition lines.

use crate::picknode::{classify_point, classify_seg};
use crate::types::*;
use crate::vertex_pool::VertexPool;
use crate::walltip::copy_wall_tips_for_split;

/// Split a seg into two halves at a new vertex.
fn create_split_segs(
    seg_idx: usize,
    v_new: usize,
    partition: &Seg,
    vertices: &[Vertex],
    segs: &mut Vec<Seg>,
) -> (usize, usize) {
    let s = segs[seg_idx].clone();
    let start_side = classify_point(partition, &vertices[s.start], vertices);

    let (mut left_half, mut right_half) =
        if start_side == PointSide::Left || start_side == PointSide::OnLine {
            let left = Seg {
                start: s.start,
                end: v_new,
                offset: s.offset,
                ..s.clone()
            };
            let dist = {
                let dx = vertices[v_new].x - vertices[s.start].x;
                let dy = vertices[v_new].y - vertices[s.start].y;
                (dx * dx + dy * dy).sqrt()
            };
            let right = Seg {
                start: v_new,
                end: s.end,
                offset: s.offset + dist,
                ..s
            };
            (left, right)
        } else {
            let right = Seg {
                start: s.start,
                end: v_new,
                offset: s.offset,
                ..s.clone()
            };
            let dist = {
                let dx = vertices[v_new].x - vertices[s.start].x;
                let dy = vertices[v_new].y - vertices[s.start].y;
                (dx * dx + dy * dy).sqrt()
            };
            let left = Seg {
                start: v_new,
                end: s.end,
                offset: s.offset + dist,
                ..s
            };
            (left, right)
        };

    // Preserve the parent seg's direction so split fragments define the same
    // infinite line as the original linedef. Only recompute fragment len.
    for half in [&mut left_half, &mut right_half] {
        let fdx = vertices[half.end].x - vertices[half.start].x;
        let fdy = vertices[half.end].y - vertices[half.start].y;
        half.len = (fdx * fdx + fdy * fdy).sqrt();
    }

    let left_idx = segs.len();
    segs.push(left_half);
    let right_idx = segs.len();
    segs.push(right_half);

    (left_idx, right_idx)
}

/// Result of splitting segs and the clip polygon against a partition line.
pub struct SplitResult {
    pub left_segs: Vec<usize>,
    pub right_segs: Vec<usize>,
    pub left_poly: ClipPoly,
    pub right_poly: ClipPoly,
}

/// Split segs and clip the polygon against a partition line. Split vertices
/// and seg endpoints are inserted into both polygon halves so that seg
/// vertices are polygon vertices.
pub fn split_segs_and_poly(
    seg_indices: &[usize],
    partition: &Seg,
    clip_poly: &ClipPoly,
    pool: &mut VertexPool,
    segs: &mut Vec<Seg>,
    linedefs: &[WadLineDef],
    sidedefs: &[WadSideDef],
    wall_tips: &mut Vec<Vec<WallTip>>,
) -> SplitResult {
    let mut left_segs = Vec::with_capacity(seg_indices.len());
    let mut right_segs = Vec::with_capacity(seg_indices.len());
    let mut split_vertices: Vec<u32> = Vec::new();

    for &seg_idx in seg_indices {
        let side = classify_seg(partition, &segs[seg_idx], &pool.vertices);

        match side {
            SegSide::Left => left_segs.push(seg_idx),
            SegSide::Right => right_segs.push(seg_idx),
            SegSide::Split => {
                let seg = &segs[seg_idx];
                let sx = pool.vertices[seg.start].x as f64;
                let sy = pool.vertices[seg.start].y as f64;
                let px = pool.vertices[partition.linedef_v1].x as f64;
                let py = pool.vertices[partition.linedef_v1].y as f64;
                let pdx = partition.dx as f64;
                let pdy = partition.dy as f64;
                let sdx = seg.dx as f64;
                let sdy = seg.dy as f64;
                let denom = pdx * sdy - pdy * sdx;

                if denom.abs() < PARALLEL_EPSILON as f64 {
                    right_segs.push(seg_idx);
                    continue;
                }

                let num = pdy * (sx - px) - pdx * (sy - py);
                let u = num / denom;

                if u < EPSILON as f64 || u > 1.0 - EPSILON as f64 {
                    let mx = (sx + 0.5 * sdx) as Float;
                    let my = (sy + 0.5 * sdy) as Float;
                    let mid_side = classify_point(
                        partition,
                        &Vertex {
                            x: mx,
                            y: my,
                        },
                        &pool.vertices,
                    );
                    match mid_side {
                        PointSide::Left => left_segs.push(seg_idx),
                        _ => right_segs.push(seg_idx),
                    }
                    continue;
                }

                let hx = (sx + u * sdx) as Float;
                let hy = (sy + u * sdy) as Float;
                let v_new = pool.dedup(hx, hy) as usize;

                split_vertices.push(v_new as u32);

                let ld = &linedefs[segs[seg_idx].linedef];
                copy_wall_tips_for_split(wall_tips, ld, sidedefs, &pool.vertices, v_new);

                let (left_idx, right_idx) =
                    create_split_segs(seg_idx, v_new, partition, &pool.vertices, segs);
                left_segs.push(left_idx);
                right_segs.push(right_idx);
            }
        }
    }

    // Clip the polygon using the same partition line.
    let (mut right_poly, mut left_poly) =
        crate::polygon::clip_convex_poly(clip_poly, partition, pool);

    // Insert split vertices into both polygon halves. These are the only
    // vertices that need explicit insertion — seg endpoints should already
    // be in the polygon from earlier partition clips.
    for poly in [&mut right_poly, &mut left_poly] {
        for &sv in &split_vertices {
            if poly.verts.contains(&sv) {
                continue;
            }
            insert_vertex_on_edge(poly, sv, &pool.vertices);
        }
    }

    SplitResult {
        left_segs,
        right_segs,
        left_poly,
        right_poly,
    }
}

/// Insert a vertex onto the polygon edge it lies on (if any).
fn insert_vertex_on_edge(poly: &mut ClipPoly, v_idx: u32, vertices: &[Vertex]) {
    let pv = &vertices[v_idx as usize];
    let n = poly.verts.len();
    for i in 0..n {
        let a = &vertices[poly.verts[i] as usize];
        let b = &vertices[poly.verts[(i + 1) % n] as usize];
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
            poly.verts.insert(i + 1, v_idx);
            return;
        }
    }
}
