//! Shared test harness for BSP comparison tests.
//!
//! Compares output from the C and Rust BSP builders lump-by-lump:
//! - VERTEXES and REJECT: byte-exact match
//! - SEGS, SSECTORS, NODES: same count, all node partition lines identical
//! - BLOCKMAP: headers match, allows minor float-rounding differences

#![allow(dead_code)]

use std::collections::HashMap;
use std::process::Command;

pub const C_BSP: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../bsp/bsp");

// ---------------------------------------------------------------------------
// WAD reading
// ---------------------------------------------------------------------------

pub fn read_wad_lumps(path: &str) -> HashMap<String, Vec<u8>> {
    let data = std::fs::read(path).unwrap_or_else(|e| panic!("Failed to read {}: {}", path, e));
    assert!(data.len() >= 12, "WAD too small");

    let num_lumps = i32::from_le_bytes(data[4..8].try_into().unwrap()) as usize;
    let dir_offset = i32::from_le_bytes(data[8..12].try_into().unwrap()) as usize;

    let mut lumps = HashMap::new();
    for i in 0..num_lumps {
        let eo = dir_offset + i * 16;
        let offset = i32::from_le_bytes(data[eo..eo + 4].try_into().unwrap()) as usize;
        let size = i32::from_le_bytes(data[eo + 4..eo + 8].try_into().unwrap()) as usize;
        let name_bytes = &data[eo + 8..eo + 16];
        let name_len = name_bytes.iter().position(|&b| b == 0).unwrap_or(8);
        let name = String::from_utf8_lossy(&name_bytes[..name_len]).to_string();
        let lump_data = if size > 0 {
            data[offset..offset + size].to_vec()
        } else {
            Vec::new()
        };
        lumps.insert(name, lump_data);
    }
    lumps
}

// ---------------------------------------------------------------------------
// Level extraction (for multi-level WADs)
// ---------------------------------------------------------------------------

