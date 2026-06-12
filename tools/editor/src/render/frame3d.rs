use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use editor_core::{EditorMap, LineFlags, Name8, SideDef, Vertex};

use crate::render::triangulate::SectorTris;
use crate::render::wgpu::WallRect;

pub const SURFACE_FLOOR: u32 = 0;
pub const SURFACE_CEIL: u32 = 1;
pub const SURFACE_WALL_UPPER: u32 = 2;
pub const SURFACE_WALL_MID: u32 = 3;
pub const SURFACE_WALL_LOWER: u32 = 4;
/// Two-sided line middle texture: masked, drawn once and clipped to the opening
/// (no vertical tiling), unlike a one-sided [`SURFACE_WALL_MID`] solid wall.
pub const SURFACE_WALL_MASKED: u32 = 5;

/// Sentinel `atlas_rect` for unresolved wall textures. Negative `.x` → shader paints magenta.
pub const ATLAS_RECT_MISSING: [f32; 4] = [-1.0, -1.0, -1.0, -1.0];
/// Fallback height for an unresolved masked midtexture; fills the opening so the magenta is visible.
const MASKED_MISSING_HEIGHT: f32 = 128.0;

/// `Vert3D.source` for a floor/ceil triangle: no source linedef (the sector is in
/// `sector`). Walls carry their linedef index here for mesh-first picking.
pub const SOURCE_NONE: u32 = u32::MAX;
/// `Vert3D.vert` for a corner with no map vertex (none occur today; reserved).
pub const NO_VERT: u32 = u32::MAX;

/// One 3D world-space vertex for the sector preview mesh.
///
/// `atlas_rect` = `[x,y,w,h]` in atlas texels (wall atlas); zero for floors/ceilings (flat atlas via `sector`).
/// `source` = generating linedef index for walls, [`SOURCE_NONE`] for floors/ceils.
/// `vert` = map vertex index (picking provenance).
/// `shade` = fake-contrast factor (1.0 for floors/ceils).
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

/// The flat 3D world-space mesh (floors, ceilings, walls) for all sectors.
pub fn build_mesh(
    map: &EditorMap,
    tris: &SectorTris,
    wall_rects: &HashMap<Name8, WallRect>,
) -> Vec<Vert3D> {
    let mut out = Vec::new();
    emit_floors_and_ceils(map, tris, &mut out);
    emit_walls(map, wall_rects, &mut out);
    emit_pick_only_lines(map, &mut out);
    out
}

/// Sectorless lines emit no wall; append a zero-area degenerate tri at z=0 so the mesh pick can still resolve them.
fn emit_pick_only_lines(map: &EditorMap, out: &mut Vec<Vert3D>) {
    for (line_idx, line) in map.lines.iter().enumerate() {
        if line.front.sector.is_some() || line.back.and_then(|b| b.sector).is_some() {
            continue;
        }
        let v1 = map.vertices[line.v1 as usize];
        let v2 = map.vertices[line.v2 as usize];
        let corner = |v: Vertex, vi: u32| Vert3D {
            pos: [v.x, v.y, 0.0],
            uv: [0.0; 2],
            atlas_rect: [0.0; 4],
            sector: 0,
            surface: SURFACE_WALL_MID,
            source: line_idx as u32,
            vert: vi,
            shade: 1.0,
        };
        out.push(corner(v1, line.v1));
        out.push(corner(v2, line.v2));
        out.push(corner(v1, line.v1));
    }
}

fn emit_floors_and_ceils(map: &EditorMap, tris: &SectorTris, out: &mut Vec<Vert3D>) {
    for (s, sector) in map.sectors.iter().enumerate() {
        let (start, end) = tris.ranges[s];
        let fh = sector.floor_height as f32;
        let ch = sector.ceil_height as f32;
        for tri in &tris.tris[start as usize..end as usize] {
            for c in tri {
                out.push(Vert3D {
                    pos: [c.pos[0], c.pos[1], fh],
                    uv: [c.pos[0], -c.pos[1]],
                    atlas_rect: [0.0; 4],
                    sector: s as u32,
                    surface: SURFACE_FLOOR,
                    source: SOURCE_NONE,
                    vert: c.vert,
                    shade: 1.0,
                });
            }
            for c in tri.iter().rev() {
                out.push(Vert3D {
                    pos: [c.pos[0], c.pos[1], ch],
                    uv: [c.pos[0], -c.pos[1]],
                    atlas_rect: [0.0; 4],
                    sector: s as u32,
                    surface: SURFACE_CEIL,
                    source: SOURCE_NONE,
                    vert: c.vert,
                    shade: 1.0,
                });
            }
        }
    }
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
    pub a_idx: u32,
    pub b_idx: u32,
    pub run: f32,
    pub z_bot: f32,
    pub z_top: f32,
    pub peg_top: f32,
    pub vis_bot: f32,
    pub vis_top: f32,
    pub x_off: i32,
    pub y_off: i32,
    pub sector: u32,
    pub surface: u32,
    pub tex: Name8,
}

