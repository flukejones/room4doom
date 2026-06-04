//! BSP3D mover behaviour: wall-vertex sharing at mover boundaries, door/floor
//! movement, stairs, two-sided face flips, cross-sector drag isolation, leaf
//! AABB coverage, and vanilla-special normalisation.

use std::collections::HashSet;

use glam::Vec2;
use level::env_target::mover_targets_for_sector;
use level::special_encode::{Category, decode, encode_vanilla, is_generalized};
use level::{BSP3D, LevelData, LineDefFlags, MovementType, SurfaceKind, WallType};
use test_utils::{
    Surface, collect_border_linedefs, collect_sector_vertices, doom_wad_path, doom1_wad_path,
    doom2_wad_path, load_map, load_map_with_flats, load_map_with_pwad, sigil_wad_path,
    sigil2_wad_path,
};
use wad::WadData;

// ---------------------------------------------------------------------------
// Helpers shared by the mover tests below.
// ---------------------------------------------------------------------------

/// Z-range of every wall quad on `ld`, paired with its sector.
fn wall_z(bsp3d: &BSP3D, ld: usize) -> Vec<(usize, (f32, f32))> {
    let mut out = Vec::new();
    for ss in 0..bsp3d.subsector_leaves.len() {
        for poly in bsp3d.leaf_polygons(ss) {
            if let SurfaceKind::Vertical {
                linedef_id,
                ..
            } = &poly.surface_kind
                && *linedef_id == ld
            {
                let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
                for &v in &poly.vertices {
                    let z = bsp3d.vertices[v].z;
                    lo = lo.min(z);
                    hi = hi.max(z);
                }
                out.push((poly.sector_id, (lo, hi)));
            }
        }
    }
    out
}

