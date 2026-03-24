use level::{SurfaceKind, WallType};
use std::collections::HashSet;
use test_utils::{doom_wad_path, load_map};

#[test]
fn test_e1m3_stair_sectors_have_moving_floors() {
    let map = load_map(&doom_wad_path(), "E1M3");

    let bsp3d = &map.bsp_3d;
    let stair_sectors = [16, 17, 18, 19, 8, 9, 10, 11, 12, 13];

    for &sid in &stair_sectors {
        let subsector_ids = &bsp3d.sector_subsectors[sid];
        let mut has_moving_floor = false;
        for &ssid in subsector_ids {
            let leaf = &bsp3d.subsector_leaves[ssid];
            for &pidx in &leaf.floor_polygons {
                if leaf.polygons[pidx].moves {
                    has_moving_floor = true;
                }
            }
        }
        assert!(
            has_moving_floor,
            "Sector {} should have moving floor polygons (zero-height lower wall detected)",
            sid
        );
    }
}

#[test]
fn test_e1m3_stair_sectors_have_lower_walls_between_steps() {
    let map = load_map(&doom_wad_path(), "E1M3");

    let bsp3d = &map.bsp_3d;
    let stair_sectors = [16, 17, 18, 19, 8, 9, 10, 11, 12, 13];

    for &sid in &stair_sectors {
        let subsector_ids = &bsp3d.sector_subsectors[sid];
        let mut lower_wall_count = 0;
        for &ssid in subsector_ids {
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
        assert!(
            lower_wall_count > 0,
            "Sector {} should have at least one lower wall polygon but has none",
            sid
        );
    }
}

#[test]
fn test_e1m3_stair_wall_vertex_sharing() {
    let map = load_map(&doom_wad_path(), "E1M3");

    let bsp3d = &map.bsp_3d;

    let sector_16_floor_verts: HashSet<usize> = bsp3d.sector_subsectors[16]
        .iter()
        .flat_map(|&ssid| {
            let leaf = &bsp3d.subsector_leaves[ssid];
            leaf.floor_polygons
                .iter()
                .flat_map(|&pidx| leaf.polygons[pidx].vertices.clone())
                .collect::<Vec<_>>()
        })
        .collect();

    let sector_17_floor_verts: HashSet<usize> = bsp3d.sector_subsectors[17]
        .iter()
        .flat_map(|&ssid| {
            let leaf = &bsp3d.subsector_leaves[ssid];
            leaf.floor_polygons
                .iter()
                .flat_map(|&pidx| leaf.polygons[pidx].vertices.clone())
                .collect::<Vec<_>>()
        })
        .collect();

    let mut wall_top_shared_with_17 = 0;
    let mut wall_bottom_shared_with_16 = 0;
    let mut total_walls = 0;

    for &ssid in &bsp3d.sector_subsectors[16] {
        let leaf = &bsp3d.subsector_leaves[ssid];
        for poly in &leaf.polygons {
            if let SurfaceKind::Vertical {
                wall_type,
                ..
            } = &poly.surface_kind
            {
                if !matches!(wall_type, WallType::Lower) {
                    continue;
                }
                total_walls += 1;

                let wall_vert_indices: Vec<usize> = poly.vertices.clone();
                let top_shared = wall_vert_indices
                    .iter()
                    .any(|vi| sector_17_floor_verts.contains(vi));
                let bottom_shared = wall_vert_indices
                    .iter()
                    .any(|vi| sector_16_floor_verts.contains(vi));

                if top_shared {
                    wall_top_shared_with_17 += 1;
                }
                if bottom_shared {
                    wall_bottom_shared_with_16 += 1;
                }
            }
        }
    }

    assert!(total_walls > 0, "Sector 16 should have lower walls");
    assert!(
        wall_top_shared_with_17 > 0 || wall_bottom_shared_with_16 > 0,
        "Lower wall vertices should share with adjacent floor polygons"
    );
}
