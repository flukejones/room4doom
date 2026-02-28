use crate::map_defs::{Node, Segment, SubSector};
use glam::Vec2;

/// Maximum number of vertices allowed in polygon clipping
const MAX_CLIP_VERTICES: usize = 128;

/// Dividing line for BSP operations
#[derive(Debug, Clone, Copy)]
pub struct DivLine {
    pub x: f32,
    pub y: f32,
    pub dx: f32,
    pub dy: f32,
}

impl DivLine {
    /// Create a dividing line from a BSP node
    pub fn from_node(node: &Node) -> Self {
        Self {
            x: node.xy.x,
            y: node.xy.y,
            dx: node.delta.x,
            dy: node.delta.y,
        }
    }

    /// Determine which side of the line a point is on
    /// Returns true if point is on the right side (positive side)
    fn point_on_side(&self, point: Vec2) -> bool {
        let cross = (point.y - self.y) * self.dx - (point.x - self.x) * self.dy;
        cross >= 0.0
    }

    /// Calculate intersection point between this line and a line segment
    /// Lines must intersect - this doesn't check for parallel lines
    fn calc_intersection(&self, seg_start: Vec2, seg_end: Vec2) -> Vec2 {
        let ax = seg_start.x as f64;
        let ay = seg_start.y as f64;
        let bx = seg_end.x as f64;
        let by = seg_end.y as f64;
        let cx = self.x as f64;
        let cy = self.y as f64;
        let dx = cx + self.dx as f64;
        let dy = cy + self.dy as f64;

        let denominator = (bx - ax) * (dy - cy) - (by - ay) * (dx - cx);

        // Check for parallel lines (should not happen in practice)
        if denominator.abs() < 1e-10 {
            return seg_start; // Return original point if parallel
        }

        let r = ((ay - cy) * (dx - cx) - (ax - cx) * (dy - cy)) / denominator;

        Vec2::new((ax + r * (bx - ax)) as f32, (ay + r * (by - ay)) as f32)
    }
}

/// Clip a polygon against a set of dividing lines using Sutherland-Hodgman
/// algorithm Similar to prboom's gld_FlatEdgeClipper
fn clip_polygon_with_divlines(mut vertices: Vec<Vec2>, clippers: &[DivLine]) -> Vec<Vec2> {
    for clipper in clippers {
        if vertices.is_empty() || vertices.len() >= MAX_CLIP_VERTICES {
            break;
        }

        let input_vertices = vertices.clone();
        vertices.clear();

        if input_vertices.is_empty() {
            continue;
        }

        // Process each edge using correct Sutherland-Hodgman clipping
        for i in 0..input_vertices.len() {
            let current_vertex = input_vertices[i];
            let next_vertex = input_vertices[(i + 1) % input_vertices.len()];

            let current_inside = !clipper.point_on_side(current_vertex);
            let next_inside = !clipper.point_on_side(next_vertex);

            match (current_inside, next_inside) {
                (true, true) => {
                    // Both inside: add next vertex
                    vertices.push(next_vertex);
                }
                (true, false) => {
                    // Current inside, next outside: add intersection only
                    let intersection = clipper.calc_intersection(current_vertex, next_vertex);
                    vertices.push(intersection);
                }
                (false, true) => {
                    // Current outside, next inside: add intersection then next vertex
                    let intersection = clipper.calc_intersection(current_vertex, next_vertex);
                    vertices.push(intersection);
                    vertices.push(next_vertex);
                }
                (false, false) => {
                    // Both outside: add nothing
                }
            }
        }

        if vertices.len() >= MAX_CLIP_VERTICES {
            break;
        }

        if vertices.is_empty() {
            break;
        }
    }

    cleanup_polygon(&mut vertices);
    vertices
}

