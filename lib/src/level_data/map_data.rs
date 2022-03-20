use std::marker::PhantomPinned;
use std::ptr::null_mut;

use crate::{
    angle::Angle,
    level_data::map_defs::{BBox, LineDef, Node, Sector, Segment, SideDef, SlopeType, SubSector},
    log::info,
    play::utilities::bam_to_radian,
    DPtr,
};
use glam::Vec2;
use wad::{lumps::*, WadData};

pub const IS_SSECTOR_MASK: u16 = 0x8000;

/// The smallest vector and the largest vertex, combined make up a
/// rectangle enclosing the level area
#[derive(Default)]
pub struct MapExtents {
    pub min_vertex: Vec2,
    pub max_vertex: Vec2,
    pub width: f32,
    pub height: f32,
    pub automap_scale: f32,
}

/// A `Map` contains everything required for building the actual level the
/// player will see in-game, such as the data to build a level, the textures used,
/// `Things`, `Sounds` and others.
///
/// `nodes`, `subsectors`, and `segments` are what get used most to render the
/// basic level
///
/// Access to the `Vec` arrays within is limited to immutable only to
/// prevent unwanted removal of items, which *will* break references and
/// segfault
pub struct MapData {
    name: String,
    /// Things will be linked to/from each other in many ways, which means this array may
    /// never be resized or it will invalidate references and pointers
    things: Vec<WadThing>,
    vertexes: Vec<Vec2>,
    pub linedefs: Vec<LineDef>,
    pub sectors: Vec<Sector>,
    sidedefs: Vec<SideDef>,
    pub subsectors: Vec<SubSector>,
    pub segments: Vec<Segment>,
    extents: MapExtents,
    nodes: Vec<Node>,
    start_node: u16,
    _pinned: PhantomPinned,
}

impl MapData {
    pub fn new(name: String) -> MapData {
        MapData {
            name,
            things: Vec::new(),
            vertexes: Vec::new(),
            linedefs: Vec::new(),
            sectors: Vec::new(),
            sidedefs: Vec::new(),
            subsectors: Vec::new(),
            segments: Vec::new(),
            extents: MapExtents::default(),
            nodes: Vec::new(),
            start_node: 0,
            _pinned: PhantomPinned,
        }
    }

    #[inline]
    pub fn get_things(&self) -> &[WadThing] {
        &self.things
    }

    #[inline]
    pub fn set_extents(&mut self) {
        // set the min/max to first vertex so we have a baseline
        // that isn't 0 causing comparison issues, eg; if it's 0,
        // then a min vertex of -3542 won't be set since it's negative
        self.extents.min_vertex.set_x(self.vertexes[0].x());
        self.extents.min_vertex.set_y(self.vertexes[0].y());
        self.extents.max_vertex.set_x(self.vertexes[0].x());
        self.extents.max_vertex.set_y(self.vertexes[0].y());
        for v in &self.vertexes {
            if self.extents.min_vertex.x() > v.x() {
                self.extents.min_vertex.set_x(v.x());
            } else if self.extents.max_vertex.x() < v.x() {
                self.extents.max_vertex.set_x(v.x());
            }

            if self.extents.min_vertex.y() > v.y() {
                self.extents.min_vertex.set_y(v.y());
            } else if self.extents.max_vertex.y() < v.y() {
                self.extents.max_vertex.set_y(v.y());
            }
        }
        self.extents.width = self.extents.max_vertex.x() - self.extents.min_vertex.x();
        self.extents.height = self.extents.max_vertex.y() - self.extents.min_vertex.y();
    }

    #[inline]
    pub fn vertexes(&self) -> &[Vec2] {
        &self.vertexes
    }

    #[inline]
    pub fn linedefs(&self) -> &[LineDef] {
        &self.linedefs
    }

    #[inline]
    pub fn sectors(&self) -> &[Sector] {
        &self.sectors
    }

    #[inline]
    pub fn sectors_mut(&mut self) -> &mut [Sector] {
        &mut self.sectors
    }

    #[inline]
    pub fn sidedefs(&self) -> &[SideDef] {
        &self.sidedefs
    }

    #[inline]
    pub fn subsectors(&self) -> &[SubSector] {
        &self.subsectors
    }

    #[inline]
    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    #[inline]
    pub fn segments_mut(&mut self) -> &mut [Segment] {
        &mut self.segments
    }

