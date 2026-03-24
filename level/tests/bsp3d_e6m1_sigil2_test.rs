use test_utils::{doom_wad_path, load_map_with_pwad, sigil2_wad_path};

/// Validate subsector 2587 floor polygon in E6M1 (sigil2.wad).
/// Checks vertex count, duplicate indices, and positive shoelace area.
#[test]
fn test_e6m1_subsector_2587_polygon() {
    let map = load_map_with_pwad(&doom_wad_path(), &sigil2_wad_path(), "E6M1");

    let bsp3d = &map.bsp_3d;

    assert!(
        bsp3d.subsector_leaves.len() > 2587,
        "Map must have at least 2588 subsectors, got {}",
        bsp3d.subsector_leaves.len()
    );

    let leaf = &bsp3d.subsector_leaves[2587];
    let vertices = &bsp3d.vertices;

    for &fp_idx in &leaf.floor_polygons {
        let poly = &leaf.polygons[fp_idx];
        assert!(
            poly.vertices.len() >= 3,
            "Floor polygon must have >= 3 vertices, got {}",
            poly.vertices.len()
        );

        // No duplicate vertex indices.
        for i in 0..poly.vertices.len() {
            for j in (i + 1)..poly.vertices.len() {
                assert_ne!(
                    poly.vertices[i], poly.vertices[j],
                    "Duplicate vertex index {} at positions {} and {}",
                    poly.vertices[i], i, j
                );
            }
        }

        // Positive shoelace area (correct winding for floor).
        let n = poly.vertices.len();
        let area: f32 = (0..n)
            .map(|i| {
                let a = vertices[poly.vertices[i]];
                let b = vertices[poly.vertices[(i + 1) % n]];
                a.x * b.y - b.x * a.y
            })
            .sum();
        assert!(
            area > 0.0,
            "Floor polygon shoelace area must be positive, got {}",
            area
        );
    }
}

/// Validate that all floor normals point up (0,0,1) with positive shoelace
/// and all ceiling normals point down (0,0,-1) with negative shoelace.
#[test]
fn test_e6m1_floor_ceiling_normals() {
    let map = load_map_with_pwad(&doom_wad_path(), &sigil2_wad_path(), "E6M1");
    let bsp3d = &map.bsp_3d;
    let verts = &bsp3d.vertices;

    let mut failures = Vec::new();

    for (ssid, leaf) in bsp3d.subsector_leaves.iter().enumerate() {
        for &pi in &leaf.floor_polygons {
            let poly = &leaf.polygons[pi];
            if poly.vertices.len() < 3 {
                continue;
            }
            if poly.normal.z < 0.99 {
                failures.push(format!(
                    "ss={} floor poly={}: normal={:?} (expected +Z)",
                    ssid, pi, poly.normal
                ));
            }
            let n = poly.vertices.len();
            let area: f32 = (0..n)
                .map(|i| {
                    let a = verts[poly.vertices[i]];
                    let b = verts[poly.vertices[(i + 1) % n]];
                    a.x * b.y - b.x * a.y
                })
                .sum();
            if area <= 0.0 {
                failures.push(format!(
                    "ss={} floor poly={}: shoelace={:.2} (expected positive)",
                    ssid, pi, area
                ));
            }
        }

        for &pi in &leaf.ceiling_polygons {
            let poly = &leaf.polygons[pi];
            if poly.vertices.len() < 3 {
                continue;
            }
            if poly.normal.z > -0.99 {
                failures.push(format!(
                    "ss={} ceil poly={}: normal={:?} (expected -Z)",
                    ssid, pi, poly.normal
                ));
            }
            let n = poly.vertices.len();
            let area: f32 = (0..n)
                .map(|i| {
                    let a = verts[poly.vertices[i]];
                    let b = verts[poly.vertices[(i + 1) % n]];
                    a.x * b.y - b.x * a.y
                })
                .sum();
            if area >= 0.0 {
                failures.push(format!(
                    "ss={} ceil poly={}: shoelace={:.2} (expected negative)",
                    ssid, pi, area
                ));
            }
        }
    }

    for f in &failures {
        println!("{}", f);
    }

    // Diagnostic: print vertex positions for each failing subsector.
    let failing_ssids: std::collections::HashSet<usize> = failures
        .iter()
        .filter_map(|f| {
            f.strip_prefix("ss=")
                .and_then(|s| s.split_whitespace().next())
                .and_then(|n| n.parse().ok())
        })
        .collect();
    for &ssid in &failing_ssids {
        let leaf = &bsp3d.subsector_leaves[ssid];
        println!("\n--- SS {} sector {} ---", ssid, leaf.sector_id);
        for &pi in &leaf.floor_polygons {
            let poly = &leaf.polygons[pi];
            let positions: Vec<_> = poly
                .vertices
                .iter()
                .map(|&vi| {
                    let v = verts[vi];
                    format!("{}=({:.1},{:.1},{:.1})", vi, v.x, v.y, v.z)
                })
                .collect();
            println!("  floor[{}]: {}", pi, positions.join(", "));
        }
        for &pi in &leaf.ceiling_polygons {
            let poly = &leaf.polygons[pi];
            let positions: Vec<_> = poly
                .vertices
                .iter()
                .map(|&vi| {
                    let v = verts[vi];
                    format!("{}=({:.1},{:.1},{:.1})", vi, v.x, v.y, v.z)
                })
                .collect();
            let n = poly.vertices.len();
            let area: f32 = (0..n)
                .map(|i| {
                    let a = verts[poly.vertices[i]];
                    let b = verts[poly.vertices[(i + 1) % n]];
                    a.x * b.y - b.x * a.y
                })
                .sum();
            println!(
                "  ceil[{}]: shoelace={:.4} normal=({:.2},{:.2},{:.2}) {}",
                pi,
                area,
                poly.normal.x,
                poly.normal.y,
                poly.normal.z,
                positions.join(", ")
            );
        }
    }

    assert!(
        failures.is_empty(),
        "{} inverted normal(s) in E6M1",
        failures.len()
    );
}

