use crate::map_data::{is_subsector, subsector_index};
use crate::map_defs::{Node, Segment, SubSector};

/// Maximum number of vertices allowed in polygon clipping.
const MAX_CLIP_VERTICES: usize = 128;

/// Epsilon for vertex classification against a half-space (map units).
/// Vertices within this distance of the splitting line are classified as `On`,
/// preventing misclassification due to floating-point rounding.
const ON_EPSILON: f64 = 0.01;

/// Minimum polygon area (map units²) below which a polygon is discarded as
/// degenerate.
const MIN_AREA: f64 = 0.5;

/// Maximum distance (map units) for snapping a clipped polygon vertex to a
/// nearby segment endpoint. Even with f64 and corrected divlines, acute
/// divline angles can produce vertices ~1–2 units from the true endpoint.
const SNAP_DIST: f64 = 2.0;

/// World-bounds half-extent for the initial clipping polygon (map units).
const WORLD_SIZE: f64 = 32768.0;

/// Near-duplicate distance² threshold for cleanup.
const DEDUP_DIST_SQ: f64 = 0.01 * 0.01;

/// Collinear cross-product threshold for cleanup.
const COLLINEAR_THRESHOLD: f64 = 0.1;

/// Vertex classification relative to a half-space.
#[derive(Clone, Copy, PartialEq)]
enum Side {
    /// Vertex is on the front (inside) side of the splitting line.
    Front,
    /// Vertex is on the back (outside) side of the splitting line.
    Back,
    /// Vertex is within [`ON_EPSILON`] of the splitting line.
    On,
}

/// BSP splitting line used as a half-space clipper.
///
/// All arithmetic is f64 to avoid precision loss at Doom's map scales
/// (up to 32768 units). The line is defined by an origin `(x, y)` and a
/// direction vector `(dx, dy)`. The "inside" half-space is to the right of
/// the direction vector (positive signed-distance side).
#[derive(Debug, Clone, Copy)]
pub struct DivLine {
    /// X coordinate of the splitting line origin.
    pub x: f64,
    /// Y coordinate of the splitting line origin.
    pub y: f64,
    /// X component of the splitting line direction.
    pub dx: f64,
    /// Y component of the splitting line direction.
    pub dy: f64,
}

impl DivLine {
    /// Create a dividing line from a BSP node (f32 → f64 promotion).
    pub fn from_node(node: &Node) -> Self {
        Self {
            x: node.xy.x as f64,
            y: node.xy.y as f64,
            dx: node.delta.x as f64,
            dy: node.delta.y as f64,
        }
    }

    /// Signed perpendicular distance from `(px, py)` to this line.
    /// Positive = front (right of direction vector), negative = back (left).
    fn signed_distance(&self, px: f64, py: f64) -> f64 {
        let len_sq = self.dx * self.dx + self.dy * self.dy;
        if len_sq < 1e-20 {
            return 0.0;
        }
        let inv_len = 1.0 / len_sq.sqrt();
        let nx = self.dy * inv_len;
        let ny = -self.dx * inv_len;
        (px - self.x) * nx + (py - self.y) * ny
    }

    /// Classify a point relative to this splitting line.
    fn classify(&self, px: f64, py: f64) -> Side {
        let d = self.signed_distance(px, py);
        if d > ON_EPSILON {
            Side::Front
        } else if d < -ON_EPSILON {
            Side::Back
        } else {
            Side::On
        }
    }

    /// Compute the intersection point of this line with edge `a→b`.
    fn intersect_edge(&self, ax: f64, ay: f64, bx: f64, by: f64) -> (f64, f64) {
        let da = self.signed_distance(ax, ay);
        let db = self.signed_distance(bx, by);
        let denom = da - db;
        if denom.abs() < 1e-20 {
            return (ax, ay);
        }
        let t = da / denom;
        (ax + t * (bx - ax), ay + t * (by - ay))
    }
}

/// Clip a convex polygon against a sequence of half-space dividing lines
/// using Sutherland-Hodgman. Each clipper retains the "inside" (right-of-
/// direction) half-space. All arithmetic is f64.
fn clip_polygon(initial: &[(f64, f64)], clippers: &[DivLine]) -> Vec<(f64, f64)> {
    let mut poly: Vec<(f64, f64)> = initial.to_vec();

    for clipper in clippers {
        if poly.is_empty() || poly.len() >= MAX_CLIP_VERTICES {
            break;
        }

        let input = std::mem::take(&mut poly);
        let n = input.len();

        for i in 0..n {
            let (cx, cy) = input[i];
            let (nx, ny) = input[(i + 1) % n];
            let cs = clipper.classify(cx, cy);
            let ns = clipper.classify(nx, ny);

            match cs {
                Side::Front | Side::On => {
                    poly.push((cx, cy));
                    if ns == Side::Back {
                        poly.push(clipper.intersect_edge(cx, cy, nx, ny));
                    }
                }
                Side::Back => {
                    if ns == Side::Front || ns == Side::On {
                        poly.push(clipper.intersect_edge(cx, cy, nx, ny));
                    }
                }
            }
        }

        if poly.is_empty() {
            break;
        }
    }

    poly
}

