use super::RenderData;
use super::defs::ClipRange;
use super::segs::SegRender;
use super::things::VisSprite;
#[cfg(feature = "hprof")]
use coarse_prof::profile;
use glam::Vec2;
use level::{LevelData, Sector, Segment, SubSector, is_subsector, subsector_index};
use log::trace;
use math::{ANG90, ANG180, ANGLETOFINESHIFT, Angle, Bam, FixedT};
use pic_data::PicData;
use render_common::{DrawBuffer, RenderView};
use std::mem;

const MAX_SEGS: usize = 128;
const MAX_SECTS: usize = 4096;
const MAX_VIS_SPRITES: usize = 1024;
const FINEANGLES: usize = 8192;

// Need to sort out what is shared and what is not so that a data struct
// can be organised along with method/ownsership
//
// seg_t *curline; // SHARED, PASS AS AN ARG to segs.c functions
//
// side_t *sidedef; // don't use..., get from curline/seg
//
// line_t *linedef; // In maputils as an arg to P_LineOpening, not global
//
// These can be chased through the chain of:
// seg.linedef.front_sidedef.sector.floorheight
// This block as a struct to pass round?
//
// sector_t *frontsector; // Shared in seg/bsp . c, in segs StoreWallRange +
// sector_t *backsector;

/// We store most of what is needed for rendering in various functions here to
/// avoid having to pass too many things in args through multiple function
/// calls. This is due to the Doom C relying a fair bit on global state.
///
/// `RenderData` will be passed to the sprite drawer/clipper to use `drawsegs`
///
/// ----------------------------------------------------------------------------
///
/// - R_DrawSprite, r_things.c
/// - R_DrawMasked, r_things.c
/// - R_StoreWallRange, r_segs.c, checks only for overflow of drawsegs, and uses
///   *one* entry through ds_p it then inserts/incs pointer to next drawseg in
///   the array when finished
/// - R_DrawPlanes, r_plane.c, checks only for overflow of drawsegs
pub struct Software25D {
    /// index in to self.solidsegs
    new_end: usize,
    solidsegs: [ClipRange; MAX_SEGS],
    /// Visible sprite data, used for Z-ordered rendering of sprites
    pub(super) vissprites: [VisSprite; MAX_VIS_SPRITES],
    /// The next `VisSprite`, incremented during the filling in of `VisSprites`
    pub(super) next_vissprite: usize,

    pub(super) r_data: RenderData,
    pub(super) seg_renderer: SegRender,
    pub(super) _debug: bool,

    /// Used for checking if a sector has been worked on when iterating over
    pub(super) checked_sectors: [i32; MAX_SECTS],
    pub(super) checked_idx: usize,

    /// Mostly used in thing drawing only
    pub y_scale: FixedT,
    /// = half_width. Used for sprite/seg scale comparison.
    pub projection: FixedT,
    /// = focal_length. Used for sprite X positioning (matches viewangletox
    /// LUT).
    pub focal_length: FixedT,

    pub fov_half_bam: u32,

    pub buf_width: usize,
    pub buf_height: usize,

    /// OG Doom `viewangletox` — maps fine angle index to screen X column.
    pub viewangletox: Vec<i32>,
    pub(super) fuzz_pos: usize,
}

impl Software25D {
    /// Main entry point for 2.5D rendering a single frame.
    ///
    /// - Clears vissprites, clip segs, and render data
    /// - Configures light scaling and view pitch
    /// - Runs BSP traversal to emit walls and collect sprites
    /// - Draws masked/translucent elements (sprites, mid-textures)
    ///   back-to-front
    pub fn draw_view(
        &mut self,
        view: &RenderView,
        level_data: &LevelData,
        pic_data: &mut PicData,
        rend: &mut impl DrawBuffer,
    ) {
        // TODO: pull duplicate functionality out to a function
        self.clear(FixedT::from(rend.size().width() as i32));
        let mut count = 0;
        // TODO: netupdate

        pic_data.set_fixed_lightscale(view.fixedcolormap);

        self.seg_renderer.clear();
        unsafe {
            let max_offset = if view.lookdir >= 0.0 {
                game_config::tic_cmd::LOOKDIRMIN
            } else {
                game_config::tic_cmd::LOOKDIRMAX
            } as f32;
            let pitch_pixels = (view.lookdir / (std::f32::consts::PI / 2.0) * max_offset) as i16;
            self.seg_renderer
                .set_view_pitch(pitch_pixels, FixedT::from(rend.size().half_view_height()));
        }
        #[cfg(feature = "hprof")]
        profile!("render_bsp_node begin!");
        self.render_bsp_node(
            level_data,
            view,
            level_data.start_node(),
            pic_data,
            rend,
            view.subsector_id,
            &mut count,
        );

        trace!("BSP traversals for render: {count}");

        #[cfg(feature = "debug_seg_clip")]
        self.seg_renderer.draw_debug_clipping(&self.r_data, rend);

        // TODO: netupdate again
        #[cfg(feature = "hprof")]
        profile!("draw_masked");
        self.draw_masked(view, pic_data, rend);
        // TODO: netupdate again
    }

