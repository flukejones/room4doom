//! Blockmap generation module
//!
//! The blockmap is a 2D grid structure used by the Doom engine for fast
//! collision detection. Each block contains a list of linedefs that
//! intersect or are near that block.
//!
//! The blockmap is divided into 128x128 unit blocks.
//!
//! ```text
//! header:   origin_x, origin_y, columns, rows   (4 × i16 LE)
//! pointers: per block, word offset to its list  (u16 LE)
//! lists:    per block: 0, linedef indices…, -1  (i16 LE)
//! ```

use crate::types::*;

const BLOCKMAP_BLOCK_SIZE: i16 = 128;
/// Blockmap origin aligns down to an 8-unit boundary (vanilla behavior).
const ORIGIN_ALIGN_MASK: i16 = !7;
/// Words in the blockmap header (origin x/y, columns, rows).
const HEADER_WORDS: usize = 4;

/// Create the BLOCKMAP lump for a level.
///
/// `linedefs` reference original WAD vertex indices; `vertices` may contain
/// additional BSP split vertices beyond those (ignored here). `map_bounds`
/// must cover all linedef endpoints.
pub fn create_blockmap(linedefs: &[WadLineDef], vertices: &[Vertex], map_bounds: &BBox) -> Vec<u8> {
    let origin_x = (map_bounds.min_x.floor() as i16) & ORIGIN_ALIGN_MASK;
    let origin_y = (map_bounds.min_y.floor() as i16) & ORIGIN_ALIGN_MASK;

    let num_cols = ((map_bounds.max_x.ceil() as i16 - origin_x) / BLOCKMAP_BLOCK_SIZE) + 1;
    let num_rows = ((map_bounds.max_y.ceil() as i16 - origin_y) / BLOCKMAP_BLOCK_SIZE) + 1;

    log::debug!("Blockmap: origin=({origin_x},{origin_y}), size={num_cols}x{num_rows} blocks");

    let num_blocks = (num_cols as usize) * (num_rows as usize);
    let mut blocklists: Vec<Vec<i16>> = Vec::with_capacity(num_blocks);
    let mut blockptrs: Vec<u16> = Vec::with_capacity(num_blocks);
    let mut blockoffs: usize = 0;

    for row in 0..num_rows {
        for col in 0..num_cols {
            let block_x = origin_x + col * BLOCKMAP_BLOCK_SIZE;
            let block_y = origin_y + row * BLOCKMAP_BLOCK_SIZE;
            let block_x2 = block_x + BLOCKMAP_BLOCK_SIZE - 1;
            let block_y2 = block_y + BLOCKMAP_BLOCK_SIZE - 1;

            blockptrs.push((blockoffs + HEADER_WORDS + num_blocks) as u16);

            let mut block_list = vec![0i16];
            for (linedef_idx, linedef) in linedefs.iter().enumerate() {
                if is_linedef_in_block(linedef, vertices, block_x, block_y, block_x2, block_y2) {
                    block_list.push(linedef_idx as i16);
                }
            }
            block_list.push(-1);

            blockoffs += block_list.len();
            blocklists.push(block_list);
        }
    }

    let total_words = HEADER_WORDS + num_blocks + blockoffs;
    let mut blockmap_data = Vec::with_capacity(total_words * 2);

    write_i16(&mut blockmap_data, origin_x);
    write_i16(&mut blockmap_data, origin_y);
    write_i16(&mut blockmap_data, num_cols);
    write_i16(&mut blockmap_data, num_rows);

    for ptr in &blockptrs {
        write_u16(&mut blockmap_data, *ptr);
    }
    for block_list in &blocklists {
        for &value in block_list {
            write_i16(&mut blockmap_data, value);
        }
    }

    blockmap_data
}