/// Remove near-duplicate vertices, collinear vertices, and discard degenerate
/// polygons (area < 0.5 map units²). Applied after Sutherland-Hodgman clipping.
fn cleanup_polygon(vertices: &mut Vec<Vec2>) {
    const DIST_SQ: f32 = 0.01 * 0.01;
    const COLLINEAR: f32 = 0.1;
    const MIN_AREA: f32 = 0.5;

    if vertices.len() < 3 {
        vertices.clear();
        return;
    }

    // 1. Remove near-duplicate consecutive vertices
    let mut i = 0;
    while i < vertices.len() && vertices.len() >= 3 {
        let prev = if i == 0 { vertices.len() - 1 } else { i - 1 };
        if (vertices[i] - vertices[prev]).length_squared() < DIST_SQ {
            vertices.remove(i);
        } else {
            i += 1;
        }
    }

    if vertices.len() < 3 {
        vertices.clear();
        return;
    }

    // 2. Remove collinear vertices (cross product of adjacent edges ≈ 0)
    let mut i = 0;
    while i < vertices.len() && vertices.len() >= 3 {
        let n = vertices.len();
        let prev = if i == 0 { n - 1 } else { i - 1 };
        let next = (i + 1) % n;
        let a = vertices[prev];
        let b = vertices[i];
        let c = vertices[next];
        let cross = (b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x);
        if cross.abs() < COLLINEAR {
            vertices.remove(i);
        } else {
            i += 1;
        }
    }

    if vertices.len() < 3 {
        vertices.clear();
        return;
    }

    // 3. Discard degenerate slivers by area
    if vertices.len() >= 3 {
        let n = vertices.len();
        let mut area = 0.0_f32;
        for i in 0..n {
            let j = (i + 1) % n;
            area += vertices[i].x * vertices[j].y;
            area -= vertices[j].x * vertices[i].y;
        }
        if area.abs() * 0.5 < MIN_AREA {
            vertices.clear();
        }
    }
}

