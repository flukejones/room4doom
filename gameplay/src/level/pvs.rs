//! Portal-based Potentially Visible Set (PVS) implementation for Doom-style BSP maps.
//!
//! This module provides efficient visibility culling by precomputing which subsectors
//! can see each other through portals (connections between sectors). The PVS data
//! structure allows for O(1) visibility queries during rendering, significantly
//! improving performance by avoiding unnecessary geometry processing.
//!
//! ## PVS Building Process
//!
//! The PVS construction follows a streamlined 4-phase approach optimized for Doom's BSP structure:
//!
//! ### Phase 1: Portal Discovery
//! - Scans all two-sided line segments (linedefs) to identify sector connections
//! - Classifies portals as either "Open" (static openings) or "Movable" (doors/platforms)
//! - Calculates valid Z-height ranges for each portal based on floor/ceiling heights
//! - Records which sectors each portal connects for implicit connectivity tracking
//!
//! ### Phase 2: Portal Graph Construction
//! - Builds a graph where nodes represent sector pairs connected by portals
//! - Creates adjacency lists for efficient portal traversal algorithms
//! - Sector connectivity is implicit in the portal relationships (no separate phase needed)
//!
//! ### Phase 3: Subsector-Level Visibility
//! - For each subsector pair, uses portal graph to check if parent sectors are connected
//! - Performs on-demand portal traversal to determine sector reachability
//! - Proceeds with geometric testing only for subsectors in connected sectors
//! - Uses BSP-derived AABBs to generate multiple test points per subsector
//!
//! ### Phase 4: Geometric Validation
//! - Casts multiple rays between subsector test points to handle complex geometry
//! - Performs 3D intersection tests considering floor/ceiling heights at portals
//! - Accounts for movable geometry by using expanded height ranges
//! - Optimizes ray tests by pre-filtering against blocking segments
//!
//! The final result is a compact bit-matrix storing subsector-to-subsector visibility
//! that can be queried in O(1) time during rendering. This approach eliminates the need
//! for a separate sector visibility matrix, making the algorithm more memory-efficient
//! while maintaining the same filtering benefits. PVS data can be cached to disk
//! to avoid recomputation between runs.

#[cfg(feature = "hprof")]
use coarse_prof::profile;
use log::info;

use crate::level::map_data::IS_SSECTOR_MASK;
use crate::level::map_defs::{BBox, Sector, Segment, SubSector};
use crate::{MapPtr, Node};
use glam::Vec2;
use std::collections::HashMap;
use std::path::Path;

const MAX_SECTOR_DEPTH: usize = 16;

// Ray casting constants
const MIN_RAY_LENGTH: f32 = 0.1;
const RAY_ENDPOINT_TOLERANCE: f32 = 1.0;
const RAY_BBOX_PADDING: f32 = 1.0;

// Height testing constants
const ZERO_HEIGHT_TOLERANCE: f32 = 0.1;
const MAX_STEP_HEIGHT: f32 = 24.0;
const MOVEMENT_RANGE: f32 = 128.0;
const PLAYER_EYE_HEIGHT: f32 = 1.0;

/// Connection between two sectors through a portal (door, window, etc.)
#[derive(Debug, Clone)]
pub struct Portal {
    pub front_sector: MapPtr<Sector>,
    pub back_sector: MapPtr<Sector>,
    pub portal_type: PortalType,
    pub z_position: f32,
    pub z_range: f32,
}

/// Type of portal connection between sectors
#[derive(Debug, Clone, PartialEq)]
pub enum PortalType {
    Movable,
    Open,
}

/// Graph representation of portal connections between sectors
pub struct PortalGraph {
    pub nodes: Vec<PortalNode>,
    pub adjacency: Vec<Vec<usize>>,
}

/// Node in the portal graph representing a sector pair connection
#[derive(Debug)]
pub struct PortalNode {
    pub sector_pair: (usize, usize),
}

/// Compact bit-packed Potentially Visible Set data structure
pub struct CompactPVS {
    subsector_count: usize,
    data: Vec<u32>,
}

impl CompactPVS {
    /// Creates a new compact PVS for the given number of subsectors
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

    /// Marks subsector `to` as visible from subsector `from`
    pub fn set_visible(&mut self, from: usize, to: usize) {
        #[cfg(feature = "hprof")]
        profile!("compact_pvs_set_visible");
        let bit_index = from * self.subsector_count + to;
        let word_index = bit_index / 32;
        let bit_offset = bit_index % 32;
        self.data[word_index] |= 1u32 << bit_offset;
    }

    /// Returns true if subsector `to` is visible from subsector `from`
    pub fn is_visible(&self, from: usize, to: usize) -> bool {
        let bit_index = from * self.subsector_count + to;
        let word_index = bit_index / 32;
        let bit_offset = bit_index % 32;
        (self.data[word_index] & (1u32 << bit_offset)) != 0
    }

    // TODO: cahce this in BSP leaves
    /// Returns all subsectors visible from the given subsector
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

    /// Returns memory usage in bytes
    pub fn memory_usage(&self) -> usize {
        #[cfg(feature = "hprof")]
        profile!("compact_pvs_memory_usage");
        std::mem::size_of::<Self>() + self.data.len() * std::mem::size_of::<u32>()
    }
}

/// Portal-based Potentially Visible Set for efficient visibility culling
pub struct PVS {
    pub(super) subsector_count: usize,
    pub(super) visibility_data: CompactPVS,

