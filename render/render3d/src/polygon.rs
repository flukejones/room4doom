use gameplay::{PicData, Segment};
use glam::{Vec2, Vec3, Vec4};

/// Convert a segment into 3D polygons based on floor/ceiling heights
pub fn segment_to_polygons(seg: &Segment, pic_data: &PicData) -> Vec<Polygon3D> {
    let mut polygons = Vec::new();

    let v1 = seg.v1;
    let v2 = seg.v2;
    let front_floor = seg.frontsector.floorheight;
    let front_ceiling = seg.frontsector.ceilingheight;

    // (sidedef.sector.lightlevel >> 4) + player.extralight
    let light = seg.sidedef.sector.lightlevel >> 4;
    let scale = 1.0;
    if let Some(back_sector) = &seg.backsector {
        // Two-sided line - may have upper wall, lower wall, and portal
        let back_floor = back_sector.floorheight;
        let back_ceiling = back_sector.ceilingheight;

        // Lower wall (step up) - if back floor is higher than front floor
        if back_floor > front_floor {
            polygons.extend(Polygon3D::from_wall_segment(
                v1,
                v2,
                front_floor,
                back_floor,
                if let Some(t) = seg.sidedef.bottomtexture {
                    pic_data.get_texture_average_color(light, scale, t)
                } else {
                    [128, 128, 128, 255]
                }, // Gray
            ));
        }

        // Upper wall (overhead) - if back ceiling is lower than front ceiling
        if back_ceiling < front_ceiling {
            polygons.extend(Polygon3D::from_wall_segment(
                v1,
                v2,
                back_ceiling,
                front_ceiling,
                if let Some(t) = seg.sidedef.toptexture {
                    pic_data.get_texture_average_color(light, scale, t)
                } else {
                    [64, 64, 64, 255]
                }, // Dark gray
            ));
        }

        // Portal opening - no polygon needed, just used for clipping
        // The portal area is defined by the gap between upper and lower walls
    } else {
        // One-sided line - solid wall from floor to ceiling
        polygons.extend(Polygon3D::from_wall_segment(
            v1,
            v2,
            front_floor,
            front_ceiling,
            if let Some(t) = seg.sidedef.midtexture {
                pic_data.get_texture_average_color(light, scale, t)
            } else {
                [255, 255, 255, 255]
            }, // White for solid walls
        ));
    }

    polygons
}

pub const POLYGON3D_VERTEX_COUNT: usize = 3;
pub const POLYGON3D_RGBA_SIZE: usize = 4;
/// Represents a 3D polygon in world space
#[derive(Debug, Clone)]
pub struct Polygon3D {
    pub vertices: [Vec3; POLYGON3D_VERTEX_COUNT],
    pub color: [u8; POLYGON3D_RGBA_SIZE],
}

impl Polygon3D {
    /// Create two triangle polygons from a line segment and heights
    /// Returns two triangles that form the wall quad
    pub fn from_wall_segment(
        v1: Vec2,
        v2: Vec2,
        bottom_height: f32,
        top_height: f32,
        color: [u8; 4],
    ) -> Vec<Self> {
        let bottom_left = Vec3::new(v1.x, v1.y, bottom_height);
        let bottom_right = Vec3::new(v2.x, v2.y, bottom_height);
        let top_right = Vec3::new(v2.x, v2.y, top_height);
        let top_left = Vec3::new(v1.x, v1.y, top_height);

        vec![
            // First triangle: bottom-left, bottom-right, top-right
            Self {
                vertices: [bottom_left, bottom_right, top_right],
                color,
            },
            // Second triangle: bottom-left, top-right, top-left
            Self {
                vertices: [bottom_left, top_right, top_left],
                color,
            },
        ]
    }

    /// Transform polygon to view space
    pub fn transform(&self, view_matrix: &glam::Mat4) -> Self {
        let vertices = [
            view_matrix.transform_point3(self.vertices[0]),
            view_matrix.transform_point3(self.vertices[1]),
            view_matrix.transform_point3(self.vertices[2]),
        ];

        Self {
            vertices,
            color: self.color,
        }
    }

