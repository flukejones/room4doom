use level::{MovementType, SurfaceKind, WallType};
use test_utils::{load_map, sigil_wad_path};

/// E5M2 ld164: a door (sector 123) drops from ceiling height to a pit. Its
/// lower wall is a single two-sided quad. While sec123 sits high the wall faces
/// sector 122 and shows the front sidedef; once sec123 travels below the floor
/// the quad inverts, the normal flips, and the back sidedef must show.
#[test]
fn ld164_lower_face_flips_when_door_drops() {
    let mut map = load_map(&sigil_wad_path(), "E5M2");
    let bsp3d = &mut map.bsp_3d;

    let (ss, front_tex, back_tex) = {
        let mut found = None;
        for ss in 0..bsp3d.subsector_leaves.len() {
            for poly in bsp3d.leaf_polygons(ss) {
                if let SurfaceKind::Vertical {
                    linedef_id: 164,
                    wall_type: WallType::Lower,
                    front,
                    back,
                    ..
                } = &poly.surface_kind
                {
                    found = Some((ss, front.texture, back.as_ref().and_then(|f| f.texture)));
                }
            }
        }
        let (ss, f, b) = found.expect("ld164 lower wall exists");
        (ss, f, b)
    };
    assert!(back_tex.is_some(), "ld164 back sidedef should be textured");

    let face_tex = |bsp3d: &level::BSP3D| {
        bsp3d
            .leaf_polygons(ss)
            .find_map(|p| match &p.surface_kind {
                SurfaceKind::Vertical {
                    linedef_id: 164,
                    wall_type: WallType::Lower,
                    ..
                } => p.visible_face(&bsp3d.vertices).map(|f| f.texture),
                _ => None,
            })
            .expect("visible face")
    };

    // At rest: front face visible.
    assert_eq!(face_tex(bsp3d), front_tex, "at rest shows front sidedef");

    // Drop sec123 below sec122's floor: the wall inverts → back face visible.
    bsp3d.move_surface(123, MovementType::Floor, -152.0, 0);
    assert_eq!(
        face_tex(bsp3d),
        back_tex,
        "after the door drops the back sidedef shows"
    );
}
