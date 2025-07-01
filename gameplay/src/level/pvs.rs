#[cfg(feature = "hprof")]
use coarse_prof::profile;

use crate::MapPtr;
use crate::level::map_defs::{BBox, Sector, Segment, SubSector};
use glam::Vec2;
use std::collections::HashMap;
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
        #[cfg(feature = "hprof")]
        profile!("sector_visibility_matrix_set_visible");
        let bit_index = from * self.size + to;
        let word_index = bit_index / 32;
        let bit_offset = bit_index % 32;
        self.data[word_index] |= 1u32 << bit_offset;
    }

    pub fn is_visible(&self, from: usize, to: usize) -> bool {
        #[cfg(feature = "hprof")]
        profile!("sector_visibility_matrix_is_visible");
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
        #[cfg(feature = "hprof")]
        profile!("compact_pvs_new");
        let bit_count = subsector_count * subsector_count;
        let words_needed = (bit_count + 31) / 32;

        Self {
            subsector_count,
            data: vec![0; words_needed],
        }
    }

    pub fn set_visible(&mut self, from: usize, to: usize) {
        #[cfg(feature = "hprof")]
        profile!("compact_pvs_set_visible");
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
        #[cfg(feature = "hprof")]
        profile!("compact_pvs_get_visible_subsectors");
        let mut visible = Vec::new();
        for to in 0..self.subsector_count {
            if self.is_visible(from, to) {
                visible.push(to);
            }
        }
        visible
    }

    pub fn memory_usage(&self) -> usize {
        #[cfg(feature = "hprof")]
        profile!("compact_pvs_memory_usage");
        std::mem::size_of::<Self>() + self.data.len() * std::mem::size_of::<u32>()
    }
}

pub struct PVS {
    pub(super) subsector_count: usize,
    pub(super) visibility_data: CompactPVS,

    portals: Vec<Portal>,
    portal_graph: PortalGraph,
    sector_visibility: SectorVisibilityMatrix,

    subsectors: Vec<SubSector>,
    sectors: Vec<Sector>,
    segments: Vec<Segment>,
    subsector_aabbs: Vec<BBox>,
    segment_bboxes: Vec<BBox>,
    blocking_segments: Vec<usize>,
}

impl PVS {
    pub fn new(subsector_count: usize) -> Self {
        #[cfg(feature = "hprof")]
        profile!("pvs_new");
        Self {
            subsector_count,
            visibility_data: CompactPVS::new(subsector_count),
            portals: Vec::new(),
            portal_graph: PortalGraph {
                nodes: Vec::new(),
                adjacency: Vec::new(),
            },
            sector_visibility: SectorVisibilityMatrix::new(0),

            subsectors: Vec::new(),
            sectors: Vec::new(),
            segments: Vec::new(),
            subsector_aabbs: Vec::new(),
            segment_bboxes: Vec::new(),
            blocking_segments: Vec::new(),
        }
    }

