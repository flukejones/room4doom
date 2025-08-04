#[cfg(feature = "hprof")]
use coarse_prof::profile;

use crate::level::map_defs::{LineDef, Node, Sector, SubSector};
use crate::level::triangulation::carve_subsector_polygon;
use crate::{DivLine, LineDefFlags, PicData, Segment};
use glam::{Vec2, Vec3};
use std::collections::HashMap;
#[allow(unused_imports)]
use std::io::Write;

const IS_SUBSECTOR_MASK: u32 = 0x8000_0000;
const QUANT_EPSILON: f32 = 0.1;
const HEIGHT_EPSILON: f32 = 0.1;

#[derive(Default, Debug, Clone, Copy, PartialEq)]
pub enum MovementType {
    Floor,
    Ceiling,
    #[default]
    None,
}

fn get_movement_type(line_special: i16) -> Option<MovementType> {
    match line_special {
        1..=4
        | 6
        | 16
        | 25
        | 26..=29
        | 31..=34
        | 40..=44
        | 46
        | 49..=50
        | 61
        | 63
        | 72..=73
        | 75..=77
        | 86
        | 90
        | 103
        | 105..=116
        | 117..=118
        | 141 => Some(MovementType::Ceiling),
        5
        | 7..=8
        | 10
        | 14..=15
        | 18..=23
        | 30
        | 36..=38
        | 53
        | 55..=56
        | 59..=60
        | 62
        | 64..=71
        | 82..=84
        | 87..=88
        | 91..=95
        | 96
        | 98
        | 100..=102
        | 119..=123
        | 127
        | 128..=132
        | 140
        | 45 => Some(MovementType::Floor),
        _ => None,
    }
}

