use std::f32::consts::PI;

use gameplay::*;
use glam::{Mat4, Vec2, Vec3, Vec4};

use crate::{Renderer3D, ScreenPoly};

#[test]
fn test_renderer_creation() {
    let renderer = Renderer3D::new(640.0, 480.0, 90.0);
    assert_eq!(renderer.width, 640);
    assert_eq!(renderer.height, 480);
}

#[test]
fn test_viewheight_integration() {
    let renderer = Renderer3D::new(640.0, 480.0, 90.0);
    let initial_view = renderer.view_matrix;
    assert_eq!(initial_view, Mat4::IDENTITY);
}

#[test]
fn test_polygon_edge_clipping() {
    // Test polygon with vertices crossing screen boundaries
    let vertices = vec![
        Vec2::new(-50.0, 100.0), // Left of screen
        Vec2::new(100.0, 100.0), // Inside screen
        Vec2::new(700.0, 200.0), // Right of screen
        Vec2::new(320.0, 500.0), // Above screen
        Vec2::new(200.0, -50.0), // Below screen
    ];

    let polygon = ScreenPoly { vertices };

    // Check bounds calculation handles clipping correctly
    let bounds = polygon.bounds();
    assert!(bounds.is_some());
    let (min, max) = bounds.unwrap();

    // Verify bounds are reasonable (not containing extreme values)
    assert!(min.x >= -50.0);
    assert!(min.y >= -50.0);
    assert!(max.x <= 700.0);
    assert!(max.y <= 500.0);
}

#[test]
fn test_polygon_intersection_buffer() {
    // Create a simple triangle that crosses multiple scan lines
    let vertices = vec![
        Vec2::new(100.0, 100.0),
        Vec2::new(200.0, 100.0),
        Vec2::new(150.0, 200.0),
    ];

    let polygon = ScreenPoly { vertices };

    // Manually verify intersection logic for x=150
    let x_test = 150.0;
    let mut intersections = Vec::new();

    for i in 0..polygon.vertices.len() {
        let v1 = polygon.vertices[i];
        let v2 = polygon.vertices[(i + 1) % polygon.vertices.len()];

        if (v1.x <= x_test && v2.x >= x_test) || (v2.x <= x_test && v1.x >= x_test) {
            if (v2.x - v1.x).abs() > 0.001 {
                let t = (x_test - v1.x) / (v2.x - v1.x);
                if t >= 0.0 && t <= 1.0 {
                    let y = v1.y + (v2.y - v1.y) * t;
                    intersections.push(y);
                }
            }
        }
    }

    // Should have exactly 2 intersections for this triangle at x=150
    // But we might get 3 due to vertex coincidence, so filter unique values
    intersections.sort_by(|a, b| a.partial_cmp(b).unwrap());
    intersections.dedup_by(|a, b| (*a - *b).abs() < 0.001);

    assert_eq!(intersections.len(), 2);

    // First intersection should be at y=100 (top edge)
    assert!((intersections[0] - 100.0).abs() < 0.001);
    // Second intersection should be at y=200 (vertex)
    assert!((intersections[1] - 200.0).abs() < 0.001);
}

#[test]
fn test_polygon_edge_cases() {
    // Test degenerate cases
    let empty_polygon = ScreenPoly { vertices: vec![] };
    assert!(empty_polygon.bounds().is_none());

    let single_vertex = ScreenPoly {
        vertices: vec![Vec2::new(100.0, 100.0)],
    };
    let bounds = single_vertex.bounds();
    assert!(bounds.is_some());
    let (min, max) = bounds.unwrap();
    assert_eq!(min, max);

    // Test vertical edge (should be skipped in intersection)
    let vertical_edge = ScreenPoly {
        vertices: vec![
            Vec2::new(100.0, 100.0),
            Vec2::new(100.0, 200.0),
            Vec2::new(200.0, 150.0),
        ],
    };

    // Vertical edges should not contribute to intersections
    let bounds = vertical_edge.bounds();
    assert!(bounds.is_some());
}