    fn set_scale(&mut self) {
        let map_width = self.extents.width as f32;
        let map_height = self.extents.height as f32;

        if map_height > map_width {
            self.extents.automap_scale = map_height / 200.0 * 1.1;
        } else {
            self.extents.automap_scale = map_width / 320.0 * 1.4;
        }
    }

    #[inline]
    pub fn get_nodes(&self) -> &[Node] {
        &self.nodes
    }

    #[inline]
    pub fn start_node(&self) -> u16 {
        self.start_node
    }

    #[inline]
    pub fn get_map_extents(&self) -> &MapExtents {
        &self.extents
    }

    // TODO: pass in TextureData
    pub fn load(&mut self, wad: &WadData) {
        // THINGS
        self.things = wad.thing_iter(&self.name).collect();
        info!("{}: Loaded things", self.name);

        // Vertexes
        self.vertexes = wad
            .vertex_iter(&self.name)
            .map(|v| Vec2::new(v.x as f32, v.y as f32))
            .collect();
        info!("{}: Loaded vertexes", self.name);

        // Sectors
        self.sectors = wad
            .sector_iter(&self.name)
            .map(|s| Sector {
                floorheight: s.floor_height as f32,
                ceilingheight: s.ceil_height as f32,
                floorpic: wad
                    .flats_iter()
                    .position(|f| f.name == s.floor_tex)
                    .unwrap_or(usize::MAX),
                ceilingpic: wad
                    .flats_iter()
                    .position(|f| f.name == s.ceil_tex)
                    .unwrap_or(usize::MAX),
                lightlevel: s.light_level as i32,
                special: s.kind,
                tag: s.tag,
                soundtraversed: 0,
                blockbox: [0, 0, 0, 0],
                validcount: 0,
                specialdata: None,
                lines: Vec::new(),
                thinglist: null_mut(),
            })
            .collect();
        info!("{}: Loaded segments", self.name);

        let mut tex_order: Vec<WadTexture> = wad.texture_iter("TEXTURE1").collect();
        if wad.lump_exists("TEXTURE2") {
            let mut pnames2: Vec<WadTexture> = wad.texture_iter("TEXTURE2").collect();
            tex_order.append(&mut pnames2);
        }
        // Sidedefs
        self.sidedefs = wad
            .sidedef_iter(&self.name)
            .map(|s| {
                let sector = &self.sectors()[s.sector as usize];

                SideDef {
                    textureoffset: s.x_offset as f32,
                    rowoffset: s.y_offset as f32,
                    toptexture: tex_order
                        .iter()
                        .position(|n| n.name == s.upper_tex.to_ascii_uppercase())
                        .unwrap_or(usize::MAX),
                    bottomtexture: tex_order
                        .iter()
                        .position(|n| n.name == s.lower_tex.to_ascii_uppercase())
                        .unwrap_or(usize::MAX),
                    midtexture: tex_order
                        .iter()
                        .position(|n| n.name == s.middle_tex.to_ascii_uppercase())
                        .unwrap_or(usize::MAX),
                    sector: DPtr::new(sector),
                }
            })
            .collect();
        info!("{}: Loaded sidedefs", self.name);

        //LineDefs
        self.linedefs = wad
            .linedef_iter(&self.name)
            .map(|l| {
                let v1 = &self.vertexes()[l.start_vertex as usize];
                let v2 = &self.vertexes()[l.end_vertex as usize];

                let front = &self.sidedefs()[l.front_sidedef as usize];

                let back_side = {
                    l.back_sidedef
                        .map(|index| DPtr::new(&self.sidedefs()[index as usize]))
                };

                let back_sector = {
                    l.back_sidedef
                        .map(|index| self.sidedefs()[index as usize].sector.clone())
                };

                let dx = v2.x() - v1.x();
                let dy = v2.y() - v1.y();

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
                    v1: DPtr::new(v1),
                    v2: DPtr::new(v2),
                    delta: Vec2::new(dx, dy),
                    flags: l.flags as u32,
                    special: l.special,
                    tag: l.sector_tag,
                    bbox: BBox::new(*v1, *v2),
                    slopetype: slope,
                    front_sidedef: DPtr::new(front),
                    back_sidedef: back_side,
                    frontsector: front.sector.clone(),
                    backsector: back_sector,
                    valid_count: 0,
                }
            })
            .collect();
        info!("{}: Loaded linedefs", self.name);

        // Now map sectors to lines
        for line in self.linedefs.iter_mut() {
            let mut sector = line.frontsector.clone();
            sector.lines.push(DPtr::new(line));
            if let Some(mut sector) = line.backsector.clone() {
                sector.lines.push(DPtr::new(line));
            }
        }
        info!("{}: Mapped liedefs to sectors", self.name);