/// Bottom-of-wall vertices on a mover boundary that fail to share a vertex
/// index with the mover sector's surface polygons — i.e. `move_surface` will
/// not drag them. `surface`/`wall_type` select floor-mover (lower walls) or
/// ceiling-mover (upper walls).
fn unshared_boundary_walls(
    map: &LevelData,
    sector_id: usize,
    surface: Surface,
    wall_type: WallType,
) -> Vec<usize> {
    let bsp3d = &map.bsp_3d;
    let verts = &bsp3d.vertices;
    let surface_verts = collect_sector_vertices(bsp3d, sector_id, surface);
    let border_lds = collect_border_linedefs(map, sector_id);

    let ssid = bsp3d.sector_subsectors[sector_id][0];
    let leaf = &bsp3d.subsector_leaves[ssid];
    let polys = match surface {
        Surface::Floor => &leaf.floor_polygons,
        Surface::Ceiling => &leaf.ceiling_polygons,
    };
    let target_z = verts[bsp3d.polygons[polys[0]].vertices[0]].z;

    let mut unshared = Vec::new();
    for leaf in &bsp3d.subsector_leaves {
        for &gi in &leaf.polygon_indices {
            let poly = &bsp3d.polygons[gi];
            if let SurfaceKind::Vertical {
                wall_type: wt,
                linedef_id,
                ..
            } = &poly.surface_kind
                && border_lds.contains(linedef_id)
                && *wt == wall_type
            {
                // A flat (single-Z) wall has no edge crossing the surface.
                let all_same_z = poly
                    .vertices
                    .iter()
                    .all(|&vi| (verts[vi].z - verts[poly.vertices[0]].z).abs() < 1.0);
                if all_same_z {
                    continue;
                }
                for &vi in &poly.vertices {
                    if (verts[vi].z - target_z).abs() < 1.0 && !surface_verts.contains(&vi) {
                        unshared.push(vi);
                    }
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
    let floor = unshared_boundary_walls(&map, 14, Surface::Floor, WallType::Lower);
    assert!(
        floor.is_empty(),
        "sector 14 floor mover: lower wall top vertices {floor:?} not shared with floor polygons"
    );
    let ceil = unshared_boundary_walls(&map, 26, Surface::Ceiling, WallType::Upper);
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
    let bsp3d = &mut map.bsp_3d;
    let initial = bsp3d.vertices.clone();

    bsp3d.move_surface(129, MovementType::Ceiling, 128.0);

    let ss_ids = bsp3d.sector_subsectors[129].clone();
    let mut stuck = 0;
    for &ssid in &ss_ids {
        let leaf = &bsp3d.subsector_leaves[ssid];
        for &ci in &leaf.ceiling_polygons {
            for &vi in &bsp3d.polygons[ci].vertices {
                if (initial[vi].z - bsp3d.vertices[vi].z).abs() <= 0.001 {
                    stuck += 1;
                }
            }
        }
    }
    assert_eq!(stuck, 0, "all sector 129 ceiling vertices should move");
}

#[cfg_attr(not(feature = "wad-doom"), ignore = "needs doom.wad (~/doom/)")]
#[test]
fn e1m2_sector129_door_boundary_vertex_sharing() {
    let map = load_map(&doom_wad_path(), "E1M2");
    let unshared = unshared_boundary_walls(&map, 129, Surface::Ceiling, WallType::Upper);
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

    let leaf267 = &bsp3d.subsector_leaves[267];
    let sector_id = bsp3d.polygons[leaf267.floor_polygons[0]].sector_id;
    let floor_verts = collect_sector_vertices(bsp3d, sector_id, Surface::Floor);
    let floor_h = verts[bsp3d.polygons[leaf267.floor_polygons[0]].vertices[0]].z;

    let target_lds: HashSet<usize> = [375, 376].into_iter().collect();
    let mut unshared = Vec::new();
    for leaf in &bsp3d.subsector_leaves {
        for &gi in &leaf.polygon_indices {
            let poly = &bsp3d.polygons[gi];
            if let SurfaceKind::Vertical {
                linedef_id,
                ..
            } = &poly.surface_kind
                && target_lds.contains(linedef_id)
            {
                for &vi in &poly.vertices {
                    if (verts[vi].z - floor_h).abs() < 1.0 && !floor_verts.contains(&vi) {
                        unshared.push(vi);
                    }
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
        let has_moving = bsp3d.sector_subsectors[sid].iter().any(|&ssid| {
            bsp3d.subsector_leaves[ssid]
                .floor_polygons
                .iter()
                .any(|&pidx| bsp3d.polygons[pidx].moves)
        });
        assert!(has_moving, "sector {sid} should have moving floor polygons");
    }
}

#[cfg_attr(not(feature = "wad-doom"), ignore = "needs doom.wad (~/doom/)")]
#[test]
fn e1m3_stair_sectors_have_lower_walls_between_steps() {
    let map = load_map(&doom_wad_path(), "E1M3");
    let bsp3d = &map.bsp_3d;
    for &sid in &E1M3_STAIR_SECTORS {
        let lower_walls = bsp3d.sector_subsectors[sid]
            .iter()
            .flat_map(|&ssid| &bsp3d.subsector_leaves[ssid].polygon_indices)
            .filter(|&&gi| {
                matches!(
                    bsp3d.polygons[gi].surface_kind,
                    SurfaceKind::Vertical {
                        wall_type: WallType::Lower,
                        ..
                    }
                )
            })
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
    for i in start as usize..end as usize {
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
    let bsp3d = &mut map.bsp_3d;

    // Find a stair lower wall that is zero-height at build (the step has not
    // risen yet) and note its sector.
    let mut target = None;
    'outer: for &sid in &E1M3_STAIR_SECTORS {
        for &ssid in &bsp3d.sector_subsectors[sid] {
            for &gi in &bsp3d.subsector_leaves[ssid].polygon_indices {
                if !matches!(
                    bsp3d.polygons[gi].surface_kind,
                    SurfaceKind::Vertical {
                        wall_type: WallType::Lower,
                        ..
                    }
                ) {
                    continue;
                }
                let (lo, hi) = {
                    let p = &bsp3d.polygons[gi];
                    let mut lo = f32::INFINITY;
                    let mut hi = f32::NEG_INFINITY;
                    for &vi in &p.vertices {
                        lo = lo.min(bsp3d.vertices[vi].z);
                        hi = hi.max(bsp3d.vertices[vi].z);
                    }
                    (lo, hi)
                };
                if (hi - lo).abs() <= 0.1 {
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

    bsp3d.move_surface(sid, MovementType::Floor, floor_z - 64.0);

    assert!(
        poly_uv_v_span(bsp3d, gi) > 1.0,
        "after the floor drops 64 units the lower wall must map its texture \
         across the opened height (got v span {})",
        poly_uv_v_span(bsp3d, gi)
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
    for &ssid in &bsp3d.sector_subsectors[16] {
        for &gi in &bsp3d.subsector_leaves[ssid].polygon_indices {
            let poly = &bsp3d.polygons[gi];
            if !matches!(
                poly.surface_kind,
                SurfaceKind::Vertical {
                    wall_type: WallType::Lower,
                    ..
                }
            ) {
                continue;
            }
            total_walls += 1;
            if poly
                .vertices
                .iter()
                .any(|vi| s17.contains(vi) || s16.contains(vi))
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
        for &ss in &bsp3d.sector_subsectors[sid] {
            let leaf = bsp3d.get_subsector_leaf(ss).expect("leaf exists");
            for poly in bsp3d.leaf_polygons(ss) {
                if let SurfaceKind::Vertical {
                    linedef_id: 808,
                    wall_type,
                    ..
                } = poly.surface_kind
                {
                    assert!(
                        leaf.aabb.min.z <= min_floor + 0.5,
                        "leaf {ss} (sector {sid}) ld808 {wall_type:?} wall aabb.min.z={} does not cover pit floor {min_floor}",
                        leaf.aabb.min.z
                    );
                    if wall_type == WallType::Lower {
                        checked_lower = true;
                    }
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
        bsp3d.sector_subsectors[sid]
            .iter()
            .flat_map(|&ssid| &bsp3d.subsector_leaves[ssid].polygon_indices)
            .filter(|&&gi| {
                matches!(
                    bsp3d.polygons[gi].surface_kind,
                    SurfaceKind::Vertical {
                        wall_type: WallType::Lower,
                        ..
                    }
                )
            })
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
    let bsp3d = &mut map.bsp_3d;

    let ld857_before = wall_z(bsp3d, 857);
    bsp3d.move_surface(125, MovementType::Floor, -8.0);

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
    let bsp3d = &mut map.bsp_3d;

    let (ss, front_tex, back_tex) = {
        let mut found = None;
        for ss in 0..bsp3d.subsector_leaves.len() {
            for poly in bsp3d.leaf_polygons(ss) {
                if let SurfaceKind::Vertical {
                    linedef_id: 164,
                    wall_type: WallType::Lower,
                    front,
                    back,
                    ..
                } = &poly.surface_kind
                {
                    found = Some((ss, front.texture, back.as_ref().and_then(|f| f.texture)));
                }
            }
        }
        found.expect("ld164 lower wall exists")
    };
    assert!(back_tex.is_some(), "ld164 back sidedef should be textured");

    let face_tex = |bsp3d: &BSP3D| {
        bsp3d
            .leaf_polygons(ss)
            .find_map(|p| match &p.surface_kind {
                SurfaceKind::Vertical {
                    linedef_id: 164,
                    wall_type: WallType::Lower,
                    ..
                } => p.visible_face(&bsp3d.vertices).map(|f| f.texture),
                _ => None,
            })
            .expect("visible face")
    };

    assert_eq!(face_tex(bsp3d), front_tex, "at rest shows front sidedef");
    bsp3d.move_surface(123, MovementType::Floor, -152.0);
    assert_eq!(
        face_tex(bsp3d),
        back_tex,
        "after the door drops the back sidedef shows"
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
    let bsp3d = &mut map.bsp_3d;

    bsp3d.move_surface(819, MovementType::Floor, 300.0);

    let wall_zs = |bsp3d: &BSP3D, ld: usize| -> Vec<f32> {
        let mut out = Vec::new();
        for ss in 0..bsp3d.subsector_leaves.len() {
            for poly in bsp3d.leaf_polygons(ss) {
                if let SurfaceKind::Vertical {
                    linedef_id,
                    wall_type: WallType::Middle,
                    ..
                } = &poly.surface_kind
                    && *linedef_id == ld
                {
                    out.extend(poly.vertices.iter().map(|&v| bsp3d.vertices[v].z));
                }
            }
        }
        out
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
        let mut found = false;
        for ss in 0..bsp3d.subsector_leaves.len() {
            for poly in bsp3d.leaf_polygons(ss) {
                if let SurfaceKind::Vertical {
                    linedef_id,
                    ..
                } = &poly.surface_kind
                    && *linedef_id == ld
                {
                    let lo = poly
                        .vertices
                        .iter()
                        .map(|&v| bsp3d.vertices[v].z)
                        .fold(f32::INFINITY, f32::min);
                    assert!(
                        (lo - 300.0).abs() < 0.5,
                        "ld{ld} bottom should track floor, got {lo}"
                    );
                    found = true;
                }
            }
        }
        assert!(found, "ld{ld} wall not found");
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
    let bsp3d = &mut map.bsp_3d;

    let gi = (0..bsp3d.polygons.len())
        .find(|&gi| {
            matches!(
                bsp3d.polygons[gi].surface_kind,
                SurfaceKind::Vertical {
                    linedef_id: 1200,
                    wall_type: WallType::Lower,
                    ..
                }
            )
        })
        .expect("ld1200 lower wall exists");

    let mut owning_sectors = std::collections::BTreeSet::new();
    for ss in 0..bsp3d.subsector_leaves.len() {
        let leaf = bsp3d.get_subsector_leaf(ss).unwrap();
        if leaf.polygon_indices.contains(&gi) {
            owning_sectors.insert(leaf.sector_id);
        }
    }
    assert!(
        owning_sectors.contains(&99) && owning_sectors.contains(&144),
        "ld1200 lower wall must be in a sector-99 and a sector-144 leaf, got {owning_sectors:?}"
    );

    bsp3d.move_surface(144, MovementType::Floor, -320.0);
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

#[cfg_attr(
    all(not(feature = "wad-doom"), not(feature = "wad-sigil2")),
    ignore = "needs doom.wad + sigil2.wad (~/doom/)"
)]
#[test]
fn e6m1_mover_wall_cross_product_normals() {
    // For mover walls the renderer recomputes the normal from the first two
    // edges; it must agree (positive dot) with the stored normal.
    use glam::Vec3;

    let map = load_map_with_pwad(&doom_wad_path(), &sigil2_wad_path(), "E6M1");
    let bsp3d = &map.bsp_3d;
    let verts = &bsp3d.vertices;
    let mut failures = Vec::new();

    for (ssid, leaf) in bsp3d.subsector_leaves.iter().enumerate() {
        for (local_pi, &gi) in leaf.polygon_indices.iter().enumerate() {
            let poly = &bsp3d.polygons[gi];
            if !poly.moves
                || poly.vertices.len() < 3
                || !matches!(poly.surface_kind, SurfaceKind::Vertical { .. })
            {
                continue;
            }
            let p0 = verts[poly.vertices[0]];
            let p1 = verts[poly.vertices[1]];
            let p2 = verts[poly.vertices[2]];
            let cross = (p1 - p0).cross(p2 - p0);
            if cross.length_squared() <= f32::EPSILON {
                continue; // degenerate → renderer falls back to stored normal
            }
            let dot = cross.normalize().dot(poly.normal);
            if dot < 0.0 {
                failures.push(format!(
                    "ss={ssid} wall poly={local_pi}: cross/stored dot={dot:.3}"
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} cross-product mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
    let _ = Vec3::ZERO;
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
    for leaf in &bsp3d.subsector_leaves {
        for &gi in &leaf.polygon_indices {
            let poly = &bsp3d.polygons[gi];
            if let SurfaceKind::Vertical {
                linedef_id,
                ..
            } = &poly.surface_kind
                && border_lds.contains(linedef_id)
            {
                for &vi in &poly.vertices {
                    if (verts[vi].z - 64.0).abs() < 1.0
                        && !ceil_vis.contains(&vi)
                        && !floor_vis.contains(&vi)
                    {
                        wall_unshared.push(vi);
                    }
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
        for ss in 0..bsp3d.subsector_leaves.len() {
            let leaf = bsp3d.get_subsector_leaf(ss).unwrap();
            for &fi in &leaf.floor_polygons {
                let poly = &bsp3d.polygons[fi];
                let verts: Vec<Vec2> = poly
                    .vertices
                    .iter()
                    .map(|&v| Vec2::new(bsp3d.vertices[v].x, bsp3d.vertices[v].y))
                    .collect();
                if verts.len() >= 3 && point_in_floor(&verts, p) {
                    let z = bsp3d.vertices[poly.vertices[0]].z;
                    assert!(hit.is_none(), "point {p:?} covered by multiple floor polys");
                    hit = Some((ss, leaf.sector_id, z));
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
    for ss in 0..bsp3d.subsector_leaves.len() {
        let leaf = bsp3d.get_subsector_leaf(ss).unwrap();
        for &fi in &leaf.floor_polygons {
            let poly = &bsp3d.polygons[fi];
            let z0 = bsp3d.vertices[poly.vertices[0]].z;
            for &v in &poly.vertices {
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
// MAP29 — two-sided wall is one shared polygon with both faces (doom2).
// ---------------------------------------------------------------------------

#[cfg_attr(not(feature = "wad-doom2"), ignore = "needs doom2.wad (~/doom/)")]
#[test]
fn map29_ld10_single_shared_lower_wall() {
    let map = load_map(&doom2_wad_path(), "MAP29");
    let bsp3d = &map.bsp_3d;

    let mut found = None;
    for (gi, poly) in bsp3d.polygons.iter().enumerate() {
        if let SurfaceKind::Vertical {
            linedef_id: 10,
            wall_type: WallType::Lower,
            front,
            back,
            ..
        } = &poly.surface_kind
        {
            assert!(found.is_none(), "ld10 lower must be a single polygon");
            found = Some((
                gi,
                poly,
                front.texture,
                back.as_ref().and_then(|f| f.texture),
            ));
        }
    }
    let (gi, poly, front_tex, back_tex) = found.expect("ld10 lower wall exists");
    assert!(
        front_tex.is_some() && back_tex.is_some(),
        "both sidedefs textured"
    );

    let refs = (0..bsp3d.subsector_leaves.len())
        .filter(|&ss| {
            bsp3d
                .get_subsector_leaf(ss)
                .unwrap()
                .polygon_indices
                .contains(&gi)
        })
        .count();
    assert!(
        refs >= 2,
        "shared wall must be referenced by both sides, got {refs}"
    );
    assert_eq!(
        poly.visible_face(&bsp3d.vertices).map(|f| f.texture),
        Some(front_tex)
    );
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
// M2.6 — flat-pic change syncs both render stores (software3d surface_kind +
// wgpu3d poly_tex) without disturbing the other surface.
// ---------------------------------------------------------------------------

#[test]
fn e1m1_update_flat_texture_syncs_both_stores() {
    let mut map = load_map(&doom1_wad_path(), "E1M1");
    let bsp3d = &mut map.bsp_3d;

    // A sector with both floor and ceiling polygons; floor gets a new pic, the
    // ceiling must be left untouched (separate store, separate surface).
    let pick = (0..bsp3d.sector_subsectors.len()).find(|&sid| {
        let has = |pick_ceiling: bool| {
            bsp3d.sector_subsectors[sid].iter().any(|&ss| {
                let leaf = &bsp3d.subsector_leaves[ss];
                let set = if pick_ceiling {
                    &leaf.ceiling_polygons
                } else {
                    &leaf.floor_polygons
                };
                !set.is_empty()
            })
        };
        has(false) && has(true)
    });
    let sector_id = pick.expect("E1M1 has a sector with floor and ceiling polygons");

    let floor_polys: Vec<usize> = bsp3d.sector_subsectors[sector_id]
        .iter()
        .flat_map(|&ss| bsp3d.subsector_leaves[ss].floor_polygons.iter().copied())
        .collect();
    let ceil_polys: Vec<usize> = bsp3d.sector_subsectors[sector_id]
        .iter()
        .flat_map(|&ss| bsp3d.subsector_leaves[ss].ceiling_polygons.iter().copied())
        .collect();

    let ceil_tex_before: Vec<u32> = ceil_polys.iter().map(|&gi| bsp3d.poly_tex[gi]).collect();
    let new_texture = bsp3d.poly_tex[floor_polys[0]] as usize + 7;

    bsp3d.clear_texture_dirty();
    bsp3d.update_flat_texture(sector_id, MovementType::Floor, new_texture);

    assert!(
        bsp3d.texture_dirty(),
        "update_flat_texture must set texture_dirty"
    );

    for &gi in &floor_polys {
        assert_eq!(
            bsp3d.poly_tex[gi], new_texture as u32,
            "floor poly {gi} poly_tex (wgpu3d store) not updated"
        );
        let SurfaceKind::Horizontal {
            texture,
            ..
        } = &bsp3d.polygons[gi].surface_kind
        else {
            panic!("floor poly {gi} is not Horizontal");
        };
        assert_eq!(
            *texture, new_texture,
            "floor poly {gi} surface_kind texture (software3d store) not updated"
        );
    }

    for (&gi, &before) in ceil_polys.iter().zip(&ceil_tex_before) {
        assert_eq!(
            bsp3d.poly_tex[gi], before,
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
