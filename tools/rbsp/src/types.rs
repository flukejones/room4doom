//! Type definitions for the BSP builder.
//!
//! Uses `Float` type alias (f64 default, f32 with `f32` feature) for storage.
//! Cross-product calculations always widen to f64 regardless of Float.
//! WAD packed types (i16) are used only for input deserialization and output.

/// Precision type alias. f64 by default, f32 with the `f32` feature.
#[cfg(feature = "f32")]
pub type Float = f32;
#[cfg(not(feature = "f32"))]
pub type Float = f64;

#[cfg(feature = "f32")]
pub const EPSILON: Float = 0.001;
#[cfg(not(feature = "f32"))]
pub const EPSILON: Float = 1e-7;

#[cfg(feature = "f32")]
pub const VERTEX_EPSILON: Float = 0.001;
#[cfg(not(feature = "f32"))]
pub const VERTEX_EPSILON: Float = 0.001;

#[cfg(feature = "f32")]
pub const PARALLEL_EPSILON: Float = 1e-6;
#[cfg(not(feature = "f32"))]
pub const PARALLEL_EPSILON: Float = 1e-6;

pub const MARGIN: Float = 64.0;

#[cfg(feature = "f32")]
pub const SPLIT_WEIGHT: Float = 10.0;
#[cfg(not(feature = "f32"))]
pub const SPLIT_WEIGHT: Float = 10.0;

/// Subsector flag for encoded `NodeChild` (bit 31).
pub const IS_SSECTOR_MASK: u32 = 0x8000_0000;

#[derive(Debug, Clone, Copy)]
pub struct Vertex {
    pub x: Float,
    pub y: Float,
}

impl Vertex {
    pub fn from_wad(v: &WadVertex) -> Self {
        Self {
            x: v.x as Float,
            y: v.y as Float,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BBox {
    pub min_x: Float,
    pub min_y: Float,
    pub max_x: Float,
    pub max_y: Float,
}

impl BBox {
    #[cfg(feature = "f32")]
    pub const EMPTY: Self = Self {
        min_x: f32::MAX,
        min_y: f32::MAX,
        max_x: f32::MIN,
        max_y: f32::MIN,
    };
    #[cfg(not(feature = "f32"))]
    pub const EMPTY: Self = Self {
        min_x: f64::MAX,
        min_y: f64::MAX,
        max_x: f64::MIN,
        max_y: f64::MIN,
    };

    /// Compute the union of two bounding boxes.
    pub fn union(a: &Self, b: &Self) -> Self {
        Self {
            min_x: a.min_x.min(b.min_x),
            min_y: a.min_y.min(b.min_y),
            max_x: a.max_x.max(b.max_x),
            max_y: a.max_y.max(b.max_y),
        }
    }
}

/// Which side of a partition line a point lies on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointSide {
    Left,
    Right,
    OnLine,
}

/// Which side of a partition line a seg lies on, or if it straddles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegSide {
    Left,
    Right,
    Split,
}

/// Which side of a linedef a seg represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Front,
    Back,
}

/// Internal seg used during BSP construction.
///
/// `dx`, `dy` preserve the original linedef direction (never recomputed
/// after splits) so that all fragments of the same linedef define the
/// same infinite partition line. `linedef_v1` is the original linedef's
/// start vertex — used as the partition origin to avoid float drift.
#[derive(Debug, Clone)]
pub struct Seg {
    pub start: usize,
    pub end: usize,
    pub linedef: usize,
    pub side: Side,
    pub sector: usize,
    pub offset: Float,
    pub angle: Float,
    pub dx: Float,
    pub dy: Float,
    pub len: Float,
    /// Length of the linedef direction vector (dx, dy). Invariant across splits
    /// since dx/dy are preserved from the parent linedef.
    pub dir_len: Float,
    /// Original linedef start vertex (dedup'd index). Used as partition
    /// origin point to avoid floating-point drift from split vertices.
    pub linedef_v1: usize,
}

/// BSP node with partition line and child bounding boxes.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub x: Float,
    pub y: Float,
    pub dx: Float,
    pub dy: Float,
    pub bbox_right: BBox,
    pub bbox_left: BBox,
    pub child_right: u32,
    pub child_left: u32,
}

