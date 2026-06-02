use level::{MovementType, SurfaceKind};
use test_utils::{load_map, sigil_wad_path};

/// Z-range of every wall quad on `ld`, paired with its sector.
fn wall_z(bsp3d: &level::BSP3D, ld: usize) -> Vec<(usize, (f32, f32))> {
    let mut out = Vec::new();
    for ss in 0..bsp3d.subsector_leaves.len() {
        for poly in bsp3d.leaf_polygons(ss) {
            if let SurfaceKind::Vertical {
                linedef_id,
                ..
            } = &poly.surface_kind
                && *linedef_id == ld
            {
                let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
                for &v in &poly.vertices {
                    let z = bsp3d.vertices[v].z;
                    lo = lo.min(z);
                    hi = hi.max(z);
                }
                out.push((poly.sector_id, (lo, hi)));
            }
        }
    }
    out
}

/// E5M2: ld890 is a one-sided wall in sector 125 (a floor mover, zero-height at
/// rest). It shares a corner with ld857 in sector 110. When sector 125's floor
/// drops, ld890's bottom must follow while its top stays at the ceiling, and
/// ld857 (a different sector) must not move at all. Regression for the
/// floor-mover surface separation in the mover pass (Step 3).
#[test]
fn e5m2_floor_mover_does_not_drag_neighbour() {
    let mut map = load_map(&sigil_wad_path(), "E5M2");
    let bsp3d = &mut map.bsp_3d;

    let ld857_before = wall_z(bsp3d, 857);
    bsp3d.move_surface(125, MovementType::Floor, -8.0, 0);

    for (sec, (lo, hi)) in wall_z(bsp3d, 890) {
        assert_eq!(sec, 125);
        assert!(
            (lo - -8.0).abs() < 0.5 && (hi - 128.0).abs() < 0.5,
            "ld890 should span -8..128 after the floor drops, got {lo}..{hi}"
        );
    }
    assert_eq!(
        wall_z(bsp3d, 857),
        ld857_before,
        "ld857 (sector 110) must not move when sector 125's floor drops"
    );
}
