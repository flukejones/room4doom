//! BSP3D mover behaviour: wall-vertex sharing at mover boundaries, door/floor
//! movement, stairs, two-sided face flips, cross-sector drag isolation, leaf
//! AABB coverage, and vanilla-special normalisation.

use glam::Vec2;
use level::env_target::mover_targets_for_sector;
use level::special_encode::{Category, decode, encode_vanilla, is_generalized};
use level::{BSP3D, LevelData, LineDefFlags, MovementType, NO_INDEX, PolyFlags, WallSlot};
use test_utils::{
    Surface, collect_border_linedefs, collect_sector_vertices, doom_wad_path, doom1_wad_path,
    doom2_wad_path, load_map, load_map_with_flats, load_map_with_pwad, move_sector_surface,
    sigil_wad_path, sigil2_wad_path,
};
use wad::WadData;

// ---------------------------------------------------------------------------
// Helpers shared by the mover tests below.
// ---------------------------------------------------------------------------

/// All wall polygons of a linedef (sky fillers excluded).
fn ld_walls(bsp3d: &BSP3D, ld: usize) -> Vec<usize> {
    bsp3d.linedef_wall_polys[ld]
        .iter()
        .copied()
        .filter(|&gi| !bsp3d.poly_flags[gi].contains(PolyFlags::SKY_FILLER))
        .collect()
}

/// Z-range of every wall quad on `ld`, paired with its front sector.
fn wall_z(bsp3d: &BSP3D, ld: usize) -> Vec<(usize, (f32, f32))> {
    ld_walls(bsp3d, ld)
        .into_iter()
        .map(|gi| {
            let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
            for &v in bsp3d.poly_vert_indices(gi) {
                let z = bsp3d.vertices[v].z;
                lo = lo.min(z);
                hi = hi.max(z);
            }
            (bsp3d.polygons[gi].sector.num as usize, (lo, hi))
        })
        .collect()
}

/// Bottom-of-wall vertices on a mover boundary that fail to share a vertex
/// index with the mover sector's surface polygons — i.e. `move_surface` will
/// not drag them. `surface`/`slot` select floor-mover (lower walls) or
/// ceiling-mover (upper walls).
fn unshared_boundary_walls(
    map: &LevelData,
    sector_id: usize,
    surface: Surface,
    slot: WallSlot,
) -> Vec<usize> {
    let bsp3d = &map.bsp_3d;
    let verts = &bsp3d.vertices;
    let surface_verts = collect_sector_vertices(bsp3d, sector_id, surface);
    let border_lds = collect_border_linedefs(map, sector_id);

    let table = match surface {
        Surface::Floor => &bsp3d.sector_floor_polys[sector_id],
        Surface::Ceiling => &bsp3d.sector_ceiling_polys[sector_id],
    };
    let target_z = verts[bsp3d.poly_vert_indices(table[0])[0]].z;

    let mut unshared = Vec::new();
    for &ld in &border_lds {
        for gi in ld_walls(bsp3d, ld) {
            if bsp3d.wall_slot(gi) != Some(slot) {
                continue;
            }
            let poly_verts = bsp3d.poly_vert_indices(gi);
            // A flat (single-Z) wall has no edge crossing the surface.
            let all_same_z = poly_verts
                .iter()
                .all(|&vi| (verts[vi].z - verts[poly_verts[0]].z).abs() < 1.0);
            if all_same_z {
                continue;
            }
            for &vi in poly_verts {
                if (verts[vi].z - target_z).abs() < 1.0 && !surface_verts.contains(&vi) {
                    unshared.push(vi);
                }
            }
        }
    }
    unshared
}

// ---------------------------------------------------------------------------
// E1M1 — floor mover (sector 14) + ceiling mover door (sector 26).
// ---------------------------------------------------------------------------

#[cfg_attr(not(feature = "wad-doom"), ignore = "needs doom.wad (~/doom/)")]
#[test]
fn e1m1_mover_boundary_vertex_sharing() {
    let map = load_map(&doom_wad_path(), "E1M1");
    let floor = unshared_boundary_walls(&map, 14, Surface::Floor, WallSlot::Lower);
    assert!(
        floor.is_empty(),
        "sector 14 floor mover: lower wall top vertices {floor:?} not shared with floor polygons"
    );
    let ceil = unshared_boundary_walls(&map, 26, Surface::Ceiling, WallSlot::Upper);
    assert!(
        ceil.is_empty(),
        "sector 26 ceiling mover: upper wall bottom vertices {ceil:?} not shared with ceiling polygons"
    );
}

// ---------------------------------------------------------------------------
// E1M2 — door ceiling movement + boundary sharing + platform ss267.
// ---------------------------------------------------------------------------

