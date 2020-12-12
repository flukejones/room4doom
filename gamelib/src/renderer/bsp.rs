use crate::angle::Angle;
use crate::level_data::map_data::{MapData, IS_SSECTOR_MASK};
use crate::level_data::map_defs::{Segment, SubSector};
use crate::p_map_object::MapObject;
use crate::player::Player;
use crate::renderer::defs::{ClipRange, DrawSeg};
use crate::renderer::portals::PortalClip;
use crate::renderer::segs::SegRender;
use glam::Vec2;
use sdl2::{render::Canvas, surface::Surface};
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};

const MAX_SEGS: usize = 32;

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

/// We store most of what is needed for rendering in various functions here to avoid
/// having to pass too many things in args through multiple function calls. This
/// is due to the Doom C relying a fair bit on global state.
///
/// `RenderData` will be passed to the sprite drawer/clipper to use `drawsegs`
/// ----------------------------------------------------------------------------
/// - R_DrawSprite, r_things.c
/// - R_DrawMasked, r_things.c
/// - R_StoreWallRange, r_segs.c, checks only for overflow of drawsegs, and uses *one* entry through ds_p
///                               it then inserts/incs pointer to next drawseg in the array when finished
/// - R_DrawPlanes, r_plane.c, checks only for overflow of drawsegs
#[derive(Default)]
pub(crate) struct RenderData {
    solidsegs:   Vec<ClipRange>,
    portal_clip: PortalClip,

    /// index in to self.solidsegs
    new_end:       usize,
    pub rw_angle1: Angle,

    /// index to drawsegs
    /// Used in r_segs and r_things
    pub ds_p:     usize, // Or, depending on place in code this can be skipped and a new
    // DrawSeg used, which is inserted in drawsegs at end of r_segs
    pub drawsegs: Vec<DrawSeg>,
}

