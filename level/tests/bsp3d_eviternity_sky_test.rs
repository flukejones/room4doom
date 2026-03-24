use std::collections::HashSet;

use level::{SurfaceKind, WallType};
use test_utils::{doom2_wad_path, eviternity_wad_path, load_map_with_flats};
use wad::types::WadSector;
use wad::{MapLump, WadData};

fn load_eviternity(map_name: &str) -> (WadData, level::LevelData, usize) {
    let mut wad = WadData::new(&doom2_wad_path());
    wad.add_file(eviternity_wad_path());
    let (map, sky_num) = load_map_with_flats(&wad, map_name);
    let sky_num = sky_num.expect("F_SKY1 flat not found in doom2 + eviternity");
    (wad, map, sky_num)
}

/// Verify that BSP3D does NOT create ceiling polygons for sectors with F_SKY1
/// ceiling.
#[test]
#[ignore = "Eviternity.wad can't be included in git"]
fn test_eviternity_map04_no_sky_ceiling_polygons() {
    let (_wad, map, sky_num) = load_eviternity("MAP04");
    let bsp3d = &map.bsp_3d;

    let sky_sector_ids: Vec<usize> = map
        .sectors
        .iter()
        .enumerate()
        .filter(|(_, s)| s.ceilingpic == sky_num)
        .map(|(i, _)| i)
        .collect();
    assert!(
        sky_sector_ids.len() >= 200,
        "Expected 200+ sky ceiling sectors, got {}",
        sky_sector_ids.len()
    );

    let mut violations = Vec::new();
    for (ss_id, leaf) in bsp3d.subsector_leaves.iter().enumerate() {
        for &ci in &leaf.ceiling_polygons {
            let poly = &leaf.polygons[ci];
            if sky_sector_ids.contains(&poly.sector_id) {
                let tex = match &poly.surface_kind {
                    SurfaceKind::Horizontal {
                        texture,
                        ..
                    } => *texture,
                    _ => usize::MAX,
                };
                violations.push((ss_id, poly.sector_id, tex));
            }
        }
    }

    if !violations.is_empty() {
        let sample: Vec<_> = violations.iter().take(10).collect();
        panic!(
            "{} ceiling polygons found in sky sectors (should be 0).\nFirst 10: {:?}",
            violations.len(),
            sample
        );
    }
}

/// Verify sky-ceiling sectors DO have floor polygons.
#[test]
#[ignore = "Eviternity.wad can't be included in git"]
fn test_eviternity_map04_sky_sectors_have_floors() {
    let (_wad, map, sky_num) = load_eviternity("MAP04");
    let bsp3d = &map.bsp_3d;

    let sky_sector_ids: Vec<usize> = map
        .sectors
        .iter()
        .enumerate()
        .filter(|(_, s)| s.ceilingpic == sky_num)
        .map(|(i, _)| i)
        .collect();

    let mut sky_sectors_with_floors = 0;
    for leaf in &bsp3d.subsector_leaves {
        for &fi in &leaf.floor_polygons {
            let poly = &leaf.polygons[fi];
            if sky_sector_ids.contains(&poly.sector_id) {
                sky_sectors_with_floors += 1;
            }
        }
    }

    assert!(
        sky_sectors_with_floors > 0,
        "Sky sectors should have floor polygons"
    );
}

/// Verify that ceilingpic matches sky_num for F_SKY1 sectors.
#[test]
#[ignore = "Eviternity.wad can't be included in git"]
fn test_eviternity_map04_sky_flat_index_matches() {
    let (wad, map, sky_num) = load_eviternity("MAP04");

    let wad_sectors: Vec<_> = wad
        .map_iter::<WadSector>("MAP04", MapLump::Sectors)
        .collect();
    let mut f_sky1_count = 0;
    let mut matching_count = 0;

    for (i, ws) in wad_sectors.iter().enumerate() {
        if ws.ceil_tex == "F_SKY1" {
            f_sky1_count += 1;
            if map.sectors[i].ceilingpic == sky_num {
                matching_count += 1;
            }
        }
    }

    assert!(f_sky1_count > 0, "Should have F_SKY1 sectors");
    assert_eq!(
        f_sky1_count, matching_count,
        "All F_SKY1 sectors should have ceilingpic == sky_num ({}). {}/{} match.",
        sky_num, matching_count, f_sky1_count
    );
}

/// Check that no ceiling polygons have sky flat texture in any subsector.
/// If skip_ceil works, no polygon with sky_num texture should exist in
/// ceiling_polygons.
#[test]
#[ignore = "Eviternity.wad can't be included in git"]
fn test_eviternity_map04_no_sky_textured_ceiling_polygons() {
    let (_wad, map, sky_num) = load_eviternity("MAP04");
    let bsp3d = &map.bsp_3d;

    let mut violations = Vec::new();
    for (ss_id, leaf) in bsp3d.subsector_leaves.iter().enumerate() {
        for &ci in &leaf.ceiling_polygons {
            let poly = &leaf.polygons[ci];
            if let SurfaceKind::Horizontal {
                texture,
                ..
            } = &poly.surface_kind
            {
                if *texture == sky_num {
                    violations.push((ss_id, poly.sector_id, *texture));
                }
            }
        }
    }

    if !violations.is_empty() {
        let sample: Vec<_> = violations.iter().take(10).collect();
        panic!(
            "{} ceiling polygons with sky texture found (should be 0).\nSamples: {:?}",
            violations.len(),
            sample
        );
    }
}

