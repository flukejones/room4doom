//! GPU mesh buffers from BSP3D. Positions are stored once per vertex; the vertex
//! shader pulls them via a per-corner index buffer, so per-face UV/texture need
//! no position duplication. UV is fanned from BSP3D's canonical per-polygon-vertex
//! store; attrs (tex/is_flat) are fanned once.

use bytemuck::{Pod, Zeroable};
use level::{BSP3D, contrast_adjust};

/// Per-BSP3D-vertex world position. `vec4` for std430 alignment (w unused).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Position {
    pub pos: [f32; 4],
}

/// Per-corner static attributes: atlas selector + lighting inputs. Sector light
/// is looked up live in the shader by `sector`; `contrast_adjust` is baked.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct CornerAttr {
    pub tex: u32,
    pub is_flat: u32,
    pub sector: u32,
    pub contrast_adjust: i32,
    pub is_sky: u32,
    /// Two-sided middle (masked): discard v outside [0,1) so it isn't tiled.
    pub is_masked_mid: u32,
}

/// CPU-side mesh buffers ready for upload.
pub struct Mesh {
    pub positions: Vec<Position>,
    pub corner_index: Vec<u32>,
    pub corner_attr: Vec<CornerAttr>,
    /// Per-polygon `(first corner, count)` in `fan_corner_attr` order; CPU-only,
    /// the walk emits these ranges as the visible-corner index list.
    pub poly_corner_range: Vec<(u32, u32)>,
}

impl Mesh {
    pub fn build(bsp3d: &BSP3D) -> Self {
        let positions = bsp3d
            .vertices
            .iter()
            .map(|p| Position {
                pos: [p.x, p.y, p.z, 1.0],
            })
            .collect();
        let corner_index = bsp3d.triangles.iter().flatten().copied().collect();
        let mut corner_attr = Vec::new();
        bsp3d.fan_corner_attr(&mut corner_attr, |p| corner_attr_of(bsp3d, p));
        Self {
            positions,
            corner_index,
            corner_attr,
            poly_corner_range: poly_corner_ranges(&bsp3d.poly_vertex_range),
        }
    }

    /// Number of triangle corners to draw.
    pub fn corner_count(&self) -> u32 {
        self.corner_index.len() as u32
    }
}

/// Per-polygon corner ranges: a fan of n vertices = `(n-2)*3` corners, in
/// `fan_corner_attr` order. u32 by contract — these become GPU index ranges.
fn poly_corner_ranges(poly_vertex_range: &[(usize, usize)]) -> Vec<(u32, u32)> {
    let mut ranges = Vec::with_capacity(poly_vertex_range.len());
    let mut cursor = 0u32;
    for &(start, end) in poly_vertex_range {
        let n = end - start;
        let count = if n < 3 { 0 } else { ((n - 2) * 3) as u32 };
        ranges.push((cursor, count));
        cursor += count;
    }
    ranges
}

/// Per-polygon corner attributes from BSP3D. Shared by the initial fan and the
/// texture_dirty re-fan (switches change `tex`).
pub fn corner_attr_of(bsp3d: &BSP3D, p: usize) -> CornerAttr {
    CornerAttr {
        tex: bsp3d.poly_tex[p],
        is_flat: bsp3d.poly_is_flat(p) as u32,
        sector: bsp3d.polygons[p].sector.num as u32,
        contrast_adjust: contrast_adjust(bsp3d.polygons[p].normal),
        is_sky: bsp3d.poly_is_sky(p) as u32,
        is_masked_mid: bsp3d.poly_is_masked_middle(p) as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corner_ranges_contiguous_and_fan_sized() {
        // Quad, degenerate line, triangle, pentagon.
        let vertex_ranges = [(0usize, 4usize), (4, 6), (6, 9), (9, 14)];
        let ranges = poly_corner_ranges(&vertex_ranges);
        assert_eq!(ranges, vec![(0, 6), (6, 0), (6, 3), (9, 9)]);
        // Contiguous: each range starts where the previous ended.
        let mut cursor = 0;
        for &(start, count) in &ranges {
            assert_eq!(start, cursor);
            cursor += count;
        }
        // Total equals the summed fan corner count.
        let expected: u32 = vertex_ranges
            .iter()
            .map(|&(s, e)| {
                if e - s < 3 {
                    0
                } else {
                    ((e - s - 2) * 3) as u32
                }
            })
            .sum();
        assert_eq!(cursor, expected);
    }
}
