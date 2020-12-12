use sdl2::{rect::Rect, render::Canvas, surface::Surface};
use std::{
    f32::consts::{FRAC_PI_2, PI},
    ptr::NonNull,
};

use crate::angle::{Angle, CLASSIC_SCREEN_X_TO_VIEW};
use crate::doom_def::{ML_DONTPEGBOTTOM, ML_MAPPED};
use crate::level_data::map_data::MapData;
use crate::level_data::map_defs::Segment;
use crate::player::Player;
use crate::renderer::bsp::RenderData;
use crate::renderer::defs::{DrawSeg, MAXDRAWSEGS};
use crate::{point_to_dist, scale_from_view_angle};
use std::f32::EPSILON;

// angle_t rw_normalangle; // From global angle? R_ScaleFromGlobalAngle
// // angle to line origin
// int rw_angle1; // SHARED, PASS AS AN ARG to segs.c functions
// lighttable_t **walllights; // Set in R_SetupFrame?
// short *maskedtexturecol;

// TODO: possibly init this once then use a `clear` func when new is required
/// All of the state in this struct is unique to it as it is used once per seg
/// to be rendered.
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

    worldtop:    f32,
    worldbottom: f32,
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

            worldtop: 0.0,
            worldbottom: 0.0,
            worldhigh: 0,
            worldlow: 0,
        }
    }

    /// R_StoreWallRange - r_segs
    pub fn store_wall_range(
        &mut self,
        start: i32,
        stop: i32,
        rdata: &mut RenderData,
        canvas: &mut Canvas<Surface>,
    ) {
        // Keep original Doom behaviour here
        if rdata.drawsegs.len() >= MAXDRAWSEGS {
            return;
        }

        if start >= 320 || start > stop {
            println!("Bad R_RenderWallRange: {} to {}", start, stop);
            return;
        }

        // These need only be locally defined to make some things easier
        let sidedef = self.seg.sidedef.clone();
        let mut linedef = self.seg.linedef.clone();

        // mark the segment as visible for auto level
        linedef.flags |= ML_MAPPED as i16;

        self.rw_normalangle = self.seg.angle;
        self.rw_normalangle += FRAC_PI_2;
        let offsetangle = self.rw_normalangle - rdata.rw_angle1; // radians

        // Unrequired with full angle range
        // if offsetangle > FRAC_PI_2 {
        //     offsetangle = FRAC_PI_2;
        // }

        let distangle = Angle::new(FRAC_PI_2 - offsetangle.rad());
        let hyp = point_to_dist(
            self.seg.v1.x(),
            self.seg.v1.y(),
            self.object.mobj.as_ref().unwrap().obj.xy,
        ); // verified correct
        self.rw_distance = hyp * distangle.sin(); // COrrect??? Seems to be...

        let mut ds_p = DrawSeg::new(NonNull::from(self.seg));

        // viewangle = player->mo->angle + viewangleoffset; // offset can be 0, 90, 270
        let view_angle = self.object.mobj.as_ref().unwrap().obj.angle;

        //m_ScreenXToAngle[i] = atan((m_HalfScreenWidth - i) / (float)m_iDistancePlayerToScreen) * 180 / PI;
        // TODO: doublecheck the angles and bounds
        let visangle =
            view_angle + CLASSIC_SCREEN_X_TO_VIEW[start as usize] * PI / 180.0; // degrees not rads
        let scale1 = scale_from_view_angle(
            visangle,
            self.rw_normalangle,
            self.rw_distance,
            view_angle,
        );

        let visangle =
            view_angle + CLASSIC_SCREEN_X_TO_VIEW[stop as usize] * PI / 180.0;
        let scale2 = scale_from_view_angle(
            visangle,
            self.rw_normalangle,
            self.rw_distance,
            view_angle,
        );

        ds_p.scale1 = scale1;
        ds_p.scale2 = scale2;
        ds_p.x1 = start;
        self.rw_x = start;
        ds_p.x2 = stop;
        self.rw_stopx = stop + 1;

        // testing draws
        self.rw_scalestep = (scale2 - scale1) / (stop - start) as f32;
        ds_p.scalestep = self.rw_scalestep;

        // calculate texture boundaries
        //  and decide if floor / ceiling marks are needed
        // `seg.sidedef.sector` is the front sector
        let viewz = self.object.viewz;
        self.worldtop = self.seg.sidedef.sector.ceilingheight - viewz;
        self.worldbottom = self.seg.sidedef.sector.floorheight - viewz;

        // These are all zeroed to start with, thanks rust.
        // midtexture = toptexture = bottomtexture = maskedtexture = 0;

        let vtop = 0.0;
        if self.seg.linedef.back_sidedef.is_none() {
            // single sided line
            // TODO: Need to R_InitTextures and figure out where to put this
            //self.midtexture = texturetranslation[sidedef.middle_tex];
            self.markfloor = true;
            self.markceiling = true;
            if linedef.flags & ML_DONTPEGBOTTOM as i16 != 0 {
                // TODO: textureheight
                //vtop = self.seg.sidedef.sector.floor_height + textureheight[self.seg.sidedef.middle_tex];
                //self.rw_midtexturemid = vtop - viewz;
            } else {
                self.rw_midtexturemid = self.worldtop;
            }
            self.rw_midtexturemid += self.seg.sidedef.rowoffset;
        }

        // ds_p is pointer to item in drawsegs, which is set by R_ClearDrawSegs
        // is drawseg_t which needs to be in an easy ref location, reffed in
        // - r_segs.c - uses it extensively, ds_p++; at end
        // - r_plane.c - only checks for an overflow?]
        // - r_bsp.c - sets t point to first element of drawsegs array

        self.topstep = -(self.worldtop as f32 * self.rw_scalestep);
        self.topfrac = 100.0 - (self.worldtop as f32 * scale1);

        self.bottomstep = -(self.worldbottom as f32 * self.rw_scalestep);
        self.bottomfrac = 100.0 - (self.worldbottom as f32 * scale1);

        // testing lighting
        let mut lightnum =
            self.seg.linedef.front_sidedef.sector.lightlevel as u8 >> 4;

        if (self.seg.v1.y() - self.seg.v2.y()).abs() < EPSILON {
            if lightnum > 5 {
                lightnum -= 5;
            }
        } else if (self.seg.v1.x() - self.seg.v2.x()).abs() < EPSILON
            && lightnum < 249
        {
            lightnum += 5;
        }

        let z = self.seg.sidedef.sector.floorheight.abs() as u8 / 2;

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