#[cfg_attr(not(feature = "wad-doom"), ignore = "needs doom.wad (~/doom/)")]
#[test]
fn e1m2_sector129_door_ceiling_moves() {
    let mut map = load_map(&doom_wad_path(), "E1M2");
    let initial = map.bsp_3d.vertices.clone();

    move_sector_surface(&mut map, 129, MovementType::Ceiling, 128.0);

    let bsp3d = &map.bsp_3d;
    let mut stuck = 0;
    for &ci in &bsp3d.sector_ceiling_polys[129] {
        for &vi in bsp3d.poly_vert_indices(ci) {
            if (initial[vi].z - bsp3d.vertices[vi].z).abs() <= 0.001 {
                stuck += 1;
            }
        }
    }
    assert_eq!(stuck, 0, "all sector 129 ceiling vertices should move");
}

#[cfg_attr(not(feature = "wad-doom"), ignore = "needs doom.wad (~/doom/)")]
#[test]
fn e1m2_sector129_door_boundary_vertex_sharing() {
    let map = load_map(&doom_wad_path(), "E1M2");
    let unshared = unshared_boundary_walls(&map, 129, Surface::Ceiling, WallSlot::Upper);
    assert!(
        unshared.is_empty(),
        "sector 129 ceiling mover: upper wall bottom vertices {unshared:?} not shared with ceiling polygons"
    );
}

#[cfg_attr(not(feature = "wad-doom"), ignore = "needs doom.wad (~/doom/)")]
#[test]
fn e1m2_ss267_platform_vertex_sharing() {
    // SS267 is a lowering platform (sector 109). Wall bottoms on lds 375/376
    // must share a vertex index with the platform's floor polygon so the
    // floor lowering drags them.
    let map = load_map(&doom_wad_path(), "E1M2");
    let bsp3d = &map.bsp_3d;
    let verts = &bsp3d.vertices;

    let floor_poly = bsp3d
        .leaf_floor_polys(267)
        .next()
        .expect("ss267 has a floor polygon");
    let sector_id = bsp3d.polygons[floor_poly].sector.num as usize;
    let floor_verts = collect_sector_vertices(bsp3d, sector_id, Surface::Floor);
    let floor_h = verts[bsp3d.poly_vert_indices(floor_poly)[0]].z;

    let mut unshared = Vec::new();
    for ld in [375usize, 376] {
        for gi in ld_walls(bsp3d, ld) {
            for &vi in bsp3d.poly_vert_indices(gi) {
                if (verts[vi].z - floor_h).abs() < 1.0 && !floor_verts.contains(&vi) {
                    unshared.push(vi);
                }
            }
        }
    }
    unshared.sort_unstable();
    unshared.dedup();
    assert!(
        unshared.is_empty(),
        "SS267 platform sector {sector_id}: wall vertices {unshared:?} at floor height not shared with floor polygon"
    );
}

// ---------------------------------------------------------------------------
// E1M3 — stairs: moving floors, lower walls between steps, vertex sharing.
// ---------------------------------------------------------------------------

const E1M3_STAIR_SECTORS: [usize; 10] = [16, 17, 18, 19, 8, 9, 10, 11, 12, 13];

#[cfg_attr(not(feature = "wad-doom"), ignore = "needs doom.wad (~/doom/)")]
#[test]
fn e1m3_stair_sectors_have_moving_floors() {
    let map = load_map(&doom_wad_path(), "E1M3");
    let bsp3d = &map.bsp_3d;
    for &sid in &E1M3_STAIR_SECTORS {
        let has_moving = bsp3d.sector_floor_polys[sid]
            .iter()
            .any(|&gi| bsp3d.poly_flags[gi].contains(PolyFlags::MOVES));
        assert!(has_moving, "sector {sid} should have moving floor polygons");
    }
}

#[cfg_attr(not(feature = "wad-doom"), ignore = "needs doom.wad (~/doom/)")]
#[test]
fn e1m3_stair_sectors_have_lower_walls_between_steps() {
    let map = load_map(&doom_wad_path(), "E1M3");
    let bsp3d = &map.bsp_3d;
    for &sid in &E1M3_STAIR_SECTORS {
        let lower_walls = bsp3d.sector_leaves[sid]
            .iter()
            .flat_map(|&ssid| bsp3d.leaf_poly_indices(ssid))
            .filter(|&gi| bsp3d.wall_slot(gi) == Some(WallSlot::Lower))
            .count();
        assert!(
            lower_walls > 0,
            "sector {sid} should have a lower wall polygon"
        );
    }
}

