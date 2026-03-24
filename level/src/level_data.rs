use crate::map_defs::{
    BBox, Blockmap, LineDef, Node, Sector, Segment, SideDef, SlopeType, SubSector, Vertex, is_subsector, subsector_index
};

use crate::MapPtr;
use crate::bsp3d::BSP3D;
use crate::flags::LineDefFlags;
use glam::Vec2;
use log::{debug, info, warn};
use math::{Angle, FixedT};
use std::time::Instant;
use rbsp::LineDefAccess;

const CELL_SIZE: f32 = 128.0;
use wad::types::*;
use wad::{MapLump, WadData};

/// The smallest vector and the largest vertex, combined make up a
/// rectangle enclosing the level area
#[derive(Default)]
pub struct MapExtents {
    pub min_vertex: Vec2,
    pub max_vertex: Vec2,
    pub width: f32,
    pub height: f32,
    pub automap_scale: f32,
    pub min_floor: f32,
    pub max_ceiling: f32,
}

/// A `Map` contains everything required for building the actual level the
/// player will see in-game-exe, such as the data to build a level, the textures
/// used, `Things`, `Sounds` and others.
///
/// `nodes`, `subsectors`, and `segments` are what get used most to render the
/// basic level
///
/// Access to the `Vec` arrays within is limited to immutable only to
/// prevent unwanted removal of items, which *will* break references and
/// segfault
#[derive(Default)]
pub struct LevelData {
    /// Things will be linked to/from each other in many ways, which means this
    /// array may never be resized or it will invalidate references and
    /// pointers
    things: Vec<WadThing>,
    pub vertexes: Vec<Vertex>,
    pub linedefs: Vec<LineDef>,
    pub sectors: Vec<Sector>,
    sidedefs: Vec<SideDef>,
    pub subsectors: Vec<SubSector>,
    pub segments: Vec<Segment>,
    blockmap: Blockmap,
    reject: Vec<u8>,
    extents: MapExtents,
    pub nodes: Vec<Node>,
    pub start_node: u32,
    pub bsp_3d: BSP3D,
}

impl LevelData {
    pub fn set_extents(&mut self) {
        // set the min/max to first vertex so we have a baseline
        // that isn't 0 causing comparison issues, eg; if it's 0,
        // then a min vertex of -3542 won't be set since it's negative
        let mut check = |v: Vec2| {
            if self.extents.min_vertex.x > v.x {
                self.extents.min_vertex.x = v.x;
            } else if self.extents.max_vertex.x < v.x {
                self.extents.max_vertex.x = v.x;
            }

            if self.extents.min_vertex.y > v.y {
                self.extents.min_vertex.y = v.y;
            } else if self.extents.max_vertex.y < v.y {
                self.extents.max_vertex.y = v.y;
            }
        };

        for line in &self.linedefs {
            check(line.v1.pos);
            check(line.v2.pos);
        }
        self.extents.width = self.extents.max_vertex.x - self.extents.min_vertex.x;
        self.extents.height = self.extents.max_vertex.y - self.extents.min_vertex.y;

        let mut min = self.sectors()[0].floorheight;
        let mut max = self.sectors()[0].ceilingheight;
        for sector in self.sectors() {
            if sector.floorheight < min {
                min = sector.floorheight;
            }
            if sector.ceilingheight > max {
                max = sector.ceilingheight;
            }
        }
        self.extents.min_floor = min.to_f32();
        self.extents.max_ceiling = max.to_f32();
    }

    pub fn bsp_3d(&self) -> &BSP3D {
        &self.bsp_3d
    }

    pub fn bsp_3d_mut(&mut self) -> &mut BSP3D {
        &mut self.bsp_3d
    }

    pub fn things(&self) -> &[WadThing] {
        &self.things
    }

    pub fn linedefs(&self) -> &[LineDef] {
        &self.linedefs
    }

    pub fn sectors(&self) -> &[Sector] {
        &self.sectors
    }

    pub fn sectors_mut(&mut self) -> &mut [Sector] {
        &mut self.sectors
    }

