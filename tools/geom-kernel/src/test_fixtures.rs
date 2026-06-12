//! Shared dense fixtures for the kernel's test modules.

use crate::flags::LineFlags;
use crate::model::{
    DenseLineDef, DenseMap, DenseSideDef, EditorMap, LineKey, Sector, SectorKey, VertKey, Vertex,
};
use crate::name8::Name8;

pub(crate) fn vtx(x: f32, y: f32) -> Vertex {
    Vertex {
        x,
        y,
    }
}

pub(crate) fn dside(sector: Option<u32>) -> DenseSideDef {
    DenseSideDef {
        x_offset: 0,
        y_offset: 0,
        top_tex: Name8::EMPTY,
        bottom_tex: Name8::EMPTY,
        middle_tex: Name8::EMPTY,
        sector,
    }
}

/// A one-sided line; each test module shims its own flag/sector convention over this.
pub(crate) fn dline_with(v1: u32, v2: u32, flags: LineFlags, sector: Option<u32>) -> DenseLineDef {
    DenseLineDef {
        v1,
        v2,
        flags,
        special: 0,
        tag: 0,
        front: dside(sector),
        back: None,
    }
}

pub(crate) fn def_sector() -> Sector {
    Sector {
        floor_height: 0,
        floor_flat: Name8::EMPTY,
        ceil_height: 128,
        ceil_flat: Name8::EMPTY,
        light_level: 160,
        special: 0,
        tag: 0,
    }
}

/// Keyed map from dense fixtures with `sectors` copies of [`def_sector`].
pub(crate) fn fixture(
    vertices: Vec<Vertex>,
    lines: Vec<DenseLineDef>,
    sectors: usize,
) -> EditorMap {
    EditorMap::from_dense(DenseMap {
        vertices,
        lines,
        sectors: vec![def_sector(); sectors],
        things: Vec::new(),
        required_wads: Vec::new(),
    })
    .expect("fixture refs valid")
}

/// Insertion-order keys (fresh maps only — mirrors the old index fixtures).
pub(crate) fn line_keys(map: &EditorMap) -> Vec<LineKey> {
    map.lines.keys().collect()
}

pub(crate) fn vert_keys(map: &EditorMap) -> Vec<VertKey> {
    map.vertices.keys().collect()
}

pub(crate) fn sector_keys(map: &EditorMap) -> Vec<SectorKey> {
    map.sectors.keys().collect()
}
