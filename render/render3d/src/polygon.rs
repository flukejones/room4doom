use gameplay::{PicData, Segment};
use glam::{Vec2, Vec3, Vec4};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PolygonType {
    Wall,      // Solid wall (one-sided)
    UpperWall, // Upper texture on two-sided wall
    LowerWall, // Lower texture on two-sided wall
    Portal,    // See-through opening
    Floor,     // Floor surface
    Ceiling,   // Ceiling surface
}

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
            polygons.push(Polygon3D::from_wall_segment(
                v1,
                v2,
                front_floor,
                back_floor,
                if let Some(t) = seg.sidedef.bottomtexture {
                    pic_data.get_texture_average_color(light, scale, t)
                } else {
                    [128, 128, 128, 255]
                }, // Gray
                PolygonType::LowerWall,
            ));
        }

        // Upper wall (overhead) - if back ceiling is lower than front ceiling
        if back_ceiling < front_ceiling {
            polygons.push(Polygon3D::from_wall_segment(
                v1,
                v2,
                back_ceiling,
                front_ceiling,
                if let Some(t) = seg.sidedef.toptexture {
                    pic_data.get_texture_average_color(light, scale, t)
                } else {
                    [64, 64, 64, 255]
                }, // Dark gray
                PolygonType::UpperWall,
            ));
        }

        // Portal opening - no polygon needed, just used for clipping
        // The portal area is defined by the gap between upper and lower walls
    } else {
        // One-sided line - solid wall from floor to ceiling
        polygons.push(Polygon3D::from_wall_segment(
            v1,
            v2,
            front_floor,
            front_ceiling,
            if let Some(t) = seg.sidedef.midtexture {
                pic_data.get_texture_average_color(light, scale, t)
            } else {
                [255, 255, 255, 255]
            }, // White for solid walls
            PolygonType::Wall,
        ));
    }

    polygons
}

/// Represents a 3D polygon in world space
#[derive(Debug, Clone)]
pub struct Polygon3D {
    pub vertices: Vec<Vec3>,
    pub color: [u8; 4],
    pub polygon_type: PolygonType,
}

impl Polygon3D {
    /// Create a vertical quad polygon from a line segment and heights
    /// The polygon vertices are ordered: bottom-left, bottom-right, top-right, top-left
    pub fn from_wall_segment(
        v1: Vec2,
        v2: Vec2,
        bottom_height: f32,
        top_height: f32,
        color: [u8; 4],
        polygon_type: PolygonType,
    ) -> Self {
        let vertices = vec![
            Vec3::new(v1.x, v1.y, bottom_height), // bottom-left
            Vec3::new(v2.x, v2.y, bottom_height), // bottom-right
            Vec3::new(v2.x, v2.y, top_height),    // top-right
            Vec3::new(v1.x, v1.y, top_height),    // top-left
        ];

        Self {
            vertices,
            color,
            polygon_type,
        }
    }

    /// Transform polygon to view space
    pub fn transform(&self, view_matrix: &glam::Mat4) -> Self {
        let vertices = self
            .vertices
            .iter()
            .map(|v| view_matrix.transform_point3(*v))
            .collect();

        Self {
            vertices,
            color: self.color,
            polygon_type: self.polygon_type,
        }
    }

    /// Project polygon to screen space
    pub fn project(
        &self,
        projection_matrix: &glam::Mat4,
        screen_width: f32,
        screen_height: f32,
    ) -> Option<Polygon2D> {
        let mut screen_vertices = Vec::new();

        // First pass: project all vertices that are in front of the camera
        let mut projected_vertices = Vec::new();
        for vertex in &self.vertices {
            if vertex.z < -0.01 {
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
        for i in 0..self.vertices.len() {
            let v1_idx = i;
            let v2_idx = (i + 1) % self.vertices.len();
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
                    // Find intersection with near plane z = -0.01
                    if v1.z < -0.01 && v2.z > -0.01 {
                        let t = (-0.01 - v1.z) / (v2.z - v1.z);
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
                    if v1.z > -0.01 && v2.z < -0.01 {
                        let t = (-0.01 - v1.z) / (v2.z - v1.z);
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
                polygon_type: self.polygon_type,
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
    pub color: [u8; 4],
    pub polygon_type: PolygonType,
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