    portals: Vec<Portal>,
    portal_graph: PortalGraph,
    sector_connectivity: Vec<Vec<bool>>,

    pub(super) subsectors: Vec<SubSector>,
    sectors: Vec<Sector>,
    segments: Vec<Segment>,
    subsector_aabbs: Vec<BBox>,
    segment_bboxes: Vec<BBox>,
    blocking_segments: Vec<usize>,
    extended_aabb_segments: Vec<Option<usize>>,
}

impl PVS {
    // ============================================================================
    // PUBLIC API FUNCTIONS
    // ============================================================================

    /// Creates a new PVS for the given number of subsectors
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
            sector_connectivity: Vec::new(),
            subsectors: Vec::new(),
            sectors: Vec::new(),
            segments: Vec::new(),
            subsector_aabbs: Vec::new(),
            segment_bboxes: Vec::new(),
            blocking_segments: Vec::new(),
            extended_aabb_segments: Vec::new(),
        }
    }

    /// Builds PVS data from map geometry using portal-based visibility
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

        // Pre-calculate BSP-based AABBs for all subsectors
        log::info!("Pre-calculating BSP AABBs...");
        let bsp_start = std::time::Instant::now();
        let mut subsector_aabbs = Vec::with_capacity(subsectors.len());
        let mut extended_aabb_segments = Vec::with_capacity(subsectors.len());
        for (subsector_idx, _subsector) in subsectors.iter().enumerate() {
            let (bsp_aabb, extended_segment_id) =
                pvs.fix_subsector_bsp_aabb(subsector_idx, nodes, start_node);
            subsector_aabbs.push(bsp_aabb);
            extended_aabb_segments.push(extended_segment_id);
        }

        pvs.subsector_aabbs = subsector_aabbs;
        pvs.extended_aabb_segments = extended_aabb_segments;
        log::info!(
            "BSP AABB calculation took {:.2}s",
            bsp_start.elapsed().as_secs_f32()
        );

        // Precompute segment optimizations
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
        let mut movable_portals = 0;
        for portal in &pvs.portals {
            match portal.portal_type {
                PortalType::Open => open_portals += 1,
                PortalType::Movable => movable_portals += 1,
            }
        }
        log::info!(
            "Discovered {} portals: {} open, {} movable",
            pvs.portals.len(),
            open_portals,
            movable_portals
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

        // Phase 3: Build sector connectivity matrix
        log::info!("Phase 3: Building sector connectivity matrix...");
        let connectivity_start = std::time::Instant::now();
        pvs.build_sector_connectivity();
        log::info!(
            "Sector connectivity building took {:.2}s",
            connectivity_start.elapsed().as_secs_f32()
        );

        // Phase 4: Build subsector visibility
        log::info!("Phase 4: Starting subsector visibility calculation...");
        let start_time = std::time::Instant::now();
        pvs.build_subsector_visibility();
        let elapsed = start_time.elapsed();

        log::info!(
            "Subsector visibility building took {:.2}s",
            elapsed.as_secs_f32()
        );

        log::info!("Portal-based PVS build complete - eliminated separate sector visibility phase");
        log::info!(
            "Performance: {} sectors, {} subsectors, {} portals",
            pvs.sectors.len(),
            pvs.subsector_count,
            pvs.portals.len()
        );
        log::info!(
            "Total PVS build time: {:.2}s",
            phase_start.elapsed().as_secs_f32()
        );
        coarse_prof::write(&mut std::io::stdout()).unwrap();
        pvs
    }

    /// Returns true if subsector `to` is visible from subsector `from`
    pub fn is_visible(&self, from: usize, to: usize) -> bool {
        #[cfg(feature = "hprof")]
        profile!("pvs_is_visible");
        if from >= self.subsector_count || to >= self.subsector_count {
            return false;
        }
        self.visibility_data.is_visible(from, to)
    }

    /// Returns all subsectors visible from the given subsector
    pub fn get_visible_subsectors(&self, from: usize) -> Vec<usize> {
        #[cfg(feature = "hprof")]
        profile!("pvs_get_visible_subsectors");
        if from >= self.subsector_count {
            return Vec::new();
        }
        self.visibility_data.get_visible_subsectors(from)
    }

    /// Returns total memory usage in bytes
    pub fn memory_usage(&self) -> usize {
        #[cfg(feature = "hprof")]
        profile!("pvs_memory_usage");
        self.visibility_data.memory_usage()
            + self.portals.len() * std::mem::size_of::<Portal>()
            + self.portal_graph.nodes.len() * std::mem::size_of::<PortalNode>()
            + self
                .portal_graph
                .adjacency
                .iter()
                .map(|adj| adj.len() * std::mem::size_of::<usize>())
                .sum::<usize>()
    }

    /// Returns the number of subsectors in this PVS
    pub fn subsector_count(&self) -> usize {
        #[cfg(feature = "hprof")]
        profile!("pvs_subsector_count");
        self.subsector_count
    }

    /// Saves PVS data to a binary file for caching
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), Box<dyn std::error::Error>> {
        #[cfg(feature = "hprof")]
        profile!("pvs_save_to_file");
        let mut file = std::fs::File::create(path)?;
        use std::io::Write;

        // Write header and data sizes
        file.write_all(b"PVS2")?;
        file.write_all(&self.subsector_count.to_le_bytes())?;
        file.write_all(&self.visibility_data.data.len().to_le_bytes())?;

        // Write visibility data as bytes
        let data_bytes: Vec<u8> = self
            .visibility_data
            .data
            .iter()
            .flat_map(|&word| word.to_le_bytes())
            .collect();
        file.write_all(&data_bytes)?;

        Ok(())
    }

    /// Attempts to load PVS data from cache, returns None if unavailable or invalid
    pub fn load_from_cache(
        wad_name: &str,
        map_name: &str,
        map_hash: u64,
        expected_subsectors: usize,
    ) -> Option<Self> {
        #[cfg(feature = "hprof")]
        profile!("load_from_cache");
        match Self::get_pvs_cache_path(wad_name, map_name, map_hash) {
            Ok(cache_path) => {
                if cache_path.exists() {
                    info!("Found PVS data at {cache_path:?}");
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

    /// Loads PVS data from a binary cache file
    pub fn load_from_file(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        #[cfg(feature = "hprof")]
        profile!("pvs_load_from_file");
        let mut file = std::fs::File::open(path)?;
        use std::io::Read;

        // Read and validate header
        let mut header = [0u8; 4];
        file.read_exact(&mut header)?;
        if &header != b"PVS2" {
            return Err("Invalid PVS file format".into());
        }

        // Read sizes
        let mut size_buffer = [0u8; 8];
        file.read_exact(&mut size_buffer)?;
        let subsector_count = usize::from_le_bytes(size_buffer);

        file.read_exact(&mut size_buffer)?;
        let data_len = usize::from_le_bytes(size_buffer);

        // Read visibility data as bytes then convert to u32 words
        let mut data_bytes = vec![0u8; data_len * 4];
        file.read_exact(&mut data_bytes)?;
        let data: Vec<u32> = data_bytes
            .chunks_exact(4)
            .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect();

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
            sector_connectivity: Vec::new(),
            subsectors: Vec::new(),
            sectors: Vec::new(),
            segments: Vec::new(),
            subsector_aabbs: Vec::new(),
            segment_bboxes: Vec::new(),
            blocking_segments: Vec::new(),
            extended_aabb_segments: Vec::new(),
        })
    }

    /// Returns the cache file path for the given WAD and map
    pub fn get_pvs_cache_path(
        wad_name: &str,
        map_name: &str,
        map_hash: u64,
    ) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
        #[cfg(feature = "hprof")]
        profile!("get_pvs_cache_path");
        let cache_dir = dirs::cache_dir()
            .ok_or("Could not determine cache directory")?
            .join("room4doom")
            .join("pvs");

        std::fs::create_dir_all(&cache_dir)?;

        let filename = format!("{wad_name}_{map_name}_{map_hash}.pvs");
        Ok(cache_dir.join(filename))
    }

    // ============================================================================
    // PHASE 1: PORTAL DISCOVERY
    // ============================================================================

    /// Discovers portal connections between sectors by examining two-sided segments
    fn discover_portals(&self, segments: &[Segment]) -> Vec<Portal> {
        #[cfg(feature = "hprof")]
        profile!("discover_portals");
        let mut portals = Vec::new();

        for segment in segments {
            if let Some(back_sector) = &segment.backsector {
                let portal_type = self.classify_portal(&segment.frontsector, back_sector, segment);

                let (z_position, z_range) =
                    self.calculate_portal_z_range(&segment.frontsector, back_sector, &portal_type);

                let portal = Portal {
                    front_sector: segment.frontsector.clone(),
                    back_sector: back_sector.clone(),
                    portal_type,
                    z_position,
                    z_range,
                };

                portals.push(portal);
            }
        }

        portals
    }

    /// Classifies a portal as either movable (doors/platforms) or open based on sector properties
    fn classify_portal(&self, front: &Sector, back: &Sector, _segment: &Segment) -> PortalType {
        #[cfg(feature = "hprof")]
        profile!("classify_portal");

        if self.is_movable_sector(&front) || self.is_movable_sector(&back) {
            return PortalType::Movable;
        }

        PortalType::Open
    }

    /// Returns true if a sector can move (doors, platforms, etc.)
    /// Checks both active movement and potential movement based on special type
    fn is_movable_sector(&self, sector: &Sector) -> bool {
        if sector.specialdata.is_some() {
            return true;
        }

        // Check if sector has special types that indicate movable floors/ceilings/doors
        // Based on Doom sector specials for moving floors, ceilings, doors, etc.
        matches!(sector.special,
            1..=11 |    // Various door types
            16..=19 |   // Door close/open types
            25..=27 |   // Ceiling crush types
            36..=37 |   // Floor lower types
            38..=40 |   // Floor raise types
            41..=47 |   // Ceiling lower/raise types
            60..=69 |   // Floor lower/raise types
            70..=79 |   // Floor turbo types
            80..=86 |   // Stair building types
            87..=90 |   // Floor/ceiling types
            100..=104 | // Various floor types
            109..=111 | // Door types
            114..=116 | // Door/floor types
            117..=119 | // Door types
            120..=127 | // Floor types
            140..=150   // Floor/ceiling/door types
        )
    }

    /// Calculates the valid Z range for a portal based on sector heights and movement
    /// Returns (z_position, z_range) representing the floor height and height span
    fn calculate_portal_z_range(
        &self,
        front_sector: &Sector,
        back_sector: &Sector,
        portal_type: &PortalType,
    ) -> (f32, f32) {
        match portal_type {
            PortalType::Movable => {
                let mut min_floor = front_sector.floorheight.min(back_sector.floorheight);
                let mut max_ceiling = front_sector.ceilingheight.max(back_sector.ceilingheight);

                // Expand range for movable geometry
                if let Some(data) = front_sector.specialdata {
                    unsafe {
                        match (*data).data() {
                            crate::thinker::ThinkerData::FloorMove(floor_move) => {
                                if floor_move.direction == -1 {
                                    min_floor = min_floor.min(floor_move.destheight);
                                } else {
                                    max_ceiling = max_ceiling.max(floor_move.destheight);
                                }
                            }
                            crate::thinker::ThinkerData::CeilingMove(ceiling_move) => {
                                min_floor = min_floor.min(ceiling_move.bottomheight);
                                max_ceiling = max_ceiling.max(ceiling_move.topheight);
                            }
                            crate::thinker::ThinkerData::VerticalDoor(door) => {
                                max_ceiling = max_ceiling.max(door.topheight);
                            }
                            _ => {}
                        }
                    }
                }

                if let Some(data) = back_sector.specialdata {
                    unsafe {
                        match (*data).data() {
                            crate::thinker::ThinkerData::FloorMove(floor_move) => {
                                if floor_move.direction == -1 {
                                    min_floor = min_floor.min(floor_move.destheight);
                                } else {
                                    max_ceiling = max_ceiling.max(floor_move.destheight);
                                }
                            }
                            crate::thinker::ThinkerData::CeilingMove(ceiling_move) => {
                                min_floor = min_floor.min(ceiling_move.bottomheight);
                                max_ceiling = max_ceiling.max(ceiling_move.topheight);
                            }
                            crate::thinker::ThinkerData::VerticalDoor(door) => {
                                max_ceiling = max_ceiling.max(door.topheight);
                            }
                            _ => {}
                        }
                    }
                }

                (min_floor, max_ceiling - min_floor)
            }
            PortalType::Open => {
                let min_floor = front_sector.floorheight.min(back_sector.floorheight);
                let max_ceiling = front_sector.ceilingheight.max(back_sector.ceilingheight);
                (min_floor, max_ceiling - min_floor)
            }
        }
    }

    // ============================================================================
    // HELPER FUNCTIONS
    // ============================================================================

    /// Builds a mapping from subsector sector pointers to sequential indices
    fn build_sector_mapping(&self) -> HashMap<usize, usize> {
        let mut sector_map = HashMap::new();
        for subsector in &self.subsectors {
            let sector_ptr = subsector.sector.inner as usize;
            if !sector_map.contains_key(&sector_ptr) {
                sector_map.insert(sector_ptr, sector_map.len());
            }
        }
        sector_map
    }

    /// Reports progress for long-running subsector visibility calculations
    fn report_progress(
        &self,
        processed_pairs: usize,
        total_pairs: f32,
        function_start: std::time::Instant,
        sector_visibility_checks: usize,
        sector_visibility_skipped: usize,
        line_of_sight_passed: usize,
        line_of_sight_tests: usize,
    ) {
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
    }

    /// Generates test points for geometric ray testing from subsector segments
    fn generate_test_points_from_subsector(&self, subsector_idx: usize, aabb: &BBox) -> Vec<Vec2> {
        let segments = self.get_subsector_segments(subsector_idx);
        let mut points = Vec::with_capacity(segments.len() * 2 + 4);

        // Special handling for single-segment single-sided subsectors
        if segments.len() == 1 && segments[0].backsector.is_none() {
            let segment = &segments[0];
            self.add_aabb_corners(aabb, segment, &mut points);
        } else {
            // Standard handling for multi-segment or two-sided subsectors
            for segment in &segments {
                if !points.contains(&segment.v1) {
                    points.push(segment.v1);
                }
                if !points.contains(&segment.v2) {
                    points.push(segment.v2);
                }

                // let midpoint = (segment.v1 + segment.v2) * 0.5;
                // if segment.backsector.is_none() {
                //     let seg_dir = segment.v2 - segment.v1;
                //     let inward_normal = Vec2::new(seg_dir.y, -seg_dir.x).normalize();
                //     let inward_point = midpoint + inward_normal * 128.0;
                //     points.push(inward_point);
                // } else {
                //     points.push(midpoint);
                // }
            }

            // If AABB was extended for a specific segment, add AABB corners
            if let Some(extended_segment_idx) = self.extended_aabb_segments[subsector_idx] {
                if extended_segment_idx < self.segments.len() {
                    let extended_segment = &self.segments[extended_segment_idx];
                    for segment in &segments {
                        if segment.v1 == extended_segment.v1 && segment.v2 == extended_segment.v2 {
                            let v = Vec2::new(aabb.left, aabb.bottom);
                            if !points.contains(&v) {
                                points.push(v);
                            }
                            let v = Vec2::new(aabb.right, aabb.bottom);
                            if !points.contains(&v) {
                                points.push(v);
                            }
                            let v = Vec2::new(aabb.left, aabb.top);
                            if !points.contains(&v) {
                                points.push(v);
                            }
                            let v = Vec2::new(aabb.right, aabb.top);
                            if !points.contains(&v) {
                                points.push(v);
                            }
                            break;
                        }
                    }
                }
            }
        }

        points
    }

    /// Returns AABB corners that are on the "front" side of a segment (not behind it)
    fn add_aabb_corners(&self, aabb: &BBox, segment: &Segment, corners: &mut Vec<Vec2>) {
        // Calculate segment normal (perpendicular vector pointing to the front)
        let seg_dir = segment.v2 - segment.v1;
        let normal = Vec2::new(-seg_dir.y, seg_dir.x).normalize();

        // Filter corners that are in front of the segment
        for corner in [
            Vec2::new(aabb.left, aabb.bottom),
            Vec2::new(aabb.right, aabb.bottom),
            Vec2::new(aabb.left, aabb.top),
            Vec2::new(aabb.right, aabb.top),
        ] {
            let to_corner = corner - segment.v1;
            if to_corner.dot(normal) >= 0.0 {
                corners.push(corner);
            }
        }

        if corners.is_empty() {
            corners.push(Vec2::new(
                (aabb.left + aabb.right) * 0.5,
                (aabb.bottom + aabb.top) * 0.5,
            ));
        }
    }

    // ============================================================================
    // PHASE 2: PORTAL GRAPH BUILDING
    // ============================================================================

    /// Builds a graph representation of portal connections for traversal algorithms
    fn build_portal_graph(&self) -> PortalGraph {
        #[cfg(feature = "hprof")]
        profile!("build_portal_graph");
        let mut graph = PortalGraph {
            nodes: Vec::with_capacity(self.portals.len()),
            adjacency: vec![Vec::new(); self.portals.len()],
        };

        let sector_map = self.build_sector_mapping();
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

    /// Returns true if two portals share a common sector
    fn portals_are_adjacent(&self, portal1: &Portal, portal2: &Portal) -> bool {
        #[cfg(feature = "hprof")]
        profile!("portals_are_adjacent");
        portal1.front_sector == portal2.front_sector
            || portal1.front_sector == portal2.back_sector
            || portal1.back_sector == portal2.front_sector
            || portal1.back_sector == portal2.back_sector
    }

    // ============================================================================
    // PHASE 3: SECTOR CONNECTIVITY
    // ============================================================================

    /// Pre-computes sector connectivity matrix for efficient subsector filtering
    fn build_sector_connectivity(&mut self) {
        #[cfg(feature = "hprof")]
        profile!("build_sector_connectivity");

        let sector_count = self.sectors.len();
        let mut connectivity = vec![vec![false; sector_count]; sector_count];

        // Mark self-connectivity
        for i in 0..sector_count {
            connectivity[i][i] = true;
        }

        // For each sector, find connected sectors within depth limit
        for source_sector in 0..sector_count {
            let mut visited = vec![false; sector_count];
            let mut stack = vec![(source_sector, 0)];
            visited[source_sector] = true;

            while let Some((current_sector, depth)) = stack.pop() {
                connectivity[source_sector][current_sector] = true;

                if depth >= MAX_SECTOR_DEPTH {
                    continue;
                }

                // Find connected sectors through portals
                for node in &self.portal_graph.nodes {
                    let next_sector = if node.sector_pair.0 == current_sector {
                        node.sector_pair.1
                    } else if node.sector_pair.1 == current_sector {
                        node.sector_pair.0
                    } else {
                        continue;
                    };

                    if next_sector < sector_count && !visited[next_sector] {
                        visited[next_sector] = true;
                        stack.push((next_sector, depth + 1));
                    }
                }
            }
        }

        self.sector_connectivity = connectivity;
    }

    /// Checks if two sectors are connected through pre-computed connectivity matrix
    fn are_sectors_connected(&self, from_sector: usize, to_sector: usize) -> bool {
        if from_sector >= self.sector_connectivity.len()
            || to_sector >= self.sector_connectivity.len()
        {
            return false;
        }
        self.sector_connectivity[from_sector][to_sector]
    }

    // ============================================================================
    // PHASE 4: SUBSECTOR VISIBILITY BUILDING
    // ============================================================================

    /// Builds subsector-to-subsector visibility using on-demand portal connectivity checks
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
        let sector_map = self.build_sector_mapping();
        let mut subsector_to_sector = vec![0; self.subsector_count];

        for (subsector_idx, subsector) in self.subsectors.iter().enumerate() {
            let sector_ptr = subsector.sector.inner as usize;
            subsector_to_sector[subsector_idx] = sector_map.get(&sector_ptr).copied().unwrap_or(0);
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

        for source_subsector_idx in 0..self.subsector_count {
            self.visibility_data
                .set_visible(source_subsector_idx, source_subsector_idx);

            let source_sector_idx = subsector_to_sector[source_subsector_idx];
            for target_subsector_idx in 0..self.subsector_count {
                processed_pairs += 1;

                // Progress reporting every 1 second
                let now = std::time::Instant::now();
                if now.duration_since(last_progress_time).as_secs() >= 1 {
                    self.report_progress(
                        processed_pairs,
                        total_pairs,
                        function_start,
                        sector_visibility_checks,
                        sector_visibility_skipped,
                        line_of_sight_passed,
                        line_of_sight_tests,
                    );
                    last_progress_time = now;
                }

                if source_subsector_idx == target_subsector_idx {
                    continue;
                }

                let target_sector_idx = subsector_to_sector[target_subsector_idx];

                // Check if sectors are connected through portals (coarse filter)
                if !self.are_sectors_connected(source_sector_idx, target_sector_idx) {
                    sector_visibility_skipped += 1;
                    continue;
                }

                sector_visibility_checks += 1;

                // Test line of sight between subsector centers
                line_of_sight_tests += 1;
                if self.can_subsector_see_subsector(source_subsector_idx, target_subsector_idx) {
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

    /// Tests if one subsector can see another through geometric line-of-sight analysis
    /// Returns true if there is an unobstructed line of sight between the subsectors
    fn can_subsector_see_subsector(&self, from_idx: usize, to_idx: usize) -> bool {
        #[cfg(feature = "hprof")]
        profile!("can_subsector_see_subsector");
        let from_sector_ptr = self.subsectors[from_idx].sector.inner as usize;
        let to_sector_ptr = self.subsectors[to_idx].sector.inner as usize;

        if from_sector_ptr == to_sector_ptr {
            return true;
        }

        let sector_map = self.build_sector_mapping();

        let from_sector_idx = sector_map.get(&from_sector_ptr).copied().unwrap_or(0);
        let to_sector_idx = sector_map.get(&to_sector_ptr).copied().unwrap_or(0);

        // Check if sectors are connected through portals (this handles indirect connections)
        if !self.are_sectors_connected(from_sector_idx, to_sector_idx) {
            return false;
        }

        // Check portal-based visibility for movable geometry
        let from_sector = &*self.subsectors[from_idx].sector;
        let to_sector = &*self.subsectors[to_idx].sector;

        if let Some(portal) = self.find_portal_between_sectors(from_sector_idx, to_sector_idx) {
            match portal.portal_type {
                PortalType::Movable => {
                    if !self.can_see_through_movable_portal(&from_sector, &to_sector) {
                        return false;
                    }
                }
                PortalType::Open => {
                    if !self.test_height_range_overlap(&from_sector, &to_sector) {
                        return false;
                    }
                }
            }
        }

        self.has_line_of_sight(from_idx, to_idx)
    }

    /// Performs fine ray testing between subsectors using multiple test points
    /// Returns true if any ray between the subsectors has an unobstructed path
    fn has_line_of_sight(&self, from_idx: usize, to_idx: usize) -> bool {
        #[cfg(feature = "hprof")]
        profile!("has_line_of_sight");

        // Use pre-calculated BSP AABBs with orphaned space fixes
        let from_aabb = &self.subsector_aabbs[from_idx];
        let to_aabb = &self.subsector_aabbs[to_idx];

        let from_points = self.generate_test_points_from_subsector(from_idx, from_aabb);
        let to_points = self.generate_test_points_from_subsector(to_idx, to_aabb);

        for &from_point in &from_points {
            for &to_point in &to_points {
                if self.is_ray_unobstructed(from_point, to_point, from_idx, to_idx) {
                    return true;
                }
            }
        }

        false
    }

    // ============================================================================
    // GEOMETRIC TESTING FUNCTIONS
    // ============================================================================

    /// Returns all line segments that belong to a specific subsector
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

    /// Tests if a 3D ray from one subsector to another is blocked by geometry
    /// Returns true if the ray has an unobstructed path, false if blocked
    fn is_ray_unobstructed(
        &self,
        ray_start: Vec2,
        ray_end: Vec2,
        from_subsector: usize,
        to_subsector: usize,
    ) -> bool {
        #[cfg(feature = "hprof")]
        profile!("is_ray_unobstructed");
        let ray_length = (ray_end - ray_start).length();

        if ray_length < MIN_RAY_LENGTH {
            return true; // Points are essentially the same
        }

        // Create ray bounding box for culling
        // let ray_min_x = ray_start.x.min(ray_end.x) - RAY_BBOX_PADDING;
        // let ray_max_x = ray_start.x.max(ray_end.x) + RAY_BBOX_PADDING;
        // let ray_min_y = ray_start.y.min(ray_end.y) - RAY_BBOX_PADDING;
        // let ray_max_y = ray_start.y.max(ray_end.y) + RAY_BBOX_PADDING;

        // Test only against precomputed blocking segments
        for &seg_idx in &self.blocking_segments {
            // let seg_bbox = &self.segment_bboxes[seg_idx];
            let segment = &self.segments[seg_idx];

            // // Fast bounding box test using precomputed bbox
            // if seg_bbox.right < ray_min_x
            //     || seg_bbox.left > ray_max_x
            //     || seg_bbox.top < ray_min_y
            //     || seg_bbox.bottom > ray_max_y
            // {
            //     continue;
            // }

            // Test intersection and Z range - if ray hits segment, check if it can pass through
            if let Some(intersection_point) =
                self.find_ray_segment_intersection_point(segment, ray_start, ray_end)
            {
                // Ignore intersections that occur exactly at the ray start point
                let distance_from_start = (intersection_point - ray_start).length();
                if distance_from_start < RAY_ENDPOINT_TOLERANCE {
                    continue;
                }

                // Also ignore intersections very close to the ray end point
                let distance_from_end = (intersection_point - ray_end).length();
                if distance_from_end < RAY_ENDPOINT_TOLERANCE {
                    continue;
                }

                let front_sector = &*segment.frontsector;
                if let Some(back_sector_ptr) = &segment.backsector {
                    let back_sector = &**back_sector_ptr;

                    // Find portal between these sectors for Z range checking
                    if let Some(portal) = self.find_portal_between_sectors(
                        front_sector.num as usize,
                        back_sector.num as usize,
                    ) {
                        // For movable portals (doors), use more permissive visibility test
                        if portal.portal_type == PortalType::Movable {
                            let movable_result =
                                self.can_see_through_movable_portal(front_sector, back_sector);
                            if !movable_result {
                                return false; // Ray blocked by movable portal
                            }
                        } else {
                            // For open portals, check if ray passes through or hits floor/ceiling
                            let z_blocked = self.is_ray_blocked_by_portal_height(
                                portal,
                                ray_start,
                                ray_end,
                                intersection_point,
                                from_subsector,
                                to_subsector,
                            );
                            if z_blocked {
                                return false; // Ray blocked by floor/ceiling at portal
                            }
                        }
                    } else {
                        // No portal found, use standard height overlap check
                        let height_overlap =
                            self.test_height_range_overlap(front_sector, back_sector);
                        if !height_overlap {
                            return false; // Ray blocked by height difference
                        }
                    }
                } else {
                    return false; // Single-sided wall always blocks ray
                }
            }
        }

        true // Ray is not blocked
    }

    /// Tests 2D ray-segment intersection and returns the intersection point if found
    /// Returns Some(Vec2) with the intersection point, or None if no intersection
    fn find_ray_segment_intersection_point(
        &self,
        segment: &Segment,
        ray_start: Vec2,
        ray_end: Vec2,
    ) -> Option<Vec2> {
        #[cfg(feature = "hprof")]
        profile!("find_ray_segment_intersection_point");
        let seg_v1 = segment.v1;
        let seg_v2 = segment.v2;

        let ray_dir = ray_end - ray_start;
        let seg_dir = seg_v2 - seg_v1;

        let det = ray_dir.x * seg_dir.y - ray_dir.y * seg_dir.x;

        if det.abs() < f32::EPSILON {
            return None; // Lines are parallel
        }

        let to_seg = seg_v1 - ray_start;
        let t = (to_seg.x * seg_dir.y - to_seg.y * seg_dir.x) / det;
        let u = (to_seg.x * ray_dir.y - to_seg.y * ray_dir.x) / det;

        if t >= 0.0 && t <= 1.0 && u >= 0.0 && u <= 1.0 {
            Some(ray_start + ray_dir * t)
        } else {
            None
        }
    }

    // ============================================================================
    // PORTAL AND HEIGHT TESTING FUNCTIONS
    // ============================================================================

    /// Finds a portal connection between two specific sectors
    /// Returns Some(portal) if a connection exists, None otherwise
    fn find_portal_between_sectors(&self, from_sector: usize, to_sector: usize) -> Option<&Portal> {
        #[cfg(feature = "hprof")]
        profile!("find_portal_between_sectors");
        self.portals.iter().find(|portal| {
            let front_idx = portal.front_sector.num as usize;
            let back_idx = portal.back_sector.num as usize;
            (front_idx == from_sector && back_idx == to_sector)
                || (front_idx == to_sector && back_idx == from_sector)
        })
    }

    /// Tests visibility through movable portals using expanded height ranges
    /// Returns true if visibility is possible through the movable portal
    fn can_see_through_movable_portal(&self, from_sector: &Sector, to_sector: &Sector) -> bool {
        #[cfg(feature = "hprof")]
        profile!("can_see_through_movable_portal");
        let from_movable = self.is_movable_sector(from_sector);
        let to_movable = self.is_movable_sector(to_sector);

        // For PVS calculations, assume doors/movable sectors can be in open state
        if from_movable || to_movable {
            // Use expanded height range for movable sectors to account for movement
            let movement_range = MOVEMENT_RANGE; // TODO: get this from surrounding sectors
            return self.test_height_range_overlap_with_expansion(
                from_sector,
                to_sector,
                movement_range,
            );
        }

        self.test_height_range_overlap(from_sector, to_sector)
    }

    /// Tests if two sectors have overlapping height ranges for visibility
    /// Returns true if sectors can potentially see each other based on floor/ceiling heights
    pub(super) fn test_height_range_overlap(
        &self,
        from_sector: &Sector,
        to_sector: &Sector,
    ) -> bool {
        self.test_height_range_overlap_with_expansion(from_sector, to_sector, 0.0)
    }

    /// Tests if two sectors have overlapping height ranges for visibility
    /// Returns true if sectors can potentially see each other based on floor/ceiling heights
    /// Can optionally expand ranges for movable geometry
    fn test_height_range_overlap_with_expansion(
        &self,
        from_sector: &Sector,
        to_sector: &Sector,
        expansion: f32,
    ) -> bool {
        #[cfg(feature = "hprof")]
        profile!("test_height_range_overlap_with_expansion");
        let from_floor = from_sector.floorheight - expansion;
        let from_ceiling = from_sector.ceilingheight + expansion;
        let to_floor = to_sector.floorheight - expansion;
        let to_ceiling = to_sector.ceilingheight + expansion;

        // Handle zero-height sectors (closed doors) - they can see neighbors
        if (from_sector.ceilingheight - from_sector.floorheight).abs() < ZERO_HEIGHT_TOLERANCE
            || (to_sector.ceilingheight - to_sector.floorheight).abs() < ZERO_HEIGHT_TOLERANCE
        {
            return true;
        }

        // More permissive check for staircase visibility - allow if there's any vertical overlap
        // or if height difference is reasonable for stairs/platforms
        let height_diff = (from_sector.floorheight - to_sector.floorheight).abs();

        // Allow visibility if sectors overlap or are within reasonable step height
        from_floor.max(to_floor) < from_ceiling.min(to_ceiling) || height_diff <= MAX_STEP_HEIGHT
    }

    /// Tests if a 3D ray is blocked by floor or ceiling when passing through a portal
    /// Returns true if ray is blocked by floor/ceiling, false if ray passes through
    pub(super) fn is_ray_blocked_by_portal_height(
        &self,
        portal: &Portal,
        ray_start: Vec2,
        ray_end: Vec2,
        intersection_point: Vec2,
        from_subsector: usize,
        to_subsector: usize,
    ) -> bool {
        #[cfg(feature = "hprof")]
        profile!("is_ray_blocked_by_portal_height");
        // Get source and target sectors from subsectors
        let from_sector = &*self.subsectors[from_subsector].sector;
        let to_sector = &*self.subsectors[to_subsector].sector;

        // Calculate interpolation factor based on 2D distance along ray
        let ray_length = (ray_end - ray_start).length();
        let intersection_distance = (intersection_point - ray_start).length();
        let t = if ray_length > 0.0 {
            intersection_distance / ray_length
        } else {
            0.0
        };

        // Test multiple ray heights - if any pass through, visibility is allowed
        let portal_floor = portal.z_position;
        let portal_ceiling = portal.z_position + portal.z_range;

        // Player eye height from floor (standard Doom player height)
        let player_eye_height = PLAYER_EYE_HEIGHT; // TODO: use portal Z center

        // Test 1: Source center (player eye level) to target bottom
        let ray1_from_z = from_sector.floorheight + player_eye_height;
        let ray1_to_z = to_sector.floorheight;
        let ray1_z_at_intersection = ray1_from_z + (ray1_to_z - ray1_from_z) * t;

        // Test 2: Source center (player eye level) to target top
        let ray2_from_z = from_sector.floorheight + player_eye_height;
        let ray2_to_z = to_sector.ceilingheight;
        let ray2_z_at_intersection = ray2_from_z + (ray2_to_z - ray2_from_z) * t;

        // Test 3: Source floor to target center
        let ray3_from_z = from_sector.floorheight;
        let ray3_to_z =
            to_sector.floorheight + (to_sector.ceilingheight - to_sector.floorheight) * 0.5;
        let ray3_z_at_intersection = ray3_from_z + (ray3_to_z - ray3_from_z) * t;

        // Test 4: Source ceiling to target center
        let ray4_from_z = from_sector.ceilingheight;
        let ray4_to_z =
            to_sector.floorheight + (to_sector.ceilingheight - to_sector.floorheight) * 0.5;
        let ray4_z_at_intersection = ray4_from_z + (ray4_to_z - ray4_from_z) * t;

        // Check if any ray passes through portal
        let ray1_passes =
            ray1_z_at_intersection >= portal_floor && ray1_z_at_intersection <= portal_ceiling;
        let ray2_passes =
            ray2_z_at_intersection >= portal_floor && ray2_z_at_intersection <= portal_ceiling;
        let ray3_passes =
            ray3_z_at_intersection >= portal_floor && ray3_z_at_intersection <= portal_ceiling;
        let ray4_passes =
            ray4_z_at_intersection >= portal_floor && ray4_z_at_intersection <= portal_ceiling;

        // Ray is blocked only if ALL rays are blocked
        !(ray1_passes || ray2_passes || ray3_passes || ray4_passes)
    }

    // ============================================================================
    // BSP TREE FUNCTIONS
    // ============================================================================

    /// Finds the BSP node AABB for a given subsector and fixes missing space coverage
    fn fix_subsector_bsp_aabb(
        &self,
        target_subsector_idx: usize,
        nodes: &mut [Node],
        start_node: u32,
    ) -> (BBox, Option<usize>) {
        #[cfg(feature = "hprof")]
        profile!("fix_subsector_bsp_aabb");
        if let Some((aabb, parent_node_idx, side)) =
            self.find_subsector_aabb_in_bsp_tree(target_subsector_idx, nodes, start_node, None, 0)
        {
            let extended_segment_id =
                self.fix_missing_space_coverage(target_subsector_idx, parent_node_idx, side, nodes);

            // If we extended an AABB, we need to recalculate it
            if extended_segment_id.is_some() {
                if let Some(updated_aabb) = self.find_subsector_aabb_in_bsp_tree(
                    target_subsector_idx,
                    nodes,
                    start_node,
                    None,
                    0,
                ) {
                    (updated_aabb.0, extended_segment_id)
                } else {
                    (aabb, extended_segment_id)
                }
            } else {
                (aabb, extended_segment_id)
            }
        } else {
            (BBox::default(), None)
        }
    }

    /// Recursively traverses BSP tree to find the AABB for a specific subsector with parent info
    /// Returns Some((aabb, parent_node_index, parent_side)) if found, None otherwise
    fn find_subsector_aabb_in_bsp_tree(
        &self,
        target_subsector_idx: usize,
        nodes: &[Node],
        node_index: u32,
        parent_node_idx: Option<usize>,
        parent_side: usize,
    ) -> Option<(BBox, Option<usize>, usize)> {
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
            if let Some(result) = self.find_subsector_aabb_in_bsp_tree(
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

    /// Fixes missing space coverage by extending this subsector's AABB to cover uncovered parent space
    /// Returns Some(segment_index) if the AABB was extended, None otherwise
    fn fix_missing_space_coverage(
        &self,
        target_subsector_idx: usize,
        parent_node_idx: Option<usize>,
        target_side: usize,
        nodes: &mut [Node],
    ) -> Option<usize> {
        let Some(parent_idx) = parent_node_idx else {
            return None;
        };
        if parent_idx >= nodes.len() {
            return None;
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
            let mut extended_aabb = current_aabb.clone();
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

            // Find the first segment in this subsector and return its index for reference
            if target_subsector_idx < self.subsectors.len() {
                let subsector = &self.subsectors[target_subsector_idx];
                let first_segment_idx = subsector.start_seg as usize;
                if first_segment_idx < self.segments.len() {
                    return Some(first_segment_idx);
                }
            }
        }

        None
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
