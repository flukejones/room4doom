use crate::level::map_defs::{Node, Sector, SubSector};
use crate::level::node;
use crate::level::triangulation::carve_subsector_polygon;
use crate::{DivLine, LineDefFlags, PVS, PicData, Segment};
use glam::{Vec2, Vec3};
use wad::WadData;

const IS_SUBSECTOR_MASK: u32 = 0x8000_0000;

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
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum WallType {
    Top,
    Bottom,
    Middle,
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
    pub vertices: Vec<Vec3>,
    pub normal: Vec3,
    pub aabb: AABB,
}

impl SurfacePolygon {
    fn new(
        v0: Vec3,
        v1: Vec3,
        v2: Vec3,
        sector_id: usize,
        subsector_id: usize,
        surface_kind: SurfaceKind,
    ) -> Self {
        let vertices = vec![v0, v1, v2];
        let mut aabb = AABB::new();
        for vertex in &vertices {
            aabb.expand_to_include_point(*vertex);
        }

        let v1 = vertices[1] - vertices[0];
        let v2 = vertices[2] - vertices[0];
        let normal = Vec3::new(
            v1.y * v2.z - v1.z * v2.y,
            v1.z * v2.x - v1.x * v2.z,
            v1.x * v2.y - v1.y * v2.x,
        )
        .normalize();

        Self {
            vertices,
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
        sector_id: usize,
        subsector_id: usize,
        surface_kind: SurfaceKind,
    ) -> Self {
        let vertices = vec![
            Vec3::new(v0.x, v0.y, height),
            Vec3::new(v1.x, v1.y, height),
            Vec3::new(v2.x, v2.y, height),
        ];

        let mut aabb = AABB::new();
        for vertex in &vertices {
            aabb.expand_to_include_point(*vertex);
        }

        let v1 = vertices[1] - vertices[0];
        let v2 = vertices[2] - vertices[0];
        let normal = Vec3::new(
            v1.y * v2.z - v1.z * v2.y,
            v1.z * v2.x - v1.x * v2.z,
            v1.x * v2.y - v1.y * v2.x,
        )
        .normalize();

        Self {
            vertices,
            normal,
            aabb,
            sector_id,
            subsector_id,
            surface_kind,
        }
    }

    /// True if the right side of the segment faces the point
    pub fn is_facing_point(&self, point: Vec3) -> bool {
        let view_vector = (point - self.vertices[0]).normalize_or_zero();
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

pub struct BSP3D {
    nodes: Vec<Node3D>,
    subsector_leaves: Vec<BSPLeaf3D>,
    root_node: u32,
    pvs: PVS,
}

impl BSP3D {
    pub fn new(
        map_name: &str,
        root_node: u32,
        nodes: &[Node],
        subsectors: &[SubSector],
        segments: &[crate::level::map_defs::Segment],
        sectors: &[Sector],
        wad: &WadData,
        pic_data: &PicData,
    ) -> Self {
        let mut bsp3d = Self {
            nodes: Vec::with_capacity(nodes.len()),
            subsector_leaves: Vec::with_capacity(nodes.len()),
            root_node: 0,
            pvs: PVS::new(0),
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
            root_node,
            Vec::new(),
            pic_data,
        );
        bsp3d.update_nodes_aabbs(root_node);

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

    pub fn get_subsector_leaf_count(&self) -> usize {
        self.subsector_leaves.len()
    }

    pub fn get_subsector_aabb(&self, subsector_id: usize) -> Option<&AABB> {
        self.subsector_leaves
            .get(subsector_id)
            .map(|geometry| &geometry.aabb)
    }

    fn compute_leaf_aabb(&mut self, leaf_id: usize) {
        if let Some(leaf) = self.subsector_leaves.get_mut(leaf_id) {
            if leaf.polygons.is_empty() {
                return;
            }

            let mut aabb = AABB::new();
            for polygon in &leaf.polygons {
                for vertex in &polygon.vertices {
                    aabb.expand_to_include_point(*vertex);
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

    fn sector_overlaps_bbox(&self, _sector: &Sector, _min_2d: Vec2, _max_2d: Vec2) -> bool {
        // For now, assume all sectors overlap all bboxes
        // This is conservative but correct
        true
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

        if polygon.len() < 3 {
            // Ensure vector is large enough and store the leaf even if empty
            if subsector_id >= self.subsector_leaves.len() {
                self.subsector_leaves.resize(subsector_id + 1, leaf.clone());
            }
            self.subsector_leaves[subsector_id] = leaf;
            return;
        }

        // Generate polygons for this subsector
        for i in 1..polygon.len() - 1 {
            // Use fixed north direction for all floor textures
            let floor_texture_direction = std::f32::consts::PI / 2.0;

            // Floor polygon (normal pointing up)
            let surface_polygon = SurfacePolygon::from_2d_with_height(
                polygon[0],
                polygon[i + 1],
                polygon[i],
                subsector.sector.floorheight,
                subsector.sector.num as usize,
                subsector_id,
                SurfaceKind::Horizontal {
                    texture: subsector.sector.floorpic,
                    texture_direction: floor_texture_direction,
                },
            );

            leaf.polygons.push(surface_polygon);

            // Use fixed north direction for all ceiling textures
            let ceiling_texture_direction = std::f32::consts::PI / 2.0;

            // Ceiling polygon (reverse winding, normal pointing down)
            let surface_polygon = SurfacePolygon::from_2d_with_height(
                polygon[i],
                polygon[i + 1],
                polygon[0],
                subsector.sector.ceilingheight,
                subsector.sector.num as usize,
                subsector_id,
                SurfaceKind::Horizontal {
                    texture: subsector.sector.ceilingpic,
                    texture_direction: ceiling_texture_direction,
                },
            );

            leaf.polygons.push(surface_polygon);
        }

        // Generate wall polygons from segments
        for segment in segments {
            let adjusted_tex_x_offset = segment.sidedef.textureoffset + segment.offset;

            // Two-sided wall - generate upper and lower wall sections
            if let Some(back_sector) = &segment.backsector {
                let front_floor = subsector.sector.floorheight;
                let front_ceiling = subsector.sector.ceilingheight;
                let back_floor = back_sector.floorheight;
                let back_ceiling = back_sector.ceilingheight;

                // Lower wall (if back floor is higher)
                if back_floor > front_floor {
                    if let Some(tex_id) = segment.sidedef.bottomtexture {
                        let texture = pic_data.get_texture(tex_id);
                        let tex_height = texture.data[0].len() as f32;

                        let wall_polygons = self.create_wall_polygons(
                            segment.v1,
                            segment.v2,
                            front_floor,
                            back_floor,
                            subsector.sector.num as usize,
                            subsector_id,
                            SurfaceKind::Vertical {
                                tex_x_offset: adjusted_tex_x_offset,
                                // Set the start point of texture
                                tex_y_offset: if segment.linedef.flags
                                    & LineDefFlags::UnpegBottom as u32
                                    != 0
                                {
                                    segment.sidedef.rowoffset + front_ceiling + tex_height
                                } else {
                                    segment.sidedef.rowoffset + back_floor
                                },
                                texture: segment.sidedef.bottomtexture,
                                texture_direction: 0.0,
                            },
                        );
                        for wall_polygon in wall_polygons {
                            leaf.polygons.push(wall_polygon);
                        }
                    }
                }

                // Upper wall (if back ceiling is lower)
                if back_ceiling < front_ceiling {
                    let wall_polygons = self.create_wall_polygons(
                        segment.v1,
                        segment.v2,
                        back_ceiling,
                        front_ceiling,
                        subsector.sector.num as usize,
                        subsector_id,
                        SurfaceKind::Vertical {
                            tex_x_offset: adjusted_tex_x_offset,
                            tex_y_offset: if segment.linedef.flags & LineDefFlags::UnpegTop as u32
                                != 0
                            {
                                segment.sidedef.rowoffset + front_ceiling
                            } else {
                                segment.sidedef.rowoffset + back_ceiling
                            },
                            texture: if back_sector.ceilingpic == pic_data.sky_num()
                                || back_sector.floorpic == pic_data.sky_num()
                            {
                                Some(pic_data.sky_pic())
                            } else {
                                segment.sidedef.toptexture
                            },
                            texture_direction: 0.0,
                        },
                    );
                    for surface_polygon in wall_polygons {
                        leaf.polygons.push(surface_polygon);
                    }
                }

                // If there is a middle texture it's most likely a masked texture
                // so we need to set the maximum size as a polygon
                if let Some(tex_id) = segment.sidedef.midtexture {
                    let texture = pic_data.get_texture(tex_id);
                    let tex_height = texture.data[0].len() as f32;

                    // default, no upper or lower textures
                    let mut world_bot = front_ceiling - tex_height;
                    let mut world_top = front_ceiling;
                    let mut rowoffset = world_top + segment.sidedef.rowoffset;
                    if segment.frontsector.ceilingpic == pic_data.sky_num()
                        && back_sector.ceilingpic == pic_data.sky_num()
                    {
                        // if the masked texture is in a wall with no upper, and high ceiling (sky)
                        world_bot = (back_floor).max(front_floor);
                        world_top = front_ceiling.min(back_ceiling) + segment.sidedef.rowoffset;
                        rowoffset = world_top;
                    } else if back_ceiling < front_ceiling {
                        // if the masked texture is in a wall with upper and lower
                        world_bot = back_floor.max(back_ceiling - tex_height);
                        world_top = back_ceiling;
                        rowoffset = world_top;
                    }

                    let wall_polygons = self.create_wall_polygons(
                        segment.v1,
                        segment.v2,
                        world_bot,
                        world_top,
                        subsector.sector.num as usize,
                        subsector_id,
                        SurfaceKind::Vertical {
                            tex_x_offset: adjusted_tex_x_offset,
                            // Set the start point of texture
                            tex_y_offset: rowoffset,
                            texture: segment.sidedef.midtexture,
                            texture_direction: 0.0,
                        },
                    );
                    for wall_polygon in wall_polygons {
                        leaf.polygons.push(wall_polygon);
                    }
                }
            } else {
                // One-sided wall - full height
                let wall_polygons = self.create_wall_polygons(
                    segment.v1,
                    segment.v2,
                    subsector.sector.floorheight,
                    subsector.sector.ceilingheight,
                    subsector.sector.num as usize,
                    subsector_id,
                    SurfaceKind::Vertical {
                        tex_x_offset: segment.sidedef.textureoffset,
                        tex_y_offset: if segment.linedef.flags & LineDefFlags::UnpegBottom as u32
                            != 0
                        {
                            segment.sidedef.rowoffset + subsector.sector.floorheight
                        } else {
                            segment.sidedef.rowoffset + subsector.sector.ceilingheight
                        },
                        texture: segment.sidedef.midtexture,
                        texture_direction: 0.0,
                    },
                );
                for surface_polygon in wall_polygons {
                    leaf.polygons.push(surface_polygon);
                }
            }
        }

        // Update AABB from generated polygons if any were created
        if !leaf.polygons.is_empty() {
            let mut aabb = AABB::new();
            for polygon in &leaf.polygons {
                for vertex in &polygon.vertices {
                    aabb.expand_to_include_point(*vertex);
                }
            }
            leaf.aabb = aabb;
        }

        // Ensure vector is large enough
        if subsector_id >= self.subsector_leaves.len() {
            self.subsector_leaves.resize(subsector_id + 1, leaf.clone());
        }
        self.subsector_leaves[subsector_id] = leaf;
    }

    /// BSP traversal to collect dividing lines and generate polygons
    fn carve_polygons_recursive(
        &mut self,
        nodes: &[Node],
        subsectors: &[SubSector],
        segments: &[Segment],
        node_id: u32,
        divlines: Vec<DivLine>,
        pic_data: &PicData,
    ) {
        // Check if this is a subsector
        if node_id & IS_SUBSECTOR_MASK != 0 {
            // We've reached a subsector - generate its polygon
            let subsector_id = if node_id == u32::MAX {
                0
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
                node.children[1],
                left_divlines,
                pic_data,
            );
        }
    }

    /// Create wall polygons from two 2D points and height range
    fn create_wall_polygons(
        &self,
        v1: Vec2,
        v2: Vec2,
        bottom_z: f32,
        top_z: f32,
        sector_id: usize,
        subsector_id: usize,
        surface_kind: SurfaceKind,
    ) -> Vec<SurfacePolygon> {
        let wall_dir = v2 - v1;
        let texture_direction = wall_dir.y.atan2(wall_dir.x);

        let surface_kind_with_direction = match surface_kind {
            SurfaceKind::Vertical {
                texture,
                tex_x_offset: textureoffset,
                tex_y_offset: rowoffset,
                ..
            } => SurfaceKind::Vertical {
                texture,
                tex_x_offset: textureoffset,
                tex_y_offset: rowoffset,
                texture_direction,
            },
            other => other,
        };
        let bottom_left = Vec3::new(v1.x, v1.y, bottom_z);
        let bottom_right = Vec3::new(v2.x, v2.y, bottom_z);
        let top_left = Vec3::new(v1.x, v1.y, top_z);
        let top_right = Vec3::new(v2.x, v2.y, top_z);

        vec![
            // First triangle: bottom-left, bottom-right, top-left
            SurfacePolygon::new(
                bottom_left,
                bottom_right,
                top_left,
                sector_id,
                subsector_id,
                surface_kind_with_direction.clone(),
            ),
            // Second triangle: top-left, bottom-right, top-right
            SurfacePolygon::new(
                top_left,
                bottom_right,
                top_right,
                sector_id,
                subsector_id,
                surface_kind_with_direction,
            ),
        ]
    }
}

impl Default for BSP3D {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            subsector_leaves: Vec::new(),
            root_node: 0,
            pvs: PVS::new(0),
        }
    }
}

impl Default for BSPLeaf3D {
    fn default() -> Self {
        Self {
            polygons: Vec::new(),
            aabb: AABB::new(),
        }
    }
}
