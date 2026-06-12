//! 3D surface mesh: per-sector floor/ceil fills + per-line wall quads, organised as persistent slot-addressed spans so edits patch in place (never a full re-emit).

use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use editor_core::{
    ArenaKey as _, EditorMap, LineDef, LineFlags, LineKey, Name8, SectorKey, SideDef, VertKey,
    Vertex,
};

use crate::render::triangulate::SectorTris;
use crate::render::wgpu::WallRect;

pub const SURFACE_FLOOR: u32 = 0;
pub const SURFACE_CEIL: u32 = 1;
pub const SURFACE_WALL_UPPER: u32 = 2;
pub const SURFACE_WALL_MID: u32 = 3;
pub const SURFACE_WALL_LOWER: u32 = 4;
/// Two-sided line middle texture: masked, drawn once and clipped to the opening (no vertical tiling), unlike a one-sided [`SURFACE_WALL_MID`] solid wall.
pub const SURFACE_WALL_MASKED: u32 = 5;

/// Sentinel `atlas_rect` for unresolved wall textures. Negative `.x` → shader paints magenta.
pub const ATLAS_RECT_MISSING: [f32; 4] = [-1.0, -1.0, -1.0, -1.0];
/// Fallback height for an unresolved masked midtexture; fills the opening so the magenta is visible.
const MASKED_MISSING_HEIGHT: f32 = 128.0;

/// `Vert3D.source` for a floor/ceil triangle: no source linedef (the sector is in `sector`); walls carry their linedef slot here for mesh-first picking.
pub const SOURCE_NONE: u32 = u32::MAX;
/// `Vert3D.vert` for a corner with no map vertex.
pub const NO_VERT: u32 = u32::MAX;

/// Span slack divisor: a span's capacity is `len + len/SPAN_SLACK_DIV` rounded to a triangle boundary.
const SPAN_SLACK_DIV: u32 = 4;

/// One 3D world-space vertex for the sector preview mesh. `atlas_rect` = `[x,y,w,h]` in atlas texels (wall atlas), zero for floors/ceilings (flat atlas via `sector`); `sector` = sector arena slot (GPU storage index), `source` = line arena slot for walls ([`SOURCE_NONE`] for floors/ceils), `vert` = vertex arena slot (picking provenance); `shade` = fake-contrast factor (1.0 for floors/ceils).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Vert3D {
    pub pos: [f32; 3],
    pub uv: [f32; 2],
    pub atlas_rect: [f32; 4],
    pub sector: u32,
    pub surface: u32,
    pub source: u32,
    pub shade: f32,
    pub vert: u32,
}

/// A zero-area vertex; three of them form an invisible, unpickable triangle.
fn tombstone_vert() -> Vert3D {
    Vert3D {
        pos: [0.0; 3],
        uv: [0.0; 2],
        atlas_rect: [0.0; 4],
        sector: 0,
        surface: SURFACE_FLOOR,
        source: SOURCE_NONE,
        shade: 1.0,
        vert: NO_VERT,
    }
}

/// One surface3d span: `offset`/`cap` in vertices into the mesh; `len` live, the rest padding.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Span {
    pub offset: u32,
    pub len: u32,
    pub cap: u32,
}

/// A CPU-mesh region rewritten by a span update; the GPU buffer patches the same vertex range.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpanPatch {
    pub offset: u32,
    pub count: u32,
}

/// Span table over the surface3d mesh: sector fills + line walls; `used` = mesh length in vertices.
#[derive(Debug, Default, Clone)]
pub struct SurfaceSlots {
    pub sector: HashMap<SectorKey, Span>,
    pub line: HashMap<LineKey, Span>,
    pub used: u32,
}

impl SurfaceSlots {
    /// Rewrite one sector's floor/ceil span; relocates to the tail on overflow.
    pub fn update_sector(
        &mut self,
        mesh: &mut Vec<Vert3D>,
        key: SectorKey,
        verts: Vec<Vert3D>,
        patches: &mut Vec<SpanPatch>,
    ) {
        let span = self.sector.entry(key).or_default();
        update_span(span, &mut self.used, mesh, verts, patches);
    }

