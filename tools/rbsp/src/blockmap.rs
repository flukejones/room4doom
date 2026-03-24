//! Blockmap generation module
//!
//! The blockmap is a 2D grid structure used by the Doom engine for fast
//! collision detection. Each block contains a list of linedefs that
//! intersect or are near that block.
//!
//! The blockmap is divided into 128x128 unit blocks.

use crate::types::*;

const BLOCKMAP_BLOCK_SIZE: i16 = 128;

/// Create blockmap for the level
///
/// The blockmap divides the level into a grid of 128x128 unit blocks.
/// Each block contains a list of linedef indices that pass through or
/// near that block.
pub fn create_blockmap(linedefs: &[LineDef], vertices: &[Vertex], map_bounds: BBox) -> Vec<u8> {
    println!("Building blockmap...");

    // Align blockmap origin to 8-unit grid
    let origin_x = map_bounds.left & !7;
    let origin_y = map_bounds.bottom & !7;

    // Calculate number of blocks needed
    let num_cols = ((map_bounds.right - origin_x) / BLOCKMAP_BLOCK_SIZE) + 1;
    let num_rows = ((map_bounds.top - origin_y) / BLOCKMAP_BLOCK_SIZE) + 1;

    println!(
        "Blockmap: origin=({},{}), size={}x{} blocks",
        origin_x, origin_y, num_cols, num_rows
    );

    // Build blocklists for each block
    let num_blocks = (num_cols * num_rows) as usize;
    let mut blocklists: Vec<Vec<i16>> = Vec::new();
    let mut blockptrs: Vec<u16> = Vec::with_capacity(num_blocks);
    let mut blockoffs: usize = 0; // word offset into blocklist data

    for row in 0..num_rows {
        for col in 0..num_cols {
            let block_x = origin_x + col * BLOCKMAP_BLOCK_SIZE;
            let block_y = origin_y + row * BLOCKMAP_BLOCK_SIZE;
            let block_x2 = block_x + BLOCKMAP_BLOCK_SIZE - 1;
            let block_y2 = block_y + BLOCKMAP_BLOCK_SIZE - 1;

            // C: blockptrs[blocknum] = blockoffs + 4 + (blockptrs_size/2)
            // 4 = header words, blockptrs_size/2 = total number of block pointers
            blockptrs.push((blockoffs + 4 + num_blocks) as u16);

            // Start with 0 marker
            let mut block_list = vec![0i16];

            // Find all linedefs that intersect this block
            for (linedef_idx, linedef) in linedefs.iter().enumerate() {
                if is_linedef_in_block(linedef, vertices, block_x, block_y, block_x2, block_y2) {
                    block_list.push(linedef_idx as i16);
                }
            }

            // End with -1 marker
            block_list.push(-1);

            blockoffs += block_list.len();
            blocklists.push(block_list);
        }
    }

    println!("Blockmap: {} blocks created", blockptrs.len());

    // Build final blockmap data
    let mut blockmap_data = Vec::new();

    // Write header
    write_i16(&mut blockmap_data, origin_x);
    write_i16(&mut blockmap_data, origin_y);
    write_i16(&mut blockmap_data, num_cols);
    write_i16(&mut blockmap_data, num_rows);

    // Write block pointers
    for ptr in &blockptrs {
        write_u16(&mut blockmap_data, *ptr);
    }

    // Write block lists
    for block_list in &blocklists {
        for &value in block_list {
            write_i16(&mut blockmap_data, value);
        }
    }

    println!("Completed blockmap building");

    blockmap_data
}

/// Check if a linedef intersects or is inside a block.
/// Uses i32 for intermediate coordinates to match C's `int` and avoid overflow.
fn is_linedef_in_block(
    linedef: &LineDef,
    vertices: &[Vertex],
    block_x: i16,
    block_y: i16,
    block_x2: i16,
    block_y2: i16,
) -> bool {
    let start = &vertices[linedef.start as usize];
    let end = &vertices[linedef.end as usize];

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
            x1 = x1 + ((x2 - x1) as f64 * (by2 - y1) as f64 / (y2 - y1) as f64) as i32;
            y1 = by2;
            count = 2;
        } else if y1 < by {
            if y2 < by {
                return false;
            }
            x1 = x1 + ((x2 - x1) as f64 * (by - y1) as f64 / (y2 - y1) as f64) as i32;
            y1 = by;
            count = 2;
        } else if x1 > bx2 {
            if x2 > bx2 {
                return false;
            }
            y1 = y1 + ((y2 - y1) as f64 * (bx2 - x1) as f64 / (x2 - x1) as f64) as i32;
            x1 = bx2;
            count = 2;
        } else if x1 < bx {
            if x2 < bx {
                return false;
            }
            y1 = y1 + ((y2 - y1) as f64 * (bx - x1) as f64 / (x2 - x1) as f64) as i32;
            x1 = bx;
            count = 2;
        } else {
            count -= 1;
            if count == 0 {
                return true;
            }
            let tmp_x = x1;
            let tmp_y = y1;
            x1 = x2;
            y1 = y2;
            x2 = tmp_x;
            y2 = tmp_y;
        }
    }
}

/// Write i16 in little-endian format
fn write_i16(data: &mut Vec<u8>, value: i16) {
    data.push((value & 0xFF) as u8);
    data.push(((value >> 8) & 0xFF) as u8);
}

/// Write u16 in little-endian format
fn write_u16(data: &mut Vec<u8>, value: u16) {
    data.push((value & 0xFF) as u8);
    data.push(((value >> 8) & 0xFF) as u8);
}