/// Generate a convex polygon for a subsector by clipping against BSP divlines
/// Similar to prboom's gld_FlatConvexCarver
pub fn carve_subsector_polygon(
    segments: &[Segment],
    divlines: &[DivLine],
    sector_subsectors: &[Vec<usize>],
    all_segments: &[Segment],
    all_subsectors: &[SubSector],
) -> Vec<Vec2> {
    if divlines.is_empty() {
        // No BSP divlines - extract sector boundary directly from segments
        return extract_sector_boundary_from_segments(segments);
    }

    // Prboom-compatible approach: combine BSP divlines + subsector segments as
    // clippers
    let mut clippers = Vec::new();

    // 1. Add BSP divlines (reversed order like prboom)
    for divline in divlines.iter().rev() {
        clippers.push(*divline);
    }

    // 2. Add subsector boundary segments as clippers
    for segment in segments {
        clippers.push(DivLine {
            x: segment.v1.x,
            y: segment.v1.y,
            dx: segment.v2.x - segment.v1.x,
            dy: segment.v2.y - segment.v1.y,
        });
    }

    // 3. Start with world-sized polygon (like prboom's gld_FlatConvexCarver)
    let world_size = 32768.0;
    let edge_points = vec![
        Vec2::new(-world_size, world_size),
        Vec2::new(world_size, world_size),
        Vec2::new(world_size, -world_size),
        Vec2::new(-world_size, -world_size),
    ];

    // 4. Clip polygon against all divlines (BSP + subsector boundaries)
    let mut clipped_points = clip_polygon_with_divlines(edge_points, &clippers);

    // 4b. Fix clipped vertices to authoritative segment positions.
    // Sutherland-Hodgman clipping can drift vertices up to ~2 units from
    // where they should be. For each polygon edge, check if it lies along
    // a local segment by verifying: (a) the edge direction is parallel to
    // the segment, (b) one vertex is near-exact to a segment endpoint, and
    // (c) the other vertex is within tolerance of the other endpoint. If
    // all three hold, replace both polygon vertices with exact segment
    // endpoints.
    const NEAR_EXACT_SQ: f32 = 0.2 * 0.2;
    const DRIFT_TOLERANCE_SQ: f32 = 2.0 * 2.0;
    const PARALLEL_EPSILON: f32 = 0.02;

    let n = clipped_points.len();
    if n >= 3 {
        let mut snapped = vec![false; n];
        for i in 0..n {
            let j = (i + 1) % n;
            let pi = clipped_points[i];
            let pj = clipped_points[j];
            let edge = pj - pi;
            let edge_len_sq = edge.length_squared();
            if edge_len_sq < 1e-6 {
                continue;
            }

            for segment in segments.iter() {
                let sv1 = *segment.v1;
                let sv2 = *segment.v2;
                let seg_dir = sv2 - sv1;
                let seg_len_sq = seg_dir.length_squared();
                if seg_len_sq < 1e-6 {
                    continue;
                }

                // Check parallelism via normalised cross product
                let cross = edge.x * seg_dir.y - edge.y * seg_dir.x;
                let cross_normalised = cross / (edge_len_sq.sqrt() * seg_len_sq.sqrt());
                if cross_normalised.abs() > PARALLEL_EPSILON {
                    continue;
                }

                // Check both orientations: edge may be same or opposite
                // direction as segment.
                // Try: pi ↔ sv1, pj ↔ sv2
                let d_i_v1 = (pi - sv1).length_squared();
                let d_j_v2 = (pj - sv2).length_squared();
                if (d_i_v1 < NEAR_EXACT_SQ && d_j_v2 < DRIFT_TOLERANCE_SQ)
                    || (d_j_v2 < NEAR_EXACT_SQ && d_i_v1 < DRIFT_TOLERANCE_SQ)
                {
                    if !snapped[i] && !breaks_convexity(&clipped_points, i, sv1) {
                        clipped_points[i] = sv1;
                        snapped[i] = true;
                    }
                    if !snapped[j] && !breaks_convexity(&clipped_points, j, sv2) {
                        clipped_points[j] = sv2;
                        snapped[j] = true;
                    }
                    break;
                }

                // Try: pi ↔ sv2, pj ↔ sv1
                let d_i_v2 = (pi - sv2).length_squared();
                let d_j_v1 = (pj - sv1).length_squared();
                if (d_i_v2 < NEAR_EXACT_SQ && d_j_v1 < DRIFT_TOLERANCE_SQ)
                    || (d_j_v1 < NEAR_EXACT_SQ && d_i_v2 < DRIFT_TOLERANCE_SQ)
                {
                    if !snapped[i] && !breaks_convexity(&clipped_points, i, sv2) {
                        clipped_points[i] = sv2;
                        snapped[i] = true;
                    }
                    if !snapped[j] && !breaks_convexity(&clipped_points, j, sv1) {
                        clipped_points[j] = sv1;
                        snapped[j] = true;
                    }
                    break;
                }
            }
        }

        // Remove vertices that were snapped to the same position as a neighbor
        let mut deduped = Vec::with_capacity(clipped_points.len());
        for i in 0..clipped_points.len() {
            let prev = if i == 0 {
                clipped_points.len() - 1
            } else {
                i - 1
            };
            if (clipped_points[i] - clipped_points[prev]).length_squared() >= NEAR_EXACT_SQ {
                deduped.push(clipped_points[i]);
            }
        }
        clipped_points = deduped;

        // Discard polygon if snapping made it degenerate
        if clipped_points.len() < 3 {
            return Vec::new();
        }
        let mut area = 0.0_f32;
        for i in 0..clipped_points.len() {
            let j = (i + 1) % clipped_points.len();
            area += clipped_points[i].x * clipped_points[j].y;
            area -= clipped_points[j].x * clipped_points[i].y;
        }
        if area.abs() * 0.5 < 0.5 {
            return Vec::new();
        }
    }

    // 5. Add missing segment vertices that lie on clipped polygon edges
    let mut final_polygon = add_missing_edge_vertices(
        &clipped_points,
        segments,
        sector_subsectors,
        all_segments,
        all_subsectors,
    );

    // 6. Remove collinear vertices introduced by drift correction (4b) and
    // edge insertion (5). Drift snaps to integer segment positions while
    // the polygon was carved in float space, so cross products of
    // near-collinear vertices can be non-zero due to precision.
    cleanup_polygon(&mut final_polygon);

    final_polygon
}

