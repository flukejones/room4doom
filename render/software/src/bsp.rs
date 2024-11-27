use super::RenderData;
use super::defs::ClipRange;
use super::segs::SegRender;
use super::things::VisSprite;
use crate::utilities::{
    angle_to_screen, corrected_fov_for_height, projection, vertex_angle_to_object, y_scale,
};
#[cfg(feature = "hprof")]
use coarse_prof::profile;
use gameplay::log::trace;
use gameplay::{
    Angle, Level, MapData, MapObject, Node, PicData, Player, Sector, Segment, SubSector,
};
use glam::Vec2;
use render_trait::{PixelBuffer, RenderTrait};
use std::f32::consts::{FRAC_PI_2, PI};
use std::mem;

const MAX_SEGS: usize = 128;
const MAX_SECTS: usize = 4096;
const MAX_VIS_SPRITES: usize = 1024;
const IS_SSECTOR_MASK: u32 = 0x80000000;

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
pub struct SoftwareRenderer {
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
    pub y_scale: f32,
    /// Mostly used in thing drawing only
    pub projection: f32,

    pub buf_width: usize,
    pub buf_height: usize,
}

impl SoftwareRenderer {
    pub fn render_player_view(
        &mut self,
        player: &Player,
        level: &Level,
        pic_data: &mut PicData,
        rend: &mut impl RenderTrait,
    ) {
        let map = &level.map_data;

        // TODO: pull duplicate functionality out to a function
        self.clear(rend.draw_buffer().size().width_f32());
        let mut count = 0;
        // TODO: netupdate

        pic_data.set_fixed_lightscale(player.fixedcolormap as usize);
        pic_data.set_player_palette(player);

        self.seg_renderer.clear();
        unsafe {
            self.seg_renderer.set_view_pitch(
                player.lookdir as i16,
                rend.draw_buffer().size().half_height_f32(),
            );
        }
        #[cfg(feature = "hprof")]
        profile!("render_bsp_node begin!");
        self.render_bsp_node(map, player, map.start_node(), pic_data, rend, &mut count);

        trace!("BSP traversals for render: {count}");
        // TODO: netupdate again
        #[cfg(feature = "hprof")]
        profile!("draw_masked");
        self.draw_masked(player, pic_data, rend);
        // TODO: netupdate again
    }

    pub fn new(fov: f32, width: f32, height: f32, double: bool, debug: bool) -> SoftwareRenderer {
        let screen_ratio = width / height;
        let mut buf_height = 200;

        let mut buf_width = (buf_height as f32 * screen_ratio) as usize;
        if double {
            buf_width *= 2;
            buf_height *= 2;
        }
        let fov = corrected_fov_for_height(fov, buf_width as f32, buf_height as f32);
        let projection = projection(fov, buf_width as f32 / 2.0);
        let y_scale = y_scale(fov, buf_width as f32, buf_height as f32);

        Self {
            r_data: RenderData::new(buf_width, buf_height),
            seg_renderer: SegRender::new(fov, buf_width, buf_height),
            new_end: 0,
            solidsegs: [ClipRange {
                first: 0.0,
                last: 0.0,
            }; MAX_SEGS],
            _debug: debug,
            checked_sectors: [-1; MAX_SECTS],
            checked_idx: 0,
            vissprites: [VisSprite::new(); MAX_VIS_SPRITES],
            next_vissprite: 0,
            y_scale,
            projection,
            buf_width,
            buf_height,
        }
    }