    /// Apply interpolated sector heights to BSP3D vertices for smooth
    /// rendering. Must call `restore_render_interpolation()` after
    /// rendering.
    pub fn apply_render_interpolation(&mut self, frac: f32) {
        self.bsp_3d
            .apply_interpolated_heights(&mut self.sectors, frac);
    }

    /// Restore true post-tic sector values and vertex Z after rendering.
    pub fn restore_render_interpolation(&mut self) {
        self.bsp_3d.restore_sector_state(&mut self.sectors);
    }

    pub fn sidedefs(&self) -> &[SideDef] {
        &self.sidedefs
    }

    pub fn sidedefs_mut(&mut self) -> &mut [SideDef] {
        &mut self.sidedefs
    }

    pub fn linedefs_mut(&mut self) -> &mut [LineDef] {
        &mut self.linedefs
    }

    pub fn subsectors(&self) -> &[SubSector] {
        &self.subsectors
    }

    pub fn subsectors_mut(&mut self) -> &mut [SubSector] {
        &mut self.subsectors
    }

    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    pub fn segments_mut(&mut self) -> &mut [Segment] {
        &mut self.segments
    }

    const fn set_scale(&mut self) {
        let map_width = self.extents.width;
        let map_height = self.extents.height;

        if map_height > map_width {
            self.extents.automap_scale = map_height / 400.0 * 1.1;
        } else {
            self.extents.automap_scale = map_width / 640.0 * 1.4;
        }
    }

    pub fn get_nodes(&self) -> &[Node] {
        &self.nodes
    }

    pub const fn start_node(&self) -> u32 {
        self.start_node
    }

    pub fn blockmap(&self) -> &Blockmap {
        &self.blockmap
    }

    pub fn get_map_extents(&self) -> &MapExtents {
        &self.extents
    }

    pub fn get_devils_rejects(&self) -> &[u8] {
        &self.reject
    }

