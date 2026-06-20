//! Runtime 3D-BSP structure, parsed from [`rbsp::bsp3d::Bsp3dLump`].
//!
//! Hot render data is flat SoA (`poly_tex`, `poly_flags`, UV, triangles);
//! per-polygon [`MapPtr`]s exist only for event-time re-resolution and light
//! lookup. The surface cache (textures, flag bits, UV) is resolved from
//! sidedef/linedef/sector data at parse and re-resolved on events
//! (`move_surface`, switch/flat texture changes) — never in the render path.

use crate::MapPtr;
use crate::flags::LineDefFlags;
use crate::map_defs::{LineDef, Sector, SideDef};
use glam::{Vec2, Vec3};
use math::{FixedT, r_point_on_side_raw};
use rbsp::bsp3d::{NO_INDEX, PolyFlags};
use std::f32::consts::FRAC_PI_2;
use std::ops::Range;

/// Bitmask that flags a BSP node ID as a leaf rather than an internal node.
pub const IS_LEAF_MASK: u32 = 0x8000_0000;

/// Returns true if this node ID refers to a leaf.
#[inline]
pub const fn is_leaf(node_id: u32) -> bool {
    node_id & IS_LEAF_MASK != 0
}

/// Extracts the leaf index from a node ID (strips the flag bit).
#[inline]
pub const fn leaf_index(node_id: u32) -> usize {
    (node_id & !IS_LEAF_MASK) as usize
}

/// Marks a node ID as a leaf.
#[inline]
pub const fn mark_leaf(index: u32) -> u32 {
    index | IS_LEAF_MASK
}

/// Max Doom light band.
pub const LIGHT_LEVELS: i32 = 15;
/// Rotation applied to horizontal surface texture coordinates (90°).
const HORIZONTAL_TEX_DIRECTION: f32 = FRAC_PI_2;
/// Dirty-poly count where scoped re-fan stops paying off (also the list bound).
const TEXTURE_DIRTY_POLY_CAP: usize = 4096;

/// Fake-contrast brightness delta for axis-aligned walls (N/S −1 darker, E/W +1
/// lighter); 0 otherwise. Shared so the formula has one home.
pub fn contrast_adjust(normal: Vec3) -> i32 {
    let horizontal = normal.z.abs() >= 0.01; // floor/ceiling
    let north_south = normal.x.abs() < 0.001;
    let east_west = normal.y.abs() < 0.001;
    match (horizontal, north_south, east_west) {
        (false, true, _) => -1,
        (false, _, true) => 1,
        _ => 0,
    }
}

/// Final light band (0..15): `(sector_light>>4 + extralight)` capped at 15, then
/// the contrast delta re-clamped. Matches the software3d order exactly.
pub fn light_band(sector_light: usize, extralight: usize, normal: Vec3) -> i32 {
    let base = ((sector_light >> 4) + extralight).min(LIGHT_LEVELS as usize) as i32;
    (base + contrast_adjust(normal)).clamp(0, LIGHT_LEVELS)
}

#[derive(Default, Debug, Clone, Copy, PartialEq)]
pub enum MovementType {
    Floor,
    Ceiling,
    #[default]
    None,
}

/// Wall texture slot, derived at resolve time from quad z vs live back-sector
/// heights (vanilla r_segs.c style). Never stored — see [`BSP3D::wall_slot`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WallSlot {
    Upper,
    Lower,
    Middle,
}

#[derive(Debug)]
pub struct AABB {
    pub min: Vec3,
    pub max: Vec3,
}

impl AABB {
    pub(super) fn new() -> Self {
        Self {
            min: Vec3::new(f32::MAX, f32::MAX, f32::MAX),
            max: Vec3::new(f32::MIN, f32::MIN, f32::MIN),
        }
    }

    pub(super) fn expand_to_include_point(&mut self, point: Vec3) {
        self.min = self.min.min(point);
        self.max = self.max.max(point);
    }

    pub(super) fn expand_to_include_aabb(&mut self, other: &Self) {
        self.min = self.min.min(other.min);
        self.max = self.max.max(other.max);
    }
}

/// Hot walk data only; cull data lives in parallel arrays on [`BSP3D`].
pub struct Node3D {
    /// Partition plane normal; vertical (z=0) for 2D-built nodes.
    pub normal: Vec3,
    pub d: f32,
    /// Partition in fixed-point, precomputed at parse for the OG side tests.
    pub xy_fp: [FixedT; 2],
    pub delta_fp: [FixedT; 2],
    pub children: [u32; 2],
}

impl Node3D {
    pub fn point_on_side_plane(&self, point: Vec3) -> usize {
        usize::from(self.normal.dot(point) <= self.d)
    }

    /// Returns (front_child_id, back_child_id) for the given point.
    /// Front is the child on the same side as the point (closer).
    pub fn front_back_children_plane(&self, point: Vec3) -> (u32, u32) {
        let side = self.point_on_side_plane(point);
        (self.children[side], self.children[side ^ 1])
    }

