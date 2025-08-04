use crate::{Node, Segment, SubSector};
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

    // Remove consecutive identical points
    let mut i = 0;
    while i < vertices.len() {
        let prev_idx = if i == 0 { vertices.len() - 1 } else { i - 1 };
        if (vertices[i] - vertices[prev_idx]).length_squared() < 1e-6 {
            vertices.remove(i);
        } else {
            i += 1;
        }
    }

    vertices
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
    let clipped_points = clip_polygon_with_divlines(edge_points, &clippers);

    // 5. Add missing segment vertices that lie on clipped polygon edges
    let final_polygon = add_missing_edge_vertices(
        &clipped_points,
        segments,
        sector_subsectors,
        all_segments,
        all_subsectors,
    );

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
    all_subsectors: &[crate::level::map_defs::SubSector],
) -> Vec<Vec2> {
    if polygon.len() < 3 {
        return polygon.to_vec();
    }

    let mut result = polygon.to_vec();
    const EPSILON: f32 = 0.001;

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
                if check_vertex_on_edge(
                    seg_vertex,
                    edge_start,
                    edge_vector,
                    edge_length_sq,
                    &result,
                    EPSILON,
                ) {
                    edge_vertices.push(seg_vertex);
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
                            if check_vertex_on_edge(
                                seg_vertex,
                                edge_start,
                                edge_vector,
                                edge_length_sq,
                                &result,
                                EPSILON,
                            ) {
                                edge_vertices.push(seg_vertex);
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

        // Add the sorted edge vertices (excluding the start vertex which is already in
        // result)
        for &vertex in &edge_vertices[1..] {
            if !result.iter().any(|&v| (v - vertex).length() < EPSILON) {
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

fn check_vertex_on_edge(
    seg_vertex: Vec2,
    edge_start: Vec2,
    edge_vector: Vec2,
    edge_length_sq: f32,
    result: &[Vec2],
    epsilon: f32,
) -> bool {
    // Skip if vertex is already in the result polygon
    if result.iter().any(|&v| (v - seg_vertex).length() < epsilon) {
        return false;
    }

    // Check if segment vertex lies on the edge
    let to_vertex = seg_vertex - edge_start;
    let projection = to_vertex.dot(edge_vector) / edge_length_sq;

    // Check if the projection is within the edge bounds
    if projection >= -epsilon && projection <= 1.0 + epsilon {
        // Check if the vertex is actually on the line (not just on the extended line)
        let projected_point = edge_start + edge_vector * projection;
        let distance_to_line = (seg_vertex - projected_point).length();

        distance_to_line < epsilon
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use glam::Vec2;

    use crate::level::triangulation::{carve_subsector_polygon, clip_polygon_with_divlines};
    use crate::{DivLine, Segment};

    impl DivLine {
        /// Create a dividing line from a segment
        fn from_segment(seg: &Segment) -> Self {
            Self {
                x: seg.v1.x,
                y: seg.v1.y,
                dx: seg.linedef.delta.x,
                dy: seg.linedef.delta.y,
            }
        }
    }

    #[test]
    fn test_e1m2_polygon_generation() {
        use crate::{MapData, PicData};
        use wad::WadData;

        let wad = WadData::new(&PathBuf::from("../doom1.wad"));
        let mut map = MapData::default();
        map.load("E1M2", &PicData::init(&wad), &wad);

        // Find subsector for Sector 109 (lift)
        let target_sector = 109;
        let mut found_subsector = None;

        for (subsector_id, subsector) in map.subsectors().iter().enumerate() {
            if subsector.sector.num == target_sector {
                found_subsector = Some((subsector_id, subsector));
                break;
            }
        }

        if let Some((subsector_id, subsector)) = found_subsector {
            let start_seg = subsector.start_seg as usize;
            let end_seg = start_seg + subsector.seg_count as usize;

            println!(
                "Found sector {} in subsector {}",
                target_sector, subsector_id
            );
            println!("Segment range: {} to {}", start_seg, end_seg);

            if let Some(segments) = map.segments().get(start_seg..end_seg) {
                println!(
                    "Processing {} segments for sector {}:",
                    segments.len(),
                    target_sector
                );
                for (i, segment) in segments.iter().enumerate() {
                    println!(
                        "  Segment {}: ({:.3}, {:.3}) -> ({:.3}, {:.3})",
                        i, segment.v1.x, segment.v1.y, segment.v2.x, segment.v2.y
                    );
                }

                let polygon = carve_subsector_polygon(segments, &[], &[], &[], &[]);

                println!(
                    "Sector {} subsector {} polygon vertices: {}",
                    target_sector,
                    subsector_id,
                    polygon.len()
                );
                for (i, vertex) in polygon.iter().enumerate() {
                    println!("  Vertex {}: ({:.3}, {:.3})", i, vertex.x, vertex.y);
                }

                println!("Expected vertices for sector {}:", target_sector);
                println!("  Right side: (-128, 448), (-128, 424), (-128, 384)");
                println!("  Left side: (-256, 448), (-256, 424), (-256, 384)");

                // Should generate a polygon with vertices from the segments
                assert!(
                    polygon.len() >= 3,
                    "Should have at least 3 vertices for triangulation"
                );
            }
        } else {
            panic!("Could not find subsector for sector {}", target_sector);
        }

        // Also test sector 24 linedef 421 to debug missing triangles - find ALL
        // subsectors
        let target_sector_24 = 24;
        let mut found_subsectors_24 = Vec::new();

        for (subsector_id, subsector) in map.subsectors().iter().enumerate() {
            if subsector.sector.num == target_sector_24 {
                found_subsectors_24.push((subsector_id, subsector));
            }
        }

        if !found_subsectors_24.is_empty() {
            println!("\n=== DEBUG SECTOR 24 - ALL SUBSECTORS ===");
            println!(
                "Found {} subsectors for sector {}",
                found_subsectors_24.len(),
                target_sector_24
            );

            let mut total_segments = 0;
            for (subsector_id, subsector) in &found_subsectors_24 {
                let start_seg = subsector.start_seg as usize;
                let end_seg = start_seg + subsector.seg_count as usize;
                total_segments += subsector.seg_count as usize;

                println!(
                    "  Subsector {}: segments {} to {} ({} segments)",
                    subsector_id, start_seg, end_seg, subsector.seg_count
                );

                if let Some(segments) = map.segments().get(start_seg..end_seg) {
                    for (i, segment) in segments.iter().enumerate() {
                        println!(
                            "    Segment {}: ({:.3}, {:.3}) -> ({:.3}, {:.3})",
                            i, segment.v1.x, segment.v1.y, segment.v2.x, segment.v2.y
                        );
                    }
                }
            }

            println!(
                "Total segments across all subsectors for sector {}: {}",
                target_sector_24, total_segments
            );
        } else {
            println!(
                "Could not find any subsectors for sector {}",
                target_sector_24
            );
        }
    }

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

    #[ignore = "Requires registered DOOM"]
    #[test]
    fn test_polygon_generation_e4m7() {
        use crate::{MapData, PicData};
        use wad::WadData;

        let wad = WadData::new(&PathBuf::from("/Users/lukejones/DOOM/doom.wad"));
        let mut map = MapData::default();
        map.load("E4M7", &PicData::init(&wad), &wad);

        // Test carve_subsector_polygon directly on the first subsector
        if let Some(subsector) = map.subsectors().get(0) {
            let start_seg = subsector.start_seg as usize;
            let end_seg = start_seg + subsector.seg_count as usize;

            if let Some(segments) = map.segments().get(start_seg..end_seg) {
                // Test basic polygon generation
                let polygon_no_divlines = carve_subsector_polygon(segments, &[], &[], &[], &[]);
                assert!(
                    !polygon_no_divlines.is_empty(),
                    "Polygon should not be empty"
                );

                // Test with one simple divline
                let simple_divline = DivLine {
                    x: 0.0,
                    y: 0.0,
                    dx: 1.0,
                    dy: 0.0,
                };
                let polygon_one_divline =
                    carve_subsector_polygon(segments, &[simple_divline], &[], &[], &[]);
                assert!(
                    !polygon_one_divline.is_empty(),
                    "Polygon should not be empty with one divline"
                );

                // Test with the actual segments as divlines
                let mut segment_divlines = Vec::new();
                for segment in segments {
                    segment_divlines.push(DivLine::from_segment(segment));
                }
                let polygon_with_segments =
                    carve_subsector_polygon(segments, &segment_divlines, &[], &[], &[]);
                assert!(
                    !polygon_with_segments.is_empty(),
                    "Polygon should not be empty with segment divlines"
                );
            }
        }
    }
}