    pub fn new(fov: f32, width: f32, height: f32, debug: bool) -> Software25D {
        let (_, _, focal_length_f) = render_common::og_projection(fov, width, height);
        // OG: projection = centerxfrac = half_width (used for scale comparison)
        let projection = FixedT::from((width / 2.0) as i32);
        // focal_length matches the viewangletox LUT (used for sprite X positioning)
        let focal_length = FixedT::from_f32(focal_length_f);
        let y_scale = FixedT::ONE;

        let width = width as usize;
        let height = height as usize;

        // Build viewangletox LUT at base resolution (200p), then scale up
        // for hi-res. This ensures hi-res columns are exactly N× base columns,
        // preventing rounding divergence that causes sprite clipping artifacts.
        let res_scale = height as i32 / 200;
        let base_width = width as i32 / res_scale;
        let base_centerx = base_width / 2;
        let base_centerxfrac = FixedT::from(base_centerx);

        // Derive fov_half_fine from integer math to avoid float→BAM rounding.
        // OG Doom uses a hardcoded FIELDOFVIEW=2048; we search the tangent table
        // for the fine angle where tan >= centerx / focal_length.
        // focal_length is from og_projection (f32 boundary, computed once).
        let base_focal_len = FixedT::from_f32(focal_length_f / (height as f32 / 200.0));
        let target_tan = base_centerxfrac / base_focal_len;
        let mut fov_half_fine = 0u32;
        for i in 0..FINEANGLES / 4 {
            if math::fine_tan(FINEANGLES / 4 + i) >= target_tan {
                fov_half_fine = i as u32;
                break;
            }
        }
        // OG Doom uses FRACUNIT*2 (2.0) as the tangent limit, not the FOV edge tangent.
        let tan_limit = FixedT::from(2);
        let mut viewangletox = vec![0i32; FINEANGLES / 2];
        for i in 0..FINEANGLES / 2 {
            let tangent = math::fine_tan(i);
            if tangent > tan_limit {
                viewangletox[i] = -1;
            } else if tangent < -tan_limit {
                viewangletox[i] = (base_width + 1) * res_scale;
            } else {
                let t = base_centerxfrac - tangent * base_focal_len;
                let t = (t.0 + math::FRACUNIT - 1) >> math::FRACBITS;
                let base_col = (t as i32).clamp(-1, base_width + 1);
                viewangletox[i] = if base_col < 0 {
                    -1
                } else {
                    base_col * res_scale
                };
            }
        }

        // OG Doom: build xtoviewangle by inverting viewangletox (before fencepost fix)
        // xtoviewangle[x] = smallest fine angle that maps to column x
        let mut xtoviewangle = vec![0u32; width + 1];
        for x in 0..=width {
            let mut i = 0;
            while i < FINEANGLES / 2 && viewangletox[i] > x as i32 {
                i += 1;
            }
            xtoviewangle[x] = ((i as u32) << math::ANGLETOFINESHIFT).wrapping_sub(ANG90);
        }

        // OG Doom fencepost fix
        let w = width as i32;
        for v in viewangletox.iter_mut() {
            if *v == -1 {
                *v = 0;
            } else if *v >= w {
                *v = w;
            }
        }

        Self {
            r_data: RenderData::new(width, height),
            seg_renderer: SegRender::new(width, height, xtoviewangle),
            new_end: 0,
            solidsegs: [ClipRange {
                first: FixedT::ZERO,
                last: FixedT::ZERO,
            }; MAX_SEGS],
            _debug: debug,
            checked_sectors: [-1; MAX_SECTS],
            checked_idx: 0,
            vissprites: [VisSprite::new(); MAX_VIS_SPRITES],
            next_vissprite: 0,
            y_scale,
            projection,
            focal_length,
            // Derive from fov_half_fine (integer) instead of float fov
            fov_half_bam: fov_half_fine << math::ANGLETOFINESHIFT,
            buf_width: width,
            buf_height: height,
            viewangletox,
            fuzz_pos: 0,
        }
    }

