use crate::Node;
use crate::level::map_defs::Segment;
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

    /// Create a dividing line from a segment
    fn from_segment(seg: &Segment) -> Self {
        Self {
            x: seg.v1.x,
            y: seg.v1.y,
            dx: seg.v2.x - seg.v1.x,
            dy: seg.v2.y - seg.v1.y,
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
        let ax = seg_start.x;
        let ay = seg_start.y;
        let bx = seg_end.x;
        let by = seg_end.y;
        let cx = self.x;
        let cy = self.y;
        let dx = cx + self.dx;
        let dy = cy + self.dy;

        let r = ((ay - cy) * (dx - cx) - (ax - cx) * (dy - cy))
            / ((bx - ax) * (dy - cy) - (by - ay) * (dx - cx));

        Vec2::new(
            seg_start.x + r * (seg_end.x - seg_start.x),
            seg_start.y + r * (seg_end.y - seg_start.y),
        )
    }
}

/// Clip a polygon against a set of dividing lines using Sutherland-Hodgman
/// algorithm Similar to prboom's gld_FlatEdgeClipper
fn clip_polygon_with_divlines(mut vertices: Vec<Vec2>, clippers: &[DivLine]) -> Vec<Vec2> {
    // Clip the polygon with each dividing line
    // The left side of each divline is discarded (following prboom convention)
    for clipper in clippers {
        if vertices.is_empty() || vertices.len() >= MAX_CLIP_VERTICES {
            break;
        }

        let input_vertices = vertices.clone();
        vertices.clear();

        if input_vertices.is_empty() {
            continue;
        }

        // Process each edge using Sutherland-Hodgman clipping
        for i in 0..input_vertices.len() {
            let current_vertex = input_vertices[i];
            let next_vertex = input_vertices[(i + 1) % input_vertices.len()];

            let current_inside = !clipper.point_on_side(current_vertex); // Invert: keep right side
            let next_inside = !clipper.point_on_side(next_vertex); // Invert: keep right side

            if next_inside {
                // Next vertex is inside
                if !current_inside {
                    // Current vertex is outside, next is inside - add intersection
                    let intersection = clipper.calc_intersection(current_vertex, next_vertex);
                    vertices.push(intersection);
                }
                // Add the next vertex (it's inside)
                vertices.push(next_vertex);
            } else if current_inside {
                // Current vertex is inside, next is outside - add intersection
                let intersection = clipper.calc_intersection(current_vertex, next_vertex);
                vertices.push(intersection);
            }
            // If both vertices are outside, add nothing
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
pub fn carve_subsector_polygon(segments: &[Segment], divlines: &[DivLine]) -> Vec<Vec2> {
    // Create clippers from both the BSP divlines and the subsector's segments
    let mut clippers = Vec::with_capacity(divlines.len() + segments.len());

    // Add BSP divlines in reverse order (following prboom convention)
    for divline in divlines.iter().rev() {
        clippers.push(*divline);
    }

    // Add segment divlines
    for segment in segments {
        clippers.push(DivLine::from_segment(segment));
    }

    // Start with a large "worldwide" polygon
    // Use reasonable bounds based on typical Doom map sizes
    let world_size = 32768.0;
    let mut edge_points = vec![
        Vec2::new(-world_size, world_size),  // top-left
        Vec2::new(world_size, world_size),   // top-right
        Vec2::new(world_size, -world_size),  // bottom-right
        Vec2::new(-world_size, -world_size), // bottom-left
    ];

    // Clip the world polygon against all the dividing lines
    edge_points = clip_polygon_with_divlines(edge_points, &clippers);

    edge_points
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use glam::Vec2;

    use crate::DivLine;
    use crate::level::triangulation::{carve_subsector_polygon, clip_polygon_with_divlines};

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

    #[test]
    fn test_polygon_generation() {
        use crate::{MapData, PicData};
        use wad::WadData;

        let mut wad = WadData::new(&PathBuf::from("../doom1.wad"));
        wad.add_file("../pvs_test_u_zig.wad".into());
        let mut map = MapData::default();
        map.load("E1M1", &PicData::default(), &wad);

        // Test carve_subsector_polygon directly on the first subsector
        if let Some(subsector) = map.subsectors().get(0) {
            let start_seg = subsector.start_seg as usize;
            let end_seg = start_seg + subsector.seg_count as usize;

            if let Some(segments) = map.segments().get(start_seg..end_seg) {
                // Test basic polygon generation
                let polygon_no_divlines = carve_subsector_polygon(segments, &[]);
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
                let polygon_one_divline = carve_subsector_polygon(segments, &[simple_divline]);
                assert!(
                    !polygon_one_divline.is_empty(),
                    "Polygon should not be empty with one divline"
                );

                // Test with the actual segments as divlines
                let mut segment_divlines = Vec::new();
                for segment in segments {
                    segment_divlines.push(DivLine::from_segment(segment));
                }
                let polygon_with_segments = carve_subsector_polygon(segments, &segment_divlines);
                assert!(
                    !polygon_with_segments.is_empty(),
                    "Polygon should not be empty with segment divlines"
                );
            }
        }
    }
}
