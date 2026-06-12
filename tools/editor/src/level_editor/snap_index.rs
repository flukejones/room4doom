//! Uniform-grid spatial index over map vertices and line segments, so draw-hover snap queries touch only the cells around the cursor instead of scanning the whole map.

use std::collections::HashMap;
use std::ops::RangeInclusive;

use editor_core::EditorMap;
use editor_core::geom::distance_to_segment;

/// Bucket edge length in world units; snap radii are a few pixels, so 3×3 cells cover any query.
const SNAP_CELL: f32 = 128.0;

/// Grid cell coordinate.
type Cell = [i32; 2];
/// Segment endpoints in world space.
type Seg = ([f32; 2], [f32; 2]);

/// XY buckets of vertex positions and segment endpoints; rebuilt lazily after the reconciler invalidates it.
pub(crate) struct SnapIndex {
    verts: HashMap<Cell, Vec<[f32; 2]>>,
    lines: HashMap<Cell, Vec<Seg>>,
}

impl SnapIndex {
    pub fn build(map: &EditorMap) -> Self {
        let mut verts: HashMap<Cell, Vec<[f32; 2]>> = HashMap::new();
        for v in map.vertices.values() {
            verts.entry(cell(v.x, v.y)).or_default().push([v.x, v.y]);
        }
        let mut lines: HashMap<Cell, Vec<Seg>> = HashMap::new();
        for line in map.lines.values() {
            let (Some(p1), Some(p2)) = (map.vertices.get(line.v1), map.vertices.get(line.v2))
            else {
                continue;
            };
            let (a, b) = ([p1.x, p1.y], [p2.x, p2.y]);
            for cx in span(a[0].min(b[0]), a[0].max(b[0])) {
                for cy in span(a[1].min(b[1]), a[1].max(b[1])) {
                    lines.entry([cx, cy]).or_default().push((a, b));
                }
            }
        }
        Self {
            verts,
            lines,
        }
    }

    /// Vertex positions within the axis-aligned `radius` box around `world`.
    pub fn verts_near(&self, world: [f32; 2], radius: f32) -> Vec<[f32; 2]> {
        let mut out = Vec::new();
        for cx in span(world[0] - radius, world[0] + radius) {
            for cy in span(world[1] - radius, world[1] + radius) {
                for &p in self.verts.get(&[cx, cy]).map_or(&[][..], Vec::as_slice) {
                    if (p[0] - world[0]).abs() <= radius && (p[1] - world[1]).abs() <= radius {
                        out.push(p);
                    }
                }
            }
        }
        out
    }

    /// Segments within `tol` of `world` (a segment spanning several visited cells may repeat; nearest-pick is unaffected).
    pub fn lines_near(&self, world: [f32; 2], tol: f32) -> Vec<Seg> {
        let mut out = Vec::new();
        for cx in span(world[0] - tol, world[0] + tol) {
            for cy in span(world[1] - tol, world[1] + tol) {
                for &(a, b) in self.lines.get(&[cx, cy]).map_or(&[][..], Vec::as_slice) {
                    if distance_to_segment(world, a, b) <= tol {
                        out.push((a, b));
                    }
                }
            }
        }
        out
    }
}

fn cell(x: f32, y: f32) -> [i32; 2] {
    [
        (x / SNAP_CELL).floor() as i32,
        (y / SNAP_CELL).floor() as i32,
    ]
}

fn span(lo: f32, hi: f32) -> RangeInclusive<i32> {
    ((lo / SNAP_CELL).floor() as i32)..=((hi / SNAP_CELL).floor() as i32)
}
