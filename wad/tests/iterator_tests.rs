use test_utils::{doom_wad_path, doom1_wad_path, doom2_wad_path};
use wad::WadData;
use wad::types::*;

#[test]
fn palette_iter() {
    let wad = WadData::new(&doom1_wad_path());
    let count = wad.lump_iter::<WadPalette>("PLAYPAL").count();
    assert_eq!(count, 14);

    let palettes: Vec<WadPalette> = wad.lump_iter::<WadPalette>("PLAYPAL").collect();

    assert_eq!(colour_r(palettes[0].0[0]), 0);
    assert_eq!(colour_g(palettes[0].0[0]), 0);
    assert_eq!(colour_b(palettes[0].0[0]), 0);

    assert_eq!(colour_r(palettes[0].0[1]), 31);
    assert_eq!(colour_g(palettes[0].0[1]), 23);
    assert_eq!(colour_b(palettes[0].0[1]), 11);

    assert_eq!(colour_r(palettes[0].0[119]), 67);
    assert_eq!(colour_g(palettes[0].0[119]), 147);
    assert_eq!(colour_b(palettes[0].0[119]), 55);

    assert_eq!(colour_r(palettes[4].0[119]), 150);
    assert_eq!(colour_g(palettes[4].0[119]), 82);
    assert_eq!(colour_b(palettes[4].0[119]), 31);
}

#[test]
fn pnames_iter() {
    let wad = WadData::new(&doom1_wad_path());
    let mut iter = wad.pnames_iter();

    assert_eq!(iter.next().unwrap(), "WALL00_3");
    assert_eq!(iter.next().unwrap(), "W13_1");
    assert_eq!(iter.next().unwrap(), "DOOR2_1");
    assert_eq!(wad.pnames_iter().count(), 350);
}

#[test]
fn texture_iter() {
    let wad = WadData::new(&doom1_wad_path());
    let mut iter = wad.texture_iter("TEXTURE1");

    let next = iter.next().unwrap();
    assert_eq!(next.name, "AASTINKY");
    assert_eq!(next.width, 24);
    assert_eq!(next.height, 72);
    assert_eq!(next.patches.len(), 2);
    assert_eq!(next.patches[0].origin_x, 0);
    assert_eq!(next.patches[0].origin_y, 0);
    assert_eq!(next.patches[0].patch_index, 0);

    assert_eq!(iter.next().unwrap().name, "BIGDOOR1");
    assert_eq!(iter.next().unwrap().name, "BIGDOOR2");
    assert_eq!(wad.texture_iter("TEXTURE1").count(), 125);
}

#[test]
fn patches_doom1_iter() {
    let wad = WadData::new(&doom1_wad_path());
    assert_eq!(wad.patches_iter().count(), 165);
}

#[test]
#[ignore = "doom.wad is commercial"]
fn patches_doom_iter_commercial() {
    // patches_iter counts all non-empty lumps in P_START/P_END sections.
    // PNAMES has 351 entries; 2 extra lumps exist in the section but are
    // not referenced by PNAMES. Harmless — pic-data looks up by name.
    let wad = WadData::new(&doom_wad_path());
    assert_eq!(wad.patches_iter().count(), 353);
}

#[test]
#[ignore = "doom2.wad is commercial"]
fn patches_doom2_iter() {
    // PNAMES has 469 entries; 1 extra lump in the section is not
    // referenced by PNAMES. Harmless — pic-data looks up by name.
    let wad = WadData::new(&doom2_wad_path());
    assert_eq!(wad.patches_iter().count(), 470);
}

#[test]
#[ignore = "doom2.wad is commercial"]
fn w94_1_commercial() {
    let wad = WadData::new(&doom2_wad_path());
    let lump = wad.get_lump("W94_1").expect("W94_1");
    assert_eq!(lump.name, "W94_1");
    let lump = wad.get_lump("w94_1").expect("w94_1");
    assert_eq!(lump.name, "W94_1");
}

#[test]
#[ignore = "doom2.wad is commercial"]
fn pnames_doom2_iter_commercial() {
    let wad = WadData::new(&doom2_wad_path());
    let mut iter = wad.pnames_iter();

    assert_eq!(iter.next().unwrap(), "BODIES");
    assert_eq!(iter.next().unwrap(), "RW22_1");
    assert_eq!(iter.next().unwrap(), "RW22_2");
    assert_eq!(wad.pnames_iter().count(), 469);
}

