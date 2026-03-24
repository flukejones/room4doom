//! Spatial superblock tree for accelerating partition candidate scoring.
//!
//! Segs are organized into a binary spatial tree. When scoring a partition
//! candidate, entire blocks can be bulk-classified as left/right using
//! `box_on_line_side`, avoiding per-seg classification for blocks that
//! don't straddle the partition line. Based on the technique from glbsp.

use crate::types::{EPSILON, Float, Vertex, Seg};

const LEAF_SIZE: Float = 256.0;

pub struct SuperBlock {
    pub x1: Float,
    pub y1: Float,
    pub x2: Float,
    pub y2: Float,
    pub children: [Option<Box<SuperBlock>>; 2],
    /// Seg indices stored at this level (segs that straddle the split).
    pub seg_indices: Vec<usize>,
    /// Total seg count in this block and all descendants.
    pub count: u32,
}

impl SuperBlock {
    fn new(x1: Float, y1: Float, x2: Float, y2: Float) -> Self {
        Self {
            x1,
            y1,
            x2,
            y2,
            children: [None, None],
            seg_indices: Vec::new(),
            count: 0,
        }
    }

    fn is_leaf(&self) -> bool {
        (self.x2 - self.x1) <= LEAF_SIZE && (self.y2 - self.y1) <= LEAF_SIZE
    }

    fn add_seg(&mut self, seg_idx: usize, segs: &[Seg], vertices: &[Vertex]) {
        self.count += 1;

        if self.is_leaf() {
            self.seg_indices.push(seg_idx);
            return;
        }

        let seg = &segs[seg_idx];
        let sx = vertices[seg.start].x;
        let sy = vertices[seg.start].y;
        let ex = vertices[seg.end].x;
        let ey = vertices[seg.end].y;

        // Split along the longer axis.
        let (p1, p2, child) = if (self.x2 - self.x1) >= (self.y2 - self.y1) {
            let mid = (self.x1 + self.x2) * 0.5;
            let p1 = sx >= mid;
            let p2 = ex >= mid;
            (p1, p2, mid)
        } else {
            let mid = (self.y1 + self.y2) * 0.5;
            let p1 = sy >= mid;
            let p2 = ey >= mid;
            (p1, p2, mid)
        };

        if p1 && p2 {
            self.ensure_child(1, child);
            self.children[1]
                .as_mut()
                .unwrap()
                .add_seg(seg_idx, segs, vertices);
        } else if !p1 && !p2 {
            self.ensure_child(0, child);
            self.children[0]
                .as_mut()
                .unwrap()
                .add_seg(seg_idx, segs, vertices);
        } else {
            // Seg straddles the midpoint — store at this level.
            self.seg_indices.push(seg_idx);
        }
    }

    fn ensure_child(&mut self, idx: usize, mid: Float) {
        if self.children[idx].is_some() {
            return;
        }
        let wide = (self.x2 - self.x1) >= (self.y2 - self.y1);
        let (x1, y1, x2, y2) = if wide {
            if idx == 0 {
                (self.x1, self.y1, mid, self.y2)
            } else {
                (mid, self.y1, self.x2, self.y2)
            }
        } else if idx == 0 {
            (self.x1, self.y1, self.x2, mid)
        } else {
            (self.x1, mid, self.x2, self.y2)
        };
        self.children[idx] = Some(Box::new(SuperBlock::new(x1, y1, x2, y2)));
    }
}

/// Build a superblock tree from seg indices and map bounds.
pub fn build_superblock(seg_indices: &[usize], segs: &[Seg], vertices: &[Vertex]) -> SuperBlock {
    // Compute bounds from seg endpoints.
    let mut min_x = Float::MAX;
    let mut min_y = Float::MAX;
    let mut max_x = Float::MIN;
    let mut max_y = Float::MIN;

    for &si in seg_indices {
        let seg = &segs[si];
        for &vi in &[seg.start, seg.end] {
            let v = &vertices[vi];
            if v.x < min_x {
                min_x = v.x;
            }
            if v.y < min_y {
                min_y = v.y;
            }
            if v.x > max_x {
                max_x = v.x;
            }
            if v.y > max_y {
                max_y = v.y;
            }
        }
    }

    // Round out to LEAF_SIZE boundaries.
    let x1 = (min_x / LEAF_SIZE).floor() * LEAF_SIZE;
    let y1 = (min_y / LEAF_SIZE).floor() * LEAF_SIZE;
    let x2 = (max_x / LEAF_SIZE).ceil() * LEAF_SIZE;
    let y2 = (max_y / LEAF_SIZE).ceil() * LEAF_SIZE;

    // Ensure power-of-two-ish sizing so splits are balanced.
    let size = (x2 - x1).max(y2 - y1).max(LEAF_SIZE);
    let cx = (x1 + x2) * 0.5;
    let cy = (y1 + y2) * 0.5;

    let mut root = SuperBlock::new(
        cx - size * 0.5,
        cy - size * 0.5,
        cx + size * 0.5,
        cy + size * 0.5,
    );

    for &si in seg_indices {
        root.add_seg(si, segs, vertices);
    }

    root
}

/// Classify a superblock's bbox against a partition line.
/// Returns -1 (all left), +1 (all right), or 0 (straddles).
pub fn box_on_line_side(block: &SuperBlock, part: &Seg, vertices: &[Vertex]) -> i32 {
    let px = vertices[part.linedef_v1].x;
    let py = vertices[part.linedef_v1].y;
    let pdx = part.dx;
    let pdy = part.dy;

    let x1 = block.x1;
    let y1 = block.y1;
    let x2 = block.x2;
    let y2 = block.y2;

    let (p1, p2) = if pdx.abs() < EPSILON {
        // Vertical partition.
        let mut a = if x1 > px { 1 } else { -1 };
        let mut b = if x2 > px { 1 } else { -1 };
        if pdy < 0.0 {
            a = -a;
            b = -b;
        }
        (a, b)
    } else if pdy.abs() < EPSILON {
        // Horizontal partition.
        let mut a = if y1 < py { 1 } else { -1 };
        let mut b = if y2 < py { 1 } else { -1 };
        if pdx < 0.0 {
            a = -a;
            b = -b;
        }
        (a, b)
    } else if pdx * pdy > 0.0 {
        // Positive slope — test bottom-right and top-left corners.
        (
            point_on_line_side(px, py, pdx, pdy, part.dir_len, x1, y2),
            point_on_line_side(px, py, pdx, pdy, part.dir_len, x2, y1),
        )
    } else {
        // Negative slope — test top-right and bottom-left corners.
        (
            point_on_line_side(px, py, pdx, pdy, part.dir_len, x1, y1),
            point_on_line_side(px, py, pdx, pdy, part.dir_len, x2, y2),
        )
    };

    if p1 == p2 { p1 } else { 0 }
}

fn point_on_line_side(
    px: Float,
    py: Float,
    pdx: Float,
    pdy: Float,
    dir_len: Float,
    x: Float,
    y: Float,
) -> i32 {
    let cross = (pdx as f64) * (y as f64 - py as f64) - (pdy as f64) * (x as f64 - px as f64);
    let dist = cross / dir_len as f64;
    if dist.abs() < EPSILON as f64 {
        0
    } else if dist > 0.0 {
        -1
    } else {
        1
    }
}
