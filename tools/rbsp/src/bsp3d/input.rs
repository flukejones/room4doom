//! Self-contained input model for the 3D builder, prepared from WAD-level
//! records (through the accessor traits) and a finished [`BspOutput`].
//!
//! Segs are compacted contiguous-per-subsector in subsector order — the same
//! stream the engine's loader builds — so the emitted lump is identical
//! whichever side runs the builder.

use crate::special_encode;
use crate::types::{BspOutput, LineDefAccess, SectorAccess, Side, SideDefAccess, SlopePlane};
use glam::Vec2;
use std::collections::BTreeMap;

/// Sentinel for "no sidedef" / "no sector" in the input model.
pub const NO_REF: u32 = u32::MAX;
/// ML_TWOSIDED — gates back-sector visibility exactly as the engine does.
const TWO_SIDED_FLAG: u32 = 0x4;

pub struct InputSector {
    pub floor_h: f32,
    pub ceil_h: f32,
    pub tag: i16,
    pub sky_floor: bool,
    pub sky_ceil: bool,
    /// Sloped floor plane; `None` = flat at `floor_h`.
    pub floor_plane: Option<SlopePlane>,
    /// Sloped ceiling plane; `None` = flat at `ceil_h`.
    pub ceil_plane: Option<SlopePlane>,
    /// Linedefs whose front or back sidedef faces this sector.
    pub lines: Vec<u32>,
}

impl InputSector {
    /// Floor z at `(x, y)` — the plane when sloped, else the flat height.
    pub fn floor_z(&self, x: f32, y: f32) -> f32 {
        self.floor_plane.map_or(self.floor_h, |p| p.z_at(x, y))
    }
    /// Ceiling z at `(x, y)`.
    pub fn ceil_z(&self, x: f32, y: f32) -> f32 {
        self.ceil_plane.map_or(self.ceil_h, |p| p.z_at(x, y))
    }
    pub fn is_sloped(&self) -> bool {
        self.floor_plane.is_some() || self.ceil_plane.is_some()
    }
}

pub struct InputSideDef {
    pub sector: u32,
    /// Texture presence is by name (`"-"` = none) — see [`SideDefAccess`].
    pub has_top: bool,
    pub has_bottom: bool,
    pub has_mid: bool,
}

pub struct InputLineDef {
    pub v1: Vec2,
    pub v2: Vec2,
    /// Generalized mover special (vanilla numbers normalized at prep).
    pub special: u32,
    pub tag: i16,
    /// [front, back]; [`NO_REF`] = none.
    pub sides: [u32; 2],
    pub two_sided: bool,
}

pub struct InputSeg {
    pub v1: u32,
    pub v2: u32,
    pub offset: f32,
    pub linedef: u32,
    /// 0 = front, 1 = back.
    pub side: u32,
    pub frontsector: u32,
    /// [`NO_REF`] = one-sided (or the linedef is not flagged two-sided).
    pub backsector: u32,
    /// Subsectors across this seg: same linedef, other side, overlapping span.
    pub back_subsectors: Vec<u32>,
}

pub struct InputSubSector {
    pub start_seg: u32,
    pub seg_count: u32,
    pub sector: u32,
}

pub struct Bsp3dInput {
    pub verts: Vec<Vec2>,
    pub segs: Vec<InputSeg>,
    pub subsectors: Vec<InputSubSector>,
    pub sectors: Vec<InputSector>,
    pub sidedefs: Vec<InputSideDef>,
    pub linedefs: Vec<InputLineDef>,
    /// Carved convex floor outline per subsector.
    pub carved: Vec<Vec<Vec2>>,
    /// Create sky filler walls (the engine gates on having a sky texture).
    pub sky_fillers: bool,
}

/// A two-sided seg's owner, side, and 1D span along its linedef.
struct SegSpan {
    seg: usize,
    owner: u32,
    side: u32,
    t0: f32,
    t1: f32,
}

