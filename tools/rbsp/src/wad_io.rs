//! WAD file I/O: read input via the `wad` crate, write output as PWAD.
//!
//! Reads geometry from WAD maps, runs `build_bsp`, and writes traditional
//! Doom-format lumps plus an RBSP lump per level.

use std::f64::consts::PI;
use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

use wad::WadData;
use wad::wad::MapLump;

use crate::rbsp_lump::write_rbsp_lump;
use crate::types::*;
use crate::{BspOutput, build_bsp};

const PWAD_MAGIC: &[u8; 4] = b"PWAD";

const LUMP_ORDER: &[&str] = &[
    "THINGS", "LINEDEFS", "SIDEDEFS", "VERTEXES", "SEGS", "SSECTORS", "NODES", "SECTORS", "REJECT",
    "BLOCKMAP", "RBSP",
];

struct OutputLump {
    name: [u8; 8],
    data: Vec<u8>,
}

impl OutputLump {
    fn new(name: &str, data: Vec<u8>) -> Self {
        let mut n = [0u8; 8];
        let len = name.len().min(8);
        n[..len].copy_from_slice(&name.as_bytes()[..len]);
        Self {
            name: n,
            data,
        }
    }

    fn marker(name: &str) -> Self {
        Self::new(name, Vec::new())
    }
}

/// Find all map marker names in a WAD.
pub fn find_maps(wad: &WadData) -> Vec<String> {
    let mut maps = Vec::new();
    for lump in wad.lumps() {
        if is_map_marker(&lump.name) {
            maps.push(lump.name.clone());
        }
    }
    maps
}

/// Load BSP input geometry from a WAD map.
pub fn load_input(wad: &WadData, map_name: &str) -> BspInput {
    BspInput {
        vertices: wad.map_iter(map_name, MapLump::Vertexes).collect(),
        linedefs: wad.map_iter(map_name, MapLump::LineDefs).collect(),
        sidedefs: wad.map_iter(map_name, MapLump::SideDefs).collect(),
        sectors: wad.map_iter(map_name, MapLump::Sectors).collect(),
    }
}

/// Process all maps in a WAD and write a PWAD with rebuilt BSP data.
pub fn process_wad(input_path: &Path, output_path: &Path, options: &BspOptions) -> io::Result<()> {
    let wad = WadData::new(input_path);
    let maps = find_maps(&wad);

    if maps.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "No maps found in WAD",
        ));
    }

    let mut all_lumps: Vec<OutputLump> = Vec::new();

    for map_name in &maps {
        log::info!("Processing {}", map_name);

        let input = load_input(&wad, map_name);
        let num_sectors = input.sectors.len();
        let output = build_bsp(input, options);

        // Level marker.
        all_lumps.push(OutputLump::marker(map_name));

        // Pass through unchanged lumps.
        for &lump_name in &["THINGS", "LINEDEFS", "SIDEDEFS", "SECTORS"] {
            if let Some(data) = find_map_lump_data(&wad, map_name, lump_name) {
                all_lumps.push(OutputLump::new(lump_name, data));
            }
        }

        // Write rebuilt BSP lumps.
        all_lumps.push(OutputLump::new("VERTEXES", write_vertexes(&output)));
        all_lumps.push(OutputLump::new("SEGS", write_segs(&output)));
        all_lumps.push(OutputLump::new("SSECTORS", write_ssectors(&output)));
        all_lumps.push(OutputLump::new("NODES", write_nodes(&output)));
        all_lumps.push(OutputLump::new("REJECT", write_reject(num_sectors)));
        // TODO: BLOCKMAP from blockmap.rs
        all_lumps.push(OutputLump::new("BLOCKMAP", Vec::new()));
        all_lumps.push(OutputLump::new("RBSP", write_rbsp_lump(&output)));
    }

    // Sort lumps within each level to standard order.
    sort_level_lumps(&mut all_lumps);

    write_pwad(output_path, &all_lumps)
}

/// Find raw lump data for a named lump within a map.
fn find_map_lump_data(wad: &WadData, map_name: &str, lump_name: &str) -> Option<Vec<u8>> {
    let lumps = wad.lumps();
    let marker_idx = lumps.iter().rposition(|l| l.name == map_name)?;
    for i in (marker_idx + 1)..lumps.len() {
        if is_map_marker(&lumps[i].name) {
            break;
        }
        if lumps[i].name == lump_name {
            return Some(lumps[i].data.clone());
        }
    }
    None
}