    pub fn build(
        subsectors: &[SubSector],
        segments: &[Segment],
        nodes: &mut [crate::level::map_defs::Node],
        start_node: u32,
    ) -> Self {
        #[cfg(feature = "hprof")]
        profile!("pvs_build");

        log::info!(
            "Building portal-based PVS for {} subsectors",
            subsectors.len()
        );

        let phase_start = std::time::Instant::now();

        let mut pvs = Self::new(subsectors.len());
        // Store subsector and segment data
        pvs.subsectors.reserve(subsectors.len());
        for subsector in subsectors {
            let ss = SubSector {
                sector: subsector.sector.clone(),
                seg_count: subsector.seg_count,
                start_seg: subsector.start_seg,
            };
            pvs.subsectors.push(ss);
        }

        pvs.segments.reserve(segments.len());
        for segment in segments {
            let seg = Segment {
                v1: segment.v1,
                v2: segment.v2,
                offset: segment.offset,
                angle: segment.angle,
                sidedef: segment.sidedef.clone(),
                linedef: segment.linedef.clone(),
                frontsector: segment.frontsector.clone(),
                backsector: segment.backsector.clone(),
            };
            pvs.segments.push(seg);
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

        // Pre-calculate BSP-based AABBs for all subsectors with orphaned space fixing
        log::info!("Pre-calculating BSP AABBs...");
        let bsp_start = std::time::Instant::now();
        let mut subsector_aabbs = Vec::with_capacity(subsectors.len());
        for (subsector_idx, _subsector) in subsectors.iter().enumerate() {
            let bsp_aabb = pvs.find_and_fix_subsector_bsp_aabb(subsector_idx, nodes, start_node);
            subsector_aabbs.push(bsp_aabb);
        }

        // Store the BSP AABBs for later use
        pvs.subsector_aabbs = subsector_aabbs;
        log::info!(
            "BSP AABB calculation took {:.2}s",
            bsp_start.elapsed().as_secs_f32()
        );

        // Precompute segment bounding boxes and identify blocking segments
        log::info!("Precomputing segment optimizations...");
        let seg_opt_start = std::time::Instant::now();
        pvs.segment_bboxes.reserve(pvs.segments.len());
        for (idx, segment) in pvs.segments.iter().enumerate() {
            let bbox = BBox {
                left: segment.v1.x.min(segment.v2.x),
                right: segment.v1.x.max(segment.v2.x),
                bottom: segment.v1.y.min(segment.v2.y),
                top: segment.v1.y.max(segment.v2.y),
            };
            pvs.segment_bboxes.push(bbox);

            // Only blocking segments (one-sided walls) matter for ray intersection
            if segment.backsector.is_none() {
                pvs.blocking_segments.push(idx);
            }
        }
        log::info!(
            "Segment optimization took {:.2}ms ({} blocking segments of {})",
            seg_opt_start.elapsed().as_secs_f32() * 1000.0,
            pvs.blocking_segments.len(),
            pvs.segments.len()
        );

        // Phase 1: Portal discovery
        log::info!("Phase 1: Portal discovery...");
        let portal_start = std::time::Instant::now();
        pvs.portals = pvs.discover_portals(segments);
        log::info!(
            "Portal discovery took {:.2}s",
            portal_start.elapsed().as_secs_f32()
        );
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
        log::info!("Phase 2: Building portal graph...");
        let graph_start = std::time::Instant::now();
        pvs.portal_graph = pvs.build_portal_graph();
        log::info!(
            "Portal graph building took {:.2}s",
            graph_start.elapsed().as_secs_f32()
        );
        log::info!(
            "Built portal graph with {} nodes, {} unique sectors",
            pvs.portal_graph.nodes.len(),
            pvs.sectors.len()
        );

        // Phase 3: Build sector visibility
        log::info!("Phase 3: Building sector visibility...");
        let sector_vis_start = std::time::Instant::now();
        pvs.build_sector_visibility();
        log::info!(
            "Sector visibility building took {:.2}s",
            sector_vis_start.elapsed().as_secs_f32()
        );
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
        log::info!(
            "Total PVS build time: {:.2}s",
            phase_start.elapsed().as_secs_f32()
        );
        coarse_prof::write(&mut std::io::stdout()).unwrap();
        pvs
    }

    fn discover_portals(&self, segments: &[Segment]) -> Vec<Portal> {
        #[cfg(feature = "hprof")]
        profile!("discover_portals");
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
        #[cfg(feature = "hprof")]
        profile!("classify_portal");
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
        #[cfg(feature = "hprof")]
        profile!("build_portal_graph");
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
        #[cfg(feature = "hprof")]
        profile!("portals_are_adjacent");
        portal1.front_sector == portal2.front_sector
            || portal1.front_sector == portal2.back_sector
            || portal1.back_sector == portal2.front_sector
            || portal1.back_sector == portal2.back_sector
    }

    fn build_sector_visibility(&mut self) {
        #[cfg(feature = "hprof")]
        profile!("build_sector_visibility");
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
        #[cfg(feature = "hprof")]
        profile!("explore_sector_visibility");
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
        #[cfg(feature = "hprof")]
        profile!("build_subsector_visibility");

        let function_start = std::time::Instant::now();
        log::info!(
            "Starting subsector visibility calculation for {} subsectors",
            self.subsector_count
        );

        // Build subsector to sector mapping
        let mapping_start = std::time::Instant::now();
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
        log::info!(
            "Subsector to sector mapping took {:.2}ms",
            mapping_start.elapsed().as_secs_f32() * 1000.0
        );

        let total_pairs = (self.subsector_count * self.subsector_count) as f32;
        let mut processed_pairs = 0;
        let mut last_progress_time = std::time::Instant::now();
        let mut sector_visibility_checks = 0;
        let mut sector_visibility_skipped = 0;
        let mut line_of_sight_tests = 0;
        let mut line_of_sight_passed = 0;

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
                    let elapsed = function_start.elapsed().as_secs_f32();
                    let estimated_total = elapsed / (progress / 100.0);
                    let remaining = estimated_total - elapsed;
                    log::info!(
                        "Subsector visibility progress: {:.1}% - {:.1}s elapsed, ~{:.1}s remaining, sector checks: {}/{}, LOS tests: {}/{}",
                        progress,
                        elapsed,
                        remaining,
                        sector_visibility_checks,
                        sector_visibility_checks + sector_visibility_skipped,
                        line_of_sight_passed,
                        line_of_sight_tests
                    );
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
                    sector_visibility_skipped += 1;
                    continue;
                }

                sector_visibility_checks += 1;

                // // Same sector: always visible
                // if source_sector_idx == target_sector_idx {
                //     self.visibility_data
                //         .set_visible(source_subsector_idx, target_subsector_idx);
                //     continue;
                // }

                // Test line of sight between subsector centers
                line_of_sight_tests += 1;
                if self.test_subsector_line_of_sight(source_subsector_idx, target_subsector_idx) {
                    line_of_sight_passed += 1;
                    self.visibility_data
                        .set_visible(source_subsector_idx, target_subsector_idx);
                }
            }
        }

        let total_time = function_start.elapsed().as_secs_f32();
        log::info!(
            "Subsector visibility calculation complete in {:.2}s - Sector visibility: {}/{} passed, Line of sight: {}/{} passed",
            total_time,
            sector_visibility_checks,
            sector_visibility_checks + sector_visibility_skipped,
            line_of_sight_passed,
            line_of_sight_tests
        );
    }

    fn test_subsector_line_of_sight(&self, from_idx: usize, to_idx: usize) -> bool {
        #[cfg(feature = "hprof")]
        profile!("test_subsector_line_of_sight");
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

        // Perform finegrained geometric ray testing
        self.test_geometric_line_of_sight(from_idx, to_idx)
    }

    // TODO: Replace this with something like a swept volume? Project lines on a
    // plane perpendicular to the line of sight from center to center, if our source projection
    // is fully occluded by lines between source and target then the target is not visible.
    // Portal lines should be excluded.
    fn test_geometric_line_of_sight(&self, from_idx: usize, to_idx: usize) -> bool {
        #[cfg(feature = "hprof")]
        profile!("test_geometric_line_of_sight");
        // Use pre-calculated BSP AABBs with orphaned space fixes
        let from_aabb = &self.subsector_aabbs[from_idx];
        let to_aabb = &self.subsector_aabbs[to_idx];

        // Generate test points for source subsector
        let mut from_points = Vec::new();

        // Add segment vertices
        let from_segments = self.get_subsector_segments(from_idx);
        // for segment in &from_segments {
        //     from_points.push(segment.v1);
        //     from_points.push(segment.v2);
        // }

        // Add AABB corner points
        from_points.push(Vec2::new(from_aabb.left, from_aabb.bottom));
        from_points.push(Vec2::new(from_aabb.right, from_aabb.bottom));
        from_points.push(Vec2::new(from_aabb.left, from_aabb.top));
        from_points.push(Vec2::new(from_aabb.right, from_aabb.top));

        // Add center point (most important)
        // let from_center = Vec2::new(
        //     (from_aabb.left + from_aabb.right) * 0.5,
        //     (from_aabb.bottom + from_aabb.top) * 0.5,
        // );
        // from_points.push(from_center);

        // Add middle points for long/narrow subsectors
        if from_segments.len() >= 2 {
            let mid_point = Vec2::new(
                (from_segments[0].v1.x + from_segments[from_segments.len() - 1].v2.x) * 0.5,
                (from_segments[0].v1.y + from_segments[from_segments.len() - 1].v2.y) * 0.5,
            );
            from_points.push(mid_point);
        }

        // Generate test points for target subsector
        let mut to_points = Vec::new();

        // Add segment vertices
        let to_segments = self.get_subsector_segments(to_idx);
        // for segment in &to_segments {
        //     to_points.push(segment.v1);
        //     to_points.push(segment.v2);
        // }

        // Add AABB corner points
        to_points.push(Vec2::new(to_aabb.left, to_aabb.bottom));
        to_points.push(Vec2::new(to_aabb.right, to_aabb.bottom));
        to_points.push(Vec2::new(to_aabb.left, to_aabb.top));
        to_points.push(Vec2::new(to_aabb.right, to_aabb.top));

        // Add center point (most important)
        // let to_center = Vec2::new(
        //     (to_aabb.left + to_aabb.right) * 0.5,
        //     (to_aabb.bottom + to_aabb.top) * 0.5,
        // );
        // to_points.push(to_center);

        // Add middle points for long/narrow subsectors
        if to_segments.len() >= 2 {
            let mid_point = Vec2::new(
                (to_segments[0].v1.x + to_segments[to_segments.len() - 1].v2.x) * 0.5,
                (to_segments[0].v1.y + to_segments[to_segments.len() - 1].v2.y) * 0.5,
            );
            to_points.push(mid_point);
        }

        // Remove duplicate points to avoid redundant tests
        // from_points.sort_by(|a, b| {
        //     a.x.partial_cmp(&b.x)
        //         .unwrap_or(std::cmp::Ordering::Equal)
        //         .then_with(|| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal))
        // });
        // from_points.dedup_by(|a, b| (a.x - b.x).abs() < 0.1 && (a.y - b.y).abs() < 0.1);

        // to_points.sort_by(|a, b| {
        //     a.x.partial_cmp(&b.x)
        //         .unwrap_or(std::cmp::Ordering::Equal)
        //         .then_with(|| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal))
        // });
        // to_points.dedup_by(|a, b| (a.x - b.x).abs() < 0.1 && (a.y - b.y).abs() < 0.1);

        // Test multiple ray combinations, prioritize center-to-center
        // // Center to center test first (most reliable)
        // if self.test_geometric_ray_intersection(from_center, to_center) {
        //     return true;
        // }

        // // Test center to all target points
        // for &to_point in &to_points {
        //     if self.test_geometric_ray_intersection(from_center, to_point) {
        //         return true;
        //     }
        // }

        // // Test all source points to center
        // for &from_point in &from_points {
        //     if self.test_geometric_ray_intersection(from_point, to_center) {
        //         return true;
        //     }
        // }

        // Finally test all combinations (more expensive)
        for &from_point in &from_points {
            for &to_point in &to_points {
                if self.test_geometric_ray_intersection(from_point, to_point) {
                    return true;
                }
            }
        }

        false
    }

    fn calculate_subsector_aabb(&self, subsector_idx: usize) -> BBox {
        #[cfg(feature = "hprof")]
        profile!("calculate_subsector_aabb");
        let segments = self.get_subsector_segments(subsector_idx);

        if segments.is_empty() {
            return BBox::default();
        }

        let mut min_x = f32::MAX;
        let mut max_x = f32::MIN;
        let mut min_y = f32::MAX;
        let mut max_y = f32::MIN;

        for segment in segments {
            min_x = min_x.min(segment.v1.x).min(segment.v2.x);
            max_x = max_x.max(segment.v1.x).max(segment.v2.x);
            min_y = min_y.min(segment.v1.y).min(segment.v2.y);
            max_y = max_y.max(segment.v1.y).max(segment.v2.y);
        }

        BBox {
            left: min_x,
            right: max_x,
            bottom: min_y,
            top: max_y,
        }
    }

    fn get_subsector_segments(&self, subsector_index: usize) -> Vec<Segment> {
        #[cfg(feature = "hprof")]
        profile!("get_subsector_segments");
        if subsector_index >= self.subsectors.len() {
            return Vec::new();
        }

        let subsector = &self.subsectors[subsector_index];
        let start = subsector.start_seg as usize;
        let count = subsector.seg_count as usize;

        self.segments
            .get(start..start + count)
            .map(|slice| slice.iter().cloned().collect())
            .unwrap_or_default()
    }

    fn test_geometric_ray_intersection(&self, ray_start: Vec2, ray_end: Vec2) -> bool {
        #[cfg(feature = "hprof")]
        profile!("test_geometric_ray_intersection");
        let ray_length = (ray_end - ray_start).length();

        if ray_length < 0.1 {
            return true; // Points are essentially the same
        }

        // Create ray bounding box for culling
        let ray_min_x = ray_start.x.min(ray_end.x) - 1.0;
        let ray_max_x = ray_start.x.max(ray_end.x) + 1.0;
        let ray_min_y = ray_start.y.min(ray_end.y) - 1.0;
        let ray_max_y = ray_start.y.max(ray_end.y) + 1.0;

        // Test only against precomputed blocking segments
        for &seg_idx in &self.blocking_segments {
            let seg_bbox = &self.segment_bboxes[seg_idx];
            let segment = &self.segments[seg_idx];

            // Fast bounding box test using precomputed bbox
            if seg_bbox.right < ray_min_x
                || seg_bbox.left > ray_max_x
                || seg_bbox.top < ray_min_y
                || seg_bbox.bottom > ray_max_y
            {
                continue;
            }

            // Detailed line intersection test
            if self.line_intersects_ray(segment, ray_start, ray_end) {
                return false; // Ray is blocked - early exit
            }
        }

        true // Ray is not blocked
    }

    fn line_intersects_ray(&self, segment: &Segment, ray_start: Vec2, ray_end: Vec2) -> bool {
        #[cfg(feature = "hprof")]
        profile!("line_intersects_ray");
        let seg_dir = segment.v2 - segment.v1;
        let ray_dir = ray_end - ray_start;
        let to_seg_start = segment.v1 - ray_start;

        let cross = ray_dir.x * seg_dir.y - ray_dir.y * seg_dir.x;

        if cross.abs() < 1e-8 {
            return false; // Parallel lines
        }

        let t = (to_seg_start.x * seg_dir.y - to_seg_start.y * seg_dir.x) / cross;
        let u = (to_seg_start.x * ray_dir.y - to_seg_start.y * ray_dir.x) / cross;

        const EPSILON: f32 = 0.01;
        t >= EPSILON && t <= 1.0 - EPSILON && u >= EPSILON && u <= 1.0 - EPSILON
    }

    /// Find the BSP node AABB for a given subsector and fix missing space coverage
    fn find_and_fix_subsector_bsp_aabb(
        &self,
        target_subsector_idx: usize,
        nodes: &mut [crate::level::map_defs::Node],
        start_node: u32,
    ) -> BBox {
        #[cfg(feature = "hprof")]
        profile!("find_and_fix_subsector_bsp_aabb");
        const IS_SSECTOR_MASK: u32 = 0x8000_0000;

        if let Some((aabb, parent_node_idx, side)) = self
            .traverse_bsp_for_subsector_aabb_with_parent(
                target_subsector_idx,
                nodes,
                start_node,
                None,
                0,
            )
        {
            // Check if we need to fix missing space coverage
            self.fix_missing_space_coverage(parent_node_idx, side, nodes);

            // Return the potentially updated AABB
            if let Some(updated_aabb) = self.traverse_bsp_for_subsector_aabb_with_parent(
                target_subsector_idx,
                nodes,
                start_node,
                None,
                0,
            ) {
                updated_aabb.0
            } else {
                aabb
            }
        } else {
            // Fallback to segment-based AABB if BSP lookup fails
            self.calculate_subsector_aabb(target_subsector_idx)
        }
    }

    /// Recursively traverse BSP tree to find the AABB for a specific subsector with parent info
    fn traverse_bsp_for_subsector_aabb_with_parent(
        &self,
        target_subsector_idx: usize,
        nodes: &[crate::level::map_defs::Node],
        node_index: u32,
        parent_node_idx: Option<usize>,
        parent_side: usize,
    ) -> Option<(BBox, Option<usize>, usize)> {
        #[cfg(feature = "hprof")]
        profile!("traverse_bsp_for_subsector_aabb_with_parent");
        const IS_SSECTOR_MASK: u32 = 0x8000_0000;

        if node_index & IS_SSECTOR_MASK != 0 {
            // This is a subsector leaf
            let subsector_index = (node_index & !IS_SSECTOR_MASK) as usize;
            if subsector_index == target_subsector_idx {
                // Found our target subsector! Return the parent node's AABB for this side
                if let Some(parent_idx) = parent_node_idx {
                    let parent = &nodes[parent_idx];
                    let bbox_pair = &parent.bboxes[parent_side];
                    let aabb = BBox {
                        left: bbox_pair[0].x.min(bbox_pair[1].x),
                        right: bbox_pair[0].x.max(bbox_pair[1].x),
                        bottom: bbox_pair[0].y.min(bbox_pair[1].y),
                        top: bbox_pair[0].y.max(bbox_pair[1].y),
                    };
                    return Some((aabb, Some(parent_idx), parent_side));
                }
            }
            return None;
        }

        if (node_index as usize) >= nodes.len() {
            return None;
        }

        let node = &nodes[node_index as usize];

        // Check both children, passing this node as the parent
        for child_idx in 0..2 {
            let child_id = node.children[child_idx];
            if let Some(result) = self.traverse_bsp_for_subsector_aabb_with_parent(
                target_subsector_idx,
                nodes,
                child_id,
                Some(node_index as usize),
                child_idx,
            ) {
                return Some(result);
            }
        }

        None
    }

    /// Fix missing space coverage by extending this subsector's AABB to cover uncovered parent space
    fn fix_missing_space_coverage(
        &self,
        parent_node_idx: Option<usize>,
        target_side: usize,
        nodes: &mut [crate::level::map_defs::Node],
    ) {
        #[cfg(feature = "hprof")]
        profile!("fix_missing_space_coverage");
        let Some(parent_idx) = parent_node_idx else {
            return;
        };
        if parent_idx >= nodes.len() {
            return;
        }

        let parent_node = &nodes[parent_idx];

        // Get parent's full AABB (union of both sides)
        let parent_bbox_0 = &parent_node.bboxes[0];
        let parent_bbox_1 = &parent_node.bboxes[1];
        let parent_full_aabb = BBox {
            left: parent_bbox_0[0]
                .x
                .min(parent_bbox_0[1].x)
                .min(parent_bbox_1[0].x.min(parent_bbox_1[1].x)),
            right: parent_bbox_0[0]
                .x
                .max(parent_bbox_0[1].x)
                .max(parent_bbox_1[0].x.max(parent_bbox_1[1].x)),
            bottom: parent_bbox_0[0]
                .y
                .min(parent_bbox_0[1].y)
                .min(parent_bbox_1[0].y.min(parent_bbox_1[1].y)),
            top: parent_bbox_0[0]
                .y
                .max(parent_bbox_0[1].y)
                .max(parent_bbox_1[0].y.max(parent_bbox_1[1].y)),
        };

        // Get target's current AABB
        let target_bbox_pair = &parent_node.bboxes[target_side];
        let current_aabb = BBox {
            left: target_bbox_pair[0].x.min(target_bbox_pair[1].x),
            right: target_bbox_pair[0].x.max(target_bbox_pair[1].x),
            bottom: target_bbox_pair[0].y.min(target_bbox_pair[1].y),
            top: target_bbox_pair[0].y.max(target_bbox_pair[1].y),
        };

        // Check if this subsector should be extended to better cover parent space
        let should_extend = current_aabb.left > parent_full_aabb.left
            || current_aabb.right < parent_full_aabb.right
            || current_aabb.bottom > parent_full_aabb.bottom
            || current_aabb.top < parent_full_aabb.top;

        if should_extend {
            // Extend this subsector's AABB toward the parent's bounds
            let mut extended_aabb = current_aabb.clone();

            // Extend toward parent bounds where there might be missing coverage
            if current_aabb.left > parent_full_aabb.left {
                extended_aabb.left = parent_full_aabb.left;
            }
            if current_aabb.right < parent_full_aabb.right {
                extended_aabb.right = parent_full_aabb.right;
            }
            if current_aabb.bottom > parent_full_aabb.bottom {
                extended_aabb.bottom = parent_full_aabb.bottom;
            }
            if current_aabb.top < parent_full_aabb.top {
                extended_aabb.top = parent_full_aabb.top;
            }

            // Update the BSP node's bounding box for this side
            let parent_node = &mut nodes[parent_idx];
            parent_node.bboxes[target_side][0].x = extended_aabb.left;
            parent_node.bboxes[target_side][0].y = extended_aabb.bottom;
            parent_node.bboxes[target_side][1].x = extended_aabb.right;
            parent_node.bboxes[target_side][1].y = extended_aabb.top;
        }
    }

    pub fn is_visible(&self, from: usize, to: usize) -> bool {
        #[cfg(feature = "hprof")]
        profile!("pvs_is_visible");
        if from >= self.subsector_count || to >= self.subsector_count {
            return false;
        }
        self.visibility_data.is_visible(from, to)
    }

    pub fn get_visible_subsectors(&self, from: usize) -> Vec<usize> {
        #[cfg(feature = "hprof")]
        profile!("pvs_get_visible_subsectors");
        if from >= self.subsector_count {
            return Vec::new();
        }
        self.visibility_data.get_visible_subsectors(from)
    }

    pub fn memory_usage(&self) -> usize {
        #[cfg(feature = "hprof")]
        profile!("pvs_memory_usage");
        self.visibility_data.memory_usage()
            + self.portals.len() * std::mem::size_of::<Portal>()
            + self.portal_graph.nodes.len() * std::mem::size_of::<PortalNode>()
    }

    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), Box<dyn std::error::Error>> {
        #[cfg(feature = "hprof")]
        profile!("pvs_save_to_file");
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

    pub fn load_from_file(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        #[cfg(feature = "hprof")]
        profile!("pvs_load_from_file");
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
            subsectors: Vec::new(),
            sectors: Vec::new(),
            segments: Vec::new(),
            subsector_aabbs: Vec::new(),
            segment_bboxes: Vec::new(),
            blocking_segments: Vec::new(),
        })
    }

    pub fn get_pvs_cache_path(
        wad_name: &str,
        map_name: &str,
    ) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
        #[cfg(feature = "hprof")]
        profile!("get_pvs_cache_path");
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
        #[cfg(feature = "hprof")]
        profile!("build_and_cache");
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
        #[cfg(feature = "hprof")]
        profile!("load_from_cache");
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
        #[cfg(feature = "hprof")]
        profile!("pvs_subsector_count");
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
    fn test_compact_pvs() {
        let mut pvs = CompactPVS::new(64);

        assert!(!pvs.is_visible(0, 1));
        pvs.set_visible(0, 1);
        assert!(pvs.is_visible(0, 1));

        pvs.set_visible(63, 0);
        assert!(pvs.is_visible(63, 0));
    }
}
