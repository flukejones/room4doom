//! RBSP binary lump writer.
//!
//! Serializes `BspOutput` into a compact binary format for room4doom.
//! All floats f32, all multi-byte values little-endian, structs 4-byte aligned.

use crate::types::*;

const MAGIC: &[u8; 4] = b"RBSP";
const VERSION: u16 = 1;
const NUM_SECTIONS: usize = 6;
const HEADER_SIZE: usize = 16;
const DIR_ENTRY_SIZE: usize = 8;
const DIR_SIZE: usize = NUM_SECTIONS * DIR_ENTRY_SIZE;
const VERTEX_SIZE: usize = 8;
const SEG_SIZE: usize = 20;
const SUBSECTOR_SIZE: usize = 20;
const NODE_SIZE: usize = 24;

pub fn write_rbsp_lump(output: &BspOutput) -> Vec<u8> {
    // Flatten subsector seg_indices into a single array.
    let mut flat_seg_indices: Vec<u32> = Vec::new();
    let mut ss_seg_starts: Vec<u32> = Vec::with_capacity(output.subsectors.len());
    let mut ss_seg_counts: Vec<u32> = Vec::with_capacity(output.subsectors.len());
    for ss in &output.subsectors {
        ss_seg_starts.push(flat_seg_indices.len() as u32);
        ss_seg_counts.push(ss.seg_indices.len() as u32);
        flat_seg_indices.extend_from_slice(&ss.seg_indices);
    }

    // Compute section sizes.
    let verts_size = output.vertices.len() * VERTEX_SIZE;
    let segs_size = output.segs.len() * SEG_SIZE;
    let ss_size = output.subsectors.len() * SUBSECTOR_SIZE;
    let nodes_size = output.nodes.len() * NODE_SIZE;
    let poly_size = output.poly_indices.len() * 4;
    let seg_idx_size = flat_seg_indices.len() * 4;

    let data_start = HEADER_SIZE + DIR_SIZE;
    let offsets = [
        data_start,
        data_start + verts_size,
        data_start + verts_size + segs_size,
        data_start + verts_size + segs_size + ss_size,
        data_start + verts_size + segs_size + ss_size + nodes_size,
        data_start + verts_size + segs_size + ss_size + nodes_size + poly_size,
    ];
    let counts = [
        output.vertices.len(),
        output.segs.len(),
        output.subsectors.len(),
        output.nodes.len(),
        output.poly_indices.len(),
        flat_seg_indices.len(),
    ];

    let total = offsets[NUM_SECTIONS - 1] + seg_idx_size;
    let mut buf = Vec::with_capacity(total);

    // Header.
    buf.extend_from_slice(MAGIC);
    buf.extend_from_slice(&VERSION.to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes()); // reserved
    buf.extend_from_slice(&output.root.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes()); // reserved

    // Section directory.
    for i in 0..NUM_SECTIONS {
        buf.extend_from_slice(&(offsets[i] as u32).to_le_bytes());
        buf.extend_from_slice(&(counts[i] as u32).to_le_bytes());
    }

    // Vertices.
    for v in &output.vertices {
        buf.extend_from_slice(&(v.x as f32).to_le_bytes());
        buf.extend_from_slice(&(v.y as f32).to_le_bytes());
    }

    // Segs.
    for seg in &output.segs {
        buf.extend_from_slice(&(seg.start as u32).to_le_bytes());
        buf.extend_from_slice(&(seg.end as u32).to_le_bytes());
        buf.extend_from_slice(&(seg.linedef as u32).to_le_bytes());
        buf.extend_from_slice(&(seg.offset as f32).to_le_bytes());
        let side: u32 = match seg.side {
            Side::Front => 0,
            Side::Back => 1,
        };
        buf.extend_from_slice(&side.to_le_bytes());
    }

    // Subsectors.
    for (i, ss) in output.subsectors.iter().enumerate() {
        buf.extend_from_slice(&ss.sector.to_le_bytes());
        buf.extend_from_slice(&ss_seg_starts[i].to_le_bytes());
        buf.extend_from_slice(&ss_seg_counts[i].to_le_bytes());
        buf.extend_from_slice(&ss.polygon.first_vertex.to_le_bytes());
        buf.extend_from_slice(&ss.polygon.num_vertices.to_le_bytes());
    }

    // Nodes.
    for node in &output.nodes {
        buf.extend_from_slice(&(node.x as f32).to_le_bytes());
        buf.extend_from_slice(&(node.y as f32).to_le_bytes());
        buf.extend_from_slice(&(node.dx as f32).to_le_bytes());
        buf.extend_from_slice(&(node.dy as f32).to_le_bytes());
        buf.extend_from_slice(&node.child_right.to_le_bytes());
        buf.extend_from_slice(&node.child_left.to_le_bytes());
    }

    // Poly indices.
    for &vi in &output.poly_indices {
        buf.extend_from_slice(&vi.to_le_bytes());
    }

    // Seg indices.
    for &si in &flat_seg_indices {
        buf.extend_from_slice(&si.to_le_bytes());
    }

    buf
}

