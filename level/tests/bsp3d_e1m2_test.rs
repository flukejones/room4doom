use level::SurfaceKind;
use test_utils::{doom_wad_path, load_map};

/// Shoelace area from vertex indices. Positive = CCW in standard coords.
fn shoelace(indices: &[usize], verts: &[glam::Vec3]) -> f32 {
    let n = indices.len();
    (0..n)
        .map(|i| {
            let a = verts[indices[i]];
            let b = verts[indices[(i + 1) % n]];
            a.x * b.y - b.x * a.y
        })
        .sum()
}

/// No floor or ceiling polygon in E1M2 may have duplicate vertex indices
/// or zero-length edges.
#[test]
fn test_e1m2_no_degenerate_polygons() {
    let map = load_map(&doom_wad_path(), "E1M2");
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

                if n < 3 {
                    failures.push(format!(
                        "ss={} {}: fewer than 3 vertices ({})",
                        ssid, label, n
                    ));
                    continue;
                }

                // Duplicate vertex indices.
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

                // Zero-length edges.
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

/// Every non-sky subsector in E1M2 must have exactly 1 floor and 1
/// ceiling polygon with correct normals and winding:
/// - Floor: normal (0,0,1), positive shoelace
/// - Ceiling: normal (0,0,-1), negative shoelace
/// - Floor and ceiling share the same XY positions in reverse order
#[test]
fn test_e1m2_floor_ceiling_polygon_normals() {
    use glam::Vec3;

    let map = load_map(&doom_wad_path(), "E1M2");
    let bsp3d = &map.bsp_3d;
    let verts = &bsp3d.vertices;

    let mut failures = Vec::new();

    for (ssid, leaf) in bsp3d.subsector_leaves.iter().enumerate() {
        if leaf.polygons.is_empty() {
            continue;
        }

        let has_floor = !leaf.floor_polygons.is_empty();
        let has_ceil = !leaf.ceiling_polygons.is_empty();
        if !has_floor && !has_ceil {
            continue;
        }

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
        if floor_poly.normal != Vec3::new(0.0, 0.0, 1.0) {
            failures.push(format!(
                "ss={}: floor normal {:?} != (0,0,1)",
                ssid, floor_poly.normal
            ));
        }
        if ceil_poly.normal != Vec3::new(0.0, 0.0, -1.0) {
            failures.push(format!(
                "ss={}: ceiling normal {:?} != (0,0,-1)",
                ssid, ceil_poly.normal
            ));
        }

        if floor_poly.vertices.len() < 3 {
            failures.push(format!(
                "ss={}: floor has {} verts (< 3)",
                ssid,
                floor_poly.vertices.len()
            ));
            continue;
        }
        if ceil_poly.vertices.len() < 3 {
            failures.push(format!(
                "ss={}: ceiling has {} verts (< 3)",
                ssid,
                ceil_poly.vertices.len()
            ));
            continue;
        }

        // Floor shoelace > 0, ceiling shoelace < 0.
        let floor_area = shoelace(&floor_poly.vertices, verts);
        let ceil_area = shoelace(&ceil_poly.vertices, verts);
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

        // The smaller polygon's XY positions must be a subset of the
        // larger one. Mover sectors may have extra boundary vertices in
        // the moving polygon only.
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
        let (smaller, larger) = if floor_xy.len() <= ceil_xy.len() {
            (&floor_xy, &ceil_xy)
        } else {
            (&ceil_xy, &floor_xy)
        };
        let all_found = smaller.iter().all(|s| {
            larger
                .iter()
                .any(|l| (s.0 - l.0).abs() < 2.0 && (s.1 - l.1).abs() < 2.0)
        });
        if !all_found {
            failures.push(format!(
                "ss={}: floor/ceiling XY mismatch (smaller not subset of larger)\n  floor={:?}\n  ceil={:?}",
                ssid, floor_xy, ceil_xy
            ));
        }
    }

    for f in &failures {
        println!("{}", f);
    }
    assert!(failures.is_empty(), "{} failures", failures.len());
}

/// All floor vertices within a single polygon must be at the same Z
/// height. Same for ceiling vertices.
#[test]
fn test_e1m2_flat_polygon_coplanarity() {
    let map = load_map(&doom_wad_path(), "E1M2");
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
                if poly.vertices.is_empty() {
                    continue;
                }
                let z0 = verts[poly.vertices[0]].z;
                for &vi in &poly.vertices[1..] {
                    let z = verts[vi].z;
                    if (z - z0).abs() > 0.01 {
                        failures.push(format!(
                            "ss={} {}: vertex {} at z={:.2}, expected z={:.2}",
                            ssid, label, vi, z, z0
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

/// SS267 is a lowering platform (sector 109). Linedefs 375 and 376 on the
/// right side share a vertex at (-128, 424) that lies on SS267's floor
/// polygon edge but is not a carved polygon vertex. These are Middle walls,
/// not ZH. The platform's floor polygon must share the same vertex index
/// so move_surface moves the wall bottom when the floor lowers.
#[test]
fn test_e1m2_ss267_platform_vertex_sharing() {
    use std::collections::HashSet;

    let map = load_map(&doom_wad_path(), "E1M2");
    let bsp3d = &map.bsp_3d;
    let verts = &bsp3d.vertices;

    let leaf267 = &bsp3d.subsector_leaves[267];
    let sector_id = leaf267.polygons[leaf267.floor_polygons[0]].sector_id;

    let floor_verts: HashSet<usize> = bsp3d.sector_subsectors[sector_id]
        .iter()
        .flat_map(|&ssid| {
            let leaf = &bsp3d.subsector_leaves[ssid];
            leaf.floor_polygons
                .iter()
                .flat_map(|&fpi| leaf.polygons[fpi].vertices.iter().copied())
                .collect::<Vec<_>>()
        })
        .collect();

    let floor_h = verts[leaf267.polygons[leaf267.floor_polygons[0]].vertices[0]].z;

    // Collect wall bottom vertices at floor height on linedefs 375/376.
    let target_lds: HashSet<usize> = [375, 376].into_iter().collect();
    let mut unshared = Vec::new();
    for leaf in &bsp3d.subsector_leaves {
        for poly in &leaf.polygons {
            if let SurfaceKind::Vertical {
                linedef_id,
                ..
            } = &poly.surface_kind
            {
                if !target_lds.contains(linedef_id) {
                    continue;
                }
                for &vi in &poly.vertices {
                    if (verts[vi].z - floor_h).abs() < 1.0 && !floor_verts.contains(&vi) {
                        unshared.push(vi);
                    }
                }
            }
        }
    }
    // Deduplicate (same vertex appears in both ld 375 and 376).
    unshared.sort_unstable();
    unshared.dedup();

    for &vi in &unshared {
        let v = verts[vi];
        println!(
            "Unshared wall vi={} ({:.1},{:.1},{:.1}) not in sector {} floor",
            vi, v.x, v.y, v.z, sector_id
        );
    }

    assert!(
        unshared.is_empty(),
        "SS267 platform sector {}: wall vertices {:?} at floor height not shared with floor polygon",
        sector_id,
        unshared
    );
}

#[test]
fn test_debug_subsectors() {
    let map = load_map(&doom_wad_path(), "E1M2");
    let bsp3d = &map.bsp_3d;
    let verts = &bsp3d.vertices;
    let target_ss = [285, 288, 290, 307, 309];

    // Build reverse map: subsector -> sector
    let mut ss_to_sector = vec![usize::MAX; bsp3d.subsector_leaves.len()];
    for (sec_id, ss_list) in bsp3d.sector_subsectors.iter().enumerate() {
        for &ss_id in ss_list {
            ss_to_sector[ss_id] = sec_id;
        }
    }

    for &ssid in &target_ss {
        let leaf = &bsp3d.subsector_leaves[ssid];
        let sector_id = ss_to_sector[ssid];
        println!("=== SUBSECTOR {} (sector {}) ===", ssid, sector_id);
        println!(
            "  floor_polygons: {}, ceiling_polygons: {}, total polygons: {}",
            leaf.floor_polygons.len(),
            leaf.ceiling_polygons.len(),
            leaf.polygons.len()
        );

        // Identify wall polygons (not in floor or ceiling lists)
        let floor_set: std::collections::HashSet<usize> =
            leaf.floor_polygons.iter().copied().collect();
        let ceil_set: std::collections::HashSet<usize> =
            leaf.ceiling_polygons.iter().copied().collect();

        // Floor polygons
        for &pi in &leaf.floor_polygons {
            let poly = &leaf.polygons[pi];
            let positions: Vec<_> = poly
                .vertices
                .iter()
                .map(|&vi| {
                    let v = verts[vi];
                    format!("v{}=({:.2},{:.2},{:.2})", vi, v.x, v.y, v.z)
                })
                .collect();
            let area = shoelace(&poly.vertices, verts);
            println!(
                "  FLOOR poly[{}]: {} verts, shoelace={:.2}, normal={:?}",
                pi,
                poly.vertices.len(),
                area,
                poly.normal
            );
            println!("    vertices: {}", positions.join(", "));
        }

        // Ceiling polygons
        for &pi in &leaf.ceiling_polygons {
            let poly = &leaf.polygons[pi];
            let positions: Vec<_> = poly
                .vertices
                .iter()
                .map(|&vi| {
                    let v = verts[vi];
                    format!("v{}=({:.2},{:.2},{:.2})", vi, v.x, v.y, v.z)
                })
                .collect();
            let area = shoelace(&poly.vertices, verts);
            println!(
                "  CEIL  poly[{}]: {} verts, shoelace={:.2}, normal={:?}",
                pi,
                poly.vertices.len(),
                area,
                poly.normal
            );
            println!("    vertices: {}", positions.join(", "));
        }

        // Wall polygons
        for (pi, poly) in leaf.polygons.iter().enumerate() {
            if floor_set.contains(&pi) || ceil_set.contains(&pi) {
                continue;
            }
            let positions: Vec<_> = poly
                .vertices
                .iter()
                .map(|&vi| {
                    let v = verts[vi];
                    format!("v{}=({:.2},{:.2},{:.2})", vi, v.x, v.y, v.z)
                })
                .collect();
            match &poly.surface_kind {
                SurfaceKind::Vertical {
                    wall_type,
                    texture,
                    linedef_id,
                    two_sided,
                    ..
                } => {
                    println!(
                        "  WALL  poly[{}]: {} verts, normal=({:.3},{:.3},{:.3}), type={:?}, tex={:?}, linedef={}, two_sided={}",
                        pi,
                        poly.vertices.len(),
                        poly.normal.x,
                        poly.normal.y,
                        poly.normal.z,
                        wall_type,
                        texture,
                        linedef_id,
                        two_sided
                    );
                }
                SurfaceKind::Horizontal {
                    ..
                } => {
                    println!(
                        "  WALL? poly[{}]: {} verts, normal={:?} (horizontal surface_kind in wall slot?)",
                        pi,
                        poly.vertices.len(),
                        poly.normal
                    );
                }
            }
            println!("    vertices: {}", positions.join(", "));
        }
        println!();
    }

    // Print sector heights
    let relevant_sectors = [120, 121, 122, 124, 125, 129, 130];
    println!("=== SECTOR HEIGHTS ===");
    for &sec_id in &relevant_sectors {
        let sec = &map.sectors[sec_id];
        println!(
            "  sector {}: floor={}, ceil={}, subsectors={:?}",
            sec_id, sec.floorheight, sec.ceilingheight, bsp3d.sector_subsectors[sec_id]
        );
    }

    // Print segments for each target subsector
    println!("=== SEGMENTS PER SUBSECTOR ===");
    for &ssid in &target_ss {
        let ss = &map.subsectors[ssid];
        println!(
            "SS{} segments (first_seg={}, seg_count={}):",
            ssid, ss.start_seg, ss.seg_count
        );
        for si in ss.start_seg..(ss.start_seg + ss.seg_count as u32) {
            let seg = &map.segments[si as usize];
            let v1 = *seg.v1;
            let v2 = *seg.v2;
            let front_sec = seg.frontsector.num;
            let back_sec = seg.backsector.as_ref().map(|s| s.num);
            let ld = seg.linedef.num;
            let has_top = seg.sidedef.toptexture.is_some();
            let has_mid = seg.sidedef.midtexture.is_some();
            let has_bot = seg.sidedef.bottomtexture.is_some();
            println!(
                "  seg {}: v1=({:.2},{:.2}) v2=({:.2},{:.2}) front_sec={} back_sec={:?} linedef={} top={} mid={} bot={}",
                si, v1.x, v1.y, v2.x, v2.y, front_sec, back_sec, ld, has_top, has_mid, has_bot
            );
        }
    }

    // Now check neighboring subsectors for walls that border our targets
    println!("=== NEIGHBORING SUBSECTOR WALLS ===");
    // Collect sectors of our target subsectors
    let target_sectors: std::collections::HashSet<usize> =
        target_ss.iter().map(|&ss| ss_to_sector[ss]).collect();
    println!("Target sectors: {:?}", target_sectors);

    // For each subsector not in our target list, check if it has walls
    // with linedefs that reference our target sectors
    for (ssid, leaf) in bsp3d.subsector_leaves.iter().enumerate() {
        if target_ss.contains(&ssid) {
            continue;
        }
        let sec = ss_to_sector[ssid];
        for (pi, poly) in leaf.polygons.iter().enumerate() {
            if let SurfaceKind::Vertical {
                wall_type,
                linedef_id,
                two_sided,
                ..
            } = &poly.surface_kind
            {
                // Check if this wall's linedef connects to one of our target sectors
                let ld = &map.linedefs[*linedef_id];
                let front_sec = ld.front_sidedef.sector.num as usize;
                let back_sec = ld.back_sidedef.as_ref().map(|sd| sd.sector.num as usize);
                let touches_target = target_sectors.contains(&front_sec)
                    || back_sec.map_or(false, |bs| target_sectors.contains(&bs));
                if touches_target {
                    let positions: Vec<_> = poly
                        .vertices
                        .iter()
                        .map(|&vi| {
                            let v = verts[vi];
                            format!("v{}=({:.2},{:.2},{:.2})", vi, v.x, v.y, v.z)
                        })
                        .collect();
                    println!(
                        "  SS{} (sec {}): poly[{}] {:?} linedef={} front_sec={} back_sec={:?} two_sided={} normal=({:.3},{:.3},{:.3})",
                        ssid,
                        sec,
                        pi,
                        wall_type,
                        linedef_id,
                        front_sec,
                        back_sec,
                        two_sided,
                        poly.normal.x,
                        poly.normal.y,
                        poly.normal.z
                    );
                    println!("    vertices: {}", positions.join(", "));
                }
            }
        }
    }
}