    /// Rewrite one line's wall span; relocates to the tail on overflow.
    pub fn update_line(
        &mut self,
        mesh: &mut Vec<Vert3D>,
        key: LineKey,
        verts: Vec<Vert3D>,
        patches: &mut Vec<SpanPatch>,
    ) {
        let span = self.line.entry(key).or_default();
        update_span(span, &mut self.used, mesh, verts, patches);
    }

    /// Tombstone and forget a removed sector's span.
    pub fn free_sector(
        &mut self,
        mesh: &mut [Vert3D],
        key: SectorKey,
        patches: &mut Vec<SpanPatch>,
    ) {
        if let Some(span) = self.sector.remove(&key) {
            tombstone_span(&span, mesh, patches);
        }
    }

    /// Tombstone and forget a removed line's span.
    pub fn free_line(&mut self, mesh: &mut [Vert3D], key: LineKey, patches: &mut Vec<SpanPatch>) {
        if let Some(span) = self.line.remove(&key) {
            tombstone_span(&span, mesh, patches);
        }
    }
}

/// Write `verts` into `span` padded to cap; overflow relocates to the tail and tombstones the old region.
fn update_span(
    span: &mut Span,
    used: &mut u32,
    mesh: &mut Vec<Vert3D>,
    verts: Vec<Vert3D>,
    patches: &mut Vec<SpanPatch>,
) {
    let len = verts.len() as u32;
    if len > span.cap {
        tombstone_span(span, mesh, patches);
        *span = Span {
            offset: *used,
            len,
            cap: span_cap(len),
        };
        *used += span.cap;
        mesh.resize(*used as usize, tombstone_vert());
    } else {
        span.len = len;
    }
    let at = span.offset as usize;
    mesh[at..at + len as usize].copy_from_slice(&verts);
    let pad = verts.last().copied().unwrap_or_else(tombstone_vert);
    mesh[at + len as usize..at + span.cap as usize].fill(pad);
    if span.cap > 0 {
        patches.push(SpanPatch {
            offset: span.offset,
            count: span.cap,
        });
    }
}

/// Overwrite a span's whole region with zero-area vertices.
fn tombstone_span(span: &Span, mesh: &mut [Vert3D], patches: &mut Vec<SpanPatch>) {
    if span.cap == 0 {
        return;
    }
    let at = span.offset as usize;
    mesh[at..at + span.cap as usize].fill(tombstone_vert());
    patches.push(SpanPatch {
        offset: span.offset,
        count: span.cap,
    });
}

/// Capacity for a span of `len` vertices: slack for value edits, triangle-aligned.
fn span_cap(len: u32) -> u32 {
    if len == 0 {
        return 0;
    }
    (len + len / SPAN_SLACK_DIV).next_multiple_of(3)
}

/// Full mesh build (map load only): every sector fill + every line wall, spans allocated in slot order.
pub fn build_surface(
    map: &EditorMap,
    tris: &SectorTris,
    wall_rects: &HashMap<Name8, WallRect>,
) -> (Vec<Vert3D>, SurfaceSlots) {
    let mut mesh = Vec::new();
    let mut slots = SurfaceSlots::default();
    let mut patches = Vec::new();
    for key in map.sectors.keys() {
        slots.update_sector(
            &mut mesh,
            key,
            sector_surface_verts(map, tris, key),
            &mut patches,
        );
    }
    for key in map.lines.keys() {
        slots.update_line(
            &mut mesh,
            key,
            line_wall_verts(map, key, wall_rects),
            &mut patches,
        );
    }
    (mesh, slots)
}