/// Create mapping of sector tags to movement types from linedefs so we don't need to iter
/// over all lines every time we check a subsector
fn create_sector_tag_movement_mapping(
    linedefs: &[LineDef],
    sectors: &[Sector],
) -> HashMap<usize, MovementType> {
    let mut mapping = HashMap::new();

    for linedef in linedefs {
        if let Some(movement_type) = get_movement_type(linedef.special) {
            if linedef.tag != 0 {
                // todo: cache the tags on first walk through
                for sector in sectors {
                    if sector.tag == linedef.tag {
                        let num = sector.num as usize;
                        if movement_type != MovementType::None && !mapping.contains_key(&num) {
                            mapping.insert(num, movement_type);
                            // break;
                        }
                    }
                }
            } else {
                let num = linedef.frontsector.num as usize;
                if movement_type != MovementType::None && !mapping.contains_key(&num) {
                    mapping.insert(num, movement_type);
                }
            }
        }
    }

    mapping
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WallType {
    Upper,
    Lower,
    Middle,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WallTexPin {
    UnpegTop = 0b0001,
    UnpegBottom = 0b0010,
    UnpegBoth = 0b0011,
    None = 0b1000,
}

impl From<u32> for WallTexPin {
    fn from(flags: u32) -> Self {
        if flags & LineDefFlags::UnpegBottom as u32 != 0
            && flags & LineDefFlags::UnpegTop as u32 != 0
        {
            WallTexPin::UnpegBoth
        } else if flags & LineDefFlags::UnpegBottom as u32 != 0 {
            WallTexPin::UnpegBottom
        } else if flags & LineDefFlags::UnpegTop as u32 != 0 {
            WallTexPin::UnpegTop
        } else {
            WallTexPin::None
        }
    }
}

#[derive(Debug, Clone)]
pub struct Node3D {
    pub xy: Vec2,
    pub delta: Vec2,
    pub bboxes: [AABB; 2],
    pub children: [u32; 2],
    pub aabb: AABB,
}

impl Node3D {
    pub const fn point_on_side(&self, point: Vec2) -> usize {
        let dx = point.x - self.xy.x;
        let dy = point.y - self.xy.y;
        if (dx * self.delta.y - dy * self.delta.x) <= 0.0 {
            0
        } else {
            1
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AABB {
    pub min: Vec3,
    pub max: Vec3,
}

impl AABB {
    fn new() -> Self {
        Self {
            min: Vec3::new(f32::MAX, f32::MAX, f32::MAX),
            max: Vec3::new(f32::MIN, f32::MIN, f32::MIN),
        }
    }

    fn expand_to_include_point(&mut self, point: Vec3) {
        self.min = self.min.min(point);
        self.max = self.max.max(point);
    }

    fn expand_to_include_aabb(&mut self, other: &AABB) {
        self.min = self.min.min(other.min);
        self.max = self.max.max(other.max);
    }
}

impl From<&[Vec3; 2]> for AABB {
    fn from(bbox: &[Vec3; 2]) -> Self {
        Self {
            min: bbox[0],
            max: bbox[1],
        }
    }
}

#[derive(Debug, Clone)]
pub struct BSPLeaf3D {
    pub polygons: Vec<SurfacePolygon>,
    pub aabb: AABB,
    pub floor_polygons: Vec<usize>,
    pub ceiling_polygons: Vec<usize>,
}

#[derive(Debug, Clone)]
pub enum SurfaceKind {
    Vertical {
        texture: Option<usize>,
        tex_x_offset: f32,
        tex_y_offset: f32,
        texture_direction: Vec3,
        wall_type: WallType,
        wall_tex_pin: WallTexPin,
        /// For texture alignment
        front_ceiling_z: f32,
    },
    Horizontal {
        texture: usize,
        tex_cos: f32,
        tex_sin: f32,
    },
}

#[derive(Debug, Clone)]
pub struct SurfacePolygon {
    pub sector_id: usize,
    pub surface_kind: SurfaceKind,
    pub vertices: Vec<usize>,
    pub normal: Vec3,
    pub aabb: AABB,
    pub moves: bool,
}

impl SurfacePolygon {
    fn new(
        sector_id: usize,
        surface_kind: SurfaceKind,
        vertices: Vec<usize>,
        normal: Vec3,
        vertex_positions: &[Vec3],
        moves: bool,
    ) -> Self {
        let mut aabb = AABB::new();
        for &vertex_idx in &vertices {
            aabb.expand_to_include_point(vertex_positions[vertex_idx]);
        }

        Self {
            sector_id,
            surface_kind,
            vertices,
            normal,
            aabb,
            moves,
        }
    }

    pub fn is_facing_point(&self, point: Vec3, vertex_positions: &[Vec3]) -> bool {
        let computed_normal = if self.moves {
            unsafe {
                let p0 = vertex_positions.get_unchecked(self.vertices[0]);
                let p1 = vertex_positions.get_unchecked(self.vertices[1]);
                let p2 = vertex_positions.get_unchecked(self.vertices[2]);
                let edge1 = p1 - p0;
                let edge2 = p2 - p0;
                edge1.cross(edge2).normalize()
            }
        } else {
            self.normal
        };

        let first_vertex_idx = unsafe { *self.vertices.get_unchecked(0) };
        let first_vertex = vertex_positions[first_vertex_idx];
        let view_vector = (point - first_vertex).normalize_or_zero();
        let dot_product = computed_normal.dot(view_vector);
        dot_product.is_sign_positive() || dot_product.is_nan()
    }
}

#[derive(Hash, PartialEq, Eq, Clone, Copy)]
struct QuantizedVec3 {
    x: i32,
    y: i32,
    z: i32,
}

impl QuantizedVec3 {
    fn from_vec3(v: Vec3, precision: f32) -> Self {
        Self {
            x: (v.x / precision).round() as i32,
            y: (v.y / precision).round() as i32,
            z: (v.z / precision).round() as i32,
        }
    }
}

/// Which wall type is the vertex for. When adding walls we have to check
/// if the vertex is allowed to be used for the required position if
/// the wall height ix zero. This is because it's impossible to know which vertex
/// in the same position is for what.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
enum VertexMappedTo {
    /// Middle wall bottom, all lower wall parts, floor
    Lower,
    /// Middle wall top, all upper wall parts, ceiling
    Upper,
    /// All vertex in and on edge of a moving floor sector/subsector must be
    /// marked with `LowerMoving`
    /// A zeroheight wall on floor space should mark its bottom vertices:
    /// - if normal faces in `LowerMoving`, else `Lower`
    /// - if normal faces out `Lower`, else `LowerMoving`
    LowerMoving,
    /// All vertex in and on edge of a moving ceiling sector/subsector must be
    /// marked with `UpperMoving`
    /// A zeroheight wall on ceiling space should mark its bottom vertices:
    /// - if normal faces in `Upper`, else `UpperMoving`
    /// - if normal faces out `UpperMoving`, else `Upper`
    UpperMoving,
    /// The vertex hasn't been assigned to anything
    Unused,
}

/// Track what vertexes are for, and where to find them
struct VertexTracking {
    /// Track what the vertex was used for, this determines what other walls are allowed to use
    /// which matters if there are more than one in a 3D position such as for zero-height walls.
    /// The index used to address this array is the same as that in global vertex array, which
    /// means both insert operations must be synced
    vertex_type: Vec<VertexMappedTo>,
    /// Tracking to enable faster vertex lookups instead of always iterating.
    vertex_map: HashMap<(QuantizedVec3, VertexMappedTo), usize>,
    precision: f32,
}

impl VertexTracking {
    fn new(segment_count: usize) -> Self {
        Self {
            vertex_type: Vec::with_capacity(segment_count * 2),
            vertex_map: HashMap::with_capacity(segment_count * 2),
            precision: QUANT_EPSILON,
        }
    }

    /// Get the index number in global array of the vertex the mapping is allowed to use.
    /// Returns None if no vertex exists.
    /// Function can be used to check if contains vertex.
    fn get_vertex_index(&mut self, vertex: Vec3, mapping: VertexMappedTo) -> Option<usize> {
        let quantized = QuantizedVec3::from_vec3(vertex, self.precision);
        self.vertex_map.get(&(quantized, mapping)).copied()
    }

    /// Insert the vertex mapping. Creates a quick lookup.
    fn insert_vertex(&mut self, vertex: Vec3, mapping: VertexMappedTo, index: usize) {
        let quantized = QuantizedVec3::from_vec3(vertex, self.precision);
        self.vertex_map.insert((quantized, mapping), index);
        if index >= self.vertex_type.len() {
            self.vertex_type.resize(index + 1, VertexMappedTo::Unused);
        }
        self.vertex_type[index] = mapping;
    }
}

pub struct BSP3D {
    nodes: Vec<Node3D>,
    pub subsector_leaves: Vec<BSPLeaf3D>,
    root_node: u32,
    pub vertices: Vec<Vec3>,
    pub(crate) sector_subsectors: Vec<Vec<usize>>,
}

impl BSP3D {
    pub fn new(
        root_node: u32,
        nodes: &[Node],
        subsectors: &[SubSector],
        segments: &[Segment],
        sectors: &[Sector],
        linedefs: &[LineDef],
        pic_data: &PicData,
    ) -> Self {
        #[cfg(feature = "hprof")]
        profile!("BSP3D::new");

        let mut bsp3d = Self {
            nodes: Vec::new(),
            subsector_leaves: Vec::new(),
            root_node,
            vertices: Vec::new(),
            sector_subsectors: vec![Vec::new(); sectors.len()],
        };

        // Create sector tag to movement type mapping
        let sector_movement_map = create_sector_tag_movement_mapping(linedefs, sectors);
        let mut vertex_tracking = VertexTracking::new(segments.len());

        bsp3d.initialize_nodes(nodes, sectors);
        bsp3d.initialize_subsectors(subsectors);
        bsp3d.build_sector_subsector_mapping(subsectors, segments, sectors);
        // Initialize subsector leaves for wall and floor/ceiling storage
        bsp3d.subsector_leaves = vec![BSPLeaf3D::default(); subsectors.len()];

        // Use BSP traversal to generate walls, floors, and ceiling polygons
        bsp3d.carve_polygons_recursive(
            nodes,
            subsectors,
            segments,
            linedefs,
            bsp3d.root_node,
            Vec::new(),
            pic_data,
            &sector_movement_map,
            &mut vertex_tracking,
        );

        bsp3d.update_all_aabbs();

        bsp3d
    }

    pub fn nodes(&self) -> &[Node3D] {
        &self.nodes
    }

    pub fn get_subsector_leaf(&self, subsector_id: usize) -> Option<&BSPLeaf3D> {
        self.subsector_leaves.get(subsector_id)
    }

    pub fn get_subsector_leaf_count(&self) -> usize {
        self.subsector_leaves.len()
    }

    pub fn root_node(&self) -> u32 {
        self.root_node
    }

    fn vertex_add(
        &mut self,
        vertex: Vec3,
        mapping: VertexMappedTo,
        vertex_tracking: &mut VertexTracking,
    ) -> usize {
        if let Some(existing_idx) = vertex_tracking.get_vertex_index(vertex, mapping) {
            existing_idx
        } else {
            let idx = self.vertices.len();
            self.vertices.push(vertex);
            vertex_tracking.insert_vertex(vertex, mapping, idx);
            idx
        }
    }

    #[inline(always)]
    pub fn vertex_get(&self, idx: usize) -> Vec3 {
        unsafe { *self.vertices.get_unchecked(idx) }
    }

    pub fn move_vertices(
        &mut self,
        sector_id: usize,
        movement_type: MovementType,
        new_height: f32,
    ) {
        let subsector_ids = self.sector_subsectors[sector_id].clone();

        for subsector_id in subsector_ids {
            let leaf = &self.subsector_leaves[subsector_id];
            let polygon_indices = match movement_type {
                MovementType::Floor => &leaf.floor_polygons,
                MovementType::Ceiling => &leaf.ceiling_polygons,
                MovementType::None => return,
            };

            for &polygon_idx in polygon_indices {
                let polygon = &leaf.polygons[polygon_idx];
                for &vertex_idx in &polygon.vertices {
                    self.vertices[vertex_idx].z = new_height;
                }
            }
        }

        self.update_affected_aabbs(sector_id);
    }

    fn initialize_nodes(&mut self, nodes: &[Node], sectors: &[Sector]) {
        self.nodes = nodes
            .iter()
            .map(|node| {
                let min_z = sectors
                    .iter()
                    .map(|s| s.floorheight)
                    .fold(f32::INFINITY, f32::min);
                let max_z = sectors
                    .iter()
                    .map(|s| s.ceilingheight)
                    .fold(f32::NEG_INFINITY, f32::max);

                Node3D {
                    xy: node.xy,
                    delta: node.delta,
                    bboxes: [
                        AABB {
                            min: Vec3::new(node.bboxes[0][0].x, node.bboxes[0][0].y, min_z),
                            max: Vec3::new(node.bboxes[0][1].x, node.bboxes[0][1].y, max_z),
                        },
                        AABB {
                            min: Vec3::new(node.bboxes[1][0].x, node.bboxes[1][0].y, min_z),
                            max: Vec3::new(node.bboxes[1][1].x, node.bboxes[1][1].y, max_z),
                        },
                    ],
                    children: node.children,
                    aabb: AABB::new(),
                }
            })
            .collect();
    }

    fn initialize_subsectors(&mut self, subsectors: &[SubSector]) {
        self.subsector_leaves = vec![BSPLeaf3D::default(); subsectors.len()];
    }

    fn build_sector_subsector_mapping(
        &mut self,
        subsectors: &[SubSector],
        segments: &[Segment],
        sectors: &[Sector],
    ) {
        for (subsector_id, subsector) in subsectors.iter().enumerate() {
            let segment = &segments[subsector.start_seg as usize];
            let sector_id = segment.frontsector.num as usize;
            if sector_id < sectors.len() {
                self.sector_subsectors[sector_id].push(subsector_id);
            }
        }
    }

    /// BSP traversal to collect dividing lines and generate polygons
    fn carve_polygons_recursive(
        &mut self,
        nodes: &[Node],
        subsectors: &[SubSector],
        segments: &[Segment],
        linedefs: &[LineDef],
        node_id: u32,
        divlines: Vec<DivLine>,
        pic_data: &PicData,
        sector_movement_map: &HashMap<usize, MovementType>,
        vertex_tracking: &mut VertexTracking,
    ) {
        #[cfg(feature = "hprof")]
        profile!("carve_polygons_recursive");

        if node_id & IS_SUBSECTOR_MASK != 0 {
            self.process_subsector_node(
                subsectors,
                segments,
                node_id,
                &divlines,
                vertex_tracking,
                sector_movement_map,
            );
        } else {
            self.process_internal_node(
                nodes,
                subsectors,
                segments,
                linedefs,
                node_id,
                divlines,
                pic_data,
                sector_movement_map,
                vertex_tracking,
            );
        }
    }

    fn process_subsector_node(
        &mut self,
        subsectors: &[SubSector],
        segments: &[Segment],
        node_id: u32,
        divlines: &[DivLine],
        vertex_tracking: &mut VertexTracking,
        sector_movement_map: &HashMap<usize, MovementType>,
    ) {
        let subsector_id = if node_id == u32::MAX {
            return;
        } else {
            (node_id & !IS_SUBSECTOR_MASK) as usize
        };

        if subsector_id < subsectors.len() {
            let subsector = &subsectors[subsector_id];
            let start_seg = subsector.start_seg as usize;
            let end_seg = start_seg + subsector.seg_count as usize;

            if let Some(subsector_segments) = segments.get(start_seg..end_seg) {
                // Process segments for walls
                for segment_idx in start_seg..end_seg {
                    let segment = &segments[segment_idx];
                    {
                        let this = &mut *self;
                        let front_subsector_id = segment.frontsector.num as usize;
                        let front_sector = &segment.frontsector;

                        // Check for back sector
                        if let Some(back_sector) = &segment.backsector {
                            this.create_two_sided_walls(
                                segment,
                                front_sector,
                                back_sector,
                                front_subsector_id,
                                vertex_tracking,
                                sector_movement_map,
                            );
                        } else {
                            this.create_one_sided_wall(
                                segment,
                                front_sector,
                                front_subsector_id,
                                vertex_tracking,
                                sector_movement_map,
                            );
                        }
                    };
                }

                // Process floors and ceilings
                let polygon = carve_subsector_polygon(
                    subsector_segments,
                    divlines,
                    &self.sector_subsectors,
                    segments,
                    subsectors,
                );
                self.create_floor_ceiling_polygons(
                    subsector_id,
                    subsector,
                    &polygon,
                    vertex_tracking,
                    sector_movement_map,
                );
            }
        }
    }

    fn create_two_sided_walls(
        &mut self,
        segment: &Segment,
        front_sector: &Sector,
        back_sector: &Sector,
        front_subsector_id: usize,
        vertex_tracking: &mut VertexTracking,
        sector_movement_map: &HashMap<usize, MovementType>,
    ) {
        // Upper wall: Create if toptexture exists and back ceiling is at or below front ceiling
        if segment.sidedef.toptexture.is_some()
            && back_sector.ceilingheight <= front_sector.ceilingheight
        {
            let texture = segment.sidedef.toptexture.unwrap();
            let bottom_height = back_sector.ceilingheight;
            let top_height = front_sector.ceilingheight;

            let wall_polygons = self.create_wall_quad_from_segment(
                segment,
                bottom_height,
                top_height,
                WallType::Upper,
                texture,
                front_subsector_id,
                vertex_tracking,
                sector_movement_map,
            );
            for wall_polygon in wall_polygons {
                self.subsector_leaves[front_subsector_id]
                    .polygons
                    .push(wall_polygon);
            }
        }

        // Lower wall: Create if bottomtexture exists and back floor is at or above front floor
        if segment.sidedef.bottomtexture.is_some()
            && back_sector.floorheight >= front_sector.floorheight
        {
            let texture = segment.sidedef.bottomtexture.unwrap();
            let bottom_height = front_sector.floorheight;
            let top_height = back_sector.floorheight;

            let wall_polygons = self.create_wall_quad_from_segment(
                segment,
                bottom_height,
                top_height,
                WallType::Lower,
                texture,
                front_sector.num as usize,
                vertex_tracking,
                sector_movement_map,
            );
            for wall_polygon in wall_polygons {
                self.subsector_leaves[front_subsector_id]
                    .polygons
                    .push(wall_polygon);
            }
        }

        // Middle wall: Create if midtexture exists
        if segment.sidedef.midtexture.is_some() {
            let texture = segment.sidedef.midtexture.unwrap();
            let bottom = front_sector.floorheight.max(back_sector.floorheight);
            let top = front_sector.ceilingheight.min(back_sector.ceilingheight);

            let wall_polygons = self.create_wall_quad_from_segment(
                segment,
                bottom,
                top,
                WallType::Middle,
                texture,
                front_sector.num as usize,
                vertex_tracking,
                sector_movement_map,
            );
            for wall_polygon in wall_polygons {
                self.subsector_leaves[front_subsector_id]
                    .polygons
                    .push(wall_polygon);
            }
        }
    }

    fn create_one_sided_wall(
        &mut self,
        segment: &Segment,
        front_sector: &Sector,
        front_subsector_id: usize,
        vertex_tracking: &mut VertexTracking,
        sector_movement_map: &HashMap<usize, MovementType>,
    ) {
        if segment.sidedef.midtexture.is_some() {
            let texture = segment.sidedef.midtexture.unwrap();
            let wall_polygons = self.create_wall_quad_from_segment(
                segment,
                front_sector.floorheight,
                front_sector.ceilingheight,
                WallType::Middle,
                texture,
                front_sector.num as usize,
                vertex_tracking,
                sector_movement_map,
            );
            for wall_polygon in wall_polygons {
                self.subsector_leaves[front_subsector_id]
                    .polygons
                    .push(wall_polygon);
            }
        }
    }

    fn create_wall_quad_from_segment(
        &mut self,
        segment: &Segment,
        bottom_height: f32,
        top_height: f32,
        wall_type: WallType,
        texture: usize,
        sector_id: usize,
        vertex_tracking: &mut VertexTracking,
        sector_movement_map: &HashMap<usize, MovementType>,
    ) -> Vec<SurfacePolygon> {
        let start_pos = *segment.v1;
        let end_pos = *segment.v2;
        let is_zero_height = (top_height - bottom_height).abs() <= HEIGHT_EPSILON;
        let mut backsector_is_ceil_mover = false;
        let mut backsector_is_floor_mover = false;
        let frontsector_is_ceil_mover = sector_movement_map
            .get(&(segment.frontsector.num as usize))
            .copied()
            .unwrap_or_default()
            == MovementType::Ceiling;
        if let Some(back) = &segment.backsector {
            backsector_is_ceil_mover = sector_movement_map
                .get(&(back.num as usize))
                .copied()
                .unwrap_or_default()
                == MovementType::Ceiling;
        }
        let frontsector_is_floor_mover = sector_movement_map
            .get(&(segment.frontsector.num as usize))
            .copied()
            .unwrap_or_default()
            == MovementType::Floor;
        if let Some(back) = &segment.backsector {
            backsector_is_floor_mover = sector_movement_map
                .get(&(back.num as usize))
                .copied()
                .unwrap_or_default()
                == MovementType::Floor;
        }
        let moves = frontsector_is_ceil_mover
            || frontsector_is_floor_mover
            || backsector_is_ceil_mover
            || backsector_is_floor_mover;

        let (bottom_start_pos, bottom_end_pos, top_start_pos, top_end_pos) = {
            // For zero-height walls, determine logical top/bottom based on wall type
            // Doom uses counter-clockwise winding when viewed from front
            match wall_type {
                WallType::Upper => {
                    // Handle self movers first
                    if backsector_is_ceil_mover
                        || (backsector_is_ceil_mover && frontsector_is_ceil_mover)
                    {
                        (
                            VertexMappedTo::UpperMoving,
                            VertexMappedTo::UpperMoving,
                            VertexMappedTo::Upper,
                            VertexMappedTo::Upper,
                        )
                    } else if backsector_is_ceil_mover {
                        (
                            VertexMappedTo::Upper,
                            VertexMappedTo::Upper,
                            VertexMappedTo::UpperMoving,
                            VertexMappedTo::UpperMoving,
                        )
                    } else {
                        (
                            VertexMappedTo::Upper,
                            VertexMappedTo::Upper,
                            VertexMappedTo::Upper,
                            VertexMappedTo::Upper,
                        )
                    }
                }
                WallType::Lower => {
                    if backsector_is_floor_mover
                        || (backsector_is_floor_mover && frontsector_is_floor_mover)
                    {
                        (
                            VertexMappedTo::Lower,
                            VertexMappedTo::Lower,
                            VertexMappedTo::LowerMoving,
                            VertexMappedTo::LowerMoving,
                            // TODO: order needs to be flipped for one line kind
                        )
                    } else if frontsector_is_floor_mover {
                        (
                            VertexMappedTo::LowerMoving,
                            VertexMappedTo::LowerMoving,
                            VertexMappedTo::Lower,
                            VertexMappedTo::Lower,
                        )
                    } else {
                        (
                            VertexMappedTo::Lower,
                            VertexMappedTo::Lower,
                            VertexMappedTo::Lower,
                            VertexMappedTo::Lower,
                        )
                    }
                }
                WallType::Middle => {
                    if frontsector_is_floor_mover {
                        (
                            VertexMappedTo::LowerMoving,
                            VertexMappedTo::LowerMoving,
                            VertexMappedTo::Upper,
                            VertexMappedTo::Upper,
                        )
                    } else if frontsector_is_ceil_mover {
                        (
                            VertexMappedTo::Lower,
                            VertexMappedTo::Lower,
                            VertexMappedTo::UpperMoving,
                            VertexMappedTo::UpperMoving,
                        )
                    } else {
                        (
                            VertexMappedTo::Lower,
                            VertexMappedTo::Lower,
                            VertexMappedTo::Upper,
                            VertexMappedTo::Upper,
                        )
                    }
                }
            }
        };

        let bottom_start = self.vertex_add(
            Vec3::new(start_pos.x, start_pos.y, bottom_height),
            bottom_start_pos,
            vertex_tracking,
        );
        let bottom_end = self.vertex_add(
            Vec3::new(end_pos.x, end_pos.y, bottom_height),
            bottom_end_pos,
            vertex_tracking,
        );
        let top_start = self.vertex_add(
            Vec3::new(start_pos.x, start_pos.y, top_height),
            top_start_pos,
            vertex_tracking,
        );
        let top_end = self.vertex_add(
            Vec3::new(end_pos.x, end_pos.y, top_height),
            top_end_pos,
            vertex_tracking,
        );

        let wall_direction = (end_pos - start_pos).normalize();
        let normal = if is_zero_height {
            Vec3::new(wall_direction.y, -wall_direction.x, 0.0)
        } else {
            let v0 = self.vertices[bottom_start];
            let v1 = self.vertices[bottom_end];
            let v2 = self.vertices[top_start];
            let edge1 = v1 - v0;
            let edge2 = v2 - v0;
            edge1.cross(edge2).normalize()
        };

        let texture_direction = wall_direction.y.atan2(wall_direction.x);
        let texture_direction = Vec3::new(texture_direction.cos(), texture_direction.sin(), 0.0);

        let surface_kind = SurfaceKind::Vertical {
            texture: Some(texture),
            tex_x_offset: segment.sidedef.textureoffset + segment.offset,
            tex_y_offset: segment.sidedef.rowoffset,
            texture_direction,
            wall_type,
            wall_tex_pin: WallTexPin::from(segment.linedef.flags),
            front_ceiling_z: segment.frontsector.ceilingheight,
        };

        let triangle1 = SurfacePolygon::new(
            sector_id,
            surface_kind.clone(),
            vec![bottom_start, bottom_end, top_start],
            normal,
            &self.vertices,
            moves,
        );

        let triangle2 = SurfacePolygon::new(
            sector_id,
            surface_kind,
            vec![top_start, bottom_end, top_end],
            normal,
            &self.vertices,
            moves,
        );

        vec![triangle1, triangle2]
    }

    fn process_internal_node(
        &mut self,
        nodes: &[Node],
        subsectors: &[SubSector],
        segments: &[Segment],
        linedefs: &[LineDef],
        node_id: u32,
        divlines: Vec<DivLine>,
        pic_data: &PicData,
        sector_movement_map: &HashMap<usize, MovementType>,
        vertex_tracking: &mut VertexTracking,
    ) {
        if let Some(node) = nodes.get(node_id as usize) {
            let node_divline = DivLine::from_node(node);

            // Process right child with original divline
            let mut right_divlines = divlines.clone();
            right_divlines.push(node_divline);
            self.carve_polygons_recursive(
                nodes,
                subsectors,
                segments,
                linedefs,
                node.children[0],
                right_divlines,
                pic_data,
                sector_movement_map,
                vertex_tracking,
            );

            // Process left child with reversed divline
            let mut left_divlines = divlines;
            let mut reversed_divline = node_divline;
            reversed_divline.dx = -reversed_divline.dx;
            reversed_divline.dy = -reversed_divline.dy;
            left_divlines.push(reversed_divline);
            self.carve_polygons_recursive(
                nodes,
                subsectors,
                segments,
                linedefs,
                node.children[1],
                left_divlines,
                pic_data,
                sector_movement_map,
                vertex_tracking,
            );
        }
    }

    fn create_floor_ceiling_polygons(
        &mut self,
        subsector_id: usize,
        subsector: &SubSector,
        polygon: &[Vec2],
        vertex_tracking: &mut VertexTracking,
        sector_movement_map: &HashMap<usize, MovementType>,
    ) {
        if polygon.len() < 3 {
            return;
        }

        let movement_type = sector_movement_map
            .get(&(subsector.sector.num as usize))
            .copied()
            .unwrap_or_default();

        // Generate floor and ceiling polygons using triangulation
        for i in 1..polygon.len() - 1 {
            // Floor polygon with vertex replacement
            let floor_vertices_2d = [polygon[0], polygon[i + 1], polygon[i]];
            let floor_vertices = self.get_polygon_vertices_index(
                &floor_vertices_2d,
                subsector.sector.floorheight,
                if movement_type == MovementType::Floor {
                    VertexMappedTo::LowerMoving
                } else {
                    VertexMappedTo::Lower
                },
                vertex_tracking,
            );

            let floor_polygon = SurfacePolygon::new(
                subsector.sector.num as usize,
                self.create_horizontal_surface_kind(subsector.sector.floorpic),
                floor_vertices,
                Vec3::new(0.0, 0.0, 1.0),
                &self.vertices,
                movement_type == MovementType::Floor,
            );

            let floor_polygon_index = self.subsector_leaves[subsector_id].polygons.len();
            self.subsector_leaves[subsector_id]
                .polygons
                .push(floor_polygon);
            self.subsector_leaves[subsector_id]
                .floor_polygons
                .push(floor_polygon_index);

            // Ceiling polygon with vertex replacement
            let ceiling_vertices_2d = [polygon[i], polygon[i + 1], polygon[0]];
            let ceiling_vertices = self.get_polygon_vertices_index(
                &ceiling_vertices_2d,
                subsector.sector.ceilingheight,
                if movement_type == MovementType::Ceiling {
                    VertexMappedTo::UpperMoving
                } else {
                    VertexMappedTo::Upper
                },
                vertex_tracking,
            );

            let ceiling_polygon = SurfacePolygon::new(
                subsector.sector.num as usize,
                self.create_horizontal_surface_kind(subsector.sector.ceilingpic),
                ceiling_vertices,
                Vec3::new(0.0, 0.0, -1.0),
                &self.vertices,
                movement_type == MovementType::Ceiling,
            );

            let ceiling_polygon_index = self.subsector_leaves[subsector_id].polygons.len();
            self.subsector_leaves[subsector_id]
                .polygons
                .push(ceiling_polygon);
            self.subsector_leaves[subsector_id]
                .ceiling_polygons
                .push(ceiling_polygon_index);
        }
    }

    fn get_polygon_vertices_index(
        &mut self,
        vertices_2d: &[Vec2],
        height: f32,
        mapping: VertexMappedTo,
        vertex_tracking: &mut VertexTracking,
    ) -> Vec<usize> {
        let mut vertex_indices = Vec::new();

        for &vertex_2d in vertices_2d {
            let vertex_idx = self.vertex_add(
                Vec3::new(vertex_2d.x, vertex_2d.y, height),
                mapping,
                vertex_tracking,
            );
            vertex_indices.push(vertex_idx);
        }

        vertex_indices
    }

    fn update_affected_aabbs(&mut self, sector_id: usize) {
        let subsector_ids = self.sector_subsectors[sector_id].clone();

        for subsector_id in subsector_ids {
            let mut aabb = AABB::new();
            let leaf = &self.subsector_leaves[subsector_id];

            for polygon in &leaf.polygons {
                for &vertex_idx in &polygon.vertices {
                    aabb.expand_to_include_point(self.vertices[vertex_idx]);
                }
            }

            for &polygon_idx in &leaf.floor_polygons {
                let polygon = &leaf.polygons[polygon_idx];
                for &vertex_idx in &polygon.vertices {
                    aabb.expand_to_include_point(self.vertices[vertex_idx]);
                }
            }

            for &polygon_idx in &leaf.ceiling_polygons {
                let polygon = &leaf.polygons[polygon_idx];
                for &vertex_idx in &polygon.vertices {
                    aabb.expand_to_include_point(self.vertices[vertex_idx]);
                }
            }

            self.subsector_leaves[subsector_id].aabb = aabb;
        }
    }

    fn update_all_aabbs(&mut self) {
        for subsector_id in 0..self.subsector_leaves.len() {
            let mut aabb = AABB::new();
            let leaf = &self.subsector_leaves[subsector_id];

            for polygon in &leaf.polygons {
                for &vertex_idx in &polygon.vertices {
                    aabb.expand_to_include_point(self.vertices[vertex_idx]);
                }
            }

            for &polygon_idx in &leaf.floor_polygons {
                let polygon = &leaf.polygons[polygon_idx];
                for &vertex_idx in &polygon.vertices {
                    aabb.expand_to_include_point(self.vertices[vertex_idx]);
                }
            }

            for &polygon_idx in &leaf.ceiling_polygons {
                let polygon = &leaf.polygons[polygon_idx];
                for &vertex_idx in &polygon.vertices {
                    aabb.expand_to_include_point(self.vertices[vertex_idx]);
                }
            }

            self.subsector_leaves[subsector_id].aabb = aabb;
        }

        self.update_node_aabbs_recursive(self.root_node);
    }

    fn create_horizontal_surface_kind(&self, texture: usize) -> SurfaceKind {
        const TEXTURE_DIRECTION: f32 = std::f32::consts::PI / 2.0;
        SurfaceKind::Horizontal {
            texture,
            tex_cos: TEXTURE_DIRECTION.cos(),
            tex_sin: TEXTURE_DIRECTION.sin(),
        }
    }

    fn update_node_aabbs_recursive(&mut self, node_id: u32) {
        if node_id & IS_SUBSECTOR_MASK != 0 {
            return;
        }

        let node_idx = node_id as usize;
        if node_idx >= self.nodes.len() {
            return;
        }

        let children = self.nodes[node_idx].children;
        let mut combined_aabb = AABB::new();
        let mut has_valid_aabb = false;

        for &child_id in &children {
            if child_id & IS_SUBSECTOR_MASK != 0 {
                let subsector_id = (child_id & !IS_SUBSECTOR_MASK) as usize;
                let leaf = &self.subsector_leaves[subsector_id];
                combined_aabb.expand_to_include_aabb(&leaf.aabb);
                has_valid_aabb = true;
            } else {
                self.update_node_aabbs_recursive(child_id);
                if let Some(child_aabb) = self.nodes.get(child_id as usize).map(|n| &n.aabb) {
                    combined_aabb.expand_to_include_aabb(child_aabb);
                    has_valid_aabb = true;
                }
            }
        }
        //Use direct addressing on arrays, not `.get()`
        if has_valid_aabb {
            self.nodes[node_idx].aabb = combined_aabb;
        }
    }

    pub fn get_node_aabb(&self, node_id: u32) -> Option<&AABB> {
        if node_id & IS_SUBSECTOR_MASK != 0 {
            let subsector_id = (node_id & !IS_SUBSECTOR_MASK) as usize;
            self.subsector_leaves
                .get(subsector_id)
                .map(|leaf| &leaf.aabb)
        } else {
            self.nodes.get(node_id as usize).map(|node| &node.aabb)
        }
    }
}

impl Default for BSP3D {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            subsector_leaves: Vec::new(),
            root_node: 0,
            vertices: Vec::new(),
            sector_subsectors: Vec::new(),
        }
    }
}

impl Default for BSPLeaf3D {
    fn default() -> Self {
        Self {
            polygons: Vec::new(),
            aabb: AABB::new(),
            floor_polygons: Vec::new(),
            ceiling_polygons: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::{MapData, PicData};
    use crate::{SurfaceKind, level::bsp3d::HEIGHT_EPSILON};
    use wad::WadData;

    #[test]
    fn test_zero_height_walls() {
        let wad = WadData::new(&PathBuf::from("/Users/lukejones/DOOM/doom.wad"));
        let mut map = MapData::default();
        map.load("E1M2", &PicData::init(&wad), &wad);

        let bsp3d = &map.bsp_3d;
        let mut zero_height_walls = Vec::new();

        for (subsector_id, leaf) in bsp3d.subsector_leaves.iter().enumerate() {
            for polygon in &leaf.polygons {
                if let SurfaceKind::Vertical { wall_type, .. } = &polygon.surface_kind {
                    // Check if this wall has zero height by examining vertices
                    if polygon.vertices.len() >= 3 {
                        // Check if all vertices are at the same Z coordinate
                        let first_z = bsp3d.vertices[polygon.vertices[0]].z;
                        let is_zero_height = polygon
                            .vertices
                            .iter()
                            .all(|&idx| (bsp3d.vertices[idx].z - first_z).abs() < HEIGHT_EPSILON);

                        if is_zero_height {
                            zero_height_walls.push((
                                subsector_id,
                                polygon.sector_id,
                                wall_type.clone(),
                                polygon.vertices.len(),
                            ));
                        }
                    }
                }
            }
        }

        println!("Zero-height walls found in E1M2:");
        for (subsector_id, sector_id, wall_type, vertex_count) in zero_height_walls {
            println!(
                "  Subsector: {}, Sector: {}, Wall Type: {:?}, Vertices: {}",
                subsector_id, sector_id, wall_type, vertex_count
            );
        }
    }
}