/// Corner-UV v span of a polygon (max v - min v across its corners), read the
/// same way wgpu3d's `fan_corner_uv` produces them.
fn poly_uv_v_span(bsp3d: &BSP3D, gi: usize) -> f32 {
    let (start, end) = bsp3d.poly_vertex_range[gi];
    let mut lo = f32::INFINITY;
    let mut hi = f32::NEG_INFINITY;
    for i in start..end {
        let v = bsp3d.poly_vertex_uv[i][1];
        lo = lo.min(v);
        hi = hi.max(v);
    }
    hi - lo
}

#[cfg_attr(not(feature = "wad-doom"), ignore = "needs doom.wad (~/doom/)")]
#[test]
fn e1m3_zh_lower_wall_gets_uv_after_expand() {
    let mut map = load_map(&doom_wad_path(), "E1M3");
    let bsp3d = &map.bsp_3d;

    // Find a stair lower wall that is zero-height at build (the step has not
    // risen yet) AND vertex-linked to the step sector's floor, so dropping the
    // floor expands it.
    let mut target = None;
    'outer: for &sid in &E1M3_STAIR_SECTORS {
        let floor_verts = collect_sector_vertices(bsp3d, sid, Surface::Floor);
        for &ssid in &bsp3d.sector_leaves[sid] {
            for gi in bsp3d.leaf_poly_indices(ssid) {
                if bsp3d.poly_is_flat(gi) || bsp3d.polygons[gi].back_sidedef.is_none() {
                    continue;
                }
                let mut lo = f32::INFINITY;
                let mut hi = f32::NEG_INFINITY;
                for &vi in bsp3d.poly_vert_indices(gi) {
                    lo = lo.min(bsp3d.vertices[vi].z);
                    hi = hi.max(bsp3d.vertices[vi].z);
                }
                // One edge must follow the dropping floor while the other
                // stays — a fully-linked quad translates instead of expanding.
                let linked = bsp3d
                    .poly_vert_indices(gi)
                    .iter()
                    .filter(|&&vi| floor_verts.contains(&vi))
                    .count();
                if (hi - lo).abs() <= 0.1 && linked > 0 && linked < 4 {
                    target = Some((sid, gi, lo));
                    break 'outer;
                }
            }
        }
    }

    let (sid, gi, floor_z) = target.expect("a zero-height stair lower wall");
    assert!(
        poly_uv_v_span(bsp3d, gi) <= 0.1,
        "zh wall starts with no vertical UV span"
    );

    move_sector_surface(&mut map, sid, MovementType::Floor, floor_z - 64.0);

    assert!(
        poly_uv_v_span(&map.bsp_3d, gi) > 1.0,
        "after the floor drops 64 units the lower wall must map its texture \
         across the opened height (got v span {})",
        poly_uv_v_span(&map.bsp_3d, gi)
    );
}

#[cfg_attr(not(feature = "wad-doom"), ignore = "needs doom.wad (~/doom/)")]
#[test]
fn e1m3_stair_wall_vertex_sharing() {
    let map = load_map(&doom_wad_path(), "E1M3");
    let bsp3d = &map.bsp_3d;
    let s16 = collect_sector_vertices(bsp3d, 16, Surface::Floor);
    let s17 = collect_sector_vertices(bsp3d, 17, Surface::Floor);

    let mut total_walls = 0;
    let mut shared = 0;
    for &ssid in &bsp3d.sector_leaves[16] {
        for gi in bsp3d.leaf_poly_indices(ssid) {
            if bsp3d.wall_slot(gi) != Some(WallSlot::Lower) {
                continue;
            }
            total_walls += 1;
            if bsp3d
                .poly_vert_indices(gi)
                .iter()
                .any(|&vi| s17.contains(&vi) || s16.contains(&vi))
            {
                shared += 1;
            }
        }
    }
    assert!(total_walls > 0, "sector 16 should have lower walls");
    assert!(
        shared > 0,
        "lower wall vertices should share with adjacent floor polygons"
    );
}

// ---------------------------------------------------------------------------
// E1M5 — cross-sector mover leaf AABB must cover the full pit travel.
// ---------------------------------------------------------------------------

