use crate::level::map_defs::{LineDef, Node, Sector, SubSector};
use crate::level::triangulation::carve_subsector_polygon;
use crate::{DivLine, LineDefFlags, PVS, PicData, Segment};
use glam::{Vec2, Vec3};
use wad::WadData;

const IS_SUBSECTOR_MASK: u32 = 0x8000_0000;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MovementType {
    Floor,
    Ceiling,
}

/// Axis-aligned bounding box
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

/// 3D polygon with vertices and color
#[derive(Debug, Clone)]
pub struct Node3D {
    pub xy: Vec2,
    pub delta: Vec2,
    pub bboxes: [[Vec3; 2]; 2],
    pub children: [u32; 2],
    pub aabb: Option<AABB>,
}

impl Node3D {
    pub const fn point_on_side(&self, v: &Vec2) -> usize {
        let dx = v.x - self.xy.x;
        let dy = v.y - self.xy.y;
        if (self.delta.y * dx) > (dy * self.delta.x) {
            return 0;
        }
        1
    }
}

#[derive(Debug, Clone)]
pub struct BSPLeaf3D {
    pub polygons: Vec<SurfacePolygon>,
    pub aabb: AABB,
    pub floor_polygons: Vec<usize>,
    pub ceiling_polygons: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum WallType {
    Top,
    Bottom,
    Middle,
    Door,
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum WallTexPin {
    UnpegTop,
    UnpegBottom,
    None,
}

impl From<LineDefFlags> for WallTexPin {
    fn from(value: LineDefFlags) -> Self {
        match value {
            // LineDefFlags::TwoSided => todo!(),
            LineDefFlags::UnpegTop => Self::UnpegTop,
            LineDefFlags::UnpegBottom => Self::UnpegBottom,
            _ => Self::None,
        }
    }
}

impl From<u32> for WallTexPin {
    fn from(value: u32) -> Self {
        if value & LineDefFlags::UnpegBottom as u32 != 0 {
            return Self::UnpegBottom;
        }
        if value & LineDefFlags::UnpegBottom as u32 != 0 {
            return Self::UnpegTop;
        }
        Self::None
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum SurfaceKind {
    Vertical {
        texture: Option<usize>,
        /// add this to the calculated texture column
        tex_x_offset: f32,
        /// add this to the calculated texture top
        /// TODO: if the vertical polygon vertexes change this needs updating
        tex_y_offset: f32,
        /// texture direction in radians (0 = east, π/2 = north)
        texture_direction: f32,
        wall_type: WallType,
        wall_tex_pin: WallTexPin,
    },
    Horizontal {
        /// Is a tag or index to patch
        texture: usize,
        // texture direction in radians (0 = east, π/2 = north)
        texture_direction: f32,
    },
}

/// For all surfaces the lightlevel comes from the sector.
/// As Thinkers are run on the Sector the light level gets updated.
/// The `SurfaceKind` provides details on the texturing required.
#[derive(Debug, Clone)]
pub struct SurfacePolygon {
    pub sector_id: usize,
    pub subsector_id: usize,
    pub surface_kind: SurfaceKind,
    pub vertices: Vec<usize>,
    pub normal: Vec3,
    pub aabb: AABB,
}

impl SurfacePolygon {
    fn new(
        vertex_indices: Vec<usize>,
        bsp3d: &BSP3D,
        sector_id: usize,
        subsector_id: usize,
        surface_kind: SurfaceKind,
    ) -> Self {
        let mut aabb = AABB::new();
        for &idx in &vertex_indices {
            aabb.expand_to_include_point(bsp3d.get_vertex(idx));
        }

        let v0 = bsp3d.get_vertex(vertex_indices[0]);
        let v1 = bsp3d.get_vertex(vertex_indices[1]);
        let v2 = bsp3d.get_vertex(vertex_indices[2]);

        let edge1 = v1 - v0;
        let edge2 = v2 - v0;
        let normal = Vec3::new(
            edge1.y * edge2.z - edge1.z * edge2.y,
            edge1.z * edge2.x - edge1.x * edge2.z,
            edge1.x * edge2.y - edge1.y * edge2.x,
        )
        .normalize();

        Self {
            vertices: vertex_indices,
            normal,
            aabb,
            sector_id,
            subsector_id,
            surface_kind,
        }
    }

    /// Create a polygon from 2D vertices at a given height
    fn from_2d_with_height(
        v0: Vec2,
        v1: Vec2,
        v2: Vec2,
        height: f32,
        bsp3d: &mut BSP3D,
        sector_id: usize,
        subsector_id: usize,
        surface_kind: SurfaceKind,
    ) -> Self {
        let vertex_positions = vec![
            Vec3::new(v0.x, v0.y, height),
            Vec3::new(v1.x, v1.y, height),
            Vec3::new(v2.x, v2.y, height),
        ];

        let vertex_indices: Vec<usize> = vertex_positions
            .into_iter()
            .map(|v| bsp3d.add_vertex(v))
            .collect();

        let mut aabb = AABB::new();
        for &idx in &vertex_indices {
            aabb.expand_to_include_point(bsp3d.get_vertex(idx));
        }

        let v0_pos = bsp3d.get_vertex(vertex_indices[0]);
        let v1_pos = bsp3d.get_vertex(vertex_indices[1]);
        let v2_pos = bsp3d.get_vertex(vertex_indices[2]);

        let edge1 = v1_pos - v0_pos;
        let edge2 = v2_pos - v0_pos;
        let normal = Vec3::new(
            edge1.y * edge2.z - edge1.z * edge2.y,
            edge1.z * edge2.x - edge1.x * edge2.z,
            edge1.x * edge2.y - edge1.y * edge2.x,
        )
        .normalize();

        Self {
            vertices: vertex_indices,
            normal,
            aabb,
            sector_id,
            subsector_id,
            surface_kind,
        }
    }

    /// True if the right side of the segment faces the point
    pub fn is_facing_point(&self, point: Vec3, bsp3d: &BSP3D) -> bool {
        let first_vertex = bsp3d.get_vertex(self.vertices[0]);
        let view_vector = (point - first_vertex).normalize_or_zero();
        let dot_product = self.normal.dot(view_vector);
        // Dynamic epsilon based on how horizontal the normal is
        let epsilon = if self.normal.z.abs() > 0.9 {
            -0.5 // More lenient for horizontal surfaces
        } else {
            -0.01
        };
        dot_product.is_nan() || dot_product > epsilon
    }
}

#[derive(Debug, Clone)]
struct FlushWall {
    segment_idx: usize,
    front_subsector_id: usize,
    wall_type: WallType,
    texture: usize,
    vertex_indices: Vec<usize>, // Boundary vertices that need unlinking
}

pub struct BSP3D {
    nodes: Vec<Node3D>,
    pub subsector_leaves: Vec<BSPLeaf3D>,
    root_node: u32,
    pvs: PVS,
    pub vertices: Vec<Vec3>,
    sector_subsectors: Vec<Vec<usize>>,
    flush_walls: Vec<FlushWall>,
    door_subsectors: Vec<usize>,
}

impl BSP3D {
    pub fn new(
        map_name: &str,
        root_node: u32,
        nodes: &[Node],
        subsectors: &[SubSector],
        segments: &[Segment],
        sectors: &[Sector],
        linedefs: &[LineDef],
        wad: &WadData,
        pic_data: &PicData,
    ) -> Self {
        let mut bsp3d = BSP3D {
            nodes: Vec::with_capacity(nodes.len()),
            subsector_leaves: Vec::with_capacity(nodes.len()),
            root_node: 0,
            pvs: PVS::new(0),
            vertices: Vec::new(),
            sector_subsectors: vec![Vec::new(); sectors.len()],
            flush_walls: Vec::new(),
            door_subsectors: Vec::new(),
        };
        bsp3d.root_node = root_node;

        for node in nodes {
            bsp3d.add_node_3d(node, sectors);
        }

        // Generate polygons using BSP traversal to collect dividing lines
        bsp3d.carve_polygons_recursive(
            nodes,
            subsectors,
            segments,
            linedefs,
            root_node,
            Vec::new(),
            pic_data,
        );
        bsp3d.update_nodes_aabbs(root_node);

        // Create flush walls after all polygons are generated
        bsp3d.create_flush_walls(segments, subsectors);

        // Handle door vertex separation after flush walls are created
        bsp3d.handle_door_vertex_separation(segments, subsectors);

        // Build PVS data for visibility culling (use cache if available)
        let hash = wad.map_bsp_hash(map_name).unwrap_or_default();
        if let Some(cached_pvs) = PVS::load_from_cache(map_name, hash, subsectors.len()) {
            bsp3d.pvs = cached_pvs;
        } else {
            bsp3d.pvs = PVS::build(subsectors, segments, &bsp3d);
        }

        bsp3d
    }

    pub fn pvs(&self) -> &PVS {
        &self.pvs
    }

    /// Check if one subsector can see another using PVS
    pub fn subsector_visible(&self, from: usize, to: usize) -> bool {
        self.pvs.is_visible(from, to)
    }

    pub fn nodes(&self) -> &[Node3D] {
        &self.nodes
    }

    pub fn get_subsector_leaf(&self, subsector_id: usize) -> Option<&BSPLeaf3D> {
        self.subsector_leaves.get(subsector_id)
    }

    pub fn add_vertex(&mut self, vertex: Vec3) -> usize {
        const EPSILON: f32 = 0.1;

        // Check if vertex already exists within tolerance
        for (index, existing_vertex) in self.vertices.iter().enumerate() {
            if self.vertex_equals_3d(*existing_vertex, vertex, EPSILON) {
                return index;
            }
        }

        // Vertex not found, add new one
        let index = self.vertices.len();
        self.vertices.push(vertex);
        index
    }

    /// Force add vertex without deduplication - used for door separation
    fn force_add_vertex(&mut self, vertex: Vec3) -> usize {
        let index = self.vertices.len();
        self.vertices.push(vertex);
        index
    }

    pub fn get_vertex(&self, index: usize) -> Vec3 {
        self.vertices[index]
    }

    fn find_or_add_vertex(&mut self, position_2d: Vec2, z: f32) -> usize {
        let vertex_3d = Vec3::new(position_2d.x, position_2d.y, z);
        self.add_vertex(vertex_3d)
    }

    pub fn get_polygon_vertices(&self, polygon: &SurfacePolygon) -> Vec<Vec3> {
        polygon
            .vertices
            .iter()
            .map(|&idx| self.vertices[idx])
            .collect()
    }

    pub fn move_floor_vertices(&mut self, sector_id: usize, new_height: f32) {
        if let Some(subsector_ids) = self.sector_subsectors.get(sector_id) {
            let subsector_ids = subsector_ids.clone();
            for subsector_id in subsector_ids {
                if let Some(leaf) = self.subsector_leaves.get(subsector_id) {
                    let floor_polygon_indices = leaf.floor_polygons.clone();
                    for &polygon_idx in &floor_polygon_indices {
                        if let Some(polygon) = self.subsector_leaves[subsector_id]
                            .polygons
                            .get(polygon_idx)
                        {
                            for &vertex_idx in &polygon.vertices {
                                self.vertices[vertex_idx].z = new_height;
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn move_ceiling_vertices(&mut self, sector_id: usize, new_height: f32) {
        if let Some(subsector_ids) = self.sector_subsectors.get(sector_id) {
            let subsector_ids = subsector_ids.clone();
            for subsector_id in subsector_ids {
                if let Some(leaf) = self.subsector_leaves.get(subsector_id) {
                    let ceiling_polygon_indices = leaf.ceiling_polygons.clone();
                    for &polygon_idx in &ceiling_polygon_indices {
                        if let Some(polygon) = self.subsector_leaves[subsector_id]
                            .polygons
                            .get(polygon_idx)
                        {
                            for &vertex_idx in &polygon.vertices {
                                self.vertices[vertex_idx].z = new_height;
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn get_subsector_leaf_count(&self) -> usize {
        self.subsector_leaves.len()
    }

    fn compute_leaf_aabb(&mut self, leaf_id: usize) {
        if let Some(leaf) = self.subsector_leaves.get_mut(leaf_id) {
            if leaf.polygons.is_empty() {
                return;
            }

            let mut aabb = AABB::new();
            for polygon in &leaf.polygons {
                for &vertex_idx in &polygon.vertices {
                    aabb.expand_to_include_point(self.vertices[vertex_idx]);
                }
            }
            leaf.aabb = aabb;
        }
    }

    fn update_nodes_aabbs(&mut self, node_id: u32) {
        for leaf_id in 0..self.subsector_leaves.len() {
            self.compute_leaf_aabb(leaf_id);
        }

        let mut visited = vec![false; self.nodes.len()];
        self.update_node_aabb_recursive(node_id, &mut visited);
    }

    fn update_node_aabb_recursive(&mut self, node_id: u32, visited: &mut [bool]) -> Option<AABB> {
        if node_id & IS_SUBSECTOR_MASK != 0 {
            let subsector_id = if node_id == u32::MAX {
                0
            } else {
                (node_id & !IS_SUBSECTOR_MASK) as usize
            };

            return self
                .subsector_leaves
                .get(subsector_id)
                .map(|leaf| leaf.aabb);
        }

        let node_idx = node_id as usize;
        if node_idx >= self.nodes.len() || visited[node_idx] {
            return None;
        }

        visited[node_idx] = true;
        let children = self.nodes[node_idx].children;
        let mut node_aabb = AABB::new();
        let mut has_valid_aabb = false;

        for child_id in &children {
            if let Some(child_aabb) = self.update_node_aabb_recursive(*child_id, visited) {
                node_aabb.expand_to_include_aabb(&child_aabb);
                has_valid_aabb = true;
            }
        }

        if has_valid_aabb {
            self.nodes[node_idx].aabb = Some(node_aabb);
            Some(node_aabb)
        } else {
            None
        }
    }

    pub fn get_node_aabb(&self, node_id: u32) -> Option<&AABB> {
        if node_id & IS_SUBSECTOR_MASK != 0 {
            let subsector_id = if node_id == u32::MAX {
                0
            } else {
                (node_id & !IS_SUBSECTOR_MASK) as usize
            };
            return self
                .subsector_leaves
                .get(subsector_id)
                .map(|leaf| &leaf.aabb);
        }

        self.nodes
            .get(node_id as usize)
            .and_then(|node| node.aabb.as_ref())
    }

    pub fn root_node(&self) -> u32 {
        self.root_node
    }

    fn add_node_3d(&mut self, node: &Node, sectors: &[Sector]) {
        let node_3d = Node3D {
            xy: node.xy,
            delta: node.delta,
            bboxes: self.compute_3d_bboxes(node, sectors),
            children: node.children,
            aabb: None,
        };
        self.nodes.push(node_3d);
    }

    fn compute_3d_bboxes(&self, node: &Node, sectors: &[Sector]) -> [[Vec3; 2]; 2] {
        let mut bboxes = [[Vec3::ZERO; 2]; 2];

        for side in 0..2 {
            let min_2d = node.bboxes[side][0];
            let max_2d = node.bboxes[side][1];
            let (min_z, max_z) = self.find_z_range_in_bbox(min_2d, max_2d, sectors);

            bboxes[side][0] = Vec3::new(min_2d.x, min_2d.y, min_z);
            bboxes[side][1] = Vec3::new(max_2d.x, max_2d.y, max_z);
        }

        bboxes
    }

    fn find_z_range_in_bbox(&self, min_2d: Vec2, max_2d: Vec2, sectors: &[Sector]) -> (f32, f32) {
        let mut min_z = f32::MAX;
        let mut max_z = f32::MIN;

        for sector in sectors {
            if self.sector_overlaps_bbox(sector, min_2d, max_2d) {
                min_z = min_z.min(sector.floorheight);
                max_z = max_z.max(sector.ceilingheight);
            }
        }

        if min_z == f32::MAX {
            min_z = -4096.0;
            max_z = 4096.0;
        }

        (min_z, max_z)
    }

    // TODO: build correctly sized BBOX
    fn sector_overlaps_bbox(&self, _sector: &Sector, _min_2d: Vec2, _max_2d: Vec2) -> bool {
        true
    }

    fn create_bottom_wall_surface(&self, segment: &Segment) -> SurfaceKind {
        self.create_vertical_surface_kind(segment, segment.sidedef.bottomtexture, WallType::Bottom)
    }

    fn create_top_wall_surface(
        &self,
        segment: &Segment,
        back_sector: &Sector,
        pic_data: &PicData,
    ) -> SurfaceKind {
        let texture = if back_sector.ceilingpic == pic_data.sky_num()
            || back_sector.floorpic == pic_data.sky_num()
        {
            None
        } else {
            segment.sidedef.toptexture
        };
        self.create_vertical_surface_kind(segment, texture, WallType::Top)
    }

    fn create_middle_wall_surface(&self, segment: &Segment) -> SurfaceKind {
        self.create_vertical_surface_kind(segment, segment.sidedef.midtexture, WallType::Middle)
    }

    fn create_one_sided_wall_surface(&self, segment: &Segment) -> SurfaceKind {
        self.create_vertical_surface_kind(segment, segment.sidedef.midtexture, WallType::Middle)
    }

    fn add_wall_polygons(
        &mut self,
        leaf: &mut BSPLeaf3D,
        segment: &Segment,
        bottom_z: f32,
        top_z: f32,
        sector_id: usize,
        subsector_id: usize,
        surface_kind: SurfaceKind,
    ) {
        let wall_polygons = self.create_wall_polygons(
            &segment.v1,
            &segment.v2,
            bottom_z,
            top_z,
            sector_id,
            subsector_id,
            surface_kind,
            segment.linedef.flags,
        );
        for wall_polygon in wall_polygons {
            leaf.polygons.push(wall_polygon);
        }
    }

    fn calculate_middle_texture_bounds(
        &self,
        segment: &Segment,
        front_sector: &Sector,
        back_sector: &Sector,
        pic_data: &PicData,
    ) -> (f32, f32) {
        let tex_id = segment.sidedef.midtexture.unwrap();
        let texture = pic_data.get_texture(tex_id);
        let tex_height = texture.height as f32;

        let mut world_bot = front_sector.ceilingheight - tex_height;
        let mut world_top = front_sector.ceilingheight;

        if segment.frontsector.ceilingpic == pic_data.sky_num()
            && back_sector.ceilingpic == pic_data.sky_num()
        {
            world_bot = back_sector.floorheight.max(front_sector.floorheight);
            world_top = front_sector.ceilingheight.min(back_sector.ceilingheight)
                + segment.sidedef.rowoffset;
        } else if back_sector.ceilingheight < front_sector.ceilingheight {
            world_bot = back_sector
                .floorheight
                .max(back_sector.ceilingheight - tex_height);
            world_top = back_sector.ceilingheight;
        }

        (world_bot, world_top)
    }

    fn generate_floor_ceiling_polygons(
        &mut self,
        leaf: &mut BSPLeaf3D,
        polygon: &[Vec2],
        subsector: &SubSector,
        subsector_id: usize,
    ) {
        let has_valid_polygon = polygon.len() >= 3;
        if !has_valid_polygon {
            return;
        }

        // Check if this is a door subsector (ceiling == floor height)
        if subsector.sector.ceilingheight == subsector.sector.floorheight {
            self.door_subsectors.push(subsector_id);
        }

        // Insert intermediate vertices that lie on polygon edges
        let expanded_polygon = self.insert_edge_vertices(polygon, subsector.sector.floorheight);

        for i in 1..expanded_polygon.len() - 1 {
            let surface_polygon = SurfacePolygon::from_2d_with_height(
                expanded_polygon[0],
                expanded_polygon[i + 1],
                expanded_polygon[i],
                subsector.sector.floorheight,
                self,
                subsector.sector.num as usize,
                subsector_id,
                Self::create_horizontal_surface_kind(subsector.sector.floorpic),
            );
            let polygon_idx = leaf.polygons.len();
            let floorheight = surface_polygon
                .vertices
                .iter()
                .all(|v| self.vertices[*v].z == subsector.sector.floorheight);
            leaf.polygons.push(surface_polygon);

            // Check if this sector is movable and track floor polygons
            if floorheight {
                leaf.floor_polygons.push(polygon_idx);
            }

            let surface_polygon = SurfacePolygon::from_2d_with_height(
                expanded_polygon[i],
                expanded_polygon[i + 1],
                expanded_polygon[0],
                subsector.sector.ceilingheight,
                self,
                subsector.sector.num as usize,
                subsector_id,
                Self::create_horizontal_surface_kind(subsector.sector.ceilingpic),
            );
            let polygon_idx = leaf.polygons.len();
            let ceilingheight = surface_polygon
                .vertices
                .iter()
                .all(|v| self.vertices[*v].z == subsector.sector.ceilingheight);
            leaf.polygons.push(surface_polygon);

            // Check if this sector is movable and track ceiling polygons
            if ceilingheight {
                leaf.ceiling_polygons.push(polygon_idx);
            }
        }
    }

    fn generate_wall_polygons(
        &mut self,
        leaf: &mut BSPLeaf3D,
        segments: &[Segment],
        subsector: &SubSector,
        subsector_id: usize,
        pic_data: &PicData,
    ) {
        let start_seg = subsector.start_seg as usize;
        for (local_idx, segment) in segments.iter().enumerate() {
            let global_segment_idx = start_seg + local_idx;
            if let Some(back_sector) = &segment.backsector {
                self.generate_two_sided_wall(
                    leaf,
                    segment,
                    subsector,
                    back_sector,
                    subsector_id,
                    global_segment_idx,
                    pic_data,
                );
            } else {
                self.generate_one_sided_wall(leaf, segment, subsector, subsector_id);
            }
        }
    }

    fn generate_two_sided_wall(
        &mut self,
        leaf: &mut BSPLeaf3D,
        segment: &Segment,
        subsector: &SubSector,
        back_sector: &Sector,
        subsector_id: usize,
        segment_idx: usize,
        pic_data: &PicData,
    ) {
        let front_floor = subsector.sector.floorheight;
        let front_ceiling = subsector.sector.ceilingheight;
        let back_floor = back_sector.floorheight;
        let back_ceiling = back_sector.ceilingheight;

        // Check for flush wall conditions
        let front_sidedef = &*segment.sidedef;

        // Case 1: Lower texture + flush floors
        if front_floor == back_floor {
            if let Some(lower_texture) = front_sidedef.bottomtexture {
                // Get boundary vertex indices for this segment
                let vertex_indices = vec![
                    self.find_or_add_vertex(*segment.v1, front_floor),
                    self.find_or_add_vertex(*segment.v2, front_floor),
                ];

                self.flush_walls.push(FlushWall {
                    segment_idx,
                    front_subsector_id: subsector_id,
                    wall_type: WallType::Bottom,
                    texture: lower_texture,
                    vertex_indices,
                });
            }
        }

        // Case 2: Upper texture + flush ceilings
        if front_ceiling == back_ceiling {
            if let Some(upper_texture) = front_sidedef.toptexture {
                // Get boundary vertex indices for this segment
                let vertex_indices = vec![
                    self.find_or_add_vertex(*segment.v1, front_ceiling),
                    self.find_or_add_vertex(*segment.v2, front_ceiling),
                ];

                self.flush_walls.push(FlushWall {
                    segment_idx,
                    front_subsector_id: subsector_id,
                    wall_type: WallType::Top,
                    texture: upper_texture,
                    vertex_indices,
                });
            }
        }

        // Check for door segments between different sectors
        if subsector.sector.ceilingheight == subsector.sector.floorheight
            && back_sector.num != subsector.sector.num
        {
            // Get boundary vertex indices for this segment
            let vertex_indices = vec![
                self.find_or_add_vertex(*segment.v1, subsector.sector.floorheight),
                self.find_or_add_vertex(*segment.v2, subsector.sector.floorheight),
            ];

            self.flush_walls.push(FlushWall {
                segment_idx,
                front_subsector_id: subsector_id,
                wall_type: WallType::Door,
                texture: 0, // Will be determined later based on adjacent walls
                vertex_indices,
            });
        }

        let (min_floor, max_floor) = if front_floor > back_floor {
            (back_floor, front_floor)
        } else {
            (front_floor, back_floor)
        };

        let (min_ceil, max_ceil) = if front_ceiling > back_ceiling {
            (back_ceiling, front_ceiling)
        } else {
            (front_ceiling, back_ceiling)
        };

        // Lower wall (if back floor is higher)
        if back_floor > front_floor {
            let surface_kind = self.create_bottom_wall_surface(segment);
            self.add_wall_polygons(
                leaf,
                segment,
                min_floor,
                max_floor,
                subsector.sector.num as usize,
                subsector_id,
                surface_kind,
            );
        }

        // Upper wall (if back ceiling is lower)
        if back_ceiling < front_ceiling {
            let surface_kind = self.create_top_wall_surface(segment, back_sector, pic_data);
            self.add_wall_polygons(
                leaf,
                segment,
                min_ceil,
                max_ceil,
                subsector.sector.num as usize,
                subsector_id,
                surface_kind,
            );
        }

        // Middle texture (masked texture)
        if segment.sidedef.midtexture.is_some() {
            let (world_bot, world_top) = self.calculate_middle_texture_bounds(
                segment,
                &subsector.sector,
                back_sector,
                pic_data,
            );
            let surface_kind = self.create_middle_wall_surface(segment);
            self.add_wall_polygons(
                leaf,
                segment,
                world_bot,
                world_top,
                subsector.sector.num as usize,
                subsector_id,
                surface_kind,
            );
        }
    }

    fn generate_one_sided_wall(
        &mut self,
        leaf: &mut BSPLeaf3D,
        segment: &Segment,
        subsector: &SubSector,
        subsector_id: usize,
    ) {
        let surface_kind = self.create_one_sided_wall_surface(segment);
        self.add_wall_polygons(
            leaf,
            segment,
            subsector.sector.floorheight,
            subsector.sector.ceilingheight,
            subsector.sector.num as usize,
            subsector_id,
            surface_kind,
        );
    }

    fn update_leaf_aabb(&mut self, leaf: &mut BSPLeaf3D) {
        if !leaf.polygons.is_empty() {
            let mut aabb = AABB::new();
            for polygon in &leaf.polygons {
                for &vertex_idx in &polygon.vertices {
                    aabb.expand_to_include_point(self.vertices[vertex_idx]);
                }
            }
            leaf.aabb = aabb;
        }
    }

    fn create_leaf_from_subsector_with_polygon(
        &mut self,
        subsector_id: usize,
        subsector: &SubSector,
        segments: &[Segment],
        polygon: &[Vec2],
        pic_data: &PicData,
    ) {
        let mut leaf = BSPLeaf3D {
            polygons: Vec::new(),
            aabb: AABB::new(),
            floor_polygons: Vec::new(),
            ceiling_polygons: Vec::new(),
        };

        // Create AABB from 2D polygon bounds even if no 3D polygons are generated
        if polygon.len() >= 3 {
            let mut aabb = AABB::new();
            for &point in polygon {
                aabb.expand_to_include_point(Vec3::new(
                    point.x,
                    point.y,
                    subsector.sector.floorheight,
                ));
                aabb.expand_to_include_point(Vec3::new(
                    point.x,
                    point.y,
                    subsector.sector.ceilingheight,
                ));
            }
            leaf.aabb = aabb;
        }

        // Generate floor/ceiling polygons
        self.generate_floor_ceiling_polygons(&mut leaf, polygon, subsector, subsector_id);

        // Generate wall polygons
        self.generate_wall_polygons(&mut leaf, segments, subsector, subsector_id, pic_data);

        // Update AABB from generated polygons
        self.update_leaf_aabb(&mut leaf);

        // Ensure vector is large enough
        if subsector_id >= self.subsector_leaves.len() {
            self.subsector_leaves.resize(subsector_id + 1, leaf.clone());
        }
        self.subsector_leaves[subsector_id] = leaf;

        // Add subsector to sector mapping
        let sector_id = subsector.sector.num as usize;
        if sector_id < self.sector_subsectors.len() {
            self.sector_subsectors[sector_id].push(subsector_id);
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
    ) {
        // Check if this is a subsector
        if node_id & IS_SUBSECTOR_MASK != 0 {
            // We've reached a subsector - generate its polygon
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
                    let polygon = carve_subsector_polygon(subsector_segments, &divlines);
                    // triangulate_subsector
                    self.create_leaf_from_subsector_with_polygon(
                        subsector_id,
                        subsector,
                        subsector_segments,
                        &polygon,
                        pic_data,
                    );
                }
            }
            return;
        }

        // It's a node - get the node data and recurse
        if let Some(node) = nodes.get(node_id as usize) {
            // Create divline from this node
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
            );
        }
    }

    /// Create wall polygons from two 2D points and height range
    fn create_wall_polygons(
        &mut self,
        v1: &Vec2,
        v2: &Vec2,
        bottom_z: f32,
        top_z: f32,
        sector_id: usize,
        subsector_id: usize,
        surface_kind: SurfaceKind,
        linedef_flags: u32,
    ) -> Vec<SurfacePolygon> {
        let wall_dir = v2 - v1;
        let texture_direction = wall_dir.y.atan2(wall_dir.x);

        let surface_kind_with_direction = match surface_kind {
            SurfaceKind::Vertical {
                texture,
                tex_x_offset,
                tex_y_offset,
                wall_type,
                ..
            } => SurfaceKind::Vertical {
                texture,
                tex_x_offset,
                tex_y_offset,
                texture_direction,
                wall_type,
                wall_tex_pin: WallTexPin::from(linedef_flags),
            },
            other => other,
        };
        let bottom_left = Vec3::new(v1.x, v1.y, bottom_z);
        let bottom_right = Vec3::new(v2.x, v2.y, bottom_z);
        let top_left = Vec3::new(v1.x, v1.y, top_z);
        let top_right = Vec3::new(v2.x, v2.y, top_z);

        let bottom_left_idx = self.add_vertex(bottom_left);
        let bottom_right_idx = self.add_vertex(bottom_right);
        let top_left_idx = self.add_vertex(top_left);
        let top_right_idx = self.add_vertex(top_right);

        {
            let (triangle1, triangle2) = self.create_wall_quad_triangles(
                bottom_left_idx,
                bottom_right_idx,
                top_left_idx,
                top_right_idx,
                sector_id,
                subsector_id,
                surface_kind_with_direction,
            );
            vec![triangle1, triangle2]
        }
    }

    /// Create flush walls after all polygons are generated
    fn create_flush_walls(&mut self, segments: &[Segment], subsectors: &[SubSector]) {
        let flush_walls = std::mem::take(&mut self.flush_walls);

        // Group flush walls by sector
        let mut flush_walls_by_sector: std::collections::HashMap<usize, Vec<FlushWall>> =
            std::collections::HashMap::new();

        for flush_wall in flush_walls {
            let subsector = &subsectors[flush_wall.front_subsector_id];
            let sector_id = subsector.sector.num as usize;
            flush_walls_by_sector
                .entry(sector_id)
                .or_insert_with(Vec::new)
                .push(flush_wall);
        }

        // Process each sector's flush walls as a batch
        for (sector_id, sector_flush_walls) in flush_walls_by_sector {
            self.create_flush_walls_for_sector(sector_id, sector_flush_walls, segments);
        }
    }

    /// Create all flush walls for a sector as a batch
    fn create_flush_walls_for_sector(
        &mut self,
        sector_id: usize,
        flush_walls: Vec<FlushWall>,
        segments: &[Segment],
    ) {
        // Collect all unique boundary vertices across all flush walls in this sector
        let mut all_boundary_vertices = std::collections::HashSet::new();
        for flush_wall in &flush_walls {
            for &vertex_idx in &flush_wall.vertex_indices {
                all_boundary_vertices.insert(vertex_idx);
            }
        }
        let boundary_vertices: Vec<usize> = all_boundary_vertices.into_iter().collect();
        // Unlink all boundary vertices once for the entire sector
        let new_vertex_mapping = self.unlink_all_sector_vertices(&boundary_vertices, sector_id);

        // Create all flush walls for this sector
        for flush_wall in flush_walls {
            let segment = &segments[flush_wall.segment_idx];

            // Map old vertex indices to new ones
            let new_vertex_indices: Vec<usize> = flush_wall
                .vertex_indices
                .iter()
                .map(|&old_idx| new_vertex_mapping.get(&old_idx).copied().unwrap_or(old_idx))
                .collect();

            match flush_wall.wall_type {
                WallType::Bottom => {
                    // Lower wall: original vertices (top) + new vertices (bottom)
                    self.create_flush_wall_polygons(
                        segment,
                        &flush_wall.vertex_indices, // top (original, will move with back sector)
                        &new_vertex_indices,        // bottom (new, stay with front sector)
                        flush_wall.texture,
                        sector_id,
                        flush_wall.front_subsector_id,
                    );
                }
                WallType::Top => {
                    // Upper wall: new vertices (top) + original vertices (bottom)
                    self.create_flush_wall_polygons(
                        segment,
                        &new_vertex_indices, // top (new, stay with front sector)
                        &flush_wall.vertex_indices, // bottom (original, will move with back sector)
                        flush_wall.texture,
                        sector_id,
                        flush_wall.front_subsector_id,
                    );
                }
                WallType::Door => {
                    // Door walls are handled separately in
                    // handle_door_vertex_separation
                }
                _ => {} // Only handle Top, Bottom, and Door walls
            }
        }
    }

    /// Unlink all boundary vertices for a sector at once
    fn unlink_all_sector_vertices(
        &mut self,
        vertex_indices: &[usize],
        sector_id: usize,
    ) -> std::collections::HashMap<usize, usize> {
        let mut vertex_mapping = std::collections::HashMap::new();
        // Create duplicate vertices by cloning existing ones
        for &old_idx in vertex_indices {
            let old_vertex = self.vertices[old_idx];
            let new_idx = self.vertices.len();
            self.vertices.push(old_vertex);
            vertex_mapping.insert(old_idx, new_idx);
        }

        // Find all subsectors in the sector and search ALL their polygons
        let subsector_list = if sector_id < self.sector_subsectors.len() {
            self.sector_subsectors[sector_id].clone()
        } else {
            Vec::new()
        };

        for &current_subsector_id in &subsector_list {
            let leaf = &mut self.subsector_leaves[current_subsector_id];
            let total_polygons = leaf.polygons.len();

            if total_polygons > 0 {
                for polygon_idx in 0..total_polygons {
                    let polygon = &mut leaf.polygons[polygon_idx];
                    for vertex_idx in &mut polygon.vertices {
                        if let Some(&new_idx) = vertex_mapping.get(vertex_idx) {
                            *vertex_idx = new_idx;
                        }
                    }
                }
            }
        }
        vertex_mapping
    }

    /// Create flush wall polygons between top and bottom vertex sets
    fn handle_door_vertex_separation(&mut self, segments: &[Segment], subsectors: &[SubSector]) {
        // Create a map of bookmarked vertices for each door subsector
        let mut door_vertex_bookmarks: std::collections::HashMap<usize, Vec<usize>> =
            std::collections::HashMap::new();

        // Phase 1: For each door subsector, duplicate ceiling vertices
        let door_subsectors = self.door_subsectors.clone();
        for &subsector_id in &door_subsectors {
            let mut new_vertex_indices = Vec::new();

            // Get ceiling polygon indices
            let ceiling_polygons = self.subsector_leaves[subsector_id].ceiling_polygons.clone();

            // Duplicate vertices for ceiling polygons
            for &polygon_idx in &ceiling_polygons {
                let vertices_to_duplicate = self.subsector_leaves[subsector_id].polygons
                    [polygon_idx]
                    .vertices
                    .clone();
                let mut new_vertices = Vec::new();

                for &vertex_idx in &vertices_to_duplicate {
                    let vertex_pos = self.vertices[vertex_idx];
                    let new_vertex_idx = self.force_add_vertex(vertex_pos);
                    new_vertices.push(new_vertex_idx);
                    new_vertex_indices.push(new_vertex_idx);
                }

                // Update polygon to use new vertices
                self.subsector_leaves[subsector_id].polygons[polygon_idx].vertices =
                    new_vertices.clone();
            }

            door_vertex_bookmarks.insert(subsector_id, new_vertex_indices);
        }

        // Phase 2: Process door flush walls to update adjacent wall polygons
        let door_flush_walls: Vec<_> = self
            .flush_walls
            .iter()
            .filter(|fw| fw.wall_type == WallType::Door)
            .cloned()
            .collect();

        for flush_wall in door_flush_walls {
            let segment = &segments[flush_wall.segment_idx];

            // Find the back sector number
            let back_sector_num = if let Some(back_sector) = &segment.backsector {
                back_sector.num
            } else {
                continue;
            };

            // Find subsectors belonging to the back sector and update their polygons
            for (subsector_id, subsector) in subsectors.iter().enumerate() {
                if subsector.sector.num == back_sector_num {
                    if let Some(bookmarked_vertices) =
                        door_vertex_bookmarks.get(&flush_wall.front_subsector_id)
                    {
                        // Collect polygon updates to avoid borrowing issues
                        let mut polygon_updates = Vec::new();

                        let leaf_polygons = &self.subsector_leaves[subsector_id].polygons;
                        for (poly_idx, polygon) in leaf_polygons.iter().enumerate() {
                            if let SurfaceKind::Vertical { wall_type: _, .. } = polygon.surface_kind
                            {
                                if self.is_polygon_aligned_with_segment(polygon, segment) {
                                    let has_upper_texture = segment.sidedef.toptexture.is_some();
                                    let new_vertices = if has_upper_texture {
                                        self.calculate_wall_bottom_vertex_updates(
                                            &polygon.vertices,
                                            bookmarked_vertices,
                                        )
                                    } else {
                                        self.calculate_wall_ceiling_vertex_updates(
                                            &polygon.vertices,
                                            bookmarked_vertices,
                                        )
                                    };
                                    polygon_updates.push((poly_idx, new_vertices));
                                }
                            }
                        }

                        // Apply updates
                        for (poly_idx, new_vertices) in polygon_updates {
                            self.subsector_leaves[subsector_id].polygons[poly_idx].vertices =
                                new_vertices;
                        }
                    }
                }
            }
        }

        // Phase 3: Update adjacent upper walls to use door ceiling vertices
        self.update_adjacent_upper_walls(segments, subsectors, &door_vertex_bookmarks);

        // Phase 4: Generate missing door walls for single-sided segments with middle
        // textures
        self.generate_missing_door_walls(segments, subsectors, &door_vertex_bookmarks);
    }

    fn is_polygon_aligned_with_segment(&self, polygon: &SurfacePolygon, segment: &Segment) -> bool {
        // Check if the polygon's vertices align with the segment's line
        if polygon.vertices.len() < 2 {
            return false;
        }

        let seg_v1 = *segment.v1;
        let seg_v2 = *segment.v2;

        // Check if any two vertices of the polygon match the segment endpoints in XY
        for i in 0..polygon.vertices.len() {
            for j in i + 1..polygon.vertices.len() {
                let v1 = self.vertices[polygon.vertices[i]];
                let v2 = self.vertices[polygon.vertices[j]];

                let v1_xy = self.vertex_to_2d(v1);
                let v2_xy = self.vertex_to_2d(v2);

                if (v1_xy == seg_v1 && v2_xy == seg_v2) || (v1_xy == seg_v2 && v2_xy == seg_v1) {
                    return true;
                }
            }
        }

        false
    }

    fn calculate_wall_bottom_vertex_updates(
        &self,
        vertices: &[usize],
        bookmarked_vertices: &[usize],
    ) -> Vec<usize> {
        // Find bottom vertices (lower Z values) and replace with bookmarked vertices
        let mut vertex_pairs: Vec<(usize, f32)> = vertices
            .iter()
            .map(|&idx| (idx, self.vertices[idx].z))
            .collect();
        vertex_pairs.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

        let mut new_vertices = vertices.to_vec();
        let bottom_count = vertex_pairs.len() / 2;
        for i in 0..bottom_count.min(bookmarked_vertices.len()) {
            let old_idx = vertex_pairs[i].0;
            if let Some(pos) = new_vertices.iter().position(|&idx| idx == old_idx) {
                new_vertices[pos] = bookmarked_vertices[i];
            }
        }
        new_vertices
    }

    fn update_adjacent_upper_walls(
        &mut self,
        segments: &[Segment],
        subsectors: &[SubSector],
        door_vertex_bookmarks: &std::collections::HashMap<usize, Vec<usize>>,
    ) {
        // Collect all update operations first to avoid borrowing issues
        let mut wall_updates = Vec::new();
        let door_subsectors = self.door_subsectors.clone();

        // For each door subsector, find segments that connect to adjacent sectors
        for &door_subsector_id in &door_subsectors {
            let door_sector_num = subsectors[door_subsector_id].sector.num;

            if let Some(door_ceiling_vertices) = door_vertex_bookmarks.get(&door_subsector_id) {
                // Find segments where back sector is the door sector
                for segment in segments.iter() {
                    if let Some(back_sector) = &segment.backsector {
                        if back_sector.num == door_sector_num {
                            // This segment has the door as back sector, check if it has upper
                            // texture
                            if segment.sidedef.toptexture.is_some() {
                                // Find the front subsector that contains this segment
                                let front_sector_num = segment.frontsector.num;
                                for (subsector_id, subsector) in subsectors.iter().enumerate() {
                                    if subsector.sector.num == front_sector_num {
                                        let updates = self.collect_upper_wall_updates(
                                            subsector_id,
                                            segment,
                                            door_ceiling_vertices,
                                        );
                                        wall_updates.extend(updates);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Apply all collected updates
        for (subsector_id, poly_idx, new_vertices) in wall_updates {
            self.subsector_leaves[subsector_id].polygons[poly_idx].vertices = new_vertices;
        }
    }

    fn collect_upper_wall_updates(
        &self,
        subsector_id: usize,
        segment: &Segment,
        door_ceiling_vertices: &[usize],
    ) -> Vec<(usize, usize, Vec<usize>)> {
        let leaf = &self.subsector_leaves[subsector_id];
        let mut wall_updates = Vec::new();

        // Find vertical polygons that align with this segment
        for (poly_idx, polygon) in leaf.polygons.iter().enumerate() {
            if let SurfaceKind::Vertical {
                wall_type: WallType::Top,
                ..
            } = polygon.surface_kind
            {
                if self.is_polygon_aligned_with_segment(polygon, segment) {
                    // Get all vertices at the bottom Z level
                    let wall_vertices = &polygon.vertices;
                    let mut vertex_z_pairs: Vec<(usize, f32)> = wall_vertices
                        .iter()
                        .map(|&idx| (idx, self.vertices[idx].z))
                        .collect();
                    vertex_z_pairs.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

                    // Find the minimum Z value (bottom level)
                    let min_z = vertex_z_pairs[0].1;
                    let bottom_vertices: Vec<usize> = vertex_z_pairs
                        .iter()
                        .filter(|&&(_, z)| (z - min_z).abs() < 0.1) // Vertices at bottom level
                        .map(|&(idx, _)| idx)
                        .collect();

                    // Debug: Check for duplicate vertices in this polygon
                    let mut vertex_counts = std::collections::HashMap::new();
                    for &vertex_idx in wall_vertices {
                        *vertex_counts.entry(vertex_idx).or_insert(0) += 1;
                    }

                    // Replace bottom vertices with matching door ceiling vertices
                    let mut new_wall_vertices = wall_vertices.clone();
                    for &bottom_vertex_idx in &bottom_vertices {
                        let bottom_pos = self.vertices[bottom_vertex_idx];

                        // Find matching door ceiling vertex with same XY coordinates
                        for &door_vertex_idx in door_ceiling_vertices {
                            let door_pos = self.vertices[door_vertex_idx];
                            if bottom_pos.x == door_pos.x && bottom_pos.y == door_pos.y {
                                // Replace ALL instances of the bottom vertex with door ceiling
                                // vertex
                                for i in 0..new_wall_vertices.len() {
                                    if new_wall_vertices[i] == bottom_vertex_idx {
                                        new_wall_vertices[i] = door_vertex_idx;
                                    }
                                }
                                break;
                            }
                        }
                    }

                    if new_wall_vertices != wall_vertices.clone() {
                        wall_updates.push((subsector_id, poly_idx, new_wall_vertices));
                    }
                }
            }
        }

        wall_updates
    }

    fn generate_missing_door_walls(
        &mut self,
        segments: &[Segment],
        subsectors: &[SubSector],
        door_vertex_bookmarks: &std::collections::HashMap<usize, Vec<usize>>,
    ) {
        let door_subsectors = self.door_subsectors.clone();

        for &door_subsector_id in &door_subsectors {
            let door_sector_num = subsectors[door_subsector_id].sector.num;

            if let Some(door_ceiling_vertices) = door_vertex_bookmarks.get(&door_subsector_id) {
                // Get door floor vertices
                let door_leaf = &self.subsector_leaves[door_subsector_id];
                let mut door_floor_vertices = Vec::new();
                for &polygon_idx in &door_leaf.floor_polygons {
                    for &vertex_idx in &door_leaf.polygons[polygon_idx].vertices {
                        door_floor_vertices.push(vertex_idx);
                    }
                }

                // Find single-sided segments with middle textures that belong to this door
                // sector
                for segment in segments.iter() {
                    // Check if this is a single-sided segment (no back sector)
                    if segment.backsector.is_none() {
                        // Check if segment has middle texture
                        if segment.sidedef.midtexture.is_some() {
                            // Check if front sector is the door sector
                            if segment.frontsector.num == door_sector_num {
                                self.create_door_wall_for_segment(
                                    door_subsector_id,
                                    segment,
                                    &door_floor_vertices,
                                    door_ceiling_vertices,
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    fn create_door_wall_for_segment(
        &mut self,
        door_subsector_id: usize,
        segment: &Segment,
        door_floor_vertices: &[usize],
        door_ceiling_vertices: &[usize],
    ) {
        let seg_v1 = *segment.v1;
        let seg_v2 = *segment.v2;

        // Find matching floor vertices (bottom of wall)
        let bottom_v1 = self.find_vertex_at_2d_position(door_floor_vertices, seg_v1, 0.01);
        let bottom_v2 = self.find_vertex_at_2d_position(door_floor_vertices, seg_v2, 0.01);

        // Find matching ceiling vertices (top of wall)
        let top_v1 = self.find_vertex_at_2d_position(door_ceiling_vertices, seg_v1, 0.01);
        let top_v2 = self.find_vertex_at_2d_position(door_ceiling_vertices, seg_v2, 0.01);

        // Create wall if we have all four vertices
        if let (Some(bottom_v1), Some(bottom_v2), Some(top_v1), Some(top_v2)) =
            (bottom_v1, bottom_v2, top_v1, top_v2)
        {
            let surface_kind = SurfaceKind::Vertical {
                texture: segment.sidedef.midtexture,
                tex_x_offset: segment.sidedef.textureoffset,
                tex_y_offset: segment.sidedef.rowoffset,
                texture_direction: 0.0,
                wall_type: WallType::Door,
                wall_tex_pin: WallTexPin::from(segment.linedef.flags),
            };

            let door_sector_num = self.subsector_leaves[door_subsector_id]
                .polygons
                .first()
                .map(|p| p.sector_id)
                .unwrap_or(0);

            // Create two triangles for the door wall
            let (wall_polygon1, wall_polygon2) = self.create_wall_quad_triangles(
                bottom_v1,
                bottom_v2,
                top_v1,
                top_v2,
                door_sector_num,
                door_subsector_id,
                surface_kind,
            );

            // Add the wall polygons to the door subsector
            let leaf = &mut self.subsector_leaves[door_subsector_id];
            leaf.polygons.push(wall_polygon1);
            leaf.polygons.push(wall_polygon2);
        }
    }

    fn calculate_wall_ceiling_vertex_updates(
        &self,
        vertices: &[usize],
        bookmarked_vertices: &[usize],
    ) -> Vec<usize> {
        // Find ceiling vertices (higher Z values) and replace with bookmarked vertices
        let mut vertex_pairs: Vec<(usize, f32)> = vertices
            .iter()
            .map(|&idx| (idx, self.vertices[idx].z))
            .collect();
        vertex_pairs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        let mut new_vertices = vertices.to_vec();
        let top_count = vertex_pairs.len() / 2;
        for i in 0..top_count.min(bookmarked_vertices.len()) {
            let old_idx = vertex_pairs[i].0;
            if let Some(pos) = new_vertices.iter().position(|&idx| idx == old_idx) {
                new_vertices[pos] = bookmarked_vertices[i];
            }
        }
        new_vertices
    }

    fn vertex_to_2d(&self, vertex_3d: Vec3) -> Vec2 {
        Vec2::new(vertex_3d.x, vertex_3d.y)
    }

    fn create_vertical_surface_kind(
        &self,
        segment: &Segment,
        texture: Option<usize>,
        wall_type: WallType,
    ) -> SurfaceKind {
        let adjusted_tex_x_offset = segment.sidedef.textureoffset + segment.offset;
        SurfaceKind::Vertical {
            texture,
            tex_x_offset: adjusted_tex_x_offset,
            tex_y_offset: segment.sidedef.rowoffset,
            texture_direction: 0.0,
            wall_type,
            wall_tex_pin: WallTexPin::from(segment.linedef.flags),
        }
    }

    fn create_horizontal_surface_kind(texture: usize) -> SurfaceKind {
        const TEXTURE_DIRECTION: f32 = std::f32::consts::PI / 2.0;
        SurfaceKind::Horizontal {
            texture,
            texture_direction: TEXTURE_DIRECTION,
        }
    }

    fn vertices_equal_2d(&self, v1: Vec2, v2: Vec2, epsilon: f32) -> bool {
        (v1 - v2).length() < epsilon
    }

    fn vertex_equals_3d(&self, v1: Vec3, v2: Vec3, epsilon: f32) -> bool {
        (v1.x - v2.x).abs() < epsilon
            && (v1.y - v2.y).abs() < epsilon
            && (v1.z - v2.z).abs() < epsilon
    }

    fn find_vertex_at_2d_position(
        &self,
        vertices: &[usize],
        target_2d: Vec2,
        epsilon: f32,
    ) -> Option<usize> {
        for &vertex_idx in vertices {
            let vertex_2d = self.vertex_to_2d(self.vertices[vertex_idx]);
            if self.vertices_equal_2d(vertex_2d, target_2d, epsilon) {
                return Some(vertex_idx);
            }
        }
        None
    }

    fn create_wall_quad_triangles(
        &mut self,
        bottom_left: usize,
        bottom_right: usize,
        top_left: usize,
        top_right: usize,
        sector_id: usize,
        subsector_id: usize,
        surface_kind: SurfaceKind,
    ) -> (SurfacePolygon, SurfacePolygon) {
        let triangle1 = SurfacePolygon::new(
            vec![bottom_left, bottom_right, top_left],
            self,
            sector_id,
            subsector_id,
            surface_kind.clone(),
        );

        let triangle2 = SurfacePolygon::new(
            vec![top_left, bottom_right, top_right],
            self,
            sector_id,
            subsector_id,
            surface_kind,
        );

        (triangle1, triangle2)
    }

    fn insert_edge_vertices(&self, polygon: &[Vec2], height: f32) -> Vec<Vec2> {
        let mut expanded_polygon = Vec::new();
        const EPSILON: f32 = 0.01;

        for i in 0..polygon.len() {
            let current_vertex = polygon[i];
            let next_vertex = polygon[(i + 1) % polygon.len()];

            expanded_polygon.push(current_vertex);

            // Find vertices that lie on this edge
            let mut edge_vertices = Vec::new();

            for vertex_3d in &self.vertices {
                // Only consider vertices at the same height
                if (vertex_3d.z - height).abs() < EPSILON {
                    let vertex_2d = self.vertex_to_2d(*vertex_3d);

                    // Skip if vertex is already one of the edge endpoints
                    if self.vertices_equal_2d(vertex_2d, current_vertex, EPSILON)
                        || self.vertices_equal_2d(vertex_2d, next_vertex, EPSILON)
                    {
                        continue;
                    }

                    // Check if vertex lies on the edge
                    if self.point_lies_on_line_segment(
                        vertex_2d,
                        current_vertex,
                        next_vertex,
                        EPSILON,
                    ) {
                        let t = self.calculate_parameter_on_line(
                            vertex_2d,
                            current_vertex,
                            next_vertex,
                        );
                        edge_vertices.push((vertex_2d, t));
                    }
                }
            }

            // Sort vertices by parameter along edge
            edge_vertices.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

            // Insert sorted vertices
            for (vertex_2d, _) in edge_vertices {
                expanded_polygon.push(vertex_2d);
            }
        }

        expanded_polygon
    }

    fn point_lies_on_line_segment(
        &self,
        point: Vec2,
        start: Vec2,
        end: Vec2,
        epsilon: f32,
    ) -> bool {
        let edge_vec = end - start;
        let point_vec = point - start;

        // Check collinearity
        let cross = point_vec.x * edge_vec.y - point_vec.y * edge_vec.x;
        if cross.abs() > epsilon {
            return false;
        }

        // Check if within segment bounds
        let dot = point_vec.dot(edge_vec);
        let edge_length_sq = edge_vec.length_squared();

        dot >= -epsilon && dot <= edge_length_sq + epsilon
    }

    fn calculate_parameter_on_line(&self, point: Vec2, start: Vec2, end: Vec2) -> f32 {
        let edge_vec = end - start;
        let point_vec = point - start;
        point_vec.dot(edge_vec) / edge_vec.length_squared()
    }

    fn create_flush_wall_polygons(
        &mut self,
        segment: &Segment,
        top_vertex_indices: &[usize],
        bottom_vertex_indices: &[usize],
        texture: usize,
        sector_id: usize,
        subsector_id: usize,
    ) {
        if top_vertex_indices.len() != 2 || bottom_vertex_indices.len() != 2 {
            return; // Only handle 2-vertex segments
        }

        let surface_kind = SurfaceKind::Vertical {
            texture: Some(texture),
            tex_x_offset: 0.0,
            tex_y_offset: 0.0,
            texture_direction: 0.0,
            wall_type: WallType::Middle,
            wall_tex_pin: WallTexPin::from(segment.linedef.flags),
        };

        // Create two triangles for the wall quad
        let (wall_polygon1, wall_polygon2) = self.create_wall_quad_triangles(
            bottom_vertex_indices[0],
            bottom_vertex_indices[1],
            top_vertex_indices[0],
            top_vertex_indices[1],
            sector_id,
            subsector_id,
            surface_kind,
        );

        let leaf = &mut self.subsector_leaves[subsector_id];
        leaf.polygons.push(wall_polygon1);
        leaf.polygons.push(wall_polygon2);
    }
}

impl Default for BSP3D {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            subsector_leaves: Vec::new(),
            root_node: 0,
            pvs: PVS::new(0),
            vertices: Vec::new(),
            sector_subsectors: Vec::new(),
            flush_walls: Vec::new(),
            door_subsectors: Vec::new(),
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
