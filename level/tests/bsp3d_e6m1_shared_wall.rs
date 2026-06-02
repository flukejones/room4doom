use level::{MovementType, SurfaceKind, WallType};
use test_utils::{doom_wad_path, load_map_with_flats, sigil2_wad_path};
use wad::WadData;

fn load() -> level::LevelData {
    let mut wad = WadData::new(&doom_wad_path());
    wad.add_file(sigil2_wad_path());
    load_map_with_flats(&wad, "E6M1").0
}

/// E6M1 ld1200 borders sector 99 (back) and the floor-mover sector 144 (front).
/// Its lower wall is a single shared polygon registered in BOTH adjacent
/// leaves, so it renders from the pit side after sector 144 drops below sector
/// 99.
#[test]
fn e6m1_ld1200_lower_wall_shared_both_sides() {
    let mut map = load();
    let bsp3d = &mut map.bsp_3d;

    // The lower wall global index, and the sectors of the leaves that reference it.
    let mut wall_gi = None;
    for gi in 0..bsp3d.polygons.len() {
        if let SurfaceKind::Vertical {
            linedef_id: 1200,
            wall_type: WallType::Lower,
            ..
        } = &bsp3d.polygons[gi].surface_kind
        {
            wall_gi = Some(gi);
            break;
        }
    }
    let gi = wall_gi.expect("ld1200 lower wall exists");

    let mut owning_sectors = std::collections::BTreeSet::new();
    for ss in 0..bsp3d.subsector_leaves.len() {
        let leaf = bsp3d.get_subsector_leaf(ss).unwrap();
        if leaf.polygon_indices.contains(&gi) {
            owning_sectors.insert(leaf.sector_id);
        }
    }
    assert!(
        owning_sectors.contains(&99) && owning_sectors.contains(&144),
        "ld1200 lower wall must be referenced by a sector-99 and a sector-144 leaf, got {owning_sectors:?}"
    );

    // After the lift drops, the wall spans the full pit (-20 down to -320).
    bsp3d.move_surface(144, MovementType::Floor, -320.0, 0);
    let (lo, hi) = bsp3d.polygons[gi].vertices.iter().fold(
        (f32::INFINITY, f32::NEG_INFINITY),
        |(lo, hi), &v| {
            let z = bsp3d.vertices[v].z;
            (lo.min(z), hi.max(z))
        },
    );
    assert!(
        (lo - -320.0).abs() < 0.5 && (hi - -20.0).abs() < 0.5,
        "ld1200 lower wall should span -320..-20 after the lift drops, got {lo}..{hi}"
    );
}
