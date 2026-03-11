#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::{MapData, PicData, SurfaceKind, WallType};
    use wad::WadData;

    /// Diagnose vertex sharing at linedef 373 boundary between sector 14
    /// (floor=32, mover) and sector 15 (floor=0). The lower wall top vertices
    /// must share indices with sector 14's floor polygon vertices so that
    /// move_surface moves the wall top when the floor moves.
    #[test]
    fn test_e1m1_linedef373_vertex_sharing() {
        let wad = WadData::new(&PathBuf::from("/Users/lukejones/DOOM/doom.wad"));
        let pic_data = PicData::init(&wad);
        let mut map = MapData::default();
        map.load("E1M1", |name| pic_data.flat_num_for_name(name), &wad);

        let bsp3d = &map.bsp_3d;
        let vertices = &bsp3d.vertices;

        // ss=26 is sector 14, ss=37 is sector 15
        println!("=== Linedef 373 vertex sharing diagnostic ===");

        // Find all wall polygons for linedef 373
        let mut wall_vertex_indices = Vec::new();
        for (ssid, leaf) in bsp3d.subsector_leaves.iter().enumerate() {
            for poly in &leaf.polygons {
                if let SurfaceKind::Vertical {
                    wall_type,
                    linedef_id,
                    ..
                } = &poly.surface_kind
                {
                    if *linedef_id == 373 && matches!(wall_type, WallType::Lower) {
                        println!(
                            "Lower wall for ld=373 in ss={}: vertices={:?}",
                            ssid, poly.vertices
                        );
                        for &vi in &poly.vertices {
                            println!("  vi={} pos={:?}", vi, vertices[vi]);
                            wall_vertex_indices.push(vi);
                        }
                    }
                }
            }
        }

        // Find floor polygon vertices for sector 14 subsectors
        let sector_14_subsectors = &bsp3d.sector_subsectors[14];
        println!("\nSector 14 subsectors: {:?}", sector_14_subsectors);

        let mut floor_vertex_indices = std::collections::HashSet::new();
        for &ssid in sector_14_subsectors {
            let leaf = &bsp3d.subsector_leaves[ssid];
            for &fp_idx in &leaf.floor_polygons {
                let poly = &leaf.polygons[fp_idx];
                for &vi in &poly.vertices {
                    floor_vertex_indices.insert(vi);
                }
            }
        }

        println!(
            "\nSector 14 floor vertex indices: {:?}",
            floor_vertex_indices
        );

        // Check which wall vertices are shared with floor
        let mut unshared = Vec::new();
        for &wvi in &wall_vertex_indices {
            let pos = vertices[wvi];
            // Wall top vertices are at sector 14's floor height (32)
            if (pos.z - 32.0).abs() < 1.0 {
                let shared = floor_vertex_indices.contains(&wvi);
                println!(
                    "Wall top vi={} pos={:?} shared_with_floor={}",
                    wvi, pos, shared
                );
                if !shared {
                    unshared.push(wvi);
                }
            }
        }

        // Print subsector 26 segments to see what boundary info is available
        println!("\n--- Subsector 26 segments ---");
        let ss26 = &map.subsectors[26];
        let start = ss26.start_seg as usize;
        let end = start + ss26.seg_count as usize;
        for i in start..end {
            let seg = &map.segments[i];
            println!(
                "  seg[{}]: v1={:?} v2={:?} linedef={} frontsec={} backsec={:?}",
                i,
                *seg.v1,
                *seg.v2,
                seg.linedef.num,
                seg.frontsector.num,
                seg.backsector.as_ref().map(|s| s.num)
            );
        }

        // Print floor polygon 2D positions
        println!("\n--- Sector 14 floor polygon vertices (2D) ---");
        for &ssid in sector_14_subsectors {
            let leaf = &bsp3d.subsector_leaves[ssid];
            for &fp_idx in &leaf.floor_polygons {
                let poly = &leaf.polygons[fp_idx];
                println!(
                    "  ss={} fp={}: {:?}",
                    ssid,
                    fp_idx,
                    poly.vertices
                        .iter()
                        .map(|&vi| {
                            let v = vertices[vi];
                            format!("{}=({:.1},{:.1},{:.1})", vi, v.x, v.y, v.z)
                        })
                        .collect::<Vec<_>>()
                );
            }
        }

        // Print unshared vertex info
        println!("\n--- Unshared wall top vertices ---");
        for &wvi in &unshared {
            let wpos = vertices[wvi];
            // Check if there's a floor vertex at the same x,y but different index
            let nearby_floor = floor_vertex_indices
                .iter()
                .filter(|&&fvi| {
                    let fpos = vertices[fvi];
                    (fpos.x - wpos.x).abs() < 1.0 && (fpos.y - wpos.y).abs() < 1.0
                })
                .collect::<Vec<_>>();
            println!(
                "  wall vi={} pos=({:.1},{:.1},{:.1}) nearby_floor={:?}",
                wvi, wpos.x, wpos.y, wpos.z, nearby_floor
            );
        }

        assert!(
            unshared.is_empty(),
            "Wall top vertices {:?} for linedef 373 are NOT shared with sector 14 floor polygons — mover will not update them",
            unshared
        );
    }

    /// Verify that ALL wall vertices at the mover boundary are correctly tagged
    /// (share indices with the mover sector's floor/ceiling polygon) for every
    /// linedef bordering the sector — not just a single linedef.
    ///
    /// Covers:
    ///   - Sector 14: floor mover (floor=32). Every lower-wall top vertex along
    ///     the sector 14 boundary must share an index with sector 14's floor
    ///     polygons.
    ///   - Sector 26: ceiling mover (door, ceiling=0). Every upper-wall bottom
    ///     vertex along the sector 26 boundary must share an index with sector
    ///     26's ceiling polygons.
    #[test]
    fn test_e1m1_all_mover_vertex_sharing() {
        use std::collections::HashSet;

        let wad = WadData::new(&PathBuf::from("/Users/lukejones/DOOM/doom.wad"));
        let pic_data = PicData::init(&wad);
        let mut map = MapData::default();
        map.load("E1M1", |name| pic_data.flat_num_for_name(name), &wad);

        let bsp3d = &map.bsp_3d;
        let verts = &bsp3d.vertices;

        // ---- Sector 14: floor mover ----
        {
            // Floor polygon vertex indices for sector 14.
            let floor_verts: HashSet<usize> = bsp3d.sector_subsectors[14]
                .iter()
                .flat_map(|&ssid| {
                    let leaf = &bsp3d.subsector_leaves[ssid];
                    leaf.floor_polygons
                        .iter()
                        .flat_map(|&fpi| leaf.polygons[fpi].vertices.iter().copied())
                        .collect::<Vec<_>>()
                })
                .collect();

            // Derive floor height from the polygon data.
            let ssid = bsp3d.sector_subsectors[14][0];
            let leaf = &bsp3d.subsector_leaves[ssid];
            let floor_h = verts[leaf.polygons[leaf.floor_polygons[0]].vertices[0]].z;

            // Linedef IDs for all segments bordering sector 14.
            let border_lds: HashSet<usize> = map
                .segments
                .iter()
                .filter(|s| {
                    s.frontsector.num == 14 || s.backsector.as_ref().map_or(false, |b| b.num == 14)
                })
                .map(|s| s.linedef.num as usize)
                .collect();

            // Non-zh lower wall vertices at floor_h must be in floor_verts.
            // Zh lower walls (all verts at same z) are excluded — their top
            // vertices correctly connect to the adjacent non-mover sector.
            let mut unshared = Vec::new();
            for leaf in &bsp3d.subsector_leaves {
                for poly in &leaf.polygons {
                    if let SurfaceKind::Vertical {
                        wall_type,
                        linedef_id,
                        ..
                    } = &poly.surface_kind
                    {
                        if border_lds.contains(linedef_id) && matches!(wall_type, WallType::Lower) {
                            let all_same_z = poly
                                .vertices
                                .iter()
                                .all(|&vi| (verts[vi].z - verts[poly.vertices[0]].z).abs() < 1.0);
                            if all_same_z {
                                continue;
                            }
                            for &vi in &poly.vertices {
                                if (verts[vi].z - floor_h).abs() < 1.0 && !floor_verts.contains(&vi)
                                {
                                    unshared.push(vi);
                                }
                            }
                        }
                    }
                }
            }

            assert!(
                unshared.is_empty(),
                "Sector 14 floor mover: lower wall top vertex indices {:?} not shared with floor polygons",
                unshared
            );
        }

        // ---- Sector 26: ceiling mover (door) ----
        {
            let ceil_verts: HashSet<usize> = bsp3d.sector_subsectors[26]
                .iter()
                .flat_map(|&ssid| {
                    let leaf = &bsp3d.subsector_leaves[ssid];
                    leaf.ceiling_polygons
                        .iter()
                        .flat_map(|&cpi| leaf.polygons[cpi].vertices.iter().copied())
                        .collect::<Vec<_>>()
                })
                .collect();

            let ssid = bsp3d.sector_subsectors[26][0];
            let leaf = &bsp3d.subsector_leaves[ssid];
            let ceil_h = verts[leaf.polygons[leaf.ceiling_polygons[0]].vertices[0]].z;

            let border_lds: HashSet<usize> = map
                .segments
                .iter()
                .filter(|s| {
                    s.frontsector.num == 26 || s.backsector.as_ref().map_or(false, |b| b.num == 26)
                })
                .map(|s| s.linedef.num as usize)
                .collect();

            // Non-zh upper wall vertices at ceil_h must be in ceil_verts.
            // Zh upper walls (all verts at same z) are excluded — their bottom
            // vertices correctly connect to the adjacent non-mover sector.
            let mut unshared = Vec::new();
            for leaf in &bsp3d.subsector_leaves {
                for poly in &leaf.polygons {
                    if let SurfaceKind::Vertical {
                        wall_type,
                        linedef_id,
                        ..
                    } = &poly.surface_kind
                    {
                        if border_lds.contains(linedef_id) && matches!(wall_type, WallType::Upper) {
                            let all_same_z = poly
                                .vertices
                                .iter()
                                .all(|&vi| (verts[vi].z - verts[poly.vertices[0]].z).abs() < 1.0);
                            if all_same_z {
                                continue;
                            }
                            for &vi in &poly.vertices {
                                if (verts[vi].z - ceil_h).abs() < 1.0 && !ceil_verts.contains(&vi) {
                                    unshared.push(vi);
                                }
                            }
                        }
                    }
                }
            }

            assert!(
                unshared.is_empty(),
                "Sector 26 ceiling mover: upper wall bottom vertex indices {:?} not shared with ceiling polygons",
                unshared
            );
        }
    }
}
