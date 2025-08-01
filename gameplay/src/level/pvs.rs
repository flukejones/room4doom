#[cfg(feature = "hprof")]
use coarse_prof::profile;
use log::info;

use crate::BSP3D;
use crate::level::map_defs::{Segment, SubSector};
use glam::Vec2;
use rayon::prelude::*;
use std::collections::HashSet;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

// Ray casting constants
const MIN_RAY_LENGTH: f32 = 0.1;
const RAY_ENDPOINT_TOLERANCE: f32 = 0.1;

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

    /// Atomically marks subsector `to` as visible from subsector `from`
    pub fn set_visible_atomic(&self, from: usize, to: usize) {
        #[cfg(feature = "hprof")]
        profile!("compact_pvs_set_visible_atomic");
        let bit_index = from * self.subsector_count + to;
        let word_index = bit_index / 32;
        let bit_offset = bit_index % 32;
        let mask = 1u32 << bit_offset;

        // Use atomic fetch_or for thread-safe bit setting
        let ptr = self.data.as_ptr() as *mut std::sync::atomic::AtomicU32;
        unsafe {
            let atomic_word = &*ptr.add(word_index);
            atomic_word.fetch_or(mask, Ordering::Relaxed);
        }
    }

    /// Returns true if subsector `to` is visible from subsector `from`
    pub fn is_visible(&self, from: usize, to: usize) -> bool {
        let bit_index = from * self.subsector_count + to;
        let word_index = bit_index / 32;
        let bit_offset = bit_index % 32;
        (self.data[word_index] & (1u32 << bit_offset)) != 0
    }

    // TODO: cache this in BSP leaves when building it so it's in order.
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

    blocking_segments: Vec<usize>,
}

impl PVS {
    // ============================================================================
    // PUBLIC API FUNCTIONS
    // ============================================================================

    /// Creates a new PVS for the given number of subsectors
    pub fn new(subsector_count: usize) -> Self {
        Self {
            subsector_count,
            visibility_data: CompactPVS::new(subsector_count),

            blocking_segments: Vec::new(),
        }
    }

    /// Builds PVS data from map geometry using portal-based visibility
    pub fn build(subsectors: &[SubSector], segments: &[Segment], bsp: &BSP3D) -> Self {
        #[cfg(feature = "hprof")]
        profile!("pvs_build");

        log::info!("Building PVS for {} subsectors", subsectors.len());
        let phase_start = std::time::Instant::now();
        let mut pvs = Self::new(subsectors.len());

        // Precompute segment optimizations
        log::info!("Precomputing segment optimizations...");
        let seg_opt_start = std::time::Instant::now();
        for (idx, segment) in segments.iter().enumerate() {
            if segment.backsector.is_none() {
                pvs.blocking_segments.push(idx);
            }
        }
        log::info!(
            "Segment optimization took {:.2}ms ({} blocking segments of {})",
            seg_opt_start.elapsed().as_secs_f32() * 1000.0,
            pvs.blocking_segments.len(),
            segments.len()
        );

        pvs.build_subsector_visibility(segments, subsectors, bsp);

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

    /// Attempts to load PVS data from cache, returns None if unavailable or
    /// invalid
    pub fn load_from_cache(
        map_name: &str,
        map_hash: u64,
        expected_subsectors: usize,
    ) -> Option<Self> {
        #[cfg(feature = "hprof")]
        profile!("load_from_cache");
        match Self::get_pvs_cache_path(map_name, map_hash) {
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
            blocking_segments: Vec::new(),
        })
    }

    /// Returns the cache file path for the given WAD and map
    pub fn get_pvs_cache_path(
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

        let filename = format!("{map_name}_{map_hash}.pvs");
        Ok(cache_dir.join(filename))
    }

    /// Reports progress for long-running subsector visibility calculations
    fn report_progress(
        &self,
        processed_pairs: usize,
        total_pairs: f32,
        function_start: std::time::Instant,
        los_passed: usize,
        los_total: usize,
    ) {
        let progress = (processed_pairs as f32 / total_pairs) * 100.0;
        let elapsed = function_start.elapsed().as_secs_f32();
        let remaining_time = if progress > 0.0 {
            elapsed * (100.0 - progress) / progress
        } else {
            0.0
        };

        print!(
            "\rPVS Build: {:.1}% | Time: {:.1}s | ETA: {:.1}s | LoS: {}/{} ({:.1}%)",
            progress,
            elapsed,
            remaining_time,
            los_passed,
            los_total,
            if los_total > 0 {
                (los_passed as f32 / los_total as f32) * 100.0
            } else {
                0.0
            },
        );
        std::io::Write::flush(&mut std::io::stdout()).unwrap();
    }

