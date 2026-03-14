//! This was more of a curiousity based on the 2D segment occluding
//! standard Doom does. It does not translate well to 3D when the player
//! view can tilt up or down. It was a fun experiment however.

use gameplay::{Angle, BSP3D, OcclusionSeg, Sector, is_subsector, subsector_index};
use glam::{Vec2, Vec3};
use std::f32::consts::{FRAC_PI_2, PI};

use crate::Software3D;

const MAX_SEGS: usize = 128;
const MAX_FRAGMENTS: usize = 32;

#[derive(Clone, Copy)]
struct ClipRange {
    first: f32,
    last: f32,
}

/// Screen-space occlusion buffer combining horizontal solidsegs with per-column
/// vertical ceiling/floor clips. One-sided segs fully block columns. Two-sided
/// segs with height differences create a window: the upper/lower walls clip
/// vertically, and only the portal opening remains visible for geometry behind.
pub struct SegOccluder {
    segs: [ClipRange; MAX_SEGS],
    new_end: usize,
    /// Per-column: topmost occluded row (-1 = nothing clipped from top).
    ceiling_clip: Vec<f32>,
    /// Per-column: bottommost occluded row (screen_height = nothing clipped).
    floor_clip: Vec<f32>,
    screen_width: f32,
    screen_height: f32,
    half_screen_width: f32,
    half_screen_height: f32,
    fov_half: f32,
    fov_half_tan: f32,
    /// Focal length for column projection: half_screen_width / fov_half_tan.
    projection: f32,
    wide_ratio: f32,
}

impl SegOccluder {
    pub fn new(fov: f32, screen_width: f32, screen_height: f32) -> Self {
        // Match the 3D renderer's horizontal FOV:
        // perspective uses vfov = fov * 0.75, then CRT stretch on Y (240/200)
        let aspect = screen_width / screen_height;
        let vfov_half = fov * 0.75 / 2.0;
        let vfov_half_tan = vfov_half.tan();
        let h_fov = 2.0 * (aspect * vfov_half_tan).atan();
        let fov_half = h_fov / 2.0;
        let half_screen_width = screen_width / 2.0;
        let fov_half_tan = Angle::new(fov_half).tan();
        let projection = half_screen_width / fov_half_tan;
        let wide_ratio = screen_height / screen_width * 1.6;
        let w = screen_width as usize;
        let mut s = Self {
            segs: [ClipRange {
                first: 0.0,
                last: 0.0,
            }; MAX_SEGS],
            new_end: 0,
            ceiling_clip: vec![-1.0; w],
            floor_clip: vec![screen_height; w],
            screen_width,
            screen_height,
            half_screen_width,
            half_screen_height: screen_height / 2.0,
            fov_half,
            fov_half_tan,
            projection,
            wide_ratio,
        };
        s.clear();
        s
    }

    pub fn clear(&mut self) {
        for s in self.segs.iter_mut() {
            s.first = self.screen_width;
            s.last = f32::MAX;
        }
        self.segs[0].first = f32::MAX;
        self.segs[0].last = f32::MIN;
        self.new_end = 1;

        self.ceiling_clip.fill(-1.0);
        self.floor_clip.fill(self.screen_height);
    }

    /// Returns true if any column in the given range has vertical opening
    /// remaining.
    fn has_vertical_opening(&self, col_start: usize, col_end: usize) -> bool {
        let end = col_end.min(self.ceiling_clip.len() - 1);
        for col in col_start..=end {
            if self.ceiling_clip[col] < self.floor_clip[col] {
                return true;
            }
        }
        false
    }