/// Convex clip polygon threaded through the BSP recursion.
///
/// Vertex indices reference the shared `HiVertex` array.
#[derive(Debug, Clone)]
pub struct ClipPoly {
    pub verts: Vec<u32>,
}

/// Packed reference to a subsector's convex polygon in the output arrays.
#[derive(Debug, Clone, Copy)]
pub struct ConvexPoly {
    pub first_vertex: u32,
    pub num_vertices: u32,
    pub first_edge: u32,
}

/// Whether a polygon edge corresponds to a real seg or a boundary miniseg.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeKind {
    Seg,
    Miniseg,
}

/// A polygon edge in the output, linking to its source seg and adjacent
/// subsector.
#[derive(Debug, Clone, Copy)]
pub struct Edge {
    pub kind: EdgeKind,
    pub start_vertex: u32,
    pub end_vertex: u32,
    pub seg: u32,
    pub partner_leaf: u32,
}

impl Edge {
    /// Sentinel value: this edge has no associated seg.
    pub const NONE_SEG: u32 = u32::MAX;
    /// Sentinel value: this edge has no linked partner subsector.
    pub const NONE_PARTNER: u32 = u32::MAX;
}

/// A BSP leaf containing a convex region of segs from one or more sectors.
#[derive(Debug, Clone)]
pub struct SubSector {
    pub sector: u32,
    pub polygon: ConvexPoly,
    pub first_seg: u32,
    pub num_segs: u32,
    /// Indices into the global seg array for all segs in this subsector.
    /// These may not be contiguous in the global array.
    pub seg_indices: Vec<u32>,
}

impl SubSector {
    /// Sentinel value: sector has not yet been assigned.
    pub const UNASSIGNED_SECTOR: u32 = u32::MAX;
}

/// A wall-tip record for sector assignment at a vertex.
///
/// At each vertex, sorted wall-tips record which sectors adjoin at each angle.
/// Used to assign sectors to seg-less subsectors.
#[derive(Debug, Clone)]
pub struct WallTip {
    pub angle: f64,
    pub front: Option<usize>,
    pub back: Option<usize>,
}

// --- WAD input types ---
// When the `wad-types` feature is enabled (default), use the wad crate's types
// directly. Otherwise, use built-in packed types for standalone use.

#[cfg(feature = "wad-types")]
pub use wad::types::{WadLineDef, WadSector, WadSideDef, WadVertex};

#[cfg(not(feature = "wad-types"))]
mod builtin_wad_types {
    /// Packed WAD vertex (i16 coordinates).
    #[repr(C, packed)]
    #[derive(Debug, Clone, Copy)]
    pub struct WadVertex {
        pub x: i16,
        pub y: i16,
    }

    /// Packed WAD linedef.
    #[repr(C, packed)]
    #[derive(Debug, Clone, Copy)]
    pub struct WadLineDef {
        pub start: i16,
        pub end: i16,
        pub flags: i16,
        pub special: i16,
        pub tag: i16,
        pub sidedef1: i16,
        pub sidedef2: i16,
    }

    /// Packed WAD sidedef.
    #[repr(C, packed)]
    #[derive(Debug, Clone, Copy)]
    pub struct WadSideDef {
        pub xoff: i16,
        pub yoff: i16,
        pub tex_upper: [u8; 8],
        pub tex_lower: [u8; 8],
        pub tex_middle: [u8; 8],
        pub sector: i16,
    }

    /// Packed WAD sector.
    #[repr(C, packed)]
    #[derive(Debug, Clone, Copy)]
    pub struct WadSector {
        pub floor_height: i16,
        pub ceiling_height: i16,
        pub floor_texture: [u8; 8],
        pub ceiling_texture: [u8; 8],
        pub light: i16,
        pub special: i16,
        pub tag: i16,
    }
}

#[cfg(not(feature = "wad-types"))]
pub use builtin_wad_types::*;

// --- Accessor traits to normalize field access across WAD type variants ---

/// Uniform access to vertex coordinates as f64.
pub trait VertexCoords {
    fn x_f64(&self) -> f64;
    fn y_f64(&self) -> f64;
}

