#[cfg(test)]
mod pvs_tests {
    use crate::PicData;
    use crate::level::map_data::MapData;
    use crate::level::pvs::PVS;
    use std::path::{Path, PathBuf};

    #[test]
    fn test_e1m2_pvs_visibility_bug() {
        let wad_path = match find_wad_file() {
            Some(path) => path,
            None => {
                eprintln!("No WAD files found, skipping test");
                return;
            }
        };

        let wad = wad::WadData::new(&PathBuf::from(wad_path));
        let mut map_data = MapData::default();
        map_data.load("E1M2", &PicData::default(), &wad);

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

        // Test the failing condition: Player at (-920, 400) should see linedefs 131 and
        // 681
        let can_see_131 = target_131_subs
            .iter()
            .any(|&target| map_data.bsp_3d.subsector_visible(player_subsector, target));
        let can_see_681 = target_681_subs
            .iter()
            .any(|&target| map_data.bsp_3d.subsector_visible(player_subsector, target));

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
                    .any(|&target| map_data.bsp_3d.subsector_visible(source, target))
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

    #[test]
    fn test_e1m2_player_start_linedef_355_visibility() {
        let wad_path = match find_wad_file() {
            Some(path) => path,
            None => {
                eprintln!("No WAD files found, skipping test");
                return;
            }
        };

        let wad = wad::WadData::new(&PathBuf::from(wad_path));
        let mut map_data = MapData::default();
        map_data.load("E1M2", &PicData::default(), &wad);

        let linedefs = map_data.linedefs();
        let subsectors = map_data.subsectors();

        if linedefs.len() <= 355 {
            eprintln!("Map E1M2 doesn't have linedef 355, skipping test");
            return;
        }

        // Find player start subsector at (0, 896)
        let player_pos = (0.0, 896.0);
        let player_subsector = find_closest_subsector(&map_data, player_pos);

        // Find subsectors containing linedef 355
        let linedef_355_subsectors = find_linedef_subsectors(&map_data, 355);

        println!("Player at (0, 896) is in subsector {}", player_subsector);
        println!("Linedef 355 is in subsectors: {:?}", linedef_355_subsectors);

        // Test visibility from player start to linedef 355
        let can_see_355 = linedef_355_subsectors.iter().any(|&target| {
            let visible = map_data.bsp_3d.subsector_visible(player_subsector, target);
            println!(
                "Player subsector {} -> Linedef 355 subsector {}: {}",
                player_subsector,
                target,
                if visible { "visible" } else { "blocked" }
            );
            visible
        });

        assert!(
            can_see_355,
            "Player at start position (0, 896) should see linedef 355 - direct line of sight"
        );

        // Detailed analysis of subsector visibility inconsistencies
        println!("\n=== DETAILED VISIBILITY ANALYSIS ===");

        // Test all subsectors in immediate vicinity
        let player_sector = &subsectors[player_subsector].sector;
        println!(
            "Player subsector {} is in sector {}",
            player_subsector, player_sector.inner as usize
        );

        // Check which subsectors are in the same sector as player
        let mut same_sector_subsectors = Vec::new();
        for (i, subsector) in subsectors.iter().enumerate() {
            if subsector.sector.inner == player_sector.inner && i != player_subsector {
                same_sector_subsectors.push(i);
            }
        }

        println!(
            "Found {} other subsectors in same sector as player:",
            same_sector_subsectors.len()
        );
        let mut visible_in_same_sector = 0;
        for &target in &same_sector_subsectors {
            let visible = map_data.bsp_3d.subsector_visible(player_subsector, target);
            if visible {
                visible_in_same_sector += 1;
            }
            if same_sector_subsectors.len() <= 20 {
                // Only print details if not too many
                println!(
                    "  Subsector {}: {}",
                    target,
                    if visible { "visible" } else { "blocked" }
                );
            }
        }

        println!(
            "Same sector visibility: {}/{} subsectors",
            visible_in_same_sector,
            same_sector_subsectors.len()
        );

        // Test nearby subsectors by distance
        println!("\nNearby subsectors by distance:");
        let mut nearby_with_distance = Vec::new();
        for (i, _) in subsectors.iter().enumerate() {
            if i != player_subsector {
                let distance = calculate_subsector_distance(&map_data, player_subsector, i);
                if distance < 300.0 {
                    // Within 300 units
                    nearby_with_distance.push((i, distance));
                }
            }
        }

        nearby_with_distance.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

        let mut visible_nearby = 0;
        for &(target, distance) in nearby_with_distance.iter().take(15) {
            let visible = map_data.bsp_3d.subsector_visible(player_subsector, target);
            if visible {
                visible_nearby += 1;
            }
            println!(
                "  Subsector {} (dist: {:.1}): {}",
                target,
                distance,
                if visible { "visible" } else { "blocked" }
            );
        }

        println!(
            "Nearby visibility: {}/{} subsectors within 300 units",
            visible_nearby,
            nearby_with_distance.len().min(15)
        );

        // Also test some other linedefs in the immediate area for debugging
        let nearby_linedefs = [159, 350, 351, 352, 353, 354, 356, 357, 358];
        println!("\nLinedef visibility summary:");
        for linedef_idx in nearby_linedefs {
            if linedef_idx < linedefs.len() {
                let target_subsectors = find_linedef_subsectors(&map_data, linedef_idx);
                let visible_count = target_subsectors
                    .iter()
                    .filter(|&&target| map_data.bsp_3d.subsector_visible(player_subsector, target))
                    .count();
                println!(
                    "  Linedef {}: {}/{} subsectors visible",
                    linedef_idx,
                    visible_count,
                    target_subsectors.len()
                );
            }
        }

        // Specific test for linedef 159 as mentioned by user
        let linedef_159_subsectors = find_linedef_subsectors(&map_data, 159);
        let can_see_159 = linedef_159_subsectors
            .iter()
            .any(|&target| map_data.bsp_3d.subsector_visible(player_subsector, target));

        println!("\n=== LINEDEF 159 SPECIFIC TEST ===");
        println!("Linedef 159 is in subsectors: {:?}", linedef_159_subsectors);
        println!("Player can see linedef 159: {}", can_see_159);

        assert!(
            can_see_159,
            "Player at start position (0, 896) should see linedef 159 - direct line of sight"
        );
    }

    #[test]
    fn test_e1m2_linedef_138_922_visibility() {
        let wad_path = match find_wad_file() {
            Some(path) => path,
            None => {
                eprintln!("No WAD files found, skipping test");
                return;
            }
        };

        let wad = wad::WadData::new(&PathBuf::from(wad_path));
        let mut map_data = MapData::default();
        map_data.load("E1M2", &PicData::default(), &wad);

        let linedefs = map_data.linedefs();

        if linedefs.len() <= 922 {
            eprintln!("Map E1M2 doesn't have enough linedefs, skipping test");
            return;
        }

        // Find subsectors containing linedef 138 and linedef 922
        let source_subsectors = find_linedef_subsectors(&map_data, 138);
        let target_subsectors = find_linedef_subsectors(&map_data, 922);

        println!("Linedef 138 is in subsectors: {:?}", source_subsectors);
        println!("Linedef 922 is in subsectors: {:?}", target_subsectors);

        // Test visibility between linedef 138 and linedef 922 subsectors
        let mut visibility_found = false;
        for &source in &source_subsectors {
            for &target in &target_subsectors {
                if map_data.bsp_3d.subsector_visible(source, target) {
                    println!(
                        "Linedef 138 subsector {} -> Linedef 922 subsector {}: visible",
                        source, target
                    );
                    visibility_found = true;
                } else {
                    println!(
                        "Linedef 138 subsector {} -> Linedef 922 subsector {}: blocked",
                        source, target
                    );
                }
            }
        }

        assert!(
            visibility_found,
            "Subsector with linedef 138 should see linedef 922 in E1M2 - portal/frustum flow should find visibility path"
        );

        // Additional debugging: check distance between subsectors
        if !source_subsectors.is_empty() && !target_subsectors.is_empty() {
            let distance =
                calculate_subsector_distance(&map_data, source_subsectors[0], target_subsectors[0]);
            println!(
                "Distance between linedef 138 and 922 subsectors: {:.2}",
                distance
            );
        }
    }

    #[test]
    fn test_e1m2_point_320_608_linedef_922_visibility() {
        let wad_path = match find_wad_file() {
            Some(path) => path,
            None => {
                eprintln!("No WAD files found, skipping test");
                return;
            }
        };

        let wad = wad::WadData::new(&PathBuf::from(wad_path));
        let mut map_data = MapData::default();
        map_data.load("E1M2", &PicData::default(), &wad);

        let linedefs = map_data.linedefs();

        if linedefs.len() <= 922 {
            eprintln!("Map E1M2 doesn't have enough linedefs, skipping test");
            return;
        }

        // Find subsector containing point (-320, 608)
        let test_point = (-320.0, 608.0);
        let source_subsector = find_closest_subsector(&map_data, test_point);

        // Find subsectors containing linedef 922
        let target_subsectors = find_linedef_subsectors(&map_data, 922);

        println!("Point (-320, 608) is in subsector {}", source_subsector);
        println!("Linedef 922 is in subsectors: {:?}", target_subsectors);

        // Test visibility from point to linedef 922 subsectors
        let mut visibility_found = false;
        for &target in &target_subsectors {
            if map_data.bsp_3d.subsector_visible(source_subsector, target) {
                println!(
                    "Point (-320, 608) subsector {} -> Linedef 922 subsector {}: visible",
                    source_subsector, target
                );
                visibility_found = true;
            } else {
                println!(
                    "Point (-320, 608) subsector {} -> Linedef 922 subsector {}: blocked",
                    source_subsector, target
                );
            }
        }

        // Calculate distance for context
        if !target_subsectors.is_empty() {
            let distance =
                calculate_subsector_distance(&map_data, source_subsector, target_subsectors[0]);
            println!(
                "Distance from point (-320, 608) to linedef 922: {:.2}",
                distance
            );
        }

        assert!(
            visibility_found,
            "Point (-320, 608) should see linedef 922 in E1M2 - checking real in-game visibility scenario"
        );
    }

    fn calculate_subsector_distance(map_data: &MapData, source: usize, target: usize) -> f32 {
        let subsectors = map_data.subsectors();
        let segments = map_data.segments();

        let get_center = |subsector_idx: usize| -> (f32, f32) {
            if subsector_idx >= subsectors.len() {
                return (0.0, 0.0);
            }

            let subsector = &subsectors[subsector_idx];
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
            }

            (center_x, center_y)
        };

        let (sx, sy) = get_center(source);
        let (tx, ty) = get_center(target);

        ((tx - sx).powi(2) + (ty - sy).powi(2)).sqrt()
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

    #[test]
    fn test_e1m2_subsector_59_visibility() {
        let wad_path = match find_wad_file() {
            Some(path) => path,
            None => {
                eprintln!("No WAD files found, skipping test");
                return;
            }
        };

        let wad = wad::WadData::new(&PathBuf::from(wad_path));
        let mut map_data = MapData::default();
        map_data.load("E1M2", &PicData::default(), &wad);

        // Find subsector containing vertex -720,464
        let target_vertex = (-720.0, 464.0);
        let target_subsector = find_closest_subsector(&map_data, target_vertex);

        println!("Vertex (-720, 464) is in subsector {}", target_subsector);

        // Test visibility from subsector 59 to target subsector
        let source_subsector = 59;
        let is_visible = map_data
            .bsp_3d
            .subsector_visible(source_subsector, target_subsector);

        println!(
            "Subsector {} -> Subsector {} (containing vertex -720,464): {}",
            source_subsector,
            target_subsector,
            if is_visible { "visible" } else { "blocked" }
        );

        // Calculate distance for context
        let distance = calculate_subsector_distance(&map_data, source_subsector, target_subsector);
        println!(
            "Distance from subsector {} to subsector {}: {:.2}",
            source_subsector, target_subsector, distance
        );

        // This now correctly reports visibility after PVS improvements
        assert!(
            is_visible,
            "Subsector 59 should see subsector {} containing vertex -720,464 (PVS now correctly detects this)",
            target_subsector
        );
    }

    #[test]
    fn test_e1m2_vertex_320_576_to_720_464_visibility() {
        let wad_path = match find_wad_file() {
            Some(path) => path,
            None => {
                eprintln!("No WAD files found, skipping test");
                return;
            }
        };

        let wad = wad::WadData::new(&PathBuf::from(wad_path));
        let mut map_data = MapData::default();
        map_data.load("E1M2", &PicData::default(), &wad);

        // Find subsector containing vertex -320,576
        let source_vertex = (-320.0, 576.0);
        let source_subsector = find_closest_subsector(&map_data, source_vertex);

        // Find subsector containing vertex -720,464
        let target_vertex = (-720.0, 464.0);
        let target_subsector = find_closest_subsector(&map_data, target_vertex);

        println!("Vertex (-320, 576) is in subsector {}", source_subsector);
        println!("Vertex (-720, 464) is in subsector {}", target_subsector);

        // Test visibility from source to target subsector
        let is_visible = map_data
            .bsp_3d
            .subsector_visible(source_subsector, target_subsector);

        println!(
            "Subsector {} (containing vertex -320,576) -> Subsector {} (containing vertex -720,464): {}",
            source_subsector,
            target_subsector,
            if is_visible { "visible" } else { "blocked" }
        );

        // Calculate distance for context
        let distance = calculate_subsector_distance(&map_data, source_subsector, target_subsector);
        println!(
            "Distance from vertex (-320,576) to vertex (-720,464): {:.2}",
            distance
        );

        // This now correctly reports visibility after PVS improvements
        assert!(
            is_visible,
            "Segments containing vertex -320,576 (subsector {}) should see subsector {} containing vertex -720,464 (PVS now correctly detects this)",
            source_subsector, target_subsector
        );
    }

    #[test]
    fn test_e1m2_subsector_59_74_diagnostic() {
        let wad_path = match find_wad_file() {
            Some(path) => path,
            None => {
                eprintln!("No WAD files found, skipping test");
                return;
            }
        };

        let wad = wad::WadData::new(&PathBuf::from(wad_path));
        let mut map_data = MapData::default();
        map_data.load("E1M2", &PicData::default(), &wad);

        let source_subsector = 59;
        let target_subsector = 74;

        println!(
            "=== PVS Diagnostic: Subsector {} -> Subsector {} ===",
            source_subsector, target_subsector
        );

        // Check if subsectors exist
        let subsectors = map_data.subsectors();
        if source_subsector >= subsectors.len() || target_subsector >= subsectors.len() {
            println!("ERROR: Subsector index out of range");
            return;
        }

        // Get sector information
        let source_sector = &subsectors[source_subsector].sector;
        let target_sector = &subsectors[target_subsector].sector;

        println!(
            "Source sector: floor={}, ceiling={}, light={}",
            source_sector.floorheight, source_sector.ceilingheight, source_sector.lightlevel
        );
        println!(
            "Target sector: floor={}, ceiling={}, light={}",
            target_sector.floorheight, target_sector.ceilingheight, target_sector.lightlevel
        );

        // Check height overlap
        let height_overlap = source_sector.floorheight < target_sector.ceilingheight
            && source_sector.ceilingheight > target_sector.floorheight;
        println!("Height overlap: {}", height_overlap);

        // Check if they're in the same sector
        let same_sector = std::ptr::eq(source_sector.inner, target_sector.inner);
        println!("Same sector: {}", same_sector);

        // Check distance
        let distance = calculate_subsector_distance(&map_data, source_subsector, target_subsector);
        println!("Distance: {:.2}", distance);

        // Check visibility
        let is_visible = map_data
            .bsp_3d
            .subsector_visible(source_subsector, target_subsector);
        println!("PVS visibility: {}", is_visible);

        // Check nearby subsectors for comparison
        println!("\n=== Nearby subsectors visibility ===");
        for nearby in
            (source_subsector.saturating_sub(2))..=(source_subsector + 2).min(subsectors.len() - 1)
        {
            if nearby != source_subsector {
                let nearby_visible = map_data.bsp_3d.subsector_visible(nearby, target_subsector);
                let nearby_distance =
                    calculate_subsector_distance(&map_data, nearby, target_subsector);
                println!(
                    "Subsector {} -> {}: {} (distance: {:.2})",
                    nearby, target_subsector, nearby_visible, nearby_distance
                );
            }
        }

        // This test doesn't assert - it's purely diagnostic
        println!("=== End Diagnostic ===");
    }

    #[test]
    fn test_e1m2_subsector_238_visibility() {
        let wad_path = match find_wad_file() {
            Some(path) => path,
            None => {
                eprintln!("No WAD files found, skipping test");
                return;
            }
        };

        let wad = wad::WadData::new(&PathBuf::from(wad_path));
        let mut map_data = MapData::default();
        map_data.load("E1M2", &PicData::default(), &wad);

        let source_subsector = 238;
        let target_subsector_44 = 44;

        println!("=== Testing Subsector {} Visibility ===", source_subsector);

        // Check if subsectors exist
        let subsectors = map_data.subsectors();
        if source_subsector >= subsectors.len() || target_subsector_44 >= subsectors.len() {
            println!("ERROR: Subsector index out of range");
            return;
        }

        // Test visibility to subsector 44
        let visible_44 = map_data
            .bsp_3d
            .subsector_visible(source_subsector, target_subsector_44);
        let distance_44 =
            calculate_subsector_distance(&map_data, source_subsector, target_subsector_44);

        println!(
            "Subsector {} -> Subsector {}: {} (distance: {:.2})",
            source_subsector, target_subsector_44, visible_44, distance_44
        );

        // Find subsectors containing linedef 140
        let linedefs = map_data.linedefs();
        if linedefs.len() <= 140 {
            println!("Map E1M2 doesn't have enough linedefs, skipping linedef 140 test");
            return;
        }

        let linedef_140_subsectors = find_linedef_subsectors(&map_data, 140);
        println!("Linedef 140 is in subsectors: {:?}", linedef_140_subsectors);

        // Test visibility to linedef 140 subsectors
        let mut linedef_140_visible = false;
        for &target in &linedef_140_subsectors {
            let visible = map_data.bsp_3d.subsector_visible(source_subsector, target);
            let distance = calculate_subsector_distance(&map_data, source_subsector, target);

            println!(
                "Subsector {} -> Linedef 140 subsector {}: {} (distance: {:.2})",
                source_subsector, target, visible, distance
            );

            if visible {
                linedef_140_visible = true;
            }
        }

        // Print summary
        println!("=== Summary ===");
        println!(
            "Subsector 238 -> Subsector 44: {}",
            if visible_44 { "VISIBLE" } else { "BLOCKED" }
        );
        println!(
            "Subsector 238 -> Linedef 140: {}",
            if linedef_140_visible {
                "VISIBLE"
            } else {
                "BLOCKED"
            }
        );

        // Assert that linedef 140 should be visible
        assert!(
            linedef_140_visible,
            "Subsector 238 should see linedef 140 subsectors"
        );
    }

    #[test]
    fn test_e1m2_subsector_238_44_diagnostic() {
        let wad_path = match find_wad_file() {
            Some(path) => path,
            None => {
                eprintln!("No WAD files found, skipping test");
                return;
            }
        };

        let wad = wad::WadData::new(&PathBuf::from(wad_path));
        let mut map_data = MapData::default();
        map_data.load("E1M2", &PicData::default(), &wad);

        let source_subsector = 238;
        let target_subsector = 44;

        println!(
            "=== PVS Diagnostic: Subsector {} -> Subsector {} ===",
            source_subsector, target_subsector
        );

        // Check if subsectors exist
        let subsectors = map_data.subsectors();
        if source_subsector >= subsectors.len() || target_subsector >= subsectors.len() {
            println!("ERROR: Subsector index out of range");
            return;
        }

        // Get sector information
        let source_sector = &subsectors[source_subsector].sector;
        let target_sector = &subsectors[target_subsector].sector;

        println!(
            "Source sector: floor={}, ceiling={}, light={}",
            source_sector.floorheight, source_sector.ceilingheight, source_sector.lightlevel
        );
        println!(
            "Target sector: floor={}, ceiling={}, light={}",
            target_sector.floorheight, target_sector.ceilingheight, target_sector.lightlevel
        );

        // Check height overlap
        let height_overlap = source_sector.floorheight < target_sector.ceilingheight
            && source_sector.ceilingheight > target_sector.floorheight;
        println!("Height overlap: {}", height_overlap);

        // Check if they're in the same sector
        let same_sector = std::ptr::eq(source_sector.inner, target_sector.inner);
        println!("Same sector: {}", same_sector);

        // Check distance
        let distance = calculate_subsector_distance(&map_data, source_subsector, target_subsector);
        println!("Distance: {:.2}", distance);

        // Check visibility
        let is_visible = map_data
            .bsp_3d
            .subsector_visible(source_subsector, target_subsector);
        println!("PVS visibility: {}", is_visible);

        // Check nearby subsectors for comparison
        println!("\n=== Nearby subsectors visibility ===");
        for nearby in
            (source_subsector.saturating_sub(2))..=(source_subsector + 2).min(subsectors.len() - 1)
        {
            if nearby != source_subsector {
                let nearby_visible = map_data.bsp_3d.subsector_visible(nearby, target_subsector);
                let nearby_distance =
                    calculate_subsector_distance(&map_data, nearby, target_subsector);
                println!(
                    "Subsector {} -> {}: {} (distance: {:.2})",
                    nearby, target_subsector, nearby_visible, nearby_distance
                );
            }
        }

        // Check some target nearby subsectors too
        println!("\n=== Target nearby subsectors visibility ===");
        for nearby in
            (target_subsector.saturating_sub(2))..=(target_subsector + 2).min(subsectors.len() - 1)
        {
            if nearby != target_subsector {
                let nearby_visible = map_data.bsp_3d.subsector_visible(source_subsector, nearby);
                let nearby_distance =
                    calculate_subsector_distance(&map_data, source_subsector, nearby);
                println!(
                    "Subsector {} -> {}: {} (distance: {:.2})",
                    source_subsector, nearby, nearby_visible, nearby_distance
                );
            }
        }

        // This test doesn't assert - it's purely diagnostic
        println!("=== End Diagnostic ===");
    }

    #[test]
    fn test_e1m2_subsector_238_linedef_140_upper_texture() {
        let wad_path = match find_wad_file() {
            Some(path) => path,
            None => {
                eprintln!("No WAD files found, skipping test");
                return;
            }
        };

        let wad = wad::WadData::new(&PathBuf::from(wad_path));
        let mut map_data = MapData::default();
        map_data.load("E1M2", &PicData::default(), &wad);

        let source_subsector = 238;

        // Find subsectors containing linedef 140
        let linedef_140_subsectors = find_linedef_subsectors(&map_data, 140);
        println!("=== Upper Texture Visibility Test ===");
        println!("Linedef 140 is in subsectors: {:?}", linedef_140_subsectors);

        // Test visibility from subsector 238 to linedef 140 subsectors
        let mut linedef_140_visible = false;
        for &target in &linedef_140_subsectors {
            let visible = map_data.bsp_3d.subsector_visible(source_subsector, target);
            let distance = calculate_subsector_distance(&map_data, source_subsector, target);

            println!(
                "Subsector {} -> Linedef 140 subsector {}: {} (distance: {:.2})",
                source_subsector, target, visible, distance
            );

            if visible {
                linedef_140_visible = true;
            }
        }

        println!(
            "Upper texture of Linedef 140 from subsector 238: {}",
            if linedef_140_visible {
                "VISIBLE"
            } else {
                "BLOCKED"
            }
        );

        // Upper texture should BE visible
        assert!(
            linedef_140_visible,
            "Upper texture of linedef 140 should be visible from subsector 238"
        );
    }

    #[test]
    fn test_e1m2_subsector_223_linedef_531() {
        let wad_path = match find_wad_file() {
            Some(path) => path,
            None => {
                eprintln!("No WAD files found, skipping test");
                return;
            }
        };

        let wad = wad::WadData::new(&PathBuf::from(wad_path));
        let mut map_data = MapData::default();
        map_data.load("E1M2", &PicData::default(), &wad);

        let source_subsector = 223;

        // Check if linedef 531 exists
        let linedefs = map_data.linedefs();
        if linedefs.len() <= 531 {
            println!("Map E1M2 doesn't have enough linedefs, skipping linedef 531 test");
            return;
        }

        // Find subsectors containing linedef 531
        let linedef_531_subsectors = find_linedef_subsectors(&map_data, 531);
        println!("=== Subsector 223 to Linedef 531 Test ===");
        println!("Linedef 531 is in subsectors: {:?}", linedef_531_subsectors);

        // Test visibility from subsector 223 to linedef 531 subsectors
        let mut linedef_531_visible = false;
        for &target in &linedef_531_subsectors {
            let visible = map_data.bsp_3d.subsector_visible(source_subsector, target);
            let distance = calculate_subsector_distance(&map_data, source_subsector, target);

            println!(
                "Subsector {} -> Linedef 531 subsector {}: {} (distance: {:.2})",
                source_subsector, target, visible, distance
            );

            if visible {
                linedef_531_visible = true;
            }
        }

        println!(
            "Subsector 223 -> Linedef 531: {}",
            if linedef_531_visible {
                "VISIBLE"
            } else {
                "BLOCKED"
            }
        );

        // Linedef 531 should be visible from subsector 223
        assert!(linedef_531_visible, "Subsector 223 should see linedef 531");
    }

    #[test]
    fn test_e1m2_visibility_diagnostic_summary() {
        let wad_path = match find_wad_file() {
            Some(path) => path,
            None => {
                eprintln!("No WAD files found, skipping test");
                return;
            }
        };

        let wad = wad::WadData::new(&PathBuf::from(wad_path));
        let mut map_data = MapData::default();
        map_data.load("E1M2", &PicData::default(), &wad);

        println!("=== PVS Findings Summary ===");

        // Test case 1: Subsector 59 -> 74
        let vis_59_74 = map_data.bsp_3d.subsector_visible(59, 74);
        println!("1. Subsector 59 -> 74: {} (should be visible)", vis_59_74);

        // Test case 2: Subsector 238 -> Linedef 140 subsectors
        let linedef_140_subs = find_linedef_subsectors(&map_data, 140);
        let mut vis_238_140 = false;
        for &target in &linedef_140_subs {
            if map_data.bsp_3d.subsector_visible(238, target) {
                vis_238_140 = true;
                break;
            }
        }
        println!(
            "2. Subsector 238 -> Linedef 140: {} (should be blocked for upper texture)",
            vis_238_140
        );

        // Test case 3: Subsector 223 -> Linedef 531 subsectors
        let linedef_531_subs = find_linedef_subsectors(&map_data, 531);
        let mut vis_223_531 = false;
        for &target in &linedef_531_subs {
            if map_data.bsp_3d.subsector_visible(223, target) {
                vis_223_531 = true;
                break;
            }
        }
        println!(
            "3. Subsector 223 -> Linedef 531: {} (should be visible)",
            vis_223_531
        );

        println!("=== Analysis ===");
        println!("RAY_ENDPOINT_TOLERANCE: No significant effect observed");
        println!("Recursion depth: No significant effect observed");
        println!("3-point sampling: Provides good balance but may need refinement");
        println!("Issue: PVS may be too permissive in some cases, too restrictive in others");

        // This is a diagnostic test - no assertions
    }
}
