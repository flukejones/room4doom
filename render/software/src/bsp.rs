use super::{defs::ClipRange, segs::SegRender, things::VisSprite, RenderData};
use crate::{
    segs::{draw_column_style_flats, draw_sky_column},
    utilities::{
        angle_to_screen,
        corrected_fov_for_height,
        projection,
        screen_to_x_view,
        vertex_angle_to_object,
        y_scale,
    },
};
use gameplay::{
    log::trace,
    Angle,
    Level,
    MapData,
    MapObject,
    Node,
    PicData,
    Player,
    Sector,
    Segment,
    SubSector,
    IS_SSECTOR_MASK,
};
use glam::Vec2;
use render_target::{PixelBuffer, PlayRenderer, RenderTarget};
use std::f32::consts::{PI, TAU};

const MAX_SEGS: usize = 64;
const MAX_VIS_SPRITES: usize = 128 * 2;

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
    solidsegs: Vec<ClipRange>,
    /// Visible sprite data, used for Z-ordered rendering of sprites
    pub(super) vissprites: [VisSprite; MAX_VIS_SPRITES],
    /// The next `VisSprite`, incremented during the filling in of `VisSprites`
    pub(super) next_vissprite: usize,

    pub(super) r_data: RenderData,
    pub(super) seg_renderer: SegRender,
    pub(super) _debug: bool,

    /// Used for checking if a sector has been worked on when iterating over
    pub(super) checked_sectors: Vec<u32>,

    /// Mostly used in thing drawing only
    pub y_scale: f32,
    /// Mostly used in thing drawing only
    pub projection: f32,
}

impl PlayRenderer for SoftwareRenderer {
    fn render_player_view(
        &mut self,
        player: &Player,
        level: &Level,
        pic_data: &mut PicData,
        buffer: &mut RenderTarget,
    ) {
        let map = &level.map_data;

        // TODO: pull duplicate functionality out to a function
        self.clear(buffer.pixel_buffer().size().width_f32());
        let mut count = 0;
        self.checked_sectors.clear();
        // TODO: netupdate

        pic_data.set_fixed_lightscale(player.fixedcolormap as usize);
        pic_data.set_player_palette(player);

        self.seg_renderer.clear();
        // let now1 = Instant::now();
        self.render_bsp_node(
            map,
            player,
            map.start_node(),
            pic_data,
            buffer.pixel_buffer(),
            &mut count,
        );
        // println!("bsp time: {:?}", now1.elapsed());
        trace!("BSP traversals for render: {count}");
        // TODO: netupdate again
        // let now1 = Instant::now();
        self.draw_planes(player, pic_data, buffer.pixel_buffer());
        // println!("draw_spans time: {:?}", now1.elapsed());
        // TODO: netupdate again
        self.draw_masked(player, pic_data, buffer.pixel_buffer());
        // TODO: netupdate again
    }
}

impl SoftwareRenderer {
    pub fn new(fov: f32, buf_width: usize, buf_height: usize, debug: bool) -> Self {
        let fov = corrected_fov_for_height(fov, buf_width as f32, buf_height as f32);
        let projection = projection(fov, buf_width as f32 / 2.0);
        let y_scale = y_scale(fov, buf_width as f32, buf_height as f32);

        Self {
            r_data: RenderData::new(buf_width, buf_height),
            seg_renderer: SegRender::new(fov, buf_width, buf_height),
            new_end: 0,
            solidsegs: vec![
                ClipRange {
                    first: 0.0,
                    last: 0.0
                };
                MAX_SEGS
            ],
            _debug: debug,
            checked_sectors: Vec::new(),
            vissprites: [VisSprite::new(); MAX_VIS_SPRITES],
            next_vissprite: 0,
            y_scale,
            projection,
        }
    }

    fn clear(&mut self, screen_width: f32) {
        for vis in self.vissprites.iter_mut() {
            vis.clear();
        }
        self.next_vissprite = 0;

        self.clear_clip_segs(screen_width);
        self.r_data.clear_data();
        // No need to recreate or clear as it is fully overwritten each frame
        // self.seg_renderer = SegRender::new(self.texture_data.clone());
    }