/// Linedef 1572: one-sided wall in sky sector 363 with OSKY28 middle texture.
/// Should produce a wall polygon (the sky wall).
#[test]
#[ignore = "Eviternity.wad can't be included in git"]
fn test_eviternity_map04_linedef1572_sky_wall() {
    let (_wad, map, _sky_num) = load_eviternity("MAP04");
    let bsp3d = &map.bsp_3d;

    let mut found_wall = false;
    for leaf in &bsp3d.subsector_leaves {
        for poly in &leaf.polygons {
            if let SurfaceKind::Vertical {
                linedef_id,
                ..
            } = &poly.surface_kind
            {
                if *linedef_id == 1572 {
                    found_wall = true;
                }
            }
        }
    }

    assert!(
        found_wall,
        "Linedef 1572 (sky wall with OSKY28) should produce a wall polygon"
    );
}

/// Linedef 1581: two-sided, both sectors (363 & 1051) have F_SKY1 ceiling.
/// Upper wall should be SKIPPED (both_sky_ceil = true).
#[test]
#[ignore = "Eviternity.wad can't be included in git"]
fn test_eviternity_map04_linedef1581_no_upper_wall() {
    let (_wad, map, _sky_num) = load_eviternity("MAP04");
    let bsp3d = &map.bsp_3d;

    let mut upper_walls_for_1581 = Vec::new();
    for (ss_id, leaf) in bsp3d.subsector_leaves.iter().enumerate() {
        for poly in &leaf.polygons {
            if let SurfaceKind::Vertical {
                linedef_id,
                wall_type,
                ..
            } = &poly.surface_kind
            {
                if *linedef_id == 1581 && *wall_type == WallType::Upper {
                    upper_walls_for_1581.push(ss_id);
                }
            }
        }
    }

    assert!(
        upper_walls_for_1581.is_empty(),
        "Linedef 1581 (both sides F_SKY1) should have NO upper wall, found in subsectors: {:?}",
        upper_walls_for_1581
    );
}

/// Linedef 1351: sector 1002 (F_SKY1) borders sector 320 (OROCKD01 ceiling).
/// Upper wall SHOULD exist with texture OROCKQ02 since only one side is sky.
#[test]
#[ignore = "Eviternity.wad can't be included in git"]
fn test_eviternity_map04_linedef1351_has_upper_wall() {
    let (_wad, map, _sky_num) = load_eviternity("MAP04");
    let bsp3d = &map.bsp_3d;

    let mut found_upper = false;
    for leaf in &bsp3d.subsector_leaves {
        for poly in &leaf.polygons {
            if let SurfaceKind::Vertical {
                linedef_id,
                wall_type,
                ..
            } = &poly.surface_kind
            {
                if *linedef_id == 1351 && *wall_type == WallType::Upper {
                    found_upper = true;
                }
            }
        }
    }

    assert!(
        found_upper,
        "Linedef 1351 (sky/non-sky border) should have an upper wall with OROCKQ02"
    );
}

/// Comprehensive: for ALL two-sided linedefs where both sectors have F_SKY1
/// ceiling, there should be NO upper wall polygons.
#[test]
#[ignore = "Eviternity.wad can't be included in git"]
fn test_eviternity_map04_no_upper_walls_between_sky_sectors() {
    let (_wad, map, sky_num) = load_eviternity("MAP04");
    let bsp3d = &map.bsp_3d;

    // Build set of linedefs where both sides have sky ceiling
    let mut both_sky_linedefs = HashSet::new();
    for (i, ld) in map.linedefs.iter().enumerate() {
        if let Some(ref back) = ld.backsector {
            if ld.frontsector.ceilingpic == sky_num && back.ceilingpic == sky_num {
                both_sky_linedefs.insert(i);
            }
        }
    }

    let mut violations = Vec::new();
    for (ss_id, leaf) in bsp3d.subsector_leaves.iter().enumerate() {
        for poly in &leaf.polygons {
            if let SurfaceKind::Vertical {
                linedef_id,
                wall_type,
                ..
            } = &poly.surface_kind
            {
                if *wall_type == WallType::Upper && both_sky_linedefs.contains(linedef_id) {
                    violations.push((ss_id, *linedef_id));
                }
            }
        }
    }

    if !violations.is_empty() {
        let sample: Vec<_> = violations.iter().take(10).collect();
        panic!(
            "{} upper wall polygons between sky-sky sectors (should be 0).\nSamples: {:?}",
            violations.len(),
            sample
        );
    }
}