    // ============================================================================
    // PHASE 4: SUBSECTOR VISIBILITY BUILDING
    // ============================================================================

    /// Builds subsector-to-subsector visibility using on-demand portal
    /// connectivity checks
    fn build_subsector_visibility(
        &mut self,
        segments: &[Segment],
        subsectors: &[SubSector],
        bsp: &BSP3D,
    ) {
        #[cfg(feature = "hprof")]
        profile!("build_subsector_visibility");

        let function_start = std::time::Instant::now();
        let total_pairs = (self.subsector_count * self.subsector_count) as f32;
        let processed_pairs = AtomicUsize::new(0);
        let last_progress_time = Mutex::new(std::time::Instant::now());
        let line_of_sight_tests = AtomicUsize::new(0);
        let line_of_sight_passed = AtomicUsize::new(0);

        let subsector_count = bsp.get_subsector_leaf_count();

        // Create thread-safe copies of necessary data
        let blocking_segments = self.blocking_segments.clone();

        // Create mapping from segment index to subsector index
        let mut segment_to_subsector: Vec<Option<usize>> = vec![None; segments.len()];
        for (subsector_idx, subsector) in subsectors.iter().enumerate() {
            let start_seg = subsector.start_seg as usize;
            let end_seg = start_seg + subsector.seg_count as usize;
            for seg_idx in start_seg..end_seg.min(segments.len()) {
                segment_to_subsector[seg_idx] = Some(subsector_idx);
            }
        }

        // Extract segment data into thread-safe format
        let segment_data: Vec<_> = segments
            .iter()
            .enumerate()
            .map(|(idx, seg)| {
                let front_sector = unsafe { &*seg.frontsector.inner };
                let back_sector = seg.backsector.as_ref().map(|bs| unsafe { &*bs.inner });
                (
                    *seg.v1.clone(),
                    *seg.v2.clone(),
                    front_sector.floorheight,
                    front_sector.ceilingheight,
                    back_sector.map(|bs| (bs.floorheight, bs.ceilingheight)),
                    segment_to_subsector[idx],
                )
            })
            .collect();

        // Process subsectors in parallel
        (0..subsector_count)
            .into_par_iter()
            .for_each(|source_subsector_idx| {
                // Set self-visibility
                self.visibility_data
                    .set_visible_atomic(source_subsector_idx, source_subsector_idx);

                for target_subsector_idx in 0..self.subsector_count {
                    processed_pairs.fetch_add(1, Ordering::Relaxed);

                    // Progress reporting every 500ms using shared state
                    let now = std::time::Instant::now();
                    if let Ok(mut last_time) = last_progress_time.try_lock() {
                        if now.duration_since(*last_time).as_millis() >= 500 {
                            self.report_progress(
                                processed_pairs.load(Ordering::Relaxed),
                                total_pairs,
                                function_start,
                                line_of_sight_passed.load(Ordering::Relaxed),
                                line_of_sight_tests.load(Ordering::Relaxed),
                            );
                            *last_time = now;
                        }
                    }

                    if source_subsector_idx == target_subsector_idx {
                        continue;
                    }

                    line_of_sight_tests.fetch_add(1, Ordering::Relaxed);

                    if self.can_subsector_see_subsector(
                        source_subsector_idx,
                        target_subsector_idx,
                        &segment_data,
                        &blocking_segments,
                        bsp,
                    ) {
                        line_of_sight_passed.fetch_add(1, Ordering::Relaxed);
                        self.visibility_data
                            .set_visible_atomic(source_subsector_idx, target_subsector_idx);
                    }
                }
            });

        // Final progress report
        self.report_progress(
            processed_pairs.load(Ordering::Relaxed),
            total_pairs,
            function_start,
            line_of_sight_passed.load(Ordering::Relaxed),
            line_of_sight_tests.load(Ordering::Relaxed),
        );
        println!(); // New line after progress
    }