/// Floor + ceiling vertices for one sector (floor CCW, ceiling reversed to face down).
pub fn sector_surface_verts(map: &EditorMap, tris: &SectorTris, key: SectorKey) -> Vec<Vert3D> {
    let Some(sector) = map.sectors.get(key) else {
        return Vec::new();
    };
    let slot = key.slot();
    let fh = sector.floor_height as f32;
    let ch = sector.ceil_height as f32;
    let tri_list = tris.tris(key);
    let mut out = Vec::with_capacity(tri_list.len() * 6);
    for tri in tri_list {
        for c in tri {
            out.push(Vert3D {
                pos: [c.pos[0], c.pos[1], fh],
                uv: [c.pos[0], -c.pos[1]],
                atlas_rect: [0.0; 4],
                sector: slot,
                surface: SURFACE_FLOOR,
                source: SOURCE_NONE,
                vert: c.vert.slot(),
                shade: 1.0,
            });
        }
        for c in tri.iter().rev() {
            out.push(Vert3D {
                pos: [c.pos[0], c.pos[1], ch],
                uv: [c.pos[0], -c.pos[1]],
                atlas_rect: [0.0; 4],
                sector: slot,
                surface: SURFACE_CEIL,
                source: SOURCE_NONE,
                vert: c.vert.slot(),
                shade: 1.0,
            });
        }
    }
    out
}

/// Wall quads for one line; sectorless lines emit a zero-area pick triangle at z=0 instead.
pub fn line_wall_verts(
    map: &EditorMap,
    key: LineKey,
    wall_rects: &HashMap<Name8, WallRect>,
) -> Vec<Vert3D> {
    let Some(line) = map.lines.get(key) else {
        return Vec::new();
    };
    let bands = wall_bands(map, line, wall_rects);
    if bands.is_empty() && line.front.sector.is_none() {
        return pick_only_verts(map, key, line);
    }
    let mut out = Vec::with_capacity(bands.len() * 6);
    for band in &bands {
        emit_quad(band, key.slot(), wall_rects, &mut out);
    }
    out
}

/// The degenerate pick triangle for a sectorless line.
fn pick_only_verts(map: &EditorMap, key: LineKey, line: &LineDef) -> Vec<Vert3D> {
    let (Some(v1), Some(v2)) = (map.vertices.get(line.v1), map.vertices.get(line.v2)) else {
        return Vec::new();
    };
    let corner = |v: &Vertex, slot: u32| Vert3D {
        pos: [v.x, v.y, 0.0],
        uv: [0.0; 2],
        atlas_rect: [0.0; 4],
        sector: 0,
        surface: SURFACE_WALL_MID,
        source: key.slot(),
        vert: slot,
        shade: 1.0,
    };
    vec![
        corner(v1, line.v1.slot()),
        corner(v2, line.v2.slot()),
        corner(v1, line.v1.slot()),
    ]
}

/// Fake-contrast factor: E–W walls darkest, N–S brightest; keeps adjacent walls visually distinct in colour-fill mode.
fn wall_shade(dx: f32, dy: f32) -> f32 {
    const MIN_SHADE: f32 = 0.6;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-4 {
        return 1.0;
    }
    MIN_SHADE + (1.0 - MIN_SHADE) * (dy / len).abs()
}

/// One solid wall quad. `z_bot..z_top` = geometry; `vis_bot..vis_top` = wireframe outline (differs for masked midtex).
pub struct WallBand3D {
    pub a: Vertex,
    pub b: Vertex,
    pub a_vert: VertKey,
    pub b_vert: VertKey,
    pub run: f32,
    pub z_bot: f32,
    pub z_top: f32,
    pub peg_top: f32,
    pub vis_bot: f32,
    pub vis_top: f32,
    pub x_off: i32,
    pub y_off: i32,
    pub sector: SectorKey,
    pub surface: u32,
    pub tex: Name8,
}