#[test]
fn e1m5_ld808_lower_wall_aabb_covers_pit() {
    let map = load_map(&doom1_wad_path(), "E1M5");
    let bsp3d = &map.bsp_3d;
    let sectors = &map.sectors;

    let s48 = &sectors[48];
    let mut min_floor = s48.floorheight.to_f32();
    for line in &s48.lines {
        if !line.flags.contains(LineDefFlags::TwoSided) {
            continue;
        }
        let neighbour = if line.frontsector.num == s48.num {
            line.backsector.as_ref()
        } else {
            Some(&line.frontsector)
        };
        if let Some(other) = neighbour {
            min_floor = min_floor.min(other.floorheight.to_f32());
        }
    }
    assert!(
        min_floor < -100.0,
        "expected sector 48 to drop into a deep pit, got {min_floor}"
    );

    let mut checked_lower = false;
    for sid in [48usize, 50] {
        for &ss in &bsp3d.sector_leaves[sid] {
            let leaf = bsp3d.get_leaf(ss).expect("leaf exists");
            for gi in bsp3d.leaf_poly_indices(ss) {
                let is_ld808 = bsp3d.polygons[gi]
                    .linedef
                    .as_ref()
                    .is_some_and(|ld| ld.num == 808);
                if !is_ld808 {
                    continue;
                }
                let slot = bsp3d.wall_slot(gi);
                assert!(
                    leaf.aabb.min.z <= min_floor + 0.5,
                    "leaf {ss} (sector {sid}) ld808 {slot:?} wall aabb.min.z={} does not cover pit floor {min_floor}",
                    leaf.aabb.min.z
                );
                if slot == Some(WallSlot::Lower) {
                    checked_lower = true;
                }
            }
        }
    }
    assert!(
        checked_lower,
        "did not find ld808 lower wall — test geometry changed"
    );
}

// ---------------------------------------------------------------------------
// E5M1 — sector 24 lower walls (sigil).
// ---------------------------------------------------------------------------

#[cfg_attr(
    all(not(feature = "wad-doom"), not(feature = "wad-sigil")),
    ignore = "needs doom.wad + sigil.wad (~/doom/)"
)]
#[test]
fn e5m1_sector24_has_lower_walls() {
    let map = load_map_with_pwad(&doom_wad_path(), &sigil_wad_path(), "E5M1");
    let bsp3d = &map.bsp_3d;

    let count_lower = |sid: usize| -> usize {
        bsp3d.sector_leaves[sid]
            .iter()
            .flat_map(|&ssid| bsp3d.leaf_poly_indices(ssid))
            .filter(|&gi| bsp3d.wall_slot(gi) == Some(WallSlot::Lower))
            .count()
    };

    let mut total = count_lower(24);
    for seg in map.segments.iter() {
        if [207, 627, 208].contains(&seg.linedef.num) && seg.sidedef.bottomtexture.is_some() {
            total += count_lower(seg.frontsector.num as usize);
        }
    }
    assert!(
        total > 0,
        "sector 24 boundary should have lower wall polygons"
    );
}

// ---------------------------------------------------------------------------
// E5M2 — floor mover isolation (sigil) + two-sided face flip on door drop.
// ---------------------------------------------------------------------------

#[cfg_attr(not(feature = "wad-sigil"), ignore = "needs sigil.wad (~/doom/)")]
#[test]
fn e5m2_floor_mover_does_not_drag_neighbour() {
    // ld890 is a one-sided wall in sector 125 (a floor mover, zero-height at
    // rest), sharing a corner with ld857 in sector 110. Dropping sector 125's
    // floor must move ld890 but never ld857.
    let mut map = load_map(&sigil_wad_path(), "E5M2");

    let ld857_before = wall_z(&map.bsp_3d, 857);
    move_sector_surface(&mut map, 125, MovementType::Floor, -8.0);

    let bsp3d = &map.bsp_3d;
    for (sec, (lo, hi)) in wall_z(bsp3d, 890) {
        assert_eq!(sec, 125);
        assert!(
            (lo - -8.0).abs() < 0.5 && (hi - 128.0).abs() < 0.5,
            "ld890 should span -8..128 after the floor drops, got {lo}..{hi}"
        );
    }
    assert_eq!(
        wall_z(bsp3d, 857),
        ld857_before,
        "ld857 (sector 110) must not move"
    );
}

#[cfg_attr(not(feature = "wad-sigil"), ignore = "needs sigil.wad (~/doom/)")]
#[test]
fn e5m2_ld164_lower_face_flips_when_door_drops() {
    // ld164 door (sector 123) drops from ceiling to a pit. Its two-sided lower
    // quad inverts as it crosses sector 122's floor, flipping the visible face.
    let mut map = load_map(&sigil_wad_path(), "E5M2");

    let bsp3d = &map.bsp_3d;
    let gi = ld_walls(bsp3d, 164)
        .into_iter()
        .find(|&gi| bsp3d.wall_slot(gi) == Some(WallSlot::Lower))
        .expect("ld164 lower wall exists");
    let front_tex = bsp3d.poly_tex[gi];
    let back_tex = bsp3d.poly_back_tex[gi];
    assert_ne!(back_tex, NO_INDEX, "ld164 back sidedef should be textured");

    assert_eq!(
        bsp3d.visible_tex(gi),
        Some(front_tex),
        "at rest shows front sidedef"
    );
    move_sector_surface(&mut map, 123, MovementType::Floor, -152.0);
    assert!(
        map.bsp_3d.poly_flags[gi].contains(PolyFlags::FLIPPED),
        "the inverted quad must be flagged"
    );
    assert_eq!(
        map.bsp_3d.visible_tex(gi),
        Some(back_tex),
        "after the door drops the back sidedef shows"
    );
    assert_eq!(
        map.bsp_3d.poly_tex[gi], back_tex,
        "poly_tex carries the facing side (the wgpu mesh reads it directly)"
    );
    assert_eq!(
        map.bsp_3d.poly_back_tex[gi], front_tex,
        "the away side now holds the front texture"
    );
}