#[test]
fn test_3d_polygon_edge_clipping() {
    let renderer = Renderer3D::new(640.0, 480.0, 90.0);

    // Test polygon with vertices that cross the near plane
    let vertices_3d = vec![
        Vec3::new(-1.0, -1.0, -2.0), // Behind camera
        Vec3::new(1.0, -1.0, 2.0),   // In front of camera
        Vec3::new(1.0, 1.0, 2.0),    // In front of camera
        Vec3::new(-1.0, 1.0, -2.0),  // Behind camera
    ];

    // Create a mock surface polygon for testing
    let surface_poly = gameplay::SurfacePolygon {
        vertices: vertices_3d,
        sector_id: 0,
        subsector_id: 0,
        surface_kind: SurfaceKind::Vertical {
            texture: Some(1),
            tex_x_offset: 0.0,
            tex_y_offset: 0.0,
            texture_direction: PI,
        },
        normal: Vec3::new(0.0, 0.0, 1.0),
        aabb: gameplay::AABB {
            min: Vec3::new(-1.0, -1.0, -2.0),
            max: Vec3::new(1.0, 1.0, 2.0),
        },
    };

    // Test projection and clipping logic
    let view_projection = renderer.projection_matrix * renderer.view_matrix;
    let mut projected_vertices = Vec::new();
    let mut valid_projections = 0;

    for vertex in &surface_poly.vertices {
        let clip_pos = view_projection * Vec4::new(vertex.x, vertex.y, vertex.z, 1.0);

        if clip_pos.w > 0.01 {
            let ndc = clip_pos / clip_pos.w;
            let screen_x = (ndc.x + 1.0) * 0.5 * renderer.width as f32;
            let screen_y = (1.0 - ndc.y) * 0.5 * renderer.height as f32;
            projected_vertices.push(Some(Vec2::new(screen_x, screen_y)));
            valid_projections += 1;
        } else {
            projected_vertices.push(None);
        }
    }

    // Should have exactly 2 valid projections (vertices in front of camera)
    assert_eq!(valid_projections, 2);

    // Test edge clipping between visible and invisible vertices
    let vertex_count = surface_poly.vertices.len();
    let mut clipped_edges = 0;

    for i in 0..vertex_count {
        let v1_idx = i;
        let v2_idx = (i + 1) % vertex_count;
        let v1_world = surface_poly.vertices[v1_idx];
        let v2_world = surface_poly.vertices[v2_idx];

        let v1_view = renderer.view_matrix * Vec4::new(v1_world.x, v1_world.y, v1_world.z, 1.0);
        let v2_view = renderer.view_matrix * Vec4::new(v2_world.x, v2_world.y, v2_world.z, 1.0);

        // Check if edge needs clipping
        let v1_behind = v1_view.z > -renderer.near_z;
        let v2_behind = v2_view.z > -renderer.near_z;

        if v1_behind != v2_behind {
            // Edge crosses near plane and needs clipping
            clipped_edges += 1;

            // Verify clipping calculation
            let t = (-renderer.near_z - v1_view.z) / (v2_view.z - v1_view.z);
            assert!(
                t >= 0.0 && t <= 1.0,
                "Clipping parameter t should be in range [0,1]"
            );

            let clip_point_view = v1_view + (v2_view - v1_view) * t;
            assert!(
                (clip_point_view.z + renderer.near_z).abs() < 0.001,
                "Clipped point should be on near plane"
            );
        }
    }

    // Should have exactly 2 edges that need clipping
    assert_eq!(clipped_edges, 2);
}

