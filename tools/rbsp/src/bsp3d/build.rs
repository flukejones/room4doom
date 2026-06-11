//! 3D-BSP construction. [`Bsp3dBuilder`] owns all construction scratch
//! (vertex dedup map, zh wall records, per-leaf polygon buckets) and emits a
//! flat [`Bsp3dLump`] via its condense step. Nothing here survives to
//! runtime — the engine's runtime structure is parsed from the lump.

use crate::bsp3d::input::{Bsp3dInput, InputSeg, InputSideDef, NO_REF};
use crate::bsp3d::lump::{Bsp3dLump, LeafRecord, NO_INDEX, PolyFlags, PolyRecord, tree_from_nodes};
use crate::types::Node;
use glam::{Vec2, Vec3};
use std::collections::HashMap;

/// Vertex deduplication grid cell size. rbsp already deduplicates at 1e-5,
/// so this only needs to catch floating-point drift from f64→f32 conversion.
pub const QUANT_PRECISION: f32 = 0.001;
pub const HEIGHT_EPSILON: f32 = 0.1;
/// Minimum cross-product magnitude for a non-degenerate polygon.
const MIN_TRI_CROSS: f32 = 1e-4;

/// Construction-only record tracking zero-height wall vertex roles.
/// Needed because zh walls have bottom and top at the same (x,y,z) — with
/// position-only dedup they'd share one index, producing degenerate triangles.
/// Fresh vertices are created instead, and this record tells the post-pass
/// which vertices are bottom (front sector) vs top (back sector).
#[derive(Clone)]
pub(crate) struct ZhWallRecord {
    /// Global index into [`Bsp3dBuilder::polygons`].
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
    fn flat(z: f32) -> Self {
        Self {
            start: z,
            end: z,
        }
    }
    /// Midpoint height, for comparisons that need a single value.
    fn mean(self) -> f32 {
        (self.start + self.end) * 0.5
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

/// Construction-time polygon kind. Everything texture/slot/peg-related is
/// derived by the engine at resolve time from the linedef/sidedef indices.
#[derive(Clone)]
pub(crate) enum BuildKind {
    Wall {
        linedef: u32,
        sidedef: u32,
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
    /// Own polygons (global indices into [`Bsp3dBuilder::polygons`]).
    pub(crate) polys: Vec<usize>,
    /// Subset of `polys`: floor flats.
    pub(crate) floor_polygons: Vec<usize>,
    /// Subset of `polys`: ceiling flats.
    pub(crate) ceiling_polygons: Vec<usize>,
    /// Two-sided walls owned by an adjacent leaf, visible from this one when a
    /// mover inverts them.
    pub(crate) shared: Vec<usize>,
}

pub struct Bsp3dBuilder {
    pub(crate) polygons: Vec<BuildPolygon>,
    pub(crate) leaves: Vec<BuildLeaf>,
    pub(crate) vertices: Vec<Vec3>,
    pub(crate) sector_subsectors: Vec<Vec<usize>>,
    pub(crate) zh_wall_records: Vec<ZhWallRecord>,
    vertex_map: HashMap<QuantizedVec3, usize>,
}

impl Bsp3dBuilder {
    /// Build the flat 3D geometry lump.
    ///
    /// - Creates wall quads, floor/ceiling N-gons (from the pre-carved convex
    ///   subsector polygons), and sky filler geometry
    /// - Runs the mover vertex pass for zero-height boundary sectors
    /// - Condenses into a leaf-contiguous [`Bsp3dLump`]
    pub fn build(input: &Bsp3dInput, nodes: &[Node]) -> Bsp3dLump {
        let mut builder = Self {
            polygons: Vec::new(),
            leaves: vec![BuildLeaf::default(); input.subsectors.len()],
            vertices: Vec::new(),
            sector_subsectors: vec![Vec::new(); input.sectors.len()],
            zh_wall_records: Vec::new(),
            vertex_map: HashMap::with_capacity(input.segs.len() * 2),
        };

        for (ss_id, ss) in input.subsectors.iter().enumerate() {
            let sector_id = ss.sector as usize;
            if sector_id < input.sectors.len() {
                builder.sector_subsectors[sector_id].push(ss_id);
            }
        }

        // Walls from segs.
        for (ss_id, ss) in input.subsectors.iter().enumerate() {
            builder.leaves[ss_id].sector_id = ss.sector as usize;
            let start = ss.start_seg as usize;
            let end = start + ss.seg_count as usize;
            for seg in &input.segs[start..end] {
                if seg.backsector != NO_REF {
                    builder.create_two_sided_walls(input, seg, ss_id);
                } else {
                    builder.create_one_sided_wall(input, seg, ss_id);
                }
            }
        }

        // Floor/ceiling N-gons from carved polygons (sky flats skipped — the
        // sky is drawn by the renderers' sky pass plus the filler walls).
        for (ssid, ss) in input.subsectors.iter().enumerate() {
            builder.create_floor_ceiling_polygons(
                input,
                ssid,
                ss.sector as usize,
                &input.carved[ssid],
            );
        }

        // Mover vertex pass — separate shared vertices at zero-height
        // boundaries, connect wall vertices, set moves flags.
        builder.mover_vertex_pass(input);

        // Sky filler — extend perimeter walls of sky sectors up to max
        // adjacent sky ceiling / down to min adjacent sky floor.
        if input.sky_fillers {
            let (sky_max_ceil, sky_min_floor) = compute_sky_bounds(input);
            builder.sky_filler_pass(input, &sky_max_ceil, &sky_min_floor);
        }

        let mut lump = builder.condense();
        lump.tree = tree_from_nodes(nodes);
        lump
    }

    /// Add or reuse a vertex by position. Simple position-only dedup.
    fn vertex_add(&mut self, vertex: Vec3) -> usize {
        let key = QuantizedVec3::from_vec3(vertex, QUANT_PRECISION);
        if let Some(&idx) = self.vertex_map.get(&key) {
            idx
        } else {
            let idx = self.vertices.len();
            self.vertices.push(vertex);
            self.vertex_map.insert(key, idx);
            idx
        }
    }

    /// A sector's floor edge z at this seg's two endpoints.
    fn floor_edge(input: &Bsp3dInput, seg: &InputSeg, sector_id: usize) -> WallEdge {
        let s = &input.sectors[sector_id];
        let (a, b) = (input.verts[seg.v1 as usize], input.verts[seg.v2 as usize]);
        WallEdge {
            start: s.floor_z(a.x, a.y),
            end: s.floor_z(b.x, b.y),
        }
    }

    /// A sector's ceiling edge z at this seg's two endpoints.
    fn ceil_edge(input: &Bsp3dInput, seg: &InputSeg, sector_id: usize) -> WallEdge {
        let s = &input.sectors[sector_id];
        let (a, b) = (input.verts[seg.v1 as usize], input.verts[seg.v2 as usize]);
        WallEdge {
            start: s.ceil_z(a.x, a.y),
            end: s.ceil_z(b.x, b.y),
        }
    }

    /// Create upper, lower, and middle wall quads for a two-sided seg.
    fn create_two_sided_walls(&mut self, input: &Bsp3dInput, seg: &InputSeg, ss_id: usize) {
        let front_id = seg.frontsector as usize;
        let back_id = seg.backsector as usize;
        let front_sector = &input.sectors[front_id];
        let back_sector = &input.sectors[back_id];

        // Sky hack: suppress upper wall between two sky-ceiling sectors and
        // lower wall between two sky-floor sectors (matches original Doom
        // r_segs.c behaviour).
        let both_sky_ceil = front_sector.sky_ceil && back_sector.sky_ceil;
        let both_sky_floor = front_sector.sky_floor && back_sector.sky_floor;

        // Build from the seg whose side shows the wall (its sector is the
        // taller/lower one). At equal heights (a mover at rest) both segs
        // qualify, so the linedef-front seg builds it.
        let is_linedef_front = seg.side == 0;
        let other = input.linedefs[seg.linedef as usize].sides[(seg.side ^ 1) as usize];
        let other_sidedef = (other != NO_REF).then(|| &input.sidedefs[other as usize]);

        let front_ceil = Self::ceil_edge(input, seg, front_id);
        let back_ceil = Self::ceil_edge(input, seg, back_id);
        let build_upper = if back_ceil.mean() == front_ceil.mean() {
            is_linedef_front
        } else {
            back_ceil.mean() < front_ceil.mean()
        };
        if build_upper && !both_sky_ceil {
            self.add_two_sided_wall(
                input,
                seg,
                WallType::Upper,
                back_ceil,
                front_ceil,
                front_id,
                back_id,
                other_sidedef,
                ss_id,
            );
        }

        let front_floor = Self::floor_edge(input, seg, front_id);
        let back_floor = Self::floor_edge(input, seg, back_id);
        let build_lower = if back_floor.mean() == front_floor.mean() {
            is_linedef_front
        } else {
            back_floor.mean() > front_floor.mean()
        };
        if build_lower && !both_sky_floor {
            self.add_two_sided_wall(
                input,
                seg,
                WallType::Lower,
                front_floor,
                back_floor,
                front_id,
                back_id,
                other_sidedef,
                ss_id,
            );
        }

        if seg_sidedef(input, seg).has_mid {
            let ff = Self::floor_edge(input, seg, front_id);
            let bf = Self::floor_edge(input, seg, back_id);
            let fc = Self::ceil_edge(input, seg, front_id);
            let bc = Self::ceil_edge(input, seg, back_id);
            let bottom = WallEdge {
                start: ff.start.max(bf.start),
                end: ff.end.max(bf.end),
            };
            let top = WallEdge {
                start: fc.start.min(bc.start),
                end: fc.end.min(bc.end),
            };
            self.add_wall_quad(
                input,
                seg,
                bottom,
                top,
                WallType::Middle,
                front_id,
                false,
                ss_id,
                None,
            );
        }
    }

    /// Build a two-sided Upper/Lower wall quad spanning `bottom_h`..`top_h`.
    /// Skips construction when neither side has the relevant texture. The quad
    /// is shared into the subsectors across the seg so a mover that inverts
    /// the wall can render it from the other side. Zero-height quads (a mover
    /// at rest) get fresh vertices and a `ZhWallRecord` so the mover pass can
    /// connect each edge to its own sector's surface.
    #[allow(clippy::too_many_arguments)]
    fn add_two_sided_wall(
        &mut self,
        input: &Bsp3dInput,
        seg: &InputSeg,
        wall_type: WallType,
        bottom: WallEdge,
        top: WallEdge,
        front_sector_id: usize,
        back_sector_id: usize,
        other_sidedef: Option<&InputSideDef>,
        ss_id: usize,
    ) {
        let tex = |sd: &InputSideDef| match wall_type {
            WallType::Upper => sd.has_top,
            _ => sd.has_bottom,
        };
        let front_tex = tex(seg_sidedef(input, seg));
        let back_tex = other_sidedef.is_some_and(tex);
        if !front_tex && !back_tex {
            return;
        }
        self.add_wall_quad(
            input,
            seg,
            bottom,
            top,
            wall_type,
            front_sector_id,
            false,
            ss_id,
            Some(back_sector_id),
        );
        let gi = self.polygons.len() - 1;
        for &back in &seg.back_subsectors {
            self.leaves[back as usize].shared.push(gi);
        }
    }

    /// Create a middle wall quad for a one-sided seg. Zero-height sectors
    /// (doors) get fresh vertices and a `ZhWallRecord` for the mover pass.
    fn create_one_sided_wall(&mut self, input: &Bsp3dInput, seg: &InputSeg, ss_id: usize) {
        if seg_sidedef(input, seg).has_mid {
            let front_id = seg.frontsector as usize;
            let front_sector = &input.sectors[front_id];
            let is_zh = (front_sector.ceil_h - front_sector.floor_h).abs() <= HEIGHT_EPSILON;
            // For zh sectors (doors): pass self as back_sector so add_wall_quad
            // creates fresh vertices and a ZhWallRecord. The mover pass
            // connects bottom → floor vertex, top → ceiling vertex.
            let back_sector_id = if is_zh { Some(front_id) } else { None };
            self.add_wall_quad(
                input,
                seg,
                Self::floor_edge(input, seg, front_id),
                Self::ceil_edge(input, seg, front_id),
                WallType::Middle,
                front_id,
                false,
                ss_id,
                back_sector_id,
            );
        }
    }

    /// Create a wall quad from a seg and push it to the subsector leaf.
    /// Winding contract: [bottom_start, bottom_end, top_end, top_start] along
    /// the seg direction, so the geometric normal faces the seg's sidedef
    /// side. For zero-height walls with a back sector, creates fresh
    /// (non-dedup'd) vertices so bottom and top have distinct indices, and
    /// records a `ZhWallRecord` for the post-pass.
    #[allow(clippy::too_many_arguments)]
    fn add_wall_quad(
        &mut self,
        input: &Bsp3dInput,
        seg: &InputSeg,
        bottom: WallEdge,
        top: WallEdge,
        wall_type: WallType,
        sector_id: usize,
        sky_filler: bool,
        ss_id: usize,
        back_sector_id: Option<usize>,
    ) {
        let start_pos = input.verts[seg.v1 as usize];
        let end_pos = input.verts[seg.v2 as usize];
        let is_zero_height = (top.start - bottom.start).abs() <= HEIGHT_EPSILON
            && (top.end - bottom.end).abs() <= HEIGHT_EPSILON;

        let (bottom_start, bottom_end, top_start, top_end) =
            if is_zero_height && back_sector_id.is_some() {
                // Fresh vertices for zh walls — bypass dedup so bottom and top
                // get distinct indices even though they're at the same position.
                let bs = self.vertices.len();
                self.vertices
                    .push(Vec3::new(start_pos.x, start_pos.y, bottom.start));
                let be = self.vertices.len();
                self.vertices
                    .push(Vec3::new(end_pos.x, end_pos.y, bottom.end));
                let ts = self.vertices.len();
                self.vertices
                    .push(Vec3::new(start_pos.x, start_pos.y, top.start));
                let te = self.vertices.len();
                self.vertices.push(Vec3::new(end_pos.x, end_pos.y, top.end));
                (bs, be, ts, te)
            } else {
                let bs = self.vertex_add(Vec3::new(start_pos.x, start_pos.y, bottom.start));
                let be = self.vertex_add(Vec3::new(end_pos.x, end_pos.y, bottom.end));
                let ts = self.vertex_add(Vec3::new(start_pos.x, start_pos.y, top.start));
                let te = self.vertex_add(Vec3::new(end_pos.x, end_pos.y, top.end));
                (bs, be, ts, te)
            };

        let quad = BuildPolygon {
            sector_id,
            vertices: vec![bottom_start, bottom_end, top_end, top_start],
            kind: BuildKind::Wall {
                linedef: seg.linedef,
                sidedef: input.linedefs[seg.linedef as usize].sides[seg.side as usize],
                wall_type,
                sky_filler,
                seg_offset: seg.offset,
            },
            moves: false,
        };
        let gi = self.polygons.len();
        self.polygons.push(quad);
        self.leaves[ss_id].polys.push(gi);

        if is_zero_height && let Some(back_id) = back_sector_id {
            self.zh_wall_records.push(ZhWallRecord {
                poly_index: gi,
                bottom: [bottom_start, bottom_end],
                top: [top_start, top_end],
                wall_type,
                front_sector: sector_id,
                back_sector: back_id,
            });
        }
    }

    /// Create the floor and ceiling N-gon for one subsector from its carved
    /// polygon. Sky surfaces produce no polygon. Winding contract: floor CCW
    /// viewed from above (+Z normal), ceiling CW (−Z normal).
    ///
    /// Input polygon winding determines whether to reverse: rbsp emits CCW,
    /// older carve paths CW.
    fn create_floor_ceiling_polygons(
        &mut self,
        input: &Bsp3dInput,
        ss_id: usize,
        sector_id: usize,
        polygon: &[Vec2],
    ) {
        if polygon.len() < 3 {
            return;
        }

        let sector = &input.sectors[sector_id];
        let skip_ceil = sector.sky_ceil;
        let skip_floor = sector.sky_floor;

        // Degenerate check via shoelace area.
        let shoelace: f32 = polygon
            .windows(2)
            .map(|w| w[0].x * w[1].y - w[1].x * w[0].y)
            .sum::<f32>()
            + polygon.last().unwrap().x * polygon[0].y
            - polygon[0].x * polygon.last().unwrap().y;
        if shoelace.abs() < MIN_TRI_CROSS {
            return;
        }
        let input_is_ccw = shoelace > 0.0;

        if !skip_floor {
            let ordered: Vec<&Vec2> = if input_is_ccw {
                polygon.iter().collect()
            } else {
                polygon.iter().rev().collect()
            };
            let fv: Vec<usize> = ordered
                .iter()
                .map(|v| self.vertex_add(Vec3::new(v.x, v.y, sector.floor_z(v.x, v.y))))
                .collect();
            if fv.len() >= 3 && vertex_shoelace(&fv, &self.vertices) > 0.0 {
                let fi = self.polygons.len();
                self.polygons.push(BuildPolygon {
                    sector_id,
                    vertices: fv,
                    kind: BuildKind::Flat,
                    moves: false,
                });
                self.leaves[ss_id].polys.push(fi);
                self.leaves[ss_id].floor_polygons.push(fi);
            }
        }

        if !skip_ceil {
            let ordered: Vec<&Vec2> = if input_is_ccw {
                polygon.iter().rev().collect()
            } else {
                polygon.iter().collect()
            };
            let cv: Vec<usize> = ordered
                .iter()
                .map(|v| self.vertex_add(Vec3::new(v.x, v.y, sector.ceil_z(v.x, v.y))))
                .collect();
            if cv.len() < 3 || vertex_shoelace(&cv, &self.vertices) >= 0.0 {
                return;
            }
            let ci = self.polygons.len();
            self.polygons.push(BuildPolygon {
                sector_id,
                vertices: cv,
                kind: BuildKind::Flat,
                moves: false,
            });
            self.leaves[ss_id].polys.push(ci);
            self.leaves[ss_id].ceiling_polygons.push(ci);
        }
    }

    /// Create sky filler walls on perimeter walls of sky sectors.
    /// Upper filler extends above sky-ceiling perimeter walls to max_ceil.
    /// Lower filler extends below sky-floor perimeter walls to min_floor.
    fn sky_filler_pass(&mut self, input: &Bsp3dInput, sky_max_ceil: &[f32], sky_min_floor: &[f32]) {
        for sector_id in 0..input.sectors.len() {
            let sector = &input.sectors[sector_id];
            if !sector.sky_ceil && !sector.sky_floor {
                continue;
            }

            let sky_ceil = sector.ceil_h;
            let sky_floor = sector.floor_h;
            let max_h = sky_max_ceil[sector_id];
            let min_h = sky_min_floor[sector_id];
            let needs_ceil_filler = sector.sky_ceil && max_h > sky_ceil;
            let needs_floor_filler = sector.sky_floor && min_h < sky_floor;

            if !needs_ceil_filler && !needs_floor_filler {
                continue;
            }

            let ss_ids: Vec<usize> = self.sector_subsectors[sector_id].clone();

            for &ss_id in &ss_ids {
                let ss = &input.subsectors[ss_id];
                let start = ss.start_seg as usize;
                let end = start + ss.seg_count as usize;
                for seg in &input.segs[start..end] {
                    // Only perimeter segs: skip interior (same-sector) and
                    // sky-to-sky boundaries.
                    let back =
                        (seg.backsector != NO_REF).then(|| &input.sectors[seg.backsector as usize]);
                    let is_perimeter_ceil = match back {
                        Some(b) => seg.backsector != seg.frontsector && !b.sky_ceil,
                        None => true,
                    };
                    let is_perimeter_floor = match back {
                        Some(b) => seg.backsector != seg.frontsector && !b.sky_floor,
                        None => true,
                    };

                    if needs_ceil_filler && is_perimeter_ceil && sky_ceil < max_h {
                        self.add_wall_quad(
                            input,
                            seg,
                            WallEdge::flat(sky_ceil),
                            WallEdge::flat(max_h),
                            WallType::Upper,
                            sector_id,
                            true,
                            ss_id,
                            None,
                        );
                    }
                    if needs_floor_filler && is_perimeter_floor && min_h < sky_floor {
                        self.add_wall_quad(
                            input,
                            seg,
                            WallEdge::flat(min_h),
                            WallEdge::flat(sky_floor),
                            WallType::Lower,
                            sector_id,
                            true,
                            ss_id,
                            None,
                        );
                    }
                }
            }
        }
    }

    /// Flatten into a leaf-contiguous [`Bsp3dLump`]: polygons reordered so
    /// each leaf's own polys form one range, vertex index lists flattened into
    /// `poly_verts`, shared-wall lists remapped into one flat array.
    fn condense(self) -> Bsp3dLump {
        let mut remap = vec![NO_INDEX; self.polygons.len()];
        let vert_total: usize = self.polygons.iter().map(|p| p.vertices.len()).sum();
        let mut polys = Vec::with_capacity(self.polygons.len());
        let mut poly_verts = Vec::with_capacity(vert_total);
        let mut leaves = Vec::with_capacity(self.leaves.len());

        for (ss_id, leaf) in self.leaves.iter().enumerate() {
            let poly_start = polys.len() as u32;
            for &gi in &leaf.polys {
                remap[gi] = polys.len() as u32;
                let p = &self.polygons[gi];
                let vert_start = poly_verts.len() as u32;
                poly_verts.extend(p.vertices.iter().map(|&v| v as u32));

                let mut flags = PolyFlags::empty();
                if p.moves {
                    flags |= PolyFlags::MOVES;
                }
                let (linedef, sidedef, seg_offset) = match p.kind {
                    BuildKind::Wall {
                        linedef,
                        sidedef,
                        sky_filler,
                        seg_offset,
                        ..
                    } => {
                        if sky_filler {
                            flags |= PolyFlags::SKY_FILLER;
                        }
                        (linedef, sidedef, seg_offset)
                    }
                    BuildKind::Flat => (NO_INDEX, NO_INDEX, 0.0),
                };
                polys.push(PolyRecord {
                    vert_start,
                    vert_count: p.vertices.len() as u16,
                    flags,
                    linedef,
                    sidedef,
                    seg_offset,
                });
            }
            leaves.push(LeafRecord {
                subsector: ss_id as u32,
                poly_start,
                poly_count: (polys.len() as u32 - poly_start) as u16,
                shared_start: 0,
                shared_count: 0,
            });
        }
        debug_assert!(
            remap.iter().all(|&r| r != NO_INDEX),
            "every polygon must belong to exactly one leaf"
        );

        let mut shared_walls = Vec::new();
        for (li, leaf) in self.leaves.iter().enumerate() {
            leaves[li].shared_start = shared_walls.len() as u32;
            leaves[li].shared_count = leaf.shared.len() as u16;
            shared_walls.extend(leaf.shared.iter().map(|&gi| remap[gi]));
        }

        Bsp3dLump {
            tree: Vec::new(),
            vertices: self.vertices,
            poly_verts,
            polys,
            leaves,
            shared_walls,
        }
    }
}

/// The seg's own sidedef.
fn seg_sidedef<'a>(input: &'a Bsp3dInput, seg: &InputSeg) -> &'a InputSideDef {
    let sd = input.linedefs[seg.linedef as usize].sides[seg.side as usize];
    &input.sidedefs[sd as usize]
}

/// Compute global sky bounds for the level.
/// Returns (sky_max_ceil, sky_min_floor) indexed by sector id.
/// All sky-ceiling sectors get the global max sky ceiling height.
/// All sky-floor sectors get the global min sky floor height.
fn compute_sky_bounds(input: &Bsp3dInput) -> (Vec<f32>, Vec<f32>) {
    let global_max_ceil = input
        .sectors
        .iter()
        .map(|s| s.ceil_h)
        .fold(f32::NEG_INFINITY, f32::max);
    let global_min_floor = input
        .sectors
        .iter()
        .map(|s| s.floor_h)
        .fold(f32::INFINITY, f32::min);

    let max_ceil: Vec<f32> = input
        .sectors
        .iter()
        .map(|s| {
            if s.sky_ceil {
                global_max_ceil
            } else {
                s.ceil_h
            }
        })
        .collect();
    let min_floor: Vec<f32> = input
        .sectors
        .iter()
        .map(|s| {
            if s.sky_floor {
                global_min_floor
            } else {
                s.floor_h
            }
        })
        .collect();

    (max_ceil, min_floor)
}
