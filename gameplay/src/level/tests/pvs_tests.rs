#[cfg(test)]
mod pvs_tests {
    use crate::level::pvs::PVS;

    #[test]
    fn test_pvs_creation() {
        let pvs = PVS::new(4);
        assert_eq!(pvs.subsector_count(), 4);
    }

    #[test]
    fn test_visibility_setting() {
        let mut pvs = PVS::new(3);

        // New API only supports setting visible, not blocking
        pvs.set_visible(0, 1);

        assert!(pvs.is_visible(0, 1));
        assert!(!pvs.is_visible(1, 2)); // Default is not visible
    }

    #[test]
    fn test_pvs_integration() {
        let pvs = PVS::new(2);

        // Test subsector count
        assert_eq!(pvs.subsector_count(), 2);

        // Test get_visible_subsectors - initially empty since we haven't set any visibility
        let visible = pvs.get_visible_subsectors(0);
        assert_eq!(visible.len(), 0);

        // Test memory usage calculation
        let memory = pvs.memory_usage();
        assert!(memory > 0);
    }

    #[test]
    fn test_portal_based_visibility() {
        let mut pvs = PVS::new(10);

        // Set some visibility relationships
        pvs.set_visible(0, 1);
        pvs.set_visible(0, 2);
        pvs.set_visible(1, 3);

        // Test individual visibility
        assert!(pvs.is_visible(0, 1));
        assert!(pvs.is_visible(0, 2));
        assert!(pvs.is_visible(1, 3));
        assert!(!pvs.is_visible(2, 3)); // Not set, so not visible

        // Test get_visible_subsectors
        let visible_from_0 = pvs.get_visible_subsectors(0);
        assert!(visible_from_0.contains(&1));
        assert!(visible_from_0.contains(&2));
        assert!(!visible_from_0.contains(&3));

        let visible_from_1 = pvs.get_visible_subsectors(1);
        assert!(visible_from_1.contains(&3));
        assert!(!visible_from_1.contains(&2));
    }