impl Bsp3dInput {
    /// `sky_flat` marks sky surfaces by flat name (typically `"F_SKY1"`);
    /// `None` disables all sky handling. `sky_fillers` additionally gates the
    /// synthetic filler walls (the engine requires a sky wall texture too).
    pub fn new<L, S, SE>(
        linedefs: &[L],
        sidedefs: &[S],
        sectors: &[SE],
        bsp: &BspOutput,
        sky_flat: Option<&str>,
        sky_fillers: bool,
    ) -> Self
    where
        L: LineDefAccess,
        S: SideDefAccess,
        SE: SectorAccess,
    {
        let verts: Vec<Vec2> = bsp
            .vertices
            .iter()
            .map(|v| Vec2::new(v.x as f32, v.y as f32))
            .collect();

        let in_sidedefs: Vec<InputSideDef> = sidedefs
            .iter()
            .map(|sd| InputSideDef {
                sector: sd.sector_idx() as u32,
                has_top: sd.has_top_tex(),
                has_bottom: sd.has_bottom_tex(),
                has_mid: sd.has_mid_tex(),
            })
            .collect();

        let in_linedefs: Vec<InputLineDef> = linedefs
            .iter()
            .map(|ld| {
                let raw = ld.special_u32();
                InputLineDef {
                    v1: verts[ld.start_vertex_idx()],
                    v2: verts[ld.end_vertex_idx()],
                    special: special_encode::encode_vanilla(raw).unwrap_or(raw),
                    tag: ld.tag_i16(),
                    sides: [
                        ld.front_sidedef_idx().map_or(NO_REF, |i| i as u32),
                        ld.back_sidedef_idx().map_or(NO_REF, |i| i as u32),
                    ],
                    two_sided: ld.flags_u32() & TWO_SIDED_FLAG != 0,
                }
            })
            .collect();

        let mut in_sectors: Vec<InputSector> = sectors
            .iter()
            .map(|s| InputSector {
                floor_h: s.floor_h(),
                ceil_h: s.ceil_h(),
                tag: s.tag_i16(),
                sky_floor: sky_flat.is_some_and(|n| s.floor_tex_is(n)),
                sky_ceil: sky_flat.is_some_and(|n| s.ceil_tex_is(n)),
                floor_plane: s.floor_plane(),
                ceil_plane: s.ceil_plane(),
                lines: Vec::new(),
            })
            .collect();
        for (li, ld) in in_linedefs.iter().enumerate() {
            for side in ld.sides {
                if side != NO_REF
                    && let Some(sec) =
                        in_sectors.get_mut(in_sidedefs[side as usize].sector as usize)
                {
                    sec.lines.push(li as u32);
                }
            }
        }

        // Segs compacted contiguous per subsector, mirroring the engine loader.
        let mut segs: Vec<InputSeg> = Vec::with_capacity(bsp.segs.len());
        let mut subsectors: Vec<InputSubSector> = Vec::with_capacity(bsp.subsectors.len());
        for ss in &bsp.subsectors {
            let start = segs.len() as u32;
            for &si in &ss.seg_indices {
                let rseg = &bsp.segs[si as usize];
                let side: u32 = match rseg.side {
                    Side::Front => 0,
                    Side::Back => 1,
                };
                let ld = &in_linedefs[rseg.linedef];
                let sidedef = ld.sides[side as usize];
                let frontsector = in_sidedefs[sidedef as usize].sector;
                let backsector = if ld.two_sided {
                    let back = ld.sides[(side ^ 1) as usize];
                    if back != NO_REF && (back as usize) < in_sidedefs.len() {
                        in_sidedefs[back as usize].sector
                    } else {
                        NO_REF
                    }
                } else {
                    NO_REF
                };
                segs.push(InputSeg {
                    v1: rseg.start as u32,
                    v2: rseg.end as u32,
                    offset: rseg.offset as f32,
                    linedef: rseg.linedef as u32,
                    side,
                    frontsector,
                    backsector,
                    back_subsectors: Vec::new(),
                });
            }
            let seg_count = segs.len() as u32 - start;
            let sector = if seg_count > 0 {
                segs[start as usize].frontsector
            } else {
                ss.sector
            };
            subsectors.push(InputSubSector {
                start_seg: start,
                seg_count,
                sector,
            });
        }

        link_back_subsectors(&mut segs, &in_linedefs, &subsectors, &verts);

        let carved: Vec<Vec<Vec2>> = bsp
            .subsectors
            .iter()
            .map(|ss| {
                let start = ss.polygon.first_vertex as usize;
                let count = ss.polygon.num_vertices as usize;
                bsp.poly_indices[start..start + count]
                    .iter()
                    .map(|&vi| verts[vi as usize])
                    .collect()
            })
            .collect();

        Self {
            verts,
            segs,
            subsectors,
            sectors: in_sectors,
            sidedefs: in_sidedefs,
            linedefs: in_linedefs,
            carved,
            sky_fillers: sky_fillers && sky_flat.is_some(),
        }
    }
}

/// For each two-sided seg, find the subsectors across it: segs on the same
/// linedef, other side, with an overlapping span.
fn link_back_subsectors(
    segs: &mut [InputSeg],
    linedefs: &[InputLineDef],
    subsectors: &[InputSubSector],
    verts: &[Vec2],
) {
    let mut owner_of = vec![0u32; segs.len()];
    for (ss_id, ss) in subsectors.iter().enumerate() {
        for si in ss.start_seg..ss.start_seg + ss.seg_count {
            owner_of[si as usize] = ss_id as u32;
        }
    }

    let mut by_linedef: BTreeMap<u32, Vec<SegSpan>> = BTreeMap::new();
    for (si, seg) in segs.iter().enumerate() {
        if seg.backsector == NO_REF {
            continue;
        }
        let ld = &linedefs[seg.linedef as usize];
        let axis = ld.v2 - ld.v1;
        let proj = |p: Vec2| (p - ld.v1).dot(axis);
        let (a, b) = (proj(verts[seg.v1 as usize]), proj(verts[seg.v2 as usize]));
        by_linedef.entry(seg.linedef).or_default().push(SegSpan {
            seg: si,
            owner: owner_of[si],
            side: seg.frontsector,
            t0: a.min(b),
            t1: a.max(b),
        });
    }

    for group in by_linedef.values() {
        for s in group {
            segs[s.seg].back_subsectors = group
                .iter()
                .filter(|o| o.side != s.side && o.t0 < s.t1 && s.t0 < o.t1)
                .map(|o| o.owner)
                .collect();
        }
    }
}