/// Read an RBSP binary lump into a `BspOutput`.
/// Returns `None` if the data is invalid or has wrong magic/version.
pub fn read_rbsp_lump(data: &[u8]) -> Option<BspOutput> {
    if data.len() < HEADER_SIZE + DIR_SIZE {
        return None;
    }
    if &data[0..4] != MAGIC {
        return None;
    }
    let version = u16::from_le_bytes(data[4..6].try_into().ok()?);
    if version != VERSION {
        return None;
    }
    let root = u32::from_le_bytes(data[8..12].try_into().ok()?);

    // Read section directory.
    let mut offsets = [0u32; NUM_SECTIONS];
    let mut counts = [0u32; NUM_SECTIONS];
    for i in 0..NUM_SECTIONS {
        let base = HEADER_SIZE + i * DIR_ENTRY_SIZE;
        offsets[i] = u32::from_le_bytes(data[base..base + 4].try_into().ok()?);
        counts[i] = u32::from_le_bytes(data[base + 4..base + 8].try_into().ok()?);
    }

    let rd_f32 = |off: usize| -> f32 { f32::from_le_bytes(data[off..off + 4].try_into().unwrap()) };
    let rd_u32 = |off: usize| -> u32 { u32::from_le_bytes(data[off..off + 4].try_into().unwrap()) };

    // Vertices.
    let mut vertices = Vec::with_capacity(counts[0] as usize);
    let mut off = offsets[0] as usize;
    for _ in 0..counts[0] {
        vertices.push(Vertex {
            x: rd_f32(off) as Float,
            y: rd_f32(off + 4) as Float,
        });
        off += VERTEX_SIZE;
    }

    // Segs.
    let mut segs = Vec::with_capacity(counts[1] as usize);
    off = offsets[1] as usize;
    for _ in 0..counts[1] {
        let start = rd_u32(off) as usize;
        let end = rd_u32(off + 4) as usize;
        let linedef = rd_u32(off + 8) as usize;
        let offset = rd_f32(off + 12) as Float;
        let side = if rd_u32(off + 16) == 0 {
            Side::Front
        } else {
            Side::Back
        };
        // Derive angle from vertex positions.
        let dx = vertices[end].x - vertices[start].x;
        let dy = vertices[end].y - vertices[start].y;
        let len = (dx * dx + dy * dy).sqrt();
        segs.push(Seg {
            start,
            end,
            linedef,
            side,
            sector: 0, // not stored — derived from sidedef at load
            offset,
            angle: dy.atan2(dx),
            dx,
            dy,
            len,
            dir_len: len,
            linedef_v1: 0, // not stored — only used during build
        });
        off += SEG_SIZE;
    }

    // Subsectors.
    let mut subsectors = Vec::with_capacity(counts[2] as usize);
    off = offsets[2] as usize;
    // Read seg_indices flat array first.
    let si_off = offsets[5] as usize;
    let si_count = counts[5] as usize;
    let flat_seg_indices: Vec<u32> = (0..si_count).map(|i| rd_u32(si_off + i * 4)).collect();

    for _ in 0..counts[2] {
        let sector = rd_u32(off);
        let seg_idx_start = rd_u32(off + 4) as usize;
        let seg_idx_count = rd_u32(off + 8) as usize;
        let poly_start = rd_u32(off + 12);
        let poly_count = rd_u32(off + 16);
        subsectors.push(SubSector {
            sector,
            polygon: ConvexPoly {
                first_vertex: poly_start,
                num_vertices: poly_count,
                first_edge: 0,
            },
            first_seg: 0,
            num_segs: seg_idx_count as u32,
            seg_indices: flat_seg_indices[seg_idx_start..seg_idx_start + seg_idx_count].to_vec(),
        });
        off += SUBSECTOR_SIZE;
    }

    // Nodes.
    let mut nodes = Vec::with_capacity(counts[3] as usize);
    off = offsets[3] as usize;
    for _ in 0..counts[3] {
        nodes.push(Node {
            x: rd_f32(off) as Float,
            y: rd_f32(off + 4) as Float,
            dx: rd_f32(off + 8) as Float,
            dy: rd_f32(off + 12) as Float,
            bbox_right: BBox::EMPTY, // recomputed at load
            bbox_left: BBox::EMPTY,
            child_right: rd_u32(off + 16),
            child_left: rd_u32(off + 20),
        });
        off += NODE_SIZE;
    }

    // Poly indices.
    let pi_off = offsets[4] as usize;
    let poly_indices: Vec<u32> = (0..counts[4] as usize)
        .map(|i| rd_u32(pi_off + i * 4))
        .collect();

    Some(BspOutput {
        vertices,
        num_original_verts: 0, // not stored — caller sets from WAD vertex count
        segs,
        subsectors,
        nodes,
        root,
        poly_indices,
    })
}