    /// Tests if one subsector can see another through geometric line-of-sight
    /// (parallel version)
    fn can_subsector_see_subsector(
        &self,
        from_idx: usize,
        to_idx: usize,
        segment_data: &[(Vec2, Vec2, f32, f32, Option<(f32, f32)>, Option<usize>)],
        blocking_segments: &[usize],
        bsp: &BSP3D,
    ) -> bool {
        if from_idx == to_idx {
            return true;
        }

        // TODO: AABB is broken in some subsector leaves, fix at source.
        // // Get subsector centers for directional filtering
        // if let (Some(from_aabb), Some(to_aabb)) = (
        //     bsp.get_subsector_leaf(from_idx),
        //     bsp.get_subsector_leaf(to_idx),
        // ) {
        //     let from_aabb = &from_aabb.aabb;
        //     let to_aabb = &to_aabb.aabb;
        //     let from_center = (from_aabb.min + from_aabb.max) * 0.5;
        //     let to_center = (to_aabb.min + to_aabb.max) * 0.5;

        //     // Calculate direction vector
        //     let direction = Vec2::new(to_center.x - from_center.x, to_center.y -
        // from_center.y);

        //     // Quick distance check - if too far, skip
        //     if direction.length() > MAX_DISTANCE {
        //         return false;
        //     }
        // }

        // Use ray casting for visibility testing
        self.has_line_of_sight(from_idx, to_idx, segment_data, blocking_segments, bsp)
    }

    // TODO: check if neighbour shares a vertex on segment, if so use neighbours
    // result for target, only if.. how? This is a potentially expensive flood fill
    //
    /// Performs ray testing between subsectors using multiple test points
    /// Returns true if any ray between the subsectors has an unobstructed path
    fn has_line_of_sight(
        &self,
        from_idx: usize,
        to_idx: usize,
        segment_data: &[(Vec2, Vec2, f32, f32, Option<(f32, f32)>, Option<usize>)],
        blocking_segments: &[usize],
        bsp: &BSP3D,
    ) -> bool {
        #[cfg(feature = "hprof")]
        profile!("has_line_of_sight");

        // Test center-to-center ray first (most common case)
        if let Some(from_aabb) = bsp.get_subsector_leaf(from_idx) {
            if let Some(to_aabb) = bsp.get_subsector_leaf(to_idx) {
                let from_aabb = &from_aabb.aabb;
                let to_aabb = &to_aabb.aabb;
                let from_center = Vec2::new(
                    (from_aabb.min.x + from_aabb.max.x) * 0.5,
                    (from_aabb.min.y + from_aabb.max.y) * 0.5,
                );
                let to_center = Vec2::new(
                    (to_aabb.min.x + to_aabb.max.x) * 0.5,
                    (to_aabb.min.y + to_aabb.max.y) * 0.5,
                );
                if self.is_ray_unobstructed(
                    from_center,
                    to_center,
                    segment_data,
                    blocking_segments,
                    bsp,
                ) {
                    return true;
                }
            }
        }

        // Get all sample points once
        let from_points = self.get_sample_points_offset(from_idx, bsp);
        let to_points = self.get_sample_points_offset(to_idx, bsp);

        // Test offset points (quick additional samples)
        for &from_point in &from_points {
            for &to_point in &to_points {
                if self.is_ray_unobstructed(
                    from_point,
                    to_point,
                    segment_data,
                    blocking_segments,
                    bsp,
                ) {
                    return true;
                }
            }
        }

        // Only test polygon vertices if offset points failed
        let from_poly_points = self.get_sample_points_polys(from_idx, bsp);
        let to_poly_points = self.get_sample_points_polys(to_idx, bsp);

        // Test from polygon points to offset points first (mixed approach)
        for &from_point in &from_poly_points {
            for &to_point in &to_points {
                if self.is_ray_unobstructed(
                    from_point,
                    to_point,
                    segment_data,
                    blocking_segments,
                    bsp,
                ) {
                    return true;
                }
            }
        }

        // Test from offset points to polygon points
        for &from_point in &from_points {
            for &to_point in &to_poly_points {
                if self.is_ray_unobstructed(
                    from_point,
                    to_point,
                    segment_data,
                    blocking_segments,
                    bsp,
                ) {
                    return true;
                }
            }
        }

        // Final test: polygon to polygon (most expensive)
        for &from_point in &from_poly_points {
            for &to_point in &to_poly_points {
                if self.is_ray_unobstructed(
                    from_point,
                    to_point,
                    segment_data,
                    blocking_segments,
                    bsp,
                ) {
                    return true;
                }
            }
        }

        false
    }

