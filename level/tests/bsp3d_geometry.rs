//! Cross-map BSP3D geometry invariants: floor/ceiling normals & winding,
//! degenerate polygons, flat coplanarity, generated blockmap coverage.

use level::{MovementType, PolyFlags};
use test_utils::{
    assert_floor_ceiling_normals, doom_wad_path, doom1_wad_path, load_map, load_map_with_pwad,
    move_sector_surface, sigil2_wad_path,
};

// ---------------------------------------------------------------------------
// Floor/ceiling normals & winding (every non-sky subsector: one +Z floor, one
// −Z ceiling, smaller XY ⊆ larger). Shared scan in `test_utils`.
// ---------------------------------------------------------------------------

#[test]
fn e1m1_floor_ceiling_normals() {
    assert_floor_ceiling_normals(&load_map(&doom1_wad_path(), "E1M1"));
}

#[cfg_attr(not(feature = "wad-doom"), ignore = "needs doom.wad (~/doom/)")]
#[test]
fn e1m2_floor_ceiling_normals() {
    assert_floor_ceiling_normals(&load_map(&doom_wad_path(), "E1M2"));
}

#[cfg_attr(
    all(not(feature = "wad-doom"), not(feature = "wad-sigil2")),
    ignore = "needs doom.wad + sigil2.wad (~/doom/)"
)]
#[test]
fn e6m1_floor_ceiling_normals() {
    let map = load_map_with_pwad(&doom_wad_path(), &sigil2_wad_path(), "E6M1");
    assert_floor_ceiling_normals(&map);
}

// ---------------------------------------------------------------------------
// Degenerate-polygon scans: no floor/ceiling polygon may have < 3 vertices,
// duplicate indices, or zero-length edges.
// ---------------------------------------------------------------------------