/// Extract a single level from a multi-level WAD into a standalone PWAD.
pub fn extract_level(wad_path: &str, level_name: &str, out_path: &str) {
    let data =
        std::fs::read(wad_path).unwrap_or_else(|e| panic!("Failed to read {}: {}", wad_path, e));
    assert!(data.len() >= 12);

    let num_lumps = i32::from_le_bytes(data[4..8].try_into().unwrap()) as usize;
    let dir_offset = i32::from_le_bytes(data[8..12].try_into().unwrap()) as usize;

    // Parse directory
    struct DirEntry {
        offset: usize,
        size: usize,
        name: [u8; 8],
        clean_name: String,
    }

    let mut entries = Vec::with_capacity(num_lumps);
    for i in 0..num_lumps {
        let eo = dir_offset + i * 16;
        let offset = i32::from_le_bytes(data[eo..eo + 4].try_into().unwrap()) as usize;
        let size = i32::from_le_bytes(data[eo + 4..eo + 8].try_into().unwrap()) as usize;
        let mut name = [0u8; 8];
        name.copy_from_slice(&data[eo + 8..eo + 16]);
        let name_len = name.iter().position(|&b| b == 0).unwrap_or(8);
        let clean_name = String::from_utf8_lossy(&name[..name_len]).to_string();
        entries.push(DirEntry {
            offset,
            size,
            name,
            clean_name,
        });
    }

    // Find level start
    let level_start = entries
        .iter()
        .position(|e| e.clean_name == level_name && e.size == 0)
        .unwrap_or_else(|| panic!("Level {} not found in {}", level_name, wad_path));

    // Find level end (next level marker or end of file)
    let level_end = entries[level_start + 1..]
        .iter()
        .position(|e| e.size == 0 && is_level_marker(&e.clean_name))
        .map(|p| p + level_start + 1)
        .unwrap_or(entries.len());

    // Build output PWAD
    let mut out = Vec::new();

    // Header placeholder (12 bytes)
    out.extend_from_slice(b"PWAD");
    let nlumps = (level_end - level_start) as i32;
    out.extend_from_slice(&nlumps.to_le_bytes());
    out.extend_from_slice(&0i32.to_le_bytes()); // dir_offset placeholder

    // Write lump data, track entries
    let mut out_entries: Vec<(i32, i32, [u8; 8])> = Vec::new();
    for entry in &entries[level_start..level_end] {
        let lump_offset = out.len() as i32;
        if entry.size > 0 {
            out.extend_from_slice(&data[entry.offset..entry.offset + entry.size]);
        }
        out_entries.push((lump_offset, entry.size as i32, entry.name));
    }

    // Write directory
    let dir_off = out.len() as i32;
    for (offset, size, name) in &out_entries {
        out.extend_from_slice(&offset.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(name);
    }

    // Fix header dir_offset
    out[8..12].copy_from_slice(&dir_off.to_le_bytes());

    std::fs::write(out_path, &out)
        .unwrap_or_else(|e| panic!("Failed to write {}: {}", out_path, e));
}

fn is_level_marker(name: &str) -> bool {
    let b = name.as_bytes();
    // E#M# format
    if b.len() >= 4
        && b[0] == b'E'
        && b[2] == b'M'
        && b[1].is_ascii_digit()
        && b[3].is_ascii_digit()
    {
        return true;
    }
    // MAP## format
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

// ---------------------------------------------------------------------------
// Lump parsers
// ---------------------------------------------------------------------------

fn parse_node_partitions(data: &[u8]) -> Vec<(i16, i16, i16, i16)> {
    (0..data.len() / 28)
        .map(|i| {
            let o = i * 28;
            (
                i16::from_le_bytes(data[o..o + 2].try_into().unwrap()),
                i16::from_le_bytes(data[o + 2..o + 4].try_into().unwrap()),
                i16::from_le_bytes(data[o + 4..o + 6].try_into().unwrap()),
                i16::from_le_bytes(data[o + 6..o + 8].try_into().unwrap()),
            )
        })
        .collect()
}

fn parse_blockmap(data: &[u8]) -> (Vec<i16>, Vec<Vec<i16>>) {
    if data.len() < 8 {
        return (Vec::new(), Vec::new());
    }
    let header: Vec<i16> = (0..4)
        .map(|i| i16::from_le_bytes(data[i * 2..i * 2 + 2].try_into().unwrap()))
        .collect();
    let nblocks = (header[2] as usize) * (header[3] as usize);

    let ptrs: Vec<u16> = (0..nblocks)
        .map(|i| {
            let off = 8 + i * 2;
            u16::from_le_bytes(data[off..off + 2].try_into().unwrap())
        })
        .collect();

    let lists = ptrs
        .iter()
        .map(|&ptr| {
            let mut list = Vec::new();
            let mut byte_off = (ptr as usize) * 2;
            if byte_off + 2 <= data.len() {
                let marker = i16::from_le_bytes(data[byte_off..byte_off + 2].try_into().unwrap());
                assert_eq!(marker, 0, "Expected 0 marker at blockmap list start");
                byte_off += 2;
            }
            loop {
                if byte_off + 2 > data.len() {
                    break;
                }
                let val = i16::from_le_bytes(data[byte_off..byte_off + 2].try_into().unwrap());
                byte_off += 2;
                if val == -1 {
                    break;
                }
                list.push(val);
            }
            list.sort();
            list
        })
        .collect();

    (header, lists)
}

// ---------------------------------------------------------------------------
// Main comparison
// ---------------------------------------------------------------------------

/// Run both C and Rust BSP builders on `input_wad`, compare output lumps.
/// `tag` is used to generate unique temp file names.
pub fn compare_bsp_output(input_wad: &str, tag: &str) {
    let rbsp = env!("CARGO_BIN_EXE_rbsp");
    let c_output = format!("/tmp/rbsp_test_{}_c.wad", tag);
    let rust_output = format!("/tmp/rbsp_test_{}_rust.wad", tag);

    // Run C BSP builder
    let c_result = Command::new(C_BSP)
        .args([input_wad, "-o", &c_output])
        .output()
        .expect("Failed to run C bsp");
    assert!(
        c_result.status.success(),
        "C bsp failed: {}",
        String::from_utf8_lossy(&c_result.stderr)
    );

    // Run Rust BSP builder
    let rust_result = Command::new(rbsp)
        .args([input_wad, "-o", &rust_output])
        .output()
        .expect("Failed to run Rust bsp");
    assert!(
        rust_result.status.success(),
        "Rust bsp failed: {}{}",
        String::from_utf8_lossy(&rust_result.stdout),
        String::from_utf8_lossy(&rust_result.stderr),
    );

    let c_lumps = read_wad_lumps(&c_output);
    let rust_lumps = read_wad_lumps(&rust_output);

    // 1. VERTEXES: byte-exact
    assert_eq!(
        c_lumps["VERTEXES"], rust_lumps["VERTEXES"],
        "VERTEXES mismatch"
    );

    // 2. SEGS: same count
    let c_seg_count = c_lumps["SEGS"].len() / 12;
    let r_seg_count = rust_lumps["SEGS"].len() / 12;
    assert_eq!(c_seg_count, r_seg_count, "Seg count mismatch");

    // 3. SSECTORS: same count
    let c_ss_count = c_lumps["SSECTORS"].len() / 4;
    let r_ss_count = rust_lumps["SSECTORS"].len() / 4;
    assert_eq!(c_ss_count, r_ss_count, "SSector count mismatch");

    // 4. NODES: same count, all partition lines identical
    let c_node_count = c_lumps["NODES"].len() / 28;
    let r_node_count = rust_lumps["NODES"].len() / 28;
    assert_eq!(c_node_count, r_node_count, "Node count mismatch");

    let c_parts = parse_node_partitions(&c_lumps["NODES"]);
    let r_parts = parse_node_partitions(&rust_lumps["NODES"]);
    for (i, (cp, rp)) in c_parts.iter().zip(r_parts.iter()).enumerate() {
        assert_eq!(
            cp, rp,
            "Node {} partition line differs: C={:?} Rust={:?}",
            i, cp, rp
        );
    }

    // 5. REJECT: byte-exact
    assert_eq!(c_lumps["REJECT"], rust_lumps["REJECT"], "REJECT mismatch");

    // 6. BLOCKMAP: headers match, allow minor float-rounding diffs
    let (c_bm_hdr, c_bm_lists) = parse_blockmap(&c_lumps["BLOCKMAP"]);
    let (r_bm_hdr, r_bm_lists) = parse_blockmap(&rust_lumps["BLOCKMAP"]);
    assert_eq!(c_bm_hdr, r_bm_hdr, "Blockmap header mismatch");
    assert_eq!(
        c_bm_lists.len(),
        r_bm_lists.len(),
        "Blockmap block count mismatch"
    );

    let mut bm_diffs = 0;
    for (i, (cl, rl)) in c_bm_lists.iter().zip(r_bm_lists.iter()).enumerate() {
        if cl != rl {
            bm_diffs += 1;
            let c_extra: Vec<_> = cl.iter().filter(|x| !rl.contains(x)).collect();
            let r_extra: Vec<_> = rl.iter().filter(|x| !cl.contains(x)).collect();
            assert!(
                c_extra.len() <= 1 && r_extra.len() <= 1,
                "Block {} has major blockmap difference: C_extra={:?}, R_extra={:?}",
                i,
                c_extra,
                r_extra
            );
        }
    }
    assert!(
        bm_diffs <= 10,
        "Too many blockmap differences: {} blocks differ",
        bm_diffs
    );

    // Summary
    eprintln!("=== {} comparison results ===", tag);
    eprintln!("VERTEXES:  MATCH ({} bytes)", c_lumps["VERTEXES"].len());
    eprintln!("SEGS:      {} segs (same count)", c_seg_count);
    eprintln!("SSECTORS:  {} ssectors (same count)", c_ss_count);
    eprintln!(
        "NODES:     {} nodes, all partition lines match",
        c_node_count
    );
    eprintln!("REJECT:    MATCH ({} bytes)", c_lumps["REJECT"].len());
    eprintln!(
        "BLOCKMAP:  headers match, {} blocks with minor float diffs",
        bm_diffs
    );

    let _ = std::fs::remove_file(&c_output);
    let _ = std::fs::remove_file(&rust_output);
}