impl RenderData {
    /// R_AddLine - r_bsp
    fn add_line<'a>(
        &'a mut self,
        map: &MapData,
        player: &Player,
        seg: &'a Segment,
        canvas: &mut Canvas<Surface>,
    ) {
        // reject orthogonal back sides
        let xy = player.mobj.as_ref().unwrap().obj.xy;
        let angle = player.mobj.as_ref().unwrap().obj.angle;

        if !seg.is_facing_point(&xy) {
            return;
        }

        let clipangle = Angle::new(FRAC_PI_4);
        // Reset to correct angles
        let mut angle1 =
            vertex_angle_to_object(&seg.v1, &player.mobj.as_ref().unwrap().obj);
        let mut angle2 =
            vertex_angle_to_object(&seg.v2, &player.mobj.as_ref().unwrap().obj);

        let span = angle1 - angle2;

        if span.rad() >= PI {
            return;
        }

        // Global angle needed by segcalc.
        self.rw_angle1 = angle1;

        angle1 -= angle;
        angle2 -= angle;

        let mut tspan = angle1 + clipangle;
        if tspan.rad() >= FRAC_PI_2 {
            tspan -= 2.0 * clipangle.rad();

            // Totally off the left edge?
            if tspan.rad() >= span.rad() {
                return;
            }
            angle1 = clipangle;
        }
        tspan = clipangle - angle2;
        if tspan.rad() > 2.0 * clipangle.rad() {
            tspan -= 2.0 * clipangle.rad();

            // Totally off the left edge?
            if tspan.rad() >= span.rad() {
                return;
            }
            angle2 = -clipangle;
        }

        angle1 += FRAC_PI_2;
        angle2 += FRAC_PI_2;
        let x1 = angle_to_screen(angle1.rad()); // this or vertex_angle_to_object incorrect
        let x2 = angle_to_screen(angle2.rad());

        // Does not cross a pixel?
        if x1 == x2 {
            return;
        }

        if let Some(back_sector) = &seg.linedef.back_sidedef {
            let front_sector = &seg.linedef.front_sidedef.sector;
            let back_sector = &back_sector.sector;

            // Doors. Block view
            if back_sector.ceilingheight <= front_sector.floorheight
                || back_sector.floorheight >= front_sector.ceilingheight
            {
                self.clip_solid_seg(x1, x2 - 1, seg, map, player, canvas);
                return;
            }

            // Windows usually, but also changes in heights from sectors eg: steps
            #[allow(clippy::float_cmp)]
            if back_sector.ceilingheight != front_sector.ceilingheight
                || back_sector.floorheight != front_sector.floorheight
            {
                self.clip_portal_seg(x1, x2 - 1, seg, map, player, canvas);
                return;
            }

            // Reject empty lines used for triggers and special events.
            // Identical floor and ceiling on both sides, identical light levels
            // on both sides, and no middle texture.
            if back_sector.ceilingpic == front_sector.ceilingpic
                && back_sector.floorpic == front_sector.floorpic
                && back_sector.lightlevel == front_sector.lightlevel
                && seg.linedef.front_sidedef.midtexture == 0
            {
                return;
            }
        }
        self.clip_solid_seg(x1, x2 - 1, seg, map, player, canvas);
    }

    /// R_Subsector - r_bsp
    fn draw_subsector<'a>(
        &'a mut self,
        map: &MapData,
        object: &Player,
        subsect: &SubSector,
        canvas: &mut Canvas<Surface>,
    ) {
        // TODO: planes for floor & ceiling
        for i in subsect.start_seg..subsect.start_seg + subsect.seg_count {
            let seg = &map.get_segments()[i as usize];
            self.add_line(map, object, &seg, canvas);
        }
    }

    /// R_ClearClipSegs - r_bsp
    pub fn clear_clip_segs(&mut self) {
        self.solidsegs.clear();
        self.solidsegs.push(ClipRange {
            first: -0x7fffffff,
            last:  -1,
        });
        for _ in 0..MAX_SEGS {
            self.solidsegs.push(ClipRange {
                first: 320,
                last:  0x7fffffff,
            });
        }
        self.new_end = 2;
    }

    /// R_ClipSolidWallSegment - r_bsp
    fn clip_solid_seg(
        &mut self,
        first: i32,
        last: i32,
        seg: &Segment,
        map: &MapData,
        object: &Player,
        canvas: &mut Canvas<Surface>,
    ) {
        let mut next;

        // Find the first range that touches the range
        //  (adjacent pixels are touching).
        let mut start = 0; // first index
        while self.solidsegs[start].last < first - 1 {
            start += 1;
        }

        // We create the seg-renderer for each seg as data is not shared
        // TODO: check the above
        let mut seg_render = SegRender::new(object, seg, map);

        if first < self.solidsegs[start].first {
            if last < self.solidsegs[start].first - 1 {
                // Post is entirely visible (above start),
                // so insert a new clippost.
                seg_render.store_wall_range(first, last, self, canvas);

                next = self.new_end;
                self.new_end += 1;

                while next != 0 && next != start {
                    self.solidsegs[next] = self.solidsegs[next - 1];
                    next -= 1;
                }

                self.solidsegs[next].first = first;
                self.solidsegs[next].last = last;
                return;
            }

            // There is a fragment above *start.
            seg_render.store_wall_range(
                first,
                self.solidsegs[start].first - 1,
                self,
                canvas,
            );
            // Now adjust the clip size.
            self.solidsegs[start].first = first;
        }

        // Bottom contained in start?
        if last <= self.solidsegs[start].last {
            return;
        }

        next = start;
        while last >= self.solidsegs[next + 1].first - 1
            && next + 1 < self.solidsegs.len() - 1
        {
            seg_render.store_wall_range(
                self.solidsegs[next].last + 1,
                self.solidsegs[next + 1].first - 1,
                self,
                canvas,
            );

            next += 1;

            if last <= self.solidsegs[next].last {
                self.solidsegs[start].last = self.solidsegs[next].last;
                return self.crunch(start, next);
            }
        }

        // There is a fragment after *next.
        seg_render.store_wall_range(
            self.solidsegs[next].last + 1,
            last,
            self,
            canvas,
        );
        // Adjust the clip size.
        self.solidsegs[start].last = last;

        //crunch
        self.crunch(start, next);
    }

    /// R_ClipPassWallSegment - r_bsp
    /// Clips the given range of columns, but does not includes it in the clip list.
    /// Does handle windows, e.g. LineDefs with upper and lower texture
    fn clip_portal_seg(
        &mut self,
        first: i32,
        last: i32,
        seg: &Segment,
        map: &MapData,
        object: &Player,
        canvas: &mut Canvas<Surface>,
    ) {
        let mut next;

        // Find the first range that touches the range
        //  (adjacent pixels are touching).
        let mut start = 0; // first index
        while self.solidsegs[start].last < first - 1 {
            start += 1;
        }

        let mut seg_render = SegRender::new(object, seg, map);

        if first < self.solidsegs[start].first {
            if last < self.solidsegs[start].first - 1 {
                // Post is entirely visible (above start),
                seg_render.store_wall_range(first, last, self, canvas);
                return;
            }

            // There is a fragment above *start.
            seg_render.store_wall_range(
                first,
                self.solidsegs[start].first - 1,
                self,
                canvas,
            );
        }

        // Bottom contained in start?
        if last <= self.solidsegs[start].last {
            return;
        }

        next = start;
        while last >= self.solidsegs[next + 1].first - 1
            && next + 1 < self.solidsegs.len() - 1
        {
            seg_render.store_wall_range(
                self.solidsegs[next].last + 1,
                self.solidsegs[next + 1].first - 1,
                self,
                canvas,
            );

            next += 1;

            if last <= self.solidsegs[next].last {
                return;
            }
        }

        // There is a fragment after *next.
        seg_render.store_wall_range(
            self.solidsegs[next].last + 1,
            last,
            self,
            canvas,
        );
    }

    fn crunch(&mut self, mut start: usize, mut next: usize) {
        {
            if next == start {
                return;
            }

            while (next + 1) != self.new_end && start < self.solidsegs.len() - 1
            {
                next += 1;
                start += 1;
                self.solidsegs[start] = self.solidsegs[next];
            }
            self.new_end = start + 1;
        }
    }

    /// R_RenderBSPNode - r_bsp
    pub fn render_bsp_node<'a>(
        &'a mut self,
        map: &MapData,
        player: &Player,
        node_id: u16,
        canvas: &mut Canvas<Surface>,
    ) {
        if node_id & IS_SSECTOR_MASK == IS_SSECTOR_MASK {
            // It's a leaf node and is the index to a subsector
            let subsect =
                &map.get_subsectors()[(node_id ^ IS_SSECTOR_MASK) as usize];
            // Check if it should be drawn, then draw
            self.draw_subsector(map, player, &subsect, canvas);
            return;
        }

        let mobj = &player.mobj.as_ref().unwrap().obj;
        // otherwise get node
        let node = &map.get_nodes()[node_id as usize];
        // find which side the point is on
        let side = node.point_on_side(&mobj.xy);
        // Recursively divide front space.
        self.render_bsp_node(map, player, node.child_index[side], canvas);

        // Possibly divide back space.
        // check if each corner of the BB is in the FOV
        //if node.point_in_bounds(&v, side ^ 1) {
        if node.bb_extents_in_fov(
            &mobj.xy,
            mobj.angle.rad(),
            FRAC_PI_4,
            side ^ 1,
        ) {
            self.render_bsp_node(
                map,
                player,
                node.child_index[side ^ 1],
                canvas,
            );
        }
    }
}

