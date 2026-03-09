use crate::{DebugDrawOptions, MAX_CLIPPED_VERTICES, Software3D};
use glam::{Vec3, Vec4};

#[test]
fn test_frustum_clipping() {
    let mut renderer = Software3D::new(800.0, 600.0, 0.8, DebugDrawOptions::default());

    // Test triangle partially outside frustum (clip-space: inside means x,y,z in
    // [-w, w]) w=3.0 so frustum bounds are [-3, 3] on each axis
    let vertices = [
        Vec4::new(0.5, 0.5, 0.0, 3.0), // Inside frustum
        Vec4::new(5.0, 0.5, 0.0, 3.0), // Outside right plane (x > w)
        Vec4::new(0.5, 5.0, 0.0, 3.0), // Outside top plane (y > w)
    ];

    let tex_coords = [
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(1.0, 0.0, 1.0),
        Vec3::new(0.0, 1.0, 1.0),
    ];

    renderer.clip_polygon_frustum(&vertices, &tex_coords, 3);

    // Clipped polygon should have at least 3 vertices
    assert!(renderer.clipped_vertices_len >= 3);
    assert!(renderer.clipped_vertices_len <= MAX_CLIPPED_VERTICES);

    // All clipped vertices should be within frustum bounds
    for i in 0..renderer.clipped_vertices_len {
        let v = renderer.clipped_vertices_buffer[i];
        assert!(
            v.x >= -v.w - 0.001 && v.x <= v.w + 0.001,
            "Vertex {} x={} outside [-w, w] where w={}",
            i,
            v.x,
            v.w
        );
        assert!(
            v.y >= -v.w - 0.001 && v.y <= v.w + 0.001,
            "Vertex {} y={} outside [-w, w] where w={}",
            i,
            v.y,
            v.w
        );
        assert!(
            v.z >= -v.w - 0.001 && v.z <= v.w + 0.001,
            "Vertex {} z={} outside [-w, w] where w={}",
            i,
            v.z,
            v.w
        );
    }
}

#[test]
fn test_frustum_clipping_completely_outside() {
    let mut renderer = Software3D::new(800.0, 600.0, 0.8, DebugDrawOptions::default());

    // Triangle completely outside frustum (all beyond right plane: x > w)
    let vertices = [
        Vec4::new(5.0, 0.0, 0.0, 3.0),
        Vec4::new(6.0, 0.0, 0.0, 3.0),
        Vec4::new(5.5, 1.0, 0.0, 3.0),
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
    let mut renderer = Software3D::new(800.0, 600.0, 0.8, DebugDrawOptions::default());

    // Triangle completely inside frustum (all coords well within [-w, w])
    let vertices = [
        Vec4::new(0.5, 0.5, 0.0, 3.0),
        Vec4::new(-0.5, 0.5, 0.0, 3.0),
        Vec4::new(0.0, -0.5, 0.0, 3.0),
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
    let mut renderer = Software3D::new(800.0, 600.0, 0.8, DebugDrawOptions::default());

    // Setup a triangle crossing the left plane (x = -w)
    // Left plane eq: x + w >= 0, so dot with (1,0,0,1)
    // v0: dot = -2 + 0 + 0 + 3 = 1 > 0 => inside
    // v1: dot = 0.5 + 0 + 0 + 3 = 3.5 > 0 => inside
    // v2: dot = -5 + 0 + 0 + 3 = -2 < 0 => outside left
    renderer.clipped_vertices_buffer[0] = Vec4::new(-2.0, 0.0, 0.0, 3.0); // Inside
    renderer.clipped_vertices_buffer[1] = Vec4::new(0.5, 0.0, 0.0, 3.0); // Inside
    renderer.clipped_vertices_buffer[2] = Vec4::new(-5.0, 0.5, 0.0, 3.0); // Outside left

    renderer.clipped_tex_coords_buffer[0] = Vec3::new(0.0, 0.0, 1.0);
    renderer.clipped_tex_coords_buffer[1] = Vec3::new(1.0, 0.0, 1.0);
    renderer.clipped_tex_coords_buffer[2] = Vec3::new(0.5, 1.0, 1.0);

    renderer.clipped_vertices_len = 3;

    // Clip against left plane: x >= -w
    let left_plane = Vec4::new(1.0, 0.0, 0.0, 1.0);
    renderer.clip_polygon_against_plane(left_plane);

    // v0 inside, v1 inside, v2 outside:
    // Edge v1->v2: exiting, adds intersection
    // Edge v2->v0: entering, adds intersection
    // Result: v0, v1, intersection(v1->v2), intersection(v2->v0) = 4 vertices
    assert_eq!(renderer.clipped_vertices_len, 4);

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
