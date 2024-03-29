use std::{collections::HashMap, f32::consts::FRAC_PI_2, time::Instant};

use crate::{
    angle::Angle,
    level::map_defs::{BBox, LineDef, Node, Sector, Segment, SideDef, SlopeType, SubSector},
    log::info,
    utilities::{bam_to_radian, circle_line_collide},
    DPtr, PicData,
};
use glam::Vec2;
#[cfg(Debug)]
use log::error;
use log::warn;
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
/// player will see in-game-exe, such as the data to build a level, the textures used,
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
    pub linedefs: Vec<LineDef>,
    pub sectors: Vec<Sector>,
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
    }

    #[inline]
    pub fn things(&self) -> &[WadThing] {
        &self.things
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
    pub fn subsectors_mut(&mut self) -> &mut [SubSector] {
        &mut self.subsectors
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
        let map_width = self.extents.width;
        let map_height = self.extents.height;

        if map_height > map_width {
            self.extents.automap_scale = map_height / 400.0 * 1.1;
        } else {
            self.extents.automap_scale = map_width / 640.0 * 1.4;
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
    pub fn load(&mut self, pic_data: &PicData, wad: &WadData) {
        // THINGS
        self.things = wad.thing_iter(&self.name).collect();
        info!("{}: Loaded things", self.name);

        // Vertexes
        let vertexes: Vec<Vec2> = wad
            .vertex_iter(&self.name)
            .map(|v| Vec2::new(v.x as f32, v.y as f32))
            .collect();
        info!("{}: Loaded vertexes", self.name);

        // Sectors
        self.sectors = wad
            .sector_iter(&self.name)
            .enumerate()
            .map(|(i, s)| {
                Sector::new(
                    i as u32,
                    s.floor_height as f32,
                    s.ceil_height as f32,
                    pic_data.flat_num_for_name(&s.floor_tex).unwrap_or_else(|| {
                        warn!("Sectors: Did not find flat for {}", s.floor_tex);
                        usize::MAX
                    }),
                    pic_data.flat_num_for_name(&s.ceil_tex).unwrap_or_else(|| {
                        warn!("Sectors: Did not find flat for {}", s.ceil_tex);
                        usize::MAX
                    }),
                    s.light_level as i32,
                    s.kind,
                    s.tag,
                )
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
                    sector: DPtr::new(sector),
                }
            })
            .collect();
        info!("{}: Loaded sidedefs", self.name);

        //LineDefs
        self.linedefs = wad
            .linedef_iter(&self.name)
            .map(|l| {
                let v1 = vertexes[l.start_vertex as usize];
                let v2 = vertexes[l.end_vertex as usize];

                let front = DPtr::new(&mut self.sidedefs[l.front_sidedef as usize]);

                let back_side = {
                    l.back_sidedef
                        .map(|index| DPtr::new(&mut self.sidedefs[index as usize]))
                };

                let back_sector = {
                    l.back_sidedef
                        .map(|index| self.sidedefs()[index as usize].sector.clone())
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
                    v1,
                    v2,
                    delta: Vec2::new(dx, dy),
                    flags: l.flags as u32,
                    special: l.special,
                    tag: l.sector_tag,
                    bbox: BBox::new(v1, v2),
                    slopetype: slope,
                    front_sidedef: front.clone(),
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
        info!("{}: Mapped linedefs to sectors", self.name);
        // TODO: iterate sector lines to find max bounding box for sector

        for sector in &mut self.sectors {
            set_sector_sound_origin(sector);
        }

        // Sector, Sidedef, Linedef, Seg all need to be preprocessed before
        // storing in level struct
        //
        // SEGS
        self.segments = wad
            .segment_iter(&self.name)
            .map(|s| {
                let v1 = vertexes[s.start_vertex as usize];
                let v2 = vertexes[s.end_vertex as usize];

                let linedef = &mut self.linedefs[s.linedef as usize];

                let frontsector;
                let backsector;
                let side;
                // The front and back sectors interchange depending on BSP
                if s.side == 0 {
                    side = linedef.front_sidedef.clone();
                    frontsector = linedef.frontsector.clone();
                    backsector = linedef.backsector.clone();
                } else {
                    side = linedef.back_sidedef.as_ref().unwrap().clone();
                    frontsector = linedef.backsector.as_ref().unwrap().clone();
                    backsector = Some(linedef.frontsector.clone());
                };

                let angle = bam_to_radian((s.angle as u32) << 16);

                Segment {
                    v1,
                    v2,
                    offset: s.offset as f32,
                    angle: Angle::new(angle),
                    sidedef: side,
                    linedef: DPtr::new(linedef),
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
        // BOXTOP = 0
        // BOXBOT = 1
        // BOXLEFT = 2
        // BOXRIGHT = 3
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

        self.start_node = (self.nodes.len() - 1) as u16;
        self.set_extents();
        self.set_scale();
        self.fix_vertices();
    }

    /// Get a raw pointer to the subsector a point is in. This is mostly used to update
    /// an objects location so that sector effects can work on objects.
    ///
    /// Doom function name  `R_PointInSubsector`
    pub fn point_in_subsector_raw(&mut self, point: Vec2) -> *mut SubSector {
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

    pub fn point_in_subsector(&mut self, point: Vec2) -> &SubSector {
        let mut node_id = self.start_node();
        let mut node;
        let mut side;

        while node_id & IS_SSECTOR_MASK == 0 {
            node = &self.get_nodes()[node_id as usize];
            side = node.point_on_side(&point);
            node_id = node.child_index[side];
        }

        &self.subsectors[(node_id ^ IS_SSECTOR_MASK) as usize]
    }

    /// Remove slime trails. killough 10/98
    ///
    // Slime trails are inherent to Doom's coordinate system -- i.e. there is
    /// nothing that a node builder can do to prevent slime trails ALL of the time,
    /// because it's a product of the integer coordinate system, and just because
    /// two lines pass through exact integer coordinates, doesn't necessarily mean
    /// that they will intersect at integer coordinates. Thus we must allow for
    /// fractional coordinates if we are to be able to split segs with node lines,
    /// as a node builder must do when creating a BSP tree.
    ///
    /// A wad file does not allow fractional coordinates, so node builders are out
    /// of luck except that they can try to limit the number of splits (they might
    /// also be able to detect the degree of roundoff error and try to avoid splits
    /// with a high degree of roundoff error). But we can use fractional coordinates
    /// here, inside the engine. It's like the difference between square inches and
    /// square miles, in terms of granularity.
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
    /// Segs corresponding to orthogonal linedefs (exactly vertical or horizontal
    /// linedefs), which comprise at least half of all linedefs in most wads, don't
    /// need to be considered, because they almost never contribute to slime trails
    /// (because then any roundoff error is parallel to the linedef, which doesn't
    /// cause slime). Skipping simple orthogonal lines lets the code finish quicker.
    ///
    /// Please note: This section of code is not interchangable with TeamTNT's
    /// code which attempts to fix the same problem.
    ///
    /// Firelines (TM) is a Rezistered Trademark of MBF Productions
    fn fix_vertices(&mut self) {
        let start = Instant::now();
        let mut vertexes: HashMap<String, Vec2> = HashMap::with_capacity(self.segments.len() * 2);

        for seg in self.segments.iter_mut() {
            let linedef = seg.linedef.as_mut();
            // Commented this part out because cycles are now very very cheap
            // if linedef.delta.x != 0.0 && linedef.delta.y != 0.0 {
            let mut step2 = false;
            let mut vertex = &mut seg.v1;

            loop {
                if let Some(v) = vertexes.get_mut(&format!("{vertex}")) {
                    vertex.x = v.x;
                    vertex.y = v.y;
                } else {
                    let dx2 = linedef.delta.x * linedef.delta.x;
                    let dy2 = linedef.delta.y * linedef.delta.y;
                    let dxy = linedef.delta.x * linedef.delta.y;
                    let s = dx2 + dy2;
                    let x0 = vertex.x;
                    let y0 = vertex.y;
                    let x1 = linedef.v1.x;
                    let y1 = linedef.v1.y;
                    vertex.x = (dx2 * x0 + dy2 * x1 + dxy * (y0 - y1)) / s;
                    vertex.y = (dy2 * y0 + dx2 * y1 + dxy * (x0 - x1)) / s;
                    vertexes.insert(format!("{vertex}"), *vertex);
                }

                if step2 {
                    // Also set the v2 linedef
                    // linedef.v2.x = vertex.x;
                    // linedef.v2.y = vertex.y;
                    break;
                }
                // Linedef are not used for rendering
                // linedef.v1.x = vertex.x;
                // linedef.v1.y = vertex.y;
                vertex = &mut seg.v2;
                step2 = true;
            }
            // }
        }

        let end = Instant::now();
        info!(
            "{}: Fixed map vertices, took: {:#?}",
            self.name,
            end.duration_since(start)
        );
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

#[derive(Debug, Default)]
pub struct BSPTrace {
    radius: f32,
    pub origin: Vec2,
    origin_left: Vec2,
    origin_right: Vec2,
    pub endpoint: Vec2,
    endpoint_left: Vec2,
    endpoint_right: Vec2,
    pub nodes: Vec<u16>,
    /// If it is a line_trace. If not then it is a radius trace.
    trace_type: BSPTraceType,
}

impl BSPTrace {
    /// Setup the trace for a line trace. Use `find_line_intercepts()` to find all
    /// intersections.
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
            nodes: Vec::with_capacity(20),
            trace_type: BSPTraceType::Line,
        }
    }

    pub fn new_radius(origin: Vec2, radius: f32) -> Self {
        Self {
            origin,
            radius,
            trace_type: BSPTraceType::Radius,
            ..Self::default()
        }
    }

    /// Do the BSP trace. The type of trace done is determined by if the trace was set up
    /// with `BSPTrace::new_line` or `BSPTrace::new_radius`.
    pub fn find_intercepts(&mut self, node_id: u16, map: &MapData, count: &mut u32) {
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
    fn find_line_inner(&mut self, node_id: u16, map: &MapData, count: &mut u32) {
        *count += 1;
        if node_id & IS_SSECTOR_MASK != 0 {
            let node = node_id & !IS_SSECTOR_MASK;
            #[cfg(Debug)]
            if (node as usize) >= map.nodes.len() {
                error!(
                    "Node {} masked to {} was out of bounds",
                    node_id,
                    node_id & !IS_SSECTOR_MASK
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
            self.find_line_inner(node.child_index[side1], map, count);
            self.find_line_inner(node.child_index[side2], map, count);
        } else if self.radius > 1.0 {
            let side_l1 = node.point_on_side(&self.origin_left);
            let side_l2 = node.point_on_side(&self.endpoint_left);

            let side_r1 = node.point_on_side(&self.origin_right);
            let side_r2 = node.point_on_side(&self.endpoint_right);

            if side_l1 != side_l2 {
                self.find_line_inner(node.child_index[side_l1], map, count);
                self.find_line_inner(node.child_index[side_l2], map, count);
            } else if side_r1 != side_r2 {
                self.find_line_inner(node.child_index[side_r1], map, count);
                self.find_line_inner(node.child_index[side_r2], map, count);
            } else {
                self.find_line_inner(node.child_index[side1], map, count);
            }
        } else {
            self.find_line_inner(node.child_index[side1], map, count);
        }
    }

    fn find_radius_inner(&mut self, node_id: u16, map: &MapData, count: &mut u32) {
        *count += 1;

        if node_id & IS_SSECTOR_MASK == IS_SSECTOR_MASK {
            let node = node_id & !IS_SSECTOR_MASK;
            #[cfg(Debug)]
            if (node as usize) >= map.nodes.len() {
                error!(
                    "Node {} masked to {} was out of bounds",
                    node_id,
                    node_id & !IS_SSECTOR_MASK
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
            self.find_radius_inner(node.child_index[side], map, count);
            self.find_radius_inner(node.child_index[other], map, count);
        } else {
            self.find_radius_inner(node.child_index[side], map, count);
        }
    }

    /// List of indexes to subsectors the trace intercepted
    pub fn intercepted_subsectors(&self) -> &[u16] {
        &self.nodes
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        angle::Angle,
        level::map_data::{BSPTrace, MapData},
        PicData,
    };
    use glam::Vec2;
    use std::f32::consts::{FRAC_PI_2, PI};
    use wad::WadData;

    #[test]
    fn test_tracing_bsp() {
        let wad = WadData::new("../doom1.wad".into());
        let mut map = MapData::new("E1M1".to_owned());
        map.load(&PicData::default(), &wad);
        let origin = Vec2::new(710.0, -3400.0); // left corner from start
        let endpoint = Vec2::new(710.0, -3000.0); // 3 sectors up

        // let origin = Vec2::new(1056.0, -3616.0); // player start
        // let endpoint = Vec2::new(1088.0, -2914.0); // corpse ahead, 10?
        //let endpoint = Vec2::new(1340.0, -2884.0); // ?
        //let endpoint = Vec2::new(2912.0, -2816.0);

        let mut bsp_trace = BSPTrace::new_line(origin, endpoint, 1.0);
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
                bsp_trace.find_line_inner(map.start_node, &map, &mut count);

                // Sector the starting vector is in. 3 segs attached
                let x = bsp_trace.intercepted_subsectors().first().unwrap();
                let start = sub_sect[*x as usize].start_seg as usize;

                // Bottom horizontal line
                assert_eq!(segs[start].v1.x, 832.0);
                assert_eq!(segs[start].v1.y, -3552.0);
                assert_eq!(segs[start].v2.x, 704.0);
                assert_eq!(segs[start].v2.y, -3552.0);
                // Left side of the pillar
                assert_eq!(segs[start + 1].v1.x, 896.0);
                assert_eq!(segs[start + 1].v1.y, -3360.0);
                assert_eq!(segs[start + 1].v2.x, 896.0);
                assert_eq!(segs[start + 1].v2.y, -3392.0);
                // Left wall
                assert_eq!(segs[start + 2].v1.x, 704.0);
                assert_eq!(segs[start + 2].v1.y, -3552.0);
                assert_eq!(segs[start + 2].v2.x, 704.0);
                assert_eq!(segs[start + 2].v2.y, -3360.0);

                // Last sector directly above starting vector
                let x = bsp_trace.intercepted_subsectors().last().unwrap();
                let start = sub_sect[*x as usize].start_seg as usize;

                assert_eq!(segs[start].v1.x, 896.0);
                assert_eq!(segs[start].v1.y, -3072.0);
                assert_eq!(segs[start].v2.x, 896.0);
                assert_eq!(segs[start].v2.y, -3104.0);
                assert_eq!(segs[start + 1].v1.x, 704.0);
                assert_eq!(segs[start + 1].v1.y, -3104.0);
                assert_eq!(segs[start + 1].v2.x, 704.0);
                assert_eq!(segs[start + 1].v2.y, -2944.0);
            }
        }
    }

    #[test]
    fn check_e1m1_things() {
        let wad = WadData::new("../doom1.wad".into());
        let mut map = MapData::new("E1M1".to_owned());
        map.load(&PicData::default(), &wad);

        let things = &map.things;
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
    #[allow(clippy::float_cmp)]
    fn check_e1m1_lump_pointers() {
        let wad = WadData::new("../doom1.wad".into());
        let mut map = MapData::new("E1M1".to_owned());
        map.load(&PicData::default(), &wad);

        let linedefs = map.linedefs;

        // Check links
        // LINEDEF->VERTEX
        assert_eq!(linedefs[2].v1.x as i32, 1088);
        assert_eq!(linedefs[2].v2.x as i32, 1088);
        // // LINEDEF->SIDEDEF
        // assert_eq!(linedefs[2].front_sidedef.midtexture, "LITE3");
        // // LINEDEF->SIDEDEF->SECTOR
        // assert_eq!(linedefs[2].front_sidedef.sector.floorpic, "FLOOR4_8");
        // // LINEDEF->SIDEDEF->SECTOR
        assert_eq!(linedefs[2].front_sidedef.sector.ceilingheight, 72.0);

        let segments = map.segments;
        // SEGMENT->VERTEX
        assert_eq!(segments[0].v1.x as i32, 1552);
        assert_eq!(segments[0].v2.x as i32, 1552);
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
        map.load(&PicData::default(), &wad);

        let linedefs = map.linedefs();
        assert_eq!(linedefs[0].v1.x as i32, 1088);
        assert_eq!(linedefs[0].v2.x as i32, 1024);
        assert_eq!(linedefs[2].v1.x as i32, 1088);
        assert_eq!(linedefs[2].v2.x as i32, 1088);

        assert_eq!(linedefs[474].v1.x as i32, 3536);
        assert_eq!(linedefs[474].v2.x as i32, 3520);
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
        map.load(&PicData::default(), &wad);

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
        map.load(&PicData::default(), &wad);

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
        map.load(&PicData::default(), &wad);

        let segments = map.segments();
        assert_eq!(segments[0].v1.x as i32, 1552);
        assert_eq!(segments[0].v2.x as i32, 1552);
        assert_eq!(segments[731].v1.x as i32, 3040);
        assert_eq!(segments[731].v2.x as i32, 2976);
        assert_eq!(segments[0].angle, Angle::new(FRAC_PI_2));
        assert_eq!(segments[0].offset, 0.0);

        assert_eq!(segments[731].angle, Angle::new(PI));
        assert_eq!(segments[731].offset, 0.0);

        let subsectors = map.subsectors();
        assert_eq!(subsectors[0].seg_count, 4);
        assert_eq!(subsectors[124].seg_count, 3);
        assert_eq!(subsectors[236].seg_count, 4);
        //assert_eq!(subsectors[0].start_seg.start_vertex.x as i32, 1552);
        //assert_eq!(subsectors[124].start_seg.start_vertex.x as i32, 472);
        //assert_eq!(subsectors[236].start_seg.start_vertex.x as i32, 3040);
    }

    #[test]
    fn find_vertex_using_bsptree() {
        let wad = WadData::new("../doom1.wad".into());
        let mut map = MapData::new("E1M1".to_owned());
        map.load(&PicData::default(), &wad);

        // The actual location of THING0
        let player = Vec2::new(1056.0, -3616.0);
        let subsector = map.point_in_subsector_raw(player);
        //assert_eq!(subsector_id, Some(103));
        unsafe {
            assert_eq!((*subsector).seg_count, 5);
            assert_eq!((*subsector).start_seg, 305);
        }
    }
}
