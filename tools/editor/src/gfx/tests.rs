use editor_core::Name8;
use wad::WadData;
use wad::types::WadPalette;

use super::GfxCache;
use super::palette::flat_to_rgba;
use crate::assets::{EditorAssets, FlatPic};

fn test_palette() -> WadPalette {
    let mut pal = WadPalette([wad::types::BLACK; 256]);
    pal.0[1] = 0xff_aa_bb_cc;
    pal.0[2] = 0xff_11_22_33;
    pal
}

#[test]
fn flat_is_row_major_passthrough() {
    let mut data = [1u16; 64 * 64];
    data[64] = 2; // first pixel of row 1
    let pic = FlatPic {
        data,
        width: 64,
        height: 64,
    };
    let buf = flat_to_rgba(&pic, &test_palette());
    let bytes = buf.as_bytes();
    assert_eq!(&bytes[0..4], &[0xaa, 0xbb, 0xcc, 0xff]);
    assert_eq!(&bytes[64 * 4..64 * 4 + 4], &[0x11, 0x22, 0x33, 0xff]);
}

#[test]
fn cache_serves_doom1_assets() {
    let wad = WadData::new(&test_utils::doom1_wad_path());
    let assets = EditorAssets::load(&[test_utils::doom1_wad_path()], &wad, None);
    let mut cache = GfxCache::new(&assets);
    assert!(assets.textures().len() > 100);
    assert!(assets.iwad_flats().len() > 30);

    let startan = assets
        .textures()
        .iter()
        .position(|t| t.name.as_str() == "STARTAN3")
        .expect("shareware has STARTAN3");
    let img = cache.texture_image(&assets, &wad, startan);
    assert!(img.size().width > 0);

    let floor = assets
        .iwad_flat_num(&Name8::new("FLOOR4_8").expect("valid name"))
        .expect("shareware has FLOOR4_8");
    let img = cache.flat_image(&assets, floor);
    assert_eq!(img.size().width, 64);
}