/// ANgle must be within 90d range
fn angle_to_screen(mut radian: f32) -> i32 {
    let mut x;

    // Left side
    let p = 160.0 / (FRAC_PI_4).tan();
    if radian > FRAC_PI_2 {
        radian -= FRAC_PI_2;
        let t = radian.tan();
        x = t * p;
        x = p - x;
    } else {
        // Right side
        radian = (FRAC_PI_2) - radian;
        let t = radian.tan();
        x = t * p;
        x += p;
    }
    x as i32
}

/// R_PointToAngle
// To get a global angle from cartesian coordinates,
//  the coordinates are flipped until they are in
//  the first octant of the coordinate system, then
//  the y (<=x) is scaled and divided by x to get a
//  tangent (slope) value which is looked up in the
//  tantoangle[] table.
///
/// The flipping isn't done here...
pub(crate) fn vertex_angle_to_object(vertex: &Vec2, mobj: &MapObject) -> Angle {
    let x = vertex.x() - mobj.xy.x();
    let y = vertex.y() - mobj.xy.y();
    Angle::new(y.atan2(x))
}

pub(crate) fn point_to_angle_2(point1: &Vec2, point2: &Vec2) -> Angle {
    let x = point1.x() - point2.x();
    let y = point1.y() - point2.y();
    Angle::new(y.atan2(x))
}

#[cfg(test)]
mod tests {
    use crate::angle::Angle;
    use crate::level_data::map_data::MapData;
    use crate::renderer::bsp::IS_SSECTOR_MASK;
    use glam::Vec2;
    use std::f32::consts::{FRAC_PI_2, PI};
    use wad::WadData;