#[cfg(test)]
mod tests {
    use std::f64::consts::FRAC_PI_2;

    use super::*;

    fn make_test_output() -> BspOutput {
        BspOutput {
            vertices: vec![
                Vertex {
                    x: 0.0,
                    y: 0.0,
                },
                Vertex {
                    x: 100.0,
                    y: 0.0,
                },
                Vertex {
                    x: 100.0,
                    y: 100.0,
                },
                Vertex {
                    x: 0.0,
                    y: 100.0,
                },
                Vertex {
                    x: 50.0,
                    y: 50.0,
                },
            ],
            num_original_verts: 4,
            segs: vec![
                Seg {
                    start: 0,
                    end: 1,
                    linedef: 0,
                    side: Side::Front,
                    sector: 0,
                    offset: 0.0,
                    angle: 0.0,
                    dx: 100.0,
                    dy: 0.0,
                    len: 100.0,
                    dir_len: 100.0,
                    linedef_v1: 0,
                },
                Seg {
                    start: 1,
                    end: 2,
                    linedef: 1,
                    side: Side::Front,
                    sector: 0,
                    offset: 0.0,
                    angle: FRAC_PI_2 as Float,
                    dx: 0.0,
                    dy: 100.0,
                    len: 100.0,
                    dir_len: 100.0,
                    linedef_v1: 1,
                },
            ],
            subsectors: vec![SubSector {
                sector: 0,
                polygon: ConvexPoly {
                    first_vertex: 0,
                    num_vertices: 3,
                    first_edge: 0,
                },
                first_seg: 0,
                num_segs: 2,
                seg_indices: vec![0, 1],
            }],
            nodes: vec![Node {
                x: 50.0,
                y: 0.0,
                dx: 0.0,
                dy: 100.0,
                bbox_right: BBox {
                    min_x: 50.0,
                    min_y: 0.0,
                    max_x: 100.0,
                    max_y: 100.0,
                },
                bbox_left: BBox {
                    min_x: 0.0,
                    min_y: 0.0,
                    max_x: 50.0,
                    max_y: 100.0,
                },
                child_right: 0x80000000,
                child_left: 0x80000001,
            }],
            root: 0,
            poly_indices: vec![0, 1, 4],
        }
    }

