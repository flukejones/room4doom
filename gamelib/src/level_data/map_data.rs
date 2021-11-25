use crate::angle::Angle;
use crate::level_data::map_defs::{
    BBox, LineDef, Node, Sector, Segment, SideDef, SlopeType, SubSector,
};
use crate::p_local::bam_to_radian;
use crate::DPtr;
use glam::Vec2;
use wad::{lumps::*, WadData};

pub(crate) const IS_SSECTOR_MASK: u16 = 0x8000;

/// The smallest vector and the largest vertex, combined make up a
/// rectangle enclosing the level area
#[derive(Debug, Default)]
pub(crate) struct MapExtents {
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
#[derive(Debug)]
pub(crate) struct MapData {
    name: String,
    /// Things will be linked to/from each other in many ways, which means this array may
    /// never be resized or it will invalidate references and pointers
    things: Vec<WadThing>,
    vertexes: Vec<Vec2>,
    linedefs: Vec<LineDef>,
    sectors: Vec<Sector>,
    sidedefs: Vec<SideDef>,
    subsectors: Vec<SubSector>,
    segments: Vec<Segment>,
    extents: MapExtents,
    nodes: Vec<Node>,
    start_node: u16,
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
        }
    }

    #[inline]
    pub fn get_things(&self) -> &[WadThing] {
        &self.things
    }

    #[inline]
    pub(crate) fn set_extents(&mut self) {
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
    pub fn get_vertexes(&self) -> &[Vec2] {
        &self.vertexes
    }

    #[inline]
    pub fn get_linedefs(&self) -> &[LineDef] {
        &self.linedefs
    }

    #[inline]
    pub fn get_sectors(&self) -> &[Sector] {
        &self.sectors
    }

    #[inline]
    pub fn get_sidedefs(&self) -> &[SideDef] {
        &self.sidedefs
    }

    #[inline]
    pub fn get_subsectors(&self) -> &[SubSector] {
        &self.subsectors
    }

    #[inline]
    pub fn get_segments(&self) -> &[Segment] {
        &self.segments
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

    pub fn load(&mut self, wad: &WadData) {
        // THINGS
        self.things = wad.thing_iter(&self.name).collect();

        // Vertexes
        self.vertexes = wad
            .vertex_iter(&self.name)
            .map(|v| Vec2::new(v.x as f32, v.y as f32))
            .collect();

        // Sectors
        self.sectors = wad
            .sector_iter(&self.name)
            .map(|s| Sector {
                floorheight: s.floor_height as f32,
                ceilingheight: s.ceil_height as f32,
                floorpic: 0,   // TODO: lookup texture
                ceilingpic: 0, // TODO: lookup texture
                lightlevel: s.light_level,
                special: s.kind,
                tag: s.tag,
                soundtraversed: 0,
                blockbox: [0, 0, 0, 0],
                validcount: 0,
                lines: Vec::new(),
            })
            .collect();

        // Sidedefs
        self.sidedefs = wad
            .sidedef_iter(&self.name)
            .map(|s| {
                let sector = &self.get_sectors()[s.sector as usize];

                SideDef {
                    textureoffset: s.y_offset as f32,
                    rowoffset: s.x_offset as f32,
                    toptexture: if s.upper_tex.is_empty() { 0 } else { 1 },
                    bottomtexture: if s.lower_tex.is_empty() { 0 } else { 1 },
                    midtexture: if s.middle_tex.is_empty() { 0 } else { 1 },
                    sector: DPtr::new(sector),
                }
            })
            .collect();

        //LineDefs
        self.linedefs = wad
            .linedef_iter(&self.name)
            .map(|l| {
                let v1 = &self.get_vertexes()[l.start_vertex as usize];
                let v2 = &self.get_vertexes()[l.end_vertex as usize];

                let front = &self.get_sidedefs()[l.front_sidedef as usize];

                let back_side = {
                    if let Some(index) = l.back_sidedef {
                        Some(DPtr::new(&self.get_sidedefs()[index as usize]))
                    } else {
                        None
                    }
                };

                let back_sector = {
                    if let Some(index) = l.back_sidedef {
                        Some(self.get_sidedefs()[index as usize].sector.clone())
                    } else {
                        None
                    }
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
                    flags: l.flags,
                    special: l.special,
                    tag: l.sector_tag,
                    bbox: BBox::new(*v1, *v2),
                    slopetype: slope,
                    front_sidedef: DPtr::new(front),
                    back_sidedef: back_side,
                    frontsector: front.sector.clone(),
                    backsector: back_sector,
                    validcount: 0,
                }
            })
            .collect();

        // Now map sectors to lines
        // This is going to be required for collision checks
        for line in self.linedefs.iter_mut() {
            let mut sector = line.frontsector.clone();
            sector.lines.push(DPtr::new(line));
        }

        // Sector, Sidedef, Linedef, Seg all need to be preprocessed before
        // storing in level struct
        //
        // SEGS
        self.segments = wad
            .segment_iter(&self.name)
            .map(|s| {
                let v1 = &self.get_vertexes()[s.start_vertex as usize];
                let v2 = &self.get_vertexes()[s.end_vertex as usize];

                let line = &self.get_linedefs()[s.linedef as usize];
                let side = if s.direction == 0 {
                    line.front_sidedef.clone()
                } else {
                    // Safe as this is not possible. If there is no back sidedef
                    // then it defaults to the front
                    line.back_sidedef.as_ref().unwrap().clone()
                };

                let angle = bam_to_radian((s.angle as u32) << 16);

                Segment {
                    v1: DPtr::new(v1),
                    v2: DPtr::new(v2),
                    offset: s.offset as f32,
                    angle: Angle::new(angle),
                    sidedef: side,
                    linedef: DPtr::new(line),
                    frontsector: line.frontsector.clone(),
                    backsector: line.backsector.clone(),
                }
            })
            .collect();

        // SSECTORS
        self.subsectors = wad
            .subsector_iter(&self.name)
            .map(|s| {
                let sector = self.get_segments()[s.start_seg as usize]
                    .sidedef
                    .sector
                    .clone();
                SubSector {
                    sector,
                    seg_count: s.seg_count,
                    start_seg: s.start_seg,
                }
            })
            .collect();

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
            })
            .collect();

        self.start_node = (self.nodes.len() - 1) as u16;
        self.set_extents();
        self.set_scale();
    }

    /// R_PointInSubsector - r_main
    pub(crate) fn point_in_subsector(&self, point: &Vec2) -> DPtr<SubSector> {
        let mut node_id = self.start_node();
        let mut node;
        let mut side;

        while node_id & IS_SSECTOR_MASK == 0 {
            node = &self.get_nodes()[node_id as usize];
            side = node.point_on_side(&point);
            node_id = node.child_index[side];
        }

        return DPtr::new(&self.get_subsectors()[(node_id ^ IS_SSECTOR_MASK) as usize]);
    }
}
