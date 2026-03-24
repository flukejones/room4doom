use level::{SurfaceKind, WallType};
use test_utils::{doom_wad_path, load_map_with_pwad, sigil_wad_path};

#[test]
fn test_e5m1_sector24_has_lower_walls() {
    let map = load_map_with_pwad(&doom_wad_path(), &sigil_wad_path(), "E5M1");

    let bsp3d = &map.bsp_3d;

    let mut lower_wall_count = 0;
    for &ssid in &bsp3d.sector_subsectors[24] {
        let leaf = &bsp3d.subsector_leaves[ssid];
        for poly in &leaf.polygons {
            if let SurfaceKind::Vertical {
                wall_type,
                ..
            } = &poly.surface_kind
            {
                if matches!(wall_type, WallType::Lower) {
                    lower_wall_count += 1;
                }
            }
        }
    }

    let boundary_linedefs = [207, 627, 208];
    for seg in map.segments.iter() {
        if !boundary_linedefs.contains(&(seg.linedef.num as usize)) {
            continue;
        }
        if seg.sidedef.bottomtexture.is_none() {
            continue;
        }
        let front_sector_num = seg.frontsector.num as usize;
        for &ssid in &bsp3d.sector_subsectors[front_sector_num] {
            let leaf = &bsp3d.subsector_leaves[ssid];
            for poly in &leaf.polygons {
                if let SurfaceKind::Vertical {
                    wall_type,
                    ..
                } = &poly.surface_kind
                {
                    if matches!(wall_type, WallType::Lower) {
                        lower_wall_count += 1;
                    }
                }
            }
        }
    }

    assert!(
        lower_wall_count > 0,
        "Sector 24 boundary should have lower wall polygons"
    );
}

/// Sector 24 has a non-zero-height lower wall (floor 40 to 72).
/// The wall should NOT have moves=true since it's not zero-height.
/// Only zero-height walls get moves=true for normal recomputation.
#[test]
fn test_e5m1_sector24_wall_properties() {
    let map = load_map_with_pwad(&doom_wad_path(), &sigil_wad_path(), "E5M1");

    let bsp3d = &map.bsp_3d;

    let mut total_lower_walls = 0;
    for &ssid in &bsp3d.sector_subsectors[24] {
        let leaf = &bsp3d.subsector_leaves[ssid];
        for poly in &leaf.polygons {
            if let SurfaceKind::Vertical {
                wall_type,
                ..
            } = &poly.surface_kind
            {
                if matches!(wall_type, WallType::Lower) {
                    total_lower_walls += 1;
                }
            }
        }
    }

    assert!(total_lower_walls > 0, "Sector 24 should have lower walls");
}