    fn get_sample_points_offset(&self, subsector_idx: usize, bsp: &BSP3D) -> Vec<Vec2> {
        let Some(poly) = bsp.get_subsector_leaf(subsector_idx) else {
            return Vec::new();
        };

        let aabb = &poly.aabb;
        let center = Vec2::new(
            (aabb.min.x + aabb.max.x) * 0.5,
            (aabb.min.y + aabb.max.y) * 0.5,
        );

        let offset = ((aabb.max.x - aabb.min.x) + (aabb.max.y - aabb.min.y)) * 0.15;
        vec![
            center,
            Vec2::new(center.x + offset, center.y),
            Vec2::new(center.x - offset, center.y),
            Vec2::new(center.x, center.y + offset),
            Vec2::new(center.x, center.y - offset),
        ]
    }

    /// Generates test points for geometric ray testing from subsector segments
    fn get_sample_points_polys(&self, subsector_idx: usize, bsp: &BSP3D) -> Vec<Vec2> {
        let Some(geometry) = bsp.get_subsector_leaf(subsector_idx) else {
            return Vec::new();
        };

        let mut unique_points = HashSet::new();

        for triangle in &geometry.polygons {
            for &vertex_idx in &triangle.vertices {
                let vertex = bsp.vertex_get(vertex_idx);
                let point2d = Vec2::new(vertex.x, vertex.y);
                unique_points.insert((point2d.x.to_bits(), point2d.y.to_bits()));
            }
        }

        unique_points
            .into_iter()
            .map(|(x_bits, y_bits)| Vec2::new(f32::from_bits(x_bits), f32::from_bits(y_bits)))
            .collect()
    }

    // ============================================================================
    // GEOMETRIC TESTING FUNCTIONS
    // ============================================================================

    /// Returns all line segments that belong to a specific subsector

    /// Tests if a 3D ray from one subsector to another is blocked by geometry
    /// Returns true if the ray has an unobstructed path, false if blocked
    fn is_ray_unobstructed(
        &self,
        ray_start: Vec2,
        ray_end: Vec2,
        segment_data: &[(Vec2, Vec2, f32, f32, Option<(f32, f32)>, Option<usize>)],
        blocking_segments: &[usize],
        bsp: &BSP3D,
    ) -> bool {
        #[cfg(feature = "hprof")]
        profile!("is_ray_unobstructed");
        let ray_length = (ray_end - ray_start).length();

        if ray_length < MIN_RAY_LENGTH {
            return true; // Points are essentially the same
        }

        const RAY_BBOX_PADDING: f32 = 1.0;
        let ray_min_x = ray_start.x.min(ray_end.x) - RAY_BBOX_PADDING;
        let ray_max_x = ray_start.x.max(ray_end.x) + RAY_BBOX_PADDING;
        let ray_min_y = ray_start.y.min(ray_end.y) - RAY_BBOX_PADDING;
        let ray_max_y = ray_start.y.max(ray_end.y) + RAY_BBOX_PADDING;

        // Test only against precomputed blocking segments
        for &seg_idx in blocking_segments {
            let (v1, v2, front_floor, front_ceiling, back_sector, subsector_idx) =
                &segment_data[seg_idx];

            // Use subsector AABB for culling if available
            let mut skip_segment = false;
            if let Some(subsector_id) = subsector_idx {
                if let Some(leaf) = bsp.get_subsector_leaf(*subsector_id) {
                    let aabb = &leaf.aabb;
                    if aabb.max.x < ray_min_x
                        || aabb.min.x > ray_max_x
                        || aabb.max.y < ray_min_y
                        || aabb.min.y > ray_max_y
                    {
                        skip_segment = true;
                    }
                }
            }

            if skip_segment {
                continue;
            }

            // Test intersection and Z range - if ray hits segment, check if it can pass
            // through
            if let Some(intersection_point) =
                self.find_ray_segment_intersection_point(*v1, *v2, ray_start, ray_end)
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

                if let Some((back_floor, back_ceiling)) = back_sector {
                    // Use height overlap check
                    let height_overlap = self.test_height_range_overlap(
                        *front_floor,
                        *front_ceiling,
                        *back_floor,
                        *back_ceiling,
                    );
                    if !height_overlap {
                        return false; // Ray blocked by height difference
                    }
                } else {
                    return false; // Single-sided wall always blocks ray
                }
            }
        }

