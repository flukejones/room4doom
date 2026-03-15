#[cfg(test)]
mod tests {
    use crate::{MovementType, SurfaceKind};

    use super::super::{DOOM_WAD, load_map};

    #[test]
    fn test_door_vertex_sharing() {
        use std::collections::HashMap;

        let mut map = load_map(DOOM_WAD, "E1M1");

        let bsp3d = &mut map.bsp_3d;

        println!("=== DOOR VERTEX SHARING TEST - SECTORS 25 & 26 ===");

        let sector_25_subsectors = bsp3d.sector_subsectors.get(25).cloned().unwrap_or_default();
        let sector_26_subsectors = bsp3d.sector_subsectors.get(26).cloned().unwrap_or_default();

        println!("Sector 25 subsectors: {:?}", sector_25_subsectors);
        println!("Sector 26 subsectors: {:?}", sector_26_subsectors);

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

        println!("\n=== RECORDING INITIAL POSITIONS ===");

        let mut initial_vertex_positions = HashMap::new();
        for vertex_idx in 0..bsp3d.vertices.len() {
            initial_vertex_positions.insert(vertex_idx, bsp3d.vertices[vertex_idx]);
        }

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

        let mut wall_polygon_vertices = HashMap::new();
        for (&linedef_id, segments) in &tracked_linedefs {
            if [148, 150, 151, 152, 153].contains(&linedef_id) {
                for &(seg_idx, _) in segments {
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
                                    if let SurfaceKind::Vertical {
                                        ..
                                    } = &polygon.surface_kind
                                    {
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

        println!("\n=== MOVING SECTOR 26 CEILING FROM 0 TO 68 ===");
        bsp3d.move_surface(26, MovementType::Ceiling, 68.0, 0);

        println!("\n=== COMPREHENSIVE MOVEMENT ANALYSIS ===");

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

        println!("\n=== POLYGON ANALYSIS FOR MOVED VERTICES ===");

        for &subsector_id in &sector_25_subsectors {
            if let Some(leaf) = bsp3d.get_subsector_leaf(subsector_id) {
                for &floor_poly_idx in &leaf.floor_polygons {
                    if let Some(polygon) = leaf.polygons.get(floor_poly_idx) {
                        let mut has_moved_vertex = false;
                        for &vertex_idx in &polygon.vertices {
                            if moved_vertices.iter().any(|(idx, ..)| *idx == vertex_idx) {
                                has_moved_vertex = true;
                                break;
                            }
                        }
                        if has_moved_vertex {
                            println!(
                                "SECTOR 25 FLOOR POLYGON {} CONTAINS MOVED VERTICES:",
                                floor_poly_idx
                            );
                            for &vertex_idx in &polygon.vertices {
                                if let Some((_, orig, curr)) =
                                    moved_vertices.iter().find(|(idx, ..)| *idx == vertex_idx)
                                {
                                    println!("    Vertex {}: {:?} -> {:?}", vertex_idx, orig, curr);
                                }
                            }
                        }
                    }
                }
            }
        }

        for &subsector_id in &sector_26_subsectors {
            if let Some(leaf) = bsp3d.get_subsector_leaf(subsector_id) {
                for &floor_poly_idx in &leaf.floor_polygons {
                    if let Some(polygon) = leaf.polygons.get(floor_poly_idx) {
                        let mut has_moved_vertex = false;
                        for &vertex_idx in &polygon.vertices {
                            if moved_vertices.iter().any(|(idx, ..)| *idx == vertex_idx) {
                                has_moved_vertex = true;
                                break;
                            }
                        }
                        if has_moved_vertex {
                            println!(
                                "SECTOR 26 FLOOR POLYGON {} CONTAINS MOVED VERTICES:",
                                floor_poly_idx
                            );
                            for &vertex_idx in &polygon.vertices {
                                if let Some((_, orig, curr)) =
                                    moved_vertices.iter().find(|(idx, ..)| *idx == vertex_idx)
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

        println!("\nChecking floor stability:");
        for ((subsector_id, floor_poly_idx), _) in &floor_polygon_vertices {
            if let Some(leaf) = bsp3d.get_subsector_leaf(*subsector_id) {
                if let Some(polygon) = leaf.polygons.get(*floor_poly_idx) {
                    let mut floor_moved = false;
                    for &vertex_idx in &polygon.vertices {
                        let original_pos = initial_vertex_positions[&vertex_idx];
                        let current_pos = bsp3d.vertices[vertex_idx];
                        if (original_pos.z - current_pos.z).abs() > 0.001 {
                            println!(
                                "  FLOOR MOVED: Subsector {} floor polygon {} vertex {} moved from {:?} to {:?}",
                                subsector_id, floor_poly_idx, vertex_idx, original_pos, current_pos
                            );
                            floor_moved = true;
                        }
                    }
                    if !floor_moved {
                        println!(
                            "  Floor polygon {} in subsector {} remained stationary",
                            floor_poly_idx, subsector_id
                        );
                    }
                }
            }
        }

        println!(
            "\nChecking wall movement (148 should shrink, 150,151 should stay, 152,153 should move):"
        );
        for ((linedef_id, subsector_id, poly_idx), _) in &wall_polygon_vertices {
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
                            "  Linedef {} wall polygon {} {} correctly",
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
                            "  WALL SHOULD MOVE: Linedef {} wall polygon {} should have {} but didn't",
                            linedef_id, poly_idx, action
                        );
                    } else if !should_move && wall_moved {
                        println!(
                            "  WALL MOVED: Linedef {} wall polygon {} should be stationary but moved",
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
                            "  Linedef {} wall polygon {} remained stationary (correct)",
                            linedef_id, poly_idx
                        );
                    }
                }
            }
        }

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
                                    "  Sector 26 ceiling vertex {} moved from {} to {} (expected)",
                                    vertex_idx, original_pos.z, current_pos.z
                                );
                            } else {
                                println!(
                                    "  Sector 26 ceiling vertex {} unexpected position: {} -> {} (expected 68)",
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
        let map = load_map(DOOM_WAD, "E1M1");

        let _linedef_484 = &map.linedefs[484];

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

        assert_eq!(segment.frontsector.num, 14, "Front sector should be 14");
        if let Some(back_sector) = &segment.backsector {
            assert_eq!(back_sector.num, 23, "Back sector should be 23");
        } else {
            panic!("Linedef 484 should have a back sector");
        }

        assert_eq!(
            segment.frontsector.floorheight, 32.0,
            "Front floor should be 32"
        );
        if let Some(back_sector) = &segment.backsector {
            assert_eq!(back_sector.floorheight, 32.0, "Back floor should be 32");
        }

        assert!(
            segment.sidedef.bottomtexture.is_some(),
            "Should have bottom texture"
        );

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

    /// Verify floor and ceiling polygon winding and normals for all E1M1
    /// subsectors. Floor polygons must have upward normals, ceiling polygons
    /// must have downward normals, and the cross-product of consecutive edges
    /// must agree with the stored normal direction.
    #[test]
    #[ignore = "Changing from triangles to n-poly has changed expectation: TODO: update"]
    /// Every non-sky subsector must have exactly 1 floor and 1 ceiling
    /// polygon. Floor and ceiling share the same XY vertex positions but
    /// with opposite winding (floor shoelace > 0, ceiling shoelace < 0).
    fn test_e1m1_floor_ceiling_polygon_normals() {
        use glam::Vec3;

        let map = load_map(DOOM_WAD, "E1M1");
        let bsp3d = &map.bsp_3d;
        let verts = &bsp3d.vertices;

        let mut failures = Vec::new();

        for (ssid, leaf) in bsp3d.subsector_leaves.iter().enumerate() {
            if leaf.polygons.is_empty() {
                continue;
            }

            // Skip sky subsectors (may lack floor or ceiling).
            let has_floor = !leaf.floor_polygons.is_empty();
            let has_ceil = !leaf.ceiling_polygons.is_empty();
            if !has_floor && !has_ceil {
                continue;
            }

            // Non-sky subsectors: exactly 1 floor and 1 ceiling.
            if leaf.floor_polygons.len() != 1 {
                failures.push(format!(
                    "ss={}: expected 1 floor polygon, got {}",
                    ssid,
                    leaf.floor_polygons.len()
                ));
                continue;
            }
            if leaf.ceiling_polygons.len() != 1 {
                failures.push(format!(
                    "ss={}: expected 1 ceiling polygon, got {}",
                    ssid,
                    leaf.ceiling_polygons.len()
                ));
                continue;
            }

            let floor_poly = &leaf.polygons[leaf.floor_polygons[0]];
            let ceil_poly = &leaf.polygons[leaf.ceiling_polygons[0]];

            // Normals.
            assert_eq!(floor_poly.normal, Vec3::new(0.0, 0.0, 1.0), "ss={}", ssid);
            assert_eq!(ceil_poly.normal, Vec3::new(0.0, 0.0, -1.0), "ss={}", ssid);

            // Same vertex count.
            if floor_poly.vertices.len() != ceil_poly.vertices.len() {
                failures.push(format!(
                    "ss={}: floor has {} verts, ceiling has {}",
                    ssid,
                    floor_poly.vertices.len(),
                    ceil_poly.vertices.len()
                ));
                continue;
            }

            let n = floor_poly.vertices.len();
            if n < 3 {
                failures.push(format!("ss={}: polygon has {} verts (< 3)", ssid, n));
                continue;
            }

            // Floor shoelace > 0, ceiling shoelace < 0.
            let floor_area = shoelace(floor_poly, verts);
            let ceil_area = shoelace(ceil_poly, verts);
            if floor_area <= 0.0 {
                failures.push(format!(
                    "ss={}: floor shoelace={:.2} (expected > 0)",
                    ssid, floor_area
                ));
            }
            if ceil_area >= 0.0 {
                failures.push(format!(
                    "ss={}: ceiling shoelace={:.2} (expected < 0)",
                    ssid, ceil_area
                ));
            }

            // XY positions must match in reverse order (opposite winding).
            // Mover sectors may have separated vertices with slightly
            // different positions, so use epsilon comparison.
            let floor_xy: Vec<(f32, f32)> = floor_poly
                .vertices
                .iter()
                .map(|&vi| (verts[vi].x, verts[vi].y))
                .collect();
            let ceil_xy: Vec<(f32, f32)> = ceil_poly
                .vertices
                .iter()
                .map(|&vi| (verts[vi].x, verts[vi].y))
                .collect();
            let ceil_xy_rev: Vec<(f32, f32)> = ceil_xy.iter().copied().rev().collect();
            let xy_match = floor_xy
                .iter()
                .zip(ceil_xy_rev.iter())
                .all(|(f, c)| (f.0 - c.0).abs() < 2.0 && (f.1 - c.1).abs() < 2.0);
            if !xy_match {
                failures.push(format!(
                    "ss={}: floor/ceiling XY mismatch\n  floor={:?}\n  ceil_rev={:?}",
                    ssid, floor_xy, ceil_xy_rev
                ));
            }
        }

        for f in &failures {
            println!("{}", f);
        }
        assert!(failures.is_empty(), "{} failures", failures.len());
    }

    /// Every floor/ceiling polygon must have no duplicate vertex indices
    /// and no zero-length edges.
    #[test]
    fn test_e1m1_no_degenerate_polygons() {
        let map = load_map(DOOM_WAD, "E1M1");
        let bsp3d = &map.bsp_3d;
        let verts = &bsp3d.vertices;

        let mut failures = Vec::new();

        for (ssid, leaf) in bsp3d.subsector_leaves.iter().enumerate() {
            for (label, indices) in [
                ("floor", &leaf.floor_polygons),
                ("ceil", &leaf.ceiling_polygons),
            ] {
                for &pi in indices {
                    let poly = &leaf.polygons[pi];
                    let n = poly.vertices.len();

                    for i in 0..n {
                        for j in (i + 1)..n {
                            if poly.vertices[i] == poly.vertices[j] {
                                failures.push(format!(
                                    "ss={} {}: duplicate index {} at positions {} and {}",
                                    ssid, label, poly.vertices[i], i, j
                                ));
                            }
                        }
                    }

                    for i in 0..n {
                        let a = verts[poly.vertices[i]];
                        let b = verts[poly.vertices[(i + 1) % n]];
                        let dist = ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt();
                        if dist < 0.01 {
                            failures.push(format!(
                                "ss={} {}: zero-length edge [{}<->{}] dist={:.6}",
                                ssid,
                                label,
                                i,
                                (i + 1) % n,
                                dist
                            ));
                        }
                    }
                }
            }
        }

        for f in &failures {
            println!("{}", f);
        }
        assert!(failures.is_empty(), "{} failures", failures.len());
    }

    fn shoelace(poly: &crate::SurfacePolygon, verts: &[glam::Vec3]) -> f32 {
        let n = poly.vertices.len();
        (0..n)
            .map(|i| {
                let a = verts[poly.vertices[i]];
                let b = verts[poly.vertices[(i + 1) % n]];
                a.x * b.y - b.x * a.y
            })
            .sum()
    }
}
