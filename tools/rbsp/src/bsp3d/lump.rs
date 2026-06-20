//! The serializable 3D-BSP data: flat, index-based records with no pointers.
//! [`Bsp3dLump`] is what the builder emits and what the runtime parses; it is
//! the disk format for the RBSP lump's 3D sections.

use crate::types::{Node, Side};
use bitflags::bitflags;
use glam::Vec3;

/// Sentinel for "no index" in [`PolyRecord`] fields.
pub const NO_INDEX: u32 = u32::MAX;

bitflags! {
    /// Per-polygon flags. Only [`PolyFlags::LUMP_BITS`] are build outputs that
    /// get serialized; the remaining bits are resolved at parse/event time from
    /// sidedef/linedef/sector data and never stored on disk.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct PolyFlags: u8 {
        /// Mover-pass output: polygon belongs to (or borders) a moving surface.
        const MOVES = 1;
        /// Synthetic sky geometry extending sky-sector perimeter walls.
        const SKY_FILLER = 1 << 1;
        /// Horizontal surface (floor/ceiling); vertical wall otherwise.
        const IS_FLAT = 1 << 2;
        /// Drawn as sky (sky-filler wall, or flat whose pic is the sky flat).
        const SKY = 1 << 3;
        /// Two-sided middle: drawn once, v outside [0,1) discarded (not tiled).
        const MASKED_MIDDLE = 1 << 4;
        /// BOOM linedef special 260 translucent middle.
        const TRANSLUCENT = 1 << 5;
        /// Wall on a linedef with a back sidedef.
        const TWO_SIDED = 1 << 6;
        /// A mover inverted this wall: its winding opposes the parse-time
        /// normal and the away side faces the viewer. Cached at resolve —
        /// flip state only changes when vertices move, and every move
        /// re-resolves.
        const FLIPPED = 1 << 7;
    }
}

impl PolyFlags {
    /// Bits persisted in the lump; everything else is derived at parse.
    pub const LUMP_BITS: Self = Self::MOVES.union(Self::SKY_FILLER);
}

/// One polygon: a contiguous slice of [`Bsp3dLump::poly_verts`] plus the map
/// objects it derives from. Everything else (sector, back sidedef, wall slot,
/// textures, UV, normals, AABBs) is derived at parse or resolve time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PolyRecord {
    pub vert_start: u32,
    pub vert_count: u16,
    /// MOVES | SKY_FILLER only — see [`PolyFlags::LUMP_BITS`].
    pub flags: PolyFlags,
    /// Linedef index; [`NO_INDEX`] = flat.
    pub linedef: u32,
    /// The building seg's sidedef index; [`NO_INDEX`] = flat. Sky fillers keep
    /// their seg's sidedef (front sector derives from it).
    pub sidedef: u32,
    /// Which linedef side the wall faces. `Front` for flats (unused). Like 2D `Seg.side`.
    pub linedef_side: Side,
    /// U anchor along the linedef (front traversal), in map units.
    pub seg_offset: f32,
}

impl PolyRecord {
    pub const fn is_flat(&self) -> bool {
        self.linedef == NO_INDEX
    }

    /// Wall faces the linedef front side. Walls only.
    pub const fn is_front(&self) -> bool {
        matches!(self.linedef_side, Side::Front)
    }
}

/// One BSP leaf.
///
/// The leaf's own polygons are a contiguous range of [`Bsp3dLump::polys`];
/// cross-subsector shared walls (mover-invertible two-sided quads owned by an
/// adjacent leaf) are a range of [`Bsp3dLump::shared_walls`].
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LeafRecord {
    /// The 2D subsector this leaf belongs to (identity for 2.5D input).
    pub subsector: u32,
    pub poly_start: u32,
    pub poly_count: u16,
    pub shared_start: u32,
    pub shared_count: u16,
}

/// One node of the unified 3D tree. Children flag leaves with the high bit;
/// either kind may appear at any depth.
#[derive(Debug, Clone, PartialEq)]
pub enum TreeNode {
    /// Vertical partition from the 2D pass: exact line coords for the
    /// fixed-point gameplay side tests plus the vanilla cull bboxes.
    Vertical(Node),
    /// General partition plane.
    Plane {
        normal: [f32; 3],
        d: f32,
        children: [u32; 2],
    },
}

/// Flat 3D-BSP geometry as emitted by the builder, including the unified
/// traversal tree.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Bsp3dLump {
    /// Unified 3D tree; root is the last node (empty = single-leaf map).
    pub tree: Vec<TreeNode>,
    pub vertices: Vec<Vec3>,
    /// Flat polygon vertex indices; each poly is a contiguous slice.
    pub poly_verts: Vec<u32>,
    /// Leaf-contiguous: each leaf's own polys form one range.
    pub polys: Vec<PolyRecord>,
    pub leaves: Vec<LeafRecord>,
    /// Flat shared-wall polygon indices; leaves reference by range.
    pub shared_walls: Vec<u32>,
}

impl Bsp3dLump {
    /// Vertex indices of polygon `gi` (slice into [`Self::poly_verts`]).
    pub fn poly_vert_indices(&self, gi: usize) -> &[u32] {
        let p = &self.polys[gi];
        let s = p.vert_start as usize;
        &self.poly_verts[s..s + p.vert_count as usize]
    }

    /// Fan triangles of polygon `gi`: `(v0, vi, vi+1)` as vertex indices. Empty
    /// for polygons with fewer than 3 vertices.
    pub fn poly_triangles(&self, gi: usize) -> impl Iterator<Item = [u32; 3]> + '_ {
        let idx = self.poly_vert_indices(gi);
        (1..idx.len().saturating_sub(1)).map(move |i| [idx[0], idx[i], idx[i + 1]])
    }

    /// Fan triangles of every polygon owned by leaf `i`, in poly order.
    pub fn leaf_triangles(&self, i: usize) -> impl Iterator<Item = [u32; 3]> + '_ {
        let leaf = &self.leaves[i];
        let range = leaf.poly_start as usize..leaf.poly_start as usize + leaf.poly_count as usize;
        range.flat_map(move |gi| self.poly_triangles(gi))
    }

    /// Vertex-index rings of every polygon owned by leaf `i`, in poly order.
    pub fn leaf_ngons(&self, i: usize) -> impl Iterator<Item = &[u32]> + '_ {
        let leaf = &self.leaves[i];
        let range = leaf.poly_start as usize..leaf.poly_start as usize + leaf.poly_count as usize;
        range.map(move |gi| self.poly_vert_indices(gi))
    }

    /// Fan triangulation of the whole map, in poly order — the GPU-ready fan
    /// index buffer parallel to [`Self::polys`].
    pub fn triangles(&self) -> Vec<[u32; 3]> {
        let tri_total: usize = self
            .polys
            .iter()
            .map(|p| (p.vert_count as usize).saturating_sub(2))
            .sum();
        let mut out = Vec::with_capacity(tri_total);
        for gi in 0..self.polys.len() {
            out.extend(self.poly_triangles(gi));
        }
        out
    }

    /// Vertex-index rings of the whole map, in poly order.
    pub fn poly_ngons(&self) -> impl Iterator<Item = &[u32]> + '_ {
        (0..self.polys.len()).map(move |gi| self.poly_vert_indices(gi))
    }
}

/// Lift the 2D pass's nodes into the unified tree (all vertical).
pub fn tree_from_nodes(nodes: &[Node]) -> Vec<TreeNode> {
    nodes
        .iter()
        .map(|n| TreeNode::Vertical(n.clone()))
        .collect()
}
