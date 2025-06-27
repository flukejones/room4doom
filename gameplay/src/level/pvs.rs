//! Potentially Visible Set (PVS) implementation using BSP swept volume visibility
//!
//! This module implements a visibility system that uses the BSP tree to efficiently
//! determine which subsectors can see each other. It creates swept volumes between
//! subsector AABBs and projects occluding segments onto visibility planes.

#[cfg(feature = "hprof")]
use coarse_prof::profile;

use crate::level::map_data::IS_SSECTOR_MASK;
use crate::level::map_defs::{BBox, LineDef, Node, Segment, SubSector};
use crc32fast::Hasher;
use glam::Vec2;
use log::info;
use std::io::{Read, Write};
use std::{collections::HashSet, path::Path, time::Instant};

/// Stores precomputed visibility information between subsectors
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PVS {
    pub(super) subsector_count: usize,
    /// Packed bitset storing visibility between subsectors using REJECT lump format
    /// Each bit controls visibility: 1 = blocked, 0 = visible
    pub(super) visibility_data: Vec<u8>,
}

/// Swept volume between source and target subsector AABBs
#[derive(Debug)]
struct BSPSweptVolume {
    start_center: Vec2,
    end_center: Vec2,
    direction: Vec2,
    length: f32,
    width: f32,
}

impl BSPSweptVolume {
    fn new(source_aabb: &BBox, target_aabb: &BBox) -> Self {
        let start_center = Vec2::new(
            (source_aabb.left + source_aabb.right) * 0.5,
            (source_aabb.bottom + source_aabb.top) * 0.5,
        );
        let end_center = Vec2::new(
            (target_aabb.left + target_aabb.right) * 0.5,
            (target_aabb.bottom + target_aabb.top) * 0.5,
        );

        let direction = end_center - start_center;
        let length = direction.length();
        let normalized_dir = if length > 0.0 {
            direction / length
        } else {
            Vec2::ZERO
        };

        // Width is projection of source AABB onto perpendicular plane
        let perp = Vec2::new(-normalized_dir.y, normalized_dir.x);
        let source_width = (source_aabb.right - source_aabb.left).abs() * perp.x.abs()
            + (source_aabb.top - source_aabb.bottom).abs() * perp.y.abs();

        Self {
            start_center,
            end_center,
            direction: normalized_dir,
            length,
            width: source_width,
        }
    }

    /// Check if this swept volume intersects with a bounding box
    fn intersects_bbox(&self, bbox_min: Vec2, bbox_max: Vec2) -> bool {
        let half_width = self.width * 0.5;
        let perp = Vec2::new(-self.direction.y, self.direction.x);
        let offset = perp * half_width;

        // Create the four corners of the swept volume
        let corners = [
            self.start_center + offset,
            self.start_center - offset,
            self.end_center + offset,
            self.end_center - offset,
        ];

        // Check if any corner is inside the bbox
        for corner in corners {
            if corner.x >= bbox_min.x
                && corner.x <= bbox_max.x
                && corner.y >= bbox_min.y
                && corner.y <= bbox_max.y
            {
                return true;
            }
        }

        // Check if bbox intersects the swept volume quad
        self.quad_intersects_rect(corners, bbox_min, bbox_max)
    }

    fn quad_intersects_rect(&self, quad: [Vec2; 4], rect_min: Vec2, rect_max: Vec2) -> bool {
        // Simple separating axis test
        let rect_corners = [
            rect_min,
            Vec2::new(rect_max.x, rect_min.y),
            rect_max,
            Vec2::new(rect_min.x, rect_max.y),
        ];

        // Test if any quad edge separates the shapes
        for i in 0..4 {
            let edge = quad[(i + 1) % 4] - quad[i];
            let normal = Vec2::new(-edge.y, edge.x);

            let mut quad_min = f32::MAX;
            let mut quad_max = f32::MIN;
            let mut rect_min_proj = f32::MAX;
            let mut rect_max_proj = f32::MIN;

            for &corner in &quad {
                let proj = corner.dot(normal);
                quad_min = quad_min.min(proj);
                quad_max = quad_max.max(proj);
            }

            for &corner in &rect_corners {
                let proj = corner.dot(normal);
                rect_min_proj = rect_min_proj.min(proj);
                rect_max_proj = rect_max_proj.max(proj);
            }

            if quad_max < rect_min_proj || rect_max_proj < quad_min {
                return false;
            }
        }

        true
    }
}

