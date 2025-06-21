use gameplay::{MapData, Node, Segment, SubSector};
use glam::{Vec2, Vec3};

/// A dividing line used for polygon clipping, similar to prboom's divline_t
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
    pub fn from_segment(seg: &Segment) -> Self {
        Self {
            x: seg.v1.x,
            y: seg.v1.y,
            dx: seg.v2.x - seg.v1.x,
            dy: seg.v2.y - seg.v1.y,
        }
    }

    /// Determine which side of the line a point is on
    /// Returns true if point is on the right side (positive side)
    pub fn point_on_side(&self, point: Vec2) -> bool {
        let cross = (point.y - self.y) * self.dx - (point.x - self.x) * self.dy;
        cross >= 0.0
    }

    /// Calculate intersection point between this line and a line segment
    /// Lines must intersect - this doesn't check for parallel lines
    pub fn calc_intersection(&self, seg_start: Vec2, seg_end: Vec2) -> Vec2 {
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

/// Maximum number of vertices allowed in polygon clipping
const MAX_CLIP_VERTICES: usize = 128;

/// Clip a polygon against a set of dividing lines using Sutherland-Hodgman algorithm
/// Similar to prboom's gld_FlatEdgeClipper
pub fn clip_polygon_with_divlines(mut vertices: Vec<Vec2>, clippers: &[DivLine]) -> Vec<Vec2> {
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

            let current_inside = !clipper.point_on_side(current_vertex); // Invert: keep right side (false)
            let next_inside = !clipper.point_on_side(next_vertex); // Invert: keep right side (false)

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
    _subsector: &SubSector,
    segments: &[Segment],
    divlines: &[DivLine],
) -> Vec<Vec2> {
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

/// Triangle represented as three Vec3 vertices
#[derive(Debug, Clone)]
pub struct Triangle {
    pub vertices: [Vec3; 3],
}

impl Triangle {
    /// Create a triangle from 2D vertices at a given height
    pub fn from_2d_with_height(v0: Vec2, v1: Vec2, v2: Vec2, height: f32) -> Self {
        Self {
            vertices: [
                Vec3::new(v0.x, v0.y, height),
                Vec3::new(v1.x, v1.y, height),
                Vec3::new(v2.x, v2.y, height),
            ],
        }
    }
}

/// BSP polygon generator - manages the recursive BSP traversal for polygon generation
pub struct BSPPolygons {
    /// Generated polygons for each subsector index
    subsector_polygons: Vec<Vec<Vec2>>,
    /// Triangulated floor/ceiling data for each subsector
    subsector_triangles: Vec<Vec<Triangle>>,
}

impl BSPPolygons {
    pub fn new() -> Self {
        Self {
            subsector_polygons: Vec::new(),
            subsector_triangles: Vec::new(),
        }
    }

    /// Generate polygons and triangles for all subsectors in the map
    pub fn generate_polygons(&mut self, map_data: &MapData) {
        // Initialize storage for all subsectors
        self.subsector_polygons.clear();
        self.subsector_triangles.clear();
        self.subsector_polygons
            .resize(map_data.subsectors().len(), Vec::new());
        self.subsector_triangles
            .resize(map_data.subsectors().len(), Vec::new());

        // Start BSP traversal from root node
        let root_node_id = map_data.start_node();
        if !map_data.get_nodes().is_empty() {
            self.carve_polygons_recursive(map_data, root_node_id, Vec::new());
        } else if !map_data.subsectors().is_empty() {
            // Handle trivial maps with no BSP nodes (single subsector)
            let subsector = &map_data.subsectors()[0];
            let start_seg = subsector.start_seg as usize;
            let end_seg = start_seg + subsector.seg_count as usize;

            if let Some(segments) = map_data.segments().get(start_seg..end_seg) {
                let polygon = carve_subsector_polygon(subsector, segments, &[]);
                self.subsector_polygons[0] = polygon.clone();
                self.triangulate_subsector(0, &polygon);
            }
        }
    }

    /// Recursively traverse BSP tree and generate polygons
    /// Similar to prboom's gld_CarveFlats
    fn carve_polygons_recursive(
        &mut self,
        map_data: &MapData,
        node_id: u32,
        divlines: Vec<DivLine>,
    ) {
        const IS_SUBSECTOR_MASK: u32 = 0x8000_0000;

        // Check if this is a subsector
        if node_id & IS_SUBSECTOR_MASK != 0 {
            // We've reached a subsector - generate its polygon
            let subsector_id = if node_id == u32::MAX {
                0
            } else {
                (node_id & !IS_SUBSECTOR_MASK) as usize
            };

            if subsector_id < map_data.subsectors().len() {
                let subsector = &map_data.subsectors()[subsector_id];
                let start_seg = subsector.start_seg as usize;
                let end_seg = start_seg + subsector.seg_count as usize;

                if let Some(segments) = map_data.segments().get(start_seg..end_seg) {
                    let polygon = carve_subsector_polygon(subsector, segments, &divlines);
                    self.subsector_polygons[subsector_id] = polygon.clone();
                    self.triangulate_subsector(subsector_id, &polygon);
                }
            }
            return;
        }

        // It's a node - get the node data
        if let Some(node) = map_data.get_nodes().get(node_id as usize) {
            // Create divline from this node
            let node_divline = DivLine::from_node(node);

            // Process right child with original divline
            let mut right_divlines = divlines.clone();
            right_divlines.push(node_divline);
            self.carve_polygons_recursive(map_data, node.children[0], right_divlines);

            // Process left child with reversed divline
            let mut left_divlines = divlines;
            let mut reversed_divline = node_divline;
            reversed_divline.dx = -reversed_divline.dx;
            reversed_divline.dy = -reversed_divline.dy;
            left_divlines.push(reversed_divline);
            self.carve_polygons_recursive(map_data, node.children[1], left_divlines);
        }
    }

    /// Get the triangulated data for a subsector (for floor/ceiling rendering)
    pub fn get_subsector_triangles(&self, subsector_id: usize) -> Option<&[Triangle]> {
        self.subsector_triangles
            .get(subsector_id)
            .map(|v| v.as_slice())
    }

    /// Triangulate a subsector polygon into triangles using fan triangulation
    pub fn triangulate_subsector(&mut self, subsector_id: usize, polygon_vertices: &[Vec2]) {
        if polygon_vertices.len() < 3 {
            return;
        }

        let mut triangles = Vec::new();

        // Create triangles using simple fan triangulation from first vertex
        for i in 1..polygon_vertices.len() - 1 {
            let triangle = Triangle::from_2d_with_height(
                polygon_vertices[0],
                polygon_vertices[i],
                polygon_vertices[i + 1],
                0.0, // Height will be set during rendering
            );
            triangles.push(triangle);
        }

        if subsector_id < self.subsector_triangles.len() {
            self.subsector_triangles[subsector_id] = triangles;
        }
    }
}

impl Default for BSPPolygons {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_polygon_clipping() {
        // Create a simple square
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
    fn test_triangulation() {
        // Create a simple square polygon
        let polygon = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(10.0, 10.0),
            Vec2::new(0.0, 10.0),
        ];

        let mut generator = BSPPolygons::new();
        // Initialize storage for at least 1 subsector
        generator.subsector_triangles.resize(1, Vec::new());
        generator.triangulate_subsector(0, &polygon);

        let triangles = generator.get_subsector_triangles(0).unwrap();

        // A square should be triangulated into 2 triangles
        assert_eq!(triangles.len(), 2);

        // Each triangle should have 3 vertices
        for triangle in triangles {
            assert_eq!(triangle.vertices.len(), 3);
        }
    }

    #[test]
    fn test_triangle_from_2d() {
        let triangle = Triangle::from_2d_with_height(
            Vec2::new(0.0, 0.0),
            Vec2::new(5.0, 0.0),
            Vec2::new(2.5, 5.0),
            10.0,
        );

        // All vertices should be at height 10.0
        for vertex in &triangle.vertices {
            assert_eq!(vertex.z, 10.0);
        }

        // Check that 2D coordinates are preserved
        assert_eq!(triangle.vertices[0], Vec3::new(0.0, 0.0, 10.0));
        assert_eq!(triangle.vertices[1], Vec3::new(5.0, 0.0, 10.0));
        assert_eq!(triangle.vertices[2], Vec3::new(2.5, 5.0, 10.0));
    }
}