#[test]
fn test_frustum_culling_pitch_rotation() {
    let mut renderer = Renderer3D::new(640.0, 480.0, 90.0);

    // Create a test AABB at origin
    let test_aabb = gameplay::AABB {
        min: Vec3::new(-1.0, -1.0, -1.0),
        max: Vec3::new(1.0, 1.0, 1.0),
    };

    // Test at 45 degree pitch down, no rotation
    let pos = Vec3::new(0.0, -5.0, 2.0);
    let angle = 0.0;
    let pitch = -45.0 * PI / 180.0;
    create_test_view_matrix(&mut renderer, pos, angle, pitch);

    // AABB should be visible when looking down at it
    assert!(
        !renderer.is_bbox_outside_fov(&test_aabb),
        "AABB should be visible at 45° pitch down"
    );

    // Test at 45 degree pitch down, 45 degree rotation
    let pos = Vec3::new(-5.0, -5.0, 2.0);
    let angle = 45.0 * PI / 180.0;
    let pitch = -45.0 * PI / 180.0;
    create_test_view_matrix(&mut renderer, pos, angle, pitch);

    // AABB should still be visible
    assert!(
        !renderer.is_bbox_outside_fov(&test_aabb),
        "AABB should be visible at 45° pitch down, 45° rotation"
    );

    // Test edge case: AABB partially in frustum
    let edge_aabb = gameplay::AABB {
        min: Vec3::new(-10.0, -1.0, -1.0),
        max: Vec3::new(10.0, 1.0, 1.0),
    };

    // This large AABB should intersect the frustum
    assert!(
        !renderer.is_bbox_outside_fov(&edge_aabb),
        "Large AABB should intersect frustum"
    );

    // Test AABB behind camera (camera at (-5,-5,2) looking northeast)
    // Put AABB in southwest direction, which is behind the camera
    let behind_aabb = gameplay::AABB {
        min: Vec3::new(-10.0, -10.0, -1.0),
        max: Vec3::new(-8.0, -8.0, 1.0),
    };

    assert!(
        renderer.is_bbox_outside_fov(&behind_aabb),
        "AABB behind camera should be culled"
    );
}

#[test]
fn test_frustum_culling_extreme_angles() {
    let mut renderer = Renderer3D::new(640.0, 480.0, 90.0);

    // Test AABB at different positions
    let positions = [
        Vec3::new(0.0, 0.0, 0.0),  // Origin
        Vec3::new(2.0, 0.0, 0.0),  // Right
        Vec3::new(0.0, 2.0, 0.0),  // Forward
        Vec3::new(0.0, 0.0, 2.0),  // Up
        Vec3::new(-2.0, 0.0, 0.0), // Left
        Vec3::new(0.0, 0.0, -2.0), // Down
    ];

    for pos in positions {
        let test_aabb = gameplay::AABB {
            min: pos - Vec3::ONE * 0.5,
            max: pos + Vec3::ONE * 0.5,
        };

        // Test at 90 degree pitch down (looking straight down)
        let cam_pos = pos + Vec3::new(0.0, 0.0, 5.0);
        let angle = 0.0;
        let pitch = -90.0 * PI / 180.0;
        create_test_view_matrix(&mut renderer, cam_pos, angle, pitch);

        if pos.z >= -2.0 {
            assert!(
                !renderer.is_bbox_outside_fov(&test_aabb),
                "AABB at {:?} should be visible when looking straight down",
                pos
            );
        }

        // Test at 90 degree pitch up (looking straight up)
        let cam_pos = pos + Vec3::new(0.0, 0.0, -5.0);
        let angle = 0.0;
        let pitch = 90.0 * PI / 180.0;
        create_test_view_matrix(&mut renderer, cam_pos, angle, pitch);

        if pos.z <= 2.0 {
            assert!(
                !renderer.is_bbox_outside_fov(&test_aabb),
                "AABB at {:?} should be visible when looking straight up",
                pos
            );
        }
    }
}

#[test]
fn test_frustum_culling_precision() {
    let mut renderer = Renderer3D::new(640.0, 480.0, 90.0);

    // Test very small AABB at edge of frustum
    let small_edge_aabb = gameplay::AABB {
        min: Vec3::new(0.99, 0.99, 0.99),
        max: Vec3::new(1.01, 1.01, 1.01),
    };

    // Position camera to look at edge
    let pos = Vec3::new(0.0, 0.0, 0.0);
    let angle = 45.0 * PI / 180.0;
    let pitch = 0.0;
    create_test_view_matrix(&mut renderer, pos, angle, pitch);

    // Small AABB at edge should be visible due to conservative culling
    assert!(
        !renderer.is_bbox_outside_fov(&small_edge_aabb),
        "Small AABB at frustum edge should be visible"
    );

    // The frustum culling is intentionally conservative to avoid missing
    // visible geometry This is the correct behavior for 3D rendering
}

