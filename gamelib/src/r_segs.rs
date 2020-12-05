use sdl2::{rect::Rect, render::Canvas, surface::Surface};
use std::f32::consts::{FRAC_PI_2, PI};
use wad::lumps::Segment;

use crate::{doom_def::ML_MAPPED, angle::{Angle, CLASSIC_SCREEN_X_TO_VIEW}, map_data::MapData, player::Player, point_to_dist, scale};

// angle_t rw_normalangle; // From global angle? R_ScaleFromGlobalAngle
// // angle to line origin
// int rw_angle1; // SHARED, PASS AS AN ARG to segs.c functions
// lighttable_t **walllights; // Set in R_SetupFrame?
// short *maskedtexturecol;

// TODO: possibly init this once then use a `clear` func when new is required
pub(crate) struct SegRender<'a> {
    object: &'a Player,
    /// Current segment, e.g, `curline` in Doom src. We can use this to get the
    /// `sector_t *frontsector;` `sector_t *backsector;` shared variables between
    /// `r_bsp.c` and `r_seg.c`.
    seg:    &'a Segment,
    map:    &'a MapData,

    /// True if any of the segs textures might be visible.
    segtextured:         bool,
    /// False if the back side is the same plane.
    markfloor:           bool,
    markceiling:         bool,
    maskedtexture:       bool,
    // Texture ID's
    toptexture:          i32,
    bottomtexture:       i32,
    midtexture:          i32,
    //
    rw_normalangle:      Angle,
    // regular wall
    rw_x:                i32,
    rw_stopx:            i32,
    rw_centerangle:      Angle,
    rw_offset:           f32,
    rw_distance:         f32, // In R_ScaleFromGlobalAngle? Compute when needed
    rw_scale:            f32,
    rw_scalestep:        f32,
    rw_midtexturemid:    f32,
    rw_toptexturemid:    f32,
    rw_bottomtexturemid: f32,

    pixhigh:     f32,
    pixlow:      f32,
    pixhighstep: f32,
    pixlowstep:  f32,

    topfrac:    f32,
    topstep:    f32,
    bottomfrac: f32,
    bottomstep: f32,

    worldtop:    i32,
    worldbottom: i32,
    worldhigh:   i32,
    worldlow:    i32,
}

impl<'a> SegRender<'a> {
    pub fn new(object: &'a Player, seg: &'a Segment, map: &'a MapData) -> Self {
        SegRender {
            object,
            seg,
            map,
            //
            segtextured: false,
            /// False if the back side is the same plane.
            markfloor: false,
            markceiling: false,
            maskedtexture: false,
            // Texture ID's
            toptexture: 0,
            bottomtexture: 0,
            midtexture: 0,
            //
            rw_normalangle: Angle::default(),
            // regular wall
            rw_x: 0,
            rw_stopx: 0,
            rw_centerangle: Angle::default(),
            rw_offset: 0.0,
            rw_distance: 0.0, // In R_ScaleFromGlobalAngle? Compute when needed
            rw_scale: 0.0,
            rw_scalestep: 0.0,
            rw_midtexturemid: 0.0,
            rw_toptexturemid: 0.0,
            rw_bottomtexturemid: 0.0,

            pixhigh: 0.0,
            pixlow: 0.0,
            pixhighstep: 0.0,
            pixlowstep: 0.0,

            topfrac: 0.0,
            topstep: 0.0,
            bottomfrac: 0.0,
            bottomstep: 0.0,

            worldtop: 0,
            worldbottom: 0,
            worldhigh: 0,
            worldlow: 0,
        }
    }