    /// Recompute derived values when the 3D view height changes (statusbar
    /// toggle).
    pub fn set_view_height(&mut self, vh: usize) {
        self.seg_renderer.set_view_height(vh);
        self.r_data.set_view_height(vh);
    }

    fn clear(&mut self, screen_width: FixedT) {
        for vis in self.vissprites.iter_mut() {
            *vis = unsafe { mem::zeroed::<VisSprite>() };
        }
        self.next_vissprite = 0;
        self.checked_sectors.copy_from_slice(&[-1; MAX_SECTS]);
        self.checked_idx = 0;

        self.clear_clip_segs(screen_width);
        self.r_data.clear_data();
    }

    /// R_AddLine - r_bsp
    ///
    /// Determines visibility of a segment and dispatches to the appropriate
    /// clipping routine.
    ///
    /// - Computes view-relative angles to both segment endpoints
    /// - Rejects back-facing segments (span >= 180 degrees)
    /// - Clips both angles against the FOV half-angle
    /// - Maps clipped angles to screen columns via `viewangletox`
    /// - Routes to `clip_solid_seg` for one-sided walls and closed doors,
    ///   `clip_portal_seg` for two-sided lines (windows, height changes)
    fn add_line<'a>(
        &'a mut self,
        view: &RenderView,
        seg: &'a Segment,
        front_sector: &'a Sector,
        pic_data: &PicData,
        rend: &mut impl DrawBuffer,
    ) {
        #[cfg(feature = "hprof")]
        profile!("add_line");
        let viewangle = view.angle;

        let mut angle1_bam = math::r_point_to_angle(seg.v1.x_fp - view.x, seg.v1.y_fp - view.y);
        let mut angle2_bam = math::r_point_to_angle(seg.v2.x_fp - view.x, seg.v2.y_fp - view.y);

        let span = angle1_bam.wrapping_sub(angle2_bam);
        if span >= ANG180 {
            return;
        }

        // Global angle needed by segcalc.
        self.r_data.rw_angle1 = Angle::<Bam>::from_bam(angle1_bam);
        let viewangle_bam = viewangle.inner().0;
        angle1_bam = angle1_bam.wrapping_sub(viewangle_bam);
        angle2_bam = angle2_bam.wrapping_sub(viewangle_bam);

        let clipangle_bam = self.fov_half_bam;

        let mut tspan = angle1_bam.wrapping_add(clipangle_bam);
        if tspan > 2u32.wrapping_mul(clipangle_bam) {
            tspan = tspan.wrapping_sub(2u32.wrapping_mul(clipangle_bam));
            if tspan >= span {
                return;
            }
            angle1_bam = clipangle_bam;
        }

        tspan = clipangle_bam.wrapping_sub(angle2_bam);
        if tspan > 2u32.wrapping_mul(clipangle_bam) {
            tspan = tspan.wrapping_sub(2u32.wrapping_mul(clipangle_bam));
            if tspan >= span {
                return;
            }
            angle2_bam = 0u32.wrapping_sub(clipangle_bam);
        }

        let ang1_fine = (angle1_bam.wrapping_add(ANG90) >> ANGLETOFINESHIFT) as usize;
        let ang2_fine = (angle2_bam.wrapping_add(ANG90) >> ANGLETOFINESHIFT) as usize;
        let x1 = FixedT::from(self.viewangletox[ang1_fine & (FINEANGLES / 2 - 1)]);
        let x2 = FixedT::from(self.viewangletox[ang2_fine & (FINEANGLES / 2 - 1)]);

        // Does not cross a pixel?
        if x1 == x2 {
            return;
        }

        if let Some(back_sector) = seg.backsector.clone() {
            // Doors. Block view
            if back_sector.ceilingheight <= front_sector.floorheight
                || back_sector.floorheight >= front_sector.ceilingheight
            {
                self.clip_solid_seg(x1, x2 - 1, seg, view, pic_data, rend);
                return;
            }

            // Windows usually, but also changes in heights from sectors eg: steps
            if back_sector.ceilingheight != front_sector.ceilingheight
                || back_sector.floorheight != front_sector.floorheight
            {
                self.clip_portal_seg(x1, x2 - 1, seg, view, pic_data, rend);
                return;
            }

            // Reject empty lines used for triggers and special events.
            // Identical floor and ceiling on both sides, identical light levels
            // on both sides, and no middle texture.
            if back_sector.ceilingpic == front_sector.ceilingpic
                && back_sector.floorpic == front_sector.floorpic
                && back_sector.lightlevel == front_sector.lightlevel
                && seg.sidedef.midtexture.is_none()
            {
                return;
            }

            self.clip_portal_seg(x1, x2 - 1, seg, view, pic_data, rend);
            return;
        }
        self.clip_solid_seg(x1, x2 - 1, seg, view, pic_data, rend);
    }

    /// R_Subsector - r_bsp
    fn draw_subsector(
        &mut self,
        map: &LevelData,
        view: &RenderView,
        subsect: &SubSector,
        pic_data: &PicData,
        rend: &mut impl DrawBuffer,
    ) {
        #[cfg(feature = "hprof")]
        profile!("draw_subsector");
        let front_sector = &subsect.sector;

        self.add_sprites(view, front_sector, rend.size().width() as u32, pic_data);

        for i in subsect.start_seg..subsect.start_seg + subsect.seg_count {
            let seg = &map.segments()[i as usize];
            self.add_line(view, seg, front_sector, pic_data, rend);
        }
    }

    /// R_ClearClipSegs - r_bsp
    fn clear_clip_segs(&mut self, screen_width: FixedT) {
        for s in self.solidsegs.iter_mut() {
            s.first = screen_width;
            s.last = FixedT::MAX;
        }
        self.solidsegs[0].first = FixedT::MIN;
        self.solidsegs[0].last = FixedT::from(-1);
        self.solidsegs[1].first = screen_width;
        self.solidsegs[1].last = FixedT::MAX;
        self.new_end = 2;
    }

    /// R_ClipSolidWallSegment - r_bsp
    ///
    /// Clips a solid (one-sided) wall segment against the `solidsegs` occlusion
    /// list, renders visible column spans via `store_wall_range`, and inserts
    /// the segment into the occlusion list so nothing behind it can be
    /// drawn.
    ///
    /// - Finds the first clip range overlapping `[first, last]`
    /// - Renders any uncovered fragments between existing ranges
    /// - Merges the new range into `solidsegs`, crunching overlapped entries
    fn clip_solid_seg(
        &mut self,
        first: FixedT,
        last: FixedT,
        seg: &Segment,
        view: &RenderView,
        pic_data: &PicData,
        rend: &mut impl DrawBuffer,
    ) {
        let mut next;

        // Find the first range that touches the range
        //  (adjacent pixels are touching).
        let mut start = 0; // first index
        while self.solidsegs[start].last < first - 1 {
            start += 1;
        }

        if first < self.solidsegs[start].first {
            if last < self.solidsegs[start].first - 1 {
                // Post is entirely visible (above start),
                // so insert a new clippost.
                self.seg_renderer.store_wall_range(
                    first,
                    last,
                    seg,
                    view,
                    &mut self.r_data,
                    pic_data,
                    rend,
                );

                next = self.new_end;
                self.new_end += 1;

                while next != start {
                    self.solidsegs[next] = self.solidsegs[next - 1];
                    next -= 1;
                }

                self.solidsegs[next].first = first;
                self.solidsegs[next].last = last;
                return;
            }

            // There is a fragment above *start.
            // TODO: this causes a glitch?
            self.seg_renderer.store_wall_range(
                first,
                self.solidsegs[start].first - 1,
                seg,
                view,
                &mut self.r_data,
                pic_data,
                rend,
            );
            // Now adjust the clip size.
            self.solidsegs[start].first = first;
        }

        // Bottom contained in start?
        if last <= self.solidsegs[start].last {
            return;
        }

        next = start;
        while last >= self.solidsegs[next + 1].first - 1 {
            self.seg_renderer.store_wall_range(
                self.solidsegs[next].last + 1,
                self.solidsegs[next + 1].first - 1,
                seg,
                view,
                &mut self.r_data,
                pic_data,
                rend,
            );

            next += 1;

            if last <= self.solidsegs[next].last {
                self.solidsegs[start].last = self.solidsegs[next].last;
                return self.crunch(start, next);
            }
        }

        // There is a fragment after *next.
        self.seg_renderer.store_wall_range(
            self.solidsegs[next].last + 1,
            last,
            seg,
            view,
            &mut self.r_data,
            pic_data,
            rend,
        );
        // Adjust the clip size.
        self.solidsegs[start].last = last;

        //crunch
        self.crunch(start, next);
    }

    /// R_ClipPassWallSegment - r_bsp
    ///
    /// Clips a portal (two-sided) wall segment against the `solidsegs`
    /// occlusion list and renders visible column spans, but does NOT insert
    /// the segment into the occlusion list. This allows geometry behind
    /// portals (windows, height changes with upper/lower textures) to
    /// remain visible.
    fn clip_portal_seg(
        &mut self,
        first: FixedT,
        last: FixedT,
        seg: &Segment,
        view: &RenderView,
        pic_data: &PicData,
        rend: &mut impl DrawBuffer,
    ) {
        // Find the first range that touches the range
        //  (adjacent pixels are touching).
        let mut start = 0; // first index
        while self.solidsegs[start].last < first - 1 {
            start += 1;
        }

        if first < self.solidsegs[start].first {
            if last < self.solidsegs[start].first - 1 {
                // Post is entirely visible (above start),
                self.seg_renderer.store_wall_range(
                    first,
                    last,
                    seg,
                    view,
                    &mut self.r_data,
                    pic_data,
                    rend,
                );
                return;
            }

            // There is a fragment above *start.
            self.seg_renderer.store_wall_range(
                first,
                self.solidsegs[start].first - 1,
                seg,
                view,
                &mut self.r_data,
                pic_data,
                rend,
            );
        }

        // Bottom contained in start?
        if last <= self.solidsegs[start].last {
            return;
        }

        while last >= self.solidsegs[start + 1].first - 1 {
            self.seg_renderer.store_wall_range(
                self.solidsegs[start].last + 1,
                self.solidsegs[start + 1].first - 1,
                seg,
                view,
                &mut self.r_data,
                pic_data,
                rend,
            );

            start += 1;

            if last <= self.solidsegs[start].last {
                return;
            }
        }

        // There is a fragment after *next.
        self.seg_renderer.store_wall_range(
            self.solidsegs[start].last + 1,
            last,
            seg,
            view,
            &mut self.r_data,
            pic_data,
            rend,
        );
    }

    /// Compacts the `solidsegs` array by removing entries between `start` and
    /// `next` that were absorbed during a solid wall merge. Shifts all
    /// subsequent entries down and updates `new_end`.
    fn crunch(&mut self, mut start: usize, mut next: usize) {
        if next == start {
            return;
        }

        while next != self.new_end && start < self.solidsegs.len() - 1 {
            next += 1;
            start += 1;
            self.solidsegs[start] = self.solidsegs[next];
        }
        self.new_end = start + 1;
    }

    /// R_RenderBSPNode - r_bsp
    ///
    /// Recursively traverses the BSP tree front-to-back from the viewer's
    /// position. Leaf nodes (subsectors) are drawn immediately. For internal
    /// nodes, the front child is always visited; the back child is only
    /// visited if its bounding box passes the FOV/occlusion test.
    fn render_bsp_node(
        &mut self,
        map: &LevelData,
        view: &RenderView,
        node_id: u32,
        pic_data: &PicData,
        rend: &mut impl DrawBuffer,
        player_subsector_id: usize,
        count: &mut usize,
    ) {
        // profile!("render_bsp_node");
        *count += 1;

        if is_subsector(node_id) {
            let subsector_id = if node_id == u32::MAX {
                0
            } else {
                subsector_index(node_id)
            };

            if subsector_id < map.subsectors().len() {
                if node_id == u32::MAX {
                    let subsect = &map.subsectors()[0];
                    self.draw_subsector(map, view, subsect, pic_data, rend);
                } else {
                    let subsect = &map.subsectors()[subsector_index(node_id)];
                    self.draw_subsector(map, view, subsect, pic_data, rend);
                }
                return;
            }
        }

        // otherwise get node
        let node = &map.get_nodes()[node_id as usize];
        let side = node.point_on_side_fixed(view.x, view.y);
        let (front, back) = node.front_back_children_fixed(view.x, view.y);
        // Recursively divide front space.
        self.render_bsp_node(map, view, front, pic_data, rend, player_subsector_id, count);

        // Possibly divide back space.
        let back_side = side ^ 1;
        if self.bb_extents_in_fov(&node.bboxes[back_side], view.x, view.y, view.angle) {
            self.render_bsp_node(map, view, back, pic_data, rend, player_subsector_id, count);
        }
    }

    /// R_CheckBBox - r_bsp
    ///
    /// Tests whether a BSP node's bounding box is potentially visible.
    ///
    /// - Selects the two bbox corners forming the widest angular span from the
    ///   viewer (lookup table indexed by viewer quadrant relative to bbox)
    /// - Clips the resulting angle span against the FOV
    /// - Maps to screen columns and checks against `solidsegs` for full
    ///   occlusion
    /// - Returns `true` if any part of the bbox is unoccluded
    fn bb_extents_in_fov(
        &self,
        bbox: &[Vec2; 2],
        origin_x: FixedT,
        origin_y: FixedT,
        view_angle: Angle<Bam>,
    ) -> bool {
        #[cfg(feature = "hprof")]
        profile!("bb_extents_in_fov");
        let lt_x = FixedT::from_f32(bbox[0].x);
        let lt_y = FixedT::from_f32(bbox[0].y);
        let rb_x = FixedT::from_f32(bbox[1].x);
        let rb_y = FixedT::from_f32(bbox[1].y);

        let boxx;
        let boxy;
        if origin_x <= lt_x {
            boxx = 0;
        } else if origin_x < rb_x {
            boxx = 1;
        } else {
            boxx = 2;
        }

        if origin_y >= lt_y {
            boxy = 0;
        } else if origin_y > rb_y {
            boxy = 1;
        } else {
            boxy = 2;
        }

        let boxpos = (boxy << 2) + boxx;
        if boxpos == 5 || boxpos > 10 {
            return true;
        }

        let (v1x, v1y, v2x, v2y) = match boxpos {
            0 => (rb_x, lt_y, lt_x, rb_y),
            1 => (rb_x, lt_y, lt_x, lt_y),
            2 => (rb_x, rb_y, lt_x, lt_y),
            4 => (lt_x, lt_y, lt_x, rb_y),
            6 => (rb_x, rb_y, rb_x, lt_y),
            8 => (lt_x, lt_y, rb_x, rb_y),
            9 => (lt_x, rb_y, rb_x, rb_y),
            10 => (lt_x, rb_y, rb_x, lt_y),
            _ => (FixedT::ZERO, FixedT::ZERO, FixedT::ZERO, FixedT::ZERO),
        };

        let clipangle_bam = self.fov_half_bam;
        let mut angle1_bam = math::r_point_to_angle(v1x - origin_x, v1y - origin_y);
        let mut angle2_bam = math::r_point_to_angle(v2x - origin_x, v2y - origin_y);

        let span = angle1_bam.wrapping_sub(angle2_bam);

        if span >= ANG180 {
            return true;
        }

        let view_angle_bam = view_angle.inner().0;
        angle1_bam = angle1_bam.wrapping_sub(view_angle_bam);
        angle2_bam = angle2_bam.wrapping_sub(view_angle_bam);

        let mut tspan = angle1_bam.wrapping_add(clipangle_bam);
        if tspan > 2u32.wrapping_mul(clipangle_bam) {
            tspan = tspan.wrapping_sub(2u32.wrapping_mul(clipangle_bam));
            if tspan >= span {
                return false;
            }
            angle1_bam = clipangle_bam;
        }
        tspan = clipangle_bam.wrapping_sub(angle2_bam);
        if tspan > 2u32.wrapping_mul(clipangle_bam) {
            tspan = tspan.wrapping_sub(2u32.wrapping_mul(clipangle_bam));
            if tspan >= span {
                return false;
            }
            angle2_bam = 0u32.wrapping_sub(clipangle_bam);
        }

        let a1_fine = (angle1_bam.wrapping_add(ANG90) >> ANGLETOFINESHIFT) as usize;
        let a2_fine = (angle2_bam.wrapping_add(ANG90) >> ANGLETOFINESHIFT) as usize;
        let x1 = FixedT::from(self.viewangletox[a1_fine & (FINEANGLES / 2 - 1)]);
        let mut x2 = FixedT::from(self.viewangletox[a2_fine & (FINEANGLES / 2 - 1)]);

        // Does not cross a pixel?
        if x1 == x2 {
            return false;
        }
        x2 -= 1;

        let mut start = 0;
        while self.solidsegs[start].last < x2 {
            start += 1;
        }

        if x1 >= self.solidsegs[start].first && x2 <= self.solidsegs[start].last {
            return false;
        }
        true
    }
}
