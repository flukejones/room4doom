use crate::level::map_defs::{Sector, Segment};
use glam::Vec3;

/// 3D portal connecting two sectors
#[derive(Debug, Clone)]
pub struct Portal3D {
    pub vertices: Vec<Vec3>,
    pub plane: Plane3D,
    pub front_sector: usize,
    pub back_sector: usize,
    pub portal_type: PortalType,
}

impl Portal3D {
    /// Calculate the plane equation for this portal
    pub fn calculate_plane(&mut self) {
        if self.vertices.len() >= 3 {
            let v1 = self.vertices[1] - self.vertices[0];
            let v2 = self.vertices[2] - self.vertices[0];

            let normal = Vec3::new(
                v1.y * v2.z - v1.z * v2.y,
                v1.z * v2.x - v1.x * v2.z,
                v1.x * v2.y - v1.y * v2.x,
            )
            .normalize_or_zero();

            let distance = normal.dot(self.vertices[0]);
            self.plane = Plane3D { normal, distance };
        } else {
            self.plane = Plane3D {
                normal: Vec3::Z,
                distance: 0.0,
            };
        }
    }

    /// Check if portal is facing toward a point
    pub fn is_facing_toward(&self, point: Vec3) -> bool {
        self.plane.distance_to_point(point) > 0.0
    }

    /// Get center point of portal
    pub fn get_center(&self) -> Vec3 {
        if self.vertices.is_empty() {
            return Vec3::ZERO;
        }

        let sum = self.vertices.iter().fold(Vec3::ZERO, |acc, v| acc + *v);
        sum / self.vertices.len() as f32
    }

    /// Get bounding box of portal
    pub fn get_bounds(&self) -> Option<(Vec3, Vec3)> {
        if self.vertices.is_empty() {
            return None;
        }

        let mut min = self.vertices[0];
        let mut max = self.vertices[0];

        for vertex in &self.vertices[1..] {
            min = min.min(*vertex);
            max = max.max(*vertex);
        }

        Some((min, max))
    }
}

/// 2D portal information
#[derive(Debug, Clone)]
pub struct Portal {
    pub front_sector: crate::MapPtr<Sector>,
    pub back_sector: crate::MapPtr<Sector>,
    pub portal_type: PortalType,
    pub z_position: f32,
    pub z_range: (f32, f32),
}

/// Type of portal connection
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PortalType {
    Open,
    Movable,
}

/// 3D plane equation
#[derive(Debug, Clone)]
pub struct Plane3D {
    pub normal: Vec3,
    pub distance: f32,
}

impl Plane3D {
    /// Create plane from three points
    pub fn new(p1: Vec3, p2: Vec3, p3: Vec3) -> Self {
        let v1 = p2 - p1;
        let v2 = p3 - p1;
        let normal = v1.cross(v2).normalize_or_zero();
        let distance = normal.dot(p1);

        Self { normal, distance }
    }

    /// Create plane from point and normal
    pub fn from_point_normal(point: Vec3, normal: Vec3) -> Self {
        let distance = normal.dot(point);
        Self { normal, distance }
    }

    /// Calculate distance from point to plane
    pub fn distance_to_point(&self, point: Vec3) -> f32 {
        self.normal.dot(point) - self.distance
    }

    /// Classify point relative to plane
    pub fn classify_point(&self, point: Vec3) -> PlaneClassification {
        let distance = self.distance_to_point(point);
        const EPSILON: f32 = 0.0001;

        if distance > EPSILON {
            PlaneClassification::Front
        } else if distance < -EPSILON {
            PlaneClassification::Back
        } else {
            PlaneClassification::OnPlane
        }
    }
}

/// Point classification relative to plane
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlaneClassification {
    Front,
    Back,
    OnPlane,
}

/// Generate portals from map data
pub fn discover_portals(_sectors: &[Sector], segments: &[Segment]) -> Vec<Portal> {
    let mut portals = Vec::new();

    for segment in segments {
        if let Some(back_sector) = &segment.backsector {
            let portal_type = classify_portal(&segment.frontsector, back_sector);

            let portal = Portal {
                front_sector: segment.frontsector.clone(),
                back_sector: back_sector.clone(),
                portal_type,
                z_position: segment.frontsector.floorheight.max(back_sector.floorheight),
                z_range: calculate_portal_z_range(&segment.frontsector, back_sector),
            };

            portals.push(portal);
        }
    }

    portals
}

/// Classify the type of portal between two sectors
fn classify_portal(
    front_sector: &crate::MapPtr<Sector>,
    back_sector: &crate::MapPtr<Sector>,
) -> PortalType {
    if is_movable_sector(&**front_sector) || is_movable_sector(&**back_sector) {
        PortalType::Movable
    } else {
        PortalType::Open
    }
}

/// Calculate the Z range for a portal between two sectors
fn calculate_portal_z_range(
    front_sector: &crate::MapPtr<Sector>,
    back_sector: &crate::MapPtr<Sector>,
) -> (f32, f32) {
    let min_floor = front_sector.floorheight.min(back_sector.floorheight);
    let max_ceiling = front_sector.ceilingheight.max(back_sector.ceilingheight);
    (min_floor, max_ceiling)
}

/// Generate 3D portals from 2D portal data
pub fn generate_3d_portals(portals: &[Portal], segments: &[Segment]) -> Vec<Portal3D> {
    let mut portals_3d = Vec::new();

    for portal in portals {
        if let Some(portal_3d) = create_3d_portal(portal, segments) {
            portals_3d.push(portal_3d);
        }
    }

    portals_3d
}

/// Create a 3D portal from 2D portal data
fn create_3d_portal(portal: &Portal, segments: &[Segment]) -> Option<Portal3D> {
    // Find the segment that corresponds to this portal
    let segment = segments.iter().find(|seg| {
        seg.frontsector.num == portal.front_sector.num
            && seg
                .backsector
                .as_ref()
                .map_or(false, |back| back.num == portal.back_sector.num)
    })?;

    // Create 3D vertices for the portal
    let bottom_z = portal.z_range.0;
    let top_z = portal.z_range.1;

    let vertices = vec![
        Vec3::new(segment.v1.x, segment.v1.y, bottom_z),
        Vec3::new(segment.v2.x, segment.v2.y, bottom_z),
        Vec3::new(segment.v2.x, segment.v2.y, top_z),
        Vec3::new(segment.v1.x, segment.v1.y, top_z),
    ];

    let mut portal_3d = Portal3D {
        vertices,
        plane: Plane3D {
            normal: Vec3::Z,
            distance: 0.0,
        },
        front_sector: portal.front_sector.num as usize,
        back_sector: portal.back_sector.num as usize,
        portal_type: portal.portal_type,
    };

    portal_3d.calculate_plane();
    Some(portal_3d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plane_distance_calculation() {
        let plane = Plane3D::new(
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        );

        assert!((plane.distance_to_point(Vec3::new(0.0, 0.0, 1.0)) - 1.0).abs() < 0.001);
        assert!((plane.distance_to_point(Vec3::new(0.0, 0.0, -1.0)) + 1.0).abs() < 0.001);
    }

    #[test]
    fn test_portal_type_classification() {
        // This would need actual Sector instances to test properly
        // For now, just test the movable sector detection
        let sector = Sector::new(0, 0.0, 128.0, 0, 0, 255, 1, 0); // Door type
        assert!(is_movable_sector(&sector));

        let normal_sector = Sector::new(0, 0.0, 128.0, 0, 0, 255, 0, 0); // Normal
        assert!(!is_movable_sector(&normal_sector));
    }
}