/// For mover wall polygons the renderer recomputes the normal from the
/// cross product of the first two edges. Verify this matches the stored
/// normal for every wall polygon with moves=true. Floor/ceiling use the
/// stored normal directly (horizontal normal never changes with movement).
#[test]
fn test_e6m1_mover_wall_cross_product_normals() {
    use glam::Vec3;
    use level::SurfaceKind;

    let map = load_map_with_pwad(&doom_wad_path(), &sigil2_wad_path(), "E6M1");
    let bsp3d = &map.bsp_3d;
    let verts = &bsp3d.vertices;

    let mut failures = Vec::new();

    for (ssid, leaf) in bsp3d.subsector_leaves.iter().enumerate() {
        for (pi, poly) in leaf.polygons.iter().enumerate() {
            if !poly.moves || poly.vertices.len() < 3 {
                continue;
            }
            if !matches!(poly.surface_kind, SurfaceKind::Vertical { .. }) {
                continue;
            }
            let p0 = verts[poly.vertices[0]];
            let p1 = verts[poly.vertices[1]];
            let p2 = verts[poly.vertices[2]];
            let edge1 = Vec3::new(p1.x - p0.x, p1.y - p0.y, p1.z - p0.z);
            let edge2 = Vec3::new(p2.x - p0.x, p2.y - p0.y, p2.z - p0.z);
            let cross = edge1.cross(edge2);
            // Degenerate cross product falls back to stored normal in
            // the renderer — not a failure.
            if cross.length_squared() <= f32::EPSILON {
                continue;
            }
            let computed = cross.normalize();
            let dot = computed.dot(poly.normal);
            if dot < 0.0 {
                failures.push(format!(
                    "ss={} wall poly={}: cross_normal=({:.3},{:.3},{:.3}) stored=({:.3},{:.3},{:.3}) dot={:.3}",
                    ssid, pi,
                    computed.x, computed.y, computed.z,
                    poly.normal.x, poly.normal.y, poly.normal.z,
                    dot
                ));
            }
        }
    }

    for f in &failures {
        println!("{}", f);
    }
    assert!(
        failures.is_empty(),
        "{} mover wall polygon(s) with cross-product normal mismatch",
        failures.len()
    );
}