    /// Check if a 2D bounding box has any visible (unoccluded) screen columns.
    /// Ports R_CheckBBox from 2.5D: picks the visible edge corners based on
    /// player position, projects to screen range, then checks against
    /// solidsegs.
    pub fn is_bbox_visible(
        &self,
        bb_min: Vec2,
        bb_max: Vec2,
        player_pos: Vec2,
        player_angle: Angle,
    ) -> bool {
        let boxx = if player_pos.x <= bb_min.x {
            0
        } else if player_pos.x < bb_max.x {
            1
        } else {
            2
        };
        let boxy = if player_pos.y >= bb_max.y {
            0
        } else if player_pos.y > bb_min.y {
            1
        } else {
            2
        };

        let boxpos = (boxy << 2) + boxx;
        // Player is inside the box
        if boxpos == 5 {
            return true;
        }

        let (v1, v2) = match boxpos {
            0 => (Vec2::new(bb_max.x, bb_max.y), Vec2::new(bb_min.x, bb_min.y)),
            1 => (Vec2::new(bb_max.x, bb_max.y), Vec2::new(bb_min.x, bb_max.y)),
            2 => (Vec2::new(bb_max.x, bb_min.y), Vec2::new(bb_min.x, bb_max.y)),
            4 => (Vec2::new(bb_min.x, bb_max.y), Vec2::new(bb_min.x, bb_min.y)),
            6 => (Vec2::new(bb_max.x, bb_min.y), Vec2::new(bb_max.x, bb_max.y)),
            8 => (Vec2::new(bb_min.x, bb_max.y), Vec2::new(bb_max.x, bb_min.y)),
            9 => (Vec2::new(bb_min.x, bb_min.y), Vec2::new(bb_max.x, bb_min.y)),
            10 => (Vec2::new(bb_min.x, bb_min.y), Vec2::new(bb_max.x, bb_max.y)),
            _ => return true,
        };

        let clipangle = Angle::new(self.fov_half);
        let clipangrad = clipangle.rad();

        let mut angle1 = vertex_angle(v1, player_pos);
        let mut angle2 = vertex_angle(v2, player_pos);

        let span = angle1 - angle2;
        if span.rad() >= FRAC_PI_2 {
            // AABB spans > 90° — skip column-range projection but still
            // reject if the entire screen is vertically closed.
            return self.has_vertical_opening(0, self.ceiling_clip.len().saturating_sub(1));
        }

        angle1 -= player_angle;
        angle2 -= player_angle;

        let mut tspan = angle1 + clipangle;
        if tspan.rad() >= clipangrad * 2.0 {
            tspan -= 2.0 * clipangrad;
            if tspan.rad() >= span.rad() {
                return false;
            }
            angle1 = clipangle;
        }
        tspan = clipangle - angle2;
        if tspan.rad() >= 2.0 * clipangrad {
            tspan -= 2.0 * clipangrad;
            if tspan.rad() >= span.rad() {
                return false;
            }
            angle2 = Angle::default() - clipangle;
        }

        let x1 = angle_to_screen(
            self.projection,
            self.fov_half_tan,
            self.half_screen_width,
            self.screen_width,
            angle1,
        );
        let mut x2 = angle_to_screen(
            self.projection,
            self.fov_half_tan,
            self.half_screen_width,
            self.screen_width,
            angle2,
        );

        if x1 == x2 {
            return false;
        }
        x2 -= 1.0;

        // Check if the projected range is fully covered by a single solidsegs
        // entry. Use a 1-column margin to guard against float rounding in
        // angle_to_screen causing a solidsegs entry to be slightly too wide.
        let mut start = 0;
        while self.segs[start].last < x2 {
            start += 1;
        }
        if x1 >= self.segs[start].first + 1.0 && x2 <= self.segs[start].last - 1.0 {
            return false;
        }

        // Also reject if every column in the projected range is vertically
        // closed by portal clips. This catches cases where solidsegs haven't
        // filled yet but all ceiling/floor clips have converged — common in
        // open maps where portals close columns before walls do.
        let cs = (x1.max(0.0) as usize).min(self.ceiling_clip.len().saturating_sub(1));
        let ce = (x2.min(self.screen_width - 1.0) as usize)
            .min(self.ceiling_clip.len().saturating_sub(1));
        if !self.has_vertical_opening(cs, ce) {
            return false;
        }

        true
    }

    /// Returns true when the entire screen width is occluded.
    pub fn all_solid(&self) -> bool {
        self.new_end == 1
            && self.segs[0].first <= 0.0
            && self.segs[0].last >= self.screen_width - 1.0
    }