fn is_map_marker(name: &str) -> bool {
    let b = name.as_bytes();
    // E#M# (Doom 1)
    if b.len() >= 4
        && b[0] == b'E'
        && b[1].is_ascii_digit()
        && b[2] == b'M'
        && b[3].is_ascii_digit()
    {
        return true;
    }
    // MAP## (Doom 2)
    if b.len() >= 5
        && b[0] == b'M'
        && b[1] == b'A'
        && b[2] == b'P'
        && b[3].is_ascii_digit()
        && b[4].is_ascii_digit()
    {
        return true;
    }
    false
}

// --- Traditional Doom format writers ---

fn write_vertexes(output: &BspOutput) -> Vec<u8> {
    let mut buf = Vec::with_capacity(output.vertices.len() * 4);
    for v in &output.vertices {
        buf.extend_from_slice(&(v.x as i16).to_le_bytes());
        buf.extend_from_slice(&(v.y as i16).to_le_bytes());
    }
    buf
}

fn write_segs(output: &BspOutput) -> Vec<u8> {
    let mut buf = Vec::with_capacity(output.segs.len() * 12);
    for seg in &output.segs {
        buf.extend_from_slice(&(seg.start as i16).to_le_bytes());
        buf.extend_from_slice(&(seg.end as i16).to_le_bytes());
        let angle = ((seg.angle as f64 * 65536.0 / (2.0 * PI)) as i32) as i16;
        buf.extend_from_slice(&angle.to_le_bytes());
        buf.extend_from_slice(&(seg.linedef as i16).to_le_bytes());
        let side: i16 = match seg.side {
            Side::Front => 0,
            Side::Back => 1,
        };
        buf.extend_from_slice(&side.to_le_bytes());
        buf.extend_from_slice(&(seg.offset as i16).to_le_bytes());
    }
    buf
}

fn write_ssectors(output: &BspOutput) -> Vec<u8> {
    let mut buf = Vec::with_capacity(output.subsectors.len() * 4);
    for ss in &output.subsectors {
        buf.extend_from_slice(&(ss.num_segs as i16).to_le_bytes());
        buf.extend_from_slice(&(ss.first_seg as i16).to_le_bytes());
    }
    buf
}

fn write_nodes(output: &BspOutput) -> Vec<u8> {
    let mut buf = Vec::with_capacity(output.nodes.len() * 28);
    for node in &output.nodes {
        buf.extend_from_slice(&(node.x as i16).to_le_bytes());
        buf.extend_from_slice(&(node.y as i16).to_le_bytes());
        buf.extend_from_slice(&(node.dx as i16).to_le_bytes());
        buf.extend_from_slice(&(node.dy as i16).to_le_bytes());
        // Right bbox: top, bottom, left, right
        write_bbox_i16(&mut buf, &node.bbox_right);
        // Left bbox
        write_bbox_i16(&mut buf, &node.bbox_left);
        buf.extend_from_slice(&(node.child_right as u16).to_le_bytes());
        buf.extend_from_slice(&(node.child_left as u16).to_le_bytes());
    }
    buf
}

fn write_bbox_i16(buf: &mut Vec<u8>, bbox: &BBox) {
    buf.extend_from_slice(&(bbox.max_y as i16).to_le_bytes()); // top
    buf.extend_from_slice(&(bbox.min_y as i16).to_le_bytes()); // bottom
    buf.extend_from_slice(&(bbox.min_x as i16).to_le_bytes()); // left
    buf.extend_from_slice(&(bbox.max_x as i16).to_le_bytes()); // right
}

fn write_reject(num_sectors: usize) -> Vec<u8> {
    let bits = num_sectors * num_sectors;
    let bytes = (bits + 7) / 8;
    vec![0u8; bytes]
}