    /// Project polygon to screen space
    pub fn project(
        &self,
        projection_matrix: &glam::Mat4,
        screen_width: f32,
        screen_height: f32,
        near_z: f32,
    ) -> Option<Polygon2D> {
        let mut screen_vertices = Vec::new();

        // First pass: project all vertices that are in front of the camera
        let mut projected_vertices = Vec::new();
        for vertex in &self.vertices {
            if vertex.z < near_z {
                // Only project vertices in front of camera
                // Apply projection matrix (vertex is in view space)
                let clip = projection_matrix * Vec4::new(vertex.x, vertex.y, vertex.z, 1.0);

                if clip.w > 0.0 {
                    // Perspective divide to get NDC
                    let ndc = Vec3::new(clip.x / clip.w, clip.y / clip.w, clip.z / clip.w);

                    // Convert NDC to screen space
                    let screen_x = (ndc.x + 1.0) * 0.5 * screen_width;
                    let screen_y = (1.0 - ndc.y) * 0.5 * screen_height;

                    projected_vertices.push(Some(Vec2::new(screen_x, screen_y)));
                } else {
                    projected_vertices.push(None);
                }
            } else {
                projected_vertices.push(None);
            }
        }

        // Second pass: handle edge clipping for vertices behind camera
        for i in 0..POLYGON3D_VERTEX_COUNT {
            let v1_idx = i;
            let v2_idx = (i + 1) % POLYGON3D_VERTEX_COUNT;
            let v1 = self.vertices[v1_idx];
            let v2 = self.vertices[v2_idx];

            match (projected_vertices[v1_idx], projected_vertices[v2_idx]) {
                (Some(p1), Some(_p2)) => {
                    // Both vertices projected successfully
                    if screen_vertices.is_empty() || screen_vertices.last() != Some(&p1) {
                        screen_vertices.push(p1);
                    }
                }
                (Some(p1), None) => {
                    // v1 is visible, v2 is behind camera - clip edge
                    if screen_vertices.is_empty() || screen_vertices.last() != Some(&p1) {
                        screen_vertices.push(p1);
                    }
                    // Find intersection with near plane
                    if v1.z < -near_z && v2.z > -near_z {
                        let t = (-near_z - v1.z) / (v2.z - v1.z);
                        let clip_point = v1 + (v2 - v1) * t;

                        let clip = projection_matrix
                            * Vec4::new(clip_point.x, clip_point.y, clip_point.z, 1.0);
                        if clip.w > 0.0 {
                            let ndc = Vec3::new(clip.x / clip.w, clip.y / clip.w, clip.z / clip.w);
                            let screen_x = (ndc.x + 1.0) * 0.5 * screen_width;
                            let screen_y = (1.0 - ndc.y) * 0.5 * screen_height;
                            screen_vertices.push(Vec2::new(screen_x, screen_y));
                        }
                    }
                }
                (None, Some(_p2)) => {
                    // v1 is behind camera, v2 is visible - clip edge
                    if v1.z > -near_z && v2.z < -near_z {
                        let t = (-near_z - v1.z) / (v2.z - v1.z);
                        let clip_point = v1 + (v2 - v1) * t;

                        let clip = projection_matrix
                            * Vec4::new(clip_point.x, clip_point.y, clip_point.z, 1.0);
                        if clip.w > 0.0 {
                            let ndc = Vec3::new(clip.x / clip.w, clip.y / clip.w, clip.z / clip.w);
                            let screen_x = (ndc.x + 1.0) * 0.5 * screen_width;
                            let screen_y = (1.0 - ndc.y) * 0.5 * screen_height;
                            screen_vertices.push(Vec2::new(screen_x, screen_y));
                        }
                    }
                }
                (None, None) => {
                    // Both vertices behind camera - skip this edge
                }
            }
        }

        // Need at least 3 vertices for a valid polygon
        if screen_vertices.len() >= 3 {
            Some(Polygon2D {
                vertices: screen_vertices,
                color: self.color,
            })
        } else {
            None
        }
    }
}

/// Represents a 2D polygon in screen space
#[derive(Debug, Clone)]
pub struct Polygon2D {
    pub vertices: Vec<Vec2>,
    pub color: [u8; POLYGON3D_RGBA_SIZE],
}

impl Polygon2D {
    /// Get axis-aligned bounding box of polygon
    pub fn bounds(&self) -> Option<(Vec2, Vec2)> {
        if self.vertices.is_empty() {
            return None;
        }

        let mut min = self.vertices[0];
        let mut max = self.vertices[0];

        for vertex in &self.vertices[1..] {
            min.x = min.x.min(vertex.x);
            min.y = min.y.min(vertex.y);
            max.x = max.x.max(vertex.x);
            max.y = max.y.max(vertex.y);
        }

        Some((min, max))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wall_segment_to_triangles() {
        let v1 = Vec2::new(0.0, 0.0);
        let v2 = Vec2::new(10.0, 0.0);
        let bottom_height = 0.0;
        let top_height = 8.0;
        let color = [255, 255, 255, 255];

        let triangles = Polygon3D::from_wall_segment(v1, v2, bottom_height, top_height, color);

        // Should return exactly 2 triangles
        assert_eq!(triangles.len(), 2);

        // Each triangle should have exactly 3 vertices
        for triangle in &triangles {
            assert_eq!(triangle.color, color);
        }

        // Verify the triangles cover the expected wall area
        let expected_vertices = [
            Vec3::new(0.0, 0.0, 0.0),  // bottom-left
            Vec3::new(10.0, 0.0, 0.0), // bottom-right
            Vec3::new(10.0, 0.0, 8.0), // top-right
            Vec3::new(0.0, 0.0, 8.0),  // top-left
        ];

        // First triangle: bottom-left, bottom-right, top-right
        assert_eq!(triangles[0].vertices[0], expected_vertices[0]);
        assert_eq!(triangles[0].vertices[1], expected_vertices[1]);
        assert_eq!(triangles[0].vertices[2], expected_vertices[2]);

        // Second triangle: bottom-left, top-right, top-left
        assert_eq!(triangles[1].vertices[0], expected_vertices[0]);
        assert_eq!(triangles[1].vertices[1], expected_vertices[2]);
        assert_eq!(triangles[1].vertices[2], expected_vertices[3]);
    }
}
