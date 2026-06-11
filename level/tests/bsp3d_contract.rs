//! New-architecture contract tests: winding-derived normals, leaf/range
//! integrity, surface-cache resolution vs independent derivation, OG-verified
//! UV anchors, and build determinism.

use glam::{Vec2, Vec3};
use level::flags::LineDefFlags;
use level::{LevelData, MovementType, NO_INDEX, PolyFlags, WallSlot};
use test_utils::{
    doom_wad_path, doom1_wad_path, load_map, load_map_with_pwad, move_sector_surface,
    sigil_wad_path,
};

const DOOM1_MAPS: [&str; 9] = [
    "E1M1", "E1M2", "E1M3", "E1M4", "E1M5", "E1M6", "E1M7", "E1M8", "E1M9",
];
const DOOM_MAPS: [&str; 27] = [
    "E1M1", "E1M2", "E1M3", "E1M4", "E1M5", "E1M6", "E1M7", "E1M8", "E1M9", "E2M1", "E2M2", "E2M3",
    "E2M4", "E2M5", "E2M6", "E2M7", "E2M8", "E2M9", "E3M1", "E3M2", "E3M3", "E3M4", "E3M5", "E3M6",
    "E3M7", "E3M8", "E3M9",
];
const SIGIL_MAPS: [&str; 9] = [
    "E5M1", "E5M2", "E5M3", "E5M4", "E5M5", "E5M6", "E5M7", "E5M8", "E5M9",
];

// ---------------------------------------------------------------------------
// Winding/normal contract: flats wind to their ±Z normal; wall normals face
// the building seg's sidedef side of the linedef. This is the proof behind
// dropping the `moves` gate from the flip test.
// ---------------------------------------------------------------------------

