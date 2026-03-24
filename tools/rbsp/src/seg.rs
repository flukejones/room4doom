//! Seg creation from linedefs.

use crate::types::*;

/// Make the initial segs list from linedefs. Each linedef becomes one or two
/// full-length segs (one per valid sidedef). Returns (global seg array, seg
/// index list).
pub fn create_segs(
    linedefs: &[WadLineDef],
    sidedefs: &[WadSideDef],
    vertices: &[Vertex],
) -> (Vec<Seg>, Vec<usize>) {
    let mut segs = Vec::new();
    let mut indices = Vec::new();

    for (ld_idx, ld) in linedefs.iter().enumerate() {
        let v1 = ld.start_vertex_idx();
        let v2 = ld.end_vertex_idx();

        let dx = vertices[v2].x - vertices[v1].x;
        let dy = vertices[v2].y - vertices[v1].y;
        let len = (dx * dx + dy * dy).sqrt();
        if len < EPSILON {
            continue;
        }

        if let Some(sd_idx) = ld.front_sidedef_idx() {
            let sd = &sidedefs[sd_idx];
            let idx = segs.len();
            segs.push(Seg {
                start: v1,
                end: v2,
                linedef: ld_idx,
                side: Side::Front,
                sector: sd.sector_idx(),
                offset: 0.0,
                angle: dy.atan2(dx),
                dx,
                dy,
                len,
                dir_len: len,
                linedef_v1: v1,
            });
            indices.push(idx);
        }

        if let Some(sd_idx) = ld.back_sidedef_idx() {
            let sd = &sidedefs[sd_idx];
            let idx = segs.len();
            segs.push(Seg {
                start: v2,
                end: v1,
                linedef: ld_idx,
                side: Side::Back,
                sector: sd.sector_idx(),
                offset: 0.0,
                angle: (-dy).atan2(-dx),
                dx: -dx,
                dy: -dy,
                len,
                dir_len: len,
                linedef_v1: v1,
            });
            indices.push(idx);
        }
    }

    (segs, indices)
}

/// Compute map bounds from vertices referenced by segs.
pub fn find_map_bounds(seg_indices: &[usize], segs: &[Seg], vertices: &[Vertex]) -> BBox {
    let mut bbox = BBox::EMPTY;
    for &idx in seg_indices {
        let seg = &segs[idx];
        for &vi in &[seg.start, seg.end] {
            let v = &vertices[vi];
            bbox.min_x = bbox.min_x.min(v.x);
            bbox.min_y = bbox.min_y.min(v.y);
            bbox.max_x = bbox.max_x.max(v.x);
            bbox.max_y = bbox.max_y.max(v.y);
        }
    }
    bbox
}