/// Uniform access to linedef fields.
pub trait LineDefAccess {
    fn start_vertex_idx(&self) -> usize;
    fn end_vertex_idx(&self) -> usize;
    fn front_sidedef_idx(&self) -> Option<usize>;
    fn back_sidedef_idx(&self) -> Option<usize>;
    fn flags_u32(&self) -> u32;
    fn special_u32(&self) -> u32;
    fn tag_i16(&self) -> i16;
}

/// Uniform access to sidedef fields. Texture presence is by name (`"-"` =
/// none), not by engine texture-list resolution — a sidedef naming a missing
/// texture counts as textured here.
pub trait SideDefAccess {
    fn sector_idx(&self) -> usize;
    fn has_top_tex(&self) -> bool;
    fn has_bottom_tex(&self) -> bool;
    fn has_mid_tex(&self) -> bool;
}

/// Sector floor/ceiling plane `a*x + b*y + c*z + d = 0`. `z` at a point is
/// `-(a*x + b*y + d) / c`. Floor normals point up, ceilings down.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlopePlane {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
}

impl SlopePlane {
    /// Build a floor/ceiling plane, rejecting a degenerate vertical one
    /// (`c == 0`) which has no single z per `(x, y)`.
    pub fn new(a: f32, b: f32, c: f32, d: f32) -> Option<Self> {
        (c.abs() > f32::EPSILON).then_some(Self {
            a,
            b,
            c,
            d,
        })
    }

    /// Plane z at `(x, y)`.
    pub fn z_at(&self, x: f32, y: f32) -> f32 {
        -(self.a * x + self.b * y + self.d) / self.c
    }
}

/// Uniform access to sector fields.
pub trait SectorAccess {
    fn floor_h(&self) -> f32;
    fn ceil_h(&self) -> f32;
    fn tag_i16(&self) -> i16;
    fn floor_tex_is(&self, name: &str) -> bool;
    fn ceil_tex_is(&self, name: &str) -> bool;
    /// Sloped floor plane; `None` = flat at `floor_h`.
    fn floor_plane(&self) -> Option<SlopePlane> {
        None
    }
    /// Sloped ceiling plane; `None` = flat at `ceil_h`.
    fn ceil_plane(&self) -> Option<SlopePlane> {
        None
    }
}

#[cfg(feature = "wad-types")]
impl VertexCoords for WadVertex {
    fn x_f64(&self) -> f64 {
        self.x as f64
    }
    fn y_f64(&self) -> f64 {
        self.y as f64
    }
}

#[cfg(feature = "wad-types")]
impl LineDefAccess for WadLineDef {
    fn start_vertex_idx(&self) -> usize {
        self.start_vertex as usize
    }
    fn end_vertex_idx(&self) -> usize {
        self.end_vertex as usize
    }
    fn front_sidedef_idx(&self) -> Option<usize> {
        if self.front_sidedef < u16::MAX {
            Some(self.front_sidedef as usize)
        } else {
            None
        }
    }
    fn back_sidedef_idx(&self) -> Option<usize> {
        self.back_sidedef.map(|s| s as usize)
    }
    fn flags_u32(&self) -> u32 {
        self.flags as u32
    }
    fn special_u32(&self) -> u32 {
        self.special as u32
    }
    fn tag_i16(&self) -> i16 {
        self.sector_tag
    }
}

#[cfg(feature = "wad-types")]
fn tex_name_present(name: &str) -> bool {
    !name.is_empty() && name != "-"
}

#[cfg(feature = "wad-types")]
impl SideDefAccess for WadSideDef {
    fn sector_idx(&self) -> usize {
        self.sector as usize
    }
    fn has_top_tex(&self) -> bool {
        tex_name_present(&self.upper_tex)
    }
    fn has_bottom_tex(&self) -> bool {
        tex_name_present(&self.lower_tex)
    }
    fn has_mid_tex(&self) -> bool {
        tex_name_present(&self.middle_tex)
    }
}

#[cfg(feature = "wad-types")]
impl SectorAccess for WadSector {
    fn floor_h(&self) -> f32 {
        self.floor_height as f32
    }
    fn ceil_h(&self) -> f32 {
        self.ceil_height as f32
    }
    fn tag_i16(&self) -> i16 {
        self.tag
    }
    fn floor_tex_is(&self, name: &str) -> bool {
        self.floor_tex.eq_ignore_ascii_case(name)
    }
    fn ceil_tex_is(&self, name: &str) -> bool {
        self.ceil_tex.eq_ignore_ascii_case(name)
    }
}

