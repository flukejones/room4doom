use level::{SurfaceKind, WallType};
use test_utils::{doom2_wad_path, load_map};

/// MAP29 ld10 is two-sided with upper+lower textures on both sides. The lower
/// wall is ONE polygon carrying a `front` and `back` face, shared by the leaves
/// either side of the line (no coplanar twin to Z-fight).
#[test]
fn map29_ld10_single_shared_lower_wall() {
    let map = load_map(&doom2_wad_path(), "MAP29");
    let bsp3d = &map.bsp_3d;

    let mut found = None;
    for (gi, poly) in bsp3d.polygons.iter().enumerate() {
        if let SurfaceKind::Vertical {
            linedef_id: 10,
            wall_type: WallType::Lower,
            front,
            back,
            ..
        } = &poly.surface_kind
        {
            assert!(found.is_none(), "ld10 lower must be a single polygon");
            found = Some((
                gi,
                poly,
                front.texture,
                back.as_ref().and_then(|f| f.texture),
            ));
        }
    }
    let (gi, poly, front_tex, back_tex) = found.expect("ld10 lower wall exists");
    assert!(
        front_tex.is_some() && back_tex.is_some(),
        "both sidedefs textured; both faces should carry a texture"
    );

    let refs = (0..bsp3d.subsector_leaves.len())
        .filter(|&ss| {
            bsp3d
                .get_subsector_leaf(ss)
                .unwrap()
                .polygon_indices
                .contains(&gi)
        })
        .count();
    assert!(
        refs >= 2,
        "shared wall must be referenced by both sides, got {refs}"
    );

    // At rest the quad is not flipped, so visible_face is the front face.
    assert_eq!(
        poly.visible_face(&bsp3d.vertices).map(|f| f.texture),
        Some(front_tex)
    );
}
