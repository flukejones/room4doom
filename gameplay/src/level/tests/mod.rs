use std::path::PathBuf;

use crate::{MapData, PicData};
use wad::WadData;

pub const DOOM_WAD: &str = "/Users/lukejones/DOOM/doom.wad";
pub const SIGIL_WAD: &str = "/Users/lukejones/DOOM/sigil.wad";
pub const SIGIL2_WAD: &str = "/Users/lukejones/DOOM/sigil2.wad";

/// Load a map from a single WAD file with all fixups applied.
/// Returns canonical post-fixup `MapData`.
pub fn load_map(wad_path: &str, map_name: &str) -> MapData {
    let wad = WadData::new(&PathBuf::from(wad_path));
    let pic_data = PicData::init(&wad);
    let mut map = MapData::default();
    map.load(
        map_name,
        |name| pic_data.flat_num_for_name(name),
        &wad,
        None,
    );
    map
}

/// Load a map from a base WAD with an additional PWAD merged.
/// Returns canonical post-fixup `MapData`.
pub fn load_map_with_pwad(base_wad: &str, pwad: &str, map_name: &str) -> MapData {
    let mut wad = WadData::new(&PathBuf::from(base_wad));
    wad.add_file(pwad.into());
    let pic_data = PicData::init(&wad);
    let mut map = MapData::default();
    map.load(
        map_name,
        |name| pic_data.flat_num_for_name(name),
        &wad,
        None,
    );
    map
}

pub mod bsp3d_e1m1_mover_test;
pub mod bsp3d_e1m1_tests;
pub mod bsp3d_e1m2_door_test;
pub mod bsp3d_e1m2_test;
pub mod bsp3d_e1m3_stairs_test;
pub mod bsp3d_e5m1_mover_test;
pub mod bsp3d_e6m1_sigil2_test;
pub mod map_data_tests;
pub mod pvs_tests;