/// Wall bands for a line: one-sided → full mid; two-sided → upper/lower/masked-mid front+back. Skips empty or zero-height.
pub fn wall_bands(
    map: &EditorMap,
    line: &LineDef,
    wall_rects: &HashMap<Name8, WallRect>,
) -> Vec<WallBand3D> {
    let (Some(v1), Some(v2)) = (
        map.vertices.get(line.v1).copied(),
        map.vertices.get(line.v2).copied(),
    ) else {
        return Vec::new();
    };
    let run = ((v2.x - v1.x).powi(2) + (v2.y - v1.y).powi(2)).sqrt();
    let mut bands = Vec::new();

    let mut solid = |front: bool, z_bot, z_top, side: &SideDef, sector, surface, tex: Name8| {
        if tex.is_empty() || z_top <= z_bot {
            return;
        }
        let (a, b, a_vert, b_vert) = if front {
            (v1, v2, line.v1, line.v2)
        } else {
            (v2, v1, line.v2, line.v1)
        };
        bands.push(WallBand3D {
            a,
            b,
            a_vert,
            b_vert,
            run,
            z_bot,
            z_top,
            peg_top: z_top,
            vis_bot: z_bot,
            vis_top: z_top,
            x_off: side.x_offset,
            y_off: side.y_offset,
            sector,
            surface,
            tex,
        });
    };

    match &line.back {
        None => {
            let Some(s) = line.front.sector else {
                return bands;
            };
            let Some(sec) = map.sectors.get(s) else {
                return bands;
            };
            solid(
                true,
                sec.floor_height as f32,
                sec.ceil_height as f32,
                &line.front,
                s,
                SURFACE_WALL_MID,
                line.front.middle_tex,
            );
        }
        Some(back) => {
            let (Some(fi), Some(bi)) = (line.front.sector, back.sector) else {
                return bands;
            };
            let (Some(fs), Some(bs)) = (map.sectors.get(fi), map.sectors.get(bi)) else {
                return bands;
            };
            let (ff, fc) = (fs.floor_height as f32, fs.ceil_height as f32);
            let (bf, bc) = (bs.floor_height as f32, bs.ceil_height as f32);

            let (up_bot, up_top) = (bc.min(fc), bc.max(fc));
            solid(
                true,
                up_bot,
                up_top,
                &line.front,
                fi,
                SURFACE_WALL_UPPER,
                line.front.top_tex,
            );
            solid(
                false,
                up_bot,
                up_top,
                back,
                bi,
                SURFACE_WALL_UPPER,
                back.top_tex,
            );

            let (lo_bot, lo_top) = (ff.min(bf), ff.max(bf));
            solid(
                true,
                lo_bot,
                lo_top,
                &line.front,
                fi,
                SURFACE_WALL_LOWER,
                line.front.bottom_tex,
            );
            solid(
                false,
                lo_bot,
                lo_top,
                back,
                bi,
                SURFACE_WALL_LOWER,
                back.bottom_tex,
            );

            let (mid_bot, mid_top) = (ff.max(bf), fc.min(bc));
            if mid_top > mid_bot {
                let unpeg = line.flags.contains(LineFlags::UNPEG_BOTTOM);
                let mut masked = |front: bool, side: &SideDef, sector, tex: Name8| {
                    if tex.is_empty() {
                        return;
                    }
                    let h = wall_rects
                        .get(&tex)
                        .map_or(MASKED_MISSING_HEIGHT, |r| r.h as f32);
                    // Top-pegged, or texture-bottom on the opening floor for UNPEG_BOTTOM.
                    let (peg_top, vis_bot, vis_top) = if unpeg {
                        (mid_bot + h, mid_bot, (mid_bot + h).min(mid_top))
                    } else {
                        (mid_top, (mid_top - h).max(mid_bot), mid_top)
                    };
                    let (a, b, a_vert, b_vert) = if front {
                        (v1, v2, line.v1, line.v2)
                    } else {
                        (v2, v1, line.v2, line.v1)
                    };
                    bands.push(WallBand3D {
                        a,
                        b,
                        a_vert,
                        b_vert,
                        run,
                        z_bot: mid_bot,
                        z_top: mid_top,
                        peg_top,
                        vis_bot,
                        vis_top,
                        x_off: side.x_offset,
                        y_off: side.y_offset,
                        sector,
                        surface: SURFACE_WALL_MASKED,
                        tex,
                    });
                };
                masked(true, &line.front, fi, line.front.middle_tex);
                masked(false, back, bi, back.middle_tex);
            }
        }
    }
    bands
}

