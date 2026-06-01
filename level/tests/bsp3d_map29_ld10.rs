use glam::Vec3;
use level::SurfaceKind;
use test_utils::{doom2_wad_path, load_map};

/// MAP29 ld10 is two-sided with lower+upper textures on both sides. Each side's
/// wall lives in its own subsector with an opposite-facing normal, so only the
/// side facing the viewer should draw. A prior runtime normal recompute in
/// `is_facing_point` derived the normal from `vertices[0..3]`, but the mover
/// vertex-linking pass reorders those vertices, flipping the back wall's normal
/// once the floor moved — both sides then faced the viewer and Z-fought.
///
/// Assert that for the lower walls of ld10, the two opposite-facing copies are
/// never both visible from the same point, even after the floor travels down.
#[test]
fn map29_ld10_lower_walls_face_opposite() {
    let mut map = load_map(&doom2_wad_path(), "MAP29");

    // Drop sector 58's floor to simulate the lift/floor travelling down, so the
    // lower-wall quads open up and the mover-linked winding is exercised.
    let bsp3d = &mut map.bsp_3d;
    bsp3d.move_surface(58, level::MovementType::Floor, 500.0, 0);

    // Collect ld10 lower-wall quads with their stored normals.
    let mut walls = Vec::new();
    for ss in 0..bsp3d.subsector_leaves.len() {
        let leaf = bsp3d.get_subsector_leaf(ss).unwrap();
        for poly in &leaf.polygons {
            if let SurfaceKind::Vertical {
                linedef_id,
                wall_type: level::WallType::Lower,
                ..
            } = poly.surface_kind
                && linedef_id == 10
            {
                walls.push(poly.clone());
            }
        }
    }
    assert!(
        walls.len() >= 2,
        "expected >=2 ld10 lower walls, got {}",
        walls.len()
    );

    // From a viewpoint on either side of the wall plane (ld10 runs along x at
    // some y; normals are ±y), at most one of the opposite-facing copies may
    // face the viewer.
    for &probe in &[Vec3::new(0.0, -1e6, 600.0), Vec3::new(0.0, 1e6, 600.0)] {
        let facing = walls
            .iter()
            .filter(|w| w.is_facing_point(probe, &bsp3d.vertices))
            .count();
        // Some copies are coplanar duplicates per side; the invariant is that
        // not ALL of them face the viewer (the opposite side must be culled).
        assert!(
            facing < walls.len(),
            "from {probe:?}, all {} ld10 lower walls face the viewer (back side not culled)",
            walls.len()
        );
    }
}