        // Sector, Sidedef, Linedef, Seg all need to be preprocessed before
        // storing in level struct
        //
        // SEGS
        self.segments = wad
            .segment_iter(&self.name)
            .map(|s| {
                let v1 = &self.vertexes()[s.start_vertex as usize];
                let v2 = &self.vertexes()[s.end_vertex as usize];

                let ldef = &self.linedefs()[s.linedef as usize];

                let frontsector;
                let backsector;
                let side;
                // The front and back sectors interchange depending on BSP
                if s.side == 0 {
                    side = ldef.front_sidedef.clone();
                    frontsector = ldef.frontsector.clone();
                    backsector = ldef.backsector.clone();
                } else {
                    side = ldef.back_sidedef.as_ref().unwrap().clone();
                    frontsector = ldef.backsector.as_ref().unwrap().clone();
                    backsector = Some(ldef.frontsector.clone());
                };

                let angle = bam_to_radian((s.angle as u32) << 16);

                Segment {
                    v1: DPtr::new(v1),
                    v2: DPtr::new(v2),
                    offset: s.offset as f32,
                    angle: Angle::new(angle),
                    sidedef: side,
                    linedef: DPtr::new(ldef),
                    frontsector,
                    backsector,
                }
            })
            .collect();
        info!("{}: Generated segments", self.name);

        // SSECTORS
        self.subsectors = wad
            .subsector_iter(&self.name)
            .map(|s| {
                let sector = self.segments()[s.start_seg as usize].sidedef.sector.clone();
                SubSector {
                    sector,
                    seg_count: s.seg_count,
                    start_seg: s.start_seg,
                }
            })
            .collect();
        info!("{}: Loaded subsectors", self.name);

        // NODES
        self.nodes = wad
            .node_iter(&self.name)
            .map(|n| Node {
                xy: Vec2::new(n.x as f32, n.y as f32),
                delta: Vec2::new(n.dx as f32, n.dy as f32),
                bounding_boxes: [
                    [
                        Vec2::new(n.bounding_boxes[0][2] as f32, n.bounding_boxes[0][0] as f32),
                        Vec2::new(n.bounding_boxes[0][3] as f32, n.bounding_boxes[0][1] as f32),
                    ],
                    [
                        Vec2::new(n.bounding_boxes[1][2] as f32, n.bounding_boxes[1][0] as f32),
                        Vec2::new(n.bounding_boxes[1][3] as f32, n.bounding_boxes[1][1] as f32),
                    ],
                ],
                child_index: n.child_index,
                parent: 0,
            })
            .collect();
        info!("{}: Loaded bsp nodes", self.name);

        for (i, wn) in wad.node_iter(&self.name).enumerate() {
            if wn.child_index[0] & IS_SSECTOR_MASK == 0 {
                self.nodes[wn.child_index[0] as usize].parent = i as u16;
            }
            if wn.child_index[1] & IS_SSECTOR_MASK == 0 {
                self.nodes[wn.child_index[1] as usize].parent = i as u16;
            }
        }
        info!("{}: Mapped bsp node children", self.name);

        // BLOCKMAP
        //let bm = wad.read_blockmap(&self.name);
        // self.blockmap.x_origin = bm.x_origin as f32;
        // self.blockmap.y_origin = bm.y_origin as f32;
        // self.blockmap.width = bm.width as i32;
        // self.blockmap.height = bm.height as i32;
        // self.blockmap.line_indexes = bm.line_indexes.iter().map(|n| *n as usize).collect();
        // self.blockmap.blockmap_offset = bm.blockmap_offset;

        self.start_node = (self.nodes.len() - 1) as u16;
        self.set_extents();
        self.set_scale();
    }

    /// R_PointInSubsector - r_main
    pub fn point_in_subsector_mut(&mut self, point: Vec2) -> *mut SubSector {
        let mut node_id = self.start_node();
        let mut node;
        let mut side;

        while node_id & IS_SSECTOR_MASK == 0 {
            node = &self.get_nodes()[node_id as usize];
            side = node.point_on_side(&point);
            node_id = node.child_index[side];
        }

        &mut self.subsectors[(node_id ^ IS_SSECTOR_MASK) as usize] as *mut _
    }

    pub fn point_in_subsector_ref(&self, point: Vec2) -> *const SubSector {
        let mut node_id = self.start_node();
        let mut node;
        let mut side;

        while node_id & IS_SSECTOR_MASK == 0 {
            node = &self.get_nodes()[node_id as usize];
            side = node.point_on_side(&point);
            node_id = node.child_index[side];
        }

        &self.subsectors[(node_id ^ IS_SSECTOR_MASK) as usize] as *const _
    }
}

