#[cfg(test)]
mod pvs_tests {
    use glam::Vec2;

    use crate::MapPtr;
    use crate::PicData;
    use crate::Sector;
    use crate::SubSector;
    use crate::level::map_data::MapData;
    use crate::level::pvs::PVS;
    use crate::level::pvs::Portal;
    use crate::level::pvs::PortalType;
    use std::path::Path;
    use std::path::PathBuf;

    #[test]
    fn test_pvs_creation() {
        let pvs = PVS::new(4);
        assert_eq!(pvs.subsector_count(), 4);
    }

    #[test]
    fn test_pvs_integration() {
        let pvs = PVS::new(2);
        assert_eq!(pvs.subsector_count(), 2);
        let visible = pvs.get_visible_subsectors(0);
        assert_eq!(visible.len(), 0);
        let memory = pvs.memory_usage();
        assert!(memory > 0);
    }

    #[test]
    fn test_e1m6_linedef_visibility() {
        let wad_path = match find_wad_file() {
            Some(path) => path,
            None => {
                eprintln!("No WAD files found, skipping test");
                return;
            }
        };

        let wad = wad::WadData::new(PathBuf::from(wad_path));
        let mut map_data = MapData::default();
        map_data.load("E1M6", &PicData::default(), &wad);
        map_data.build_pvs();

        let linedefs = map_data.linedefs();
        let subsectors = map_data.subsectors();

        if linedefs.len() <= 1200 {
            eprintln!("Map E1M6 doesn't have enough linedefs, skipping test");
            return;
        }

        // Test subsector visibility for specific linedef groups
        let source_linedefs = [13, 14, 173];
        let target_linedefs = [214, 215, 217, 1200];

        let mut source_subsectors = Vec::new();
        let mut target_subsectors = Vec::new();

        for (i, subsector) in subsectors.iter().enumerate() {
            let sector_ptr = subsector.sector.inner;

            for &linedef_idx in &source_linedefs {
                if sector_ptr == linedefs[linedef_idx].frontsector.inner {
                    source_subsectors.push(i);
                    break;
                }
            }

            for &linedef_idx in &target_linedefs {
                if sector_ptr == linedefs[linedef_idx].frontsector.inner {
                    target_subsectors.push(i);
                    break;
                }
            }
        }

        // Ensure most source subsectors can see at least one target subsector
        let mut visible_count = 0;
        for &source in &source_subsectors {
            for &target in &target_subsectors {
                if map_data.subsector_visible(source, target) {
                    visible_count += 1;
                    break;
                }
            }
        }

        let min_required = (source_subsectors.len() + 1) / 2;
        assert!(
            visible_count >= min_required,
            "At least {} out of {} source subsectors should see target group (got {})",
            min_required,
            source_subsectors.len(),
            visible_count
        );
    }

    #[test]
    fn test_e1m2_linedef_510_953_scenario() {
        // Test based on actual E1M2 geometry: linedef 510 should see linedef 953
        let mut pvs = PVS::new(2);

        // Create sectors matching E1M2 linedef 510 and 953
        let mut sector_510 = Sector::default();
        sector_510.floorheight = 24.0; // E1M2 linedef 510 sector
        sector_510.ceilingheight = 96.0;

        let mut sector_953 = Sector::default();
        sector_953.floorheight = 40.0; // E1M2 linedef 953 sector
        sector_953.ceilingheight = 320.0;

        // Create subsectors
        let subsector_510 = SubSector {
            seg_count: 1,
            start_seg: 0,
            sector: MapPtr::new(&mut sector_510),
        };

        let subsector_953 = SubSector {
            seg_count: 1,
            start_seg: 1,
            sector: MapPtr::new(&mut sector_953),
        };

        pvs.subsectors = vec![subsector_510, subsector_953];

        // Test height range overlap (should pass)
        assert!(
            pvs.test_height_range_overlap(&sector_510, &sector_953),
            "E1M2 linedef 510/953 sectors should have visibility (step height difference)"
        );

        // Test portal Z range with realistic geometry
        let ray_start = Vec2::new(504.0, 8.0); // Near linedef 510 position
        let ray_end = Vec2::new(1024.0, -64.0); // Near linedef 953 position
        let intersection_point = Vec2::new(760.0, -28.0); // Midpoint

        let portal = Portal {
            front_sector: MapPtr::new(&mut sector_510),
            back_sector: MapPtr::new(&mut sector_953),
            portal_type: PortalType::Open,
            z_position: 24.0, // Floor of lower sector
            z_range: 296.0,   // Up to ceiling of higher sector (320 - 24)
        };

        // Should not be blocked - multiple ray heights should allow visibility
        assert!(
            !pvs.is_ray_blocked_by_portal_height(
                &portal,
                ray_start,
                ray_end,
                intersection_point,
                0,
                1
            ),
            "E1M2 linedef 510/953 should be visible through portal Z range check"
        );
    }