/// Sector 76 is a zero-height ceiling mover (floor=64, ceil=64). When the
/// ceiling raises, floor and ceiling vertices must be fully separated —
/// no shared vertex indices between floor and ceiling polygons.
#[test]
fn test_e6m1_sector76_floor_ceil_separation() {
    use std::collections::HashSet;

    let map = load_map_with_pwad(&doom_wad_path(), &sigil2_wad_path(), "E6M1");
    let bsp3d = &map.bsp_3d;
    let verts = &bsp3d.vertices;

    let sector_id = 76;
    let ss_ids = &bsp3d.sector_subsectors[sector_id];

    let mut floor_vis: HashSet<usize> = HashSet::new();
    let mut ceil_vis: HashSet<usize> = HashSet::new();

    for &ssid in ss_ids {
        let leaf = &bsp3d.subsector_leaves[ssid];
        for &pi in &leaf.floor_polygons {
            for &vi in &leaf.polygons[pi].vertices {
                floor_vis.insert(vi);
            }
        }
        for &pi in &leaf.ceiling_polygons {
            for &vi in &leaf.polygons[pi].vertices {
                ceil_vis.insert(vi);
            }
        }
    }

    let shared: Vec<usize> = floor_vis.intersection(&ceil_vis).copied().collect();
    for &vi in &shared {
        let v = verts[vi];
        println!("SHARED vi={} pos=({:.1},{:.1},{:.1})", vi, v.x, v.y, v.z);
    }

    // Also check: are there wall vertices at ceil height not in ceil polys?
    let mut wall_at_ceil_unshared = Vec::new();
    for &ssid in ss_ids {
        let leaf = &bsp3d.subsector_leaves[ssid];
        for poly in &leaf.polygons {
            if let level::SurfaceKind::Vertical {
                ..
            } = &poly.surface_kind
            {
                for &vi in &poly.vertices {
                    let v = verts[vi];
                    if (v.z - 64.0).abs() < 1.0
                        && !ceil_vis.contains(&vi)
                        && !floor_vis.contains(&vi)
                    {
                        wall_at_ceil_unshared.push((ssid, vi));
                    }
                }
            }
        }
    }
    for &(ssid, vi) in &wall_at_ceil_unshared {
        let v = verts[vi];
        println!(
            "WALL NOT IN CEIL/FLOOR ss={} vi={} pos=({:.1},{:.1},{:.1})",
            ssid, vi, v.x, v.y, v.z
        );
    }

    // Print all floor and ceiling polys for context.
    for &ssid in ss_ids {
        let leaf = &bsp3d.subsector_leaves[ssid];
        println!("\n--- SS {} ---", ssid);
        for &pi in &leaf.floor_polygons {
            let poly = &leaf.polygons[pi];
            let positions: Vec<_> = poly
                .vertices
                .iter()
                .map(|&vi| {
                    let v = verts[vi];
                    format!("{}=({:.1},{:.1},{:.1})", vi, v.x, v.y, v.z)
                })
                .collect();
            println!("  floor[{}]: {}", pi, positions.join(", "));
        }
        for &pi in &leaf.ceiling_polygons {
            let poly = &leaf.polygons[pi];
            let positions: Vec<_> = poly
                .vertices
                .iter()
                .map(|&vi| {
                    let v = verts[vi];
                    format!("{}=({:.1},{:.1},{:.1})", vi, v.x, v.y, v.z)
                })
                .collect();
            println!("  ceil[{}]: {}", pi, positions.join(", "));
        }
    }

    assert!(
        shared.is_empty(),
        "Sector 76: {} vertex indices shared between floor and ceiling — ceiling mover will drag floor",
        shared.len()
    );

    // Check wall vertices at ceiling height share with ceiling polygons.
    // Upper walls on the boundary (sector 76 side) should have bottom
    // vertices in ceil_vis so move_surface moves them.
    let border_lds: HashSet<usize> = map
        .segments
        .iter()
        .filter(|s| {
            s.frontsector.num == sector_id as i32
                || s.backsector
                    .as_ref()
                    .map_or(false, |b| b.num == sector_id as i32)
        })
        .map(|s| s.linedef.num as usize)
        .collect();

    let mut wall_unshared = Vec::new();
    for leaf in &bsp3d.subsector_leaves {
        for poly in &leaf.polygons {
            if let level::SurfaceKind::Vertical {
                linedef_id,
                wall_type,
                ..
            } = &poly.surface_kind
            {
                if !border_lds.contains(linedef_id) {
                    continue;
                }
                for &vi in &poly.vertices {
                    let v = verts[vi];
                    if (v.z - 64.0).abs() < 1.0
                        && !ceil_vis.contains(&vi)
                        && !floor_vis.contains(&vi)
                    {
                        wall_unshared.push((vi, *linedef_id, format!("{:?}", wall_type)));
                    }
                }
            }
        }
    }
    wall_unshared.sort_by_key(|x| x.0);
    wall_unshared.dedup_by_key(|x| x.0);
    for &(vi, ld, ref wt) in &wall_unshared {
        let v = verts[vi];
        // Find nearby ceil vertex.
        let near_ceil: Vec<_> = ceil_vis
            .iter()
            .filter(|&&cvi| {
                let cv = verts[cvi];
                (cv.x - v.x).abs() < 2.0 && (cv.y - v.y).abs() < 2.0
            })
            .collect();
        println!(
            "WALL NOT IN CEIL vi={} ({:.1},{:.1},{:.1}) ld={} wt={} near_ceil={:?}",
            vi, v.x, v.y, v.z, ld, wt, near_ceil
        );
    }
    assert!(
        wall_unshared.is_empty(),
        "Sector 76: {} wall vertices at ceiling height not shared with ceiling polygons",
        wall_unshared.len()
    );
}