    #[test]
    fn check_e1m1_things() {
        let wad = WadData::new("../doom1.wad".into());
        let mut map = MapData::new("E1M1".to_owned());
        map.load(&wad);

        let things = map.get_things();
        assert_eq!(things[0].x as i32, 1056);
        assert_eq!(things[0].y as i32, -3616);
        assert_eq!(things[0].angle, 90);
        assert_eq!(things[0].kind, 1);
        assert_eq!(things[0].flags, 7);
        assert_eq!(things[137].x as i32, 3648);
        assert_eq!(things[137].y as i32, -3840);
        assert_eq!(things[137].angle, 0);
        assert_eq!(things[137].kind, 2015);
        assert_eq!(things[137].flags, 7);

        assert_eq!(things[0].angle, 90);
        assert_eq!(things[9].angle, 135);
        assert_eq!(things[14].angle, 0);
        assert_eq!(things[16].angle, 90);
        assert_eq!(things[17].angle, 180);
        assert_eq!(things[83].angle, 270);
    }

    #[test]
    fn check_e1m1_vertexes() {
        let wad = WadData::new("../doom1.wad".into());
        let mut map = MapData::new("E1M1".to_owned());
        map.load(&wad);

        let vertexes = map.get_vertexes();
        assert_eq!(vertexes[0].x() as i32, 1088);
        assert_eq!(vertexes[0].y() as i32, -3680);
        assert_eq!(vertexes[466].x() as i32, 2912);
        assert_eq!(vertexes[466].y() as i32, -4848);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn check_e1m1_lump_pointers() {
        let wad = WadData::new("../doom1.wad".into());
        let mut map = MapData::new("E1M1".to_owned());
        map.load(&wad);

        let linedefs = map.get_linedefs();

        // Check links
        // LINEDEF->VERTEX
        assert_eq!(linedefs[2].v1.x() as i32, 1088);
        assert_eq!(linedefs[2].v2.x() as i32, 1088);
        // // LINEDEF->SIDEDEF
        // assert_eq!(linedefs[2].front_sidedef.midtexture, "LITE3");
        // // LINEDEF->SIDEDEF->SECTOR
        // assert_eq!(linedefs[2].front_sidedef.sector.floorpic, "FLOOR4_8");
        // // LINEDEF->SIDEDEF->SECTOR
        assert_eq!(linedefs[2].front_sidedef.sector.ceilingheight, 72.0);

        let segments = map.get_segments();
        // SEGMENT->VERTEX
        assert_eq!(segments[0].v1.x() as i32, 1552);
        assert_eq!(segments[0].v2.x() as i32, 1552);
        // SEGMENT->LINEDEF->SIDEDEF->SECTOR
        // seg:0 -> line:152 -> side:209 -> sector:0 -> ceiltex:CEIL3_5 lightlevel:160
        // assert_eq!(
        //     segments[0].linedef.front_sidedef.sector.ceilingpic,
        //     "CEIL3_5"
        // );
        // // SEGMENT->LINEDEF->SIDEDEF
        // assert_eq!(segments[0].linedef.front_sidedef.toptexture, "BIGDOOR2");

        // let sides = map.get_sidedefs();
        // assert_eq!(sides[211].sector.ceilingpic, "TLITE6_4");
    }

    #[test]
    fn check_e1m1_linedefs() {
        let wad = WadData::new("../doom1.wad".into());
        let mut map = MapData::new("E1M1".to_owned());
        map.load(&wad);

        let linedefs = map.get_linedefs();
        assert_eq!(linedefs[0].v1.x() as i32, 1088);
        assert_eq!(linedefs[0].v2.x() as i32, 1024);
        assert_eq!(linedefs[2].v1.x() as i32, 1088);
        assert_eq!(linedefs[2].v2.x() as i32, 1088);

        assert_eq!(linedefs[474].v1.x() as i32, 3536);
        assert_eq!(linedefs[474].v2.x() as i32, 3520);
        assert!(linedefs[2].back_sidedef.is_none());
        assert_eq!(linedefs[474].flags, 1);
        assert!(linedefs[474].back_sidedef.is_none());
        assert!(linedefs[466].back_sidedef.is_some());

        // Flag check
        assert_eq!(linedefs[26].flags, 29);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn check_e1m1_sectors() {
        let wad = WadData::new("../doom1.wad".into());
        let mut map = MapData::new("E1M1".to_owned());
        map.load(&wad);

        let sectors = map.get_sectors();
        assert_eq!(sectors[0].floorheight, 0.0);
        assert_eq!(sectors[0].ceilingheight, 72.0);
        assert_eq!(sectors[0].lightlevel, 160);
        assert_eq!(sectors[0].tag, 0);
        assert_eq!(sectors[84].floorheight, -24.0);
        assert_eq!(sectors[84].ceilingheight, 48.0);
        assert_eq!(sectors[84].lightlevel, 255);
        assert_eq!(sectors[84].special, 0);
        assert_eq!(sectors[84].tag, 0);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn check_e1m1_sidedefs() {
        let wad = WadData::new("../doom1.wad".into());
        let mut map = MapData::new("E1M1".to_owned());
        map.load(&wad);

        let sidedefs = map.get_sidedefs();
        assert_eq!(sidedefs[0].rowoffset, 0.0);
        assert_eq!(sidedefs[0].textureoffset, 0.0);
        assert_eq!(sidedefs[9].rowoffset, 0.0);
        assert_eq!(sidedefs[9].textureoffset, 48.0);
        assert_eq!(sidedefs[647].rowoffset, 4.0);
        assert_eq!(sidedefs[647].textureoffset, 0.0);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn check_e1m1_segments() {
        let wad = WadData::new("../doom1.wad".into());

        let mut map = MapData::new("E1M1".to_owned());
        map.load(&wad);

        let segments = map.get_segments();
        assert_eq!(segments[0].v1.x() as i32, 1552);
        assert_eq!(segments[0].v2.x() as i32, 1552);
        assert_eq!(segments[731].v1.x() as i32, 3040);
        assert_eq!(segments[731].v2.x() as i32, 2976);
        assert_eq!(segments[0].angle, Angle::new(FRAC_PI_2));
        assert_eq!(segments[0].offset, 0.0);

        assert_eq!(segments[731].angle, Angle::new(PI));
        assert_eq!(segments[731].offset, 0.0);

        let subsectors = map.get_subsectors();
        assert_eq!(subsectors[0].seg_count, 4);
        assert_eq!(subsectors[124].seg_count, 3);
        assert_eq!(subsectors[236].seg_count, 4);
        //assert_eq!(subsectors[0].start_seg.start_vertex.x() as i32, 1552);
        //assert_eq!(subsectors[124].start_seg.start_vertex.x() as i32, 472);
        //assert_eq!(subsectors[236].start_seg.start_vertex.x() as i32, 3040);
    }

    #[test]
    fn check_nodes_of_e1m1() {
        let wad = WadData::new("../doom1.wad".into());
        let mut map = MapData::new("E1M1".to_owned());
        map.load(&wad);

        let nodes = map.get_nodes();
        assert_eq!(nodes[0].xy.x() as i32, 1552);
        assert_eq!(nodes[0].xy.y() as i32, -2432);
        assert_eq!(nodes[0].delta.x() as i32, 112);
        assert_eq!(nodes[0].delta.y() as i32, 0);

        assert_eq!(nodes[0].bounding_boxes[0][0].x() as i32, 1552); //left
        assert_eq!(nodes[0].bounding_boxes[0][0].y() as i32, -2432); //top
        assert_eq!(nodes[0].bounding_boxes[0][1].x() as i32, 1664); //right
        assert_eq!(nodes[0].bounding_boxes[0][1].y() as i32, -2560); //bottom

        assert_eq!(nodes[0].bounding_boxes[1][0].x() as i32, 1600);
        assert_eq!(nodes[0].bounding_boxes[1][0].y() as i32, -2048);

        assert_eq!(nodes[0].child_index[0], 32768);
        assert_eq!(nodes[0].child_index[1], 32769);
        assert_eq!(IS_SSECTOR_MASK, 0x8000);

        assert_eq!(nodes[235].xy.x() as i32, 2176);
        assert_eq!(nodes[235].xy.y() as i32, -3776);
        assert_eq!(nodes[235].delta.x() as i32, 0);
        assert_eq!(nodes[235].delta.y() as i32, -32);
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

    #[test]
    fn find_vertex_using_bsptree() {
        let wad = WadData::new("../doom1.wad".into());
        let mut map = MapData::new("E1M1".to_owned());
        map.load(&wad);

        // The actual location of THING0
        let player = Vec2::new(1056.0, -3616.0);
        let subsector = map.point_in_subsector(&player);
        //assert_eq!(subsector_id, Some(103));
        assert_eq!(subsector.seg_count, 5);
        assert_eq!(subsector.start_seg, 305);
    }
}