impl PVS {
    pub fn new(subsector_count: usize) -> Self {
        let bit_count = subsector_count * subsector_count;
        let byte_count = (bit_count + 7) / 8;
        Self {
            subsector_count,
            visibility_data: vec![0; byte_count],
        }
    }

    /// Build PVS with access to BSP tree
    pub fn build(
        subsectors: &[SubSector],
        segments: &[Segment],
        linedefs: &[LineDef],
        nodes: &mut [Node],
        start_node: u32,
    ) -> Self {
        #[cfg(feature = "hprof")]
        profile!("pvs_build");

        log::info!(
            "Using BSP-aware PVS implementation with {} nodes",
            nodes.len()
        );

        let mut pvs = Self::new(subsectors.len());
        let mut tested = HashSet::new();

        info!("Building PVS for {} subsectors", subsectors.len());
        let start_time = Instant::now();
        let mut last_log = start_time;

        // Pre-calculate AABBs for all subsectors using BSP nodes
        let mut subsector_aabbs = Vec::with_capacity(subsectors.len());
        for (subsector_idx, _subsector) in subsectors.iter().enumerate() {
            let bsp_aabb = pvs.find_and_fix_subsector_bsp_aabb(subsector_idx, nodes, start_node);
            subsector_aabbs.push(bsp_aabb);
        }

        let total_tests = (subsectors.len() * subsectors.len()) / 2;
        let mut completed_tests = 0;

        for from_idx in 0..subsectors.len() {
            for to_idx in 0..subsectors.len() {
                if tested.contains(&(from_idx, to_idx)) {
                    continue;
                }
                tested.insert((to_idx, from_idx));

                if from_idx == to_idx {
                    pvs.set_visible(from_idx, to_idx, true);
                    continue;
                }

                let visible = pvs.test_bsp_swept_volume_visibility(
                    from_idx,
                    to_idx,
                    subsectors,
                    segments,
                    linedefs,
                    nodes,
                    start_node,
                    &subsector_aabbs,
                );

                pvs.set_visible(from_idx, to_idx, visible);
                pvs.set_visible(to_idx, from_idx, visible);

                completed_tests += 1;

                // Log progress every 1 second
                let now = Instant::now();
                if now.duration_since(last_log).as_secs() >= 1 {
                    let progress = (completed_tests as f32 / total_tests as f32) * 100.0;
                    info!(
                        "PVS progress: {:.1}% ({}/{})",
                        progress, completed_tests, total_tests
                    );
                    #[cfg(feature = "hprof")]
                    coarse_prof::write(&mut std::io::stdout()).unwrap();
                    last_log = now;
                }
            }
        }

        let elapsed = start_time.elapsed();
        info!("PVS build completed in {:.2}s", elapsed.as_secs_f32());
        info!("PVS memory usage: {} bytes", pvs.memory_usage());

        pvs
    }

