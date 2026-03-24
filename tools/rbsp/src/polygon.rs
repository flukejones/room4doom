//! Clip polygon operations.

use crate::picknode::classify_point;
use crate::types::*;
use crate::vertex_pool::VertexPool;

/// Compute the signed area of a polygon (shoelace formula).
/// Positive = CCW, negative = CW.
pub fn signed_area(verts: &[u32], vertices: &[Vertex]) -> Float {
    let mut area = 0.0;
    let n = verts.len();
    for i in 0..n {
        let vi = &vertices[verts[i] as usize];
        let vj = &vertices[verts[(i + 1) % n] as usize];
        area += vi.x * vj.y - vj.x * vi.y;
    }
    area * 0.5
}

/// Create the initial clip polygon from map bounds + margin.
pub fn make_initial_clip_poly(
    min_x: Float,
    min_y: Float,
    max_x: Float,
    max_y: Float,
    pool: &mut VertexPool,
) -> ClipPoly {
    let v0 = pool.dedup(min_x - MARGIN, min_y - MARGIN);
    let v1 = pool.dedup(max_x + MARGIN, min_y - MARGIN);
    let v2 = pool.dedup(max_x + MARGIN, max_y + MARGIN);
    let v3 = pool.dedup(min_x - MARGIN, max_y + MARGIN);
    ClipPoly {
        verts: vec![v0, v1, v2, v3],
    }
}

/// Split a convex clip polygon against a partition line into right and left
/// halves.
///
/// OnLine vertices go to Right only (matching engine's cross <= 0 → Right
/// convention). All intersection vertices are inserted via dedup into the
/// shared pool.
pub fn clip_convex_poly(
    poly: &ClipPoly,
    partition: &Seg,
    pool: &mut VertexPool,
) -> (ClipPoly, ClipPoly) {
    let cap = poly.verts.len() + 2;
    let mut right_verts = Vec::with_capacity(cap);
    let mut left_verts = Vec::with_capacity(cap);

    let px = pool.vertices[partition.linedef_v1].x;
    let py = pool.vertices[partition.linedef_v1].y;

    let n = poly.verts.len();
    for i in 0..n {
        let curr_idx = poly.verts[i];
        let next_idx = poly.verts[(i + 1) % n];

        let curr_side =
            classify_point(partition, &pool.vertices[curr_idx as usize], &pool.vertices);
        let next_side =
            classify_point(partition, &pool.vertices[next_idx as usize], &pool.vertices);

        match curr_side {
            PointSide::Right | PointSide::OnLine => right_verts.push(curr_idx),
            PointSide::Left => left_verts.push(curr_idx),
        }

        if (curr_side == PointSide::OnLine && next_side == PointSide::Left)
            || (curr_side == PointSide::Left && next_side == PointSide::OnLine)
        {
            let shared_idx = if curr_side == PointSide::OnLine {
                curr_idx
            } else {
                next_idx
            };
            if left_verts.last() != Some(&shared_idx) {
                left_verts.push(shared_idx);
            }
        }

        let crossing = matches!(
            (curr_side, next_side),
            (PointSide::Left, PointSide::Right) | (PointSide::Right, PointSide::Left)
        );

        if crossing {
            let cx = pool.vertices[curr_idx as usize].x as f64;
            let cy = pool.vertices[curr_idx as usize].y as f64;
            let nx = pool.vertices[next_idx as usize].x as f64;
            let ny = pool.vertices[next_idx as usize].y as f64;
            let edx = nx - cx;
            let edy = ny - cy;
            let pdx = partition.dx as f64;
            let pdy = partition.dy as f64;

            let denom = pdx * edy - pdy * edx;
            if denom.abs() > PARALLEL_EPSILON as f64 {
                let num = pdy * (cx - px as f64) - pdx * (cy - py as f64);
                let u = (num / denom).clamp(0.0, 1.0);
                let hx = (cx + u * edx) as Float;
                let hy = (cy + u * edy) as Float;
                let hit_idx = pool.dedup(hx, hy);
                right_verts.push(hit_idx);
                left_verts.push(hit_idx);
            }
        }
    }

    right_verts.dedup();
    left_verts.dedup();
    if right_verts.len() > 1 && right_verts.last() == right_verts.first() {
        right_verts.pop();
    }
    if left_verts.len() > 1 && left_verts.last() == left_verts.first() {
        left_verts.pop();
    }

    (
        ClipPoly {
            verts: right_verts,
        },
        ClipPoly {
            verts: left_verts,
        },
    )
}