pub struct BSPTrace {
    origin: Vec2,
    endpoint: Vec2,
    node_id: u16,
    nodes: Vec<u16>,
}

impl BSPTrace {
    pub fn new(origin: Vec2, endpoint: Vec2, node_id: u16) -> Self {
        Self {
            origin,
            endpoint,
            node_id,
            nodes: Vec::with_capacity(20),
        }
    }

    pub fn set_line(&mut self, origin: Vec2, endpoint: Vec2) {
        self.origin = origin;
        self.endpoint = endpoint;
    }

    /// Trace a line through the BSP from origin vector to endpoint vector.
    ///
    /// Any node in the tree that has a splitting line separating the two points
    /// is added to the `nodes` list. The recursion always traverses down the
    /// the side closest to `origin` resulting in an ordered node list where
    /// the first node is the subsector the origin is in.
    pub fn find_ssect_intercepts(&mut self, map: &MapData, count: &mut u32) {
        *count += 1;
        if self.node_id & IS_SSECTOR_MASK == IS_SSECTOR_MASK {
            if !self.nodes.contains(&(self.node_id ^ IS_SSECTOR_MASK)) {
                self.nodes.push(self.node_id ^ IS_SSECTOR_MASK);
            }
            return;
        }
        let node = &map.get_nodes()[self.node_id as usize];

        // find which side the point is on
        let side1 = node.point_on_side(&self.origin);
        let side2 = node.point_on_side(&self.endpoint);
        if side1 != side2 {
            // On opposite sides of the splitting line, recurse down both sides
            // Traverse the side the origin is on first, then backside last. This
            // gives an ordered list of nodes from closest to furtherest.
            self.node_id = node.child_index[side1];
            self.find_ssect_intercepts(map, count);
            self.node_id = node.child_index[side2];
            self.find_ssect_intercepts(map, count);
        } else {
            self.node_id = node.child_index[side1];
            self.find_ssect_intercepts(map, count);
        }
    }

    pub fn intercepted_nodes(&self) -> &[u16] {
        &self.nodes
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        angle::Angle,
        level_data::map_data::{BSPTrace, MapData},
    };
    use glam::Vec2;
    use std::f32::consts::{FRAC_PI_2, PI};
    use wad::WadData;

    #[test]
    fn test_tracing_bsp() {
        let wad = WadData::new("../doom1.wad".into());
        let mut map = MapData::new("E1M1".to_owned());
        map.load(&wad);
        let origin = Vec2::new(710.0, -3400.0); // left corner from start
        let endpoint = Vec2::new(710.0, -3000.0); // 3 sectors up

        // let origin = Vec2::new(1056.0, -3616.0); // player start
        // let endpoint = Vec2::new(1088.0, -2914.0); // corpse ahead, 10?
        //let endpoint = Vec2::new(1340.0, -2884.0); // ?
        //let endpoint = Vec2::new(2912.0, -2816.0);

        let mut bsp_trace = BSPTrace::new(origin, endpoint, map.start_node);
        // bsp_trace.trace_to_point(&map);
        // dbg!(&nodes.len());
        // dbg!(&nodes);

        let sub_sect = map.subsectors();
        // let segs = map.get_segments();
        // for x in nodes.iter() {
        //     //let x = nodes.last().unwrap();
        //     let start = sub_sect[*x as usize].start_seg as usize;
        //     let end = sub_sect[*x as usize].seg_count as usize + start;
        //     for seg in &segs[start..end] {
        //         dbg!(x);
        //         dbg!(sub_sect[*x as usize].seg_count);
        //         dbg!(&seg.v1);
        //         dbg!(&seg.v2);
        //     }
        // }

        let _endpoint = Vec2::new(710.0, -3000.0); // 3 sectors up
        let segs = map.segments();
        // wander around the coords of the subsector corner from player start
        let mut count = 0;
        for x in 705..895 {
            for y in -3551..-3361 {
                bsp_trace.origin = Vec2::new(x as f32, y as f32);
                bsp_trace.find_ssect_intercepts(&map, &mut count);

                // Sector the starting vector is in. 3 segs attached
                let x = bsp_trace.intercepted_nodes().first().unwrap();
                let start = sub_sect[*x as usize].start_seg as usize;

                // Bottom horizontal line
                assert_eq!(segs[start].v1.x(), 832.0);
                assert_eq!(segs[start].v1.y(), -3552.0);
                assert_eq!(segs[start].v2.x(), 704.0);
                assert_eq!(segs[start].v2.y(), -3552.0);
                // Left side of the pillar
                assert_eq!(segs[start + 1].v1.x(), 896.0);
                assert_eq!(segs[start + 1].v1.y(), -3360.0);
                assert_eq!(segs[start + 1].v2.x(), 896.0);
                assert_eq!(segs[start + 1].v2.y(), -3392.0);
                // Left wall
                assert_eq!(segs[start + 2].v1.x(), 704.0);
                assert_eq!(segs[start + 2].v1.y(), -3552.0);
                assert_eq!(segs[start + 2].v2.x(), 704.0);
                assert_eq!(segs[start + 2].v2.y(), -3360.0);

                // Last sector directly above starting vector
                let x = bsp_trace.intercepted_nodes().last().unwrap();
                let start = sub_sect[*x as usize].start_seg as usize;

                assert_eq!(segs[start].v1.x(), 896.0);
                assert_eq!(segs[start].v1.y(), -3072.0);
                assert_eq!(segs[start].v2.x(), 896.0);
                assert_eq!(segs[start].v2.y(), -3104.0);
                assert_eq!(segs[start + 1].v1.x(), 704.0);
                assert_eq!(segs[start + 1].v1.y(), -3104.0);
                assert_eq!(segs[start + 1].v2.x(), 704.0);
                assert_eq!(segs[start + 1].v2.y(), -2944.0);
            }
        }
    }