fn sort_level_lumps(lumps: &mut Vec<OutputLump>) {
    let mut i = 0;
    while i < lumps.len() {
        let name = std::str::from_utf8(&lumps[i].name)
            .unwrap_or("")
            .trim_end_matches('\0');
        if is_map_marker(name) {
            let start = i + 1;
            let mut end = start;
            while end < lumps.len() {
                let n = std::str::from_utf8(&lumps[end].name)
                    .unwrap_or("")
                    .trim_end_matches('\0');
                if is_map_marker(n) {
                    break;
                }
                end += 1;
            }
            lumps[start..end].sort_by(|a, b| {
                let an = std::str::from_utf8(&a.name)
                    .unwrap_or("")
                    .trim_end_matches('\0');
                let bn = std::str::from_utf8(&b.name)
                    .unwrap_or("")
                    .trim_end_matches('\0');
                let ao = LUMP_ORDER.iter().position(|&n| n == an).unwrap_or(99);
                let bo = LUMP_ORDER.iter().position(|&n| n == bn).unwrap_or(99);
                ao.cmp(&bo)
            });
            i = end;
        } else {
            i += 1;
        }
    }
}

fn write_pwad(path: &Path, lumps: &[OutputLump]) -> io::Result<()> {
    let mut file = File::create(path)?;

    // Calculate directory offset (after header + all lump data).
    let mut data_size: u32 = 0;
    for lump in lumps {
        data_size += lump.data.len() as u32;
    }
    let dir_offset = 12 + data_size;

    // Header.
    file.write_all(PWAD_MAGIC)?;
    file.write_all(&(lumps.len() as i32).to_le_bytes())?;
    file.write_all(&(dir_offset as i32).to_le_bytes())?;

    // Lump data.
    for lump in lumps {
        if !lump.data.is_empty() {
            file.write_all(&lump.data)?;
        }
    }

    // Directory.
    let mut offset: u32 = 12;
    for lump in lumps {
        file.write_all(&(offset as i32).to_le_bytes())?;
        file.write_all(&(lump.data.len() as i32).to_le_bytes())?;
        file.write_all(&lump.name)?;
        offset += lump.data.len() as u32;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_map_marker_detects_formats() {
        assert!(is_map_marker("E1M1"));
        assert!(is_map_marker("E4M9"));
        assert!(is_map_marker("MAP01"));
        assert!(is_map_marker("MAP32"));
        assert!(!is_map_marker("THINGS"));
        assert!(!is_map_marker("SEGS"));
        assert!(!is_map_marker(""));
        assert!(!is_map_marker("MAP"));
    }

    #[test]
    fn write_reject_size() {
        let r = write_reject(10);
        assert_eq!(r.len(), 13); // ceil(100/8) = 13
        assert!(r.iter().all(|&b| b == 0));

        let r = write_reject(1);
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn write_vertexes_roundtrip() {
        let output = BspOutput {
            vertices: vec![
                Vertex {
                    x: 100.0,
                    y: -200.0,
                },
                Vertex {
                    x: 32767.0,
                    y: -32768.0,
                },
            ],
            num_original_verts: 2,
            segs: vec![],
            subsectors: vec![],
            nodes: vec![],
            root: 0,
            poly_indices: vec![],
        };
        let buf = write_vertexes(&output);
        assert_eq!(buf.len(), 8);
        assert_eq!(i16::from_le_bytes([buf[0], buf[1]]), 100);
        assert_eq!(i16::from_le_bytes([buf[2], buf[3]]), -200);
    }

    #[test]
    fn pwad_header_structure() {
        let lumps = vec![
            OutputLump::marker("E1M1"),
            OutputLump::new("THINGS", vec![1, 2, 3, 4]),
        ];
        let path = std::env::temp_dir().join("rbsp_test.wad");
        write_pwad(&path, &lumps).unwrap();

        let data = std::fs::read(&path).unwrap();
        assert_eq!(&data[0..4], b"PWAD");
        let num_lumps = i32::from_le_bytes(data[4..8].try_into().unwrap());
        assert_eq!(num_lumps, 2);
        let dir_offset = i32::from_le_bytes(data[8..12].try_into().unwrap());
        assert_eq!(dir_offset, 16); // 12 header + 4 bytes data

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn lump_ordering() {
        let mut lumps = vec![
            OutputLump::marker("E1M1"),
            OutputLump::new("RBSP", vec![]),
            OutputLump::new("NODES", vec![]),
            OutputLump::new("THINGS", vec![]),
            OutputLump::new("VERTEXES", vec![]),
        ];
        sort_level_lumps(&mut lumps);

        let names: Vec<&str> = lumps
            .iter()
            .map(|l| std::str::from_utf8(&l.name).unwrap().trim_end_matches('\0'))
            .collect();
        assert_eq!(names, vec!["E1M1", "THINGS", "VERTEXES", "NODES", "RBSP"]);
    }
}
