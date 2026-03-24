use egui::Color32;
use glam::Vec2;
use level::{LevelData, SurfaceKind, is_subsector, subsector_index};
use wad::WadData;
use wad::types::WadPalette;

const BRIGHTNESS_REF_RANGE: f32 = 1024.0;
const BRIGHTNESS_MIN: f32 = 0.2;
const BRIGHTNESS_MAX: f32 = 1.0;
const BRIGHTNESS_CENTER: f32 = 0.5;

pub struct ViewerData {
    pub map_name: String,
    pub min: Vec2,
    pub max: Vec2,
    pub linedefs: Vec<ViewLinedef>,
    pub sectors: Vec<ViewSector>,
    pub subsectors: Vec<ViewSubsector>,
    pub divlines: Vec<ViewDivline>,
    pub vertices: Vec<ViewVertex>,
    pub ss_divline_path: Vec<Vec<usize>>,
    pub floor_height_value: Vec<f32>,
    pub floor_texture_color: Vec<Color32>,
}

pub struct ViewLinedef {
    pub index: usize,
    pub v1: Vec2,
    pub v2: Vec2,
    pub is_two_sided: bool,
    pub front_sector_id: usize,
    pub back_sector_id: Option<usize>,
    pub special: i16,
    pub tag: i16,
}

pub struct ViewSector {
    pub floor_height: f32,
    pub ceiling_height: f32,
    pub light_level: usize,
    pub special: i16,
    pub tag: i16,
}

pub struct ViewSubsector {
    pub index: usize,
    pub sector_id: usize,
    pub vertices: Vec<Vec2>,
    pub polygons: Vec<ViewPolygon>,
    pub aabb_min: Vec2,
    pub aabb_max: Vec2,
    #[allow(dead_code)]
    pub wall_linedef_ids: Vec<usize>,
}

pub struct ViewPolygon {
    pub kind: String,
    pub vertices: Vec<glam::Vec3>,
}

pub struct ViewDivline {
    pub index: usize,
    pub origin: Vec2,
    pub dir: Vec2,
}

pub struct ViewVertex {
    pub index: usize,
    pub pos: Vec2,
    pub linedef_ids: Vec<usize>,
}

pub fn extract_viewer_data(map_name: &str, level_data: &LevelData, wad: &WadData) -> ViewerData {
    let extents = level_data.get_map_extents();

    let linedefs: Vec<ViewLinedef> = level_data
        .linedefs
        .iter()
        .enumerate()
        .map(|(i, ld)| ViewLinedef {
            index: i,
            v1: ld.v1.pos,
            v2: ld.v2.pos,
            is_two_sided: ld.backsector.is_some(),
            front_sector_id: ld.frontsector.num as usize,
            back_sector_id: ld.backsector.as_ref().map(|bs| bs.num as usize),
            special: ld.special,
            tag: ld.tag,
        })
        .collect();

    let sectors: Vec<ViewSector> = level_data
        .sectors
        .iter()
        .map(|s| ViewSector {
            floor_height: s.floorheight.to_f32(),
            ceiling_height: s.ceilingheight.to_f32(),
            light_level: s.lightlevel,
            special: s.special,
            tag: s.tag,
        })
        .collect();

    let floor_height_value: Vec<f32> = {
        let min_h = sectors
            .iter()
            .map(|s| s.floor_height)
            .fold(f32::MAX, f32::min);
        let max_h = sectors
            .iter()
            .map(|s| s.floor_height)
            .fold(f32::MIN, f32::max);
        let range = max_h - min_h;
        let half_band = (BRIGHTNESS_MAX - BRIGHTNESS_MIN) * 0.5 * (range / BRIGHTNESS_REF_RANGE);
        sectors
            .iter()
            .map(|s| {
                if range <= 0.0 {
                    BRIGHTNESS_CENTER
                } else {
                    let t = (s.floor_height - min_h) / range;
                    (BRIGHTNESS_CENTER - half_band + t * 2.0 * half_band)
                        .clamp(BRIGHTNESS_MIN, BRIGHTNESS_MAX)
                }
            })
            .collect()
    };

    let floor_texture_color: Vec<Color32> = {
        let palette = wad.lump_iter::<WadPalette>("PLAYPAL").next().unwrap();
        let flats: Vec<_> = wad.flats_iter().collect();
        level_data
            .sectors
            .iter()
            .map(|s| match flats.get(s.floorpic) {
                Some(f) if !f.data.is_empty() => {
                    let (mut r, mut g, mut b) = (0u64, 0u64, 0u64);
                    let count = f.data.len() as u64;
                    for &pi in &f.data {
                        let c = palette.0[pi as usize];
                        r += ((c >> 16) & 0xFF) as u64;
                        g += ((c >> 8) & 0xFF) as u64;
                        b += (c & 0xFF) as u64;
                    }
                    Color32::from_rgb((r / count) as u8, (g / count) as u8, (b / count) as u8)
                }
                _ => Color32::from_rgb(128, 128, 128),
            })
            .collect()
    };

    let pos_key = |v: Vec2| -> (u32, u32) { (v.x.to_bits(), v.y.to_bits()) };
    let pos_to_vidx: std::collections::HashMap<(u32, u32), usize> = level_data
        .vertexes
        .iter()
        .enumerate()
        .map(|(i, &v)| (pos_key(v.pos), i))
        .collect();
    let mut vert_ld_ids: Vec<Vec<usize>> = vec![Vec::new(); level_data.vertexes.len()];
    for ld in &linedefs {
        if let Some(&vi) = pos_to_vidx.get(&pos_key(ld.v1)) {
            vert_ld_ids[vi].push(ld.index);
        }
        if let Some(&vi) = pos_to_vidx.get(&pos_key(ld.v2)) {
            vert_ld_ids[vi].push(ld.index);
        }
    }
    drop(pos_to_vidx);
    let vertices: Vec<ViewVertex> = level_data
        .vertexes
        .iter()
        .enumerate()
        .map(|(i, &v)| ViewVertex {
            index: i,
            pos: v.pos,
            linedef_ids: std::mem::take(&mut vert_ld_ids[i]),
        })
        .collect();

    let bsp = &level_data.bsp_3d;
    let subsectors: Vec<ViewSubsector> = bsp
        .carved_polygons
        .iter()
        .enumerate()
        .zip(bsp.subsector_leaves.iter())
        .map(|((i, verts), leaf)| {
            let sector_id = leaf.sector_id;
            if verts.len() >= 3 {
                validate_polygon(i, sector_id, verts);
            }
            let mut wall_linedef_ids: Vec<usize> = leaf
                .polygons
                .iter()
                .filter_map(|p| match &p.surface_kind {
                    SurfaceKind::Vertical {
                        linedef_id,
                        ..
                    } => Some(*linedef_id),
                    _ => None,
                })
                .collect();
            wall_linedef_ids.sort_unstable();
            wall_linedef_ids.dedup();
            let view_polys: Vec<ViewPolygon> = leaf
                .polygons
                .iter()
                .map(|p| {
                    let kind = match &p.surface_kind {
                        SurfaceKind::Vertical {
                            wall_type,
                            linedef_id,
                            ..
                        } => {
                            format!("{:?} ld={}", wall_type, linedef_id)
                        }
                        SurfaceKind::Horizontal {
                            ..
                        } => {
                            if leaf
                                .floor_polygons
                                .iter()
                                .any(|&fi| std::ptr::eq(&leaf.polygons[fi], p))
                            {
                                "floor".into()
                            } else {
                                "ceiling".into()
                            }
                        }
                    };
                    let vertices: Vec<glam::Vec3> =
                        p.vertices.iter().map(|&vi| bsp.vertices[vi]).collect();
                    ViewPolygon {
                        kind,
                        vertices,
                    }
                })
                .collect();
            ViewSubsector {
                index: i,
                sector_id,
                vertices: verts.clone(),
                polygons: view_polys,
                aabb_min: leaf.aabb.min.truncate(),
                aabb_max: leaf.aabb.max.truncate(),
                wall_linedef_ids,
            }
        })
        .collect();

    let mut divlines = Vec::new();
    let mut ss_divline_path = vec![Vec::new(); level_data.subsectors.len()];
    collect_divline_paths(
        level_data.start_node,
        &level_data.nodes,
        &mut divlines,
        &mut Vec::new(),
        &mut ss_divline_path,
    );

    ViewerData {
        map_name: map_name.to_string(),
        min: extents.min_vertex,
        max: extents.max_vertex,
        linedefs,
        sectors,
        subsectors,
        divlines,
        vertices,
        ss_divline_path,
        floor_height_value,
        floor_texture_color,
    }
}

