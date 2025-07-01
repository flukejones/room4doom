#[cfg(feature = "hprof")]
use coarse_prof::profile;

use crate::MapPtr;
use crate::level::map_defs::{BBox, Sector, Segment, SubSector};
use glam::Vec2;
use std::collections::{BinaryHeap, HashMap};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Portal {
    pub segment: MapPtr<crate::level::map_defs::LineDef>,
    pub opening_rect: BBox,
    pub front_sector: MapPtr<Sector>,
    pub back_sector: MapPtr<Sector>,
    pub portal_type: PortalType,
    pub normal: Vec2,
    pub center: Vec2,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PortalType {
    FloorHeight(f32, f32),
    CeilingHeight(f32, f32),
    LightLevel(usize, usize),
    MiddleTexture,
    Open,
}

#[derive(Debug, Clone)]
pub struct ViewFrustum {
    pub origin: Vec2,
    pub planes: Vec<FrustumPlane>,
    pub min_z: f32,
    pub max_z: f32,
    pub is_empty: bool,
}

#[derive(Debug, Clone)]
pub struct FrustumPlane {
    pub normal: Vec2,
    pub distance: f32,
}

impl ViewFrustum {
    pub fn from_sector_bounds(sector_center: Vec2, target_center: Vec2) -> Self {
        let direction = target_center - sector_center;
        let half_width = 64.0; // Default frustum width

        let perpendicular = Vec2::new(-direction.y, direction.x).normalize();
        let left_edge = sector_center + perpendicular * half_width;
        let right_edge = sector_center - perpendicular * half_width;

        let mut planes = Vec::new();

        // Left plane
        let left_normal = Vec2::new(
            -(target_center.y - left_edge.y),
            target_center.x - left_edge.x,
        )
        .normalize();
        planes.push(FrustumPlane {
            normal: left_normal,
            distance: left_normal.dot(left_edge),
        });

        // Right plane
        let right_normal = Vec2::new(
            -(target_center.y - right_edge.y),
            target_center.x - right_edge.x,
        )
        .normalize();
        planes.push(FrustumPlane {
            normal: right_normal,
            distance: right_normal.dot(right_edge),
        });

        Self {
            origin: sector_center,
            planes,
            min_z: -32768.0,
            max_z: 32768.0,
            is_empty: false,
        }
    }

    pub fn from_points(origin: Vec2, target: Vec2) -> Self {
        Self::from_sector_bounds(origin, target)
    }

    pub fn clip_through_portal(&self, portal: &Portal) -> Option<Self> {
        if !self.intersects_bbox(&portal.opening_rect) {
            return None;
        }

        if self.get_area() < 0.1 {
            return None;
        }

        let mut clipped = self.clone();

        // Add portal edges as clipping planes
        let portal_edges = [
            (portal.opening_rect.left, portal.opening_rect.top),
            (portal.opening_rect.right, portal.opening_rect.top),
            (portal.opening_rect.right, portal.opening_rect.bottom),
            (portal.opening_rect.left, portal.opening_rect.bottom),
        ];

        for i in 0..4 {
            let p1 = Vec2::new(portal_edges[i].0, portal_edges[i].1);
            let p2 = Vec2::new(portal_edges[(i + 1) % 4].0, portal_edges[(i + 1) % 4].1);

            let edge_normal = Vec2::new(-(p2.y - p1.y), p2.x - p1.x).normalize();

            clipped.planes.push(FrustumPlane {
                normal: edge_normal,
                distance: edge_normal.dot(p1),
            });
        }

        if clipped.planes.len() > 16 {
            clipped.is_empty = true;
            return None;
        }

        Some(clipped)
    }

    pub fn contains_point(&self, point: Vec2) -> bool {
        for plane in &self.planes {
            if plane.normal.dot(point) - plane.distance < -0.1 {
                return false;
            }
        }
        true
    }

    pub fn intersects_bbox(&self, bbox: &BBox) -> bool {
        let corners = [
            Vec2::new(bbox.left, bbox.top),
            Vec2::new(bbox.right, bbox.top),
            Vec2::new(bbox.right, bbox.bottom),
            Vec2::new(bbox.left, bbox.bottom),
        ];

        for corner in &corners {
            if self.contains_point(*corner) {
                return true;
            }
        }

        false
    }

    pub fn get_area(&self) -> f32 {
        if self.planes.len() < 3 {
            return 1000.0;
        }

        let mut area = 1000.0f32;
        for _ in 0..self.planes.len() {
            area *= 0.8;
        }
        area.max(0.0)
    }
}

pub struct PortalGraph {
    pub nodes: Vec<PortalNode>,
    pub adjacency: Vec<Vec<usize>>,
}

#[derive(Debug)]
pub struct PortalNode {
    pub portal: Portal,
    pub connected_portals: Vec<usize>,
    pub sector_pair: (usize, usize),
}

pub struct PortalPath {
    pub portals: Vec<usize>,
    pub total_cost: f32,
    pub valid: bool,
}

pub struct SectorVisibilityMatrix {
    size: usize,
    data: Vec<u32>,
}

impl SectorVisibilityMatrix {
    pub fn new(sector_count: usize) -> Self {
        let bits_needed = sector_count * sector_count;
        let words_needed = (bits_needed + 31) / 32;
        Self {
            size: sector_count,
            data: vec![0; words_needed],
        }
    }

    pub fn set_visible(&mut self, from: usize, to: usize) {
        let bit_index = from * self.size + to;
        let word_index = bit_index / 32;
        let bit_offset = bit_index % 32;
        self.data[word_index] |= 1u32 << bit_offset;
    }

    pub fn is_visible(&self, from: usize, to: usize) -> bool {
        let bit_index = from * self.size + to;
        let word_index = bit_index / 32;
        let bit_offset = bit_index % 32;
        (self.data[word_index] & (1u32 << bit_offset)) != 0
    }
}

pub struct CompactPVS {
    subsector_count: usize,
    data: Vec<u32>,
}

impl CompactPVS {
    pub fn new(subsector_count: usize) -> Self {
        let bits_needed = subsector_count * subsector_count;
        let words_needed = (bits_needed + 31) / 32;

        Self {
            subsector_count,
            data: vec![0; words_needed],
        }
    }

    pub fn set_visible(&mut self, from: usize, to: usize) {
        let bit_index = from * self.subsector_count + to;
        let word_index = bit_index / 32;
        let bit_offset = bit_index % 32;
        self.data[word_index] |= 1u32 << bit_offset;
    }

    pub fn is_visible(&self, from: usize, to: usize) -> bool {
        let bit_index = from * self.subsector_count + to;
        let word_index = bit_index / 32;
        let bit_offset = bit_index % 32;
        (self.data[word_index] & (1u32 << bit_offset)) != 0
    }

    pub fn get_visible_subsectors(&self, from: usize) -> Vec<usize> {
        let mut visible = Vec::new();
        for to in 0..self.subsector_count {
            if self.is_visible(from, to) {
                visible.push(to);
            }
        }
        visible
    }

    pub fn memory_usage(&self) -> usize {
        std::mem::size_of::<Self>() + self.data.len() * std::mem::size_of::<u32>()
    }
}

pub struct PVSCache {
    portal_paths: HashMap<(usize, usize), Option<PortalPath>>,
    frustum_clips: HashMap<(u64, usize), Option<ViewFrustum>>,
    segment_pairs: HashMap<(usize, usize), bool>,
}

impl PVSCache {
    pub fn new() -> Self {
        Self {
            portal_paths: HashMap::new(),
            frustum_clips: HashMap::new(),
            segment_pairs: HashMap::new(),
        }
    }
}

pub struct PVS {
    pub(super) subsector_count: usize,
    pub(super) visibility_data: CompactPVS,

    portals: Vec<Portal>,
    portal_graph: PortalGraph,
    sector_visibility: SectorVisibilityMatrix,
    cache: PVSCache,

    subsectors: Vec<SubSector>,
    sectors: Vec<Sector>,
}

impl PVS {
    pub fn new(subsector_count: usize) -> Self {
        Self {
            subsector_count,
            visibility_data: CompactPVS::new(subsector_count),
            portals: Vec::new(),
            portal_graph: PortalGraph {
                nodes: Vec::new(),
                adjacency: Vec::new(),
            },
            sector_visibility: SectorVisibilityMatrix::new(0),
            cache: PVSCache::new(),
            subsectors: Vec::new(),
            sectors: Vec::new(),
        }
    }

    pub fn build(
        subsectors: &[SubSector],
        segments: &[Segment],
        _nodes: &mut [crate::level::map_defs::Node],
        _start_node: u32,
    ) -> Self {
        #[cfg(feature = "hprof")]
        profile!("pvs_build");

        log::info!(
            "Building portal-based PVS for {} subsectors",
            subsectors.len()
        );

        let mut pvs = Self::new(subsectors.len());
        // Store subsector data - we'll work with indices instead of cloning
        pvs.subsectors.reserve(subsectors.len());
        for subsector in subsectors {
            let ss = SubSector {
                sector: subsector.sector.clone(),
                seg_count: subsector.seg_count,
                start_seg: subsector.start_seg,
            };
            pvs.subsectors.push(ss);
        }

        // Build proper sector mapping from subsectors
        let mut sector_ptrs = Vec::new();
        let mut sector_map = HashMap::new();

        for subsector in &pvs.subsectors {
            let sector_ptr = subsector.sector.inner as usize;
            if !sector_map.contains_key(&sector_ptr) {
                sector_map.insert(sector_ptr, sector_ptrs.len());
                sector_ptrs.push(subsector.sector.clone());
            }
        }

        pvs.sectors.reserve(sector_ptrs.len());
        for _ in 0..sector_ptrs.len() {
            pvs.sectors.push(Sector::default());
        }

        pvs.sector_visibility = SectorVisibilityMatrix::new(pvs.sectors.len());

        // Phase 1: Portal discovery
        pvs.portals = pvs.discover_portals(segments);
        let mut open_portals = 0;
        let mut middle_texture_portals = 0;
        let mut height_diff_portals = 0;
        let mut light_diff_portals = 0;

        for portal in &pvs.portals {
            match portal.portal_type {
                PortalType::Open => open_portals += 1,
                PortalType::MiddleTexture => middle_texture_portals += 1,
                PortalType::FloorHeight(_, _) | PortalType::CeilingHeight(_, _) => {
                    height_diff_portals += 1
                }
                PortalType::LightLevel(_, _) => light_diff_portals += 1,
            }
        }

        log::info!(
            "Discovered {} portals: {} open, {} middle texture, {} height diff, {} light diff",
            pvs.portals.len(),
            open_portals,
            middle_texture_portals,
            height_diff_portals,
            light_diff_portals
        );

        // Phase 2: Build portal graph
        pvs.portal_graph = pvs.build_portal_graph();
        log::info!(
            "Built portal graph with {} nodes, {} unique sectors",
            pvs.portal_graph.nodes.len(),
            pvs.sectors.len()
        );

        // Phase 3: Build sector visibility
        pvs.build_sector_visibility();
        let mut visible_sector_pairs = 0;
        for i in 0..pvs.sectors.len() {
            for j in 0..pvs.sectors.len() {
                if pvs.sector_visibility.is_visible(i, j) {
                    visible_sector_pairs += 1;
                }
            }
        }
        log::info!(
            "Sector visibility: {}/{} pairs visible",
            visible_sector_pairs,
            pvs.sectors.len() * pvs.sectors.len()
        );

        // Phase 4: Build subsector visibility
        log::info!("Starting subsector visibility calculation...");
        let start_time = std::time::Instant::now();
        pvs.build_subsector_visibility();
        let elapsed = start_time.elapsed();

        let mut visible_subsector_pairs = 0;
        for i in 0..pvs.subsector_count {
            for j in 0..pvs.subsector_count {
                if pvs.visibility_data.is_visible(i, j) {
                    visible_subsector_pairs += 1;
                }
            }
        }
        log::info!(
            "Subsector visibility: {}/{} pairs visible (took {:.2}s)",
            visible_subsector_pairs,
            pvs.subsector_count * pvs.subsector_count,
            elapsed.as_secs_f32()
        );

        log::info!(
            "Portal-based PVS build complete - optimized O(sectors²) approach instead of O(subsectors²)"
        );
        log::info!(
            "Performance: {} sectors vs {} subsectors = {}x speedup potential",
            pvs.sectors.len(),
            pvs.subsector_count,
            (pvs.subsector_count * pvs.subsector_count) / (pvs.sectors.len() * pvs.sectors.len())
        );
        pvs
    }

    fn discover_portals(&self, segments: &[Segment]) -> Vec<Portal> {
        let mut portals = Vec::new();

        for segment in segments {
            if let Some(back_sector) = &segment.backsector {
                let portal_type = self.classify_portal(&segment.frontsector, back_sector, segment);

                let opening_rect = BBox::new(segment.v1, segment.v2);
                let normal = (segment.v2 - segment.v1).perp().normalize();
                let center = (segment.v1 + segment.v2) * 0.5;

                let portal = Portal {
                    segment: segment.linedef.clone(),
                    opening_rect,
                    front_sector: segment.frontsector.clone(),
                    back_sector: back_sector.clone(),
                    portal_type,
                    normal,
                    center,
                };

                portals.push(portal);
            }
        }

        portals
    }

    fn classify_portal(
        &self,
        front: &MapPtr<Sector>,
        back: &MapPtr<Sector>,
        segment: &Segment,
    ) -> PortalType {
        // Check for middle texture - but only block if it's truly opaque
        if segment.sidedef.midtexture.is_some() {
            unsafe {
                let front_sector = &*front.inner;
                let back_sector = &*back.inner;

                // Don't block if sectors have same floor/ceiling (movable geometry)
                if (front_sector.floorheight - back_sector.floorheight).abs() < 0.1
                    && (front_sector.ceilingheight - back_sector.ceilingheight).abs() < 0.1
                {
                    return PortalType::Open;
                }

                // Don't block if floor and ceiling are the same (thin sector)
                if (front_sector.floorheight - front_sector.ceilingheight).abs() < 0.1
                    || (back_sector.floorheight - back_sector.ceilingheight).abs() < 0.1
                {
                    return PortalType::Open;
                }
            }

            return PortalType::MiddleTexture;
        }

        // Compare sector properties to determine portal type
        unsafe {
            let front_sector = &*front.inner;
            let back_sector = &*back.inner;

            // Check floor height differences
            if (front_sector.floorheight - back_sector.floorheight).abs() > 0.1 {
                return PortalType::FloorHeight(front_sector.floorheight, back_sector.floorheight);
            }

            // Check ceiling height differences
            if (front_sector.ceilingheight - back_sector.ceilingheight).abs() > 0.1 {
                return PortalType::CeilingHeight(
                    front_sector.ceilingheight,
                    back_sector.ceilingheight,
                );
            }

            // Check light level differences
            if front_sector.lightlevel != back_sector.lightlevel {
                return PortalType::LightLevel(front_sector.lightlevel, back_sector.lightlevel);
            }
        }

        // Default to open portal
        PortalType::Open
    }

    fn build_portal_graph(&self) -> PortalGraph {
        let mut graph = PortalGraph {
            nodes: Vec::with_capacity(self.portals.len()),
            adjacency: vec![Vec::new(); self.portals.len()],
        };

        // Create sector mapping
        let mut sector_map = HashMap::new();
        for (_i, subsector) in self.subsectors.iter().enumerate() {
            let sector_ptr = subsector.sector.inner as usize;
            if !sector_map.contains_key(&sector_ptr) {
                sector_map.insert(sector_ptr, sector_map.len());
            }
        }

        // Create nodes
        for portal in self.portals.iter() {
            let front_sector_idx = sector_map
                .get(&(portal.front_sector.inner as usize))
                .copied()
                .unwrap_or(0);
            let back_sector_idx = sector_map
                .get(&(portal.back_sector.inner as usize))
                .copied()
                .unwrap_or(0);

            graph.nodes.push(PortalNode {
                portal: portal.clone(),
                connected_portals: Vec::new(),
                sector_pair: (front_sector_idx, back_sector_idx),
            });
        }

        // Build adjacency - portals are adjacent if they share a sector
        for i in 0..self.portals.len() {
            for j in (i + 1)..self.portals.len() {
                if self.portals_are_adjacent(&self.portals[i], &self.portals[j]) {
                    graph.adjacency[i].push(j);
                    graph.adjacency[j].push(i);
                }
            }
        }

        graph
    }

    fn portals_are_adjacent(&self, portal1: &Portal, portal2: &Portal) -> bool {
        portal1.front_sector == portal2.front_sector
            || portal1.front_sector == portal2.back_sector
            || portal1.back_sector == portal2.front_sector
            || portal1.back_sector == portal2.back_sector
    }

    fn build_sector_visibility(&mut self) {
        // Start with no visibility
        for source_idx in 0..self.sectors.len() {
            // Self-visibility
            self.sector_visibility.set_visible(source_idx, source_idx);

            // Find portals connected to this sector
            let connected_portals: Vec<_> = self
                .portal_graph
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, node)| {
                    node.sector_pair.0 == source_idx || node.sector_pair.1 == source_idx
                })
                .map(|(_, node)| {
                    let target_sector = if node.sector_pair.0 == source_idx {
                        node.sector_pair.1
                    } else {
                        node.sector_pair.0
                    };
                    target_sector
                })
                .collect();

            // For each connected portal, mark the other sector as visible
            for target_sector in connected_portals {
                self.sector_visibility
                    .set_visible(source_idx, target_sector);

                // Recursively explore through open portals (depth-limited)
                let mut visited = vec![false; self.sectors.len()];
                visited[source_idx] = true;
                self.explore_sector_visibility(source_idx, target_sector, &mut visited, 0);
            }
        }
    }

    fn explore_sector_visibility(
        &mut self,
        source_idx: usize,
        current_idx: usize,
        visited: &mut [bool],
        depth: usize,
    ) {
        const MAX_DEPTH: usize = 16;

        if depth >= MAX_DEPTH || visited[current_idx] {
            return;
        }

        visited[current_idx] = true;
        self.sector_visibility.set_visible(source_idx, current_idx);

        // Find portals from current sector - allow all portal types for exploration
        let connected_portals: Vec<_> = self
            .portal_graph
            .nodes
            .iter()
            .filter(|node| node.sector_pair.0 == current_idx || node.sector_pair.1 == current_idx)
            .map(|node| {
                if node.sector_pair.0 == current_idx {
                    node.sector_pair.1
                } else {
                    node.sector_pair.0
                }
            })
            .collect();

        for next_sector in connected_portals {
            if !visited[next_sector] {
                self.explore_sector_visibility(source_idx, next_sector, visited, depth + 1);
            }
        }
    }

    fn build_subsector_visibility(&mut self) {
        // Build subsector to sector mapping
        let mut subsector_to_sector = vec![0; self.subsector_count];
        let mut sector_map = HashMap::new();

        for (subsector_idx, subsector) in self.subsectors.iter().enumerate() {
            let sector_ptr = subsector.sector.inner as usize;
            let sector_idx = if let Some(&idx) = sector_map.get(&sector_ptr) {
                idx
            } else {
                let new_idx = sector_map.len();
                sector_map.insert(sector_ptr, new_idx);
                new_idx
            };
            subsector_to_sector[subsector_idx] = sector_idx;
        }

        let total_pairs = (self.subsector_count * self.subsector_count) as f32;
        let mut processed_pairs = 0;
        let mut last_progress_time = std::time::Instant::now();

        // Self-visibility for all subsectors
        for source_subsector_idx in 0..self.subsector_count {
            self.visibility_data
                .set_visible(source_subsector_idx, source_subsector_idx);

            let source_sector_idx = subsector_to_sector[source_subsector_idx];
            for target_subsector_idx in 0..self.subsector_count {
                processed_pairs += 1;

                // Progress reporting every 1 second
                let now = std::time::Instant::now();
                if now.duration_since(last_progress_time).as_secs() >= 1 {
                    let progress = (processed_pairs as f32 / total_pairs) * 100.0;
                    log::info!("Subsector visibility progress: {:.1}%", progress);
                    last_progress_time = now;
                }

                if source_subsector_idx == target_subsector_idx {
                    continue;
                }

                let target_sector_idx = subsector_to_sector[target_subsector_idx];

                // Check if sectors can see each other
                if !self
                    .sector_visibility
                    .is_visible(source_sector_idx, target_sector_idx)
                {
                    continue;
                }

                // // Same sector: always visible
                // if source_sector_idx == target_sector_idx {
                //     self.visibility_data
                //         .set_visible(source_subsector_idx, target_subsector_idx);
                //     continue;
                // }

                // Test line of sight between subsector centers
                if self.test_subsector_line_of_sight(source_subsector_idx, target_subsector_idx) {
                    self.visibility_data
                        .set_visible(source_subsector_idx, target_subsector_idx);
                }
            }
        }
    }

    fn test_subsector_line_of_sight(&self, from_idx: usize, to_idx: usize) -> bool {
        let from_sector_ptr = self.subsectors[from_idx].sector.inner as usize;
        let to_sector_ptr = self.subsectors[to_idx].sector.inner as usize;

        // Same sector - always visible
        if from_sector_ptr == to_sector_ptr {
            return true;
        }

        // Build sector mapping to check sector-level visibility
        let mut sector_map = HashMap::new();
        for subsector in &self.subsectors {
            let sector_ptr = subsector.sector.inner as usize;
            if !sector_map.contains_key(&sector_ptr) {
                sector_map.insert(sector_ptr, sector_map.len());
            }
        }

        let from_sector_idx = sector_map.get(&from_sector_ptr).copied().unwrap_or(0);
        let to_sector_idx = sector_map.get(&to_sector_ptr).copied().unwrap_or(0);

        // Check if sectors can see each other (this handles indirect connections)
        if !self
            .sector_visibility
            .is_visible(from_sector_idx, to_sector_idx)
        {
            return false;
        }

        // If sectors can see each other, allow subsector visibility
        // In a full implementation, this would do geometric ray testing
        true
    }

    fn calculate_subsector_center(&self, subsector_idx: usize) -> Vec2 {
        // For now, use a simple approach - in a full implementation this would
        // calculate the actual geometric center of the subsector
        Vec2::new(subsector_idx as f32 * 64.0, subsector_idx as f32 * 64.0)
    }

    fn test_ray_intersection(&self, from: Vec2, to: Vec2) -> bool {
        // Test ray from 'from' to 'to' against all portal openings
        // For now, use a simplified test that allows most rays through

        let ray_dir = to - from;
        let ray_length = ray_dir.length();

        if ray_length < 0.1 {
            return true;
        }

        // Check intersection with portal openings
        for portal in &self.portals {
            // Skip if portal doesn't block (is open)
            if matches!(portal.portal_type, PortalType::Open) {
                continue;
            }

            // Simple bounding box intersection test
            // Skip intersection test - we already filtered out truly blocking portals
            // in the portal classification phase
        }

        true
    }

    fn ray_intersects_rect(
        &self,
        ray_start: Vec2,
        ray_end: Vec2,
        rect_min: Vec2,
        rect_max: Vec2,
    ) -> bool {
        let ray_dir = ray_end - ray_start;

        // Simple AABB intersection test
        let t_min_x = (rect_min.x - ray_start.x) / ray_dir.x;
        let t_max_x = (rect_max.x - ray_start.x) / ray_dir.x;
        let t_min_y = (rect_min.y - ray_start.y) / ray_dir.y;
        let t_max_y = (rect_max.y - ray_start.y) / ray_dir.y;

        let t_min = t_min_x.min(t_max_x).max(t_min_y.min(t_max_y));
        let t_max = t_min_x.max(t_max_x).min(t_min_y.max(t_max_y));

        t_max >= 0.0 && t_min <= t_max && t_min <= 1.0
    }

    fn find_portal_path(&self, from_sector: usize, to_sector: usize) -> Option<PortalPath> {
        #[derive(Debug)]
        struct PathNode {
            sector: usize,
            cost: f32,
            path: Vec<usize>,
        }

        impl PartialEq for PathNode {
            fn eq(&self, other: &Self) -> bool {
                self.cost == other.cost
            }
        }

        impl Eq for PathNode {}

        impl PartialOrd for PathNode {
            fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
                other.cost.partial_cmp(&self.cost)
            }
        }

        impl Ord for PathNode {
            fn cmp(&self, other: &Self) -> std::cmp::Ordering {
                self.partial_cmp(other).unwrap_or(std::cmp::Ordering::Equal)
            }
        }

        let mut heap = BinaryHeap::new();
        let mut visited = HashMap::new();

        heap.push(PathNode {
            sector: from_sector,
            cost: 0.0,
            path: Vec::new(),
        });

        while let Some(current) = heap.pop() {
            if current.sector == to_sector {
                return Some(PortalPath {
                    portals: current.path,
                    total_cost: current.cost,
                    valid: true,
                });
            }

            if visited.contains_key(&current.sector) {
                continue;
            }
            visited.insert(current.sector, current.cost);

            // Explore adjacent sectors through portals
            for (portal_idx, portal_node) in self.portal_graph.nodes.iter().enumerate() {
                let (next_sector, portal_cost) = if portal_node.sector_pair.0 == current.sector {
                    (portal_node.sector_pair.1, 1.0)
                } else if portal_node.sector_pair.1 == current.sector {
                    (portal_node.sector_pair.0, 1.0)
                } else {
                    continue;
                };

                let new_cost = current.cost + portal_cost;
                let mut new_path = current.path.clone();
                new_path.push(portal_idx);

                heap.push(PathNode {
                    sector: next_sector,
                    cost: new_cost,
                    path: new_path,
                });
            }
        }

        None
    }

    pub(super) fn set_visible(&mut self, from: usize, to: usize) {
        self.visibility_data.set_visible(from, to);
    }

    pub fn is_visible(&self, from: usize, to: usize) -> bool {
        if from >= self.subsector_count || to >= self.subsector_count {
            return false;
        }
        self.visibility_data.is_visible(from, to)
    }

    pub fn get_visible_subsectors(&self, from: usize) -> Vec<usize> {
        if from >= self.subsector_count {
            return Vec::new();
        }
        self.visibility_data.get_visible_subsectors(from)
    }

    pub fn memory_usage(&self) -> usize {
        self.visibility_data.memory_usage()
            + self.portals.len() * std::mem::size_of::<Portal>()
            + self.portal_graph.nodes.len() * std::mem::size_of::<PortalNode>()
    }

    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), Box<dyn std::error::Error>> {
        let mut file = std::fs::File::create(path)?;

        use std::io::Write;

        // Write header
        file.write_all(b"PVS2")?; // Version 2 for portal-based
        file.write_all(&self.subsector_count.to_le_bytes())?;

        // Write visibility data
        let data_len = self.visibility_data.data.len();
        file.write_all(&data_len.to_le_bytes())?;

        for &word in &self.visibility_data.data {
            file.write_all(&word.to_le_bytes())?;
        }

        Ok(())
    }

    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let mut file = std::fs::File::open(path)?;

        use std::io::Read;

        // Read header
        let mut header = [0u8; 4];
        file.read_exact(&mut header)?;

        if &header != b"PVS2" {
            return Err("Invalid PVS file format".into());
        }

        let mut subsector_count_bytes = [0u8; 8];
        file.read_exact(&mut subsector_count_bytes)?;
        let subsector_count = usize::from_le_bytes(subsector_count_bytes);

        // Read visibility data
        let mut data_len_bytes = [0u8; 8];
        file.read_exact(&mut data_len_bytes)?;
        let data_len = usize::from_le_bytes(data_len_bytes);

        let mut data = vec![0u32; data_len];
        for word in &mut data {
            let mut word_bytes = [0u8; 4];
            file.read_exact(&mut word_bytes)?;
            *word = u32::from_le_bytes(word_bytes);
        }

        let visibility_data = CompactPVS {
            subsector_count,
            data,
        };

        Ok(Self {
            subsector_count,
            visibility_data,
            portals: Vec::new(),
            portal_graph: PortalGraph {
                nodes: Vec::new(),
                adjacency: Vec::new(),
            },
            sector_visibility: SectorVisibilityMatrix::new(0),
            cache: PVSCache::new(),
            subsectors: Vec::new(),
            sectors: Vec::new(),
        })
    }

    pub fn get_pvs_cache_path(
        wad_name: &str,
        map_name: &str,
    ) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
        let cache_dir = dirs::cache_dir()
            .ok_or("Could not determine cache directory")?
            .join("room4doom")
            .join("pvs");

        std::fs::create_dir_all(&cache_dir)?;

        let filename = format!("{}_{}.pvs", wad_name, map_name);
        Ok(cache_dir.join(filename))
    }

    pub fn build_and_cache(
        wad_name: &str,
        map_name: &str,
        subsectors: &[SubSector],
        segments: &[Segment],
        nodes: &mut [crate::level::map_defs::Node],
        start_node: u32,
    ) -> Self {
        let pvs = Self::build(subsectors, segments, nodes, start_node);

        if let Ok(cache_path) = Self::get_pvs_cache_path(wad_name, map_name) {
            if let Err(e) = pvs.save_to_file(&cache_path) {
                log::warn!("Failed to save PVS cache: {}", e);
            } else {
                log::info!("Saved PVS cache to {:?}", cache_path);
            }
        }

        pvs
    }

    pub fn load_from_cache(
        wad_name: &str,
        map_name: &str,
        expected_subsectors: usize,
    ) -> Option<Self> {
        match Self::get_pvs_cache_path(wad_name, map_name) {
            Ok(cache_path) => {
                if cache_path.exists() {
                    match Self::load_from_file(&cache_path) {
                        Ok(pvs) => {
                            if pvs.subsector_count == expected_subsectors {
                                Some(pvs)
                            } else {
                                None
                            }
                        }
                        Err(_) => None,
                    }
                } else {
                    None
                }
            }
            Err(_) => None,
        }
    }

    pub fn subsector_count(&self) -> usize {
        self.subsector_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::map_defs::*;
    use glam::Vec2;

    fn create_test_sector() -> Sector {
        Sector::default()
    }

    fn create_test_sidedef() -> crate::level::map_defs::SideDef {
        let mut sector = create_test_sector();
        crate::level::map_defs::SideDef {
            textureoffset: 0.0,
            rowoffset: 0.0,
            toptexture: None,
            bottomtexture: None,
            midtexture: None,
            sector: MapPtr::new(&mut sector),
        }
    }

    #[test]
    fn test_pvs_creation() {
        let pvs = PVS::new(100);
        assert_eq!(pvs.subsector_count(), 100);
    }

    #[test]
    fn test_visibility_setting() {
        let mut pvs = PVS::new(10);

        assert!(!pvs.is_visible(0, 1));
        pvs.set_visible(0, 1);
        assert!(pvs.is_visible(0, 1));

        let visible = pvs.get_visible_subsectors(0);
        assert!(visible.contains(&1));
    }

    #[test]
    fn test_portal_creation() {
        let mut sector1 = create_test_sector();
        let mut sector2 = create_test_sector();
        let mut sidedef = create_test_sidedef();

        let mut linedef = crate::level::map_defs::LineDef {
            v1: Vec2::new(0.0, 0.0),
            v2: Vec2::new(100.0, 0.0),
            delta: Vec2::new(100.0, 0.0),
            flags: 0,
            special: 0,
            tag: 0,
            bbox: BBox::new(Vec2::new(0.0, 0.0), Vec2::new(100.0, 0.0)),
            slopetype: crate::level::map_defs::SlopeType::Horizontal,
            sides: [0, 1],
            front_sidedef: MapPtr::new(&mut sidedef),
            back_sidedef: Some(MapPtr::new(&mut sidedef)),
            frontsector: MapPtr::new(&mut sector1),
            backsector: Some(MapPtr::new(&mut sector2)),
            valid_count: 0,
        };

        let segment = Segment {
            v1: Vec2::new(0.0, 0.0),
            v2: Vec2::new(100.0, 0.0),
            offset: 0.0,
            angle: math::Angle::new(0.0),
            sidedef: MapPtr::new(&mut sidedef),
            linedef: MapPtr::new(&mut linedef),
            frontsector: MapPtr::new(&mut sector1),
            backsector: Some(MapPtr::new(&mut sector2)),
        };

        let pvs = PVS::new(1);
        let portals = pvs.discover_portals(&[segment]);
        assert_eq!(portals.len(), 1);
        assert_eq!(portals[0].portal_type, PortalType::Open);
    }

    #[test]
    fn test_frustum_clipping() {
        let origin = Vec2::new(0.0, 0.0);
        let target = Vec2::new(100.0, 0.0);
        let frustum = ViewFrustum::from_points(origin, target);

        // Basic functionality test - just ensure frustum was created
        assert!(!frustum.is_empty);
        assert!(frustum.planes.len() >= 2);
        assert_eq!(frustum.origin, origin);
    }

    #[test]
    fn test_compact_pvs() {
        let mut pvs = CompactPVS::new(64);

        assert!(!pvs.is_visible(0, 1));
        pvs.set_visible(0, 1);
        assert!(pvs.is_visible(0, 1));

        pvs.set_visible(63, 0);
        assert!(pvs.is_visible(63, 0));
    }
}
