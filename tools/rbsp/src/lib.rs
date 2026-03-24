//! # rbsp — High-precision BSP node builder
//!
//! Builds BSP trees from Doom-format level geometry, producing nodes, segs,
//! and explicit leaf polygons in a single pass. All internal computation and
//! output uses f64 precision.
//!
//! ## Usage
//!
//! ```no_run
//! use rbsp::{BspInput, BspOptions, build_bsp};
//!
//! let input = BspInput {
//!     vertices: vec![/* WadVertex ... */],
//!     linedefs: vec![/* WadLineDef ... */],
//!     sidedefs: vec![/* WadSideDef ... */],
//!     sectors:  vec![/* WadSector ... */],
//! };
//! let result = build_bsp(input, &BspOptions::default());
//! ```

pub mod node;
pub mod picknode;
pub mod polygon;
pub mod rbsp_lump;
pub mod seg;
pub mod split;
pub mod superblock;
pub mod types;
pub mod vertex_pool;
#[cfg(feature = "wad-types")]
pub mod wad_io;
pub mod walltip;

pub use types::*;

use std::time::Instant;

use node::BuildState;
use vertex_pool::VertexPool;

/// Build a BSP tree from level geometry.
///
/// Produces f64-precision output: vertices, segs, subsectors with explicit
/// polygons, nodes, edges, and polygon vertex indices.
pub fn build_bsp(input: BspInput, options: &BspOptions) -> BspOutput {
    let total_start = Instant::now();
    let mut pool = VertexPool::new();

    // Insert WAD vertices without dedup — preserves original indices so
    // linedefs can reference them directly without remapping.
    let num_original_verts = input.vertices.len();
    for wv in &input.vertices {
        pool.insert(wv.x as Float, wv.y as Float);
    }

    let mut wall_tips = walltip::build_wall_tips(
        &input.linedefs,
        &input.sidedefs,
        &pool.vertices,
        pool.vertices.len(),
    );

    let (mut segs, seg_indices) =
        seg::create_segs(&input.linedefs, &input.sidedefs, &pool.vertices);
    segs.reserve(segs.len() / 2);

    log::info!(
        "Initial: {} segs from {} linedefs",
        segs.len(),
        input.linedefs.len(),
    );

    let bounds = seg::find_map_bounds(&seg_indices, &segs, &pool.vertices);
    let clip_poly = polygon::make_initial_clip_poly(
        bounds.min_x,
        bounds.min_y,
        bounds.max_x,
        bounds.max_y,
        &mut pool,
    );

    // Build BSP tree — estimate capacities from input seg count.
    let seg_count = segs.len();
    let mut nodes = Vec::with_capacity(seg_count);
    let mut subsectors = Vec::with_capacity(seg_count);
    let mut poly_indices = Vec::with_capacity(seg_count * 2);
    let mut edges = Vec::with_capacity(seg_count * 2);

    let build_start = Instant::now();

    let mut bs = BuildState {
        pool: &mut pool,
        segs: &mut segs,
        nodes: &mut nodes,
        subsectors: &mut subsectors,
        poly_indices: &mut poly_indices,
        edges: &mut edges,
        linedefs: &input.linedefs,
        sidedefs: &input.sidedefs,
        wall_tips: &mut wall_tips,
        options,
        start_time: build_start,
    };

    let root = node::build_node(seg_indices, clip_poly, &mut bs, 0);

    let build_elapsed = build_start.elapsed();
    print!(
        "\r  rBSP: {} ss, {} segs, {} nodes, {} verts [{:.2}s]",
        subsectors.len(),
        segs.len(),
        nodes.len(),
        pool.vertices.len(),
        build_elapsed.as_secs_f64(),
    );

    // Top-down pass: share boundary vertices between sibling subtrees
    node::share_all_boundary_vertices(
        &nodes,
        &mut subsectors,
        &mut poly_indices,
        &mut edges,
        &pool,
    );

    let total_elapsed = total_start.elapsed();
    println!(
        "\n  rBSP done [{:.2}s total] ({:.2}s build)",
        total_elapsed.as_secs_f64(),
        build_elapsed.as_secs_f64(),
    );

    // Compact vertices: remove unreferenced entries, remap all indices.
    let vertices = compact_vertices(pool.vertices, &mut segs, &mut poly_indices, &input.linedefs);

    BspOutput {
        vertices,
        num_original_verts,
        segs,
        subsectors,
        nodes,
        root,
        poly_indices,
    }
}

fn compact_vertices(
    vertices: Vec<Vertex>,
    segs: &mut Vec<Seg>,
    poly_indices: &mut Vec<u32>,
    linedefs: &[WadLineDef],
) -> Vec<Vertex> {
    let n = vertices.len();
    let mut used = vec![false; n];

    for ld in linedefs {
        used[ld.start_vertex_idx()] = true;
        used[ld.end_vertex_idx()] = true;
    }
    for seg in segs.iter() {
        used[seg.start] = true;
        used[seg.end] = true;
    }
    for &vi in poly_indices.iter() {
        used[vi as usize] = true;
    }

    let mut old_to_new = vec![u32::MAX; n];
    let mut new_vertices = Vec::with_capacity(n);
    for (old_idx, &is_used) in used.iter().enumerate() {
        if is_used {
            old_to_new[old_idx] = new_vertices.len() as u32;
            new_vertices.push(vertices[old_idx]);
        }
    }

    let removed = n - new_vertices.len();
    if removed == 0 {
        return vertices;
    }

    for seg in segs.iter_mut() {
        seg.start = old_to_new[seg.start] as usize;
        seg.end = old_to_new[seg.end] as usize;
    }
    for vi in poly_indices.iter_mut() {
        *vi = old_to_new[*vi as usize];
    }

    log::info!(
        "Compacted vertices: {} -> {} ({} removed)",
        n,
        new_vertices.len(),
        removed
    );

    new_vertices
}