    /// Doom function name `R_DrawPlanes`
    fn draw_planes(&mut self, player: &Player, pic_data: &PicData, pixels: &mut dyn PixelBuffer) {
        let mobj = unsafe { player.mobj_unchecked() };
        let view_angle = mobj.angle;

        // let basexscale = self.r_data.visplane_render.basexscale;
        // let baseyscale = self.r_data.visplane_render.baseyscale;
        let visplanes = &mut self.r_data.visplane_render;
        let sky_doubled = pixels.size().height() != 200;
        let down_shift = if sky_doubled { 12 } else { 6 };
        for plane in &mut visplanes.visplanes[0..=visplanes.lastvisplane] {
            if plane.minx > plane.maxx {
                continue;
            }

            if plane.picnum == pic_data.sky_num() {
                let colourmap = pic_data.colourmap(0);
                let sky_mid = pixels.size().height() / 2 - down_shift; // shift down by 6 pixels
                let skytex = pic_data.sky_pic();

                for x in plane.minx as i32..=plane.maxx as i32 {
                    let dc_yl = plane.top[x as usize];
                    let dc_yh = plane.bottom[x as usize];
                    if dc_yl <= dc_yh {
                        let screen_x_degrees = screen_to_x_view(
                            self.seg_renderer.fov,
                            x as f32,
                            pixels.size().half_width_f32(),
                        );
                        let angle =
                            (view_angle.rad() + screen_x_degrees + TAU * 2.).to_degrees() * 2.8444; // 2.8444 seems to give the corect skybox width
                        let texture_column = pic_data.wall_pic_column(skytex, angle.abs() as usize);
                        // TODO: there is a flaw in this for loop where the sigil II sky causes a
                        // crash
                        draw_sky_column(
                            texture_column,
                            colourmap,
                            0.94,
                            x as f32,
                            sky_mid as f32,
                            dc_yl,
                            dc_yh,
                            pic_data,
                            sky_doubled,
                            pixels,
                        );
                    }
                }
                continue;
            }

            let total_light = (plane.lightlevel >> 4) + player.extralight;
            let plane_height = (plane.height - player.viewz).abs();
            let texture = pic_data.get_flat(plane.picnum);
            for x_start in plane.minx as i32..=plane.maxx as i32 {
                let dc_yl = plane.top[x_start as usize];
                let dc_yh = plane.bottom[x_start as usize];
                if dc_yl <= dc_yh {
                    // TODO: there is a flaw in this for loop where the sigil II sky causes a crash
                    draw_column_style_flats(
                        texture,
                        mobj.xy,
                        plane_height,
                        total_light,
                        x_start as f32,
                        mobj.angle,
                        dc_yl,
                        dc_yh,
                        pic_data,
                        pixels,
                        &self.seg_renderer.yslope,
                        self.seg_renderer.fov,
                        self.seg_renderer.wide_ratio,
                    );
                }
            }
        }
    }