    #[test]
    fn test_e1m6_specific_linedef_visibility() {
        use crate::PicData;
        use crate::level::map_data::MapData;
        use std::path::Path;
        use std::path::PathBuf;

        // Try to find WAD files - check both doom1.wad and doom2.wad in project root
        let wad_paths = ["../doom1.wad"];
        let mut wad_path = None;
        let map_name = "E1M6";

        for path in &wad_paths {
            if Path::new(path).exists() {
                wad_path = Some(*path);
                break;
            }
        }

        let wad_path = match wad_path {
            Some(path) => path,
            None => {
                eprintln!("No WAD files found, skipping test");
                return;
            }
        };

        let wad = wad::WadData::new(PathBuf::from(wad_path));

        let mut map_data = MapData::default();
        map_data.load(map_name, &PicData::default(), &wad);

        // Build PVS for the map
        map_data.build_pvs();

        let linedefs = map_data.linedefs();
        let subsectors = map_data.subsectors();

        // Check if we have enough linedefs for the test
        if linedefs.len() <= 1200 {
            eprintln!(
                "Map {} doesn't have enough linedefs ({}), skipping test",
                map_name,
                linedefs.len()
            );
            return;
        }

        // Find sectors using linedef numbers 13, 14, 173
        let linedef_13 = &linedefs[13];
        let linedef_14 = &linedefs[14];
        let linedef_173 = &linedefs[173];

        // Find sectors using linedef 214, 215, 217
        let linedef_214 = &linedefs[214];
        let linedef_215 = &linedefs[215];
        let linedef_217 = &linedefs[217];
        let linedef_1200 = &linedefs[1200];

        println!(
            "Linedef 13 front sector: {:p}",
            linedef_13.frontsector.inner
        );
        println!(
            "Linedef 14 front sector: {:p}",
            linedef_14.frontsector.inner
        );
        println!(
            "Linedef 173 front sector: {:p}",
            linedef_173.frontsector.inner
        );
        println!(
            "Linedef 214 front sector: {:p}",
            linedef_214.frontsector.inner
        );
        println!(
            "Linedef 215 front sector: {:p}",
            linedef_215.frontsector.inner
        );
        println!(
            "Linedef 217 front sector: {:p}",
            linedef_217.frontsector.inner
        );
        println!(
            "Linedef 1200 front sector: {:p}",
            linedef_1200.frontsector.inner
        );

        // Find subsectors that belong to these sectors
        let mut subsectors_group1 = Vec::new();
        let mut subsectors_group2 = Vec::new();

        for (i, subsector) in subsectors.iter().enumerate() {
            let sector_ptr = subsector.sector.inner;

            // Check if this subsector belongs to any of the first group sectors
            if sector_ptr == linedef_13.frontsector.inner
                || sector_ptr == linedef_14.frontsector.inner
                || sector_ptr == linedef_173.frontsector.inner
            {
                subsectors_group1.push(i);
            }

            // Check if this subsector belongs to any of the second group sectors
            if sector_ptr == linedef_214.frontsector.inner
                || sector_ptr == linedef_215.frontsector.inner
                || sector_ptr == linedef_217.frontsector.inner
                || sector_ptr == linedef_1200.frontsector.inner
            {
                subsectors_group2.push(i);
            }
        }

        println!("Group 1 subsectors (13,14,173): {:?}", subsectors_group1);
        println!(
            "Group 2 subsectors (214,215,217,1200): {:?}",
            subsectors_group2
        );

        // Test specific linedef 1200 ray intersection from subsectors containing lines 13, 14, 173
        println!("Testing linedef 1200 ray intersection from source subsectors...");

        // Get linedef 1200 coordinates for ray intersection testing
        let line_start = linedef_1200.v1;
        let line_end = linedef_1200.v2;
        println!(
            "Linedef 1200: ({}, {}) to ({}, {})",
            line_start.x, line_start.y, line_end.x, line_end.y
        );

        // Test visibility between the groups
        let mut any_visible = false;
        for &sub1 in &subsectors_group1 {
            for &sub2 in &subsectors_group2 {
                let visible = map_data.subsector_visible(sub1, sub2);
                println!("Subsector {} -> {}: {}", sub1, sub2, visible);

                if visible {
                    any_visible = true;
                }
            }
        }

        println!("Any visibility found: {}", any_visible);

        // Test specific case: subsector 272 should see subsector 464
        if subsectors.len() > 464 {
            let visible_272_464 = map_data.subsector_visible(272, 464);
            println!("Subsector 272 -> 464: {}", visible_272_464);
            if !visible_272_464 {
                println!(
                    "NOTE: Subsector 272 cannot see subsector 464 - this is not correct as other segs in the subsector can see"
                );
            } else {
                println!("SUCCESS: Subsector 272 can see subsector 464");
            }
        }

        // Check the specific subsector 272 -> 346 visibility
        if subsectors_group1.contains(&272) && subsectors_group2.contains(&346) {
            let visible = map_data.subsector_visible(272, 346);
            assert!(
                visible,
                "Subsector 272 should be able to see subsector 346 after PVS fix"
            );
        }

        // Verify that the specific subsector 272 -> 346 visibility issue is fixed
        if subsectors_group1.contains(&272) && subsectors_group2.contains(&346) {
            let visible = map_data.subsector_visible(272, 464);
            assert!(
                visible,
                "Subsector 272 should be able to see subsector 464 after PVS fix"
            );
        }

        // Ensure all subsectors from group 1 can see at least one subsector from group 2
        for &sub1 in &subsectors_group1 {
            let mut can_see_any = false;
            for &sub2 in &subsectors_group2 {
                if map_data.subsector_visible(sub1, sub2) {
                    can_see_any = true;
                    break;
                }
            }
            assert!(
                can_see_any,
                "Subsector {} should be able to see at least one subsector from the target group",
                sub1
            );
        }
    }
}
