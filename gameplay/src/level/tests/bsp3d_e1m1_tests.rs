#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::{MovementType, SurfaceKind, WallType};

    #[test]
    fn test_door_vertex_sharing() {
        use crate::{MapData, PicData};
        use std::collections::HashMap;
        use wad::WadData;

        let wad = WadData::new(&PathBuf::from("/Users/lukejones/DOOM/doom.wad"));
        let mut map = MapData::default();
        map.load("E1M1", &&PicData::init(false, &wad), &wad);

        let bsp3d = &mut map.bsp_3d;

        println!("=== DOOR VERTEX SHARING TEST - SECTORS 25 & 26 ===");

        // Find subsectors for sectors 25 and 26
        let sector_25_subsectors = bsp3d.sector_subsectors.get(25).cloned().unwrap_or_default();
        let sector_26_subsectors = bsp3d.sector_subsectors.get(26).cloned().unwrap_or_default();

        println!("Sector 25 subsectors: {:?}", sector_25_subsectors);
        println!("Sector 26 subsectors: {:?}", sector_26_subsectors);

        // Find linedef 148, 150, 151 segments
        let segments = &map.segments;
        let linedefs = &map.linedefs;

        let mut tracked_linedefs = HashMap::new();
        for (seg_idx, segment) in segments.iter().enumerate() {
            let linedef = &*segment.linedef;
            let linedef_id = linedefs
                .iter()
                .position(|ld| std::ptr::eq(ld as *const _, linedef as *const _));

            if let Some(ld_id) = linedef_id {
                if [148, 150, 151, 152, 153].contains(&ld_id) {
                    tracked_linedefs
                        .entry(ld_id)
                        .or_insert_with(Vec::new)
                        .push((seg_idx, segment));
                    println!(
                        "Found linedef {} segment {}: front sector {}, back sector {:?}",
                        ld_id,
                        seg_idx,
                        segment.frontsector.num,
                        segment.backsector.as_ref().map(|s| s.num)
                    );
                }
            }
        }

        // Record initial vertex positions
        println!("\n=== RECORDING INITIAL POSITIONS ===");

        let mut initial_vertex_positions = HashMap::new();
        for vertex_idx in 0..bsp3d.vertices.len() {
            initial_vertex_positions.insert(vertex_idx, bsp3d.vertices[vertex_idx]);
        }

        // Record floor polygon positions for sectors 25 and 26
        let mut floor_polygon_vertices = HashMap::new();

        for &subsector_id in &sector_25_subsectors {
            if let Some(leaf) = bsp3d.get_subsector_leaf(subsector_id) {
                println!("\nSubsector {} (Sector 25) polygons:", subsector_id);
                for &floor_poly_idx in &leaf.floor_polygons {
                    if let Some(polygon) = leaf.polygons.get(floor_poly_idx) {
                        println!(
                            "  Floor polygon {} vertices: {:?}",
                            floor_poly_idx, polygon.vertices
                        );
                        floor_polygon_vertices
                            .insert((subsector_id, floor_poly_idx), polygon.vertices.clone());
                        for &vertex_idx in &polygon.vertices {
                            println!(
                                "    Vertex {}: {:?}",
                                vertex_idx, bsp3d.vertices[vertex_idx]
                            );
                        }
                    }
                }
            }
        }

        for &subsector_id in &sector_26_subsectors {
            if let Some(leaf) = bsp3d.get_subsector_leaf(subsector_id) {
                println!("\nSubsector {} (Sector 26) polygons:", subsector_id);
                for &floor_poly_idx in &leaf.floor_polygons {
                    if let Some(polygon) = leaf.polygons.get(floor_poly_idx) {
                        println!(
                            "  Floor polygon {} vertices: {:?}",
                            floor_poly_idx, polygon.vertices
                        );
                        floor_polygon_vertices
                            .insert((subsector_id, floor_poly_idx), polygon.vertices.clone());
                        for &vertex_idx in &polygon.vertices {
                            println!(
                                "    Vertex {}: {:?}",
                                vertex_idx, bsp3d.vertices[vertex_idx]
                            );
                        }
                    }
                }
            }
        }

        // Record wall positions from linedefs 148 (should shrink), 150, 151 (should be stationary) and 152, 153 (should move with ceiling)
        let mut wall_polygon_vertices = HashMap::new();
        for (&linedef_id, segments) in &tracked_linedefs {
            if [148, 150, 151, 152, 153].contains(&linedef_id) {
                for &(seg_idx, segment) in segments {
                    // Find subsector containing this segment
                    let subsectors = &map.subsectors;
                    for (subsector_id, subsector) in subsectors.iter().enumerate() {
                        let start_seg = subsector.start_seg as usize;
                        let end_seg = start_seg + subsector.seg_count as usize;

                        if seg_idx >= start_seg && seg_idx < end_seg {
                            if let Some(leaf) = bsp3d.get_subsector_leaf(subsector_id) {
                                let behavior = if linedef_id == 148 {
                                    "shrink"
                                } else if [152, 153].contains(&linedef_id) {
                                    "move"
                                } else {
                                    "NOT move"
                                };
                                println!(
                                    "\nLinedef {} walls in subsector {} (should {}):",
                                    linedef_id, subsector_id, behavior
                                );
                                for (poly_idx, polygon) in leaf.polygons.iter().enumerate() {
                                    if let SurfaceKind::Vertical { .. } = &polygon.surface_kind {
                                        println!(
                                            "  Wall polygon {} vertices: {:?}",
                                            poly_idx, polygon.vertices
                                        );
                                        wall_polygon_vertices.insert(
                                            (linedef_id, subsector_id, poly_idx),
                                            polygon.vertices.clone(),
                                        );
                                        for &vertex_idx in &polygon.vertices {
                                            println!(
                                                "    Vertex {}: {:?}",
                                                vertex_idx, bsp3d.vertices[vertex_idx]
                                            );
                                        }
                                    }
                                }
                            }
                            break;
                        }
                    }
                }
            }
        }

        // Debug: Check marked vertices before movement
        println!("\n=== MARKED VERTICES BEFORE MOVEMENT ===");
        for &subsector_id in &[55, 56] {
            println!("Subsector {} marked floor vertices:", subsector_id);
            // for (key, vertex_idx) in &bsp3d.marked_floor_vertices[subsector_id] {
            //     println!(
            //         "  Key {:?} -> Vertex {}: {:?}",
            //         key, vertex_idx, bsp3d.vertices[*vertex_idx]
            //     );
            // }
            println!("Subsector {} marked ceiling vertices:", subsector_id);
            // for (key, vertex_idx) in &bsp3d.marked_ceiling_vertices[subsector_id] {
            //     println!(
            //         "  Key {:?} -> Vertex {}: {:?}",
            //         key, vertex_idx, bsp3d.vertices[*vertex_idx]
            //     );
            // }
        }

        println!("\n=== MOVING SECTOR 26 CEILING FROM 0 TO 68 ===");
        bsp3d.move_vertices(26, MovementType::Ceiling, 68.0);

        println!("\n=== COMPREHENSIVE MOVEMENT ANALYSIS ===");

        // Find ALL vertices that moved with sector 26 ceiling
        let mut moved_vertices = Vec::new();
        for vertex_idx in 0..bsp3d.vertices.len() {
            let original_pos = initial_vertex_positions[&vertex_idx];
            let current_pos = bsp3d.vertices[vertex_idx];
            if (original_pos - current_pos).length() > 0.001 {
                moved_vertices.push((vertex_idx, original_pos, current_pos));
            }
        }

        println!("Total vertices that moved: {}", moved_vertices.len());
        for (vertex_idx, orig, curr) in &moved_vertices {
            println!("  Vertex {}: {:?} -> {:?}", vertex_idx, orig, curr);
        }

        // Categorize which polygons contain moved vertices
        println!("\n=== POLYGON ANALYSIS FOR MOVED VERTICES ===");

        // Check sector 25 floor polygons
        for &subsector_id in &sector_25_subsectors {
            if let Some(leaf) = bsp3d.get_subsector_leaf(subsector_id) {
                for &floor_poly_idx in &leaf.floor_polygons {
                    if let Some(polygon) = leaf.polygons.get(floor_poly_idx) {
                        let mut has_moved_vertex = false;
                        for &vertex_idx in &polygon.vertices {
                            if moved_vertices.iter().any(|(idx, _, _)| *idx == vertex_idx) {
                                has_moved_vertex = true;
                                break;
                            }
                        }
                        if has_moved_vertex {
                            println!(
                                "❌ SECTOR 25 FLOOR POLYGON {} CONTAINS MOVED VERTICES:",
                                floor_poly_idx
                            );
                            for &vertex_idx in &polygon.vertices {
                                if let Some((_, orig, curr)) =
                                    moved_vertices.iter().find(|(idx, _, _)| *idx == vertex_idx)
                                {
                                    println!("    Vertex {}: {:?} -> {:?}", vertex_idx, orig, curr);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Check sector 26 floor polygons
        for &subsector_id in &sector_26_subsectors {
            if let Some(leaf) = bsp3d.get_subsector_leaf(subsector_id) {
                for &floor_poly_idx in &leaf.floor_polygons {
                    if let Some(polygon) = leaf.polygons.get(floor_poly_idx) {
                        let mut has_moved_vertex = false;
                        for &vertex_idx in &polygon.vertices {
                            if moved_vertices.iter().any(|(idx, _, _)| *idx == vertex_idx) {
                                has_moved_vertex = true;
                                break;
                            }
                        }
                        if has_moved_vertex {
                            println!(
                                "❌ SECTOR 26 FLOOR POLYGON {} CONTAINS MOVED VERTICES:",
                                floor_poly_idx
                            );
                            for &vertex_idx in &polygon.vertices {
                                if let Some((_, orig, curr)) =
                                    moved_vertices.iter().find(|(idx, _, _)| *idx == vertex_idx)
                                {
                                    println!("    Vertex {}: {:?} -> {:?}", vertex_idx, orig, curr);
                                }
                            }
                        }
                    }
                }
            }
        }

        println!("\n=== VALIDATING AFTER MOVEMENT ===");

        // Check that floor vertices did NOT move
        println!("\nChecking floor stability:");
        for ((subsector_id, floor_poly_idx), original_vertices) in &floor_polygon_vertices {
            if let Some(leaf) = bsp3d.get_subsector_leaf(*subsector_id) {
                if let Some(polygon) = leaf.polygons.get(*floor_poly_idx) {
                    let mut floor_moved = false;
                    for &vertex_idx in &polygon.vertices {
                        let original_pos = initial_vertex_positions[&vertex_idx];
                        let current_pos = bsp3d.vertices[vertex_idx];
                        if (original_pos.z - current_pos.z).abs() > 0.001 {
                            println!(
                                "  ❌ FLOOR MOVED: Subsector {} floor polygon {} vertex {} moved from {:?} to {:?}",
                                subsector_id, floor_poly_idx, vertex_idx, original_pos, current_pos
                            );
                            floor_moved = true;
                        }
                    }
                    if !floor_moved {
                        println!(
                            "  ✅ Floor polygon {} in subsector {} remained stationary",
                            floor_poly_idx, subsector_id
                        );
                    }
                }
            }
        }

        // Check wall movement: 148 should shrink, 150,151 should stay, 152,153 should move
        println!(
            "\nChecking wall movement (148 should shrink, 150,151 should stay, 152,153 should move):"
        );
        for ((linedef_id, subsector_id, poly_idx), original_vertices) in &wall_polygon_vertices {
            if let Some(leaf) = bsp3d.get_subsector_leaf(*subsector_id) {
                if let Some(polygon) = leaf.polygons.get(*poly_idx) {
                    let should_move = [148, 152, 153].contains(linedef_id);
                    let mut wall_moved = false;
                    let mut moved_vertices = Vec::new();

                    for &vertex_idx in &polygon.vertices {
                        let original_pos = initial_vertex_positions[&vertex_idx];
                        let current_pos = bsp3d.vertices[vertex_idx];
                        if (original_pos - current_pos).length() > 0.001 {
                            wall_moved = true;
                            moved_vertices.push((vertex_idx, original_pos, current_pos));
                        }
                    }

                    if should_move && wall_moved {
                        let action = if *linedef_id == 148 {
                            "shrunk"
                        } else {
                            "moved"
                        };
                        println!(
                            "  ✅ Linedef {} wall polygon {} {} correctly",
                            linedef_id, poly_idx, action
                        );
                        for (vertex_idx, orig, curr) in moved_vertices {
                            println!(
                                "    Vertex {} moved from {:?} to {:?}",
                                vertex_idx, orig, curr
                            );
                        }
                    } else if should_move && !wall_moved {
                        let action = if *linedef_id == 148 {
                            "shrunk"
                        } else {
                            "moved"
                        };
                        println!(
                            "  ❌ WALL SHOULD MOVE: Linedef {} wall polygon {} should have {} but didn't",
                            linedef_id, poly_idx, action
                        );
                    } else if !should_move && wall_moved {
                        println!(
                            "  ❌ WALL MOVED: Linedef {} wall polygon {} should be stationary but moved",
                            linedef_id, poly_idx
                        );
                        for (vertex_idx, orig, curr) in moved_vertices {
                            println!(
                                "    Vertex {} moved from {:?} to {:?}",
                                vertex_idx, orig, curr
                            );
                        }
                    } else {
                        println!(
                            "  ✅ Linedef {} wall polygon {} remained stationary (correct)",
                            linedef_id, poly_idx
                        );
                    }
                }
            }
        }

        // Check that sector 26 ceiling vertices DID move
        println!("\nChecking sector 26 ceiling movement:");
        for &subsector_id in &sector_26_subsectors {
            if let Some(leaf) = bsp3d.get_subsector_leaf(subsector_id) {
                for &ceiling_poly_idx in &leaf.ceiling_polygons {
                    if let Some(polygon) = leaf.polygons.get(ceiling_poly_idx) {
                        for &vertex_idx in &polygon.vertices {
                            let original_pos = initial_vertex_positions[&vertex_idx];
                            let current_pos = bsp3d.vertices[vertex_idx];
                            if (current_pos.z - 68.0).abs() < 0.001
                                && (original_pos.z - 0.0).abs() < 0.001
                            {
                                println!(
                                    "  ✅ Sector 26 ceiling vertex {} moved from {} to {} (expected)",
                                    vertex_idx, original_pos.z, current_pos.z
                                );
                            } else {
                                println!(
                                    "  ❌ Sector 26 ceiling vertex {} unexpected position: {} -> {} (expected 68)",
                                    vertex_idx, original_pos.z, current_pos.z
                                );
                            }
                        }
                    }
                }
            }
        }

        println!("\n=== TEST COMPLETE ===");
    }

    #[test]
    fn test_wall_marking() {
        use crate::{MapData, PicData};
        use wad::WadData;

        let wad = WadData::new(&PathBuf::from("/Users/lukejones/DOOM/doom.wad"));
        let mut map = MapData::default();
        map.load("E1M1", &&PicData::init(false, &wad), &wad);

        // Find linedef 484
        let _linedef_484 = &map.linedefs[484];

        // Find segment that references linedef 484
        let mut segment_484 = None;
        for segment in &map.segments {
            if segment.linedef.num == 484 {
                segment_484 = Some(segment);
                break;
            }
        }

        assert!(
            segment_484.is_some(),
            "Should find segment pointing to linedef 484"
        );
        let segment = segment_484.unwrap();

        // Verify front sector is 14, back sector is 23
        assert_eq!(segment.frontsector.num, 14, "Front sector should be 14");
        if let Some(back_sector) = &segment.backsector {
            assert_eq!(back_sector.num, 23, "Back sector should be 23");
        } else {
            panic!("Linedef 484 should have a back sector");
        }

        // Verify both sectors have floor height 32 (same height)
        assert_eq!(
            segment.frontsector.floorheight, 32.0,
            "Front floor should be 32"
        );
        if let Some(back_sector) = &segment.backsector {
            assert_eq!(back_sector.floorheight, 32.0, "Back floor should be 32");
        }

        // Verify linedef has bottom texture
        assert!(
            segment.sidedef.bottomtexture.is_some(),
            "Should have bottom texture"
        );

        // Find back subsector for sector 23
        let mut back_subsector_id = None;
        for (subsector_id, subsector_ids) in map.bsp_3d.sector_subsectors.iter().enumerate() {
            if subsector_id == 23 && !subsector_ids.is_empty() {
                back_subsector_id = Some(subsector_ids[0]);
                break;
            }
        }

        assert!(
            back_subsector_id.is_some(),
            "Should find back subsector for sector 23"
        );
    }
}