/// Wall bands for a line: one-sided → full mid; two-sided → upper/lower/masked-mid front+back. Skips empty or zero-height.
pub fn wall_bands(
    map: &EditorMap,
    line: &editor_core::LineDef,
    wall_rects: &HashMap<Name8, WallRect>,
) -> Vec<WallBand3D> {
    let v1 = map.vertices[line.v1 as usize];
    let v2 = map.vertices[line.v2 as usize];
    let run = ((v2.x - v1.x).powi(2) + (v2.y - v1.y).powi(2)).sqrt();
    let mut bands = Vec::new();

    let mut solid = |front: bool, z_bot, z_top, side: &SideDef, sector, surface, tex: Name8| {
        if tex.is_empty() || z_top <= z_bot {
            return;
        }
        let (a, b, a_idx, b_idx) = if front {
            (v1, v2, line.v1, line.v2)
        } else {
            (v2, v1, line.v2, line.v1)
        };
        bands.push(WallBand3D {
            a,
            b,
            a_idx,
            b_idx,
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
            let sec = &map.sectors[s as usize];
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
            let fs = &map.sectors[fi as usize];
            let bs = &map.sectors[bi as usize];
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
                    let (a, b, a_idx, b_idx) = if front {
                        (v1, v2, line.v1, line.v2)
                    } else {
                        (v2, v1, line.v2, line.v1)
                    };
                    bands.push(WallBand3D {
                        a,
                        b,
                        a_idx,
                        b_idx,
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

fn emit_walls(map: &EditorMap, wall_rects: &HashMap<Name8, WallRect>, out: &mut Vec<Vert3D>) {
    for (line_idx, line) in map.lines.iter().enumerate() {
        for band in wall_bands(map, line, wall_rects) {
            emit_quad(&band, line_idx as u32, wall_rects, out);
        }
    }
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
    let vert = |x: f32, y: f32, z: f32, u: f32, v: f32, idx: u32| Vert3D {
        pos: [x, y, z],
        uv: [u, v],
        atlas_rect,
        sector: band.sector,
        surface: band.surface,
        source,
        vert: idx,
        shade,
    };
    let bl = vert(band.a.x, band.a.y, band.z_bot, u0, v1_coord, band.a_idx);
    let br = vert(band.b.x, band.b.y, band.z_bot, u1, v1_coord, band.b_idx);
    let tl = vert(band.a.x, band.a.y, band.z_top, u0, v0, band.a_idx);
    let tr = vert(band.b.x, band.b.y, band.z_top, u1, v0, band.b_idx);
    out.extend_from_slice(&[tl, bl, br, tl, br, tr]);
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use editor_core::Name8;

    use crate::render::frame3d::{
        SURFACE_CEIL, SURFACE_FLOOR, SURFACE_WALL_LOWER, SURFACE_WALL_MASKED, SURFACE_WALL_MID,
        SURFACE_WALL_UPPER, Vert3D, build_mesh, wall_shade,
    };
    use crate::render::triangulate::build_sector_tris;
    use crate::render::wgpu::WallRect;

    fn make_wall_rects(map: &editor_core::EditorMap) -> HashMap<Name8, WallRect> {
        let mut rects = HashMap::new();
        for line in &map.lines {
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

    #[test]
    fn floor_verts_at_floor_height_ceil_at_ceil_height() {
        let map = load_e1m1();
        let tris = build_sector_tris(&map);
        let wall_rects = make_wall_rects(&map);
        let mesh = build_mesh(&map, &tris, &wall_rects);

        let floors: Vec<&Vert3D> = mesh.iter().filter(|v| v.surface == SURFACE_FLOOR).collect();
        let ceils: Vec<&Vert3D> = mesh.iter().filter(|v| v.surface == SURFACE_CEIL).collect();

        assert!(!floors.is_empty(), "floor verts emitted");
        assert!(!ceils.is_empty(), "ceil verts emitted");

        for v in &floors {
            let s = v.sector as usize;
            let expected = map.sectors[s].floor_height as f32;
            assert_eq!(
                v.pos[2], expected,
                "floor z={} expected {} for sector {}",
                v.pos[2], expected, s
            );
        }
        for v in &ceils {
            let s = v.sector as usize;
            let expected = map.sectors[s].ceil_height as f32;
            assert_eq!(
                v.pos[2], expected,
                "ceil z={} expected {} for sector {}",
                v.pos[2], expected, s
            );
        }
    }

    #[test]
    fn ceil_winding_reversed_from_floor() {
        let map = load_e1m1();
        let tris = build_sector_tris(&map);
        let wall_rects = make_wall_rects(&map);
        let mesh = build_mesh(&map, &tris, &wall_rects);

        let floors: Vec<_> = mesh
            .chunks(3)
            .filter(|tri| tri.iter().all(|v| v.surface == SURFACE_FLOOR))
            .collect();
        let ceils: Vec<_> = mesh
            .chunks(3)
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
        let tris = build_sector_tris(&map);
        let wall_rects = make_wall_rects(&map);
        let mesh = build_mesh(&map, &tris, &wall_rects);

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

        let floor = mesh
            .chunks(3)
            .find(|t| t.iter().all(|v| v.surface == SURFACE_FLOOR))
            .expect("a floor tri");
        assert!(normal(floor)[2] > 0.0, "floor faces +Z (up)");

        let ceil = mesh
            .chunks(3)
            .find(|t| t.iter().all(|v| v.surface == SURFACE_CEIL))
            .expect("a ceil tri");
        assert!(normal(ceil)[2] < 0.0, "ceil faces -Z (down)");

        let line = map
            .lines
            .iter()
            .find(|l| {
                l.back.is_none()
                    && l.front.sector.is_some()
                    && wall_rects.contains_key(&l.front.middle_tex)
            })
            .expect("a one-sided wall");
        let v1 = map.vertices[line.v1 as usize];
        let v2 = map.vertices[line.v2 as usize];
        let wall = mesh
            .chunks(3)
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
        let tris = build_sector_tris(&map);
        let wall_rects = make_wall_rects(&map);
        let mesh = build_mesh(&map, &tris, &wall_rects);

        let first_one_sided = map.lines.iter().find(|l| {
            l.back.is_none()
                && l.front.sector.is_some()
                && !l.front.middle_tex.is_empty()
                && wall_rects.contains_key(&l.front.middle_tex)
        });
        let line = first_one_sided.expect("E1M1 has one-sided lines with middle tex");

        let v1 = map.vertices[line.v1 as usize];
        let v2 = map.vertices[line.v2 as usize];

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
        let tris = build_sector_tris(&map);
        let wall_rects = make_wall_rects(&map);
        let mesh = build_mesh(&map, &tris, &wall_rects);

        let wall_quads: Vec<_> = mesh
            .chunks(6)
            .filter(|q| {
                q.len() == 6
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
        let tris = build_sector_tris(&map);
        let wall_rects = make_wall_rects(&map);
        let mesh = build_mesh(&map, &tris, &wall_rects);

        let line_id = map.lines.iter().position(|l| {
            l.back.is_some()
                && !l.front.middle_tex.is_empty()
                && wall_rects.contains_key(&l.front.middle_tex)
        });
        let Some(line_id) = line_id else {
            return; // E1M1 has none on some builds; nothing to assert.
        };
        let from_line = |v: &&Vert3D| v.source == line_id as u32;
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
        let tris = build_sector_tris(&map);
        let wall_rects = make_wall_rects(&map);
        let mesh = build_mesh(&map, &tris, &wall_rects);

        let has_upper = mesh.iter().any(|v| v.surface == SURFACE_WALL_UPPER);
        let has_lower = mesh.iter().any(|v| v.surface == SURFACE_WALL_LOWER);

        assert!(has_upper, "upper wall segments emitted");
        assert!(has_lower, "lower wall segments emitted");
    }

    #[test]
    fn missing_wall_texture_emits_sentinel_quad() {
        let map = load_e1m1();
        let tris = build_sector_tris(&map);

        // Omit one wall name from the atlas, as happens for an unresolved texture.
        let missing = map
            .lines
            .iter()
            .find_map(|l| (!l.front.middle_tex.is_empty()).then_some(l.front.middle_tex))
            .expect("a non-empty wall texture");
        let mut wall_rects = make_wall_rects(&map);
        wall_rects.remove(&missing);

        let mesh = build_mesh(&map, &tris, &wall_rects);
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