    #[test]
    fn test_e1m2_pvs_visibility_bug() {
        let wad_path = match find_wad_file() {
            Some(path) => path,
            None => {
                eprintln!("No WAD files found, skipping test");
                return;
            }
        };

        let wad = wad::WadData::new(PathBuf::from(wad_path));
        let mut map_data = MapData::default();
        map_data.load("E1M2", &PicData::default(), &wad);
        map_data.build_pvs();

        let linedefs = map_data.linedefs();
        let subsectors = map_data.subsectors();

        if linedefs.len() <= 681 {
            eprintln!("Map E1M2 doesn't have enough linedefs, skipping test");
            return;
        }

        // Find player subsector at (-920, 400)
        let player_pos = (-920.0, 400.0);
        let player_subsector = find_closest_subsector(&map_data, player_pos);

        // Find subsectors containing linedefs 131 and 681
        let linedef_131 = &linedefs[131];
        let linedef_681 = &linedefs[681];

        let mut target_131_subs = Vec::new();
        let mut target_681_subs = Vec::new();

        for (i, subsector) in subsectors.iter().enumerate() {
            let sector_ptr = subsector.sector.inner;
            if sector_ptr == linedef_131.frontsector.inner {
                target_131_subs.push(i);
            }
            if sector_ptr == linedef_681.frontsector.inner {
                target_681_subs.push(i);
            }
        }

        // Test the failing condition: Player at (-920, 400) should see linedefs 131 and 681
        let can_see_131 = target_131_subs
            .iter()
            .any(|&target| map_data.subsector_visible(player_subsector, target));
        let can_see_681 = target_681_subs
            .iter()
            .any(|&target| map_data.subsector_visible(player_subsector, target));

        assert!(
            can_see_131 && can_see_681,
            "Player at (-920, 400) should see linedefs 131 and 681 but PVS blocks them"
        );

        // Test general linedef visibility for debugging
        let test_cases = [(129, 131), (129, 681), (521, 131), (521, 681)];

        for (source_linedef, target_linedef) in test_cases {
            let source_subsectors = find_linedef_subsectors(&map_data, source_linedef);
            let target_subsectors = find_linedef_subsectors(&map_data, target_linedef);

            let visibility_exists = source_subsectors.iter().any(|&source| {
                target_subsectors
                    .iter()
                    .any(|&target| map_data.subsector_visible(source, target))
            });

            println!(
                "Linedef {} -> {}: {}",
                source_linedef,
                target_linedef,
                if visibility_exists {
                    "visible"
                } else {
                    "blocked"
                }
            );
        }
    }

    fn find_wad_file() -> Option<&'static str> {
        let wad_paths = ["../doom1.wad"];
        for path in &wad_paths {
            if Path::new(path).exists() {
                return Some(*path);
            }
        }
        None
    }

    fn find_closest_subsector(map_data: &MapData, pos: (f32, f32)) -> usize {
        let subsectors = map_data.subsectors();
        let segments = map_data.segments();
        let mut best_subsector = 0;
        let mut min_distance = f32::MAX;

        for (i, subsector) in subsectors.iter().enumerate() {
            let mut center_x = 0.0;
            let mut center_y = 0.0;
            let mut point_count = 0;

            for j in 0..subsector.seg_count {
                let seg_idx = subsector.start_seg + j;
                if (seg_idx as usize) < segments.len() {
                    let segment = &segments[seg_idx as usize];
                    center_x += segment.v1.x + segment.v2.x;
                    center_y += segment.v1.y + segment.v2.y;
                    point_count += 2;
                }
            }

            if point_count > 0 {
                center_x /= point_count as f32;
                center_y /= point_count as f32;
                let distance = ((center_x - pos.0).powi(2) + (center_y - pos.1).powi(2)).sqrt();
                if distance < min_distance {
                    min_distance = distance;
                    best_subsector = i;
                }
            }
        }

        best_subsector
    }

    fn find_linedef_subsectors(map_data: &MapData, linedef_idx: usize) -> Vec<usize> {
        let linedefs = map_data.linedefs();
        let subsectors = map_data.subsectors();
        let mut result = Vec::new();

        if linedef_idx < linedefs.len() {
            let linedef = &linedefs[linedef_idx];
            for (i, subsector) in subsectors.iter().enumerate() {
                if subsector.sector.inner == linedef.frontsector.inner {
                    result.push(i);
                }
            }
        }

        result
    }
}
