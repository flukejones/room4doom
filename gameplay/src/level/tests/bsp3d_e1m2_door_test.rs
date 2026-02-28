#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::{MapData, MovementType, PicData};
    use wad::WadData;

    /// Verify that all sector 129 (door) ceiling vertices move when the
    /// ceiling is raised. Regression test for segment-edge matching fix
    /// where a clipped polygon vertex drifted ~1.6 units from the correct
    /// segment endpoint, breaking zero-height wall vertex separation.
    #[test]
    fn test_e1m2_sector129_door_ceiling_moves() {
        let wad = WadData::new(&PathBuf::from("/Users/lukejones/DOOM/doom.wad"));
        let pic_data = PicData::init(&wad);
        let mut map = MapData::default();
        map.load("E1M2", |name| pic_data.flat_num_for_name(name), &wad);

        let bsp3d = &mut map.bsp_3d;
        let initial_positions: Vec<_> = bsp3d.vertices.iter().copied().collect();

        // Move sector 129 ceiling from 0 to 128
        bsp3d.move_surface(129, MovementType::Ceiling, 128.0, 0);

        // Every ceiling vertex in sector 129 must have moved
        let ss_ids = bsp3d.sector_subsectors[129].clone();
        let mut stuck_count = 0;
        for &ssid in &ss_ids {
            let leaf = &bsp3d.subsector_leaves[ssid];
            for &ceil_idx in &leaf.ceiling_polygons {
                let poly = &leaf.polygons[ceil_idx];
                for &vidx in &poly.vertices {
                    let orig = initial_positions[vidx];
                    let curr = bsp3d.vertices[vidx];
                    if (orig.z - curr.z).abs() <= 0.001 {
                        stuck_count += 1;
                    }
                }
            }
        }

        assert_eq!(
            stuck_count, 0,
            "All sector 129 ceiling vertices should move"
        );
    }
}