    /// Project a world-space segment to screen-column range.
    /// Returns None if segment is behind player or fully outside FOV.
    pub fn project_seg(
        &self,
        v1: Vec2,
        v2: Vec2,
        player_pos: Vec2,
        player_angle: Angle,
    ) -> Option<(f32, f32)> {
        let mut angle1 = vertex_angle(v1, player_pos);
        let mut angle2 = vertex_angle(v2, player_pos);

        let span = (angle1 - angle2).rad();
        if span.abs() > PI {
            return None;
        }

        angle1 -= player_angle;
        angle2 -= player_angle;

        let clipangle = Angle::new(self.fov_half);

        let mut tspan = angle1 + clipangle;
        if tspan.rad() > 2.0 * clipangle.rad() {
            tspan -= 2.0 * clipangle.rad();
            if tspan.rad() >= span {
                return None;
            }
            angle1 = clipangle;
        }

        tspan = clipangle - angle2;
        if tspan.rad() > 2.0 * clipangle.rad() {
            tspan -= 2.0 * clipangle.rad();
            if tspan.rad() >= span {
                return None;
            }
            angle2 = Angle::default() - clipangle;
        }

        let x1 = angle_to_screen(
            self.projection,
            self.fov_half_tan,
            self.half_screen_width,
            self.screen_width,
            angle1,
        );
        let x2 = angle_to_screen(
            self.projection,
            self.fov_half_tan,
            self.half_screen_width,
            self.screen_width,
            angle2,
        );

        if x1 == x2 {
            return None;
        }

        Some((x1, x2 - 1.0))
    }