    #[test]
    fn header_magic_and_version() {
        let output = make_test_output();
        let buf = write_rbsp_lump(&output);

        assert_eq!(&buf[0..4], b"RBSP");
        assert_eq!(u16::from_le_bytes([buf[4], buf[5]]), VERSION);
        assert_eq!(u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]), 0); // root_node
    }

    #[test]
    fn section_directory_offsets_and_counts() {
        let output = make_test_output();
        let buf = write_rbsp_lump(&output);
        let expected_counts: [u32; NUM_SECTIONS] = [5, 2, 1, 1, 3, 2];

        let dir_start = HEADER_SIZE;
        for i in 0..NUM_SECTIONS {
            let off = dir_start + i * DIR_ENTRY_SIZE;
            let offset = u32::from_le_bytes(buf[off..off + 4].try_into().unwrap());
            let count = u32::from_le_bytes(buf[off + 4..off + 8].try_into().unwrap());

            assert!(
                (offset as usize) <= buf.len(),
                "section {} offset {} > buf len {}",
                i,
                offset,
                buf.len()
            );
            assert_eq!(count, expected_counts[i], "section {} count mismatch", i);
        }
    }

    #[test]
    fn total_size_matches() {
        let output = make_test_output();
        let buf = write_rbsp_lump(&output);

        let expected = HEADER_SIZE + DIR_SIZE
            + 5 * VERTEX_SIZE
            + 2 * SEG_SIZE
            + 1 * SUBSECTOR_SIZE
            + 1 * NODE_SIZE
            + 3 * 4  // poly_indices
            + 2 * 4; // seg_indices
        assert_eq!(buf.len(), expected);
    }

    #[test]
    fn vertices_roundtrip() {
        let output = make_test_output();
        let buf = write_rbsp_lump(&output);

        let dir_start = HEADER_SIZE;
        let v_offset =
            u32::from_le_bytes(buf[dir_start..dir_start + 4].try_into().unwrap()) as usize;

        let x0 = f32::from_le_bytes(buf[v_offset..v_offset + 4].try_into().unwrap());
        let y0 = f32::from_le_bytes(buf[v_offset + 4..v_offset + 8].try_into().unwrap());
        assert_eq!(x0, 0.0);
        assert_eq!(y0, 0.0);

        let x4 = f32::from_le_bytes(buf[v_offset + 32..v_offset + 36].try_into().unwrap());
        let y4 = f32::from_le_bytes(buf[v_offset + 36..v_offset + 40].try_into().unwrap());
        assert_eq!(x4, 50.0);
        assert_eq!(y4, 50.0);
    }

    #[test]
    fn seg_fields() {
        let output = make_test_output();
        let buf = write_rbsp_lump(&output);

        let dir_start = HEADER_SIZE + DIR_ENTRY_SIZE; // segs entry
        let s_offset =
            u32::from_le_bytes(buf[dir_start..dir_start + 4].try_into().unwrap()) as usize;

        let start = u32::from_le_bytes(buf[s_offset..s_offset + 4].try_into().unwrap());
        let end = u32::from_le_bytes(buf[s_offset + 4..s_offset + 8].try_into().unwrap());
        let linedef = u32::from_le_bytes(buf[s_offset + 8..s_offset + 12].try_into().unwrap());
        let offset = f32::from_le_bytes(buf[s_offset + 12..s_offset + 16].try_into().unwrap());
        let side = u32::from_le_bytes(buf[s_offset + 16..s_offset + 20].try_into().unwrap());

        assert_eq!(start, 0);
        assert_eq!(end, 1);
        assert_eq!(linedef, 0);
        assert_eq!(offset, 0.0);
        assert_eq!(side, 0); // Front
    }

    #[test]
    fn subsector_seg_indices_flattened() {
        let output = make_test_output();
        let buf = write_rbsp_lump(&output);

        // Read subsector entry.
        let dir_start = HEADER_SIZE + 2 * DIR_ENTRY_SIZE;
        let ss_offset =
            u32::from_le_bytes(buf[dir_start..dir_start + 4].try_into().unwrap()) as usize;

        let seg_idx_start =
            u32::from_le_bytes(buf[ss_offset + 4..ss_offset + 8].try_into().unwrap());
        let seg_idx_count =
            u32::from_le_bytes(buf[ss_offset + 8..ss_offset + 12].try_into().unwrap());
        assert_eq!(seg_idx_start, 0);
        assert_eq!(seg_idx_count, 2);

        // Read actual seg indices from the flat array.
        let si_dir = HEADER_SIZE + 5 * DIR_ENTRY_SIZE;
        let si_offset = u32::from_le_bytes(buf[si_dir..si_dir + 4].try_into().unwrap()) as usize;

        let si0 = u32::from_le_bytes(buf[si_offset..si_offset + 4].try_into().unwrap());
        let si1 = u32::from_le_bytes(buf[si_offset + 4..si_offset + 8].try_into().unwrap());
        assert_eq!(si0, 0);
        assert_eq!(si1, 1);
    }

    #[test]
    fn node_fields() {
        let output = make_test_output();
        let buf = write_rbsp_lump(&output);

        let dir_start = HEADER_SIZE + 3 * DIR_ENTRY_SIZE;
        let n_offset =
            u32::from_le_bytes(buf[dir_start..dir_start + 4].try_into().unwrap()) as usize;

        let x = f32::from_le_bytes(buf[n_offset..n_offset + 4].try_into().unwrap());
        let y = f32::from_le_bytes(buf[n_offset + 4..n_offset + 8].try_into().unwrap());
        let dx = f32::from_le_bytes(buf[n_offset + 8..n_offset + 12].try_into().unwrap());
        let dy = f32::from_le_bytes(buf[n_offset + 12..n_offset + 16].try_into().unwrap());
        let child_r = u32::from_le_bytes(buf[n_offset + 16..n_offset + 20].try_into().unwrap());
        let child_l = u32::from_le_bytes(buf[n_offset + 20..n_offset + 24].try_into().unwrap());

        assert_eq!(x, 50.0);
        assert_eq!(y, 0.0);
        assert_eq!(dx, 0.0);
        assert_eq!(dy, 100.0);
        assert_eq!(child_r, 0x80000000);
        assert_eq!(child_l, 0x80000001);
    }

    #[test]
    fn empty_output() {
        let output = BspOutput {
            vertices: vec![],
            num_original_verts: 0,
            segs: vec![],
            subsectors: vec![],
            nodes: vec![],
            root: 0,
            poly_indices: vec![],
        };
        let buf = write_rbsp_lump(&output);
        assert_eq!(buf.len(), HEADER_SIZE + DIR_SIZE);
        assert_eq!(&buf[0..4], b"RBSP");
    }

    #[test]
    fn write_read_roundtrip() {
        let output = make_test_output();
        let buf = write_rbsp_lump(&output);
        let read = read_rbsp_lump(&buf).expect("failed to read RBSP lump");

        assert_eq!(read.root, output.root);
        assert_eq!(read.vertices.len(), output.vertices.len());
        assert_eq!(read.segs.len(), output.segs.len());
        assert_eq!(read.subsectors.len(), output.subsectors.len());
        assert_eq!(read.nodes.len(), output.nodes.len());
        assert_eq!(read.poly_indices, output.poly_indices);

        // Vertex positions survive f64→f32→f64 roundtrip.
        for (a, b) in read.vertices.iter().zip(output.vertices.iter()) {
            assert!((a.x - b.x).abs() < 0.01, "vertex x mismatch");
            assert!((a.y - b.y).abs() < 0.01, "vertex y mismatch");
        }

        // Seg fields.
        for (a, b) in read.segs.iter().zip(output.segs.iter()) {
            assert_eq!(a.start, b.start);
            assert_eq!(a.end, b.end);
            assert_eq!(a.linedef, b.linedef);
            assert_eq!(a.side, b.side);
        }

        // Subsector seg_indices.
        for (a, b) in read.subsectors.iter().zip(output.subsectors.iter()) {
            assert_eq!(a.sector, b.sector);
            assert_eq!(a.seg_indices, b.seg_indices);
            assert_eq!(a.polygon.first_vertex, b.polygon.first_vertex);
            assert_eq!(a.polygon.num_vertices, b.polygon.num_vertices);
        }

        // Node partition line + children.
        for (a, b) in read.nodes.iter().zip(output.nodes.iter()) {
            assert!((a.x - b.x).abs() < 0.01);
            assert!((a.y - b.y).abs() < 0.01);
            assert_eq!(a.child_right, b.child_right);
            assert_eq!(a.child_left, b.child_left);
        }
    }

    #[test]
    fn read_invalid_data() {
        assert!(read_rbsp_lump(&[]).is_none());
        assert!(read_rbsp_lump(b"NOPE").is_none());
        assert!(read_rbsp_lump(&[0u8; 100]).is_none());
    }
}