fn collect_divline_paths(
    node_id: u32,
    nodes: &[level::map_defs::Node],
    divlines: &mut Vec<ViewDivline>,
    path: &mut Vec<usize>,
    ss_paths: &mut [Vec<usize>],
) {
    if is_subsector(node_id) {
        let ss_id = if node_id == u32::MAX {
            0
        } else {
            subsector_index(node_id)
        };
        if ss_id < ss_paths.len() {
            ss_paths[ss_id] = path.clone();
        }
        return;
    }
    let Some(node) = nodes.get(node_id as usize) else {
        return;
    };
    let divline_idx = divlines.len();
    divlines.push(ViewDivline {
        index: node_id as usize,
        origin: Vec2::new(node.xy.x, node.xy.y),
        dir: Vec2::new(node.delta.x, node.delta.y),
    });
    path.push(divline_idx);
    collect_divline_paths(node.children[0], nodes, divlines, path, ss_paths);
    collect_divline_paths(node.children[1], nodes, divlines, path, ss_paths);
    path.pop();
}

fn validate_polygon(subsector_id: usize, sector_id: usize, verts: &[Vec2]) {
    let n = verts.len();

    for (i, v) in verts.iter().enumerate() {
        if !v.x.is_finite() || !v.y.is_finite() {
            log::warn!(
                "ss{subsector_id} (sector {sector_id}): vertex {i}/{n} non-finite ({}, {})",
                v.x,
                v.y
            );
            return;
        }
    }

    let mut area = 0.0_f32;
    for i in 0..n {
        let j = (i + 1) % n;
        area += verts[i].x * verts[j].y;
        area -= verts[j].x * verts[i].y;
    }
    area *= 0.5;

    if area.abs() < 0.5 {
        let mut dupes = Vec::new();
        for i in 0..n {
            for j in (i + 1)..n {
                if (verts[i] - verts[j]).length() < 0.01 {
                    dupes.push((i, j));
                }
            }
        }

        let seg_info: Vec<String> = verts
            .iter()
            .map(|v| format!("({:.1},{:.1})", v.x, v.y))
            .collect();
        log::warn!(
            "ss{subsector_id} (sector {sector_id}): degenerate polygon, {n} verts, area={area:.4}, \
             verts=[{}]{}",
            seg_info.join(", "),
            if dupes.is_empty() {
                String::new()
            } else {
                format!(", duplicate pairs: {:?}", dupes)
            }
        );
    }
}