// ---------------------------------------------------------------------------
// E5M7 — floor mover must not drag neighbour sky walls (doom + sigil).
// ---------------------------------------------------------------------------

#[cfg_attr(
    all(not(feature = "wad-doom"), not(feature = "wad-sigil")),
    ignore = "needs doom.wad + sigil.wad (~/doom/)"
)]
#[test]
fn e5m7_floor_mover_does_not_drag_sky_walls() {
    let mut wad = WadData::new(&doom_wad_path());
    wad.add_file(sigil_wad_path());
    let mut map = load_map_with_flats(&wad, "E5M7").0;

    move_sector_surface(&mut map, 819, MovementType::Floor, 300.0);

    let bsp3d = &map.bsp_3d;
    let wall_zs = |bsp3d: &BSP3D, ld: usize| -> Vec<f32> {
        ld_walls(bsp3d, ld)
            .into_iter()
            .filter(|&gi| bsp3d.wall_slot(gi) == Some(WallSlot::Middle))
            .flat_map(|gi| {
                bsp3d
                    .poly_vert_indices(gi)
                    .iter()
                    .map(|&v| bsp3d.vertices[v].z)
                    .collect::<Vec<_>>()
            })
            .collect()
    };

    // Neighbour one-sided perimeter walls (820/821): every vertex stays at the
    // static floor (160) or sky ceiling (448), never the mover's 300.
    for ld in [5530usize, 5531, 5532, 5533, 1010, 1011, 1036] {
        for z in wall_zs(bsp3d, ld) {
            assert!(
                (z - 160.0).abs() < 0.5 || (z - 448.0).abs() < 0.5,
                "neighbour wall ld{ld} vertex dragged to {z} (expected 160 or 448)"
            );
        }
    }

    // Sector 819's own walls track the floor: bottom 300, top 448.
    for ld in [5534usize, 5539] {
        let walls = ld_walls(bsp3d, ld);
        assert!(!walls.is_empty(), "ld{ld} wall not found");
        for gi in walls {
            let lo = bsp3d
                .poly_vert_indices(gi)
                .iter()
                .map(|&v| bsp3d.vertices[v].z)
                .fold(f32::INFINITY, f32::min);
            assert!(
                (lo - 300.0).abs() < 0.5,
                "ld{ld} bottom should track floor, got {lo}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// E6M1 — shared lower wall both sides + mover-wall cross-product normals +
// zero-height ceiling-mover floor/ceiling separation (doom + sigil2).
// ---------------------------------------------------------------------------

#[cfg_attr(
    all(not(feature = "wad-doom"), not(feature = "wad-sigil2")),
    ignore = "needs doom.wad + sigil2.wad (~/doom/)"
)]
#[test]
fn e6m1_ld1200_lower_wall_shared_both_sides() {
    let mut wad = WadData::new(&doom_wad_path());
    wad.add_file(sigil2_wad_path());
    let mut map = load_map_with_flats(&wad, "E6M1").0;

    let bsp3d = &map.bsp_3d;
    let gi = ld_walls(bsp3d, 1200)
        .into_iter()
        .find(|&gi| bsp3d.wall_slot(gi) == Some(WallSlot::Lower))
        .expect("ld1200 lower wall exists");

    let mut owning_sectors = std::collections::BTreeSet::new();
    for ss in 0..bsp3d.leaves.len() {
        if bsp3d.leaf_poly_indices(ss).any(|g| g == gi) {
            owning_sectors.insert(bsp3d.leaves[ss].sector.num as usize);
        }
    }
    assert!(
        owning_sectors.contains(&99) && owning_sectors.contains(&144),
        "ld1200 lower wall must be in a sector-99 and a sector-144 leaf, got {owning_sectors:?}"
    );

    move_sector_surface(&mut map, 144, MovementType::Floor, -320.0);
    let bsp3d = &map.bsp_3d;
    let (lo, hi) = bsp3d.poly_vert_indices(gi).iter().fold(
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

#[cfg_attr(
    all(not(feature = "wad-doom"), not(feature = "wad-sigil2")),
    ignore = "needs doom.wad + sigil2.wad (~/doom/)"
)]
#[test]
fn e6m1_mover_wall_cross_product_normals() {
    // For mover walls the renderer's flip test recomputes the winding cross
    // from the first two edges; at rest it must agree (non-negative dot) with
    // the parse-derived normal.
    let map = load_map_with_pwad(&doom_wad_path(), &sigil2_wad_path(), "E6M1");
    let bsp3d = &map.bsp_3d;
    let verts = &bsp3d.vertices;
    let mut failures = Vec::new();

    for gi in 0..bsp3d.polygons.len() {
        if bsp3d.poly_is_flat(gi) || !bsp3d.poly_flags[gi].contains(PolyFlags::MOVES) {
            continue;
        }
        let poly_verts = bsp3d.poly_vert_indices(gi);
        if poly_verts.len() < 3 {
            continue;
        }
        let p0 = verts[poly_verts[0]];
        let p1 = verts[poly_verts[1]];
        let p2 = verts[poly_verts[2]];
        let cross = (p1 - p0).cross(p2 - p0);
        if cross.length_squared() <= f32::EPSILON {
            continue; // degenerate (zh at rest) → flip test treats as unflipped
        }
        let dot = cross.normalize().dot(bsp3d.polygons[gi].normal);
        if dot < 0.0 {
            failures.push(format!("wall poly={gi}: cross/stored dot={dot:.3}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} cross-product mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[cfg_attr(
    all(not(feature = "wad-doom"), not(feature = "wad-sigil2")),
    ignore = "needs doom.wad + sigil2.wad (~/doom/)"
)]
#[test]
fn e6m1_sector76_floor_ceil_separation() {
    // Sector 76 is a zero-height ceiling mover (floor=ceil=64). Floor and
    // ceiling polygons must share no vertex index (else the mover drags the
    // floor), and boundary upper-wall bottoms at ceil height must share with
    // the ceiling polygons.
    let map = load_map_with_pwad(&doom_wad_path(), &sigil2_wad_path(), "E6M1");
    let bsp3d = &map.bsp_3d;
    let verts = &bsp3d.vertices;
    let sector_id = 76;

    let floor_vis = collect_sector_vertices(bsp3d, sector_id, Surface::Floor);
    let ceil_vis = collect_sector_vertices(bsp3d, sector_id, Surface::Ceiling);

    let shared: Vec<usize> = floor_vis.intersection(&ceil_vis).copied().collect();
    assert!(
        shared.is_empty(),
        "sector 76: {} vertex indices shared between floor and ceiling",
        shared.len()
    );

    let border_lds = collect_border_linedefs(&map, sector_id);
    let mut wall_unshared = Vec::new();
    for &ld in &border_lds {
        for gi in ld_walls(bsp3d, ld) {
            for &vi in bsp3d.poly_vert_indices(gi) {
                if (verts[vi].z - 64.0).abs() < 1.0
                    && !ceil_vis.contains(&vi)
                    && !floor_vis.contains(&vi)
                {
                    wall_unshared.push(vi);
                }
            }
        }
    }
    wall_unshared.sort_unstable();
    wall_unshared.dedup();
    assert!(
        wall_unshared.is_empty(),
        "sector 76: wall vertices {wall_unshared:?} at ceiling height not shared with ceiling polygons"
    );
}

// ---------------------------------------------------------------------------
// MAP03 — stairs must not overrun the neighbouring sector floor (doom2).
// ---------------------------------------------------------------------------

#[cfg_attr(not(feature = "wad-doom2"), ignore = "needs doom2.wad (~/doom/)")]
#[test]
fn map03_stairs_do_not_overrun_sector_90() {
    let map = load_map(&doom2_wad_path(), "MAP03");

    // 2D point-in-polygon over a subsector's carved floor (XY).
    fn point_in_floor(verts: &[Vec2], p: Vec2) -> bool {
        let mut inside = false;
        let n = verts.len();
        let mut j = n - 1;
        for i in 0..n {
            let (vi, vj) = (verts[i], verts[j]);
            if (vi.y > p.y) != (vj.y > p.y) {
                let t = (p.y - vi.y) / (vj.y - vi.y);
                if p.x < vi.x + t * (vj.x - vi.x) {
                    inside = !inside;
                }
            }
            j = i;
        }
        inside
    }

    // Find the subsector whose floor polygon contains `p` → (ss, sector, z).
    let floor_at = |p: Vec2| -> (usize, usize, f32) {
        let bsp3d = &map.bsp_3d;
        let mut hit = None;
        for ss in 0..bsp3d.leaves.len() {
            for fi in bsp3d.leaf_floor_polys(ss) {
                let poly_verts = bsp3d.poly_vert_indices(fi);
                let verts: Vec<Vec2> = poly_verts
                    .iter()
                    .map(|&v| Vec2::new(bsp3d.vertices[v].x, bsp3d.vertices[v].y))
                    .collect();
                if verts.len() >= 3 && point_in_floor(&verts, p) {
                    let z = bsp3d.vertices[poly_verts[0]].z;
                    assert!(hit.is_none(), "point {p:?} covered by multiple floor polys");
                    hit = Some((ss, bsp3d.leaves[ss].sector.num as usize, z));
                }
            }
        }
        hit.unwrap_or_else(|| panic!("no floor polygon covers point {p:?}"))
    };

    // Steps (north of y=3328): each renders at its own height.
    let steps = [
        (4020.0, 80.0),
        (3996.0, 88.0),
        (3972.0, 96.0),
        (3948.0, 104.0),
    ];
    for (x, expect_floor) in steps {
        let (ss, sid, z) = floor_at(Vec2::new(x, 3360.0));
        assert!(
            (z - expect_floor).abs() < 0.5,
            "step at x={x}: ss {ss} sector {sid} floor {z}, expected {expect_floor}"
        );
    }
    // Sector 90 area south of the steps (y<3328): floor 112, not a step height.
    for (x, _) in steps {
        let (ss, sid, z) = floor_at(Vec2::new(x, 3312.0));
        assert!(
            (z - 112.0).abs() < 0.5,
            "sector-90 area at x={x},y=3312: ss {ss} sector {sid} floor {z}, expected 112"
        );
    }
    // No leaf merges two step sectors: each floor polygon is flat.
    let bsp3d = &map.bsp_3d;
    for ss in 0..bsp3d.leaves.len() {
        for fi in bsp3d.leaf_floor_polys(ss) {
            let poly_verts = bsp3d.poly_vert_indices(fi);
            let z0 = bsp3d.vertices[poly_verts[0]].z;
            for &v in poly_verts {
                assert!(
                    (bsp3d.vertices[v].z - z0).abs() < 0.5,
                    "ss {ss} floor poly spans heights {z0}..{} (non-flat leaf)",
                    bsp3d.vertices[v].z
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// MAP03 — lift wall inverts; the back sidedef's texture must face the pit
// (regression: rendered untextured grey in the wgpu renderers).
// ---------------------------------------------------------------------------

#[cfg_attr(not(feature = "wad-doom2"), ignore = "needs doom2.wad (~/doom/)")]
#[test]
fn map03_ld86_inverted_lift_wall_presents_back_texture() {
    let mut map = load_map(&doom2_wad_path(), "MAP03");

    // ld86: front sector 13 (no bottomtexture), back sector 14 = the lift.
    let back_tex = map.linedefs[86]
        .back_sidedef
        .as_ref()
        .and_then(|sd| sd.bottomtexture)
        .expect("ld86 back sidedef has a lower texture") as u32;
    let gi = ld_walls(&map.bsp_3d, 86)
        .into_iter()
        .find(|&gi| map.bsp_3d.wall_slot(gi) == Some(WallSlot::Lower))
        .expect("ld86 lower wall exists");
    assert_eq!(
        map.bsp_3d.poly_tex[gi], NO_INDEX,
        "at rest (zero height, unflipped) the facing side is the untextured front"
    );

    move_sector_surface(&mut map, 14, MovementType::Floor, 0.0);

    let bsp3d = &map.bsp_3d;
    assert!(
        bsp3d.poly_flags[gi].contains(PolyFlags::FLIPPED),
        "lift drop inverts the quad"
    );
    assert_eq!(
        bsp3d.poly_tex[gi], back_tex,
        "facing texture must be the lift side's lower texture"
    );
    assert_eq!(bsp3d.visible_tex(gi), Some(back_tex), "visible_tex agrees");
}

// ---------------------------------------------------------------------------
// MAP29 — two-sided wall is one shared polygon with both faces (doom2).
// ---------------------------------------------------------------------------

#[cfg_attr(not(feature = "wad-doom2"), ignore = "needs doom2.wad (~/doom/)")]
#[test]
fn map29_ld10_single_shared_lower_wall() {
    let map = load_map(&doom2_wad_path(), "MAP29");
    let bsp3d = &map.bsp_3d;

    let lowers: Vec<usize> = ld_walls(bsp3d, 10)
        .into_iter()
        .filter(|&gi| bsp3d.wall_slot(gi) == Some(WallSlot::Lower))
        .collect();
    assert_eq!(lowers.len(), 1, "ld10 lower must be a single polygon");
    let gi = lowers[0];

    let p = &bsp3d.polygons[gi];
    let front_lower = p.sidedef.as_ref().and_then(|sd| sd.bottomtexture);
    let back_lower = p.back_sidedef.as_ref().and_then(|sd| sd.bottomtexture);
    assert!(
        front_lower.is_some() && back_lower.is_some(),
        "both sidedefs textured"
    );
    assert_eq!(bsp3d.poly_tex[gi], front_lower.unwrap() as u32);
    assert_eq!(bsp3d.poly_back_tex[gi], back_lower.unwrap() as u32);

    let refs = (0..bsp3d.leaves.len())
        .filter(|&ss| bsp3d.leaf_poly_indices(ss).any(|g| g == gi))
        .count();
    assert!(
        refs >= 2,
        "shared wall must be referenced by both sides, got {refs}"
    );
    assert_eq!(bsp3d.visible_tex(gi), Some(bsp3d.poly_tex[gi]));
}

// ---------------------------------------------------------------------------
// Mover-target computation + vanilla-special normalisation (doom1).
// ---------------------------------------------------------------------------

#[test]
fn e1m5_movers_have_targets() {
    let mut map = load_map(&doom1_wad_path(), "E1M5");
    let mut any = false;
    for sid in 0..map.sectors.len() {
        let targets = mover_targets_for_sector(sid, &mut map, &|_| 0);
        if !targets.is_empty() {
            any = true;
            for t in &targets {
                assert!(t.height.is_finite(), "sector {sid} target not finite");
            }
        }
    }
    assert!(any, "E1M5 should yield at least one mover target");
}

#[test]
fn e1m5_specials_normalized_at_load() {
    let map = load_map(&doom1_wad_path(), "E1M5");
    let mut saw_mover = false;
    for line in map.linedefs.iter() {
        let original = line.default_special;
        if original <= 0 {
            assert_eq!(line.special, original as u32);
            continue;
        }
        match encode_vanilla(original as u32) {
            Some(generalized) => {
                saw_mover = true;
                assert_eq!(
                    line.special, generalized,
                    "linedef special {original} not normalized (got {:#x})",
                    line.special
                );
                assert!(is_generalized(line.special));
                assert!(
                    decode(line.special).is_some(),
                    "normalized special {:#x} (from {original}) did not decode",
                    line.special
                );
            }
            None => assert_eq!(line.special, original as u32),
        }
    }
    assert!(saw_mover, "E1M5 should contain at least one mover special");
}

// ---------------------------------------------------------------------------
// Flat-pic change re-resolves the surface cache from the (already mutated)
// sector pic without disturbing the other surface.
// ---------------------------------------------------------------------------

#[test]
fn e1m1_update_flat_texture_resolves_from_sector() {
    let mut map = load_map(&doom1_wad_path(), "E1M1");

    // A sector with both floor and ceiling polygons; floor gets a new pic, the
    // ceiling must be left untouched.
    let sector_id = (0..map.bsp_3d.sector_leaves.len())
        .find(|&sid| {
            !map.bsp_3d.sector_floor_polys[sid].is_empty()
                && !map.bsp_3d.sector_ceiling_polys[sid].is_empty()
        })
        .expect("E1M1 has a sector with floor and ceiling polygons");

    let floor_polys = map.bsp_3d.sector_floor_polys[sector_id].clone();
    let ceil_polys = map.bsp_3d.sector_ceiling_polys[sector_id].clone();
    let ceil_tex_before: Vec<u32> = ceil_polys
        .iter()
        .map(|&gi| map.bsp_3d.poly_tex[gi])
        .collect();
    let new_texture = map.bsp_3d.poly_tex[floor_polys[0]] as usize + 7;

    map.sectors[sector_id].floorpic = new_texture;
    map.bsp_3d.clear_texture_dirty();
    map.bsp_3d
        .update_flat_texture(sector_id, MovementType::Floor);

    assert!(
        map.bsp_3d.texture_dirty(),
        "update_flat_texture must set texture_dirty"
    );

    for &gi in &floor_polys {
        assert_eq!(
            map.bsp_3d.poly_tex[gi], new_texture as u32,
            "floor poly {gi} poly_tex not re-resolved from the sector pic"
        );
    }
    for (&gi, &before) in ceil_polys.iter().zip(&ceil_tex_before) {
        assert_eq!(
            map.bsp_3d.poly_tex[gi], before,
            "ceiling poly {gi} changed by a floor-only update"
        );
    }
}

#[test]
fn e1m5_manual_door_decodes_manual() {
    let map = load_map(&doom1_wad_path(), "E1M5");
    let manual_specials = [1i16, 26, 27, 28, 31, 32, 33, 34, 117, 118];
    let mut found = false;
    for line in map.linedefs.iter() {
        if manual_specials.contains(&line.default_special) {
            found = true;
            let spec = decode(line.special).expect("manual door decodes");
            assert_eq!(spec.category, Category::Door);
            assert!(
                spec.manual,
                "door {} not flagged manual",
                line.default_special
            );
        }
    }
    assert!(found, "E1M5 should contain at least one manual door");
}