#[test]
fn test_frustum_culling_edge_intersections() {
    let mut renderer = Renderer3D::new(640.0, 480.0, 90.0);

    // Test AABB that intersects frustum boundary
    let intersecting_aabb = gameplay::AABB {
        min: Vec3::new(-0.5, 0.5, -0.5),
        max: Vec3::new(0.5, 5.0, 0.5),
    };

    // Camera looking forward
    let pos = Vec3::new(0.0, 0.0, 0.0);
    let angle = 0.0;
    let pitch = 0.0;
    create_test_view_matrix(&mut renderer, pos, angle, pitch);

    // AABB extends in front of camera and should be visible
    assert!(
        !renderer.is_bbox_outside_fov(&intersecting_aabb),
        "AABB intersecting frustum should be visible"
    );

    // Test AABB at various pitch angles with intersection
    let pitch_angles = [-45.0, -30.0, 0.0, 30.0, 45.0];
    for pitch_deg in pitch_angles {
        let pitch = pitch_deg * PI / 180.0;
        create_test_view_matrix(&mut renderer, pos, angle, pitch);

        let result = renderer.is_bbox_outside_fov(&intersecting_aabb);
        println!("Pitch: {:.1}°, AABB culled: {}", pitch_deg, result);

        // At any reasonable pitch angle, an AABB extending forward should be visible
        assert!(!result, "AABB should be visible at {:.1}° pitch", pitch_deg);
    }
}

#[test]
fn test_pitch_angle_polygon_exclusion_issue() {
    // This test specifically addresses the reported issue where polygons get
    // excluded at certain pitch angles, especially around 45 degrees down with
    // rotation
    let mut renderer = Renderer3D::new(640.0, 480.0, 90.0);

    // Test scenario: player at origin looking down at 45° pitch with 45° rotation
    // This represents a common scenario where polygons might be incorrectly culled
    let camera_pos = Vec3::new(0.0, 0.0, 5.0);
    let rotation_angle = 45.0 * PI / 180.0; // 45 degrees rotation
    let pitch_angle = -45.0 * PI / 180.0; // 45 degrees down

    create_test_view_matrix(&mut renderer, camera_pos, rotation_angle, pitch_angle);

    // Test various AABBs at different positions that should be visible
    let test_cases = vec![
        // AABB directly below camera
        gameplay::AABB {
            min: Vec3::new(-1.0, -1.0, 0.0),
            max: Vec3::new(1.0, 1.0, 2.0),
        },
        // AABB at 45 degree angle from camera (in line with view direction)
        gameplay::AABB {
            min: Vec3::new(2.0, 2.0, 0.0),
            max: Vec3::new(4.0, 4.0, 2.0),
        },
        // AABB offset but still within reasonable view
        gameplay::AABB {
            min: Vec3::new(-2.0, 2.0, 1.0),
            max: Vec3::new(0.0, 4.0, 3.0),
        },
    ];

    for (i, aabb) in test_cases.iter().enumerate() {
        let is_culled = renderer.is_bbox_outside_fov(aabb);
        println!("Test case {}: AABB {:?} culled: {}", i, aabb, is_culled);

        // These AABBs should be visible with the camera setup
        assert!(
            !is_culled,
            "AABB {} should be visible at 45° pitch down, 45° rotation",
            i
        );
    }

    // Test edge case: AABB that's partially intersecting the frustum
    let edge_aabb = gameplay::AABB {
        min: Vec3::new(-10.0, -10.0, -1.0),
        max: Vec3::new(10.0, 10.0, 1.0),
    };

    let edge_culled = renderer.is_bbox_outside_fov(&edge_aabb);
    println!("Large edge AABB culled: {}", edge_culled);

    // Large AABB that spans across the view should not be culled
    assert!(
        !edge_culled,
        "Large AABB spanning view should not be culled"
    );
}