    /// The level struct *must not move after this*
    pub fn load(
        &mut self,
        map_name: &str,
        flat_num_for_name: impl Fn(&str) -> Option<usize>,
        wad: &WadData,
        sky_num: Option<usize>,
        sky_pic: Option<usize>,
    ) {
        let mut tex_order: Vec<WadTexture> = wad.texture_iter("TEXTURE1").collect();
        if wad.lump_exists("TEXTURE2") {
            let mut pnames2: Vec<WadTexture> = wad.texture_iter("TEXTURE2").collect();
            tex_order.append(&mut pnames2);
        }

        self.things = wad
            .map_iter::<WadThing>(map_name, MapLump::Things)
            .collect();
        info!("{}: Loaded {} things", map_name, self.things.len());

        // Sectors and sidedefs from WAD (unchanged by BSP builder)
        self.load_sectors(map_name, wad, &flat_num_for_name);
        self.load_sidedefs(map_name, wad, &tex_order);

        // Try to load pre-built RBSP lump; fall back to building from scratch.
        let bsp = if let Some(rbsp_data) = find_map_lump(wad, map_name, "RBSP") {
            if let Some(bsp) = rbsp::rbsp_lump::read_rbsp_lump(&rbsp_data) {
                info!(
                    "{}: Loaded RBSP lump: {} verts, {} segs, {} ssectors, {} nodes",
                    map_name,
                    bsp.vertices.len(),
                    bsp.segs.len(),
                    bsp.subsectors.len(),
                    bsp.nodes.len(),
                );
                bsp
            } else {
                info!("{}: RBSP lump invalid, rebuilding", map_name);
                self.build_bsp(map_name, wad)
            }
        } else {
            self.build_bsp(map_name, wad)
        };

        let wad_linedefs: Vec<WadLineDef> = wad
            .map_iter::<WadLineDef>(map_name, MapLump::LineDefs)
            .collect();

        // --- Vertices: direct from rbsp, exact capacity (MapPtr stability) ---
        self.vertexes = Vec::with_capacity(bsp.vertices.len());
        for hv in &bsp.vertices {
            self.vertexes.push(Vertex::new(
                hv.x as f32,
                hv.y as f32,
                FixedT::from_fixed((hv.x * 65536.0).round() as i32),
                FixedT::from_fixed((hv.y * 65536.0).round() as i32),
            ));
        }
        info!("{}: Loaded {} vertexes", map_name, self.vertexes.len());

        // --- LineDefs: from WAD, with rbsp vertex remap ---
        self.linedefs = wad_linedefs
            .iter()
            .enumerate()
            .map(|(num, l)| {
                let v1_idx = l.start_vertex_idx();
                let v2_idx = l.end_vertex_idx();
                let v1 = MapPtr::new(&mut self.vertexes[v1_idx]);
                let v2 = MapPtr::new(&mut self.vertexes[v2_idx]);

                let front_sd_idx = l.front_sidedef_idx().expect("linedef has no front sidedef");
                let back_sd_idx = l.back_sidedef_idx();

                let front = MapPtr::new(&mut self.sidedefs[front_sd_idx]);
                let back_side = back_sd_idx.map(|i| MapPtr::new(&mut self.sidedefs[i]));
                let back_sector = back_sd_idx.map(|i| self.sidedefs[i].sector.clone());

                let dx = v2.x - v1.x;
                let dy = v2.y - v1.y;
                let x1 = v1.x_fp.to_fixed_raw();
                let y1 = v1.y_fp.to_fixed_raw();
                let x2 = v2.x_fp.to_fixed_raw();
                let y2 = v2.y_fp.to_fixed_raw();
                let delta_fp = [x2.wrapping_sub(x1), y2.wrapping_sub(y1)];

                let slope = if delta_fp[0] == 0 {
                    SlopeType::Vertical
                } else if delta_fp[1] == 0 {
                    SlopeType::Horizontal
                } else if (delta_fp[1] ^ delta_fp[0]) >= 0 {
                    SlopeType::Positive
                } else {
                    SlopeType::Negative
                };

                // Derive sides array from sidedef indices
                let sides = [
                    front_sd_idx as u16,
                    back_sd_idx.map_or(u16::MAX, |i| i as u16),
                ];

                LineDef {
                    num,
                    v1: v1.clone(),
                    v2: v2.clone(),
                    delta: Vec2::new(dx, dy),
                    delta_fp,
                    flags: LineDefFlags::from_bits_truncate(l.flags as u32),
                    special: l.special,
                    tag: l.sector_tag,
                    default_special: l.special,
                    default_tag: l.sector_tag,
                    bbox: BBox::new(v1.pos, v2.pos),
                    bbox_int: [y1.max(y2), y1.min(y2), x1.min(x2), x1.max(x2)],
                    slopetype: slope,
                    front_sidedef: front.clone(),
                    back_sidedef: back_side,
                    frontsector: front.sector.clone(),
                    backsector: back_sector,
                    valid_count: 0,
                    sides,
                }
            })
            .collect();
        info!("{}: Loaded {} linedefs", map_name, self.linedefs.len());

        // Map sectors to lines
        for line in self.linedefs.iter_mut() {
            let mut sector = line.frontsector.clone();
            sector.lines.push(MapPtr::new(line));
            if let Some(mut sector) = line.backsector.clone() {
                sector.lines.push(MapPtr::new(line));
            }
        }

        // --- Segments + SubSectors: compacted, built together ---
        // Walk rbsp subsectors, collect segs contiguously per subsector.
        {
            let mut segments = Vec::new();
            let mut subsectors = Vec::new();

            for ss in &bsp.subsectors {
                let start = segments.len() as u32;

                // Use seg_indices directly — NOT polygon edges.
                // Polygon edges may miss segs that didn't match (interior segs).
                // All segs must be present for wall rendering.
                for &seg_idx in &ss.seg_indices {
                    let rseg = &bsp.segs[seg_idx as usize];
                    let v1 = MapPtr::new(&mut self.vertexes[rseg.start]);
                    let v2 = MapPtr::new(&mut self.vertexes[rseg.end]);
                    let linedef = MapPtr::new(&mut self.linedefs[rseg.linedef]);
                    let side_idx = match rseg.side {
                        rbsp::Side::Front => 0usize,
                        rbsp::Side::Back => 1usize,
                    };
                    let sidedef_num = linedef.sides[side_idx] as usize;
                    let sidedef = MapPtr::new(&mut self.sidedefs[sidedef_num]);
                    let frontsector = sidedef.sector.clone();

                    let backsector = if linedef.flags.contains(LineDefFlags::TwoSided) {
                        let back_num = linedef.sides[side_idx ^ 1] as usize;
                        if back_num < self.sidedefs.len() && back_num != u16::MAX as usize {
                            Some(self.sidedefs[back_num].sector.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    let offset = FixedT::from_fixed((rseg.offset * 65536.0).round() as i32);
                    let angle = Angle::new(rseg.angle as f32);

                    segments.push(Segment {
                        v1,
                        v2,
                        offset,
                        angle,
                        sidedef,
                        linedef,
                        frontsector,
                        backsector,
                    });
                }

                let count = segments.len() as u32 - start;
                let sector = if count > 0 {
                    segments[start as usize].sidedef.sector.clone()
                } else {
                    // Seg-less subsector: use rbsp's sector assignment
                    MapPtr::new(&mut self.sectors[ss.sector as usize])
                };
                subsectors.push(SubSector {
                    sector,
                    seg_count: count,
                    start_seg: start,
                });
            }

            self.segments = segments;
            self.subsectors = subsectors;
        }
        info!(
            "{}: Loaded {} segments, {} subsectors",
            map_name,
            self.segments.len(),
            self.subsectors.len(),
        );

        // --- Nodes: direct from rbsp ---
        self.nodes = bsp
            .nodes
            .iter()
            .map(|n| Node {
                xy: Vec2::new(n.x as f32, n.y as f32),
                delta: Vec2::new(n.dx as f32, n.dy as f32),
                bboxes: [
                    [
                        Vec2::new(n.bbox_right.min_x as f32, n.bbox_right.max_y as f32),
                        Vec2::new(n.bbox_right.max_x as f32, n.bbox_right.min_y as f32),
                    ],
                    [
                        Vec2::new(n.bbox_left.min_x as f32, n.bbox_left.max_y as f32),
                        Vec2::new(n.bbox_left.max_x as f32, n.bbox_left.min_y as f32),
                    ],
                ],
                children: [n.child_right, n.child_left],
            })
            .collect();
        self.start_node = if self.nodes.is_empty() {
            0
        } else {
            (self.nodes.len() - 1) as u32
        };
        info!(
            "{}: Loaded {} nodes, start_node={}",
            map_name,
            self.nodes.len(),
            self.start_node,
        );

        // --- Polygons for BSP3D: from rbsp poly_indices ---
        let carved_polygons: Vec<Vec<Vec2>> = bsp
            .subsectors
            .iter()
            .map(|ss| {
                let start = ss.polygon.first_vertex as usize;
                let count = ss.polygon.num_vertices as usize;
                bsp.poly_indices[start..start + count]
                    .iter()
                    .map(|&vi| {
                        let v = &bsp.vertices[vi as usize];
                        Vec2::new(v.x as f32, v.y as f32)
                    })
                    .collect()
            })
            .collect();

        // --- Finalize ---
        let t = Instant::now();
        self.build_blockmap(map_name);
        self.compute_sector_blockboxes();
        self.reject = vec![];

        for sector in &mut self.sectors {
            set_sector_sound_origin(sector);
        }

        self.set_extents();
        self.set_scale();
        log::info!(
            "{}: Blockmap + finalize [{:.2}s]",
            map_name,
            t.elapsed().as_secs_f64()
        );

        let t = Instant::now();
        self.bsp_3d = BSP3D::new(
            self.start_node,
            &self.nodes,
            &self.subsectors,
            &self.segments,
            &self.sectors,
            &self.linedefs,
            carved_polygons,
            sky_num,
            sky_pic,
        );
        log::info!(
            "{}: BSP3D built [{:.2}s]",
            map_name,
            t.elapsed().as_secs_f64()
        );
    }

    fn build_bsp(&self, map_name: &str, wad: &WadData) -> rbsp::BspOutput {
        let wad_vertices: Vec<WadVertex> = wad
            .map_iter::<WadVertex>(map_name, MapLump::Vertexes)
            .collect();
        let wad_linedefs: Vec<WadLineDef> = wad
            .map_iter::<WadLineDef>(map_name, MapLump::LineDefs)
            .collect();
        let wad_sidedefs: Vec<WadSideDef> = wad
            .map_iter::<WadSideDef>(map_name, MapLump::SideDefs)
            .collect();
        let wad_sectors: Vec<WadSector> = wad
            .map_iter::<WadSector>(map_name, MapLump::Sectors)
            .collect();

        let bsp = rbsp::build_bsp(
            rbsp::BspInput {
                vertices: wad_vertices,
                linedefs: wad_linedefs,
                sidedefs: wad_sidedefs,
                sectors: wad_sectors,
            },
            &rbsp::BspOptions::default(),
        );
        info!(
            "{}: BSP built: {} verts, {} segs, {} ssectors, {} nodes",
            map_name,
            bsp.vertices.len(),
            bsp.segs.len(),
            bsp.subsectors.len(),
            bsp.nodes.len(),
        );
        bsp
    }

    fn load_sectors(
        &mut self,
        map_name: &str,
        wad: &WadData,
        flat_num_for_name: &impl Fn(&str) -> Option<usize>,
    ) {
        self.sectors = wad
            .map_iter::<WadSector>(map_name, MapLump::Sectors)
            .enumerate()
            .map(|(i, s)| {
                Sector::new(
                    i as u32,
                    FixedT::from(s.floor_height as i32),
                    FixedT::from(s.ceil_height as i32),
                    flat_num_for_name(&s.floor_tex).unwrap_or_else(|| {
                        debug!("Sectors: Did not find flat for {}", s.floor_tex);
                        1
                    }),
                    flat_num_for_name(&s.ceil_tex).unwrap_or_else(|| {
                        debug!("Sectors: Did not find flat for {}", s.ceil_tex);
                        1
                    }),
                    s.light_level as usize,
                    s.kind,
                    s.tag,
                )
            })
            .collect();
        info!("{}: Loaded {} sectors", map_name, self.sectors.len());
    }

    fn load_sidedefs(&mut self, map_name: &str, wad: &WadData, tex_order: &[WadTexture]) {
        if self.sectors.is_empty() {
            panic!("sectors must be loaded before sidedefs");
        }
        // dbg!(tex_order.iter().position(|n| n.name == "METAL"));
        self.sidedefs = wad
            .map_iter::<WadSideDef>(map_name, MapLump::SideDefs)
            .map(|s| {
                let sector = &mut self.sectors[s.sector as usize];
                SideDef {
                    textureoffset: FixedT::from(s.x_offset as i32),
                    rowoffset: FixedT::from(s.y_offset as i32),
                    toptexture: tex_order
                        .iter()
                        .position(|n| n.name == s.upper_tex.to_ascii_uppercase()),
                    bottomtexture: tex_order
                        .iter()
                        .position(|n| n.name == s.lower_tex.to_ascii_uppercase()),
                    midtexture: tex_order
                        .iter()
                        .position(|n| n.name == s.middle_tex.to_ascii_uppercase()),
                    sector: MapPtr::new(sector),
                }
            })
            .collect();
        info!("{}: Loaded {} sidedefs", map_name, self.sidedefs.len());
    }

    pub fn build_blockmap(&mut self, map_name: &str) {
        if self.linedefs.is_empty() {
            warn!("{}: No linedefs, cannot build blockmap", map_name);
            return;
        }

        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;
        for ld in &self.linedefs {
            for v in [&ld.v1, &ld.v2] {
                min_x = min_x.min(v.x);
                min_y = min_y.min(v.y);
                max_x = max_x.max(v.x);
                max_y = max_y.max(v.y);
            }
        }

        let org_x = min_x.floor();
        let org_y = min_y.floor();
        let cols = ((max_x - org_x) / CELL_SIZE) as i32 + 1;
        let rows = ((max_y - org_y) / CELL_SIZE) as i32 + 1;
        let num_blocks = (cols * rows) as usize;

        let mut cell_lines: Vec<Vec<usize>> = vec![Vec::new(); num_blocks];

        for (li, ld) in self.linedefs.iter().enumerate() {
            let x1 = (ld.v1.x - org_x) as f64;
            let y1 = (ld.v1.y - org_y) as f64;
            let x2 = (ld.v2.x - org_x) as f64;
            let y2 = (ld.v2.y - org_y) as f64;
            let cs = CELL_SIZE as f64;

            let mut cx = (x1 / cs) as i32;
            let mut cy = (y1 / cs) as i32;
            let cx2 = (x2 / cs) as i32;
            let cy2 = (y2 / cs) as i32;

            // Clamp to grid bounds
            cx = cx.clamp(0, cols - 1);
            cy = cy.clamp(0, rows - 1);
            let cx2 = cx2.clamp(0, cols - 1);
            let cy2 = cy2.clamp(0, rows - 1);

            if cx == cx2 && cy == cy2 {
                cell_lines[(cy * cols + cx) as usize].push(li);
                continue;
            }

            let dx = x2 - x1;
            let dy = y2 - y1;
            let step_x: i32 = if dx > 0.0 {
                1
            } else if dx < 0.0 {
                -1
            } else {
                0
            };
            let step_y: i32 = if dy > 0.0 {
                1
            } else if dy < 0.0 {
                -1
            } else {
                0
            };

            let t_dx = if dx != 0.0 { cs / dx.abs() } else { f64::MAX };
            let t_dy = if dy != 0.0 { cs / dy.abs() } else { f64::MAX };

            let mut t_max_x = if dx > 0.0 {
                ((cx + 1) as f64 * cs - x1) / dx
            } else if dx < 0.0 {
                (cx as f64 * cs - x1) / dx
            } else {
                f64::MAX
            };
            let mut t_max_y = if dy > 0.0 {
                ((cy + 1) as f64 * cs - y1) / dy
            } else if dy < 0.0 {
                (cy as f64 * cs - y1) / dy
            } else {
                f64::MAX
            };

            let max_steps = (cx - cx2).unsigned_abs() + (cy - cy2).unsigned_abs() + 2;
            for _ in 0..max_steps {
                if cx >= 0 && cx < cols && cy >= 0 && cy < rows {
                    cell_lines[(cy * cols + cx) as usize].push(li);
                }
                if cx == cx2 && cy == cy2 {
                    break;
                }
                if t_max_x < t_max_y {
                    cx += step_x;
                    t_max_x += t_dx;
                } else {
                    cy += step_y;
                    t_max_y += t_dy;
                }
            }
        }

        let mut block_offsets = Vec::with_capacity(num_blocks + 1);
        let mut block_lines = Vec::new();

        for cell in &cell_lines {
            block_offsets.push(block_lines.len());
            for &li in cell {
                block_lines.push(MapPtr::new(&mut self.linedefs[li]));
            }
        }
        block_offsets.push(block_lines.len());

        info!(
            "{}: Built blockmap {}x{} ({} blocks, {} line refs)",
            map_name,
            cols,
            rows,
            num_blocks,
            block_lines.len()
        );

        self.blockmap = Blockmap {
            x_origin: (org_x as i32) << 16,
            y_origin: (org_y as i32) << 16,
            columns: cols,
            rows,
            block_offsets,
            block_lines,
        };
    }

    /// Compute blockmap bounding box for each sector from its linedef vertices.
    /// Matches OG Doom's P_GroupLines blockbox computation.
    fn compute_sector_blockboxes(&mut self) {
        let bm = &self.blockmap;
        let orgx = bm.x_origin;
        let orgy = bm.y_origin;
        let bmw = bm.columns;
        let bmh = bm.rows;
        // MAXRADIUS in 16.16 fixed-point = 32 << 16
        let maxradius_fixed: i32 = 32 << 16;

        for sector in self.sectors.iter_mut() {
            if sector.lines.is_empty() {
                continue;
            }
            let mut bbox_top = i32::MIN;
            let mut bbox_bottom = i32::MAX;
            let mut bbox_right = i32::MIN;
            let mut bbox_left = i32::MAX;

            for line in sector.lines.iter() {
                let v1x = line.v1.x_fp.to_fixed_raw();
                let v1y = line.v1.y_fp.to_fixed_raw();
                let v2x = line.v2.x_fp.to_fixed_raw();
                let v2y = line.v2.y_fp.to_fixed_raw();
                bbox_left = bbox_left.min(v1x).min(v2x);
                bbox_right = bbox_right.max(v1x).max(v2x);
                bbox_bottom = bbox_bottom.min(v1y).min(v2y);
                bbox_top = bbox_top.max(v1y).max(v2y);
            }

            // OG: BOXTOP=0, BOXBOTTOM=1, BOXLEFT=2, BOXRIGHT=3
            let mut block = (bbox_top - orgy + maxradius_fixed) >> 23;
            if block >= bmh {
                block = bmh - 1;
            }
            sector.blockbox[0] = block; // BOXTOP

            block = (bbox_bottom - orgy - maxradius_fixed) >> 23;
            if block < 0 {
                block = 0;
            }
            sector.blockbox[1] = block; // BOXBOTTOM

            block = (bbox_left - orgx - maxradius_fixed) >> 23;
            if block < 0 {
                block = 0;
            }
            sector.blockbox[2] = block; // BOXLEFT

            block = (bbox_right - orgx + maxradius_fixed) >> 23;
            if block >= bmw {
                block = bmw - 1;
            }
            sector.blockbox[3] = block; // BOXRIGHT
        }
    }

    /// OG Doom `R_PointInSubsector` — find which subsector a point is in.
    pub fn point_in_subsector(&mut self, x: FixedT, y: FixedT) -> MapPtr<SubSector> {
        let mut node_id = self.start_node();

        while !is_subsector(node_id) {
            let node = &self.get_nodes()[node_id as usize];
            (node_id, _) = node.front_back_children_fixed(x, y);
        }

        MapPtr::new(&mut self.subsectors[subsector_index(node_id)])
    }
}

/// Find raw lump data for a named lump within a map.
fn find_map_lump(wad: &WadData, map_name: &str, lump_name: &str) -> Option<Vec<u8>> {
    let lumps = wad.lumps();
    let marker_idx = lumps.iter().rposition(|l| l.name == map_name)?;
    for lump in &lumps[marker_idx + 1..] {
        if lump.name == lump_name {
            return Some(lump.data.clone());
        }
        // Stop at next map marker.
        if lump.data.is_empty() && lump.name.len() >= 4 {
            let b = lump.name.as_bytes();
            if (b[0] == b'E' && b[2] == b'M') || (b[0] == b'M' && b[1] == b'A' && b[2] == b'P') {
                break;
            }
        }
    }
    None
}

pub fn set_sector_sound_origin(sector: &mut Sector) {
    let mut minx = sector.lines[0].v1.x;
    let mut miny = sector.lines[0].v1.y;
    let mut maxx = sector.lines[0].v2.x;
    let mut maxy = sector.lines[0].v2.y;

    let mut check = |v: Vec2| {
        if minx > v.x {
            minx = v.x;
        } else if maxx < v.x {
            maxx = v.x;
        }

        if miny > v.y {
            miny = v.y;
        } else if maxy < v.y {
            maxy = v.y;
        }
    };

    for line in sector.lines.iter() {
        check(line.v1.pos);
        check(line.v2.pos);
    }
    sector.sound_origin = Vec2::new(minx + ((maxx - minx) / 2.0), miny + ((maxy - miny) / 2.0));
}