    /// Process an occlusion seg: projects it, classifies as solid or portal,
    /// and updates the occlusion buffer accordingly. Uses live sector heights.
    /// Returns true if any fragment of the seg was visible (not already
    /// occluded).
    pub fn process_seg(
        &mut self,
        occ_seg: &OcclusionSeg,
        sectors: &[Sector],
        player_pos: Vec2,
        player_angle: Angle,
        view_z: f32,
    ) -> bool {
        let Some((x1, x2)) = self.project_seg(occ_seg.v1, occ_seg.v2, player_pos, player_angle)
        else {
            return false;
        };

        // Determine if solid or portal, collect visible fragments from solidsegs
        let is_solid;
        let back_sector;
        match occ_seg.back_sector_id {
            None => {
                is_solid = true;
                back_sector = None;
            }
            Some(back_id) => {
                let front = &sectors[occ_seg.front_sector_id];
                let back = &sectors[back_id];
                if back.ceilingheight <= front.floorheight
                    || back.floorheight >= front.ceilingheight
                {
                    is_solid = true;
                } else {
                    is_solid = false;
                }
                back_sector = Some(back);
            }
        }

        // Collect visible fragments — only these columns get vertical clip updates.
        // This matches 2.5D where store_wall_range is only called for visible gaps.
        let mut fragments = [(0.0f32, 0.0f32); MAX_FRAGMENTS];
        let frag_count = if is_solid {
            self.clip_solid_fragments(x1, x2, &mut fragments)
        } else {
            self.clip_portal_fragments(x1, x2, &mut fragments)
        };

        // Check visibility: any fragment must exist AND have vertical opening
        let mut any_visible = false;
        for i in 0..frag_count {
            let (f_start, f_end) = fragments[i];
            let cs = (f_start.max(0.0) as usize).min(self.ceiling_clip.len() - 1);
            let ce = (f_end.min(self.screen_width - 1.0) as usize).min(self.ceiling_clip.len() - 1);
            if self.has_vertical_opening(cs, ce) {
                any_visible = true;
                break;
            }
        }

        if is_solid {
            // Close vertical clips only for visible fragment columns
            for i in 0..frag_count {
                let (f_start, f_end) = fragments[i];
                let cs = (f_start.max(0.0) as usize).min(self.ceiling_clip.len() - 1);
                let ce =
                    (f_end.min(self.screen_width - 1.0) as usize).min(self.ceiling_clip.len() - 1);
                for col in cs..=ce {
                    self.ceiling_clip[col] = self.screen_height;
                    self.floor_clip[col] = -1.0;
                }
            }
        } else if let Some(back) = back_sector {
            let front = &sectors[occ_seg.front_sector_id];
            let opening_ceil = front.ceilingheight.min(back.ceilingheight);
            let opening_floor = front.floorheight.max(back.floorheight);

            // Compute wall normal and perpendicular distance
            let seg_angle = Angle::new(occ_seg.seg_angle_rad);
            let rw_normalangle = Angle::new(occ_seg.seg_angle_rad + FRAC_PI_2);

            let hyp = point_to_dist(occ_seg.v1, player_pos);
            let dist_angle = vertex_angle(occ_seg.v1, player_pos) - seg_angle;
            let rw_distance = (hyp * dist_angle.sin()).abs();

            // Compute scale at the projected endpoints
            let vis1 = Angle::new(
                player_angle.rad() + screen_to_angle(x1, self.half_screen_width, self.projection),
            );
            let vis2 = Angle::new(
                player_angle.rad() + screen_to_angle(x2, self.half_screen_width, self.projection),
            );
            let scale1 = scale_from_view_angle(
                vis1,
                rw_normalangle,
                rw_distance,
                player_angle,
                self.half_screen_width,
            ) * self.wide_ratio;
            let scale2 = scale_from_view_angle(
                vis2,
                rw_normalangle,
                rw_distance,
                player_angle,
                self.half_screen_width,
            ) * self.wide_ratio;

            let col_span = x2 - x1;
            let scale_step = if col_span > 0.0 {
                (scale2 - scale1) / col_span
            } else {
                0.0
            };

            let half_h = self.half_screen_height;
            let world_high = opening_ceil - view_z;
            let world_low = opening_floor - view_z;

            // Update vertical clips only for visible fragment columns
            for i in 0..frag_count {
                let (f_start, f_end) = fragments[i];
                let cs = (f_start.max(0.0) as usize).min(self.ceiling_clip.len() - 1);
                let ce =
                    (f_end.min(self.screen_width - 1.0) as usize).min(self.ceiling_clip.len() - 1);
                let mut scale = scale1 + scale_step * (cs as f32 - x1);
                for col in cs..=ce {
                    let ceil_y = half_h - (world_high * scale);
                    if ceil_y > self.ceiling_clip[col] {
                        self.ceiling_clip[col] = ceil_y;
                    }
                    let floor_y = half_h - (world_low * scale);
                    if floor_y < self.floor_clip[col] {
                        self.floor_clip[col] = floor_y;
                    }
                    scale += scale_step;
                }
            }
        }

        any_visible
    }

    /// Mark columns [first,last] as solid. Records visible fragments into
    /// `out`. Returns the number of fragments written.
    fn clip_solid_fragments(
        &mut self,
        first: f32,
        last: f32,
        out: &mut [(f32, f32); MAX_FRAGMENTS],
    ) -> usize {
        let mut frag_count = 0;
        let mut next;

        let mut start = 0;
        while self.segs[start].last < first - 1.0 {
            start += 1;
        }

        if first < self.segs[start].first {
            if last < self.segs[start].first - 1.0 {
                // Entirely visible — insert new range
                if frag_count < MAX_FRAGMENTS {
                    out[frag_count] = (first, last);
                    frag_count += 1;
                }

                next = self.new_end;
                self.new_end += 1;

                while next != start {
                    self.segs[next] = self.segs[next - 1];
                    next -= 1;
                }

                self.segs[next].first = first;
                self.segs[next].last = last;
                return frag_count;
            }

            // Fragment above start is visible
            if frag_count < MAX_FRAGMENTS {
                out[frag_count] = (first, self.segs[start].first - 1.0);
                frag_count += 1;
            }
            self.segs[start].first = first;
        }

        if last <= self.segs[start].last {
            return frag_count;
        }

        next = start;
        while last >= self.segs[next + 1].first - 1.0 {
            // Gap between solidsegs is visible
            if frag_count < MAX_FRAGMENTS {
                out[frag_count] = (self.segs[next].last + 1.0, self.segs[next + 1].first - 1.0);
                frag_count += 1;
            }
            next += 1;

            if last <= self.segs[next].last {
                self.segs[start].last = self.segs[next].last;
                self.crunch(start, next);
                return frag_count;
            }
        }

        // Fragment after last merged range is visible
        if frag_count < MAX_FRAGMENTS {
            out[frag_count] = (self.segs[next].last + 1.0, last);
            frag_count += 1;
        }
        self.segs[start].last = last;
        self.crunch(start, next);
        frag_count
    }