/// Check that no subsector in E6M1 has degenerate floor polygons.
/// Validates: >= 3 vertices, no duplicate indices, positive shoelace area.
#[test]
fn test_e6m1_no_degenerate_floor_polygons() {
    let map = load_map_with_pwad(&doom_wad_path(), &sigil2_wad_path(), "E6M1");

    let bsp3d = &map.bsp_3d;
    let vertices = &bsp3d.vertices;

    let mut failures = Vec::new();

    for (ssid, leaf) in bsp3d.subsector_leaves.iter().enumerate() {
        for &fp_idx in &leaf.floor_polygons {
            let poly = &leaf.polygons[fp_idx];

            if poly.vertices.len() < 3 {
                failures.push((ssid, fp_idx, "fewer than 3 vertices"));
                continue;
            }

            // Check duplicate vertex indices.
            let has_dup = (0..poly.vertices.len()).any(|i| {
                ((i + 1)..poly.vertices.len()).any(|j| poly.vertices[i] == poly.vertices[j])
            });
            if has_dup {
                failures.push((ssid, fp_idx, "duplicate vertex index"));
                continue;
            }

            // Check shoelace area.
            let n = poly.vertices.len();
            let area: f32 = (0..n)
                .map(|i| {
                    let a = vertices[poly.vertices[i]];
                    let b = vertices[poly.vertices[(i + 1) % n]];
                    a.x * b.y - b.x * a.y
                })
                .sum();
            if area <= 0.0 {
                failures.push((ssid, fp_idx, "non-positive shoelace area"));
            }
        }
    }

    if !failures.is_empty() {
        for (ssid, fp_idx, reason) in &failures {
            println!(
                "Degenerate: subsector={} floor_poly={} reason={}",
                ssid, fp_idx, reason
            );
        }
    }

    assert!(
        failures.is_empty(),
        "{} degenerate floor polygon(s) found across E6M1",
        failures.len()
    );
}