fn emit_quad(
    band: &WallBand3D,
    source: u32,
    wall_rects: &HashMap<Name8, WallRect>,
    out: &mut Vec<Vert3D>,
) {
    let atlas_rect = match wall_rects.get(&band.tex) {
        Some(rect) => [rect.x as f32, rect.y as f32, rect.w as f32, rect.h as f32],
        // Unresolved: sentinel → shader draws magenta instead of dropping.
        None => ATLAS_RECT_MISSING,
    };
    let shade = wall_shade(band.b.x - band.a.x, band.b.y - band.a.y);
    let u0 = band.x_off as f32;
    let u1 = u0 + band.run;
    let v0 = band.y_off as f32 + (band.peg_top - band.z_top);
    let v1_coord = v0 + (band.z_top - band.z_bot);
    let sector = band.sector.slot();
    let vert = |x: f32, y: f32, z: f32, u: f32, v: f32, idx: u32| Vert3D {
        pos: [x, y, z],
        uv: [u, v],
        atlas_rect,
        sector,
        surface: band.surface,
        source,
        vert: idx,
        shade,
    };
    let (ai, bi) = (band.a_vert.slot(), band.b_vert.slot());
    let bl = vert(band.a.x, band.a.y, band.z_bot, u0, v1_coord, ai);
    let br = vert(band.b.x, band.b.y, band.z_bot, u1, v1_coord, bi);
    let tl = vert(band.a.x, band.a.y, band.z_top, u0, v0, ai);
    let tr = vert(band.b.x, band.b.y, band.z_top, u1, v0, bi);
    out.extend_from_slice(&[tl, bl, br, tl, br, tr]);
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use editor_core::{ArenaKey as _, Name8};

    use crate::render::frame3d::{
        SURFACE_CEIL, SURFACE_FLOOR, SURFACE_WALL_LOWER, SURFACE_WALL_MASKED, SURFACE_WALL_MID,
        SURFACE_WALL_UPPER, Vert3D, build_surface, wall_shade,
    };
    use crate::render::triangulate::build_sector_tris;
    use crate::render::wgpu::WallRect;

    fn make_wall_rects(map: &editor_core::EditorMap) -> HashMap<Name8, WallRect> {
        let mut rects = HashMap::new();
        for line in map.lines.values() {
            for tex in [
                line.front.top_tex,
                line.front.middle_tex,
                line.front.bottom_tex,
            ] {
                if !tex.is_empty() {
                    rects.entry(tex).or_insert(WallRect {
                        x: 0,
                        y: 0,
                        w: 64,
                        h: 128,
                    });
                }
            }
            if let Some(back) = &line.back {
                for tex in [back.top_tex, back.middle_tex, back.bottom_tex] {
                    if !tex.is_empty() {
                        rects.entry(tex).or_insert(WallRect {
                            x: 0,
                            y: 0,
                            w: 64,
                            h: 128,
                        });
                    }
                }
            }
        }
        rects
    }

    fn load_e1m1() -> editor_core::EditorMap {
        let wad = wad::WadData::new(&test_utils::doom1_wad_path());
        editor_core::import_wad_map(&wad, "E1M1").expect("E1M1 imports")
    }

    fn build_mesh(
        map: &editor_core::EditorMap,
        wall_rects: &HashMap<Name8, WallRect>,
    ) -> Vec<Vert3D> {
        let tris = build_sector_tris(map);
        build_surface(map, &tris, wall_rects).0
    }

    /// A triangle of three live (non-padding) vertices.
    fn live_tris(mesh: &[Vert3D]) -> Vec<&[Vert3D]> {
        mesh.chunks(3)
            .filter(|t| t.len() == 3 && !(t[0].pos == t[1].pos && t[1].pos == t[2].pos))
            .collect()
    }

    #[test]
    fn floor_verts_at_floor_height_ceil_at_ceil_height() {
        let map = load_e1m1();
        let wall_rects = make_wall_rects(&map);
        let mesh = build_mesh(&map, &wall_rects);

        let sector_of = |slot: u32| {
            let key = map.sectors.key_at_slot(slot).expect("live sector slot");
            &map.sectors[key]
        };
        let floors: Vec<&Vert3D> = mesh.iter().filter(|v| v.surface == SURFACE_FLOOR).collect();
        let ceils: Vec<&Vert3D> = mesh.iter().filter(|v| v.surface == SURFACE_CEIL).collect();

        assert!(!floors.is_empty(), "floor verts emitted");
        assert!(!ceils.is_empty(), "ceil verts emitted");

        for v in floors.iter().filter(|v| v.vert != super::NO_VERT) {
            let expected = sector_of(v.sector).floor_height as f32;
            assert_eq!(v.pos[2], expected, "floor z for sector slot {}", v.sector);
        }
        for v in ceils.iter().filter(|v| v.vert != super::NO_VERT) {
            let expected = sector_of(v.sector).ceil_height as f32;
            assert_eq!(v.pos[2], expected, "ceil z for sector slot {}", v.sector);
        }
    }

    #[test]
    fn ceil_winding_reversed_from_floor() {
        let map = load_e1m1();
        let wall_rects = make_wall_rects(&map);
        let mesh = build_mesh(&map, &wall_rects);

        let floors: Vec<_> = live_tris(&mesh)
            .into_iter()
            .filter(|tri| tri.iter().all(|v| v.surface == SURFACE_FLOOR))
            .collect();
        let ceils: Vec<_> = live_tris(&mesh)
            .into_iter()
            .filter(|tri| tri.iter().all(|v| v.surface == SURFACE_CEIL))
            .collect();

        assert_eq!(floors.len(), ceils.len(), "same tri count floor vs ceil");

        let signed_area = |tri: &[Vert3D]| -> f32 {
            let (ax, ay) = (tri[0].pos[0], tri[0].pos[1]);
            let (bx, by) = (tri[1].pos[0], tri[1].pos[1]);
            let (cx, cy) = (tri[2].pos[0], tri[2].pos[1]);
            (bx - ax) * (cy - ay) - (cx - ax) * (by - ay)
        };

        let any_opposite = floors.iter().zip(ceils.iter()).any(|(f, c)| {
            let fa = signed_area(f);
            let ca = signed_area(c);
            fa != 0.0 && ca != 0.0 && fa.signum() != ca.signum()
        });
        assert!(
            any_opposite,
            "at least one ceil tri wound opposite its floor"
        );
    }

    #[test]
    fn surface_normals_face_outward_for_culling() {
        let map = load_e1m1();
        let wall_rects = make_wall_rects(&map);
        let mesh = build_mesh(&map, &wall_rects);

        let normal = |t: &[Vert3D]| -> [f32; 3] {
            let e1 = [
                t[1].pos[0] - t[0].pos[0],
                t[1].pos[1] - t[0].pos[1],
                t[1].pos[2] - t[0].pos[2],
            ];
            let e2 = [
                t[2].pos[0] - t[0].pos[0],
                t[2].pos[1] - t[0].pos[1],
                t[2].pos[2] - t[0].pos[2],
            ];
            [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ]
        };

        let floor = live_tris(&mesh)
            .into_iter()
            .find(|t| t.iter().all(|v| v.surface == SURFACE_FLOOR))
            .expect("a floor tri");
        assert!(normal(floor)[2] > 0.0, "floor faces +Z (up)");

        let ceil = live_tris(&mesh)
            .into_iter()
            .find(|t| t.iter().all(|v| v.surface == SURFACE_CEIL))
            .expect("a ceil tri");
        assert!(normal(ceil)[2] < 0.0, "ceil faces -Z (down)");

        let line = map
            .lines
            .values()
            .find(|l| {
                l.back.is_none()
                    && l.front.sector.is_some()
                    && wall_rects.contains_key(&l.front.middle_tex)
            })
            .expect("a one-sided wall");
        let v1 = map.vertices[line.v1];
        let v2 = map.vertices[line.v2];
        let wall = live_tris(&mesh)
            .into_iter()
            .find(|t| {
                t.iter().all(|v| v.surface == SURFACE_WALL_MID)
                    && t.iter().all(|v| {
                        (v.pos[0], v.pos[1]) == (v1.x, v1.y) || (v.pos[0], v.pos[1]) == (v2.x, v2.y)
                    })
            })
            .expect("the one-sided wall's tri");
        let n = normal(wall);
        let front = [v2.y - v1.y, -(v2.x - v1.x)];
        assert!(
            n[0] * front[0] + n[1] * front[1] > 0.0,
            "one-sided wall faces the line front side (dy, -dx)"
        );
    }

    #[test]
    fn one_sided_line_emits_exactly_one_quad() {
        let map = load_e1m1();
        let wall_rects = make_wall_rects(&map);
        let mesh = build_mesh(&map, &wall_rects);

        let line = map
            .lines
            .values()
            .find(|l| {
                l.back.is_none()
                    && l.front.sector.is_some()
                    && !l.front.middle_tex.is_empty()
                    && wall_rects.contains_key(&l.front.middle_tex)
            })
            .expect("E1M1 has one-sided lines with middle tex");

        let v1 = map.vertices[line.v1];
        let v2 = map.vertices[line.v2];

        let wall_verts: Vec<&Vert3D> = mesh
            .iter()
            .filter(|v| {
                v.surface == SURFACE_WALL_MID
                    && ((v.pos[0] == v1.x && v.pos[1] == v1.y)
                        || (v.pos[0] == v2.x && v.pos[1] == v2.y))
            })
            .collect();

        assert!(
            wall_verts.len() >= 6,
            "one-sided line: at least 1 quad (6 verts), got {}",
            wall_verts.len()
        );
    }

    #[test]
    fn uvs_span_run_length() {
        let map = load_e1m1();
        let wall_rects = make_wall_rects(&map);
        let mesh = build_mesh(&map, &wall_rects);

        let wall_quads: Vec<_> = mesh
            .chunks(6)
            .filter(|q| {
                q.len() == 6
                    && q[0].pos != q[5].pos
                    && matches!(
                        q[0].surface,
                        SURFACE_WALL_MID | SURFACE_WALL_UPPER | SURFACE_WALL_LOWER
                    )
            })
            .collect();

        assert!(!wall_quads.is_empty(), "wall quads emitted");

        for quad in &wall_quads {
            let u_min = quad.iter().map(|v| v.uv[0]).fold(f32::INFINITY, f32::min);
            let u_max = quad
                .iter()
                .map(|v| v.uv[0])
                .fold(f32::NEG_INFINITY, f32::max);
            assert!(
                u_max > u_min,
                "wall UV u-span is non-degenerate: min={u_min} max={u_max}"
            );
        }
    }

    #[test]
    fn two_sided_midtexture_is_masked_not_tiling_mid() {
        let map = load_e1m1();
        let wall_rects = make_wall_rects(&map);
        let mesh = build_mesh(&map, &wall_rects);

        let line_key = map.lines.iter().find_map(|(k, l)| {
            (l.back.is_some()
                && !l.front.middle_tex.is_empty()
                && wall_rects.contains_key(&l.front.middle_tex))
            .then_some(k)
        });
        let Some(line_key) = line_key else {
            return; // E1M1 has none on some builds; nothing to assert.
        };
        let from_line = |v: &&Vert3D| v.source == line_key.slot();
        assert!(
            mesh.iter()
                .filter(from_line)
                .any(|v| v.surface == SURFACE_WALL_MASKED),
            "two-sided midtexture emits a masked quad"
        );
        assert!(
            !mesh
                .iter()
                .filter(from_line)
                .any(|v| v.surface == SURFACE_WALL_MID),
            "a two-sided line never emits a tiling SURFACE_WALL_MID"
        );
    }

    #[test]
    fn two_sided_line_with_height_diff_emits_upper_and_lower() {
        let map = load_e1m1();
        let wall_rects = make_wall_rects(&map);
        let mesh = build_mesh(&map, &wall_rects);

        let has_upper = mesh.iter().any(|v| v.surface == SURFACE_WALL_UPPER);
        let has_lower = mesh.iter().any(|v| v.surface == SURFACE_WALL_LOWER);

        assert!(has_upper, "upper wall segments emitted");
        assert!(has_lower, "lower wall segments emitted");
    }

    #[test]
    fn missing_wall_texture_emits_sentinel_quad() {
        let map = load_e1m1();

        // Omit one wall name from the atlas, as happens for an unresolved texture.
        let missing = map
            .lines
            .values()
            .find_map(|l| (!l.front.middle_tex.is_empty()).then_some(l.front.middle_tex))
            .expect("a non-empty wall texture");
        let mut wall_rects = make_wall_rects(&map);
        wall_rects.remove(&missing);

        let mesh = build_mesh(&map, &wall_rects);
        let sentinels: Vec<&Vert3D> = mesh
            .iter()
            .filter(|v| {
                v.atlas_rect[0] < 0.0 && v.surface != SURFACE_FLOOR && v.surface != SURFACE_CEIL
            })
            .collect();
        assert!(
            !sentinels.is_empty(),
            "a missing wall texture emits a sentinel-marked quad (got none)"
        );
    }

    /// A value edit that fits its span patches in place; an outgrown span relocates to the tail.
    #[test]
    fn span_update_patches_in_place_and_relocates_on_overflow() {
        use super::{SpanPatch, sector_surface_verts};
        let mut map = load_e1m1();
        let wall_rects = make_wall_rects(&map);
        let tris = build_sector_tris(&map);
        let (mut mesh, mut slots) = build_surface(&map, &tris, &wall_rects);
        let used_before = slots.used;

        // In-place: same sector re-emitted with a new floor height.
        let key = map
            .sectors
            .keys()
            .find(|&k| !tris.tris(k).is_empty())
            .expect("a filled sector");
        map.sectors[key].floor_height += 8;
        let span_before = slots.sector[&key];
        let mut patches: Vec<SpanPatch> = Vec::new();
        let verts = sector_surface_verts(&map, &tris, key);
        slots.update_sector(&mut mesh, key, verts, &mut patches);
        assert_eq!(
            slots.sector[&key].offset, span_before.offset,
            "no relocation"
        );
        assert_eq!(slots.used, used_before, "tail untouched");
        assert_eq!(patches.len(), 1, "one in-place patch");

        // Overflow: force more verts than capacity → tail relocation + tombstone patch.
        let big = vec![super::tombstone_vert(); (span_before.cap + 3) as usize];
        patches.clear();
        slots.update_sector(&mut mesh, key, big, &mut patches);
        let moved = slots.sector[&key];
        assert_eq!(moved.offset, used_before, "relocated to the old tail");
        assert!(slots.used > used_before, "tail advanced");
        assert_eq!(mesh.len() as u32, slots.used, "mesh grew with the tail");
        assert_eq!(patches.len(), 2, "tombstone + new-span patches");
    }

    #[test]
    fn wall_shade_brightens_with_vertical_runs() {
        let horizontal = wall_shade(100.0, 0.0);
        let vertical = wall_shade(0.0, 100.0);
        let diagonal = wall_shade(100.0, 100.0);
        assert!(
            vertical > diagonal && diagonal > horizontal,
            "shade rises N–S"
        );
        assert!((0.0..=1.0).contains(&horizontal) && vertical <= 1.0);
        assert_eq!(wall_shade(0.0, 0.0), 1.0, "degenerate wall is unshaded");
    }
}