    /// OG Doom `R_PointOnSide` — 16.16 fixed-point side test matching OG
    /// integer arithmetic exactly. Used for gameplay subsector lookup.
    #[inline]
    pub fn point_on_side_fixed(&self, x: FixedT, y: FixedT) -> usize {
        r_point_on_side_raw(
            x.to_fixed_raw(),
            y.to_fixed_raw(),
            self.xy_fp[0].to_fixed_raw(),
            self.xy_fp[1].to_fixed_raw(),
            self.delta_fp[0].to_fixed_raw(),
            self.delta_fp[1].to_fixed_raw(),
        )
    }

    /// Fixed-point variant of `front_back_children_plane` for gameplay
    /// subsector lookup. Matches OG Doom `R_PointOnSide` exactly.
    pub fn front_back_children_fixed(&self, x: FixedT, y: FixedT) -> (u32, u32) {
        let side = self.point_on_side_fixed(x, y);
        (self.children[side], self.children[side ^ 1])
    }
}

/// Cold per-polygon data: map-object pointers for event-time re-resolution and
/// per-frame light lookup. The render path reads the SoA arrays on [`BSP3D`].
#[derive(Debug)]
pub struct Polygon3D {
    /// Winding normal from the lump: flats ±Z, walls the sidedef side.
    pub normal: Vec3,
    /// Front sector (flats: the only reference set).
    pub sector: MapPtr<Sector>,
    /// Walls only; `None` = flat.
    pub linedef: Option<MapPtr<LineDef>>,
    /// The building seg's sidedef. Walls only (sky fillers keep theirs).
    pub sidedef: Option<MapPtr<SideDef>>,
    /// The linedef's other sidedef, when it has one.
    pub back_sidedef: Option<MapPtr<SideDef>>,
    /// U anchor along the linedef (front traversal), in map units.
    pub seg_offset: f32,
}

pub struct BSPLeaf3D {
    /// The 2D subsector this leaf belongs to (identity for 2.5D input).
    pub subsector: usize,
    pub sector: MapPtr<Sector>,
    pub aabb: AABB,
    /// The leaf's own polygons are the contiguous range
    /// `poly_start..poly_start + poly_count`; render walks it whole.
    pub poly_start: usize,
    pub poly_count: usize,
    /// Range into [`BSP3D::shared_walls`].
    pub shared_start: usize,
    pub shared_count: usize,
}

#[derive(Default)]
pub struct BSP3D {
    pub(super) nodes: Vec<Node3D>,
    /// Per-node `[right, left]` child bounds as `[left-top, right-bottom]` (vanilla `R_CheckBBox`).
    pub(super) node_bboxes: Vec<[[Vec2; 2]; 2]>,
    /// Per-node subtree AABB, built bottom-up from leaf AABBs.
    pub(super) node_aabbs: Vec<AABB>,
    pub(super) root_node: u32,
    /// First general-plane node; vertical (gameplay-walkable) nodes sit below.
    pub(super) first_plane_node: u32,
    pub leaves: Vec<BSPLeaf3D>,
    /// Cross-subsector shared wall polygon indices; leaves reference by range.
    pub shared_walls: Vec<usize>,
    pub vertices: Vec<Vec3>,
    /// Flat polygon vertex indices; [`Self::poly_vertex_range`] slices both
    /// this and [`Self::poly_vertex_uv`].
    pub poly_verts: Vec<usize>,
    /// `[start, end)` per polygon into `poly_verts` / `poly_vertex_uv`.
    pub poly_vertex_range: Vec<(usize, usize)>,
    /// Texel-space UV per polygon vertex. Texel, not normalised; renderers
    /// divide by the texture dims.
    pub poly_vertex_uv: Vec<[f32; 2]>,
    /// Fan triangulation: triples of indices into [`Self::vertices`]. Topology
    /// is stable across moves (movers only change vertex `z`).
    pub triangles: Vec<[u32; 3]>,
    /// Per-polygon texture id of the side currently facing the viewer
    /// (resolve swaps the sides when a mover inverts the wall, so renderers —
    /// including the static-mesh wgpu pair — read this directly).
    /// [`NO_INDEX`] = untextured.
    pub poly_tex: Vec<u32>,
    /// The away side's texture id. [`NO_INDEX`] = untextured.
    pub poly_back_tex: Vec<u32>,
    /// Resolved per-polygon flag bits (see [`PolyFlags`]).
    pub poly_flags: Vec<PolyFlags>,
    /// Per-polygon horizontal texture scroll in texels (special-48 scrollers),
    /// added to U at sample time. Delta beyond the resolve-baked offset.
    pub poly_scroll: Vec<f32>,
    /// Cold per-polygon data (events + light lookup).
    pub polygons: Vec<Polygon3D>,
    pub sector_leaves: Vec<Vec<usize>>,
    /// Event tables (parse-built, never serialized): mover/texture events
    /// index straight to their polygons.
    pub sector_floor_polys: Vec<Vec<usize>>,
    pub sector_ceiling_polys: Vec<Vec<usize>>,
    /// Walls touching each sector: its own plus shared walls whose vertices
    /// track this sector's surfaces.
    pub sector_wall_polys: Vec<Vec<usize>>,
    pub linedef_wall_polys: Vec<Vec<usize>>,
    /// Wall texture heights by texture id, captured at parse (peg anchors).
    pub(super) wall_tex_height: Vec<f32>,
    pub(super) sky_num: Option<usize>,
    /// Set when a surface moves; renderers re-upload only when set.
    pub(super) geometry_dirty: bool,
    /// Set when poly_tex or poly_scroll changed (switch/scroll). Separate from
    /// geometry_dirty: scroll dirties every tic, movement only on move.
    pub(super) texture_dirty: bool,
    /// Polygons behind `texture_dirty`; spills to `texture_dirty_full` at
    /// [`TEXTURE_DIRTY_POLY_CAP`] so a non-draining renderer can't grow it.
    pub(super) texture_dirty_polys: Vec<usize>,
    pub(super) texture_dirty_full: bool,
}