    /// R_StoreWallRange - r_segs (required in r_bsp)
    pub fn store_wall_range(
        &mut self,
        start: i32,
        stop: i32,
        rw_angle: Angle,
        canvas: &mut Canvas<Surface>,
    ) {
        if start >= 320 || start > stop {
            println!("Bad R_RenderWallRange: {} to {}", start, stop);
            return;
        }

        // These need only be locally defined to make some things easier
        let sidedef = self.seg.sidedef.clone();
        let mut linedef = self.seg.linedef.clone();

        // mark the segment as visible for auto map
        linedef.flags |= ML_MAPPED as u16;

        self.rw_normalangle = Angle::new(self.seg.angle_rads() + FRAC_PI_2);
        let offsetangle = self.rw_normalangle - rw_angle; // radians

        // Unrequired with full angle range
        // if offsetangle > FRAC_PI_2 {
        //     offsetangle = FRAC_PI_2;
        // }

        let distangle = Angle::new(FRAC_PI_2 - offsetangle.rad());
        let hyp = point_to_dist(
            self.seg.start_vertex.x(),
            self.seg.start_vertex.y(),
            self.object.mobj.as_ref().unwrap().obj.xy,
        ); // verified correct
        self.rw_distance = hyp * distangle.sin(); // COrrect??? Seems to be...

        // viewangle = player->mo->angle + viewangleoffset; // offset can be 0, 90, 270
        let view_angle = self.object.mobj.as_ref().unwrap().obj.angle;

        //m_ScreenXToAngle[i] = atan((m_HalfScreenWidth - i) / (float)m_iDistancePlayerToScreen) * 180 / PI;
        let visangle =
            view_angle + CLASSIC_SCREEN_X_TO_VIEW[start as usize] * PI / 180.0; // degress not rads
        let scale1 =
            scale(visangle, self.rw_normalangle, self.rw_distance, view_angle);

        let visangle =
            view_angle + CLASSIC_SCREEN_X_TO_VIEW[stop as usize] * PI / 180.0;
        let scale2 =
            scale(visangle, self.rw_normalangle, self.rw_distance, view_angle);

        // testing draws
        self.rw_scalestep = (scale2 - scale1) / (stop - start) as f32;
        let z = self.object.viewz as i32;

        // calculate texture boundaries
        //  and decide if floor / ceiling marks are needed
        self.worldtop = self.seg.sidedef.sector.ceil_height as i32 - z;
        self.worldbottom = self.seg.sidedef.sector.floor_height as i32 - z;

        // TODO: Texture stuff here
        //  midtexture = toptexture = bottomtexture = maskedtexture = 0;

        self.topstep = -(self.worldtop as f32 * self.rw_scalestep);
        self.topfrac = 100.0 - (self.worldtop as f32 * scale1);

        self.bottomstep = -(self.worldbottom as f32 * self.rw_scalestep);
        self.bottomfrac = 100.0 - (self.worldbottom as f32 * scale1);

        // testing lighting
        let mut lightnum =
            self.seg.linedef.front_sidedef.sector.light_level as u8 >> 4;

        if self.seg.start_vertex.y() == self.seg.end_vertex.y() {
            if lightnum > 5 {
                lightnum -= 5;
            }
        } else if self.seg.start_vertex.x() == self.seg.end_vertex.x() {
            if lightnum < 249 {
                lightnum += 5;
            }
        }

        let z = self.seg.sidedef.sector.floor_height.abs() as u8 / 2;

        let colour = sdl2::pixels::Color::RGBA(
            150 + lightnum - (z >> 2) as u8,
            130 + lightnum - (z >> 2) as u8,
            130 + lightnum - (z >> 2) as u8,
            255,
        );
        canvas.set_draw_color(colour);

        // R_RenderSegLoop
        let mut curr = start;
        while curr <= stop {
            let rect = Rect::new(
                curr,
                self.topfrac as i32,
                1 as u32,
                (self.bottomfrac as i32 - self.topfrac as i32) as u32, // WOAH! floating point rounding stuff
            );
            canvas.fill_rect(rect).unwrap();

            curr += 1;
            self.topfrac += self.topstep;
            self.bottomfrac += self.bottomstep;
            if curr < 0 {
                break;
            }
        }
    }
}