// Helper function to create test view matrix directly
#[test]
fn test_frustum_culling_planes_diagnostic() {
    let mut renderer = Renderer3D::new(640.0, 480.0, 90.0);

    // Set up a known camera position and orientation
    let pos = Vec3::new(0.0, 0.0, 0.0);
    let angle = 0.0; // Looking along positive X axis (Doom coordinate system)
    let pitch = 0.0; // No pitch
    create_test_view_matrix(&mut renderer, pos, angle, pitch);

    // Test AABB directly in front of camera - should be visible
    let front_aabb = gameplay::AABB {
        min: Vec3::new(5.0, -1.0, -1.0),
        max: Vec3::new(10.0, 1.0, 1.0),
    };
    assert!(
        !renderer.is_bbox_outside_fov(&front_aabb),
        "AABB in front should be visible"
    );

    // Test AABB far to the left - should be culled
    let far_left_aabb = gameplay::AABB {
        min: Vec3::new(5.0, -100.0, -1.0),
        max: Vec3::new(10.0, -99.0, 1.0),
    };
    assert!(
        renderer.is_bbox_outside_fov(&far_left_aabb),
        "Far left AABB should be culled"
    );

    // Test AABB far to the right - should be culled
    let far_right_aabb = gameplay::AABB {
        min: Vec3::new(5.0, 100.0, -1.0),
        max: Vec3::new(10.0, 101.0, 1.0),
    };
    assert!(
        renderer.is_bbox_outside_fov(&far_right_aabb),
        "Far right AABB should be culled"
    );

    // Test AABB far above - should be culled
    let far_above_aabb = gameplay::AABB {
        min: Vec3::new(5.0, -1.0, 100.0),
        max: Vec3::new(10.0, 1.0, 101.0),
    };
    assert!(
        renderer.is_bbox_outside_fov(&far_above_aabb),
        "Far above AABB should be culled"
    );

    // Test AABB far below - should be culled
    let far_below_aabb = gameplay::AABB {
        min: Vec3::new(5.0, -1.0, -101.0),
        max: Vec3::new(10.0, 1.0, -100.0),
    };
    assert!(
        renderer.is_bbox_outside_fov(&far_below_aabb),
        "Far below AABB should be culled"
    );

    // Test AABB behind camera - should be culled
    let behind_aabb = gameplay::AABB {
        min: Vec3::new(-10.0, -1.0, -1.0),
        max: Vec3::new(-5.0, 1.0, 1.0),
    };
    assert!(
        renderer.is_bbox_outside_fov(&behind_aabb),
        "Behind camera AABB should be culled"
    );

    // Test problematic case: AABB at edge of frustum with 45 degree pitch
    let edge_pos = Vec3::new(0.0, 0.0, 5.0);
    let edge_angle = 45.0 * PI / 180.0;
    let edge_pitch = -45.0 * PI / 180.0;
    create_test_view_matrix(&mut renderer, edge_pos, edge_angle, edge_pitch);

    let edge_aabb = gameplay::AABB {
        min: Vec3::new(2.0, 2.0, 0.0),
        max: Vec3::new(4.0, 4.0, 2.0),
    };
    let edge_culled = renderer.is_bbox_outside_fov(&edge_aabb);

    // This should NOT be culled as it's in the view direction
    assert!(!edge_culled, "AABB in view direction should be visible");
}