    #[test]
    fn check_e1m1_things() {
        let wad = WadData::new("../doom1.wad".into());
        let mut map = MapData::new("E1M1".to_owned());
        map.load(&wad);

        let things = map.get_things();
        assert_eq!(things[0].x as i32, 1056);
        assert_eq!(things[0].y as i32, -3616);
        assert_eq!(things[0].angle, 90);
        assert_eq!(things[0].kind, 1);
        assert_eq!(things[0].flags, 7);
        assert_eq!(things[137].x as i32, 3648);
        assert_eq!(things[137].y as i32, -3840);
        assert_eq!(things[137].angle, 0);
        assert_eq!(things[137].kind, 2015);
        assert_eq!(things[137].flags, 7);

        assert_eq!(things[0].angle, 90);
        assert_eq!(things[9].angle, 135);
        assert_eq!(things[14].angle, 0);
        assert_eq!(things[16].angle, 90);
        assert_eq!(things[17].angle, 180);
        assert_eq!(things[83].angle, 270);
    }

    #[test]
    fn check_e1m1_vertexes() {
        let wad = WadData::new("../doom1.wad".into());
        let mut map = MapData::new("E1M1".to_owned());
        map.load(&wad);

        let vertexes = map.vertexes();
        assert_eq!(vertexes[0].x() as i32, 1088);
        assert_eq!(vertexes[0].y() as i32, -3680);
        assert_eq!(vertexes[466].x() as i32, 2912);
        assert_eq!(vertexes[466].y() as i32, -4848);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn check_e1m1_lump_pointers() {
        let wad = WadData::new("../doom1.wad".into());
        let mut map = MapData::new("E1M1".to_owned());
        map.load(&wad);

        let linedefs = map.linedefs();

        // Check links
        // LINEDEF->VERTEX
        assert_eq!(linedefs[2].v1.x() as i32, 1088);
        assert_eq!(linedefs[2].v2.x() as i32, 1088);
        // // LINEDEF->SIDEDEF
        // assert_eq!(linedefs[2].front_sidedef.midtexture, "LITE3");
        // // LINEDEF->SIDEDEF->SECTOR
        // assert_eq!(linedefs[2].front_sidedef.sector.floorpic, "FLOOR4_8");
        // // LINEDEF->SIDEDEF->SECTOR
        assert_eq!(linedefs[2].front_sidedef.sector.ceilingheight, 72.0);

        let segments = map.segments();
        // SEGMENT->VERTEX
        assert_eq!(segments[0].v1.x() as i32, 1552);
        assert_eq!(segments[0].v2.x() as i32, 1552);
        // SEGMENT->LINEDEF->SIDEDEF->SECTOR
        // seg:0 -> line:152 -> side:209 -> sector:0 -> ceiltex:CEIL3_5 lightlevel:160
        // assert_eq!(
        //     segments[0].linedef.front_sidedef.sector.ceilingpic,
        //     "CEIL3_5"
        // );
        // // SEGMENT->LINEDEF->SIDEDEF
        // assert_eq!(segments[0].linedef.front_sidedef.toptexture, "BIGDOOR2");

        // let sides = map.get_sidedefs();
        // assert_eq!(sides[211].sector.ceilingpic, "TLITE6_4");
    }

    #[test]
    fn check_e1m1_linedefs() {
        let wad = WadData::new("../doom1.wad".into());
        let mut map = MapData::new("E1M1".to_owned());
        map.load(&wad);

        let linedefs = map.linedefs();
        assert_eq!(linedefs[0].v1.x() as i32, 1088);
        assert_eq!(linedefs[0].v2.x() as i32, 1024);
        assert_eq!(linedefs[2].v1.x() as i32, 1088);
        assert_eq!(linedefs[2].v2.x() as i32, 1088);

        assert_eq!(linedefs[474].v1.x() as i32, 3536);
        assert_eq!(linedefs[474].v2.x() as i32, 3520);
        assert!(linedefs[2].back_sidedef.is_none());
        assert_eq!(linedefs[474].flags, 1);
        assert!(linedefs[474].back_sidedef.is_none());
        assert!(linedefs[466].back_sidedef.is_some());

        // Flag check
        assert_eq!(linedefs[26].flags, 29);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn check_e1m1_sectors() {
        let wad = WadData::new("../doom1.wad".into());
        let mut map = MapData::new("E1M1".to_owned());
        map.load(&wad);

        let sectors = map.sectors();
        assert_eq!(sectors[0].floorheight, 0.0);
        assert_eq!(sectors[0].ceilingheight, 72.0);
        assert_eq!(sectors[0].lightlevel, 160);
        assert_eq!(sectors[0].tag, 0);
        assert_eq!(sectors[84].floorheight, -24.0);
        assert_eq!(sectors[84].ceilingheight, 48.0);
        assert_eq!(sectors[84].lightlevel, 255);
        assert_eq!(sectors[84].special, 0);
        assert_eq!(sectors[84].tag, 0);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn check_e1m1_sidedefs() {
        let wad = WadData::new("../doom1.wad".into());
        let mut map = MapData::new("E1M1".to_owned());
        map.load(&wad);

        let sidedefs = map.sidedefs();
        assert_eq!(sidedefs[0].rowoffset, 0.0);
        assert_eq!(sidedefs[0].textureoffset, 0.0);
        assert_eq!(sidedefs[9].rowoffset, 48.0);
        assert_eq!(sidedefs[9].textureoffset, 0.0);
        assert_eq!(sidedefs[647].rowoffset, 0.0);
        assert_eq!(sidedefs[647].textureoffset, 4.0);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn check_e1m1_segments() {
        let wad = WadData::new("../doom1.wad".into());

        let mut map = MapData::new("E1M1".to_owned());
        map.load(&wad);

        let segments = map.segments();
        assert_eq!(segments[0].v1.x() as i32, 1552);
        assert_eq!(segments[0].v2.x() as i32, 1552);
        assert_eq!(segments[731].v1.x() as i32, 3040);
        assert_eq!(segments[731].v2.x() as i32, 2976);
        assert_eq!(segments[0].angle, Angle::new(FRAC_PI_2));
        assert_eq!(segments[0].offset, 0.0);

        assert_eq!(segments[731].angle, Angle::new(PI));
        assert_eq!(segments[731].offset, 0.0);

        let subsectors = map.subsectors();
        assert_eq!(subsectors[0].seg_count, 4);
        assert_eq!(subsectors[124].seg_count, 3);
        assert_eq!(subsectors[236].seg_count, 4);
        //assert_eq!(subsectors[0].start_seg.start_vertex.x() as i32, 1552);
        //assert_eq!(subsectors[124].start_seg.start_vertex.x() as i32, 472);
        //assert_eq!(subsectors[236].start_seg.start_vertex.x() as i32, 3040);
    }

    #[test]
    fn find_vertex_using_bsptree() {
        let wad = WadData::new("../doom1.wad".into());
        let mut map = MapData::new("E1M1".to_owned());
        map.load(&wad);

        // The actual location of THING0
        let player = Vec2::new(1056.0, -3616.0);
        let subsector = map.point_in_subsector_mut(player);
        //assert_eq!(subsector_id, Some(103));
        unsafe {
            assert_eq!((*subsector).seg_count, 5);
            assert_eq!((*subsector).start_seg, 305);
        }
    }
}