fn extract_sector_boundary_from_segments(segments: &[Segment]) -> Vec<Vec2> {
    if segments.is_empty() {
        return Vec::new();
    }

    // Collect all unique vertices from segment endpoints
    let mut all_vertices = Vec::new();
    const EPSILON: f32 = 0.001;

    for segment in segments {
        let v1 = *segment.v1;
        let v2 = *segment.v2;

        if !all_vertices
            .iter()
            .any(|v: &Vec2| (*v - v1).length() < EPSILON)
        {
            all_vertices.push(v1);
        }
        if !all_vertices
            .iter()
            .any(|v: &Vec2| (*v - v2).length() < EPSILON)
        {
            all_vertices.push(v2);
        }
    }

    if all_vertices.len() < 3 {
        return Vec::new();
    }

    // Sort vertices by angle from centroid to create proper winding order
    let centroid =
        all_vertices.iter().fold(Vec2::ZERO, |acc, &v| acc + v) / all_vertices.len() as f32;

    all_vertices.sort_by(|&a, &b| {
        let angle_a = (a - centroid).y.atan2((a - centroid).x);
        let angle_b = (b - centroid).y.atan2((b - centroid).x);
        angle_a
            .partial_cmp(&angle_b)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    all_vertices
}

/// Add missing segment vertices that lie on polygon edges
fn add_missing_edge_vertices(
    polygon: &[Vec2],
    segments: &[Segment],
    sector_subsectors: &[Vec<usize>],
    all_segments: &[Segment],
    all_subsectors: &[SubSector],
) -> Vec<Vec2> {
    if polygon.len() < 3 {
        return polygon.to_vec();
    }

    let mut result = polygon.to_vec();
    const EPSILON: f32 = 0.001;
    // Wider epsilon for duplicate detection — drift correction can snap
    // vertices to positions that differ by more than EPSILON
    const DEDUP_EPSILON: f32 = 0.1;

    // For each polygon edge, check if any segment vertices lie on it
    for i in 0..polygon.len() {
        let edge_start = polygon[i];
        let edge_end = polygon[(i + 1) % polygon.len()];
        let edge_vector = edge_end - edge_start;
        let edge_length_sq = edge_vector.length_squared();

        if edge_length_sq < EPSILON * EPSILON {
            continue; // Skip degenerate edges
        }

        let mut edge_vertices = vec![edge_start];
        let mut checked_segments = std::collections::HashSet::new();

        // Check vertices from local subsector segments
        for (segment_idx, segment) in segments.iter().enumerate() {
            checked_segments.insert(segment_idx);
            for seg_vertex in [*segment.v1, *segment.v2] {
                if let Some(projected) = check_and_project_vertex_on_edge(
                    seg_vertex,
                    edge_start,
                    edge_vector,
                    edge_length_sq,
                    &result,
                    EPSILON,
                    DEDUP_EPSILON,
                ) {
                    edge_vertices.push(projected);
                }
            }
        }

        // Check vertices from backsector segments
        for segment in segments {
            if let Some(backsector) = &segment.backsector {
                let backsector_id = backsector.num as usize;
                for &subsector_id in &sector_subsectors[backsector_id] {
                    let subsector = &all_subsectors[subsector_id];
                    let start_seg = subsector.start_seg as usize;
                    let end_seg = start_seg + subsector.seg_count as usize;

                    for global_idx in start_seg..end_seg {
                        if checked_segments.contains(&global_idx) {
                            continue;
                        }
                        let global_segment = &all_segments[global_idx];
                        checked_segments.insert(global_idx);
                        for seg_vertex in [*global_segment.v1, *global_segment.v2] {
                            if let Some(projected) = check_and_project_vertex_on_edge(
                                seg_vertex,
                                edge_start,
                                edge_vector,
                                edge_length_sq,
                                &result,
                                EPSILON,
                                DEDUP_EPSILON,
                            ) {
                                edge_vertices.push(projected);
                            }
                        }
                    }
                }
            }
        }

        // Sort vertices along the edge by their projection parameter
        edge_vertices.sort_by(|&a, &b| {
            let proj_a = (a - edge_start).dot(edge_vector) / edge_length_sq;
            let proj_b = (b - edge_start).dot(edge_vector) / edge_length_sq;
            proj_a
                .partial_cmp(&proj_b)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Deduplicate sorted edge vertices (drift correction can create
        // near-duplicates)
        edge_vertices.dedup_by(|a, b| (*a - *b).length() < DEDUP_EPSILON);

        // Add the sorted edge vertices (excluding the start vertex which is already in
        // result)
        for &vertex in &edge_vertices[1..] {
            if !result
                .iter()
                .any(|&v| (v - vertex).length() < DEDUP_EPSILON)
            {
                // Find the correct insertion point in the result polygon
                let insert_pos = result
                    .iter()
                    .position(|&v| (v - edge_end).length() < EPSILON)
                    .unwrap_or(result.len());
                result.insert(insert_pos, vertex);
            }
        }
    }

    result
}

/// Check if a segment vertex lies on a polygon edge. If so, return the vertex
/// projected exactly onto the edge line to guarantee collinearity.
fn check_and_project_vertex_on_edge(
    seg_vertex: Vec2,
    edge_start: Vec2,
    edge_vector: Vec2,
    edge_length_sq: f32,
    result: &[Vec2],
    epsilon: f32,
    dedup_epsilon: f32,
) -> Option<Vec2> {
    // Skip if vertex is already in the result polygon (wider tolerance for drift)
    if result
        .iter()
        .any(|&v| (v - seg_vertex).length() < dedup_epsilon)
    {
        return None;
    }

    // Check if segment vertex lies on the edge
    let to_vertex = seg_vertex - edge_start;
    let projection = to_vertex.dot(edge_vector) / edge_length_sq;

    // Check if the projection is within the edge bounds
    if projection >= -epsilon && projection <= 1.0 + epsilon {
        let projected_point = edge_start + edge_vector * projection;
        let distance_to_line = (seg_vertex - projected_point).length();

        if distance_to_line < epsilon {
            // Return the projected point — exactly on the edge line
            Some(projected_point)
        } else {
            None
        }
    } else {
        None
    }
}

/// Check if moving `polygon[idx]` to `new_pos` would flip the winding at
/// that vertex or its neighbors. The polygon from S-H is convex (CW in
/// Doom's coordinate system, negative cross products). A snap that produces
/// a positive cross product breaks convexity.
fn breaks_convexity(polygon: &[Vec2], idx: usize, new_pos: Vec2) -> bool {
    let n = polygon.len();
    if n < 3 {
        return false;
    }
    // Check the three triplets that include the moved vertex:
    // (prev, idx, next), (prev2, prev, idx), (idx, next, next2)
    let prev = if idx == 0 { n - 1 } else { idx - 1 };
    let next = (idx + 1) % n;
    let prev2 = if prev == 0 { n - 1 } else { prev - 1 };
    let next2 = (next + 1) % n;

    // Determine original winding from first non-degenerate triplet
    let mut winding_sign = 0.0_f32;
    for i in 0..n {
        let a = polygon[i];
        let b = polygon[(i + 1) % n];
        let c = polygon[(i + 2) % n];
        let cross = (b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x);
        if cross.abs() > 1e-4 {
            winding_sign = cross.signum();
            break;
        }
    }
    if winding_sign == 0.0 {
        return false;
    }

    let triplets = [
        (polygon[prev2], polygon[prev], new_pos),
        (polygon[prev], new_pos, polygon[next]),
        (new_pos, polygon[next], polygon[next2]),
    ];
    for (a, b, c) in triplets {
        let cross = (b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x);
        // If the cross product flips sign (beyond noise), convexity is broken
        if cross * winding_sign < -1e-4 {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use glam::Vec2;

    use super::{DivLine, clip_polygon_with_divlines};

    // NOTE: WAD-loading tests (test_e1m2_polygon_generation,
    // test_polygon_generation_e4m7) remain in gameplay crate as integration
    // tests since they need PicData.

    #[test]
    fn test_polygon_clipping() {
        let square = vec![
            Vec2::new(-5.0, -5.0),
            Vec2::new(5.0, -5.0),
            Vec2::new(5.0, 5.0),
            Vec2::new(-5.0, 5.0),
        ];

        // Clip with a vertical line at x=0 (keep right side)
        // Note: DivLine direction matters - we want to keep the right side
        let clipper = DivLine {
            x: 0.0,
            y: -10.0,
            dx: 0.0,
            dy: 20.0,
        };

        let clipped = clip_polygon_with_divlines(square.clone(), &[clipper]);

        // Should result in a rectangle on the right side
        assert!(
            clipped.len() >= 3,
            "Should have at least 3 vertices, got {}",
            clipped.len()
        );

        // All x coordinates should be >= 0 (approximately)
        for (i, vertex) in clipped.iter().enumerate() {
            assert!(
                vertex.x >= -1e-5,
                "Vertex {}: x={} should be >= 0",
                i,
                vertex.x
            );
        }
    }
}