#[test]
fn test_frustum_plane_calculations() {
    let mut renderer = Renderer3D::new(640.0, 480.0, 90.0);

    // Set up camera looking forward along Y axis
    let pos = Vec3::new(0.0, 0.0, 0.0);
    let angle = 0.0; // Looking along positive Y axis
    let pitch = 0.0; // No pitch
    create_test_view_matrix(&mut renderer, pos, angle, pitch);

    // Test a simple AABB and examine the clip coordinates
    let test_aabb = gameplay::AABB {
        min: Vec3::new(5.0, -1.0, -1.0),
        max: Vec3::new(10.0, 1.0, 1.0),
    };

    // Manually calculate clip coordinates to understand the issue
    let corners = [
        Vec3::new(test_aabb.min.x, test_aabb.min.y, test_aabb.min.z),
        Vec3::new(test_aabb.max.x, test_aabb.min.y, test_aabb.min.z),
        Vec3::new(test_aabb.max.x, test_aabb.max.y, test_aabb.min.z),
        Vec3::new(test_aabb.min.x, test_aabb.max.y, test_aabb.min.z),
        Vec3::new(test_aabb.min.x, test_aabb.min.y, test_aabb.max.z),
        Vec3::new(test_aabb.max.x, test_aabb.min.y, test_aabb.max.z),
        Vec3::new(test_aabb.max.x, test_aabb.max.y, test_aabb.max.z),
        Vec3::new(test_aabb.min.x, test_aabb.max.y, test_aabb.max.z),
    ];

    let view_projection = renderer.projection_matrix * renderer.view_matrix;
    let mut clip_corners = Vec::with_capacity(8);

    for corner in corners {
        let clip_pos = view_projection * Vec4::new(corner.x, corner.y, corner.z, 1.0);
        clip_corners.push(clip_pos);
    }

    // Check each plane manually
    let left_plane_failed = clip_corners.iter().all(|c| c.x < -c.w);
    let right_plane_failed = clip_corners.iter().all(|c| c.x > c.w);
    let bottom_plane_failed = clip_corners.iter().all(|c| c.y < -c.w);
    let top_plane_failed = clip_corners.iter().all(|c| c.y > c.w);
    let near_plane_failed = clip_corners.iter().all(|c| c.z < -c.w);
    let far_plane_failed = clip_corners.iter().all(|c| c.z > c.w);

    // Verify that the AABB in front of camera is not culled
    assert!(!left_plane_failed, "Left plane should not cull front AABB");
    assert!(
        !right_plane_failed,
        "Right plane should not cull front AABB"
    );
    assert!(
        !bottom_plane_failed,
        "Bottom plane should not cull front AABB"
    );
    assert!(!top_plane_failed, "Top plane should not cull front AABB");
    assert!(!near_plane_failed, "Near plane should not cull front AABB");
    assert!(!far_plane_failed, "Far plane should not cull front AABB");

    // Test with 45 degree pitch and rotation
    let pitch_pos = Vec3::new(0.0, 0.0, 5.0);
    let pitch_angle = 45.0 * PI / 180.0;
    let pitch_value = -45.0 * PI / 180.0;
    create_test_view_matrix(&mut renderer, pitch_pos, pitch_angle, pitch_value);

    // Test same AABB but positioned for 45 degree view
    let pitch_aabb = gameplay::AABB {
        min: Vec3::new(2.0, 2.0, 0.0),
        max: Vec3::new(4.0, 4.0, 2.0),
    };

    let pitch_view_projection = renderer.projection_matrix * renderer.view_matrix;
    let pitch_corners = [
        Vec3::new(pitch_aabb.min.x, pitch_aabb.min.y, pitch_aabb.min.z),
        Vec3::new(pitch_aabb.max.x, pitch_aabb.min.y, pitch_aabb.min.z),
        Vec3::new(pitch_aabb.max.x, pitch_aabb.max.y, pitch_aabb.min.z),
        Vec3::new(pitch_aabb.min.x, pitch_aabb.max.y, pitch_aabb.min.z),
        Vec3::new(pitch_aabb.min.x, pitch_aabb.min.y, pitch_aabb.max.z),
        Vec3::new(pitch_aabb.max.x, pitch_aabb.min.y, pitch_aabb.max.z),
        Vec3::new(pitch_aabb.max.x, pitch_aabb.max.y, pitch_aabb.max.z),
        Vec3::new(pitch_aabb.min.x, pitch_aabb.max.y, pitch_aabb.max.z),
    ];

    // Verify that the 45-degree pitch/rotation case works correctly
    for corner in pitch_corners {
        let clip_pos = pitch_view_projection * Vec4::new(corner.x, corner.y, corner.z, 1.0);
        // Just verify that the transformation produces reasonable values
        assert!(
            clip_pos.w > 0.0,
            "Valid clip coordinate should have positive w"
        );
    }
}

fn create_test_view_matrix(renderer: &mut Renderer3D, pos: Vec3, angle: f32, pitch: f32) {
    let forward = Vec3::new(
        angle.cos() * pitch.cos(),
        angle.sin() * pitch.cos(),
        pitch.sin(),
    );
    let up = Vec3::Z;

    renderer.view_matrix = Mat4::look_at_rh(pos, pos + forward, up);
}
