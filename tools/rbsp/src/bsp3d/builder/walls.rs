//! Wall-quad construction: upper/lower/middle quads for two-sided and one-sided
//! segs, the shared-wall registration that lets a mover render an inverted wall
//! from the far side, and the zero-height handling that gives door/lift walls
//! distinct bottom/top vertices for the mover pass.

use glam::Vec3;

use crate::bsp3d::input::{Bsp3dInput, InputSeg, InputSideDef, NO_REF};
use crate::types::Side;

use super::Bsp3dBuilder;
use super::types::{
    BuildKind, BuildPolygon, HEIGHT_EPSILON, QUANT_PRECISION, QuantizedVec3, WallEdge, WallType,
    ZhWallRecord,
};

impl Bsp3dBuilder {
    /// Add or reuse a vertex by position. Simple position-only dedup.
    pub(super) fn vertex_add(&mut self, vertex: Vec3) -> usize {
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
    pub(super) fn floor_edge(input: &Bsp3dInput, seg: &InputSeg, sector_id: usize) -> WallEdge {
        let s = &input.sectors[sector_id];
        let (a, b) = (input.verts[seg.v1 as usize], input.verts[seg.v2 as usize]);
        WallEdge {
            start: s.floor_z(a.x, a.y),
            end: s.floor_z(b.x, b.y),
        }
    }

    /// A sector's ceiling edge z at this seg's two endpoints.
    pub(super) fn ceil_edge(input: &Bsp3dInput, seg: &InputSeg, sector_id: usize) -> WallEdge {
        let s = &input.sectors[sector_id];
        let (a, b) = (input.verts[seg.v1 as usize], input.verts[seg.v2 as usize]);
        WallEdge {
            start: s.ceil_z(a.x, a.y),
            end: s.ceil_z(b.x, b.y),
        }
    }

    /// Create upper, lower, and middle wall quads for a two-sided seg.
    pub(super) fn create_two_sided_walls(
        &mut self,
        input: &Bsp3dInput,
        seg: &InputSeg,
        ss_id: usize,
    ) {
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
    pub(super) fn create_one_sided_wall(
        &mut self,
        input: &Bsp3dInput,
        seg: &InputSeg,
        ss_id: usize,
    ) {
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
    pub(super) fn add_wall_quad(
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
                linedef_side: if seg.side == 0 {
                    Side::Front
                } else {
                    Side::Back
                },
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
}

/// The seg's own sidedef.
pub(super) fn seg_sidedef<'a>(input: &'a Bsp3dInput, seg: &InputSeg) -> &'a InputSideDef {
    let sd = input.linedefs[seg.linedef as usize].sides[seg.side as usize];
    &input.sidedefs[sd as usize]
}