#[test]
fn colormap_iter() {
    let wad = WadData::new(&doom1_wad_path());
    let mut iter = wad.colourmap_iter();

    assert_eq!(iter.next().unwrap(), 0);
    assert_eq!(iter.next().unwrap(), 1);
    assert_eq!(iter.next().unwrap(), 2);

    assert_eq!(wad.colourmap_iter().count(), 8704);
    assert_eq!(wad.colourmap_iter().count() / 256, 34);

    let colourmap: Vec<u8> = wad.colourmap_iter().collect();
    assert_eq!(colourmap[256], 0);
    assert_eq!(colourmap[8 * 256], 0);
    assert_eq!(colourmap[16 * 256], 0);
    assert_eq!(colourmap[256 + 32], 33);
    assert_eq!(colourmap[8 * 256 + 32], 36);
    assert_eq!(colourmap[16 * 256 + 32], 15);
    assert_eq!(colourmap[256 + 48], 49);
    assert_eq!(colourmap[8 * 256 + 48], 89);
    assert_eq!(colourmap[16 * 256 + 48], 98);
    assert_eq!(colourmap[256 + 64], 64);
    assert_eq!(colourmap[8 * 256 + 64], 69);
    assert_eq!(colourmap[16 * 256 + 64], 74);
}

#[test]
fn flats_doom1() {
    let wad = WadData::new(&doom1_wad_path());
    assert!(wad.get_lump("NUKAGE3").is_some());
    assert_eq!(wad.flats_iter().count(), 54);
}

#[ignore = "doom.wad is commercial"]
#[test]
fn flats_doom_commercial() {
    let wad = WadData::new(&doom_wad_path());
    assert!(wad.get_lump("NUKAGE3").is_some());
    assert_eq!(wad.flats_iter().count(), 107);
}

#[ignore = "doom2.wad is commercial"]
#[test]
fn flats_doom2_commercial() {
    let wad = WadData::new(&doom2_wad_path());
    assert!(wad.get_lump("NUKAGE3").is_some());
    assert_eq!(wad.flats_iter().count(), 147);
}

/// Cross-reference WAD parsing against omgifol-generated JSON
mod cross_ref {
    use test_utils::doom1_wad_path;
    use wad::types::*;
    use wad::{MapLump, WadData};

    fn load_ref(path: &str) -> Vec<serde_json::Value> {
        let data = std::fs::read_to_string(path).expect(path);
        serde_json::from_str(&data).expect("parse JSON")
    }

