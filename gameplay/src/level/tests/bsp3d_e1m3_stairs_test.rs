#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::path::PathBuf;

    use crate::{MapData, PicData, SurfaceKind, WallType};
    use wad::WadData;

    #[test]
    fn test_e1m3_stair_sectors_have_moving_floors() {
        let wad = WadData::new(&PathBuf::from("/Users/lukejones/DOOM/doom.wad"));
        let mut map = MapData::default();
        map.load("E1M3", &&PicData::init(&wad), &wad);

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
        let wad = WadData::new(&PathBuf::from("/Users/lukejones/DOOM/doom.wad"));
        let mut map = MapData::default();
        map.load("E1M3", &&PicData::init(&wad), &wad);

        let bsp3d = &map.bsp_3d;
        let stair_sectors = [16, 17, 18, 19, 8, 9, 10, 11, 12, 13];

        for &sid in &stair_sectors {
            let subsector_ids = &bsp3d.sector_subsectors[sid];
            let mut lower_wall_count = 0;
            for &ssid in subsector_ids {
                let leaf = &bsp3d.subsector_leaves[ssid];
                for poly in &leaf.polygons {
                    if let SurfaceKind::Vertical { wall_type, .. } = &poly.surface_kind {
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
        let wad = WadData::new(&PathBuf::from("/Users/lukejones/DOOM/doom.wad"));
        let mut map = MapData::default();
        map.load("E1M3", &&PicData::init(&wad), &wad);

        let bsp3d = &map.bsp_3d;

        // Sector 16 -> 17 boundary: both at floor height 48 (zero-height wall).
        // Lower wall top verts use LowerSeparated, bottom verts use Lower.
        // Sector 16 and 17 floor polygons both use LowerSeparated (has_lower = true).
        // So wall top verts should share indices with both floors' vertices.

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

        // Find lower wall polygons in sector 16's subsectors
        let mut wall_top_shared_with_17 = 0;
        let mut wall_bottom_shared_with_16 = 0;
        let mut total_walls = 0;

        for &ssid in &bsp3d.sector_subsectors[16] {
            let leaf = &bsp3d.subsector_leaves[ssid];
            for poly in &leaf.polygons {
                if let SurfaceKind::Vertical { wall_type, .. } = &poly.surface_kind {
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
}