    /// R_AddLine - r_bsp
    fn add_line<'a>(
        &'a mut self,
        player: &Player,
        seg: &'a Segment,
        front_sector: &'a Sector,
        pic_data: &PicData,
        pixels: &mut dyn PixelBuffer,
    ) {
        let mobj = unsafe { player.mobj_unchecked() };
        // reject orthogonal back sides
        let viewangle = mobj.angle;

        if !seg.is_facing_point(&mobj.xy) {
            return;
        }

        let clipangle = Angle::new(self.seg_renderer.fov_half); // widescreen: Leave as is
        let mut angle1 = vertex_angle_to_object(&seg.v1, mobj); // widescreen: Leave as is
        let mut angle2 = vertex_angle_to_object(&seg.v2, mobj); // widescreen: Leave as is

        let span = (angle1 - angle2).rad();
        if span >= PI {
            // widescreen: Leave as is
            return;
        }

        // Global angle needed by segcalc.
        self.r_data.rw_angle1 = angle1; // widescreen: Leave as is

        angle1 -= viewangle; // widescreen: Leave as is
        angle2 -= viewangle; // widescreen: Leave as is

        let mut tspan = angle1 + clipangle;
        if tspan.rad() > 2.0 * clipangle.rad() {
            tspan -= 2.0 * clipangle.rad();
            if tspan.rad() > span {
                return;
            }
            angle1 = clipangle;
        }
        tspan = clipangle - angle2;
        if tspan.rad() > 2.0 * clipangle.rad() {
            tspan -= 2.0 * clipangle.rad();
            if tspan.rad() >= span {
                return;
            }
            angle2 = -clipangle;
        }
        // OK down to here

        let x1 = angle_to_screen(
            self.seg_renderer.fov,
            pixels.size().half_width_f32(),
            pixels.size().width_f32(),
            angle1,
        );
        let x2 = angle_to_screen(
            self.seg_renderer.fov,
            pixels.size().half_width_f32(),
            pixels.size().width_f32(),
            angle2,
        );

        // Does not cross a pixel?
        if x1 == x2 {
            return;
        }

        if let Some(back_sector) = &seg.backsector {
            // Doors. Block view
            if back_sector.ceilingheight <= front_sector.floorheight
                || back_sector.floorheight >= front_sector.ceilingheight
            {
                self.clip_solid_seg(x1, x2 - 1.0, seg, player, pic_data, pixels);
                return;
            }

            // Windows usually, but also changes in heights from sectors eg: steps
            #[allow(clippy::float_cmp)]
            if back_sector.ceilingheight != front_sector.ceilingheight
                || back_sector.floorheight != front_sector.floorheight
            {
                self.clip_portal_seg(x1, x2 - 1.0, seg, player, pic_data, pixels);
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
            self.clip_portal_seg(x1, x2 - 1.0, seg, player, pic_data, pixels);
        } else {
            self.clip_solid_seg(x1, x2 - 1.0, seg, player, pic_data, pixels);
        }
    }

    /// R_Subsector - r_bsp
    fn draw_subsector(
        &mut self,
        map: &MapData,
        player: &Player,
        subsect: &SubSector,
        pic_data: &PicData,
        pixels: &mut dyn PixelBuffer,
    ) {
        let skynum = pic_data.sky_num();
        // TODO: planes for floor & ceiling
        if subsect.sector.floorheight <= player.viewz && subsect.sector.floorpic != usize::MAX {
            self.r_data.visplane_render.floorplane = self.r_data.visplane_render.find_plane(
                subsect.sector.floorheight,
                subsect.sector.floorpic,
                skynum,
                subsect.sector.lightlevel,
            );
        }

        if (subsect.sector.ceilingheight >= player.viewz
            || subsect.sector.ceilingpic == pic_data.sky_num())
            && subsect.sector.ceilingpic != usize::MAX
        {
            self.r_data.visplane_render.ceilingplane = self.r_data.visplane_render.find_plane(
                subsect.sector.ceilingheight,
                subsect.sector.ceilingpic,
                skynum,
                subsect.sector.lightlevel,
            );
        }

        let front_sector = &subsect.sector;

        self.add_sprites(player, front_sector, pixels.size().width() as u32, pic_data);

        for i in subsect.start_seg..subsect.start_seg + subsect.seg_count {
            let seg = &map.segments()[i as usize];
            self.add_line(player, seg, front_sector, pic_data, pixels);
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
        pixels: &mut dyn PixelBuffer,
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
                    pixels,
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
                pixels,
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
                pixels,
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
            pixels,
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
        object: &Player,
        pic_data: &PicData,
        pixels: &mut dyn PixelBuffer,
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
                    object,
                    &mut self.r_data,
                    pic_data,
                    pixels,
                );
                return;
            }

            // There is a fragment above *start.
            self.seg_renderer.store_wall_range(
                first,
                self.solidsegs[start].first - 1.0,
                seg,
                object,
                &mut self.r_data,
                pic_data,
                pixels,
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
                object,
                &mut self.r_data,
                pic_data,
                pixels,
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
            object,
            &mut self.r_data,
            pic_data,
            pixels,
        );
    }

    fn crunch(&mut self, mut start: usize, mut next: usize) {
        if next == start {
            return;
        }

        while next != self.new_end && start < self.solidsegs.len() {
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
        node_id: u16,
        pic_data: &PicData,
        pixels: &mut dyn PixelBuffer,
        count: &mut usize,
    ) {
        *count += 1;
        let mobj = unsafe { player.mobj_unchecked() };

        if node_id & IS_SSECTOR_MASK != 0 {
            // It's a leaf node and is the index to a subsector
            let subsect = &map.subsectors()[(node_id & !IS_SSECTOR_MASK) as usize];
            // Check if it should be drawn, then draw
            self.draw_subsector(map, player, subsect, pic_data, pixels);
            return;
        }

        // otherwise get node
        let node = &map.get_nodes()[node_id as usize];
        // find which side the point is on
        let side = node.point_on_side(&mobj.xy);
        // Recursively divide front space.
        self.render_bsp_node(map, player, node.child_index[side], pic_data, pixels, count);

        // Possibly divide back space.
        // check if each corner of the BB is in the FOV
        //if node.point_in_bounds(&v, side ^ 1) {
        if self.bb_extents_in_fov(
            node,
            mobj,
            side ^ 1,
            pixels.size().half_width_f32(),
            pixels.size().width_f32(),
        ) {
            self.render_bsp_node(
                map,
                player,
                node.child_index[side ^ 1],
                pic_data,
                pixels,
                count,
            );
        }
    }

    /// R_CheckBBox - r_bsp
    ///
    /// TODO: solidsegs list
    fn bb_extents_in_fov(
        &self,
        node: &Node,
        mobj: &MapObject,
        side: usize,
        half_screen_width: f32,
        screen_width: f32,
    ) -> bool {
        let view_angle = mobj.angle;
        // BOXTOP = 0
        // BOXBOT = 1
        // BOXLEFT = 2
        // BOXRIGHT = 3
        let lt = node.bounding_boxes[side][0];
        let rb = node.bounding_boxes[side][1];

        if node.point_in_bounds(&mobj.xy, side) {
            return true;
        }

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
        if boxpos == 5 {
            return true;
        }

        let v1;
        let v2;
        match boxpos {
            0 => {
                v1 = Vec2::new(rb.x, lt.y);
                v2 = Vec2::new(lt.x, rb.y);
            }
            1 => {
                v1 = Vec2::new(rb.x, lt.y);
                v2 = lt;
            }
            2 => {
                v1 = rb;
                v2 = lt;
            }
            4 => {
                v1 = lt;
                v2 = Vec2::new(lt.x, rb.y);
            }
            6 => {
                v1 = rb;
                v2 = Vec2::new(rb.x, lt.y);
            }
            8 => {
                v1 = lt;
                v2 = rb;
            }
            9 => {
                v1 = Vec2::new(lt.x, rb.y);
                v2 = rb;
            }
            10 => {
                v1 = Vec2::new(lt.x, rb.y);
                v2 = Vec2::new(rb.x, lt.y);
            }
            _ => {
                return false;
            }
        }

        let clipangle = Angle::new(self.seg_renderer.fov_half);
        // Reset to correct angles
        let mut angle1 = vertex_angle_to_object(&v1, mobj);
        let mut angle2 = vertex_angle_to_object(&v2, mobj);

        let span = angle1 - angle2;

        if span.rad() >= PI {
            return true;
        }

        angle1 -= view_angle;
        angle2 -= view_angle;

        let mut tspan = angle1 + clipangle;
        if tspan.rad() >= clipangle.rad() * 2.0 {
            tspan -= 2.0 * clipangle.rad();
            if tspan.rad() >= span.rad() {
                return false;
            }
            angle1 = clipangle;
        }
        tspan = clipangle - angle2;
        if tspan.rad() >= 2.0 * clipangle.rad() {
            tspan -= 2.0 * clipangle.rad();
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

#[cfg(test)]
mod tests {
    use gameplay::{MapData, PicData, IS_SSECTOR_MASK};
    use wad::WadData;

    #[test]
    fn check_nodes_of_e1m1() {
        let wad = WadData::new("../../doom1.wad".into());
        let mut map = MapData::new("E1M1".to_owned());
        map.load(&PicData::default(), &wad);

        let nodes = map.get_nodes();
        assert_eq!(nodes[0].xy.x as i32, 1552);
        assert_eq!(nodes[0].xy.y as i32, -2432);
        assert_eq!(nodes[0].delta.x as i32, 112);
        assert_eq!(nodes[0].delta.y as i32, 0);

        assert_eq!(nodes[0].bounding_boxes[0][0].x as i32, 1552); //left
        assert_eq!(nodes[0].bounding_boxes[0][0].y as i32, -2432); //top
        assert_eq!(nodes[0].bounding_boxes[0][1].x as i32, 1664); //right
        assert_eq!(nodes[0].bounding_boxes[0][1].y as i32, -2560); //bottom

        assert_eq!(nodes[0].bounding_boxes[1][0].x as i32, 1600);
        assert_eq!(nodes[0].bounding_boxes[1][0].y as i32, -2048);

        assert_eq!(nodes[0].child_index[0], 32768);
        assert_eq!(nodes[0].child_index[1], 32769);
        assert_eq!(IS_SSECTOR_MASK, 0x8000);

        assert_eq!(nodes[235].xy.x as i32, 2176);
        assert_eq!(nodes[235].xy.y as i32, -3776);
        assert_eq!(nodes[235].delta.x as i32, 0);
        assert_eq!(nodes[235].delta.y as i32, -32);
        assert_eq!(nodes[235].child_index[0], 128);
        assert_eq!(nodes[235].child_index[1], 234);

        println!("{:#018b}", IS_SSECTOR_MASK);

        println!("00: {:#018b}", nodes[0].child_index[0]);
        println!("00: {:#018b}", nodes[0].child_index[1]);

        println!("01: {:#018b}", nodes[1].child_index[0]);
        println!("01: {:#018b}", nodes[1].child_index[1]);

        println!("02: {:#018b}", nodes[2].child_index[0]);
        println!("02: {:#018b}", nodes[2].child_index[1]);

        println!("03: {:#018b}", nodes[3].child_index[0]);
        println!("03: {:#018b}", nodes[3].child_index[1]);
    }
}