/// Remove near-duplicate vertices, collinear vertices, and discard degenerate
/// polygons (area < [`MIN_AREA`] map units²).
fn cleanup_polygon(vertices: &mut Vec<(f64, f64)>) {
    if vertices.len() < 3 {
        vertices.clear();
        return;
    }

    // 1. Remove near-duplicate consecutive vertices.
    let mut i = 0;
    while i < vertices.len() && vertices.len() >= 3 {
        let prev = if i == 0 { vertices.len() - 1 } else { i - 1 };
        let dx = vertices[i].0 - vertices[prev].0;
        let dy = vertices[i].1 - vertices[prev].1;
        if dx * dx + dy * dy < DEDUP_DIST_SQ {
            vertices.remove(i);
        } else {
            i += 1;
        }
    }

    if vertices.len() < 3 {
        vertices.clear();
        return;
    }

    // 2. Remove collinear vertices.
    let mut i = 0;
    while i < vertices.len() && vertices.len() >= 3 {
        let n = vertices.len();
        let prev = if i == 0 { n - 1 } else { i - 1 };
        let next = (i + 1) % n;
        let (ax, ay) = vertices[prev];
        let (bx, by) = vertices[i];
        let (cx, cy) = vertices[next];
        let cross = (bx - ax) * (cy - by) - (by - ay) * (cx - bx);
        if cross.abs() < COLLINEAR_THRESHOLD {
            vertices.remove(i);
        } else {
            i += 1;
        }
    }

    if vertices.len() < 3 {
        vertices.clear();
        return;
    }

    // 3. Discard degenerate slivers by area.
    let n = vertices.len();
    let mut area = 0.0_f64;
    for i in 0..n {
        let j = (i + 1) % n;
        area += vertices[i].0 * vertices[j].1;
        area -= vertices[j].0 * vertices[i].1;
    }
    if area.abs() * 0.5 < MIN_AREA {
        vertices.clear();
    }
}

/// Snap polygon vertices to nearby segment endpoints. BSP half-space
/// intersections can produce vertices slightly offset from the true segment
/// endpoint; this corrects the drift.
fn snap_to_segment_endpoints(polygon: &mut [(f64, f64)], segments: &[Segment]) {
    for vertex in polygon.iter_mut() {
        let mut best_dist = SNAP_DIST;
        let mut best_pos = None;
        for segment in segments {
            for &seg_v in &[*segment.v1, *segment.v2] {
                let dx = vertex.0 - seg_v.x as f64;
                let dy = vertex.1 - seg_v.y as f64;
                let d = (dx * dx + dy * dy).sqrt();
                if d > 0.001 && d < best_dist {
                    best_dist = d;
                    best_pos = Some((seg_v.x as f64, seg_v.y as f64));
                }
            }
        }
        if let Some(pos) = best_pos {
            *vertex = pos;
        }
    }
}

/// Generate a convex polygon for a subsector by clipping against BSP divlines
/// and subsector segment boundaries.
///
/// Returns f64 polygon vertices. Caller converts to f32 at vertex storage.
pub fn carve_subsector_polygon(segments: &[Segment], divlines: &[DivLine]) -> Vec<(f64, f64)> {
    if divlines.is_empty() {
        return extract_sector_boundary_from_segments(segments);
    }

    let initial: [(f64, f64); 4] = [
        (-WORLD_SIZE, WORLD_SIZE),
        (WORLD_SIZE, WORLD_SIZE),
        (WORLD_SIZE, -WORLD_SIZE),
        (-WORLD_SIZE, -WORLD_SIZE),
    ];

    // BSP divlines (reversed order matches root-to-leaf accumulation).
    let mut clippers: Vec<DivLine> = divlines.iter().rev().copied().collect();

    // Subsector boundary segments tighten the polygon to actual wall edges.
    // With f64 + epsilon classification, collinear opposite-direction segments
    // are handled naturally (on-line vertices classify as On, not Back).
    for segment in segments {
        let dx = segment.v2.x as f64 - segment.v1.x as f64;
        let dy = segment.v2.y as f64 - segment.v1.y as f64;
        if dx.abs() + dy.abs() < 1e-6 {
            continue;
        }
        clippers.push(DivLine {
            x: segment.v1.x as f64,
            y: segment.v1.y as f64,
            dx,
            dy,
        });
    }

    let mut clipped = clip_polygon(&initial, &clippers);

    cleanup_polygon(&mut clipped);
    snap_to_segment_endpoints(&mut clipped, segments);

    clipped
}