fn assert_winding_contract(map: &LevelData, name: &str) {
    let bsp3d = &map.bsp_3d;
    let verts = &bsp3d.vertices;
    let mut failures = Vec::new();

    for gi in 0..bsp3d.polygons.len() {
        let poly_verts = bsp3d.poly_vert_indices(gi);
        let normal = bsp3d.polygons[gi].normal;
        if bsp3d.poly_is_flat(gi) {
            let area = test_utils::shoelace(poly_verts, verts);
            let expected = if area > 0.0 { Vec3::Z } else { Vec3::NEG_Z };
            if area == 0.0 || normal != expected {
                failures.push(format!(
                    "{name} flat {gi}: area={area:.3} normal={normal:?}"
                ));
            }
        } else {
            let v0 = verts[poly_verts[0]];
            let v1 = verts[poly_verts[1]];
            let d = Vec2::new(v1.x - v0.x, v1.y - v0.y);
            if d.length_squared() < 1e-6 {
                continue; // degenerate seg
            }
            let d = d.normalize();
            // Geometric: normal is the right-hand horizontal of v0→v1.
            let geo = Vec3::new(d.y, -d.x, 0.0);
            if (geo - normal).length() > 1e-3 {
                failures.push(format!("{name} wall {gi}: normal {normal:?} != {geo:?}"));
                continue;
            }
            // Map contract: the materialized sidedef is the one on the seg's
            // traversal side (front segs run with the linedef, back against).
            let p = &bsp3d.polygons[gi];
            let ld = p.linedef.as_ref().expect("wall has linedef");
            let sd = p.sidedef.as_ref().expect("wall has sidedef");
            let ld_dir = (ld.v2.pos - ld.v1.pos).normalize_or_zero();
            if ld_dir == Vec2::ZERO {
                continue;
            }
            let is_front = d.dot(ld_dir) >= 0.0;
            if d.dot(ld_dir).abs() < 0.99 {
                failures.push(format!(
                    "{name} wall {gi} (ld {}): seg dir {d:?} not colinear with linedef {ld_dir:?}",
                    ld.num,
                ));
                continue;
            }
            let expected = if is_front {
                ld.front_sidedef.as_ptr()
            } else {
                ld.back_sidedef
                    .as_ref()
                    .map_or(std::ptr::null_mut(), |b| b.as_ptr())
            };
            if sd.as_ptr() != expected {
                failures.push(format!(
                    "{name} wall {gi} (ld {}): sidedef is not the {} side's",
                    ld.num,
                    if is_front { "front" } else { "back" },
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} winding-contract failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn doom1_all_maps_winding_contract() {
    for name in DOOM1_MAPS {
        assert_winding_contract(&load_map(&doom1_wad_path(), name), name);
    }
}

#[cfg_attr(not(feature = "wad-doom"), ignore = "needs doom.wad (~/doom/)")]
#[test]
fn doom_all_maps_winding_contract() {
    for name in DOOM_MAPS {
        assert_winding_contract(&load_map(&doom_wad_path(), name), name);
    }
}

#[cfg_attr(
    all(not(feature = "wad-doom"), not(feature = "wad-sigil")),
    ignore = "needs doom.wad + sigil.wad (~/doom/)"
)]
#[test]
fn sigil_all_maps_winding_contract() {
    for name in SIGIL_MAPS {
        assert_winding_contract(
            &load_map_with_pwad(&doom_wad_path(), &sigil_wad_path(), name),
            name,
        );
    }
}

// ---------------------------------------------------------------------------
// Leaf/range integrity: leaf ranges tile the polygon array exactly; shared
// lists point only at two-sided walls owned by other leaves; per-sector event
// tables cover exactly that sector's flats classified by winding.
// ---------------------------------------------------------------------------

fn assert_leaf_integrity(map: &LevelData, name: &str) {
    let bsp3d = &map.bsp_3d;
    let n = bsp3d.polygons.len();

    let mut cursor = 0;
    for (ss, leaf) in bsp3d.leaves.iter().enumerate() {
        assert_eq!(
            leaf.poly_start, cursor,
            "{name} leaf {ss}: ranges must tile the polygon array"
        );
        cursor += leaf.poly_count;
        assert!(cursor <= n, "{name} leaf {ss}: range out of bounds");

        let own_start = leaf.poly_start;
        let own_end = own_start + leaf.poly_count;
        let shared = &bsp3d.shared_walls[leaf.shared_start..leaf.shared_start + leaf.shared_count];
        for &gi in shared {
            assert!(gi < n, "{name} leaf {ss}: shared index out of bounds");
            assert!(
                !(own_start..own_end).contains(&gi),
                "{name} leaf {ss}: shared wall {gi} is the leaf's own polygon"
            );
            assert!(
                !bsp3d.poly_is_flat(gi),
                "{name} leaf {ss}: shared entry {gi} is a flat"
            );
            assert!(
                bsp3d.poly_flags[gi].contains(PolyFlags::TWO_SIDED),
                "{name} leaf {ss}: shared wall {gi} is not two-sided"
            );
        }
    }
    assert_eq!(cursor, n, "{name}: leaf ranges must cover all polys");

    // Event tables: every flat appears in exactly its sector's table, on the
    // side its winding says.
    let mut seen = vec![false; n];
    for sid in 0..bsp3d.sector_floor_polys.len() {
        for (&table_gi, is_floor) in bsp3d.sector_floor_polys[sid]
            .iter()
            .map(|gi| (gi, true))
            .chain(bsp3d.sector_ceiling_polys[sid].iter().map(|gi| (gi, false)))
        {
            let gi = table_gi;
            assert!(!seen[gi], "{name}: flat {gi} listed twice in event tables");
            seen[gi] = true;
            assert!(bsp3d.poly_is_flat(gi), "{name}: table entry {gi} not flat");
            assert_eq!(
                bsp3d.polygons[gi].sector.num as usize, sid,
                "{name}: flat {gi} in the wrong sector table"
            );
            assert_eq!(
                bsp3d.polygons[gi].normal.z > 0.0,
                is_floor,
                "{name}: flat {gi} on the wrong floor/ceiling table"
            );
        }
    }
    for (gi, seen) in seen.iter().enumerate() {
        if bsp3d.poly_is_flat(gi) {
            assert!(*seen, "{name}: flat {gi} missing from event tables");
        }
    }
}

#[test]
fn doom1_all_maps_leaf_integrity() {
    for name in DOOM1_MAPS {
        assert_leaf_integrity(&load_map(&doom1_wad_path(), name), name);
    }
}

#[cfg_attr(
    all(not(feature = "wad-doom"), not(feature = "wad-sigil")),
    ignore = "needs doom.wad + sigil.wad (~/doom/)"
)]
#[test]
fn sigil_all_maps_leaf_integrity() {
    for name in SIGIL_MAPS {
        assert_leaf_integrity(
            &load_map_with_pwad(&doom_wad_path(), &sigil_wad_path(), name),
            name,
        );
    }
}

// ---------------------------------------------------------------------------
// Surface cache: resolved textures/flags equal values derived independently
// from sidedef/linedef data through the polygon's MapPtrs.
// ---------------------------------------------------------------------------

fn assert_surface_cache(map: &LevelData, name: &str) {
    let bsp3d = &map.bsp_3d;
    for gi in 0..bsp3d.polygons.len() {
        let p = &bsp3d.polygons[gi];
        let flags = bsp3d.poly_flags[gi];
        if bsp3d.poly_is_flat(gi) {
            let expected = if p.normal.z > 0.0 {
                p.sector.floorpic
            } else {
                p.sector.ceilingpic
            } as u32;
            assert_eq!(bsp3d.poly_tex[gi], expected, "{name} flat {gi} poly_tex");
            continue;
        }
        let ld = p.linedef.as_ref().expect("wall linedef");
        assert_eq!(
            flags.contains(PolyFlags::TWO_SIDED),
            p.back_sidedef.is_some(),
            "{name} wall {gi} TWO_SIDED"
        );
        assert_eq!(
            flags.contains(PolyFlags::TRANSLUCENT),
            ld.special == 260 && !flags.contains(PolyFlags::SKY_FILLER),
            "{name} wall {gi} TRANSLUCENT"
        );
        if flags.contains(PolyFlags::SKY_FILLER) {
            assert!(flags.contains(PolyFlags::SKY), "{name} filler {gi} SKY");
            assert_eq!(bsp3d.poly_tex[gi], NO_INDEX, "{name} filler {gi} tex");
            continue;
        }
        let sd = p.sidedef.as_ref().expect("wall sidedef");
        let slot = bsp3d.wall_slot(gi).expect("wall slot");
        let slot_tex = |sd: &level::SideDef| match slot {
            WallSlot::Upper => sd.toptexture,
            WallSlot::Lower => sd.bottomtexture,
            WallSlot::Middle => sd.midtexture,
        };
        let front = slot_tex(sd).map_or(NO_INDEX, |t| t as u32);
        let back = match (&p.back_sidedef, slot) {
            (Some(bsd), WallSlot::Upper | WallSlot::Lower) => {
                slot_tex(bsd).map_or(NO_INDEX, |t| t as u32)
            }
            _ => front,
        };
        // poly_tex carries the side facing the viewer; an inverted mover wall
        // presents its back sidedef.
        let (facing, away) = if flags.contains(PolyFlags::FLIPPED) {
            (back, front)
        } else {
            (front, back)
        };
        assert_eq!(bsp3d.poly_tex[gi], facing, "{name} wall {gi} poly_tex");
        assert_eq!(bsp3d.poly_back_tex[gi], away, "{name} wall {gi} away tex");
        assert_eq!(
            flags.contains(PolyFlags::MASKED_MIDDLE),
            slot == WallSlot::Middle && p.back_sidedef.is_some(),
            "{name} wall {gi} MASKED_MIDDLE"
        );
    }
}

#[test]
fn doom1_all_maps_surface_cache() {
    for name in DOOM1_MAPS {
        assert_surface_cache(&load_map(&doom1_wad_path(), name), name);
    }
}

// ---------------------------------------------------------------------------
// UV anchors — verified against doom-og-src r_segs.c. The independent
// derivation below re-states the OG texturemid table; every wall's baked UV
// must match it, at rest and after a mover re-resolve.
// ---------------------------------------------------------------------------

fn assert_uv_anchors(map: &LevelData, name: &str, tex_height: &dyn Fn(u32) -> f32) {
    let bsp3d = &map.bsp_3d;
    let verts = &bsp3d.vertices;
    let mut masked_unpeg_seen = 0usize;

    for gi in 0..bsp3d.polygons.len() {
        if bsp3d.poly_is_flat(gi) || bsp3d.poly_flags[gi].contains(PolyFlags::SKY_FILLER) {
            continue;
        }
        let p = &bsp3d.polygons[gi];
        let ld = p.linedef.as_ref().unwrap();
        let sd = p.sidedef.as_ref().unwrap();
        let slot = bsp3d.wall_slot(gi).unwrap();
        let poly_verts = bsp3d.poly_vert_indices(gi);
        // Topological quad edges ([b0, b1, t1, t0]) — inversion-aware.
        let bottom_z = verts[poly_verts[0]].z;
        let top_z = verts[poly_verts[2]].z;

        let tex = if bsp3d.poly_tex[gi] != NO_INDEX {
            bsp3d.poly_tex[gi]
        } else {
            bsp3d.poly_back_tex[gi]
        };
        let tex_h = if tex != NO_INDEX {
            tex_height(tex)
        } else {
            0.0
        };
        let unpeg_top = ld.flags.contains(LineDefFlags::UnpegTop);
        let unpeg_bottom = ld.flags.contains(LineDefFlags::UnpegBottom);
        // OG r_segs.c texturemid (texture-top world z):
        //   upper:  unpeg-top → worldtop (quad top); else back ceil + texheight
        //   lower:  unpeg-bottom → FRONT CEILING; else back floor (quad top)
        //   middle: unpeg-bottom → quad bottom + texheight; else quad top
        let anchor = match slot {
            WallSlot::Upper => {
                if unpeg_top {
                    top_z
                } else {
                    bottom_z + tex_h
                }
            }
            WallSlot::Lower => {
                if unpeg_bottom {
                    sd.sector.ceilingheight.to_f32()
                } else {
                    top_z
                }
            }
            WallSlot::Middle => {
                if unpeg_bottom {
                    bottom_z + tex_h
                } else {
                    top_z
                }
            }
        };
        if slot == WallSlot::Middle && unpeg_bottom && p.back_sidedef.is_some() {
            masked_unpeg_seen += 1;
        }

        let v0 = verts[poly_verts[0]];
        let v1 = verts[poly_verts[1]];
        let dxy = Vec2::new(v1.x - v0.x, v1.y - v0.y);
        if dxy.length_squared() < 1e-6 {
            continue;
        }
        let dir = dxy.normalize();
        let x_off = f32::from(sd.textureoffset) + p.seg_offset;
        let y_off = f32::from(sd.rowoffset);
        let (s, _) = bsp3d.poly_vertex_range[gi];
        for (i, &vi) in poly_verts.iter().enumerate() {
            let world = verts[vi];
            let expect_u = (world.x - v0.x) * dir.x + (world.y - v0.y) * dir.y + x_off;
            let expect_v = anchor - world.z + y_off;
            let [u, v] = bsp3d.poly_vertex_uv[s + i];
            assert!(
                (u - expect_u).abs() < 1e-3 && (v - expect_v).abs() < 1e-3,
                "{name} wall {gi} ({slot:?}) vertex {i}: uv=({u},{v}) expected ({expect_u},{expect_v})"
            );
        }
    }
    let _ = masked_unpeg_seen;
}

/// Texture heights straight from the WAD's TEXTURE1/2 order — the same source
/// `LevelData::load` feeds the parse step, derived independently here.
fn wad_tex_heights(wad_path: &std::path::Path) -> Vec<f32> {
    let wad = wad::WadData::new(wad_path);
    let mut tex_order: Vec<wad::types::WadTexture> = wad.texture_iter("TEXTURE1").collect();
    if wad.lump_exists("TEXTURE2") {
        tex_order.extend(wad.texture_iter("TEXTURE2"));
    }
    tex_order.iter().map(|t| t.height as f32).collect()
}

#[test]
fn doom1_all_maps_uv_anchors() {
    let heights = wad_tex_heights(&doom1_wad_path());
    for name in DOOM1_MAPS {
        let map = load_map(&doom1_wad_path(), name);
        assert_uv_anchors(&map, name, &|t| heights[t as usize]);
    }
}

#[test]
fn e1m1_uv_anchor_stable_across_mover_round_trip() {
    let heights = wad_tex_heights(&doom1_wad_path());
    let mut map = load_map(&doom1_wad_path(), "E1M1");
    let sector_id = map
        .bsp_3d
        .sector_wall_polys
        .iter()
        .position(|w| !w.is_empty())
        .expect("a sector with walls");
    let h = map.sectors[sector_id].floorheight.to_f32();

    let uv_before = map.bsp_3d.poly_vertex_uv.clone();
    move_sector_surface(&mut map, sector_id, MovementType::Floor, h - 32.0);
    // Mid-travel UV still matches the OG anchor derivation at the new height.
    assert_uv_anchors(&map, "E1M1@-32", &|t| heights[t as usize]);
    move_sector_surface(&mut map, sector_id, MovementType::Floor, h);
    assert_eq!(
        uv_before, map.bsp_3d.poly_vertex_uv,
        "mover round trip must restore the exact baked UV"
    );
}

// ---------------------------------------------------------------------------
// Determinism: loading the same map twice yields identical runtime data.
// ---------------------------------------------------------------------------

#[test]
fn e1m1_build_is_deterministic() {
    let a = load_map(&doom1_wad_path(), "E1M1");
    let b = load_map(&doom1_wad_path(), "E1M1");
    let (ba, bb) = (&a.bsp_3d, &b.bsp_3d);
    assert_eq!(ba.vertices, bb.vertices);
    assert_eq!(ba.poly_verts, bb.poly_verts);
    assert_eq!(ba.poly_vertex_range, bb.poly_vertex_range);
    assert_eq!(ba.poly_vertex_uv, bb.poly_vertex_uv);
    assert_eq!(ba.triangles, bb.triangles);
    assert_eq!(ba.poly_tex, bb.poly_tex);
    assert_eq!(ba.poly_back_tex, bb.poly_back_tex);
    assert_eq!(ba.poly_flags, bb.poly_flags);
    assert_eq!(ba.shared_walls, bb.shared_walls);
    assert_eq!(ba.sector_floor_polys, bb.sector_floor_polys);
    assert_eq!(ba.sector_ceiling_polys, bb.sector_ceiling_polys);
    assert_eq!(ba.sector_wall_polys, bb.sector_wall_polys);
    assert_eq!(ba.linedef_wall_polys, bb.linedef_wall_polys);
}

// ---------------------------------------------------------------------------
// Wall-slot derivation on known shapes: one-sided middles, door at rest.
// ---------------------------------------------------------------------------

#[test]
fn e1m1_slot_derivation_known_cases() {
    let map = load_map(&doom1_wad_path(), "E1M1");
    let bsp3d = &map.bsp_3d;
    let mut one_sided = 0usize;
    let mut uppers = 0usize;
    let mut lowers = 0usize;

    for gi in 0..bsp3d.polygons.len() {
        if bsp3d.poly_is_flat(gi) || bsp3d.poly_flags[gi].contains(PolyFlags::SKY_FILLER) {
            continue;
        }
        let p = &bsp3d.polygons[gi];
        let slot = bsp3d.wall_slot(gi).unwrap();
        match slot {
            WallSlot::Middle => {
                if p.back_sidedef.is_none() {
                    one_sided += 1;
                    // One-sided walls span their sector's full height.
                    let (s, e) = bsp3d.poly_vertex_range[gi];
                    let zs: Vec<f32> = bsp3d.poly_verts[s..e]
                        .iter()
                        .map(|&v| bsp3d.vertices[v].z)
                        .collect();
                    let lo = zs.iter().fold(f32::MAX, |a, &b| a.min(b));
                    let hi = zs.iter().fold(f32::MIN, |a, &b| a.max(b));
                    assert_eq!(lo, p.sector.floorheight.to_f32(), "wall {gi} bottom");
                    assert_eq!(hi, p.sector.ceilingheight.to_f32(), "wall {gi} top");
                }
            }
            WallSlot::Upper => uppers += 1,
            WallSlot::Lower => lowers += 1,
        }
    }
    assert!(one_sided > 100, "E1M1 has many one-sided walls");
    assert!(uppers > 10, "E1M1 has upper walls");
    assert!(lowers > 10, "E1M1 has lower walls");
}
