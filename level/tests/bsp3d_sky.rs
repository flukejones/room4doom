//! BSP3D sky-ceiling handling on Eviternity MAP04 (F_SKY1 ceilings): sky
//! sectors get no ceiling polygons but do get floors, and upper walls are
//! skipped only between two sky sectors.
//!
//! All gated on `wad-eviternity` (also needs doom2.wad as the base IWAD).

#![cfg(feature = "wad-eviternity")]

use std::collections::HashSet;

use level::{SurfaceKind, WallType};
use test_utils::{doom2_wad_path, eviternity_wad_path, load_map_with_flats};
use wad::types::WadSector;
use wad::{MapLump, WadData};

fn load_map04() -> (WadData, level::LevelData, usize) {
    let mut wad = WadData::new(&doom2_wad_path());
    wad.add_file(eviternity_wad_path());
    let (map, sky_num) = load_map_with_flats(&wad, "MAP04");
    let sky_num = sky_num.expect("F_SKY1 flat not found in doom2 + eviternity");
    (wad, map, sky_num)
}

/// Sectors with `ceilingpic == sky_num`.
fn sky_sector_ids(map: &level::LevelData, sky_num: usize) -> Vec<usize> {
    map.sectors
        .iter()
        .enumerate()
        .filter(|(_, s)| s.ceilingpic == sky_num)
        .map(|(i, _)| i)
        .collect()
}

#[test]
fn map04_no_sky_ceiling_polygons() {
    let (_wad, map, sky_num) = load_map04();
    let bsp3d = &map.bsp_3d;
    let sky_ids = sky_sector_ids(&map, sky_num);
    assert!(
        sky_ids.len() >= 200,
        "expected 200+ sky ceiling sectors, got {}",
        sky_ids.len()
    );

    let violations: Vec<usize> = bsp3d
        .subsector_leaves
        .iter()
        .flat_map(|leaf| &leaf.ceiling_polygons)
        .filter(|&&ci| sky_ids.contains(&bsp3d.polygons[ci].sector_id))
        .copied()
        .collect();
    assert!(
        violations.is_empty(),
        "{} ceiling polygons in sky sectors",
        violations.len()
    );
}

#[test]
fn map04_sky_sectors_have_floors() {
    let (_wad, map, sky_num) = load_map04();
    let bsp3d = &map.bsp_3d;
    let sky_ids = sky_sector_ids(&map, sky_num);

    let with_floors = bsp3d
        .subsector_leaves
        .iter()
        .flat_map(|leaf| &leaf.floor_polygons)
        .filter(|&&fi| sky_ids.contains(&bsp3d.polygons[fi].sector_id))
        .count();
    assert!(with_floors > 0, "sky sectors should have floor polygons");
}

#[test]
fn map04_sky_flat_index_matches() {
    let (wad, map, sky_num) = load_map04();
    let wad_sectors: Vec<_> = wad
        .map_iter::<WadSector>("MAP04", MapLump::Sectors)
        .collect();

    let mut f_sky1 = 0;
    let mut matching = 0;
    for (i, ws) in wad_sectors.iter().enumerate() {
        if ws.ceil_tex == "F_SKY1" {
            f_sky1 += 1;
            if map.sectors[i].ceilingpic == sky_num {
                matching += 1;
            }
        }
    }
    assert!(f_sky1 > 0, "should have F_SKY1 sectors");
    assert_eq!(
        f_sky1, matching,
        "all F_SKY1 sectors should have ceilingpic == sky_num ({sky_num})"
    );
}

#[test]
fn map04_no_sky_textured_ceiling_polygons() {
    let (_wad, map, sky_num) = load_map04();
    let bsp3d = &map.bsp_3d;

    let violations: Vec<usize> = bsp3d
        .subsector_leaves
        .iter()
        .flat_map(|leaf| &leaf.ceiling_polygons)
        .filter(|&&ci| {
            matches!(
                bsp3d.polygons[ci].surface_kind,
                SurfaceKind::Horizontal { texture, .. } if texture == sky_num
            )
        })
        .copied()
        .collect();
    assert!(
        violations.is_empty(),
        "{} ceiling polygons with sky texture",
        violations.len()
    );
}

#[test]
fn map04_linedef1572_sky_wall() {
    // One-sided wall in sky sector 363 (OSKY28 middle) — should produce a wall.
    let (_wad, map, _sky_num) = load_map04();
    let found = wall_exists(&map.bsp_3d, 1572, None);
    assert!(found, "ld1572 (sky wall) should produce a wall polygon");
}

#[test]
fn map04_linedef1581_no_upper_wall() {
    // Two-sided, both sectors F_SKY1 — upper wall skipped.
    let (_wad, map, _sky_num) = load_map04();
    let found = wall_exists(&map.bsp_3d, 1581, Some(WallType::Upper));
    assert!(
        !found,
        "ld1581 (both sides F_SKY1) should have NO upper wall"
    );
}

#[test]
fn map04_linedef1351_has_upper_wall() {
    // Two-sided, one side F_SKY1 — upper wall exists.
    let (_wad, map, _sky_num) = load_map04();
    let found = wall_exists(&map.bsp_3d, 1351, Some(WallType::Upper));
    assert!(
        found,
        "ld1351 (sky/non-sky border) should have an upper wall"
    );
}

#[test]
fn map04_no_upper_walls_between_sky_sectors() {
    let (_wad, map, sky_num) = load_map04();
    let bsp3d = &map.bsp_3d;

    let both_sky: HashSet<usize> = map
        .linedefs
        .iter()
        .enumerate()
        .filter(|(_, ld)| {
            ld.backsector
                .as_ref()
                .is_some_and(|b| ld.frontsector.ceilingpic == sky_num && b.ceilingpic == sky_num)
        })
        .map(|(i, _)| i)
        .collect();

    let violations: Vec<usize> = bsp3d
        .subsector_leaves
        .iter()
        .flat_map(|leaf| &leaf.polygon_indices)
        .filter_map(|&gi| match &bsp3d.polygons[gi].surface_kind {
            SurfaceKind::Vertical {
                linedef_id,
                wall_type: WallType::Upper,
                ..
            } if both_sky.contains(linedef_id) => Some(*linedef_id),
            _ => None,
        })
        .collect();
    assert!(
        violations.is_empty(),
        "{} upper walls between sky-sky sectors",
        violations.len()
    );
}

/// Whether any polygon on `ld` exists (optionally filtered to `wall_type`).
fn wall_exists(bsp3d: &level::BSP3D, ld: usize, wall_type: Option<WallType>) -> bool {
    bsp3d.subsector_leaves.iter().any(|leaf| {
        leaf.polygon_indices
            .iter()
            .any(|&gi| match &bsp3d.polygons[gi].surface_kind {
                SurfaceKind::Vertical {
                    linedef_id,
                    wall_type: wt,
                    ..
                } if *linedef_id == ld => wall_type.is_none_or(|want| *wt == want),
                _ => false,
            })
    })
}