        true // Ray is not blocked
    }

    /// Tests 2D ray-segment intersection and returns the intersection point if
    /// found Returns Some(Vec2) with the intersection point, or None if no
    /// intersection
    fn find_ray_segment_intersection_point(
        &self,
        seg_v1: Vec2,
        seg_v2: Vec2,
        ray_start: Vec2,
        ray_end: Vec2,
    ) -> Option<Vec2> {
        #[cfg(feature = "hprof")]
        profile!("find_ray_segment_intersection_point");

        let ray_dir = ray_end - ray_start;
        let seg_dir = seg_v2 - seg_v1;

        let denominator = ray_dir.x * seg_dir.y - ray_dir.y * seg_dir.x;
        if denominator.abs() < 1e-6 {
            return None; // Lines are parallel
        }

        let t = ((seg_v1.x - ray_start.x) * seg_dir.y - (seg_v1.y - ray_start.y) * seg_dir.x)
            / denominator;
        let u = ((seg_v1.x - ray_start.x) * ray_dir.y - (seg_v1.y - ray_start.y) * ray_dir.x)
            / denominator;

        if t >= 0.0 && t <= 1.0 && u >= 0.0 && u <= 1.0 {
            Some(ray_start + t * ray_dir)
        } else {
            None
        }
    }

    // ============================================================================
    // HEIGHT TESTING FUNCTIONS
    // ============================================================================

    /// Tests if two sectors have overlapping height ranges for visibility
    /// Returns true if sectors can potentially see each other based on
    /// floor/ceiling heights
    fn test_height_range_overlap(
        &self,
        front_floor: f32,
        front_ceiling: f32,
        back_floor: f32,
        back_ceiling: f32,
    ) -> bool {
        let front_bottom = front_floor;
        let front_top = front_ceiling;
        let back_bottom = back_floor;
        let back_top = back_ceiling;

        !(front_top <= back_bottom || back_top <= front_bottom)
    }

    // pub(super) fn test_height_range_overlap(
    //     &self,
    //     front_sector: &Sector,
    //     back_sector: &Sector,
    // ) -> bool {
    //     self.test_height_range_overlap_with_expansion(front_sector, back_sector,
    // 0.0) }

    // /// Tests if two sectors have overlapping height ranges for visibility
    // /// Returns true if sectors can potentially see each other based on
    // /// floor/ceiling heights Can optionally expand ranges for movable
    // /// geometry
    // fn test_height_range_overlap_with_expansion(
    //     &self,
    //     from_sector: &Sector,
    //     to_sector: &Sector,
    //     expansion: f32,
    // ) -> bool {
    //     #[cfg(feature = "hprof")]
    //     profile!("test_height_range_overlap_with_expansion");
    //     let from_floor = from_sector.floorheight - expansion;
    //     let from_ceiling = from_sector.ceilingheight + expansion;
    //     let to_floor = to_sector.floorheight - expansion;
    //     let to_ceiling = to_sector.ceilingheight + expansion;

    //     // Handle zero-height sectors (closed doors) - they can see neighbors
    //     if (from_sector.ceilingheight - from_sector.floorheight).abs() <
    // ZERO_HEIGHT_TOLERANCE         || (to_sector.ceilingheight -
    // to_sector.floorheight).abs() < ZERO_HEIGHT_TOLERANCE     {
    //         return true;
    //     }

    //     // More permissive check for staircase visibility - allow if there's any
    //     // vertical overlap or if height difference is reasonable for
    //     // stairs/platforms
    //     let height_diff = (from_sector.floorheight -
    // to_sector.floorheight).abs();

    //     // Allow visibility if sectors overlap or are within reasonable step
    // height     from_floor.max(to_floor) < from_ceiling.min(to_ceiling) ||
    // height_diff <= MAX_STEP_HEIGHT }

    // ============================================================================
    // BSP TREE FUNCTIONS
    // ============================================================================
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compact_pvs() {
        let pvs = CompactPVS::new(64);

        assert!(!pvs.is_visible(0, 1));
        pvs.set_visible_atomic(0, 1);
        assert!(pvs.is_visible(0, 1));

        pvs.set_visible_atomic(63, 0);
        assert!(pvs.is_visible(63, 0));
    }
}