    fn clear(&mut self, screen_width: f32) {
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
    fn add_line<'a>(
        &'a mut self,
        player: &Player,
        seg: &'a Segment,
        front_sector: &'a Sector,
        pic_data: &PicData,
        rend: &mut impl RenderTrait,
    ) {
        #[cfg(feature = "hprof")]
        profile!("add_line");
        let mobj = unsafe { player.mobj_unchecked() };
        // reject orthogonal back sides
        let viewangle = mobj.angle;

        // Blocks some zdoom segs rendering
        if !seg.is_facing_point(&mobj.xy) {
            return;
        }

        let mut angle1 = vertex_angle_to_object(&seg.v1, mobj); // widescreen: Leave as is
        let mut angle2 = vertex_angle_to_object(&seg.v2, mobj); // widescreen: Leave as is

        let span = (angle1 - angle2).rad();
        if span.abs() >= PI {
            // widescreen: Leave as is
            return;
        }

        // Global angle needed by segcalc.
        self.r_data.rw_angle1 = angle1; // widescreen: Leave as is
        angle1 -= viewangle; // widescreen: Leave as is
        angle2 -= viewangle; // widescreen: Leave as is

        let clipangle = Angle::new(self.seg_renderer.fov_half); // widescreen: Leave as is
        let clipangrad = clipangle.rad();
        let mut tspan = angle1 + clipangle;
        if tspan.rad() > 2.0 * clipangrad {
            tspan -= 2.0 * clipangrad;
            if tspan.rad() > span {
                return;
            }
            angle1 = clipangle;
        }
        tspan = clipangle - angle2;
        if tspan.rad() > 2.0 * clipangrad {
            tspan -= 2.0 * clipangrad;
            if tspan.rad() >= span {
                return;
            }
            angle2 = -clipangle;
        }
        // OK down to here

        let s = rend.draw_buffer().size();
        let x1 = angle_to_screen(
            self.seg_renderer.fov,
            s.half_width_f32(),
            s.width_f32(),
            angle1,
        );
        let x2 = angle_to_screen(
            self.seg_renderer.fov,
            s.half_width_f32(),
            s.width_f32(),
            angle2,
        );

        // Does not cross a pixel?
        if x1 == x2 {
            return;
        }

        if let Some(back_sector) = seg.backsector.clone() {
            // Doors. Block view
            if back_sector.ceilingheight <= front_sector.floorheight
                || back_sector.floorheight >= front_sector.ceilingheight
            {
                self.clip_solid_seg(x1, x2 - 1.0, seg, player, pic_data, rend);
                return;
            }

            // Windows usually, but also changes in heights from sectors eg: steps
            #[allow(clippy::float_cmp)]
            if back_sector.ceilingheight != front_sector.ceilingheight
                || back_sector.floorheight != front_sector.floorheight
            {
                self.clip_portal_seg(x1, x2 - 1.0, seg, player, pic_data, rend);
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

            self.clip_portal_seg(x1, x2 - 1.0, seg, player, pic_data, rend);
            return;
        }
        self.clip_solid_seg(x1, x2 - 1.0, seg, player, pic_data, rend);
    }

    /// R_Subsector - r_bsp
    fn draw_subsector(
        &mut self,
        map: &MapData,
        player: &Player,
        subsect: &SubSector,
        pic_data: &PicData,
        rend: &mut impl RenderTrait,
    ) {
        #[cfg(feature = "hprof")]
        profile!("draw_subsector");
        let front_sector = &subsect.sector;

        self.add_sprites(
            player,
            front_sector,
            rend.draw_buffer().size().width() as u32,
            pic_data,
        );

        for i in subsect.start_seg..subsect.start_seg + subsect.seg_count {
            let seg = &map.segments()[i as usize];
            self.add_line(player, seg, front_sector, pic_data, rend);
        }
    }

    /// R_ClearClipSegs - r_bsp
    fn clear_clip_segs(&mut self, screen_width: f32) {
        for s in self.solidsegs.iter_mut() {
            s.first = screen_width;
            s.last = f32::MAX;
        }
        self.solidsegs[0].first = f32::MAX;
        self.solidsegs[0].last = f32::MIN;
        self.new_end = 1;
    }

    /// R_ClipSolidWallSegment - r_bsp
    fn clip_solid_seg(
        &mut self,
        first: f32,
        last: f32,
        seg: &Segment,
        object: &Player,
        pic_data: &PicData,
        rend: &mut impl RenderTrait,
    ) {
        let mut next;

        // Find the first range that touches the range
        //  (adjacent pixels are touching).
        let mut start = 0; // first index
        while self.solidsegs[start].last < first - 1.0 {
            start += 1;
        }

        if first < self.solidsegs[start].first {
            if last < self.solidsegs[start].first - 1.0 {
                // Post is entirely visible (above start),
                // so insert a new clippost.
                self.seg_renderer.store_wall_range(
                    first,
                    last,
                    seg,
                    object,
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
                self.solidsegs[start].first - 1.0,
                seg,
                object,
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
        while last >= self.solidsegs[next + 1].first - 1.0 {
            self.seg_renderer.store_wall_range(
                self.solidsegs[next].last + 1.0,
                self.solidsegs[next + 1].first - 1.0,
                seg,
                object,
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
            self.solidsegs[next].last + 1.0,
            last,
            seg,
            object,
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
    /// Clips the given range of columns, but does not includes it in the clip
    /// list. Does handle windows, e.g. LineDefs with upper and lower
    /// texture
    fn clip_portal_seg(
        &mut self,
        first: f32,
        last: f32,
        seg: &Segment,
        player: &Player,
        pic_data: &PicData,
        rend: &mut impl RenderTrait,
    ) {
        // Find the first range that touches the range
        //  (adjacent pixels are touching).
        let mut start = 0; // first index
        while self.solidsegs[start].last < first - 1.0 {
            start += 1;
        }

        if first < self.solidsegs[start].first {
            if last < self.solidsegs[start].first - 1.0 {
                // Post is entirely visible (above start),
                self.seg_renderer.store_wall_range(
                    first,
                    last,
                    seg,
                    player,
                    &mut self.r_data,
                    pic_data,
                    rend,
                );
                return;
            }

            // There is a fragment above *start.
            self.seg_renderer.store_wall_range(
                first,
                self.solidsegs[start].first - 1.0,
                seg,
                player,
                &mut self.r_data,
                pic_data,
                rend,
            );
        }

        // Bottom contained in start?
        if last <= self.solidsegs[start].last {
            return;
        }

        while last >= self.solidsegs[start + 1].first - 1.0 {
            self.seg_renderer.store_wall_range(
                self.solidsegs[start].last + 1.0,
                self.solidsegs[start + 1].first - 1.0,
                seg,
                player,
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
            self.solidsegs[start].last + 1.0,
            last,
            seg,
            player,
            &mut self.r_data,
            pic_data,
            rend,
        );
    }

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
    fn render_bsp_node(
        &mut self,
        map: &MapData,
        player: &Player,
        node_id: u32,
        pic_data: &PicData,
        rend: &mut impl RenderTrait,

        count: &mut usize,
    ) {
        // profile!("render_bsp_node");
        *count += 1;
        let mobj = unsafe { player.mobj_unchecked() };

        if node_id & IS_SSECTOR_MASK != 0 {
            if node_id == u32::MAX {
                let subsect = &map.subsectors()[0];
                // Check if it should be drawn, then draw
                self.draw_subsector(map, player, subsect, pic_data, rend);
            } else {
                // It's a leaf node and is the index to a subsector
                let subsect = &map.subsectors()[(node_id & !IS_SSECTOR_MASK) as usize];
                // Check if it should be drawn, then draw
                self.draw_subsector(map, player, subsect, pic_data, rend);
            }
            return;
        }

        // otherwise get node
        let node = &map.get_nodes()[node_id as usize];
        // find which side the point is on
        let side = node.point_on_side(&mobj.xy);
        // Recursively divide front space.
        self.render_bsp_node(map, player, node.children[side], pic_data, rend, count);

        // Possibly divide back space.
        // check if each corner of the BB is in the FOV
        //if node.point_in_bounds(&v, side ^ 1) {
        if self.bb_extents_in_fov(
            node,
            mobj,
            side ^ 1,
            rend.draw_buffer().size().half_width_f32(),
            rend.draw_buffer().size().width_f32(),
        ) {
            self.render_bsp_node(map, player, node.children[side ^ 1], pic_data, rend, count);
        }
    }

    /// R_CheckBBox - r_bsp
    fn bb_extents_in_fov(
        &self,
        node: &Node,
        mobj: &MapObject,
        side: usize,
        half_screen_width: f32,
        screen_width: f32,
    ) -> bool {
        #[cfg(feature = "hprof")]
        profile!("bb_extents_in_fov");
        let view_angle = mobj.angle;
        // BOXTOP = 0
        // BOXBOT = 1
        // BOXLEFT = 2
        // BOXRIGHT = 3
        let lt = node.bboxes[side][0];
        let rb = node.bboxes[side][1];

        // if node.point_in_bounds(mobj.xyz, side) {
        //     return true;
        // }

        let boxx;
        let boxy;
        if mobj.xy.x <= lt.x {
            boxx = 0;
        } else if mobj.xy.x < rb.x {
            boxx = 1;
        } else {
            boxx = 2;
        }

        if mobj.xy.y >= lt.y {
            boxy = 0;
        } else if mobj.xy.y > rb.y {
            boxy = 1;
        } else {
            boxy = 2;
        }

        let boxpos = (boxy << 2) + boxx;
        if boxpos == 5 || boxpos > 10 {
            return true;
        }

        let (v1, v2) = match boxpos {
            0 => (Vec2::new(rb.x, lt.y), Vec2::new(lt.x, rb.y)),
            1 => (Vec2::new(rb.x, lt.y), lt),
            2 => (rb, lt),
            4 => (lt, Vec2::new(lt.x, rb.y)),
            6 => (rb, Vec2::new(rb.x, lt.y)),
            8 => (lt, rb),
            9 => (Vec2::new(lt.x, rb.y), rb),
            10 => (Vec2::new(lt.x, rb.y), Vec2::new(rb.x, lt.y)),
            _ => (Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0)),
        };

        let clipangle = Angle::new(self.seg_renderer.fov_half);
        let clipangrad = clipangle.rad();
        // Reset to correct angles
        let mut angle1 = vertex_angle_to_object(&v1, mobj);
        let mut angle2 = vertex_angle_to_object(&v2, mobj);

        let span = angle1 - angle2;

        if span.rad() >= FRAC_PI_2 {
            return true;
        }

        angle1 -= view_angle;
        angle2 -= view_angle;

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
            angle2 = -clipangle;
        }

        let x1 = angle_to_screen(
            self.seg_renderer.fov,
            half_screen_width,
            screen_width,
            angle1,
        );
        let mut x2 = angle_to_screen(
            self.seg_renderer.fov,
            half_screen_width,
            screen_width,
            angle2,
        );

        // Does not cross a pixel?
        if x1 == x2 {
            return false;
        }
        x2 -= 1.0;

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
