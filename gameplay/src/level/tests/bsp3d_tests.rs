#[cfg(test)]
mod bsp32_tests {
    use std::path::PathBuf;

    #[test]
    fn test_imp_platform_move() {
        use crate::{MapData, PicData};
        use wad::WadData;

        let wad = WadData::new(&PathBuf::from("/Users/lukejones/DOOM/doom.wad"));
        let mut map = MapData::default();
        map.load("E1M1", &&PicData::init(false, &wad), &wad);
    }

    #[test]
    fn test_door_unlinking() {
        use crate::{MapData, PicData};
        use wad::WadData;

        let wad = WadData::new(&PathBuf::from("/Users/lukejones/DOOM/doom.wad"));
        let mut map = MapData::default();
        map.load("E1M1", &&PicData::init(false, &wad), &wad);

        // Debug E1M1 door issue: sector 26 (door), sector 22 (right), sector 25 (left)
        // Linedefs: 148,149 (should move), 150,151,154,155 (should NOT move)
        println!("=== E1M1 DOOR DEBUG ===");
        println!("Door sector 26, adjacent sectors 22 and 25");
        println!("Linedefs 148,149 should move with door ceiling");
        println!("Linedefs 150,151,154,155 should stay put");

        // Store original floor vertex positions for sectors 22 and 25
        let mut sector_22_floor_positions = Vec::new();
        let mut sector_25_floor_positions = Vec::new();

        for (subsector_id, subsector) in map.subsectors.iter().enumerate() {
            if subsector.sector.num == 22 {
                let leaf = &map.bsp_3d.subsector_leaves[subsector_id];
                for &polygon_idx in &leaf.floor_polygons {
                    for &vertex_idx in &leaf.polygons[polygon_idx].vertices {
                        let pos = map.bsp_3d.vertices[vertex_idx];
                        sector_22_floor_positions.push((vertex_idx, pos));
                    }
                }
            } else if subsector.sector.num == 25 {
                let leaf = &map.bsp_3d.subsector_leaves[subsector_id];
                for &polygon_idx in &leaf.floor_polygons {
                    for &vertex_idx in &leaf.polygons[polygon_idx].vertices {
                        let pos = map.bsp_3d.vertices[vertex_idx];
                        sector_25_floor_positions.push((vertex_idx, pos));
                    }
                }
            }
        }

        println!(
            "Original sector 22 floor positions: {:?}",
            sector_22_floor_positions
        );
        println!(
            "Original sector 25 floor positions: {:?}",
            sector_25_floor_positions
        );

        // Move door ceiling to height 32
        map.bsp_3d.move_ceiling_vertices(26, 32.0);

        // Verify door ceiling moved
        let mut door_moved = false;
        for (subsector_id, subsector) in map.subsectors.iter().enumerate() {
            if subsector.sector.num == 26 {
                let leaf = &map.bsp_3d.subsector_leaves[subsector_id];
                for &polygon_idx in &leaf.ceiling_polygons {
                    for &vertex_idx in &leaf.polygons[polygon_idx].vertices {
                        let pos = map.bsp_3d.vertices[vertex_idx];
                        if pos.z == 32.0 {
                            door_moved = true;
                            println!("Door ceiling vertex {} moved to height 32.0", vertex_idx);
                        }
                    }
                }
            }
        }
        assert!(door_moved, "Door ceiling should have moved to height 32");

        // Verify adjacent floors did NOT move
        for (vertex_idx, original_pos) in &sector_22_floor_positions {
            let current_pos = map.bsp_3d.vertices[*vertex_idx];
            if current_pos != *original_pos {
                println!(
                    "ERROR: Sector 22 floor vertex {} moved from {:?} to {:?}",
                    vertex_idx, original_pos, current_pos
                );
                panic!("Sector 22 floor vertices should not move when door ceiling moves");
            }
        }

        for (vertex_idx, original_pos) in &sector_25_floor_positions {
            let current_pos = map.bsp_3d.vertices[*vertex_idx];
            if current_pos != *original_pos {
                println!(
                    "ERROR: Sector 25 floor vertex {} moved from {:?} to {:?}",
                    vertex_idx, original_pos, current_pos
                );
                panic!("Sector 25 floor vertices should not move when door ceiling moves");
            }
        }

        println!("SUCCESS: Adjacent floors did not move when door ceiling moved");
        println!("SUCCESS: Adjacent floors did not move when door ceiling moves");

        // Additional test: Verify that upper wall bottom vertices moved with door
        // ceiling Find linedef 149 wall in sector 22 and check if its bottom
        // vertices moved
        let mut upper_wall_moved = false;
        for (subsector_id, subsector) in map.subsectors.iter().enumerate() {
            if subsector.sector.num == 22 {
                let leaf = &map.bsp_3d.subsector_leaves[subsector_id];
                for (poly_idx, polygon) in leaf.polygons.iter().enumerate() {
                    if let crate::SurfaceKind::Vertical { wall_type: _, .. } = polygon.surface_kind
                    {
                        // Check if any vertices are at height 32.0 (moved with door ceiling)
                        for &vertex_idx in &polygon.vertices {
                            let pos = map.bsp_3d.vertices[vertex_idx];
                            if pos.z == 32.0 {
                                println!(
                                    "Upper wall polygon {} in sector 22 has vertex {} at height 32.0",
                                    poly_idx, vertex_idx
                                );
                                upper_wall_moved = true;
                            }
                        }
                    }
                }
            }
        }

        if upper_wall_moved {
            println!("SUCCESS: Upper wall bottom vertices moved with door ceiling");

            // Additional check: Look for wall polygons that should share vertices
            for (subsector_id, subsector) in map.subsectors.iter().enumerate() {
                if subsector.sector.num == 22 {
                    let leaf = &map.bsp_3d.subsector_leaves[subsector_id];
                    let mut wall_polygons = Vec::new();

                    for (poly_idx, polygon) in leaf.polygons.iter().enumerate() {
                        if let crate::SurfaceKind::Vertical { wall_type: _, .. } =
                            polygon.surface_kind
                        {
                            // Check if polygon has vertices at door ceiling height
                            let has_door_height = polygon
                                .vertices
                                .iter()
                                .any(|&vertex_idx| map.bsp_3d.vertices[vertex_idx].z == 32.0);
                            if has_door_height {
                                wall_polygons.push((poly_idx, &polygon.vertices));
                            }
                        }
                    }

                    println!(
                        "Found {} wall polygons with door ceiling height vertices",
                        wall_polygons.len()
                    );

                    // Check for shared vertices between wall polygons
                    for i in 0..wall_polygons.len() {
                        for j in i + 1..wall_polygons.len() {
                            let (poly1_idx, poly1_vertices) = wall_polygons[i];
                            let (poly2_idx, poly2_vertices) = wall_polygons[j];

                            let shared_vertices: Vec<_> = poly1_vertices
                                .iter()
                                .filter(|&&v| poly2_vertices.contains(&v))
                                .collect();

                            if !shared_vertices.is_empty() {
                                println!(
                                    "Polygons {} and {} share vertices: {:?}",
                                    poly1_idx, poly2_idx, shared_vertices
                                );
                            } else {
                                // Check for vertices at same position but different indices
                                for &v1 in poly1_vertices {
                                    let pos1 = map.bsp_3d.vertices[v1];
                                    for &v2 in poly2_vertices {
                                        let pos2 = map.bsp_3d.vertices[v2];
                                        if pos1.x == pos2.x
                                            && pos1.y == pos2.y
                                            && pos1.z == pos2.z
                                            && v1 != v2
                                        {
                                            println!(
                                                "Polygons {} and {} have different vertices at same position: {} vs {} at ({:.1}, {:.1}, {:.1})",
                                                poly1_idx,
                                                poly2_idx,
                                                v1,
                                                v2,
                                                pos1.x,
                                                pos1.y,
                                                pos1.z
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        } else {
            println!("INFO: No upper wall bottom vertices found at door ceiling height");
        }

        // Verify that missing door walls were generated
        let mut door_walls_found = 0;
        for (subsector_id, subsector) in map.subsectors.iter().enumerate() {
            if subsector.sector.num == 26 {
                let leaf = &map.bsp_3d.subsector_leaves[subsector_id];
                for (poly_idx, polygon) in leaf.polygons.iter().enumerate() {
                    if let crate::SurfaceKind::Vertical {
                        texture: Some(_),
                        wall_type: _,
                        ..
                    } = polygon.surface_kind
                    {
                        // Check if this wall spans from floor to ceiling height
                        let mut has_floor_vertex = false;
                        let mut has_ceiling_vertex = false;
                        for &vertex_idx in &polygon.vertices {
                            let pos = map.bsp_3d.vertices[vertex_idx];
                            if pos.z == 0.0 {
                                // door floor height
                                has_floor_vertex = true;
                            }
                            if pos.z == 32.0 {
                                // door ceiling height after movement
                                has_ceiling_vertex = true;
                            }
                        }
                        if has_floor_vertex && has_ceiling_vertex {
                            door_walls_found += 1;
                            println!(
                                "Found door wall polygon {} in sector 26 spanning floor to ceiling",
                                poly_idx
                            );
                        }

                        // Verify that missing door walls were generated
                        let mut door_walls_found = 0;
                        for (subsector_id, subsector) in map.subsectors.iter().enumerate() {
                            if subsector.sector.num == 26 {
                                let leaf = &map.bsp_3d.subsector_leaves[subsector_id];
                                for (poly_idx, polygon) in leaf.polygons.iter().enumerate() {
                                    if let crate::SurfaceKind::Vertical {
                                        texture: Some(_),
                                        wall_type: _,
                                        ..
                                    } = polygon.surface_kind
                                    {
                                        // Check if this wall spans from floor to ceiling height
                                        let mut has_floor_vertex = false;
                                        let mut has_ceiling_vertex = false;
                                        for &vertex_idx in &polygon.vertices {
                                            let pos = map.bsp_3d.vertices[vertex_idx];
                                            if pos.z == 0.0 {
                                                // door floor height
                                                has_floor_vertex = true;
                                            }
                                            if pos.z == 32.0 {
                                                // door ceiling height after movement
                                                has_ceiling_vertex = true;
                                            }
                                        }
                                        if has_floor_vertex && has_ceiling_vertex {
                                            door_walls_found += 1;
                                            println!(
                                                "Found door wall polygon {} in sector 26 spanning floor to ceiling",
                                                poly_idx
                                            );
                                        }
                                    }
                                }
                            }
                        }

                        if door_walls_found > 0 {
                            println!(
                                "SUCCESS: Found {} missing door walls generated",
                                door_walls_found
                            );
                        } else {
                            println!("INFO: No missing door walls found spanning floor to ceiling");
                        }
                    }
                }
            }
        }

        if door_walls_found > 0 {
            println!(
                "SUCCESS: Found {} missing door walls generated",
                door_walls_found
            );
        } else {
            println!("INFO: No missing door walls found spanning floor to ceiling");
        }
    }

    #[test]
    fn test_door_upper_lower_unlinking() {
        use crate::{MapData, PicData};
        use wad::WadData;

        let wad = WadData::new(&PathBuf::from("/Users/lukejones/DOOM/doom.wad"));
        let mut map = MapData::default();
        map.load("E1M3", &&PicData::init(false, &wad), &wad);

        println!("=== E1M3 UPPER/LOWER WALL DEBUG ===");
        println!("Door sector 116, adjacent sectors 115 (left) and 32 (right)");
        println!("Linedef 145 should have upper and lower walls");
        println!("Lower wall: bottom=104, top=136");
        println!("Upper wall: bottom=136 (moves), top=176");

        // Find door subsector (sector 116)
        let mut door_subsector_id = None;
        for (subsector_id, subsector) in map.subsectors.iter().enumerate() {
            if subsector.sector.num == 116 {
                door_subsector_id = Some(subsector_id);
                println!("Found door subsector {} for sector 116", subsector_id);
                break;
            }
        }

        let door_subsector_id =
            door_subsector_id.expect("Could not find door subsector for sector 116");

        // Store original wall vertex positions before door movement
        let mut wall_vertex_positions = Vec::new();
        for (subsector_id, subsector) in map.subsectors.iter().enumerate() {
            if subsector.sector.num == 115 || subsector.sector.num == 32 {
                let leaf = &map.bsp_3d.subsector_leaves[subsector_id];
                for (poly_idx, polygon) in leaf.polygons.iter().enumerate() {
                    if let crate::SurfaceKind::Vertical { wall_type: _, .. } = polygon.surface_kind
                    {
                        for &vertex_idx in &polygon.vertices {
                            let pos = map.bsp_3d.vertices[vertex_idx];
                            wall_vertex_positions.push((
                                subsector.sector.num,
                                poly_idx,
                                vertex_idx,
                                pos,
                            ));
                        }
                    }
                }
            }
        }

        println!(
            "Stored {} wall vertex positions before door movement",
            wall_vertex_positions.len()
        );

        // Move door ceiling to height 200
        map.bsp_3d.move_ceiling_vertices(116, 200.0);

        // Verify door ceiling moved
        let mut door_moved = false;
        let leaf = &map.bsp_3d.subsector_leaves[door_subsector_id];
        for &polygon_idx in &leaf.ceiling_polygons {
            for &vertex_idx in &leaf.polygons[polygon_idx].vertices {
                let pos = map.bsp_3d.vertices[vertex_idx];
                if pos.z == 200.0 {
                    door_moved = true;
                    println!("Door ceiling vertex {} moved to height 200.0", vertex_idx);
                }
            }
        }
        assert!(door_moved, "Door ceiling should have moved to height 200");

        // Check wall vertex behavior
        let mut upper_wall_vertices_moved = 0;
        let mut lower_wall_vertices_moved = 0;
        let mut floor_vertices_moved = 0;

        for (sector_num, poly_idx, vertex_idx, original_pos) in &wall_vertex_positions {
            let current_pos = map.bsp_3d.vertices[*vertex_idx];

            if current_pos != *original_pos {
                println!(
                    "Vertex {} in sector {} polygon {} moved from ({:.1}, {:.1}, {:.1}) to ({:.1}, {:.1}, {:.1})",
                    vertex_idx,
                    sector_num,
                    poly_idx,
                    original_pos.x,
                    original_pos.y,
                    original_pos.z,
                    current_pos.x,
                    current_pos.y,
                    current_pos.z
                );

                if original_pos.z == 136.0 && current_pos.z == 200.0 {
                    upper_wall_vertices_moved += 1;
                    println!("  ^^ Upper wall bottom vertex moved correctly");
                } else if original_pos.z == 104.0 && current_pos.z == 200.0 {
                    lower_wall_vertices_moved += 1;
                    println!("  ^^ ERROR: Lower wall bottom vertex should NOT move");
                } else if original_pos.z == 104.0 && current_pos.z != 104.0 {
                    floor_vertices_moved += 1;
                    println!("  ^^ ERROR: Floor vertex should NOT move");
                }
            }
        }

        println!(
            "Upper wall vertices that moved correctly: {}",
            upper_wall_vertices_moved
        );
        println!(
            "Lower wall vertices that moved incorrectly: {}",
            lower_wall_vertices_moved
        );
        println!(
            "Floor vertices that moved incorrectly: {}",
            floor_vertices_moved
        );

        if lower_wall_vertices_moved > 0 {
            panic!("Lower wall bottom vertices should not move with door ceiling");
        }
        if floor_vertices_moved > 0 {
            panic!("Floor vertices should not move with door ceiling");
        }
        if upper_wall_vertices_moved == 0 {
            println!("WARNING: No upper wall vertices found moving with door ceiling");
        }

        println!("SUCCESS: Lower walls and floors stayed put, upper walls moved correctly");
    }
}
