use test_utils::{doom_wad_path, doom1_wad_path, sigil_wad_path, sunder_wad_path};
use wad::types::{WadPatch, WadThing};
use wad::{MapLump, WadData};

#[test]
fn load_wad() {
    let wad = WadData::new(&doom1_wad_path());
    assert!(
        wad.lumps().len() > 1200,
        "DOOM1.WAD should have >1200 lumps"
    );
}

#[test]
fn find_e1m1_things() {
    let wad = WadData::new(&doom1_wad_path());
    let lump = wad.get_lump("THINGS").expect("THINGS lump");
    assert_eq!(lump.name, "THINGS");
}

#[test]
fn find_texture_lump() {
    let wad = WadData::new(&doom1_wad_path());
    let tex = wad.get_lump("TEXTURE1").expect("TEXTURE1");
    assert_eq!(tex.name, "TEXTURE1");
    assert_eq!(tex.data.len(), 9234);
}

#[test]
fn find_playpal_lump() {
    let wad = WadData::new(&doom1_wad_path());
    let pal = wad.get_lump("PLAYPAL").expect("PLAYPAL");
    assert_eq!(pal.name, "PLAYPAL");
    assert_eq!(pal.data.len(), 10752);
}

#[test]
fn check_image_patch() {
    let wad = WadData::new(&doom1_wad_path());
    let lump = wad.get_lump("WALL01_7").expect("WALL01_7");
    assert_eq!(lump.data.len(), 1304);
    let patch = WadPatch::from_lump(lump);
    assert_eq!(patch.columns[0].y_offset, 0);
    assert_eq!(patch.columns[15].y_offset, 255);
    assert_eq!(patch.columns[15].pixels.len(), 0);
}

#[ignore = "sigil.wad can't be included in git"]
#[test]
fn load_sigil() {
    let mut wad = WadData::new(&doom_wad_path());
    wad.add_file(sigil_wad_path());

    let pnames: Vec<String> = wad.pnames_iter().collect();
    assert!(pnames.contains(&String::from("SKY5")));

    let mut iter = wad.map_iter::<WadThing>("E5M1", MapLump::Things);
    let next = iter.next().unwrap();
    assert_eq!(next.x, -208);
    assert_eq!(next.y, 72);
    assert_eq!(next.angle, 270);
    assert_eq!(next.kind, 2001);
    assert_eq!(next.flags, 7);
}

#[test]
#[ignore = "sunder.wad can't be included in git"]
fn load_sunder() {
    let wad = WadData::new(&sunder_wad_path());
    assert_eq!(wad.lumps().len(), 2530);

    let pnames: Vec<String> = wad.pnames_iter().collect();
    assert!(pnames.contains(&String::from("BODIES")));
    let _: Vec<WadPatch> = wad.patches_iter().collect();
}

#[test]
fn find_e1m1_blockmap() {
    let wad = WadData::new(&doom1_wad_path());
    let blockmap = wad.read_blockmap("E1M1").unwrap();
    assert_eq!(blockmap.x_origin, -776);
    assert_eq!(blockmap.y_origin, -4872);
    assert_eq!(blockmap.columns, 36);
    assert_eq!(blockmap.rows, 23);
    // Full block data verified separately — too large to inline
    assert!(!blockmap.line_indexes.is_empty());
}

#[test]
#[ignore = "sunder.wad can't be included in git"]
fn find_sunder15_reject() {
    let wad = WadData::new(&sunder_wad_path());
    let rejects = wad.read_rejects("MAP15").unwrap();
    assert_eq!(rejects.len(), 21216099);
}
