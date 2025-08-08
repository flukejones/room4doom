use crate::{MAX_CLIPPED_VERTICES, Software3D};
use glam::{Vec3, Vec4};

#[test]
fn test_frustum_clipping() {
    let mut renderer = Software3D::new(800.0, 600.0, 0.8);

    // Test triangle partially outside frustum
    let vertices = [
        Vec4::new(0.5, 0.5, -2.0, 1.0), // Inside frustum
        Vec4::new(2.0, 0.5, -2.0, 1.0), // Outside right plane
        Vec4::new(0.5, 2.0, -2.0, 1.0), // Outside top plane
    ];

    let tex_coords = [
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(1.0, 0.0, 1.0),
        Vec3::new(0.0, 1.0, 1.0),
    ];

    renderer.clip_polygon_frustum(&vertices, &tex_coords, 3);

    // Clipped polygon should have more vertices than original
    // (corner cases create additional vertices at frustum intersections)
    assert!(renderer.clipped_vertices_len >= 3);
    assert!(renderer.clipped_vertices_len <= MAX_CLIPPED_VERTICES);

    // All clipped vertices should be within frustum bounds
    for i in 0..renderer.clipped_vertices_len {
        let v = renderer.clipped_vertices_buffer[i];
        assert!(v.x >= -v.w && v.x <= v.w);
        assert!(v.y >= -v.w && v.y <= v.w);
        assert!(v.z >= -v.w && v.z <= v.w);
    }
}

#[test]
fn test_frustum_clipping_completely_outside() {
    let mut renderer = Software3D::new(800.0, 600.0, 0.8);

    // Triangle completely outside frustum (all beyond right plane)
    let vertices = [
        Vec4::new(2.0, 0.0, -2.0, 1.0),
        Vec4::new(3.0, 0.0, -2.0, 1.0),
        Vec4::new(2.5, 1.0, -2.0, 1.0),
    ];

    let tex_coords = [
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(1.0, 0.0, 1.0),
        Vec3::new(0.5, 1.0, 1.0),
    ];

    renderer.clip_polygon_frustum(&vertices, &tex_coords, 3);

    // Should be completely clipped away
    assert_eq!(renderer.clipped_vertices_len, 0);
}

#[test]
fn test_frustum_clipping_completely_inside() {
    let mut renderer = Software3D::new(800.0, 600.0, 0.8);

    // Triangle completely inside frustum
    let vertices = [
        Vec4::new(0.1, 0.1, -2.0, 1.0),
        Vec4::new(-0.1, 0.1, -2.0, 1.0),
        Vec4::new(0.0, -0.1, -2.0, 1.0),
    ];

    let tex_coords = [
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(1.0, 0.0, 1.0),
        Vec3::new(0.5, 1.0, 1.0),
    ];

    renderer.clip_polygon_frustum(&vertices, &tex_coords, 3);

    // Should remain unchanged (3 vertices)
    assert_eq!(renderer.clipped_vertices_len, 3);

    // Vertices should be approximately the same
    for i in 0..3 {
        let clipped = renderer.clipped_vertices_buffer[i];
        let original = vertices[i];
        assert!((clipped.x - original.x).abs() < 0.001);
        assert!((clipped.y - original.y).abs() < 0.001);
        assert!((clipped.z - original.z).abs() < 0.001);
        assert!((clipped.w - original.w).abs() < 0.001);
    }
}

#[test]
fn test_clip_against_single_plane() {
    let mut renderer = Software3D::new(800.0, 600.0, 0.8);

    // Setup a triangle crossing the left plane (x = -w)
    renderer.clipped_vertices_buffer[0] = Vec4::new(-2.0, 0.0, -1.0, 1.0); // Outside left
    renderer.clipped_vertices_buffer[1] = Vec4::new(0.5, 0.0, -1.0, 1.0); // Inside
    renderer.clipped_vertices_buffer[2] = Vec4::new(0.0, 0.5, -1.0, 1.0); // Inside

    renderer.clipped_tex_coords_buffer[0] = Vec3::new(0.0, 0.0, 1.0);
    renderer.clipped_tex_coords_buffer[1] = Vec3::new(1.0, 0.0, 1.0);
    renderer.clipped_tex_coords_buffer[2] = Vec3::new(0.5, 1.0, 1.0);

    renderer.clipped_vertices_len = 3;

    // Clip against left plane: x >= -w
    let left_plane = Vec4::new(1.0, 0.0, 0.0, 1.0);
    renderer.clip_polygon_against_plane(left_plane);

    // Should have 3 vertices: intersection point + 2 inside vertices
    assert_eq!(renderer.clipped_vertices_len, 3);

    // All resulting vertices should satisfy the plane equation
    for i in 0..renderer.clipped_vertices_len {
        let v = renderer.clipped_vertices_buffer[i];
        let distance = left_plane.dot(v);
        assert!(
            distance >= -0.001,
            "Vertex {} failed plane test: {}",
            i,
            distance
        );
    }
}