/// Record a dirty polygon, spilling to the full-map flag at the cap.
fn mark_texture_dirty_poly(dirty_polys: &mut Vec<usize>, dirty_full: &mut bool, gi: usize) {
    if *dirty_full {
        return;
    }
    if dirty_polys.len() >= TEXTURE_DIRTY_POLY_CAP {
        *dirty_full = true;
        dirty_polys.clear();
        return;
    }
    dirty_polys.push(gi);
}

impl BSP3D {
    // ------------------------------------------------------------------
    // Accessors
    // ------------------------------------------------------------------

    pub fn nodes(&self) -> &[Node3D] {
        &self.nodes
    }

    /// A node's 2D child bbox, `[left-top, right-bottom]`. `side` 0 = right.
    pub fn node_bbox(&self, node_id: u32, side: usize) -> &[Vec2; 2] {
        &self.node_bboxes[node_id as usize][side]
    }

    pub fn node_bboxes(&self) -> &[[[Vec2; 2]; 2]] {
        &self.node_bboxes
    }

    pub fn root_node(&self) -> u32 {
        self.root_node
    }

    /// OG Doom `R_PointInSubsector` — fixed-point node walk to the containing
    /// leaf. Returns a leaf id; the subsector is `leaves[leaf_id].subsector`.
    /// Plane subtrees are one subsector, so the walk stops at their root.
    pub fn point_in_leaf(&self, x: FixedT, y: FixedT) -> usize {
        let mut node_id = self.root_node;

        while !is_leaf(node_id) {
            if node_id >= self.first_plane_node {
                return self.subtree_leaf(node_id);
            }
            let node = &self.nodes[node_id as usize];
            (node_id, _) = node.front_back_children_fixed(x, y);
        }

        leaf_index(node_id)
    }

    /// First general-plane node id; vertical nodes sit below this index.
    pub fn first_plane_node(&self) -> u32 {
        self.first_plane_node
    }

    /// A leaf of the subtree at `node_id` (all of a plane subtree's leaves
    /// share one subsector, so any branch resolves it).
    pub fn subtree_leaf(&self, mut node_id: u32) -> usize {
        while !is_leaf(node_id) {
            node_id = self.nodes[node_id as usize].children[0];
        }
        leaf_index(node_id)
    }

    pub fn get_leaf(&self, leaf_id: usize) -> Option<&BSPLeaf3D> {
        self.leaves.get(leaf_id)
    }