#[cfg_attr(not(feature = "wad-doom"), ignore = "needs doom.wad (~/doom/)")]
#[test]
fn e1m2_no_degenerate_polygons() {
    let map = load_map(&doom_wad_path(), "E1M2");
    let bsp3d = &map.bsp_3d;
    let verts = &bsp3d.vertices;
    let mut failures = Vec::new();

    for ssid in 0..bsp3d.leaves.len() {
        let floors: Vec<usize> = bsp3d.leaf_floor_polys(ssid).collect();
        let ceils: Vec<usize> = bsp3d.leaf_ceiling_polys(ssid).collect();
        for (label, indices) in [("floor", &floors), ("ceil", &ceils)] {
            for &pi in indices {
                let poly_verts = bsp3d.poly_vert_indices(pi);
                let n = poly_verts.len();
                if n < 3 {
                    failures.push(format!("ss={ssid} {label}: < 3 vertices ({n})"));
                    continue;
                }
                for i in 0..n {
                    for j in (i + 1)..n {
                        if poly_verts[i] == poly_verts[j] {
                            failures.push(format!(
                                "ss={ssid} {label}: duplicate index {} at {i},{j}",
                                poly_verts[i]
                            ));
                        }
                    }
                }
                for i in 0..n {
                    let a = verts[poly_verts[i]];
                    let b = verts[poly_verts[(i + 1) % n]];
                    let dist = ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt();
                    if dist < 0.01 {
                        failures.push(format!(
                            "ss={ssid} {label}: zero-length edge [{i}<->{}] dist={dist:.6}",
                            (i + 1) % n
                        ));
                    }
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[cfg_attr(
    all(not(feature = "wad-doom"), not(feature = "wad-sigil2")),
    ignore = "needs doom.wad + sigil2.wad (~/doom/)"
)]
#[test]
fn e6m1_no_degenerate_floor_polygons() {
    let map = load_map_with_pwad(&doom_wad_path(), &sigil2_wad_path(), "E6M1");
    let bsp3d = &map.bsp_3d;
    let verts = &bsp3d.vertices;
    let mut failures = Vec::new();

    for ssid in 0..bsp3d.leaves.len() {
        for fp_idx in bsp3d.leaf_floor_polys(ssid) {
            let poly_verts = bsp3d.poly_vert_indices(fp_idx);
            let n = poly_verts.len();
            if n < 3 {
                failures.push(format!("ss={ssid} fp={fp_idx}: < 3 vertices"));
                continue;
            }
            let has_dup = (0..n).any(|i| ((i + 1)..n).any(|j| poly_verts[i] == poly_verts[j]));
            if has_dup {
                failures.push(format!("ss={ssid} fp={fp_idx}: duplicate vertex index"));
                continue;
            }
            let area = test_utils::shoelace(poly_verts, verts);
            if area <= 0.0 {
                failures.push(format!(
                    "ss={ssid} fp={fp_idx}: shoelace={area:.2} (expected > 0)"
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

// ---------------------------------------------------------------------------
// Flat coplanarity: every vertex in a floor (or ceiling) polygon shares one Z.
// ---------------------------------------------------------------------------

#[cfg_attr(not(feature = "wad-doom"), ignore = "needs doom.wad (~/doom/)")]
#[test]
fn e1m2_flat_polygon_coplanarity() {
    let map = load_map(&doom_wad_path(), "E1M2");
    let bsp3d = &map.bsp_3d;
    let verts = &bsp3d.vertices;
    let mut failures = Vec::new();

    for ssid in 0..bsp3d.leaves.len() {
        let floors: Vec<usize> = bsp3d.leaf_floor_polys(ssid).collect();
        let ceils: Vec<usize> = bsp3d.leaf_ceiling_polys(ssid).collect();
        for (label, indices) in [("floor", &floors), ("ceil", &ceils)] {
            for &pi in indices {
                let poly_verts = bsp3d.poly_vert_indices(pi);
                if poly_verts.is_empty() {
                    continue;
                }
                let z0 = verts[poly_verts[0]].z;
                for &vi in &poly_verts[1..] {
                    let z = verts[vi].z;
                    if (z - z0).abs() > 0.01 {
                        failures.push(format!(
                            "ss={ssid} {label}: vertex {vi} z={z:.2} != {z0:.2}"
                        ));
                    }
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

// ---------------------------------------------------------------------------
// Specific-subsector floor polygon validity (E6M1 ss2587 regression).
// ---------------------------------------------------------------------------

#[cfg_attr(
    all(not(feature = "wad-doom"), not(feature = "wad-sigil2")),
    ignore = "needs doom.wad + sigil2.wad (~/doom/)"
)]
#[test]
fn e6m1_subsector_2587_polygon() {
    let map = load_map_with_pwad(&doom_wad_path(), &sigil2_wad_path(), "E6M1");
    let bsp3d = &map.bsp_3d;
    let verts = &bsp3d.vertices;

    assert!(
        bsp3d.leaves.len() > 2587,
        "map must have >= 2588 subsectors, got {}",
        bsp3d.leaves.len()
    );

    for fp_idx in bsp3d.leaf_floor_polys(2587) {
        let poly_verts = bsp3d.poly_vert_indices(fp_idx);
        let n = poly_verts.len();
        assert!(n >= 3, "floor polygon must have >= 3 vertices, got {n}");
        for i in 0..n {
            for j in (i + 1)..n {
                assert_ne!(
                    poly_verts[i], poly_verts[j],
                    "duplicate vertex index {} at {i},{j}",
                    poly_verts[i]
                );
            }
        }
        let area = test_utils::shoelace(poly_verts, verts);
        assert!(area > 0.0, "floor shoelace must be positive, got {area}");
    }
}

// ---------------------------------------------------------------------------
// Generated blockmap covers the same grid as the WAD's and every linedef.
// ---------------------------------------------------------------------------

#[test]
fn e1m1_generated_blockmap_coverage() {
    let mut map = load_map(&doom1_wad_path(), "E1M1");
    let wad_bm = map.blockmap();
    let (wad_cols, wad_rows) = (wad_bm.columns, wad_bm.rows);
    assert!(
        wad_cols > 0 && wad_rows > 0,
        "WAD blockmap should exist for E1M1"
    );

    map.build_blockmap("E1M1");
    let gen_bm = map.blockmap();
    assert_eq!(
        gen_bm.columns, wad_cols,
        "generated columns should match WAD"
    );
    assert_eq!(gen_bm.rows, wad_rows, "generated rows should match WAD");
    assert!(
        !gen_bm.block_lines.is_empty(),
        "generated blockmap should have line refs"
    );

    let num_lines = map.linedefs.len();
    let mut line_found = vec![false; num_lines];
    for i in 0..gen_bm.block_offsets.len() - 1 {
        for j in gen_bm.block_offsets[i]..gen_bm.block_offsets[i + 1] {
            let ld_num = gen_bm.block_lines[j].num;
            if ld_num < num_lines {
                line_found[ld_num] = true;
            }
        }
    }
    let missing: Vec<usize> = line_found
        .iter()
        .enumerate()
        .filter(|(_, found)| !**found)
        .map(|(i, _)| i)
        .collect();
    assert!(
        missing.is_empty(),
        "every linedef should appear in a blockmap cell; missing: {:?}",
        &missing[..missing.len().min(10)]
    );
}

// ---------------------------------------------------------------------------
// Triangulation: the fan triangulation must cover every polygon exactly, index
// valid global vertices, reproduce each polygon's fan, and leave the existing
// n-poly output untouched. Movers must not invalidate the triangle topology.
// ---------------------------------------------------------------------------

#[test]
fn e1m1_triangulation_covers_polygons() {
    let map = load_map(&doom1_wad_path(), "E1M1");
    let bsp3d = &map.bsp_3d;

    assert!(!bsp3d.polygons.is_empty(), "E1M1 produced no polygons");

    // Triangle count == sum(n-2) over all polygons.
    let expected: usize = bsp3d
        .poly_vertex_range
        .iter()
        .map(|&(s, e)| (e - s).saturating_sub(2))
        .sum();
    assert_eq!(
        bsp3d.triangles.len(),
        expected,
        "triangle count must equal sum of (verts-2) over polygons"
    );

    let nverts = bsp3d.vertices.len();
    let mut cursor = 0usize;
    for poly_idx in 0..bsp3d.polygons.len() {
        let poly_verts = bsp3d.poly_vert_indices(poly_idx);
        if poly_verts.len() < 3 {
            continue;
        }
        let v0 = poly_verts[0];
        for i in 1..poly_verts.len() - 1 {
            // The triangle list is u32 (GPU index buffer); widen to compare.
            let tri = bsp3d.triangles[cursor].map(|v| v as usize);
            // Reproduces the fan (v0, vi, vi+1) in global vertex indices.
            assert_eq!(
                tri,
                [v0, poly_verts[i], poly_verts[i + 1]],
                "triangle {cursor} is not the expected fan for polygon {poly_idx}"
            );
            // Indices are valid global vertices.
            for &vi in &tri {
                assert!(vi < nverts, "triangle index {vi} out of range ({nverts})");
            }
            cursor += 1;
        }
    }
    assert_eq!(cursor, bsp3d.triangles.len(), "not all triangles consumed");
}

#[test]
fn e1m1_triangulation_survives_movers() {
    let mut map = load_map(&doom1_wad_path(), "E1M1");

    // A mover sector: pick the first polygon flagged as moving and its sector.
    let Some(gi) = (0..map.bsp_3d.polygons.len())
        .find(|&gi| map.bsp_3d.poly_flags[gi].contains(PolyFlags::MOVES))
    else {
        // E1M1 always has movers (doors/lifts); fail loudly if not.
        panic!("E1M1 has no mover polygons");
    };
    let sector_id = map.bsp_3d.polygons[gi].sector.num as usize;

    let triangles_before = map.bsp_3d.triangles.clone();

    move_sector_surface(&mut map, sector_id, MovementType::Floor, -64.0);

    // Topology is stable: index buffer unchanged across the move.
    let bsp3d = &map.bsp_3d;
    assert_eq!(
        bsp3d.triangles, triangles_before,
        "move_surface must not change triangle indices"
    );

    // The triangles still index valid, moved vertices (z changed for the sector).
    let nverts = bsp3d.vertices.len();
    for tri in &bsp3d.triangles {
        for &vi in tri {
            assert!(
                (vi as usize) < nverts,
                "triangle index {vi} out of range after move"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// wgpu3d fans per-corner UV from poly_vertex_uv at upload. This guards the fan
// against drift: it must reproduce the (v0, vi, vi+1) fan in triangles order,
// byte-for-byte, and stay stable across a no-op mover (proving renderer input
// data did not change).
// ---------------------------------------------------------------------------

#[test]
fn e1m1_fan_corner_uv_aligns_with_triangles() {
    let map = load_map(&doom1_wad_path(), "E1M1");
    let bsp3d = &map.bsp_3d;

    let mut fanned = Vec::new();
    bsp3d.fan_corner_uv(&mut fanned);

    // One UV per triangle corner, in triangles order.
    assert_eq!(
        fanned.len(),
        bsp3d.triangles.len() * 3,
        "fan_corner_uv must emit 3 UVs per triangle"
    );

    // Each corner UV must be the poly_vertex_uv of the vertex that triangle
    // corner references — verified independently against the triangle's global
    // vertex indices, not by replaying the same fan loop.
    let mut corner = 0usize;
    for poly_idx in 0..bsp3d.polygons.len() {
        let (start, end) = bsp3d.poly_vertex_range[poly_idx];
        let n = end - start;
        if n < 3 {
            continue;
        }
        let poly_verts = bsp3d.poly_vert_indices(poly_idx);
        for i in 1..n - 1 {
            let tri = bsp3d.triangles[corner / 3].map(|v| v as usize);
            // Triangle's global vertices fan as (v0, vi, vi+1).
            assert_eq!(
                tri,
                [poly_verts[0], poly_verts[i], poly_verts[i + 1]],
                "triangle {} not the expected fan for polygon {poly_idx}",
                corner / 3
            );
            // Fanned UV at each corner == that vertex's poly_vertex_uv slot.
            assert_eq!(
                fanned[corner], bsp3d.poly_vertex_uv[start],
                "corner {corner} UV"
            );
            assert_eq!(fanned[corner + 1], bsp3d.poly_vertex_uv[start + i]);
            assert_eq!(fanned[corner + 2], bsp3d.poly_vertex_uv[start + i + 1]);
            corner += 3;
        }
    }
    assert_eq!(corner, fanned.len(), "not all corners covered");
}

#[test]
fn e1m1_fan_corner_uv_stable_across_noop_move() {
    let mut map = load_map(&doom1_wad_path(), "E1M1");
    let sector_id = map
        .bsp_3d
        .sector_wall_polys
        .iter()
        .position(|w| !w.is_empty())
        .expect("E1M1 has a sector with walls");
    let h = map.sectors[sector_id].floorheight.to_f32();
    let bsp3d = &mut map.bsp_3d;

    let mut before = Vec::new();
    bsp3d.fan_corner_uv(&mut before);
    bsp3d.move_surface(sector_id, MovementType::Floor, h);
    let mut after = Vec::new();
    bsp3d.fan_corner_uv(&mut after);

    assert_eq!(before, after, "no-op move changed the fanned corner UV");
}

#[test]
fn e1m1_move_to_same_height_keeps_wall_uv() {
    let mut map = load_map(&doom1_wad_path(), "E1M1");
    // Moving a sector's floor to its current height reproduces the baked UV.
    let sector_id = map
        .bsp_3d
        .sector_wall_polys
        .iter()
        .position(|w| !w.is_empty())
        .expect("E1M1 has a sector with walls");
    let h = map.sectors[sector_id].floorheight.to_f32();
    let bsp3d = &mut map.bsp_3d;
    let before = bsp3d.poly_vertex_uv.clone();
    bsp3d.move_surface(sector_id, MovementType::Floor, h);
    assert_eq!(before.len(), bsp3d.poly_vertex_uv.len(), "UV count changed");
    for (i, (a, b)) in before.iter().zip(&bsp3d.poly_vertex_uv).enumerate() {
        assert_eq!(a, b, "vertex {i} UV changed by no-op move");
    }
}

#[test]
fn e1m1_sector_wall_polys_are_vertical() {
    let map = load_map(&doom1_wad_path(), "E1M1");
    let bsp3d = &map.bsp_3d;
    assert_eq!(
        bsp3d.sector_wall_polys.len(),
        bsp3d.sector_leaves.len(),
        "sector_wall_polys must be parallel to sectors"
    );
    let mut total = 0;
    for walls in &bsp3d.sector_wall_polys {
        for &gi in walls {
            assert!(
                !bsp3d.poly_is_flat(gi),
                "sector_wall_polys must only list wall polygons"
            );
            total += 1;
        }
    }
    assert!(total > 0, "E1M1 should have wall polygons");
}

#[test]
fn light_band_matches_doom_rules() {
    use glam::Vec3;
    use level::{LIGHT_LEVELS, contrast_adjust, light_band};

    let floor = Vec3::Z; // horizontal: no contrast
    let east = Vec3::X; // E/W wall: +1
    let north = Vec3::Y; // N/S wall: -1

    assert_eq!(contrast_adjust(floor), 0);
    assert_eq!(contrast_adjust(east), 1);
    assert_eq!(contrast_adjust(north), -1);

    // Band = (lightlevel>>4 + extralight) capped at 15, then contrast clamped.
    assert_eq!(light_band(160, 0, floor), 160 >> 4); // 10
    assert_eq!(light_band(160, 0, east), (160 >> 4) + 1); // 11
    assert_eq!(light_band(160, 0, north), (160 >> 4) - 1); // 9
    // Extralight before the cap; full bright stays clamped.
    assert_eq!(light_band(255, 0, floor), LIGHT_LEVELS); // 15
    assert_eq!(light_band(255, 4, floor), LIGHT_LEVELS); // capped
    assert_eq!(light_band(160, 8, floor), LIGHT_LEVELS); // 10+8 -> 15
    // Darkness floor.
    assert_eq!(light_band(0, 0, north), 0);
}

#[cfg_attr(not(feature = "wad-sunder"), ignore = "needs sunder.wad (~/doom/)")]
#[test]
fn sunder_map20_generated_blockmap() {
    use test_utils::sunder_wad_path;
    let map = load_map(&sunder_wad_path(), "MAP20");
    let bm = map.blockmap();
    assert!(
        bm.columns > 0 && bm.rows > 0,
        "blockmap should have valid dimensions"
    );
    assert!(!bm.block_lines.is_empty(), "blockmap should have line refs");
    let total = bm.columns * bm.rows;
    assert!(
        total > 1000,
        "MAP20 blockmap should be large, got {}x{}",
        bm.columns,
        bm.rows
    );
}
