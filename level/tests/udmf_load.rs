//! Engine UDMF load: a `TEXTMAP` map loads through `LevelData::load`, its
//! sector slope plane reaches the engine `Sector`, and the 3D-BSP floor
//! geometry sits on that plane.

use std::io::Write as _;

use level::LevelData;
use math::FixedT;
use wad::WadData;

const SLOPED_FLOOR: &str = include_str!("../../data/test_files/udmf/sloped_floor.textmap");
const FLAT_ROOM: &str = include_str!("../../data/test_files/udmf/flat_room.textmap");

/// Write a minimal PWAD with one UDMF map: marker, TEXTMAP, ENDMAP.
fn write_udmf_wad(path: &std::path::Path, map: &str, textmap: &[u8]) {
    let lumps: [(&str, &[u8]); 3] = [(map, &[]), ("TEXTMAP", textmap), ("ENDMAP", &[])];

    let data_size: u32 = lumps.iter().map(|(_, d)| d.len() as u32).sum();
    let dir_offset = 12 + data_size;

    let mut buf = Vec::new();
    buf.extend_from_slice(b"PWAD");
    buf.extend_from_slice(&(lumps.len() as i32).to_le_bytes());
    buf.extend_from_slice(&(dir_offset as i32).to_le_bytes());
    for (_, d) in &lumps {
        buf.extend_from_slice(d);
    }
    let mut offset: u32 = 12;
    for (name, d) in &lumps {
        buf.extend_from_slice(&(offset as i32).to_le_bytes());
        buf.extend_from_slice(&(d.len() as i32).to_le_bytes());
        let mut name8 = [0u8; 8];
        name8[..name.len()].copy_from_slice(name.as_bytes());
        buf.extend_from_slice(&name8);
        offset += d.len() as u32;
    }

    let mut f = std::fs::File::create(path).expect("create temp wad");
    f.write_all(&buf).expect("write temp wad");
}

fn load(map: &str, tag: &str, textmap: &str) -> LevelData {
    // Unique per (test, process): the suite runs tests in parallel and two
    // would otherwise race on the same temp WAD path.
    let path = std::env::temp_dir().join(format!("r4d-udmf-{tag}-{}.wad", std::process::id()));
    write_udmf_wad(&path, map, textmap.as_bytes());
    let wad = WadData::new(&path);
    let mut level = LevelData::default();
    level.load(map, |_| None, &wad, None, None);
    std::fs::remove_file(&path).ok();
    level
}

#[test]
fn sloped_floor_reaches_engine_sector() {
    let level = load("MAP01", "sloped", SLOPED_FLOOR);

    assert_eq!(level.sectors.len(), 1, "one sector");
    let sector = &level.sectors[0];
    let plane = sector
        .floor_plane
        .expect("floor plane reached the engine Sector");
    assert_eq!(plane.c, 1.0);
    assert!(sector.ceil_plane.is_none(), "ceiling stays flat");

    // The subsector lookup works on the loaded map.
    let mut level = level;
    let ss = level.point_in_subsector(FixedT::from(128), FixedT::from(128));
    assert_eq!(ss.sector.num, 0);

    // A 3D-BSP floor vertex sits on the slope plane (z = 0.25*x, rising).
    let bsp = &level.bsp_3d;
    let mut saw_raised = false;
    for v in &bsp.vertices {
        if (v.z - plane.z_at(v.x, v.y)).abs() < 1e-3 && v.z > 1.0 {
            saw_raised = true;
        }
    }
    assert!(saw_raised, "a sloped floor vertex was lifted above z=0");
}

#[test]
fn flat_udmf_map_has_no_slope() {
    let level = load("MAP01", "flat", FLAT_ROOM);
    assert_eq!(level.sectors.len(), 1);
    assert!(level.sectors[0].floor_plane.is_none());
    assert!(level.sectors[0].ceil_plane.is_none());
}