    /// Find visible fragments of [first,last] without updating solidsegs.
    /// Returns the number of fragments written.
    fn clip_portal_fragments(
        &self,
        first: f32,
        last: f32,
        out: &mut [(f32, f32); MAX_FRAGMENTS],
    ) -> usize {
        let mut frag_count = 0;
        let mut start = 0;
        while self.segs[start].last < first - 1.0 {
            start += 1;
        }

        // Fragment before first solidsegs entry
        if first < self.segs[start].first {
            let frag_end = last.min(self.segs[start].first - 1.0);
            if frag_count < MAX_FRAGMENTS {
                out[frag_count] = (first, frag_end);
                frag_count += 1;
            }
            if last <= self.segs[start].first - 1.0 {
                return frag_count;
            }
        }

        if last <= self.segs[start].last {
            return frag_count;
        }

        // Gaps between solidsegs entries
        while last >= self.segs[start + 1].first - 1.0 {
            let gap_start = self.segs[start].last + 1.0;
            let gap_end = self.segs[start + 1].first - 1.0;
            if gap_start <= gap_end && frag_count < MAX_FRAGMENTS {
                out[frag_count] = (gap_start, gap_end);
                frag_count += 1;
            }
            start += 1;
            if last <= self.segs[start].last {
                return frag_count;
            }
        }

        // Fragment after last overlapping solidsegs entry
        let gap_start = self.segs[start].last + 1.0;
        if gap_start <= last && frag_count < MAX_FRAGMENTS {
            out[frag_count] = (gap_start, last);
            frag_count += 1;
        }
        frag_count
    }

    fn crunch(&mut self, mut start: usize, mut next: usize) {
        if next == start {
            return;
        }

        while next != self.new_end && start < self.segs.len() - 1 {
            next += 1;
            start += 1;
            self.segs[start] = self.segs[next];
        }
        self.new_end = start + 1;
    }
}

fn vertex_angle(vertex: Vec2, pos: Vec2) -> Angle {
    let x = vertex.x - pos.x;
    let y = vertex.y - pos.y;
    Angle::new(y.atan2(x))
}

fn point_to_dist(point: Vec2, pos: Vec2) -> f32 {
    let mut dx = (point.x - pos.x).abs();
    let mut dy = (point.y - pos.y).abs();
    if dy > dx {
        std::mem::swap(&mut dx, &mut dy);
    }
    (dx * dx + dy * dy).sqrt()
}

fn screen_to_angle(x: f32, half_screen_width: f32, projection: f32) -> f32 {
    ((half_screen_width - x) / projection).atan()
}

/// R_ScaleFromGlobalAngle — compute per-column scale from wall distance
fn scale_from_view_angle(
    visangle: Angle,
    rw_normalangle: Angle,
    rw_distance: f32,
    view_angle: Angle,
    half_screen_width: f32,
) -> f32 {
    let anglea = Angle::new(FRAC_PI_2 + (visangle.sub_other(view_angle)).rad());
    let angleb = Angle::new(FRAC_PI_2 + (visangle.sub_other(rw_normalangle)).rad());
    let projection = half_screen_width;
    let num = projection * angleb.sin();
    let den = rw_distance * anglea.sin();

    const MIN_DEN: f32 = 0.0001;
    if den.abs() < MIN_DEN {
        if num > 0.0 { 64.0 } else { -64.0 }
    } else {
        (num / den).clamp(-180.0, 180.0)
    }
}

