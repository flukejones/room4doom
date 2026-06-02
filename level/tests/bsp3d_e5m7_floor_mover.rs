use level::{MovementType, SurfaceKind, WallType};
use test_utils::{doom_wad_path, load_map_with_flats, sigil_wad_path};
use wad::WadData;

fn load() -> level::LevelData {
    let mut wad = WadData::new(&doom_wad_path());
    wad.add_file(sigil_wad_path());
    load_map_with_flats(&wad, "E5M7").0
}

/// Every z value of every Middle wall quad on `ld`.
fn wall_zs(bsp3d: &level::BSP3D, ld: usize) -> Vec<f32> {
    let mut out = Vec::new();
    for ss in 0..bsp3d.subsector_leaves.len() {
        for poly in bsp3d.leaf_polygons(ss) {
            if let SurfaceKind::Vertical {
                linedef_id,
                wall_type: WallType::Middle,
                ..
            } = &poly.surface_kind
                && *linedef_id == ld
            {
                out.extend(poly.vertices.iter().map(|&v| bsp3d.vertices[v].z));
            }
        }
    }
    out
}

/// E5M7 sector 819 is a floor mover (448 → 160) whose ceiling is sky. Its
/// neighbours 820/821 have one-sided perimeter walls reaching the sky ceiling
/// (z=448). At a shared corner those wall tops coincide with sector 819's floor
/// (also 448 at rest). When 819's floor drops, those neighbour wall tops must
/// stay at the ceiling, not follow the floor down. Regression for
/// foreign-sector separation of a moving surface (mover pass Step 3).
#[test]
fn e5m7_floor_mover_does_not_drag_sky_walls() {
    let mut map = load();
    let bsp3d = &mut map.bsp_3d;

    // Move part-way so a drag would be visible (160 would close the gap).
    bsp3d.move_surface(819, MovementType::Floor, 300.0, 0);

    // Neighbour one-sided perimeter Middle walls (sectors 820 and 821): every
    // vertex must stay at the static floor (160) or sky ceiling (448) — never
    // the mover's intermediate height (300).
    for ld in [5530usize, 5531, 5532, 5533, 1010, 1011, 1036] {
        for z in wall_zs(bsp3d, ld) {
            assert!(
                (z - 160.0).abs() < 0.5 || (z - 448.0).abs() < 0.5,
                "neighbour wall ld{ld} vertex dragged to {z} (expected 160 or 448)"
            );
        }
    }

    // Sector 819's own one-sided walls track the floor: bottom 300, top 448.
    for ld in [5534usize, 5539] {
        let mut found = false;
        for ss in 0..bsp3d.subsector_leaves.len() {
            for poly in bsp3d.leaf_polygons(ss) {
                if let SurfaceKind::Vertical {
                    linedef_id,
                    ..
                } = &poly.surface_kind
                    && *linedef_id == ld
                {
                    let lo = poly
                        .vertices
                        .iter()
                        .map(|&v| bsp3d.vertices[v].z)
                        .fold(f32::INFINITY, f32::min);
                    assert!(
                        (lo - 300.0).abs() < 0.5,
                        "ld{ld} bottom should track floor, got {lo}"
                    );
                    found = true;
                }
            }
        }
        assert!(found, "ld{ld} wall not found");
    }
}