    /// Indices of a leaf's own polygons (contiguous range) followed by the
    /// shared walls visible from it.
    pub fn leaf_poly_indices(&self, leaf_id: usize) -> impl Iterator<Item = usize> + '_ {
        let leaf = &self.leaves[leaf_id];
        let shared = &self.shared_walls[leaf.shared_start..leaf.shared_start + leaf.shared_count];
        self.leaf_own_polys(leaf_id).chain(shared.iter().copied())
    }

    /// Indices of a leaf's own polygons (the contiguous range only).
    pub fn leaf_own_polys(&self, leaf_id: usize) -> Range<usize> {
        let leaf = &self.leaves[leaf_id];
        leaf.poly_start..leaf.poly_start + leaf.poly_count
    }

    /// A leaf's own floor flats (winding-classified).
    pub fn leaf_floor_polys(&self, leaf_id: usize) -> impl Iterator<Item = usize> + '_ {
        self.leaf_own_polys(leaf_id)
            .filter(|&gi| self.poly_is_flat(gi) && self.polygons[gi].normal.z > 0.0)
    }

    /// A leaf's own ceiling flats (winding-classified).
    pub fn leaf_ceiling_polys(&self, leaf_id: usize) -> impl Iterator<Item = usize> + '_ {
        self.leaf_own_polys(leaf_id)
            .filter(|&gi| self.poly_is_flat(gi) && self.polygons[gi].normal.z < 0.0)
    }

    /// The vertex indices of one polygon.
    #[inline]
    pub fn poly_vert_indices(&self, gi: usize) -> &[usize] {
        let (s, e) = self.poly_vertex_range[gi];
        &self.poly_verts[s..e]
    }

    #[inline(always)]
    pub fn vertex_get(&self, idx: usize) -> Vec3 {
        unsafe { *self.vertices.get_unchecked(idx) }
    }

    #[inline]
    pub fn poly_is_flat(&self, gi: usize) -> bool {
        self.poly_flags[gi].contains(PolyFlags::IS_FLAT)
    }

    #[inline]
    pub fn poly_is_sky(&self, gi: usize) -> bool {
        self.poly_flags[gi].contains(PolyFlags::SKY)
    }

    #[inline]
    pub fn poly_is_masked_middle(&self, gi: usize) -> bool {
        self.poly_flags[gi].contains(PolyFlags::MASKED_MIDDLE)
    }

    #[inline]
    pub fn poly_is_translucent(&self, gi: usize) -> bool {
        self.poly_flags[gi].contains(PolyFlags::TRANSLUCENT)
    }

    /// A moving wall inverts when its floor crosses its ceiling, flipping the
    /// geometric winding against the parse-time normal. The decision is a dot,
    /// not a winding, so it is exact for quads whose vertices were relinked by
    /// the mover pass. Resolve-time only — renderers read the cached
    /// [`PolyFlags::FLIPPED`] bit.
    fn winding_flipped(&self, gi: usize) -> bool {
        let (s, _) = self.poly_vertex_range[gi];
        let p0 = self.vertices[self.poly_verts[s]];
        let p1 = self.vertices[self.poly_verts[s + 1]];
        let p2 = self.vertices[self.poly_verts[s + 2]];
        (p1 - p0)
            .cross(p2 - p0)
            .dot(self.polygons[gi].normal)
            .is_sign_negative()
    }

    /// Sign test only — the offset is left unnormalised (no sqrt).
    #[inline]
    pub fn is_facing_point(&self, gi: usize, point: Vec3) -> bool {
        let normal = self.polygons[gi].normal;
        let normal = if self.poly_flags[gi].contains(PolyFlags::FLIPPED) {
            -normal
        } else {
            normal
        };
        let (s, _) = self.poly_vertex_range[gi];
        let first_vertex = self.vertices[unsafe { *self.poly_verts.get_unchecked(s) }];
        let dot = normal.dot(point - first_vertex);
        dot.is_sign_positive() || dot.is_nan()
    }

    /// The texture facing the viewer (resolve keeps `poly_tex` flip-correct).
    /// `None` = untextured (draw nothing — sky surfaces are flagged, not
    /// textured).
    #[inline]
    pub fn visible_tex(&self, gi: usize) -> Option<u32> {
        let tex = self.poly_tex[gi];
        (tex != NO_INDEX).then_some(tex)
    }

    /// A wall quad's topological bottom/top edge z. Quads wind
    /// [bottom_start, bottom_end, top_end, top_start] and movers only relink
    /// to same-XY vertices, so the edge identity survives — including when a
    /// mover inverts the quad (bottom edge above the top edge).
    fn wall_edge_z(&self, gi: usize) -> (f32, f32) {
        let (s, e) = self.poly_vertex_range[gi];
        debug_assert_eq!(e - s, 4, "wall polygons are quads");
        (
            self.vertices[self.poly_verts[s]].z,
            self.vertices[self.poly_verts[s + 2]].z,
        )
    }

    /// Wall slot derived vanilla r_segs.c style: no back sidedef → middle;
    /// quad bottom edge at the back ceiling → upper; quad top edge at the back
    /// floor → lower; else middle (masked when two-sided). `None` for flats.
    /// Edges are topological so an inverted mover wall keeps its slot. Exact
    /// f32 equality holds because vertices and sector heights are written from
    /// the same FixedT value (interp quantizes through FixedT too).
    pub fn wall_slot(&self, gi: usize) -> Option<WallSlot> {
        if self.poly_flags[gi].contains(PolyFlags::IS_FLAT) {
            return None;
        }
        let Some(bsd) = &self.polygons[gi].back_sidedef else {
            return Some(WallSlot::Middle);
        };
        let (bottom_z, top_z) = self.wall_edge_z(gi);
        if bottom_z == bsd.sector.ceilingheight.to_f32() {
            Some(WallSlot::Upper)
        } else if top_z == bsd.sector.floorheight.to_f32() {
            Some(WallSlot::Lower)
        } else {
            Some(WallSlot::Middle)
        }
    }

    // ------------------------------------------------------------------
    // Surface cache resolution
    // ------------------------------------------------------------------

    /// Re-derive a wall polygon's surface cache: slot, textures, flag bits and
    /// UV from its sidedef/linedef/sector through the MapPtrs. Marks texture
    /// dirty only when a resolved value actually changed.
    pub(super) fn resolve_wall(&mut self, gi: usize) {
        debug_assert!(
            !self.poly_flags[gi].contains(PolyFlags::IS_FLAT),
            "resolve_wall called on a flat"
        );
        let persistent =
            self.poly_flags[gi] & (PolyFlags::MOVES | PolyFlags::SKY_FILLER | PolyFlags::IS_FLAT);
        let mut flags = persistent;
        let p = &self.polygons[gi];
        if p.back_sidedef.is_some() {
            flags |= PolyFlags::TWO_SIDED;
        }

        let flipped = self.winding_flipped(gi);
        if flipped {
            flags |= PolyFlags::FLIPPED;
        }

        let (facing_tex, away_tex);
        if persistent.contains(PolyFlags::SKY_FILLER) {
            // Sky geometry: never textured, drawn by the renderers' sky path.
            flags |= PolyFlags::SKY;
            facing_tex = NO_INDEX;
            away_tex = NO_INDEX;
        } else {
            let sidedef = p.sidedef.clone().expect("wall polygon without sidedef");
            let linedef = p.linedef.clone().expect("wall polygon without linedef");
            let back_sidedef = p.back_sidedef.clone();
            let seg_offset = p.seg_offset;
            let (bottom_z, top_z) = self.wall_edge_z(gi);
            let slot = self.wall_slot(gi).expect("resolve_wall on a flat");
            let slot_tex = |sd: &SideDef| match slot {
                WallSlot::Upper => sd.toptexture,
                WallSlot::Lower => sd.bottomtexture,
                WallSlot::Middle => sd.midtexture,
            };
            let front_tex = slot_tex(&sidedef).map_or(NO_INDEX, |t| t as u32);
            let back_tex = match (&back_sidedef, slot) {
                (Some(bsd), WallSlot::Upper | WallSlot::Lower) => {
                    slot_tex(bsd).map_or(NO_INDEX, |t| t as u32)
                }
                // A flipped wall with no back face shows its front.
                _ => front_tex,
            };
            // An inverted quad presents its back sidedef's texture to the
            // viewer; renderers always read poly_tex.
            (facing_tex, away_tex) = if flipped {
                (back_tex, front_tex)
            } else {
                (front_tex, back_tex)
            };
            if slot == WallSlot::Middle && back_sidedef.is_some() {
                flags |= PolyFlags::MASKED_MIDDLE;
            }
            if linedef.special == 260 {
                flags |= PolyFlags::TRANSLUCENT;
            }

            // UV bake — anchors verified against doom-og-src r_segs.c. The
            // texture-top z (`anchor`) includes the +texheight shifts: they
            // wrap to a no-op for tiled walls but matter for masked middles,
            // where v outside [0,1) is discarded instead of wrapped.
            let tex_for_height = if facing_tex != NO_INDEX {
                facing_tex
            } else {
                away_tex
            };
            let tex_h = self
                .wall_tex_height
                .get(tex_for_height as usize)
                .copied()
                .unwrap_or(0.0);
            let unpeg_top = linedef.flags.contains(LineDefFlags::UnpegTop);
            let unpeg_bottom = linedef.flags.contains(LineDefFlags::UnpegBottom);
            let anchor = match slot {
                WallSlot::Upper => {
                    if unpeg_top {
                        top_z
                    } else {
                        bottom_z + tex_h
                    }
                }
                // The only non-quad anchor: vanilla pegs an unpegged lower to
                // the front sector's ceiling.
                WallSlot::Lower => {
                    if unpeg_bottom {
                        sidedef.sector.ceilingheight.to_f32()
                    } else {
                        top_z
                    }
                }
                WallSlot::Middle => {
                    if unpeg_bottom {
                        bottom_z + tex_h
                    } else {
                        top_z
                    }
                }
            };

            let (s, e) = self.poly_vertex_range[gi];
            let v0 = self.vertices[self.poly_verts[s]];
            let v1 = self.vertices[self.poly_verts[s + 1]];
            let dir = Vec2::new(v1.x - v0.x, v1.y - v0.y).normalize();
            let x_off = f32::from(sidedef.textureoffset) + seg_offset;
            let y_off = f32::from(sidedef.rowoffset);
            for i in s..e {
                let world = self.vertices[self.poly_verts[i]];
                let u = (world.x - v0.x) * dir.x + (world.y - v0.y) * dir.y + x_off;
                let v = anchor - world.z + y_off;
                self.poly_vertex_uv[i] = [u, v];
            }
        }

        if self.poly_tex[gi] != facing_tex
            || self.poly_back_tex[gi] != away_tex
            || self.poly_flags[gi] != flags
        {
            self.poly_tex[gi] = facing_tex;
            self.poly_back_tex[gi] = away_tex;
            self.poly_flags[gi] = flags;
            self.texture_dirty = true;
            mark_texture_dirty_poly(
                &mut self.texture_dirty_polys,
                &mut self.texture_dirty_full,
                gi,
            );
        }
    }

    /// Re-derive a flat polygon's texture and sky bit from its sector's pic.
    pub(super) fn resolve_flat(&mut self, gi: usize) {
        debug_assert!(
            self.poly_flags[gi].contains(PolyFlags::IS_FLAT),
            "resolve_flat called on a wall"
        );
        let p = &self.polygons[gi];
        let pic = if p.normal.z > 0.0 {
            p.sector.floorpic
        } else {
            p.sector.ceilingpic
        };
        let mut flags = (self.poly_flags[gi] & PolyFlags::MOVES) | PolyFlags::IS_FLAT;
        if self.sky_num == Some(pic) {
            flags |= PolyFlags::SKY;
        }
        let tex = pic as u32;
        if self.poly_tex[gi] != tex || self.poly_flags[gi] != flags {
            self.poly_tex[gi] = tex;
            self.poly_back_tex[gi] = tex;
            self.poly_flags[gi] = flags;
            self.texture_dirty = true;
            mark_texture_dirty_poly(
                &mut self.texture_dirty_polys,
                &mut self.texture_dirty_full,
                gi,
            );
        }
    }

    /// Bake a flat's texel UV (pure XY rotation — never re-resolved).
    pub(super) fn resolve_flat_uv(&mut self, gi: usize) {
        let tex_cos = HORIZONTAL_TEX_DIRECTION.cos();
        let tex_sin = HORIZONTAL_TEX_DIRECTION.sin();
        let (s, e) = self.poly_vertex_range[gi];
        for i in s..e {
            let world = self.vertices[self.poly_verts[i]];
            let u = world.x * tex_cos - world.y * tex_sin;
            let v = world.x * tex_sin + world.y * tex_cos;
            self.poly_vertex_uv[i] = [u, v];
        }
    }

    // ------------------------------------------------------------------
    // Mover / event API (called from gameplay)
    // ------------------------------------------------------------------

    /// Move a sector's floor or ceiling polygons to `new_height` (vertex z),
    /// then re-resolve the sector's walls (UV re-anchors; a zh wall's slot and
    /// texture self-correct). Flat texture changes go through
    /// [`Self::update_flat_texture`], not here.
    pub fn move_surface(&mut self, sector_id: usize, movement_type: MovementType, new_height: f32) {
        if movement_type == MovementType::None {
            return;
        }
        self.set_surface_z(sector_id, movement_type, new_height);
        self.resolve_sector_walls(sector_id);
        self.geometry_dirty = true;
        self.update_affected_aabbs(sector_id);
    }

    /// Set vertex Z for all polygons of a surface type in a sector.
    fn set_surface_z(&mut self, sector_id: usize, movement: MovementType, height: f32) {
        let Self {
            sector_floor_polys,
            sector_ceiling_polys,
            poly_vertex_range,
            poly_verts,
            vertices,
            ..
        } = self;
        let table = match movement {
            MovementType::Floor => &sector_floor_polys[sector_id],
            MovementType::Ceiling => &sector_ceiling_polys[sector_id],
            MovementType::None => return,
        };
        for &gi in table {
            let (s, e) = poly_vertex_range[gi];
            for &vi in &poly_verts[s..e] {
                vertices[vi].z = height;
            }
        }
    }

    /// Re-resolve every wall touching a sector (the sector's own walls plus
    /// shared walls whose vertices track this sector's surfaces).
    fn resolve_sector_walls(&mut self, sector_id: usize) {
        if sector_id >= self.sector_wall_polys.len() {
            return;
        }
        for i in 0..self.sector_wall_polys[sector_id].len() {
            let gi = self.sector_wall_polys[sector_id][i];
            self.resolve_wall(gi);
        }
    }

    /// Apply interpolated sector heights to BSP3D vertices for smooth
    /// rendering. Called before each frame render with the sub-tic
    /// fraction. Saves true post-tic values into `interp_*` fields so they
    /// can be restored after rendering via `restore_sector_state()`.
    ///
    /// Heights quantize through [`FixedT`] before being applied so vertex z
    /// and `sector.*height` stay bit-equal — wall slot derivation depends on
    /// that equality.
    pub fn apply_interpolated_heights(&mut self, sectors: &mut [Sector], frac: f32) {
        for (sector_id, sector) in sectors.iter_mut().enumerate() {
            if sector_id >= self.sector_leaves.len() {
                break;
            }
            // Save true post-tic values before overwriting
            sector.interp_floorheight = sector.floorheight;
            sector.interp_ceilingheight = sector.ceilingheight;
            sector.interp_lightlevel = sector.lightlevel;

            let prev_floor = sector.prev_floorheight.to_f32();
            let curr_floor = sector.floorheight.to_f32();
            if prev_floor != curr_floor {
                let h = FixedT::from_f32(prev_floor + (curr_floor - prev_floor) * frac);
                sector.floorheight = h;
                self.set_surface_height(sector_id, MovementType::Floor, h.to_f32());
            }

            let prev_ceil = sector.prev_ceilingheight.to_f32();
            let curr_ceil = sector.ceilingheight.to_f32();
            if prev_ceil != curr_ceil {
                let h = FixedT::from_f32(prev_ceil + (curr_ceil - prev_ceil) * frac);
                sector.ceilingheight = h;
                self.set_surface_height(sector_id, MovementType::Ceiling, h.to_f32());
            }

            if sector.prev_lightlevel != sector.lightlevel {
                let l = sector.prev_lightlevel as f32
                    + (sector.lightlevel as f32 - sector.prev_lightlevel as f32) * frac;
                sector.lightlevel = l.round() as usize;
            }
        }
    }

    /// Restore true post-tic sector values and vertex Z after rendering.
    pub fn restore_sector_state(&mut self, sectors: &mut [Sector]) {
        for (sector_id, sector) in sectors.iter_mut().enumerate() {
            if sector_id >= self.sector_leaves.len() {
                break;
            }
            let floor_changed = sector.floorheight != sector.interp_floorheight;
            let ceil_changed = sector.ceilingheight != sector.interp_ceilingheight;

            sector.floorheight = sector.interp_floorheight;
            sector.ceilingheight = sector.interp_ceilingheight;
            sector.lightlevel = sector.interp_lightlevel;

            if floor_changed {
                self.set_surface_height(
                    sector_id,
                    MovementType::Floor,
                    sector.floorheight.to_f32(),
                );
            }
            if ceil_changed {
                self.set_surface_height(
                    sector_id,
                    MovementType::Ceiling,
                    sector.ceilingheight.to_f32(),
                );
            }
        }
    }

    /// Set vertex Z for all polygons of a surface type in a sector and
    /// re-resolve the sector's walls so textures stay anchored (tile) instead
    /// of stretching. No AABB update — interp travel stays inside the
    /// mover-expanded bounds.
    fn set_surface_height(&mut self, sector_id: usize, movement: MovementType, height: f32) {
        self.set_surface_z(sector_id, movement, height);
        self.resolve_sector_walls(sector_id);
        self.geometry_dirty = true;
    }

    /// Set the floor or ceiling flat texture for a sector's polygons from the
    /// sector's (already mutated) pic. Called from env when a sector flat pic
    /// changes.
    pub fn update_flat_texture(&mut self, sector_id: usize, movement: MovementType) {
        let table = match movement {
            MovementType::Floor => &self.sector_floor_polys[sector_id],
            MovementType::Ceiling => &self.sector_ceiling_polys[sector_id],
            MovementType::None => return,
        };
        for i in 0..table.len() {
            let gi = match movement {
                MovementType::Floor => self.sector_floor_polys[sector_id][i],
                MovementType::Ceiling => self.sector_ceiling_polys[sector_id][i],
                MovementType::None => unreachable!(),
            };
            self.resolve_flat(gi);
        }
    }

    /// Re-resolve all wall polygons of `linedef_id` from their (already
    /// mutated) sidedefs. Called by the switch system after a sidedef texture
    /// change so the 3D scene stays in sync.
    pub fn update_wall_texture(&mut self, linedef_id: usize) {
        if linedef_id >= self.linedef_wall_polys.len() {
            return;
        }
        for i in 0..self.linedef_wall_polys[linedef_id].len() {
            let gi = self.linedef_wall_polys[linedef_id][i];
            self.resolve_wall(gi);
        }
    }

    /// Set horizontal texture scroll (texels) for all wall polygons of a
    /// scrolling linedef (special 48). `delta` is the live offset minus the
    /// build-baked one, added to U at sample time so it does not double-count.
    pub fn set_wall_scroll(&mut self, linedef_id: usize, delta: f32) {
        if linedef_id >= self.linedef_wall_polys.len() {
            return;
        }
        let Self {
            linedef_wall_polys,
            poly_scroll,
            texture_dirty,
            texture_dirty_polys,
            texture_dirty_full,
            ..
        } = self;
        for &gi in &linedef_wall_polys[linedef_id] {
            poly_scroll[gi] = delta;
            mark_texture_dirty_poly(texture_dirty_polys, texture_dirty_full, gi);
        }
        *texture_dirty = true;
    }

    // ------------------------------------------------------------------
    // Dirty tracking
    // ------------------------------------------------------------------

    /// True if vertex positions / UV changed since the last
    /// [`Self::clear_geometry_dirty`]. A renderer uploads dynamic buffers only
    /// when set.
    pub fn geometry_dirty(&self) -> bool {
        self.geometry_dirty
    }

    /// Clear the dirty flag after a renderer has uploaded the dynamic buffers.
    pub fn clear_geometry_dirty(&mut self) {
        self.geometry_dirty = false;
    }

    /// True if poly_tex/flags or poly_scroll changed since
    /// [`Self::clear_texture_dirty`] (switch swap, flat change, or texture
    /// scroll). Renderers re-upload texture buffers only when set.
    pub fn texture_dirty(&self) -> bool {
        self.texture_dirty
    }

    /// Dirty polygons for scoped re-fan; `None` = re-fan the whole map.
    /// Valid only while [`Self::texture_dirty`] is set.
    pub fn texture_dirty_polys(&self) -> Option<&[usize]> {
        if self.texture_dirty_full {
            None
        } else {
            Some(&self.texture_dirty_polys)
        }
    }

    pub fn clear_texture_dirty(&mut self) {
        self.texture_dirty = false;
        self.texture_dirty_full = false;
        self.texture_dirty_polys.clear();
    }

    // ------------------------------------------------------------------
    // Per-corner fanning (triangulating renderers)
    // ------------------------------------------------------------------

    /// Fan a per-polygon attribute into per-corner entries (static, order matches
    /// [`Self::triangles`]). `attr(poly_idx)` builds the value. `out` is cleared.
    pub fn fan_corner_attr<T: Copy>(&self, out: &mut Vec<T>, attr: impl Fn(usize) -> T) {
        out.clear();
        for poly_idx in 0..self.poly_vertex_range.len() {
            let (start, end) = self.poly_vertex_range[poly_idx];
            let n = end - start;
            if n < 3 {
                continue;
            }
            let v = attr(poly_idx);
            for _ in 0..(n - 2) * 3 {
                out.push(v);
            }
        }
    }

    /// Fan per-polygon-vertex UV ([`Self::poly_vertex_uv`]) into per-corner UV
    /// (fan `(v0, vi, vi+1)`, order matches [`Self::triangles`]). `out` is
    /// cleared. Triangulating renderers (wgpu3d) call this at mesh upload and on
    /// geometry/texture re-upload instead of storing a second UV array.
    pub fn fan_corner_uv(&self, out: &mut Vec<[f32; 2]>) {
        out.clear();
        for &(start, end) in &self.poly_vertex_range {
            let n = end - start;
            if n < 3 {
                continue;
            }
            for i in 1..n - 1 {
                out.push(self.poly_vertex_uv[start]);
                out.push(self.poly_vertex_uv[start + i]);
                out.push(self.poly_vertex_uv[start + i + 1]);
            }
        }
    }

    // ------------------------------------------------------------------
    // AABBs
    // ------------------------------------------------------------------

    /// Compute the AABB for a single subsector leaf from its polygon vertices
    /// (own + shared).
    pub(super) fn compute_leaf_aabb(&self, subsector_id: usize) -> AABB {
        let mut aabb = AABB::new();
        for gi in self.leaf_poly_indices(subsector_id) {
            for &vi in self.poly_vert_indices(gi) {
                aabb.expand_to_include_point(self.vertices[vi]);
            }
        }
        aabb
    }

    /// Recompute AABBs for all subsectors in a sector.
    fn update_affected_aabbs(&mut self, sector_id: usize) {
        for i in 0..self.sector_leaves[sector_id].len() {
            let subsector_id = self.sector_leaves[sector_id][i];
            let aabb = self.compute_leaf_aabb(subsector_id);
            self.leaves[subsector_id].aabb = aabb;
        }
    }

    pub(crate) fn update_node_aabbs_recursive(&mut self, node_id: u32) {
        if is_leaf(node_id) {
            return;
        }

        let node_idx = node_id as usize;
        if node_idx >= self.nodes.len() {
            return;
        }

        let children = self.nodes[node_idx].children;
        let mut combined_aabb = AABB::new();
        let mut has_valid_aabb = false;

        for &child_id in &children {
            if is_leaf(child_id) {
                let leaf = &self.leaves[leaf_index(child_id)];
                combined_aabb.expand_to_include_aabb(&leaf.aabb);
                has_valid_aabb = true;
            } else {
                self.update_node_aabbs_recursive(child_id);
                if let Some(child_aabb) = self.node_aabbs.get(child_id as usize) {
                    combined_aabb.expand_to_include_aabb(child_aabb);
                    has_valid_aabb = true;
                }
            }
        }
        if has_valid_aabb {
            self.node_aabbs[node_idx] = combined_aabb;
        }
    }

    pub fn get_node_aabb(&self, node_id: u32) -> Option<&AABB> {
        if is_leaf(node_id) {
            self.leaves.get(leaf_index(node_id)).map(|leaf| &leaf.aabb)
        } else {
            self.node_aabbs.get(node_id as usize)
        }
    }
}