/// Fallback for subsectors with zero BSP divlines: extract boundary from
/// segment endpoints sorted by angle.
fn extract_sector_boundary_from_segments(segments: &[Segment]) -> Vec<(f64, f64)> {
    if segments.is_empty() {
        return Vec::new();
    }

    let mut all_vertices: Vec<(f64, f64)> = Vec::new();
    const EPSILON: f64 = 0.001;

    for segment in segments {
        let v1 = (segment.v1.x as f64, segment.v1.y as f64);
        let v2 = (segment.v2.x as f64, segment.v2.y as f64);

        if !all_vertices
            .iter()
            .any(|v| (v.0 - v1.0).abs() + (v.1 - v1.1).abs() < EPSILON)
        {
            all_vertices.push(v1);
        }
        if !all_vertices
            .iter()
            .any(|v| (v.0 - v2.0).abs() + (v.1 - v2.1).abs() < EPSILON)
        {
            all_vertices.push(v2);
        }
    }

    if all_vertices.len() < 3 {
        return Vec::new();
    }

    let n = all_vertices.len() as f64;
    let cx = all_vertices.iter().map(|v| v.0).sum::<f64>() / n;
    let cy = all_vertices.iter().map(|v| v.1).sum::<f64>() / n;

    all_vertices.sort_by(|a, b| {
        let angle_a = (a.1 - cy).atan2(a.0 - cx);
        let angle_b = (b.1 - cy).atan2(b.0 - cx);
        angle_a
            .partial_cmp(&angle_b)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    all_vertices
}

/// Traverse the BSP tree and return the carved 2D convex polygon for each
/// subsector. Produces f64 polygons indexed by subsector ID.
pub fn carve_subsector_polygons_2d(
    root_node: u32,
    nodes: &[Node],
    subsectors: &[SubSector],
    segments: &[Segment],
) -> Vec<Vec<(f64, f64)>> {
    let mut result = vec![Vec::new(); subsectors.len()];
    carve_2d_recursive(
        root_node,
        nodes,
        subsectors,
        segments,
        Vec::new(),
        &mut result,
    );
    result
}

/// Recursive BSP traversal for 2D polygon carving.
fn carve_2d_recursive(
    node_id: u32,
    nodes: &[Node],
    subsectors: &[SubSector],
    segments: &[Segment],
    divlines: Vec<DivLine>,
    result: &mut [Vec<(f64, f64)>],
) {
    if is_subsector(node_id) {
        if node_id == u32::MAX {
            return;
        }
        let subsector_id = subsector_index(node_id);
        if subsector_id < subsectors.len() {
            let ss = &subsectors[subsector_id];
            let start = ss.start_seg as usize;
            let end = start + ss.seg_count as usize;
            if let Some(ss_segments) = segments.get(start..end) {
                result[subsector_id] = carve_subsector_polygon(ss_segments, &divlines);
            }
        }
    } else if let Some(node) = nodes.get(node_id as usize) {
        let node_divline = DivLine::from_node(node);

        let mut right_divlines = divlines.clone();
        right_divlines.push(node_divline);
        carve_2d_recursive(
            node.children[0],
            nodes,
            subsectors,
            segments,
            right_divlines,
            result,
        );

        let mut left_divlines = divlines;
        left_divlines.push(DivLine {
            dx: -node_divline.dx,
            dy: -node_divline.dy,
            ..node_divline
        });
        carve_2d_recursive(
            node.children[1],
            nodes,
            subsectors,
            segments,
            left_divlines,
            result,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{DivLine, clip_polygon};

    #[test]
    fn test_polygon_clipping() {
        let square: [(f64, f64); 4] = [(-5.0, -5.0), (5.0, -5.0), (5.0, 5.0), (-5.0, 5.0)];

        let clipper = DivLine {
            x: 0.0,
            y: -10.0,
            dx: 0.0,
            dy: 20.0,
        };

        let clipped = clip_polygon(&square, &[clipper]);

        assert!(
            clipped.len() >= 3,
            "Should have at least 3 vertices, got {}",
            clipped.len()
        );

        for (i, vertex) in clipped.iter().enumerate() {
            assert!(
                vertex.0 >= -1e-5,
                "Vertex {}: x={} should be >= 0",
                i,
                vertex.0
            );
        }
    }
}