/// Check if a linedef intersects or is inside a block.
/// Uses i32 for intermediate coordinates to match C's `int` and avoid
/// overflow.
fn is_linedef_in_block(
    linedef: &WadLineDef,
    vertices: &[Vertex],
    block_x: i16,
    block_y: i16,
    block_x2: i16,
    block_y2: i16,
) -> bool {
    let start = &vertices[linedef.start_vertex as usize];
    let end = &vertices[linedef.end_vertex as usize];

    let mut x1 = start.x as i32;
    let mut y1 = start.y as i32;
    let mut x2 = end.x as i32;
    let mut y2 = end.y as i32;

    let bx = block_x as i32;
    let by = block_y as i32;
    let bx2 = block_x2 as i32;
    let by2 = block_y2 as i32;

    let mut count = 2;

    loop {
        if y1 > by2 {
            if y2 > by2 {
                return false;
            }
            x1 += ((x2 - x1) as f64 * (by2 - y1) as f64 / (y2 - y1) as f64) as i32;
            y1 = by2;
            count = 2;
        } else if y1 < by {
            if y2 < by {
                return false;
            }
            x1 += ((x2 - x1) as f64 * (by - y1) as f64 / (y2 - y1) as f64) as i32;
            y1 = by;
            count = 2;
        } else if x1 > bx2 {
            if x2 > bx2 {
                return false;
            }
            y1 += ((y2 - y1) as f64 * (bx2 - x1) as f64 / (x2 - x1) as f64) as i32;
            x1 = bx2;
            count = 2;
        } else if x1 < bx {
            if x2 < bx {
                return false;
            }
            y1 += ((y2 - y1) as f64 * (bx - x1) as f64 / (x2 - x1) as f64) as i32;
            x1 = bx;
            count = 2;
        } else {
            count -= 1;
            if count == 0 {
                return true;
            }
            std::mem::swap(&mut x1, &mut x2);
            std::mem::swap(&mut y1, &mut y2);
        }
    }
}

fn write_i16(data: &mut Vec<u8>, value: i16) {
    data.extend_from_slice(&value.to_le_bytes());
}

fn write_u16(data: &mut Vec<u8>, value: u16) {
    data.extend_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square_map() -> (Vec<WadLineDef>, Vec<Vertex>, BBox) {
        let vertices = vec![
            Vertex {
                x: 0.0,
                y: 0.0,
            },
            Vertex {
                x: 200.0,
                y: 0.0,
            },
            Vertex {
                x: 200.0,
                y: 200.0,
            },
            Vertex {
                x: 0.0,
                y: 200.0,
            },
        ];
        let linedefs = (0..4u16)
            .map(|i| WadLineDef::new(i, (i + 1) % 4, 1, 0, 0, 0, None, [0, u16::MAX]))
            .collect();
        let bounds = BBox {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 200.0,
            max_y: 200.0,
        };
        (linedefs, vertices, bounds)
    }

    #[test]
    fn header_and_pointer_layout() {
        let (linedefs, vertices, bounds) = square_map();
        let data = create_blockmap(&linedefs, &vertices, &bounds);

        let rd = |i: usize| i16::from_le_bytes([data[i * 2], data[i * 2 + 1]]);
        assert_eq!(rd(0), 0); // origin x
        assert_eq!(rd(1), 0); // origin y
        let cols = rd(2);
        let rows = rd(3);
        assert_eq!((cols, rows), (2, 2));

        // First block pointer targets the word right after header + pointers.
        let first_ptr = u16::from_le_bytes([data[8], data[9]]) as usize;
        assert_eq!(first_ptr, HEADER_WORDS + 4);
        // Every block list begins with 0 and ends with -1.
        let first_list_start = first_ptr * 2;
        assert_eq!(
            i16::from_le_bytes([data[first_list_start], data[first_list_start + 1]]),
            0
        );
        assert_eq!(
            i16::from_le_bytes([data[data.len() - 2], data[data.len() - 1]]),
            -1
        );
    }

    #[test]
    fn boundary_lines_land_in_touching_blocks() {
        let (linedefs, vertices, bounds) = square_map();
        let data = create_blockmap(&linedefs, &vertices, &bounds);

        // Block (0,0) covers 0..=127: must contain the bottom (0) and left
        // (3) lines plus the parts of right/top? Right line x=200 is outside,
        // top y=200 outside. Expect lines 0 and 3.
        let ptr = u16::from_le_bytes([data[8], data[9]]) as usize;
        let mut idx = ptr;
        let rd = |i: usize| i16::from_le_bytes([data[i * 2], data[i * 2 + 1]]);
        assert_eq!(rd(idx), 0);
        idx += 1;
        let mut found = Vec::new();
        while rd(idx) != -1 {
            found.push(rd(idx));
            idx += 1;
        }
        assert_eq!(found, vec![0, 3]);
    }
}
