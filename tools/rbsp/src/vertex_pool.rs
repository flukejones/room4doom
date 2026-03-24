//! Shared vertex deduplication with spatial hash lookup.

use std::collections::HashMap;

use crate::types::{Float, Vertex, VERTEX_EPSILON};

const GRID_SCALE: Float = 1.0 / VERTEX_EPSILON;

fn grid_key(x: Float, y: Float) -> (i32, i32) {
    (
        (x * GRID_SCALE).floor() as i32,
        (y * GRID_SCALE).floor() as i32,
    )
}

/// Shared vertex array with O(1) deduplication via spatial hashing.
pub struct VertexPool {
    pub vertices: Vec<Vertex>,
    grid: HashMap<(i32, i32), Vec<u32>>,
}

impl VertexPool {
    pub fn new() -> Self {
        Self {
            vertices: Vec::new(),
            grid: HashMap::new(),
        }
    }

    /// Insert a vertex without dedup. Guarantees sequential index assignment.
    pub fn insert(&mut self, x: Float, y: Float) -> u32 {
        let idx = self.vertices.len() as u32;
        let (gx, gy) = grid_key(x, y);
        self.vertices.push(Vertex {
            x,
            y,
        });
        self.grid.entry((gx, gy)).or_default().push(idx);
        idx
    }

    /// Insert a vertex, deduplicating by proximity. Returns the index.
    pub fn dedup(&mut self, x: Float, y: Float) -> u32 {
        let (gx, gy) = grid_key(x, y);

        // Check the cell and its 8 neighbors
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(indices) = self.grid.get(&(gx + dx, gy + dy)) {
                    for &idx in indices {
                        let v = &self.vertices[idx as usize];
                        if (v.x - x).abs() < VERTEX_EPSILON && (v.y - y).abs() < VERTEX_EPSILON {
                            return idx;
                        }
                    }
                }
            }
        }

        let idx = self.vertices.len() as u32;
        self.vertices.push(Vertex {
            x,
            y,
        });
        self.grid.entry((gx, gy)).or_default().push(idx);
        idx
    }
}