#[cfg(not(feature = "wad-types"))]
impl VertexCoords for WadVertex {
    fn x_f64(&self) -> f64 {
        self.x as f64
    }
    fn y_f64(&self) -> f64 {
        self.y as f64
    }
}

#[cfg(not(feature = "wad-types"))]
impl LineDefAccess for WadLineDef {
    fn start_vertex_idx(&self) -> usize {
        self.start as usize
    }
    fn end_vertex_idx(&self) -> usize {
        self.end as usize
    }
    fn front_sidedef_idx(&self) -> Option<usize> {
        if self.sidedef1 >= 0 {
            Some(self.sidedef1 as usize)
        } else {
            None
        }
    }
    fn back_sidedef_idx(&self) -> Option<usize> {
        if self.sidedef2 >= 0 {
            Some(self.sidedef2 as usize)
        } else {
            None
        }
    }
    fn flags_u32(&self) -> u32 {
        let flags = self.flags;
        flags as u16 as u32
    }
    fn special_u32(&self) -> u32 {
        let special = self.special;
        special as u32
    }
    fn tag_i16(&self) -> i16 {
        self.tag
    }
}

#[cfg(not(feature = "wad-types"))]
fn tex_bytes_present(name: &[u8; 8]) -> bool {
    name[0] != 0 && name[0] != b'-'
}

#[cfg(not(feature = "wad-types"))]
impl SideDefAccess for WadSideDef {
    fn sector_idx(&self) -> usize {
        self.sector as usize
    }
    fn has_top_tex(&self) -> bool {
        let tex = self.tex_upper;
        tex_bytes_present(&tex)
    }
    fn has_bottom_tex(&self) -> bool {
        let tex = self.tex_lower;
        tex_bytes_present(&tex)
    }
    fn has_mid_tex(&self) -> bool {
        let tex = self.tex_middle;
        tex_bytes_present(&tex)
    }
}

#[cfg(not(feature = "wad-types"))]
fn tex_bytes_is(name: &[u8; 8], want: &str) -> bool {
    let want = want.as_bytes();
    let len = name.iter().position(|&b| b == 0).unwrap_or(8);
    name[..len].eq_ignore_ascii_case(want)
}

#[cfg(not(feature = "wad-types"))]
impl SectorAccess for WadSector {
    fn floor_h(&self) -> f32 {
        let h = self.floor_height;
        h as f32
    }
    fn ceil_h(&self) -> f32 {
        let h = self.ceiling_height;
        h as f32
    }
    fn tag_i16(&self) -> i16 {
        self.tag
    }
    fn floor_tex_is(&self, name: &str) -> bool {
        let tex = self.floor_texture;
        tex_bytes_is(&tex, name)
    }
    fn ceil_tex_is(&self, name: &str) -> bool {
        let tex = self.ceiling_texture;
        tex_bytes_is(&tex, name)
    }
}

/// Input geometry for the BSP builder.
pub struct BspInput<V, L, S, SE> {
    pub vertices: Vec<V>,
    pub linedefs: Vec<L>,
    pub sidedefs: Vec<S>,
    pub sectors: Vec<SE>,
}

/// Configuration options for the BSP builder.
pub struct BspOptions {
    /// Split cost multiplier (glBSP calls this "factor"). Default 11.
    pub split_weight: Float,
    /// Also emit the classic 2D NODE section in the RBSP lump.
    pub classic_nodes: bool,
}

impl Default for BspOptions {
    fn default() -> Self {
        Self {
            split_weight: SPLIT_WEIGHT,
            classic_nodes: false,
        }
    }
}

/// Output of the BSP builder: the complete BSP tree with explicit leaf
/// polygons.
pub struct BspOutput {
    pub vertices: Vec<Vertex>,
    /// Number of original WAD vertices at the start of the vertex array.
    /// Vertices 0..num_original_verts preserve their WAD indices.
    /// Vertices num_original_verts.. are new split vertices from BSP building.
    pub num_original_verts: usize,
    pub segs: Vec<Seg>,
    pub subsectors: Vec<SubSector>,
    pub nodes: Vec<Node>,
    pub root: u32,
    pub poly_indices: Vec<u32>,
}