fn angle_to_screen(
    focal_len: f32,
    fov_half_tan: f32,
    half_screen_width: f32,
    screen_width: f32,
    angle: Angle,
) -> f32 {
    let limit = fov_half_tan;
    let tan_angle = angle.tan();

    if tan_angle > limit {
        -1.0
    } else if tan_angle < -limit {
        screen_width + 1.0
    } else {
        let t = tan_angle * focal_len;
        let t = half_screen_width - t + 0.99998474;
        t.floor().clamp(-1.0, screen_width + 1.0)
    }
}

impl Software3D {
    /// Collect visible polygons using BSP front-to-back traversal with
    /// screen-space segment occlusion (solidsegs). Walks the Node3D tree
    /// and uses OcclusionSeg data stored on each BSPLeaf3D to determine
    /// which subsectors have visible screen columns.
    pub(crate) fn collect_bsp_clipped_polygons(
        &mut self,
        node_id: u32,
        bsp3d: &BSP3D,
        sectors: &[Sector],
        player_pos: Vec3,
        player_angle: Angle,
    ) {
        if self.seg_occluder.all_solid() {
            return;
        }

        if is_subsector(node_id) {
            let subsector_id = if node_id == u32::MAX {
                0
            } else {
                subsector_index(node_id)
            };

            let Some(leaf) = bsp3d.get_subsector_leaf(subsector_id) else {
                return;
            };

            if self.is_bbox_outside_fov(&leaf.aabb) {
                return;
            }

            // Update the occlusion buffer with this leaf's segments.
            let player_pos_2d = Vec2::new(player_pos.x, player_pos.y);
            let view_z = player_pos.z;

            let mut seg_visible = false;
            for occ_seg in &leaf.occlusion_segs {
                if self.seg_occluder.all_solid() {
                    break;
                }
                if self.seg_occluder.process_seg(
                    occ_seg,
                    sectors,
                    player_pos_2d,
                    player_angle,
                    view_z,
                ) {
                    seg_visible = true;
                }
            }

            // Skip leaf if no seg fragment was visible — check 2D bbox, and
            // when the occluder is saturated (very steep pitch), also accept
            // via the 3D frustum test to avoid missing wing geometry.
            if !seg_visible && !leaf.occlusion_segs.is_empty() {
                let bb_min = Vec2::new(leaf.aabb.min.x, leaf.aabb.min.y);
                let bb_max = Vec2::new(leaf.aabb.max.x, leaf.aabb.max.y);
                if !self
                    .seg_occluder
                    .is_bbox_visible(bb_min, bb_max, player_pos_2d, player_angle)
                {
                    return;
                }
                self.stats.bsp_fallback += 1;
            }

            // Submit all polygons — leaf has visible seg fragments
            for poly_surface in &leaf.polygons {
                let sid = poly_surface.sector_id;
                if !self.seen_sectors[sid] {
                    self.seen_sectors[sid] = true;
                    self.visible_sectors
                        .push((sid, sectors[sid].lightlevel >> 4));
                }
                if poly_surface.is_facing_point(player_pos, &bsp3d.vertices) {
                    if let Some(depth) = self.cull_polygon_bounds(poly_surface, bsp3d) {
                        self.visible_polygons
                            .push((poly_surface as *const _, depth));
                    }
                }
            }
            return;
        }

        // Internal node
        let Some(node) = bsp3d.nodes().get(node_id as usize) else {
            return;
        };

        let (front, back) = node.front_back_children(Vec2::new(player_pos.x, player_pos.y));

        // Front side first (closer to player) — None = leaf node, always enter.
        if bsp3d
            .get_node_aabb(front)
            .map_or(true, |aabb| !self.is_bbox_outside_fov(aabb))
        {
            self.collect_bsp_clipped_polygons(front, bsp3d, sectors, player_pos, player_angle);
        }

        // Back side — skip if screen fully occluded or child bbox outside frustum
        let back_visible = bsp3d
            .get_node_aabb(back)
            .map_or(false, |aabb| !self.is_bbox_outside_fov(aabb));
        if !self.seg_occluder.all_solid() && back_visible {
            self.collect_bsp_clipped_polygons(back, bsp3d, sectors, player_pos, player_angle);
        }
    }
}
