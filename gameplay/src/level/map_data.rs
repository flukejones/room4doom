use std::f32::consts::FRAC_PI_2;
use std::time::Instant;

use crate::level::map_defs::{BBox, LineDef, Node, Sector, Segment, SideDef, SlopeType, SubSector};

use crate::level::bsp3d::BSP3D;
use crate::log::info;
use crate::{LineDefFlags, MapPtr, PicData};
use glam::Vec2;
#[cfg(Debug)]
use log::error;
use log::{debug, warn};
use math::{Angle, bam_to_radian, circle_line_collide, fixed_to_float};
use wad::WadData;
use wad::extended::{ExtendedNodeType, NodeLumpType, WadExtendedMap};
use wad::types::*;

use super::map_defs::Blockmap;

const IS_OLD_SSECTOR_MASK: u32 = 0x8000;
pub const IS_SSECTOR_MASK: u32 = 0x80000000;

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
pub struct MapData {
    /// Things will be linked to/from each other in many ways, which means this
    /// array may never be resized or it will invalidate references and
    /// pointers
    things: Vec<WadThing>,
    pub vertexes: Vec<Vec2>,
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
    /// Precomputed visibility between subsectors
    pub bsp_3d: BSP3D,
}

impl MapData {
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
            check(line.v1);
            check(line.v2);
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
        self.extents.min_floor = min;
        self.extents.max_ceiling = max;
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

