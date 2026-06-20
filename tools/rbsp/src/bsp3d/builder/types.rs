//! Construction-time data types and shared helpers for the 3D-BSP builder.
//!
//! None of these survive to runtime: the builder accumulates [`BuildPolygon`]s
//! and [`BuildLeaf`]s, then the condense step flattens them into the serializable
//! [`Bsp3dLump`](crate::bsp3d::lump::Bsp3dLump).

use glam::Vec3;

use crate::types::Side;

/// Vertex deduplication grid cell size. rbsp already deduplicates at 1e-5,
/// so this only needs to catch floating-point drift from f64→f32 conversion.
pub const QUANT_PRECISION: f32 = 0.001;
/// Heights within this of each other count as equal (zero-height detection).
pub const HEIGHT_EPSILON: f32 = 0.1;
/// Minimum cross-product magnitude for a non-degenerate polygon.
pub(crate) const MIN_TRI_CROSS: f32 = 1e-4;

/// Build-time wall slot. The lump never stores this — the engine's resolve
/// step derives it from quad z vs live sector heights.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum WallType {
    Upper,
    Lower,
    Middle,
}

/// A wall quad edge's z at the seg's two endpoints (equal when not sloped).
#[derive(Debug, Clone, Copy)]
pub(crate) struct WallEdge {
    pub(crate) start: f32,
    pub(crate) end: f32,
}

impl WallEdge {
    /// A level edge at one height.
    pub(crate) fn flat(z: f32) -> Self {
        Self {
            start: z,
            end: z,
        }
    }

    /// Midpoint height, for comparisons that need a single value.
    pub(crate) fn mean(self) -> f32 {
        (self.start + self.end) * 0.5
    }
}

/// Construction-time polygon kind. Everything texture/slot/peg-related is
/// derived by the engine at resolve time from the linedef/sidedef indices.
#[derive(Clone)]
pub(crate) enum BuildKind {
    Wall {
        linedef: u32,
        sidedef: u32,
        linedef_side: Side,
        wall_type: WallType,
        sky_filler: bool,
        seg_offset: f32,
    },
    Flat,
}

/// Construction-time polygon: a mutable vertex index list plus the inputs the
/// mover pass and the condense step need.
#[derive(Clone)]
pub(crate) struct BuildPolygon {
    pub(crate) sector_id: usize,
    pub(crate) vertices: Vec<usize>,
    pub(crate) kind: BuildKind,
    pub(crate) moves: bool,
}

impl BuildPolygon {
    pub(crate) fn is_wall(&self) -> bool {
        matches!(self.kind, BuildKind::Wall { .. })
    }
}

/// Construction-time leaf: own polygons (creation order, condensed into a
/// contiguous range) plus floor/ceiling buckets for the mover pass and the
/// shared walls owned by adjacent leaves.
#[derive(Default, Clone)]
pub(crate) struct BuildLeaf {
    pub(crate) sector_id: usize,
    /// Own polygons (global indices into [`Bsp3dBuilder::polygons`](super::Bsp3dBuilder)).
    pub(crate) polys: Vec<usize>,
    /// Subset of `polys`: floor flats.
    pub(crate) floor_polygons: Vec<usize>,
    /// Subset of `polys`: ceiling flats.
    pub(crate) ceiling_polygons: Vec<usize>,
    /// Two-sided walls owned by an adjacent leaf, visible from this one when a
    /// mover inverts them.
    pub(crate) shared: Vec<usize>,
}

/// Construction-only record tracking zero-height wall vertex roles.
/// Needed because zh walls have bottom and top at the same (x,y,z) — with
/// position-only dedup they'd share one index, producing degenerate triangles.
/// Fresh vertices are created instead, and this record tells the post-pass
/// which vertices are bottom (front sector) vs top (back sector).
#[derive(Clone)]
pub(crate) struct ZhWallRecord {
    /// Global index into [`Bsp3dBuilder::polygons`](super::Bsp3dBuilder).
    pub(crate) poly_index: usize,
    /// Vertex indices for the bottom edge [start, end].
    pub(crate) bottom: [usize; 2],
    /// Vertex indices for the top edge [start, end].
    pub(crate) top: [usize; 2],
    /// Wall type (Upper/Lower/Middle).
    pub(crate) wall_type: WallType,
    /// Front sector of the seg.
    pub(crate) front_sector: usize,
    /// Back sector of the seg.
    pub(crate) back_sector: usize,
}

/// Bit-exact Vec3 key for vertex deduplication. Two vertices share an index
/// only if their quantized coordinates match.
#[derive(Hash, PartialEq, Eq, Clone, Copy)]
pub(crate) struct QuantizedVec3 {
    x: i32,
    y: i32,
    z: i32,
}

impl QuantizedVec3 {
    pub(crate) fn from_vec3(v: Vec3, precision: f32) -> Self {
        Self {
            x: (v.x / precision).round() as i32,
            y: (v.y / precision).round() as i32,
            z: (v.z / precision).round() as i32,
        }
    }
}

/// Compute shoelace signed area from vertex indices in XY.
pub(crate) fn vertex_shoelace(indices: &[usize], vertices: &[Vec3]) -> f32 {
    let n = indices.len();
    (0..n)
        .map(|i| {
            let a = vertices[indices[i]];
            let b = vertices[indices[(i + 1) % n]];
            a.x * b.y - b.x * a.y
        })
        .sum()
}
