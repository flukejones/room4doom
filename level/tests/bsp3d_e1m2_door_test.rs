use level::{MovementType, SurfaceKind, WallType};
use test_utils::{doom_wad_path, load_map};

/// Verify that all sector 129 (door) ceiling vertices move when the
/// ceiling is raised. Regression test for segment-edge matching fix
/// where a clipped polygon vertex drifted ~1.6 units from the correct
/// segment endpoint, breaking zero-height wall vertex separation.
#[test]
fn test_e1m2_sector129_door_ceiling_moves() {
    let mut map = load_map(&doom_wad_path(), "E1M2");

    let bsp3d = &mut map.bsp_3d;
    let initial_positions: Vec<_> = bsp3d.vertices.iter().copied().collect();

    // Move sector 129 ceiling from 0 to 128
    bsp3d.move_surface(129, MovementType::Ceiling, 128.0, 0);

    // Every ceiling vertex in sector 129 must have moved
    let ss_ids = bsp3d.sector_subsectors[129].clone();
    let mut stuck_count = 0;
    for &ssid in &ss_ids {
        let leaf = &bsp3d.subsector_leaves[ssid];
        for &ceil_idx in &leaf.ceiling_polygons {
            let poly = &leaf.polygons[ceil_idx];
            for &vidx in &poly.vertices {
                let orig = initial_positions[vidx];
                let curr = bsp3d.vertices[vidx];
                if (orig.z - curr.z).abs() <= 0.001 {
                    stuck_count += 1;
                }
            }
        }
    }

    assert_eq!(
        stuck_count, 0,
        "All sector 129 ceiling vertices should move"
    );
}

/// Verify that ALL upper-wall bottom vertices along the sector 129 (door)
/// boundary share indices with sector 129's ceiling polygons — static check
/// that the triangulation tagged them correctly before any movement occurs.
#[test]
fn test_e1m2_all_mover_vertex_sharing() {
    use std::collections::HashSet;

    let map = load_map(&doom_wad_path(), "E1M2");

    let bsp3d = &map.bsp_3d;
    let verts = &bsp3d.vertices;

    // Sector 129: ceiling mover (door).
    let ceil_verts: HashSet<usize> = bsp3d.sector_subsectors[129]
        .iter()
        .flat_map(|&ssid| {
            let leaf = &bsp3d.subsector_leaves[ssid];
            leaf.ceiling_polygons
                .iter()
                .flat_map(|&cpi| leaf.polygons[cpi].vertices.iter().copied())
                .collect::<Vec<_>>()
        })
        .collect();

    let ssid = bsp3d.sector_subsectors[129][0];
    let leaf = &bsp3d.subsector_leaves[ssid];
    let ceil_h = verts[leaf.polygons[leaf.ceiling_polygons[0]].vertices[0]].z;

    let border_lds: HashSet<usize> = map
        .segments
        .iter()
        .filter(|s| {
            s.frontsector.num == 129 || s.backsector.as_ref().map_or(false, |b| b.num == 129)
        })
        .map(|s| s.linedef.num as usize)
        .collect();

    // Upper-wall bottom vertices at ceil_h must share indices with sector
    // 129's ceiling polygons so move_surface propagates to those walls.
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
        "Sector 129 ceiling mover: upper wall bottom vertex indices {:?} not shared with ceiling polygons",
        unshared
    );
}