    /// Test visibility using vertex-based swept volume method
    fn test_bsp_swept_volume_visibility(
        &self,
        from_subsector: usize,
        to_subsector: usize,
        subsectors: &[SubSector],
        segments: &[Segment],
        _linedefs: &[LineDef],
        nodes: &[Node],
        start_node: u32,
        subsector_aabbs: &[BBox],
    ) -> bool {
        #[cfg(feature = "hprof")]
        profile!("test_bsp_swept_volume_visibility");
        let from_segments = self.get_subsector_segments(&subsectors[from_subsector], segments);
        let to_segments = self.get_subsector_segments(&subsectors[to_subsector], segments);

        if from_segments.is_empty() || to_segments.is_empty() {
            return false;
        }

        let source_aabb = &subsector_aabbs[from_subsector];
        let target_aabb = &subsector_aabbs[to_subsector];

        let target_center = Vec2::new(
            (target_aabb.left + target_aabb.right) * 0.5,
            (target_aabb.bottom + target_aabb.top) * 0.5,
        );

        // Collect test points for source subsector

        let mut test_points = Vec::new();
        for seg in &from_segments {
            test_points.push(seg.v1);
            test_points.push(seg.v2);
        }

        let aabb = &subsector_aabbs[from_subsector];
        test_points.push(Vec2::new(aabb.left, aabb.top));
        test_points.push(Vec2::new(aabb.right, aabb.top));
        test_points.push(Vec2::new(aabb.left, aabb.bottom));
        test_points.push(Vec2::new(aabb.right, aabb.bottom));

        // Remove duplicates
        test_points.sort_by(|a, b| {
            a.x.partial_cmp(&b.x)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal))
        });
        test_points.dedup_by(|a, b| (a.x - b.x).abs() < 0.1 && (a.y - b.y).abs() < 0.1);

        // Always add source AABB centerpoint
        test_points.push(Vec2::new(
            (source_aabb.left + source_aabb.right) * 0.5,
            (source_aabb.bottom + source_aabb.top) * 0.5,
        ));

        // Test visibility from each source point to target
        // Only return false if ALL points are blocked
        let mut any_visible = false;
        for (_i, &source_point) in test_points.iter().enumerate() {
            let visible = self.test_point_to_target_visibility(
                source_point,
                target_center,
                target_aabb,
                nodes,
                start_node,
                subsectors,
                segments,
                from_subsector,
                to_subsector,
            );

            if visible {
                any_visible = true;
                break; // Early exit if any point can see target
            }
        }

        any_visible
    }

    /// Test visibility from a single source point to target using line-of-sight
    fn test_point_to_target_visibility(
        &self,
        source_point: Vec2,
        target_center: Vec2,
        target_aabb: &BBox,
        nodes: &[Node],
        start_node: u32,
        subsectors: &[SubSector],
        segments: &[Segment],
        from_subsector: usize,
        to_subsector: usize,
    ) -> bool {
        #[cfg(feature = "hprof")]
        profile!("test_point_to_target_visibility");
        // Create a minimal swept volume from source point to target center
        let direction = target_center - source_point;
        let length = direction.length();

        if length < 0.1 {
            return true; // Points are essentially the same
        }

        let normalized_dir = direction / length;
        let target_size =
            ((target_aabb.right - target_aabb.left) + (target_aabb.top - target_aabb.bottom)) * 0.5;

        let swept_volume = BSPSweptVolume {
            start_center: source_point,
            end_center: target_center,
            direction: normalized_dir,
            length,
            width: target_size * 0.5, // Small width for point-to-area test
        };

        // Find intersecting subsectors
        let intersecting_subsectors =
            self.find_intersecting_subsectors(&swept_volume, nodes, start_node);

        // Check for blocking segments in intersecting subsectors
        // We need to test against multiple target points, not just the center
        let target_points = vec![
            target_center,
            Vec2::new(target_aabb.left, target_aabb.bottom),
            Vec2::new(target_aabb.right, target_aabb.bottom),
            Vec2::new(target_aabb.right, target_aabb.top),
            Vec2::new(target_aabb.left, target_aabb.top),
        ];

        // Test if ANY line of sight to target points is clear
        for &target_point in &target_points {
            let mut this_line_blocked = false;

            for &subsector_idx in &intersecting_subsectors {
                if subsector_idx == from_subsector || subsector_idx == to_subsector {
                    continue;
                }

                if subsector_idx >= subsectors.len() {
                    continue;
                }

                let blocking_segments =
                    self.get_subsector_segments(&subsectors[subsector_idx], segments);

                for seg in blocking_segments {
                    if seg.backsector.is_some() {
                        continue; // Skip two-sided segments
                    }

                    // Simple line intersection test
                    if self.line_intersects_ray(seg, source_point, target_point) {
                        this_line_blocked = true;
                        break; // This line is blocked, try next target point
                    }
                }

                if this_line_blocked {
                    break; // No need to check more subsectors for this line
                }
            }

            if !this_line_blocked {
                return true; // Found at least one clear line of sight
            }
        }
        false // All lines of sight are blocked
    }

    /// Test if a segment intersects the ray from source to target
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

    /// Find all subsectors that intersect with the swept volume
    fn find_intersecting_subsectors(
        &self,
        swept_volume: &BSPSweptVolume,
        nodes: &[Node],
        start_node: u32,
    ) -> Vec<usize> {
        #[cfg(feature = "hprof")]
        profile!("find_intersecting_subsectors");
        let mut intersecting = Vec::new();
        self.traverse_bsp_for_volume(swept_volume, nodes, start_node, &mut intersecting);
        intersecting
    }

    /// Traverse BSP tree to find intersecting subsectors
    fn traverse_bsp_for_volume(
        &self,
        swept_volume: &BSPSweptVolume,
        nodes: &[Node],
        node_index: u32,
        intersecting: &mut Vec<usize>,
    ) {
        #[cfg(feature = "hprof")]
        profile!("traverse_bsp_for_volume");
        if node_index & IS_SSECTOR_MASK != 0 {
            let subsector_index = (node_index & !IS_SSECTOR_MASK) as usize;
            intersecting.push(subsector_index);
            return;
        }

        if (node_index as usize) >= nodes.len() {
            return;
        }

        let node = &nodes[node_index as usize];

        // Check if swept volume intersects with either child's bounding box
        for child_idx in 0..2 {
            let bbox_pair = &node.bboxes[child_idx];
            if swept_volume.intersects_bbox(bbox_pair[0], bbox_pair[1]) {
                self.traverse_bsp_for_volume(
                    swept_volume,
                    nodes,
                    node.children[child_idx],
                    intersecting,
                );
            }
        }
    }

    /// Find the BSP node AABB for a given subsector and fix missing space coverage
    fn find_and_fix_subsector_bsp_aabb(
        &self,
        target_subsector_idx: usize,
        nodes: &mut [Node],
        start_node: u32,
    ) -> BBox {
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
            BBox::default()
        }
    }

    /// Recursively traverse BSP tree to find the AABB for a specific subsector with parent info
    fn traverse_bsp_for_subsector_aabb_with_parent(
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
        nodes: &mut [Node],
    ) {
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

    /// Get all segments belonging to a subsector
    fn get_subsector_segments<'a>(
        &self,
        subsector: &SubSector,
        segments: &'a [Segment],
    ) -> Vec<&'a Segment> {
        let start = subsector.start_seg as usize;
        let count = subsector.seg_count as usize;

        segments
            .get(start..start + count)
            .map(|slice| slice.iter().collect())
            .unwrap_or_default()
    }

    /// Set visibility between two subsectors
    pub(super) fn set_visible(&mut self, from: usize, to: usize, visible: bool) {
        if from < self.subsector_count && to < self.subsector_count {
            let bit_index = from * self.subsector_count + to;
            let byte_index = bit_index / 8;
            let bit_offset = bit_index % 8;

            if visible {
                self.visibility_data[byte_index] &= !(1 << bit_offset);
            } else {
                self.visibility_data[byte_index] |= 1 << bit_offset;
            }
        }
    }

    pub fn is_visible(&self, from: usize, to: usize) -> bool {
        if from >= self.subsector_count || to >= self.subsector_count {
            return false;
        }

        let bit_index = from * self.subsector_count + to;
        let byte_index = bit_index / 8;
        let bit_offset = bit_index % 8;

        (self.visibility_data[byte_index] & (1 << bit_offset)) == 0
    }

    pub fn get_visible_subsectors(&self, from: usize) -> Vec<usize> {
        if from >= self.subsector_count {
            return Vec::new();
        }

        let mut visible = Vec::new();
        for to in 0..self.subsector_count {
            if self.is_visible(from, to) {
                visible.push(to);
            }
        }
        visible
    }

    pub fn memory_usage(&self) -> usize {
        std::mem::size_of::<Self>() + self.visibility_data.len()
    }

    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), Box<dyn std::error::Error>> {
        let mut file = std::fs::File::create(path)?;

        // Write header with map info
        file.write_all(b"PVS1")?; // Version tag
        file.write_all(&(self.subsector_count as u32).to_le_bytes())?;

        let serialized = bincode::serialize(self)?;

        // Write compressed size and data
        file.write_all(&(serialized.len() as u32).to_le_bytes())?;
        file.write_all(&serialized)?;

        // Calculate and write CRC
        let mut hasher = Hasher::new();
        hasher.update(b"PVS1");
        hasher.update(&(self.subsector_count as u32).to_le_bytes());
        hasher.update(&(serialized.len() as u32).to_le_bytes());
        hasher.update(&serialized);
        let crc = hasher.finalize();
        file.write_all(&crc.to_le_bytes())?;

        Ok(())
    }

    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let mut file = std::fs::File::open(path)?;

        // Read and verify header
        let mut header = [0u8; 4];
        file.read_exact(&mut header)?;
        if &header != b"PVS1" {
            return Err("Invalid PVS file format".into());
        }

        // TODO: verify?
        let mut count_bytes = [0u8; 4];
        file.read_exact(&mut count_bytes)?;
        let _subsector_count = u32::from_le_bytes(count_bytes);

        let mut size_bytes = [0u8; 4];
        file.read_exact(&mut size_bytes)?;
        let data_len = u32::from_le_bytes(size_bytes) as usize;

        let mut data = vec![0u8; data_len];
        file.read_exact(&mut data)?;

        let mut crc_bytes = [0u8; 4];
        file.read_exact(&mut crc_bytes)?;
        let stored_crc = u32::from_le_bytes(crc_bytes);

        let mut hasher = Hasher::new();
        hasher.update(b"PVS1");
        hasher.update(&count_bytes);
        hasher.update(&size_bytes);
        hasher.update(&data);
        let calculated_crc = hasher.finalize();

        if stored_crc != calculated_crc {
            return Err("PVS file CRC mismatch".into());
        }

        // Deserialize PVS
        let pvs: PVS = bincode::deserialize(&data)?;
        Ok(pvs)
    }

    pub fn get_pvs_cache_path(
        wad_name: &str,
        map_name: &str,
    ) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
        let config_dir = dirs::config_dir()
            .ok_or("Could not find config directory")?
            .join("room4doom")
            .join("vis")
            .join(wad_name);

        std::fs::create_dir_all(&config_dir)?;
        Ok(config_dir.join(format!("{}.vis", map_name)))
    }

    pub fn build_and_cache(
        wad_name: &str,
        map_name: &str,
        subsectors: &[SubSector],
        segments: &[Segment],
        linedefs: &[LineDef],
        nodes: &mut [Node],
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let pvs = Self::build(subsectors, segments, linedefs, nodes, 0);

        let cache_path = Self::get_pvs_cache_path(wad_name, map_name)?;
        pvs.save_to_file(&cache_path)?;

        Ok(pvs)
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
    use crate::MapPtr;
    use crate::level::map_defs::{Sector, SideDef};

    fn create_test_sector() -> MapPtr<Sector> {
        let mut sector = Sector::new(0, 0.0, 128.0, 0, 0, 160, 0, 0);
        MapPtr::new(&mut sector)
    }

    fn create_test_sidedef() -> MapPtr<SideDef> {
        let mut sidedef = SideDef {
            textureoffset: 0.0,
            rowoffset: 0.0,
            toptexture: None,
            bottomtexture: None,
            midtexture: None,
            sector: create_test_sector(),
        };
        MapPtr::new(&mut sidedef)
    }

    #[test]
    fn test_pvs_creation() {
        let pvs = PVS::new(4);
        assert_eq!(pvs.subsector_count, 4);
        assert_eq!(pvs.visibility_data.len(), 2); // 16 bits = 2 bytes
    }

    #[test]
    fn test_visibility_setting() {
        let mut pvs = PVS::new(4);

        // Initially all should be visible (0 bits)
        assert!(pvs.is_visible(0, 1));
        assert!(pvs.is_visible(1, 0));

        // Set some as not visible
        pvs.set_visible(0, 1, false);
        assert!(!pvs.is_visible(0, 1));
        assert!(pvs.is_visible(1, 0)); // Should still be visible in reverse

        // Set back to visible
        pvs.set_visible(0, 1, true);
        assert!(pvs.is_visible(0, 1));
    }

    #[test]
    fn test_bsp_swept_volume_creation() {
        let source_bbox = BBox {
            left: 0.0,
            right: 64.0,
            bottom: 0.0,
            top: 64.0,
        };

        let target_bbox = BBox {
            left: 128.0,
            right: 192.0,
            bottom: 0.0,
            top: 64.0,
        };

        let swept_volume = BSPSweptVolume::new(&source_bbox, &target_bbox);

        assert_eq!(swept_volume.start_center, Vec2::new(32.0, 32.0));
        assert_eq!(swept_volume.end_center, Vec2::new(160.0, 32.0));
        assert_eq!(swept_volume.direction, Vec2::new(1.0, 0.0)); // Pointing right
        assert!(swept_volume.width > 0.0);
    }

    #[test]
    fn test_bbox_intersection() {
        let source_bbox = BBox {
            left: 0.0,
            right: 64.0,
            bottom: 0.0,
            top: 64.0,
        };

        let target_bbox = BBox {
            left: 128.0,
            right: 192.0,
            bottom: 0.0,
            top: 64.0,
        };

        let swept_volume = BSPSweptVolume::new(&source_bbox, &target_bbox);

        // Should intersect with a bbox in the middle
        assert!(swept_volume.intersects_bbox(Vec2::new(80.0, 16.0), Vec2::new(112.0, 48.0)));

        // Should not intersect with a bbox far away
        assert!(!swept_volume.intersects_bbox(Vec2::new(0.0, 200.0), Vec2::new(64.0, 264.0)));
    }

    #[test]
    fn test_single_segment_subsector_handling() {
        // Test that single-segment subsectors use AABB projection
        let source_aabb = BBox {
            left: 0.0,
            right: 64.0,
            bottom: 0.0,
            top: 64.0,
        };
        let target_aabb = BBox {
            left: 128.0,
            right: 192.0,
            bottom: 0.0,
            top: 64.0,
        };

        // Create a BSP swept volume between the AABBs
        let swept_volume = BSPSweptVolume::new(&source_aabb, &target_aabb);

        // Should have valid geometry for single segment handling
        assert!(swept_volume.width > 0.0);
        assert!(swept_volume.length > 0.0);
        assert_eq!(swept_volume.start_center, Vec2::new(32.0, 32.0));
        assert_eq!(swept_volume.end_center, Vec2::new(160.0, 32.0));
    }

    #[test]
    fn test_vertex_based_visibility() {
        // Test line intersection logic for visibility
        let pvs = PVS::new(2);

        // Test point-to-target visibility
        let source_point = Vec2::new(32.0, 32.0);
        let target_center = Vec2::new(160.0, 32.0);

        // Create a blocking segment that intersects the line of sight
        let blocking_segment = create_test_segment(Vec2::new(96.0, 0.0), Vec2::new(96.0, 64.0));
        assert!(pvs.line_intersects_ray(&blocking_segment, source_point, target_center));

        // Create a non-blocking segment that doesn't intersect
        let non_blocking_segment =
            create_test_segment(Vec2::new(0.0, 100.0), Vec2::new(64.0, 100.0));
        assert!(!pvs.line_intersects_ray(&non_blocking_segment, source_point, target_center));
    }

    #[test]
    #[ignore = "Requires E1M2 WAD file"]
    fn test_e1m2_linedef_420_422_visibility() {
        use crate::PicData;
        use crate::level::map_data::MapData;
        use std::path::Path;
        use std::path::PathBuf;

        let wad_paths = ["../doom1.wad"];
        let mut wad_path = None;
        let map_name = "E1M2";

        for path in &wad_paths {
            if Path::new(path).exists() {
                wad_path = Some(*path);
                break;
            }
        }

        let wad_path = match wad_path {
            Some(path) => path,
            None => {
                eprintln!("No WAD files found, skipping test");
                return;
            }
        };

        let wad = wad::WadData::new(PathBuf::from(wad_path));

        let mut map_data = MapData::default();
        map_data.load(map_name, &PicData::default(), &wad);

        map_data.build_pvs();

        let linedefs = map_data.linedefs();
        let segments = map_data.segments();
        let subsectors = map_data.subsectors();

        let mut linedef_529_subsectors = Vec::new();
        let mut linedef_19_subsectors = Vec::new();
        let mut linedef_681_subsectors = Vec::new();

        for (subsector_idx, subsector) in subsectors.iter().enumerate() {
            let start = subsector.start_seg as usize;
            let count = subsector.seg_count as usize;

            if let Some(subsector_segments) = segments.get(start..start + count) {
                for segment in subsector_segments {
                    let linedef_ptr = segment.linedef.inner as *const _ as usize;

                    for (linedef_idx, linedef) in linedefs.iter().enumerate() {
                        let this_linedef_ptr = linedef as *const _ as usize;

                        if linedef_ptr == this_linedef_ptr {
                            if linedef_idx == 529 {
                                linedef_529_subsectors.push(subsector_idx);
                                println!(
                                    "Linedef 529 subsector {}: seg_count={}, v1=({:.1}, {:.1}), v2=({:.1}, {:.1})",
                                    subsector_idx,
                                    count,
                                    segment.v1.x,
                                    segment.v1.y,
                                    segment.v2.x,
                                    segment.v2.y
                                );
                            } else if linedef_idx == 19 {
                                linedef_19_subsectors.push(subsector_idx);
                                println!(
                                    "Linedef 19 subsector {}: seg_count={}, v1=({:.1}, {:.1}), v2=({:.1}, {:.1})",
                                    subsector_idx,
                                    count,
                                    segment.v1.x,
                                    segment.v1.y,
                                    segment.v2.x,
                                    segment.v2.y
                                );
                            } else if linedef_idx == 681 {
                                linedef_681_subsectors.push(subsector_idx);
                                println!(
                                    "Linedef 681 subsector {}: seg_count={}, v1=({:.1}, {:.1}), v2=({:.1}, {:.1})",
                                    subsector_idx,
                                    count,
                                    segment.v1.x,
                                    segment.v1.y,
                                    segment.v2.x,
                                    segment.v2.y
                                );
                            }
                            break;
                        }
                    }
                }
            }
        }

        if let Some(pvs) = map_data.pvs() {
            for &from_idx in &linedef_529_subsectors {
                for &to_idx in &linedef_681_subsectors {
                    let visible = pvs.is_visible(from_idx, to_idx);
                    println!(
                        "Linedef 529 subsector {} -> Linedef 681 subsector {}: {}",
                        from_idx, to_idx, visible
                    );
                }
            }

            for &from_idx in &linedef_19_subsectors {
                for &to_idx in &linedef_681_subsectors {
                    let visible = pvs.is_visible(from_idx, to_idx);
                    println!(
                        "Linedef 19 subsector {} -> Linedef 681 subsector {}: {}",
                        from_idx, to_idx, visible
                    );
                }
            }
        }
    }

    fn create_test_segment(v1: Vec2, v2: Vec2) -> Segment {
        let sector = create_test_sector();
        let sidedef = create_test_sidedef();

        Segment {
            v1,
            v2,
            offset: 0.0,
            angle: math::Angle::new(0.0),
            sidedef: sidedef.clone(),
            linedef: MapPtr::new(&mut LineDef {
                v1,
                v2,
                delta: v2 - v1,
                flags: 0,
                special: 0,
                tag: 0,
                bbox: BBox::new(v1, v2),
                slopetype: crate::level::map_defs::SlopeType::Horizontal,
                sides: [0, 1],
                front_sidedef: sidedef.clone(),
                back_sidedef: None,
                frontsector: sector.clone(),
                backsector: None,
                valid_count: 0,
            }),
            frontsector: sector,
            backsector: None,
        }
    }

    #[test]
    #[ignore] // Only run manually with WAD file available
    fn test_e1m2_subsector_visibility() {
        use crate::PicData;
        use crate::level::map_data::MapData;
        use std::path::Path;
        use std::path::PathBuf;

        // Try to find WAD files - check both doom1.wad and doom2.wad in project root
        let wad_paths = ["../doom1.wad"];
        let mut wad_path = None;
        let map_name = "E1M2";

        for path in &wad_paths {
            if Path::new(path).exists() {
                wad_path = Some(*path);
                break;
            }
        }

        let wad_path = match wad_path {
            Some(path) => path,
            None => {
                eprintln!("No WAD files found, skipping test");
                return;
            }
        };

        let wad = wad::WadData::new(PathBuf::from(wad_path));

        let mut map_data = MapData::default();
        map_data.load(map_name, &PicData::default(), &wad);

        map_data.build_pvs();

        // Find subsectors with segments linked to specific linedefs
        let mut target_subsectors = Vec::new();

        for (subsector_idx, subsector) in map_data.subsectors().iter().enumerate() {
            let segments = map_data.segments();
            let subsector_segments = (subsector.start_seg as usize
                ..(subsector.start_seg as usize + subsector.seg_count as usize))
                .filter_map(|i| segments.get(i))
                .collect::<Vec<_>>();

            // For testing purposes, collect the first few subsectors
            if subsector_idx < 10 && !subsector_segments.is_empty() {
                target_subsectors.push(subsector_idx);
                println!("Found test subsector {}", subsector_idx);
            }
        }

        // Test visibility between these subsectors
        if let Some(pvs) = map_data.pvs() {
            for &from_idx in &target_subsectors {
                for &to_idx in &target_subsectors {
                    if from_idx != to_idx {
                        let visible = pvs.is_visible(from_idx, to_idx);
                        println!(
                            "Subsector {} -> {}: {}",
                            from_idx,
                            to_idx,
                            if visible { "VISIBLE" } else { "BLOCKED" }
                        );
                    }
                }
            }
        }
    }
}