    #[test]
    fn e1m1_vertexes() {
        let wad = WadData::new(&doom1_wad_path());
        let verts: Vec<_> = wad
            .map_iter::<WadVertex>("E1M1", MapLump::Vertexes)
            .collect();
        let reference = load_ref(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../data/test_files/vanilla/doom1_e1m1_vertexes.json"
        ));
        assert_eq!(verts.len(), reference.len());
        for (i, (v, r)) in verts.iter().zip(reference.iter()).enumerate() {
            assert_eq!(
                v.x as i16,
                r["x"].as_i64().unwrap() as i16,
                "vertex {} x",
                i
            );
            assert_eq!(
                v.y as i16,
                r["y"].as_i64().unwrap() as i16,
                "vertex {} y",
                i
            );
        }
    }

    #[test]
    fn e1m1_linedefs() {
        let wad = WadData::new(&doom1_wad_path());
        let lines: Vec<_> = wad
            .map_iter::<WadLineDef>("E1M1", MapLump::LineDefs)
            .collect();
        let reference = load_ref(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../data/test_files/vanilla/doom1_e1m1_linedefs.json"
        ));
        assert_eq!(lines.len(), reference.len());
        for (i, (l, r)) in lines.iter().zip(reference.iter()).enumerate() {
            assert_eq!(
                l.start_vertex,
                r["vx_a"].as_u64().unwrap() as u16,
                "linedef {} start",
                i
            );
            assert_eq!(
                l.end_vertex,
                r["vx_b"].as_u64().unwrap() as u16,
                "linedef {} end",
                i
            );
            assert_eq!(
                l.front_sidedef,
                r["front"].as_u64().unwrap() as u16,
                "linedef {} front",
                i
            );
            let back = r["back"].as_u64().unwrap() as u16;
            assert_eq!(
                l.back_sidedef,
                if back == 0xFFFF { None } else { Some(back) },
                "linedef {} back",
                i
            );
            assert_eq!(
                l.flags,
                r["flags"].as_u64().unwrap() as u16,
                "linedef {} flags",
                i
            );
            assert_eq!(
                l.special,
                r["action"].as_i64().unwrap() as i16,
                "linedef {} action",
                i
            );
            assert_eq!(
                l.sector_tag,
                r["tag"].as_i64().unwrap() as i16,
                "linedef {} tag",
                i
            );
        }
    }

    #[test]
    fn e1m1_sidedefs() {
        let wad = WadData::new(&doom1_wad_path());
        let sides: Vec<_> = wad
            .map_iter::<WadSideDef>("E1M1", MapLump::SideDefs)
            .collect();
        let reference = load_ref(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../data/test_files/vanilla/doom1_e1m1_sidedefs.json"
        ));
        assert_eq!(sides.len(), reference.len());
        fn norm(s: &str) -> &str {
            let s = s.trim_end_matches('\0');
            if s == "-" { "" } else { s }
        }
        for (i, (s, r)) in sides.iter().zip(reference.iter()).enumerate() {
            assert_eq!(
                s.x_offset,
                r["off_x"].as_i64().unwrap() as i16,
                "sidedef {} off_x",
                i
            );
            assert_eq!(
                s.y_offset,
                r["off_y"].as_i64().unwrap() as i16,
                "sidedef {} off_y",
                i
            );
            assert_eq!(
                s.upper_tex.as_str(),
                norm(r["tx_up"].as_str().unwrap()),
                "sidedef {} tx_up",
                i
            );
            assert_eq!(
                s.lower_tex.as_str(),
                norm(r["tx_low"].as_str().unwrap()),
                "sidedef {} tx_low",
                i
            );
            assert_eq!(
                s.middle_tex.as_str(),
                norm(r["tx_mid"].as_str().unwrap()),
                "sidedef {} tx_mid",
                i
            );
            assert_eq!(
                s.sector,
                r["sector"].as_i64().unwrap() as i16,
                "sidedef {} sector",
                i
            );
        }
    }

    #[test]
    fn e1m1_sectors() {
        let wad = WadData::new(&doom1_wad_path());
        let sectors: Vec<_> = wad
            .map_iter::<WadSector>("E1M1", MapLump::Sectors)
            .collect();
        let reference = load_ref(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../data/test_files/vanilla/doom1_e1m1_sectors.json"
        ));
        assert_eq!(sectors.len(), reference.len());
        for (i, (s, r)) in sectors.iter().zip(reference.iter()).enumerate() {
            assert_eq!(
                s.floor_height,
                r["z_floor"].as_i64().unwrap() as i16,
                "sector {} z_floor",
                i
            );
            assert_eq!(
                s.ceil_height,
                r["z_ceil"].as_i64().unwrap() as i16,
                "sector {} z_ceil",
                i
            );
            assert_eq!(
                s.floor_tex,
                r["tx_floor"].as_str().unwrap().trim_end_matches('\0'),
                "sector {} tx_floor",
                i
            );
            assert_eq!(
                s.ceil_tex,
                r["tx_ceil"].as_str().unwrap().trim_end_matches('\0'),
                "sector {} tx_ceil",
                i
            );
            assert_eq!(
                s.light_level,
                r["light"].as_i64().unwrap() as i16,
                "sector {} light",
                i
            );
            assert_eq!(
                s.kind,
                r["type"].as_i64().unwrap() as i16,
                "sector {} type",
                i
            );
            assert_eq!(s.tag, r["tag"].as_i64().unwrap() as i16, "sector {} tag", i);
        }
    }

    #[test]
    fn e1m1_things() {
        let wad = WadData::new(&doom1_wad_path());
        let things: Vec<_> = wad.map_iter::<WadThing>("E1M1", MapLump::Things).collect();
        let reference = load_ref(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../data/test_files/vanilla/doom1_e1m1_things.json"
        ));
        assert_eq!(things.len(), reference.len());
        for (i, (t, r)) in things.iter().zip(reference.iter()).enumerate() {
            assert_eq!(t.x, r["x"].as_i64().unwrap() as i16, "thing {} x", i);
            assert_eq!(t.y, r["y"].as_i64().unwrap() as i16, "thing {} y", i);
            assert_eq!(
                t.angle,
                r["angle"].as_i64().unwrap() as i16,
                "thing {} angle",
                i
            );
            assert_eq!(
                t.kind,
                r["type"].as_i64().unwrap() as i16,
                "thing {} type",
                i
            );
            assert_eq!(
                t.flags,
                r["flags"].as_i64().unwrap() as i16,
                "thing {} flags",
                i
            );
        }
    }
}

/// PWAD sprites override IWAD sprites when merged.
#[test]
#[ignore = "doom2.wad and Eviternity.wad can't be included in git"]
fn pwad_sprites_override_iwad() {
    use test_utils::{doom2_wad_path, eviternity_wad_path};

    let mut wad = WadData::new(&doom2_wad_path());
    wad.add_file(eviternity_wad_path());

    let sprites: Vec<_> = wad.sprites_iter().collect();
    let tre1: Vec<_> = sprites.iter().filter(|s| s.name == "TRE1A0").collect();
    assert!(!tre1.is_empty(), "TRE1A0 should be found in merged WAD");
    assert!(
        tre1.iter().any(|s| s.width > 100),
        "PWAD TRE1A0 must be present"
    );
}

/// F_SKY1 flat survives when PWAD with FF_START is merged.
#[test]
#[ignore = "doom2.wad and Eviternity.wad can't be included in git"]
fn pwad_flats_preserve_iwad_fsky1() {
    use test_utils::{doom2_wad_path, eviternity_wad_path};

    let mut wad = WadData::new(&doom2_wad_path());
    wad.add_file(eviternity_wad_path());

    let flats: Vec<_> = wad.flats_iter().collect();
    assert!(
        flats.iter().any(|f| f.name == "F_SKY1"),
        "F_SKY1 must survive PWAD merge"
    );
}