    pub fn sidedefs(&self) -> &[SideDef] {
        &self.sidedefs
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

    pub fn get_map_extents(&self) -> &MapExtents {
        &self.extents
    }

    pub fn get_devils_rejects(&self) -> &[u8] {
        &self.reject
    }

    // TODO: pass in TextureData
    // None of this is efficient as it iterates over wad data many multiples of
    // times
    /// The level struct *must not move after this*
    pub fn load(&mut self, map_name: &str, pic_data: &PicData, wad: &WadData) {
        let mut tex_order: Vec<WadTexture> = wad.texture_iter("TEXTURE1").collect();
        if wad.lump_exists("TEXTURE2") {
            let mut pnames2: Vec<WadTexture> = wad.texture_iter("TEXTURE2").collect();
            tex_order.append(&mut pnames2);
        }

        self.things = wad.thing_iter(map_name).collect();
        info!("{}: Loaded {} things", map_name, self.things.len());

        // We may need to append ZDoom vertices to the vertexes, so check and lod now
        let node_type = wad.node_lump_type(map_name);
        let extended = match node_type {
            NodeLumpType::OGDoom => None,
            NodeLumpType::Extended(ExtendedNodeType::XNOD) => WadExtendedMap::parse(wad, map_name),
            _ => panic!("Unsupported zddom node type (yet)"),
        };
        // The overall level information. You can rebuild a BSP from this.
        // A lot of what happens here is using the wad data to fill in
        // structures, and then creating (unsafe) internal pointers to everything
        self.vertexes = self.load_vertexes(map_name, wad, extended.as_ref());
        self.load_sectors(map_name, wad, pic_data);
        self.load_sidedefs(map_name, wad, &tex_order);
        self.load_linedefs(map_name, wad);
        Self::prepass_fix_vertices(
            map_name,
            wad,
            &mut self.vertexes,
            &self.linedefs,
            extended.as_ref(),
        );
        self.load_blockmap(map_name, wad);
        self.load_devils_rejects(map_name, wad);
        // TODO: iterate sector lines to find max bounding box for sector

        // The BSP level structure for rendering, movement, collisions etc
        self.load_segments(map_name, wad, extended.as_ref());
        self.load_subsectors(map_name, wad, extended.as_ref());
        // Should always be last to ensure we can access subsectors and sectors during
        // it
        self.load_nodes(map_name, wad, node_type, extended.as_ref());

        for sector in &mut self.sectors {
            set_sector_sound_origin(sector);
        }

        self.set_extents();
        self.set_scale();

        // Build 3D BSP
        self.bsp_3d = BSP3D::new(
            map_name,
            self.start_node,
            &self.nodes,
            &self.subsectors,
            &self.segments,
            &self.sectors,
            &self.linedefs,
            wad,
            pic_data,
        );
    }

    fn load_vertexes(
        &mut self,
        map_name: &str,
        wad: &WadData,
        extended: Option<&WadExtendedMap>,
    ) -> Vec<Vec2> {
        let mut vertexes: Vec<Vec2> = wad
            .vertex_iter(map_name)
            .map(|v| Vec2::new(v.x, v.y))
            .collect();
        info!("{}: Loaded {} vertexes", map_name, vertexes.len());

        if let Some(ext) = extended.as_ref() {
            vertexes.reserve(ext.vertexes.len());
            for v in ext.vertexes.iter() {
                vertexes.push(Vec2::new(v.x, v.y));
            }
            info!("{}: Loaded {} zdoom vertexes", map_name, ext.vertexes.len());
        }

        vertexes
    }

    fn load_sectors(&mut self, map_name: &str, wad: &WadData, pic_data: &PicData) {
        self.sectors = wad
            .sector_iter(map_name)
            .enumerate()
            .map(|(i, s)| {
                Sector::new(
                    i as u32,
                    s.floor_height as f32,
                    s.ceil_height as f32,
                    pic_data.flat_num_for_name(&s.floor_tex).unwrap_or_else(|| {
                        debug!("Sectors: Did not find flat for {}", s.floor_tex);
                        // usize::MAX
                        1
                    }),
                    pic_data.flat_num_for_name(&s.ceil_tex).unwrap_or_else(|| {
                        debug!("Sectors: Did not find flat for {}", s.ceil_tex);
                        // usize::MAX
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
            .sidedef_iter(map_name)
            .map(|s| {
                let sector = &mut self.sectors[s.sector as usize];
                SideDef {
                    textureoffset: s.x_offset as f32,
                    rowoffset: s.y_offset as f32,
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

    fn load_linedefs(&mut self, map_name: &str, wad: &WadData) {
        if self.vertexes.is_empty() {
            panic!("Vertexes must be loaded before linedefs");
        }
        if self.sidedefs.is_empty() {
            panic!("sidedefs must be loaded before linedefs");
        }
        self.linedefs = wad
            .linedef_iter(map_name)
            .enumerate()
            .map(|(num, l)| {
                let v1 = self.vertexes[l.start_vertex as usize];
                let v2 = self.vertexes[l.end_vertex as usize];

                let front = MapPtr::new(&mut self.sidedefs[l.front_sidedef as usize]);
                let back_side = {
                    if l.back_sidedef == Some(u16::MAX) {
                        None
                    } else {
                        l.back_sidedef
                            .map(|index| MapPtr::new(&mut self.sidedefs[index as usize]))
                    }
                };

                let back_sector = {
                    if l.back_sidedef == Some(u16::MAX) {
                        None
                    } else {
                        l.back_sidedef
                            .map(|index| self.sidedefs()[index as usize].sector.clone())
                    }
                };

                let dx = v2.x - v1.x;
                let dy = v2.y - v1.y;

                let slope = if dx == 0.0 {
                    SlopeType::Vertical
                } else if dy == 0.0 {
                    SlopeType::Horizontal
                } else if dy / dx > 0.0 {
                    SlopeType::Positive
                } else {
                    SlopeType::Negative
                };

                LineDef {
                    num,
                    v1,
                    v2,
                    delta: Vec2::new(dx, dy),
                    flags: l.flags as u32,
                    special: l.special,
                    tag: l.sector_tag,
                    default_special: l.special,
                    default_tag: l.sector_tag,
                    bbox: BBox::new(v1, v2),
                    slopetype: slope,
                    front_sidedef: front.clone(),
                    back_sidedef: back_side,
                    frontsector: front.sector.clone(),
                    backsector: back_sector,
                    valid_count: 0,
                    sides: l.sides,
                }
            })
            .collect();
        info!("{}: Loaded {} linedefs", map_name, self.linedefs.len());
        // Now map sectors to lines
        for line in self.linedefs.iter_mut() {
            let mut sector = line.frontsector.clone();
            sector.lines.push(MapPtr::new(line));
            if let Some(mut sector) = line.backsector.clone() {
                sector.lines.push(MapPtr::new(line));
            }
        }
        info!(
            "{}: Mapped linedefs to {} sectors",
            map_name,
            self.sectors.len()
        );
    }

    // TODO: Verified
    fn load_segments(&mut self, map_name: &str, wad: &WadData, extended: Option<&WadExtendedMap>) {
        if self.vertexes.is_empty() {
            panic!("Vertexes must be loaded before segs");
        }
        let mut parse_segs = |ms: WadSegment| {
            if ms.side as usize >= self.sidedefs.len() {
                panic!("Invalid side num on segment");
            }

            let v1 = MapPtr::new(&mut self.vertexes[ms.start_vertex as usize]);
            let v2 = MapPtr::new(&mut self.vertexes[ms.end_vertex as usize]);
            let linedef = MapPtr::new(&mut self.linedefs[ms.linedef as usize]);

            let angle = if extended.is_none() {
                Angle::new(bam_to_radian((ms.angle as u32) << 16))
            } else {
                let dx = v2.x - v1.x;
                let dy = v2.y - v1.y;
                Angle::new(Vec2::new(dx, dy).to_angle())
            };

            let offset = if ms.offset == i16::MIN {
                let v2 = if ms.side == 1 { linedef.v2 } else { linedef.v1 };
                Segment::recalc_offset(&v1, &v2)
            } else {
                ms.offset as f32
            };
            let sidedef = MapPtr::new(&mut self.sidedefs[linedef.sides[ms.side as usize] as usize]);
            let frontsector = sidedef.sector.clone();

            let mut backsector = None;
            if linedef.flags & LineDefFlags::TwoSided as u32 != 0 {
                let sidenum = linedef.sides[ms.side as usize ^ 1] as usize;
                if sidenum == u16::MAX as usize || sidenum >= self.sidedefs().len() {
                    if sidedef.midtexture.is_some() {
                        backsector = None;
                        warn!("Two-sided line with midtexture: removed back sector")
                    }
                } else {
                    backsector = Some(self.sidedefs[sidenum].sector.clone());
                }
            }

            Segment {
                v1,
                v2,
                angle,
                offset,
                sidedef,
                linedef,
                frontsector,
                backsector,
            }
        };

        if let Some(ext) = extended.as_ref() {
            self.segments = ext.segments.iter().map(|s| parse_segs(s.clone())).collect();
        } else {
            self.segments = wad.segment_iter(map_name).map(parse_segs).collect();
        }
        info!("{}: Generated {} segments", map_name, self.segments.len());
    }

    fn load_subsectors(
        &mut self,
        map_name: &str,
        wad: &WadData,
        extended: Option<&WadExtendedMap>,
    ) {
        if self.segments.is_empty() {
            panic!("segments must be loaded before subsectors");
        }
        let parse_subs = |s: WadSubSector| {
            let sector = self.segments[s.start_seg as usize].sidedef.sector.clone();
            SubSector {
                sector,
                seg_count: s.seg_count,
                start_seg: s.start_seg,
            }
        };
        if let Some(ext) = extended.as_ref() {
            self.subsectors = ext
                .subsectors
                .iter()
                .map(|s| parse_subs(s.clone()))
                .collect();
        } else {
            self.subsectors = wad.subsector_iter(map_name).map(parse_subs).collect();
        }
        // iter through subsectors and check the lines have front/back sectors matching?
        // for ss in self.subsectors.iter() {
        //     for i in ss.start_seg..ss.start_seg + ss.seg_count {
        //         let seg = &mut self.segments[i as usize];
        //         if seg.frontsector.num != ss.sector.num {
        //             // sub.frontsector = ss.sector.clone();
        //             if let Some(back) = seg.backsector.take() {
        //                 let tmp = seg.frontsector.clone();
        //                 seg.frontsector = back;
        //                 seg.backsector = Some(tmp);
        //             }
        //         }
        //     }
        // }
        info!("{}: Loaded {} subsectors", map_name, self.subsectors.len());
    }

    fn load_blockmap(&mut self, map_name: &str, wad: &WadData) {
        if let Some(wadblock) = wad.read_blockmap(map_name) {
            let mut blockmap = Blockmap {
                x_origin: fixed_to_float(wadblock.x_origin as i32),
                y_origin: fixed_to_float(wadblock.y_origin as i32),
                columns: wadblock.columns as usize,
                rows: wadblock.rows as usize,
                lines: Vec::with_capacity(wadblock.line_indexes.len()),
            };

            for l in wadblock.line_indexes {
                if l > 0 {
                    let linedef = MapPtr::new(&mut self.linedefs[l as usize]);
                    blockmap.lines.push(linedef);
                }
            }

            info!(
                "{}: Loaded blockmap, {} blocks",
                map_name,
                blockmap.columns * blockmap.rows
            );
            self.blockmap = blockmap;
        } else {
            info!("{}: No blockmap: TODO: build one", map_name);
        }
    }

    fn load_devils_rejects(&mut self, map_name: &str, wad: &WadData) {
        if let Some(rejects) = wad.read_rejects(map_name) {
            self.reject = rejects;
            info!("{}: Loaded {} reject bytes", map_name, self.reject.len());
        }
    }

    fn load_nodes(
        &mut self,
        map_name: &str,
        wad: &WadData,
        node_type: NodeLumpType,
        extended: Option<&WadExtendedMap>,
    ) {
        // BOXTOP = 0
        // BOXBOT = 1
        // BOXLEFT = 2
        // BOXRIGHT = 3
        let parse_nodes = |n: WadNode| {
            let bounding_boxes = [
                [
                    Vec2::new(n.bboxes[0][2] as f32, n.bboxes[0][0] as f32),
                    Vec2::new(n.bboxes[0][3] as f32, n.bboxes[0][1] as f32),
                ],
                [
                    Vec2::new(n.bboxes[1][2] as f32, n.bboxes[1][0] as f32),
                    Vec2::new(n.bboxes[1][3] as f32, n.bboxes[1][1] as f32),
                ],
            ];
            Node {
                xy: Vec2::new(n.x as f32, n.y as f32),
                delta: Vec2::new(n.dx as f32, n.dy as f32),
                bboxes: bounding_boxes,
                children: n.children,
            }
        };

        if node_type == NodeLumpType::OGDoom {
            self.nodes = wad.node_iter(map_name).map(parse_nodes).collect();
        } else if let Some(ext) = extended {
            self.nodes = ext.nodes.iter().map(|s| parse_nodes(s.clone())).collect();
        }
        info!("{}: Loaded {} bsp nodes", map_name, self.nodes.len());

        if extended.is_none() {
            for i in 0..self.nodes.len() {
                for n in 0..2 {
                    let node_num = &mut self.nodes[i].children[n];
                    // Correct the nodes for extended node support
                    if *node_num != u32::MAX && *node_num & IS_OLD_SSECTOR_MASK != 0 {
                        if *node_num == u16::MAX as u32 {
                            *node_num = u32::MAX;
                        } else {
                            *node_num &= !&IS_OLD_SSECTOR_MASK;
                            if *node_num as usize >= self.subsectors.len() {
                                *node_num = 0;
                            }
                            *node_num |= IS_SSECTOR_MASK;
                        }
                    }
                }
            }
            info!("{}: Fixed bsp node children", map_name);
        }

        self.start_node = (self.nodes.len() - 1) as u32;
    }

    /// Get a raw pointer to the subsector a point is in. This is mostly used to
    /// update an objects location so that sector effects can work on
    /// objects.
    ///
    /// Doom function name  `R_PointInSubsector`
    pub fn point_in_subsector_raw(&mut self, point: Vec2) -> MapPtr<SubSector> {
        let mut node_id = self.start_node();
        let mut node;
        let mut side;
        let mut count = self.nodes.len();

        while node_id & IS_SSECTOR_MASK == 0 {
            node = &self.get_nodes()[node_id as usize];
            side = node.point_on_side(&point);
            node_id = node.children[side];
            if count == 0 {
                dbg!(node, point);
                break;
            }
            count -= 1;
        }

        MapPtr::new(&mut self.subsectors[(node_id ^ IS_SSECTOR_MASK) as usize])
    }

    pub fn point_in_subsector(&mut self, point: Vec2) -> &SubSector {
        let mut node_id = self.start_node();
        let mut node;
        let mut side;

        while node_id & IS_SSECTOR_MASK == 0 {
            node = &self.get_nodes()[node_id as usize];
            side = node.point_on_side(&point);
            node_id = node.children[side];
        }

        &self.subsectors[(node_id ^ IS_SSECTOR_MASK) as usize]
    }

    /// Remove slime trails. killough 10/98
    // Slime trails are inherent to Doom's coordinate system -- i.e. there is
    /// nothing that a node builder can do to prevent slime trails ALL of the
    /// time, because it's a product of the integer coordinate system, and
    /// just because two lines pass through exact integer coordinates,
    /// doesn't necessarily mean that they will intersect at integer
    /// coordinates. Thus we must allow for fractional coordinates if we are
    /// to be able to split segs with node lines, as a node builder must do
    /// when creating a BSP tree.
    ///
    /// A wad file does not allow fractional coordinates, so node builders are
    /// out of luck except that they can try to limit the number of splits
    /// (they might also be able to detect the degree of roundoff error and
    /// try to avoid splits with a high degree of roundoff error). But we
    /// can use fractional coordinates here, inside the engine. It's like
    /// the difference between square centimetres and square millimetres, in
    /// terms of granularity.
    ///
    /// For each vertex of every seg, check to see whether it's also a vertex of
    /// the linedef associated with the seg (i.e, it's an endpoint). If it's not
    /// an endpoint, and it wasn't already moved, move the vertex towards the
    /// linedef by projecting it using the law of cosines. Formula:
    ///
    /// ```ignore
    ///      2        2                         2        2
    ///    dx  x0 + dy  x1 + dx dy (y0 - y1)  dy  y0 + dx  y1 + dx dy (x0 - x1)
    ///   {---------------------------------, ---------------------------------}
    ///                  2     2                            2     2
    ///                dx  + dy                           dx  + dy
    /// ```
    ///
    /// (x0,y0) is the vertex being moved, and (x1,y1)-(x1+dx,y1+dy) is the
    /// reference linedef.
    ///
    /// Segs corresponding to orthogonal linedefs (exactly vertical or
    /// horizontal linedefs), which comprise at least half of all linedefs
    /// in most wads, don't need to be considered, because they almost never
    /// contribute to slime trails (because then any roundoff error is
    /// parallel to the linedef, which doesn't cause slime). Skipping simple
    /// orthogonal lines lets the code finish quicker.
    ///
    /// Please note: This section of code is not interchangable with TeamTNT's
    /// code which attempts to fix the same problem.
    ///
    /// Firelines (TM) is a Rezistered Trademark of MBF Productions
    fn prepass_fix_vertices(
        map_name: &str,
        wad: &WadData,
        vertexes: &mut [Vec2],
        linedefs: &[LineDef],
        extended: Option<&WadExtendedMap>,
    ) {
        let start = Instant::now();
        let mut hit = vec![false; vertexes.len()];

        let mut parse_segs = |ms: WadSegment| {
            let v1 = vertexes[ms.start_vertex as usize];
            let v2 = vertexes[ms.end_vertex as usize];
            let linedef = &linedefs[ms.linedef as usize];

            if linedef.delta.x != 0.0 && linedef.delta.y != 0.0 {
                let vertices = [v1, v2];
                let vertex_indices = [ms.start_vertex as usize, ms.end_vertex as usize];

                for (&vertex_val, &v_idx) in vertices.iter().zip(vertex_indices.iter()) {
                    if !hit[v_idx] {
                        hit[v_idx] = true;

                        if vertex_val != linedef.v1 && vertex_val != linedef.v2 {
                            let dx2 = linedef.delta.x * linedef.delta.x;
                            let dy2 = linedef.delta.y * linedef.delta.y;
                            let dxy = linedef.delta.x * linedef.delta.y;
                            let s = dx2 + dy2;
                            let x0 = vertex_val.x;
                            let y0 = vertex_val.y;
                            let x1 = linedef.v1.x;
                            let y1 = linedef.v1.y;

                            let px = (dx2 * x0 + dy2 * x1 + dxy * (y0 - y1)) / s;
                            let py = (dy2 * y0 + dx2 * y1 + dxy * (x0 - x1)) / s;

                            // const FRACUNIT: f32 = 65536.0;
                            // if (px - x0).abs() <= 8.0 * FRACUNIT
                            //     && (py - y0).abs() <= 8.0 * FRACUNIT
                            // {
                            vertexes[v_idx].x = px;
                            vertexes[v_idx].y = py;
                            // }
                        }
                    }
                }
            }
        };

        if let Some(ext) = extended.as_ref() {
            ext.segments.iter().for_each(|s| parse_segs(s.clone()));
        } else {
            wad.segment_iter(map_name).for_each(parse_segs);
        }

        let end = Instant::now();
        info!("Fixed map vertices, took: {:#?}", end.duration_since(start));
    }
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
        check(line.v1);
        check(line.v2);
    }
    sector.sound_origin = Vec2::new(minx + ((maxx - minx) / 2.0), miny + ((maxy - miny) / 2.0));
}

#[derive(Debug, PartialEq, Eq)]
enum BSPTraceType {
    Line,
    Radius,
}

impl Default for BSPTraceType {
    fn default() -> Self {
        Self::Line
    }
}

pub struct BSPTrace {
    radius: f32,
    pub origin: Vec2,
    origin_left: Vec2,
    origin_right: Vec2,
    pub endpoint: Vec2,
    endpoint_left: Vec2,
    endpoint_right: Vec2,
    pub nodes: Vec<u32>,
    /// If it is a line_trace. If not then it is a radius trace.
    trace_type: BSPTraceType,
}

impl BSPTrace {
    /// Setup the trace for a line trace. Use `find_line_intercepts()` to find
    /// all intersections.
    #[inline]
    pub fn new_line(origin: Vec2, endpoint: Vec2, radius: f32) -> Self {
        let forward = Angle::from_vector(endpoint - origin);
        let back = Angle::from_vector(origin - endpoint);
        let left_rad_vec = (forward + FRAC_PI_2).unit() * radius;
        let right_rad_vec = (forward - FRAC_PI_2).unit() * radius;

        Self {
            origin: origin + back.unit() * radius,
            origin_left: origin + left_rad_vec + back.unit() * radius,
            origin_right: origin + right_rad_vec + back.unit() * radius,
            endpoint: endpoint + forward.unit() * radius,
            endpoint_left: endpoint + left_rad_vec + forward.unit() * radius,
            endpoint_right: endpoint + right_rad_vec + forward.unit() * radius,
            radius,
            nodes: Vec::with_capacity(50),
            trace_type: BSPTraceType::Line,
        }
    }

    #[inline]
    pub const fn new_radius(origin: Vec2, radius: f32) -> Self {
        Self {
            origin,
            radius,
            trace_type: BSPTraceType::Radius,
            origin_left: Vec2::new(0., 0.),
            origin_right: Vec2::new(0., 0.),
            endpoint: Vec2::new(0., 0.),
            endpoint_left: Vec2::new(0., 0.),
            endpoint_right: Vec2::new(0., 0.),
            nodes: Vec::new(),
        }
    }

    /// Do the BSP trace. The type of trace done is determined by if the trace
    /// was set up with `BSPTrace::new_line` or `BSPTrace::new_radius`.
    #[inline]
    pub fn find_intercepts(&mut self, node_id: u32, map: &MapData, count: &mut u32) {
        match self.trace_type {
            BSPTraceType::Line => self.find_line_inner(node_id, map, count),
            BSPTraceType::Radius => self.find_radius_inner(node_id, map, count),
        }
    }

    /// Trace a line through the BSP from origin vector to endpoint vector.
    ///
    /// Any node in the tree that has a splitting line separating the two points
    /// is added to the `nodes` list. The recursion always traverses down the
    /// the side closest to `origin` resulting in an ordered node list where
    /// the first node is the subsector the origin is in.
    #[inline]
    pub(super) fn find_line_inner(&mut self, node_id: u32, map: &MapData, count: &mut u32) {
        *count += 1;
        if node_id & IS_SSECTOR_MASK != 0 {
            let node = node_id & !IS_SSECTOR_MASK;
            #[cfg(Debug)]
            if (node as usize) >= map.nodes.len() {
                error!(
                    "Node {} masked to {} was out of bounds",
                    node_id,
                    node_id & !IS_ZSSECTOR_MASK
                );
                return;
            }
            if !self.nodes.contains(&node) {
                self.nodes.push(node);
            }
            return;
        }

        let node = &map.nodes[node_id as usize];

        // find which side the point is on
        let side1 = node.point_on_side(&self.origin);
        let side2 = node.point_on_side(&self.endpoint);

        if side1 != side2 {
            // On opposite sides of the splitting line, recurse down both sides
            // Traverse the side the origin is on first, then backside last. This
            // gives an ordered list of nodes from closest to furtherest.
            self.find_line_inner(node.children[side1], map, count);
            self.find_line_inner(node.children[side2], map, count);
        } else if self.radius > 1.0 {
            let side_l1 = node.point_on_side(&self.origin_left);
            let side_l2 = node.point_on_side(&self.endpoint_left);

            let side_r1 = node.point_on_side(&self.origin_right);
            let side_r2 = node.point_on_side(&self.endpoint_right);

            if side_l1 != side_l2 {
                self.find_line_inner(node.children[side_l1], map, count);
                self.find_line_inner(node.children[side_l2], map, count);
            } else if side_r1 != side_r2 {
                self.find_line_inner(node.children[side_r1], map, count);
                self.find_line_inner(node.children[side_r2], map, count);
            } else {
            }
            self.find_line_inner(node.children[side1], map, count);
        } else {
            self.find_line_inner(node.children[side1], map, count);
        }
    }

    #[inline]
    fn find_radius_inner(&mut self, node_id: u32, map: &MapData, count: &mut u32) {
        *count += 1;

        if node_id & IS_SSECTOR_MASK == IS_SSECTOR_MASK {
            let node = node_id & !IS_SSECTOR_MASK;
            #[cfg(Debug)]
            if (node as usize) >= map.nodes.len() {
                error!(
                    "Node {} masked to {} was out of bounds",
                    node_id,
                    node_id & !IS_ZSSECTOR_MASK
                );
                return;
            }
            // Commented out because it cuts off some sectors
            // if node.point_in_bounds(&self.origin, side)
            //     || circle_line_collide(self.origin, self.radius, l_start, l_end)
            // {
            if !self.nodes.contains(&node) {
                self.nodes.push(node);
            }
            // };
            return;
        }

        let node = &map.nodes[node_id as usize];
        let l_start = node.xy;
        let l_end = l_start + node.delta;
        let side = node.point_on_side(&self.origin);

        if circle_line_collide(self.origin, self.radius, l_start, l_end) {
            let other = if side == 1 { 0 } else { 1 };
            self.find_radius_inner(node.children[side], map, count);
            self.find_radius_inner(node.children[other], map, count);
        } else {
            self.find_radius_inner(node.children[side], map, count);
        }
    }

    /// List of indexes to subsectors the trace intercepted
    #[inline]
    pub fn intercepted_subsectors(&self) -> &[u32] {
        &self.nodes
    }
}
