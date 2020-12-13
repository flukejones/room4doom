use sdl2::{rect::Rect, render::Canvas, surface::Surface};
use std::{
    f32::consts::{FRAC_PI_2, PI},
    ptr::NonNull,
};

use crate::angle::{Angle, CLASSIC_SCREEN_X_TO_VIEW};
use crate::doom_def::{ML_DONTPEGBOTTOM, ML_MAPPED};
use crate::level_data::map_defs::Segment;
use crate::player::Player;
use crate::renderer::defs::{
    DrawSeg, MAXDRAWSEGS, SCREENHEIGHT, SIL_BOTH, SIL_BOTTOM, SIL_NONE, SIL_TOP,
};
use crate::renderer::RenderData;
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
#[derive(Default)]
pub(crate) struct SegRender {
    /// Current segment, e.g, `curline` in Doom src. We can use this to get the
    /// `sector_t *frontsector;` `sector_t *backsector;` shared variables between
    /// `r_bsp.c` and `r_seg.c`.

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

impl SegRender {
    pub fn new() -> Self {
        SegRender {
            segtextured:         false,
            /// False if the back side is the same plane.
            markfloor:           false,
            markceiling:         false,
            maskedtexture:       false,
            // Texture ID's
            toptexture:          0,
            bottomtexture:       0,
            midtexture:          0,
            //
            rw_normalangle:      Angle::default(),
            // regular wall
            rw_x:                0,
            rw_stopx:            0,
            rw_centerangle:      Angle::default(),
            rw_offset:           0.0,
            rw_distance:         0.0, // In R_ScaleFromGlobalAngle? Compute when needed
            rw_scale:            0.0,
            rw_scalestep:        0.0,
            rw_midtexturemid:    0.0,
            rw_toptexturemid:    0.0,
            rw_bottomtexturemid: 0.0,

            pixhigh:     0.0,
            pixlow:      0.0,
            pixhighstep: 0.0,
            pixlowstep:  0.0,

            topfrac:    0.0,
            topstep:    0.0,
            bottomfrac: 0.0,
            bottomstep: 0.0,

            worldtop:    0.0,
            worldbottom: 0.0,
            worldhigh:   0,
            worldlow:    0,
        }
    }

    /// R_StoreWallRange - r_segs
    pub fn store_wall_range(
        &mut self,
        start: i32,
        stop: i32,
        seg: &Segment,
        object: &Player,
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
        let sidedef = seg.sidedef.clone();
        let mut linedef = seg.linedef.clone();

        // mark the segment as visible for auto level
        linedef.flags |= ML_MAPPED as i16;

        self.rw_normalangle = seg.angle;
        self.rw_normalangle += FRAC_PI_2;
        let mut offsetangle = self.rw_normalangle - rdata.rw_angle1; // radians

        // Unrequired with full angle range
        // if offsetangle > FRAC_PI_2 {
        //     offsetangle = FRAC_PI_2;
        // }

        let distangle = Angle::new(FRAC_PI_2 - offsetangle.rad());
        let hyp = point_to_dist(
            seg.v1.x(),
            seg.v1.y(),
            object.mobj.as_ref().unwrap().obj.xy,
        ); // verified correct
        self.rw_distance = hyp * distangle.sin(); // COrrect??? Seems to be...

        let mut ds_p = DrawSeg::new(NonNull::from(seg));

        // viewangle = player->mo->angle + viewangleoffset; // offset can be 0, 90, 270
        let view_angle = object.mobj.as_ref().unwrap().obj.angle;

        //m_ScreenXToAngle[i] = atan((m_HalfScreenWidth - i) / (float)m_iDistancePlayerToScreen) * 180 / PI;
        // TODO: doublecheck the angles and bounds
        let visangle =
            view_angle + CLASSIC_SCREEN_X_TO_VIEW[start as usize] * PI / 180.0; // degrees not rads
        self.rw_scale = scale_from_view_angle(
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

        ds_p.scale1 = self.rw_scale;
        ds_p.scale2 = scale2;
        ds_p.x1 = start;
        self.rw_x = start;
        ds_p.x2 = stop;
        self.rw_stopx = stop + 1;

        // testing draws
        self.rw_scalestep = (scale2 - self.rw_scale) / (stop - start) as f32;
        ds_p.scalestep = self.rw_scalestep;

        // calculate texture boundaries
        //  and decide if floor / ceiling marks are needed
        // `seg.sidedef.sector` is the front sector
        let frontsector = &seg.frontsector;
        let viewz = object.viewz;
        self.worldtop = frontsector.ceilingheight - viewz;
        self.worldbottom = frontsector.floorheight - viewz;

        // These are all zeroed to start with, thanks rust.
        // midtexture = toptexture = bottomtexture = maskedtexture = 0;

        let vtop = 0.0;
        if seg.backsector.is_none() {
            // single sided line
            // TODO: Need to R_InitTextures and figure out where to put this
            //self.midtexture = texturetranslation[sidedef.middle_tex];
            self.midtexture = 1;
            self.markfloor = true;
            self.markceiling = true;
            if linedef.flags & ML_DONTPEGBOTTOM as i16 != 0 {
                // TODO: textureheight
                //vtop = seg.sidedef.sector.floor_height + textureheight[seg.sidedef.middle_tex];
                self.rw_midtexturemid = vtop - viewz;
            } else {
                // top of texture at top
                self.rw_midtexturemid = self.worldtop;
            }
            self.rw_midtexturemid += seg.sidedef.rowoffset;

            ds_p.silhouette = SIL_BOTH;
            // TODO: ds_p.sprtopclip = screenheightarray;
            // TODO: ds_p.sprbottomclip = negonearray;
            ds_p.bsilheight = f32::MAX;
            ds_p.tsilheight = f32::MAX;
        } else {
            let backsector = seg.backsector.as_ref().unwrap();
            // two sided line
            // TODO: when thing render started
            //  ds_p->sprtopclip = ds_p->sprbottomclip = NULL;
            ds_p.silhouette = SIL_NONE;

            if frontsector.floorheight > backsector.floorheight {
                ds_p.silhouette = SIL_BOTTOM;
                ds_p.bsilheight = frontsector.floorheight;
            } else if backsector.floorheight > viewz {
                ds_p.silhouette = SIL_BOTTOM;
                ds_p.bsilheight = f32::MAX;
            }

            if frontsector.ceilingheight < backsector.ceilingheight {
                ds_p.silhouette = SIL_TOP;
                ds_p.tsilheight = frontsector.ceilingheight;
            } else if backsector.ceilingheight < viewz {
                ds_p.silhouette = SIL_TOP;
                ds_p.bsilheight = f32::MIN;
            }

            if backsector.ceilingheight <= frontsector.floorheight {
                // TODO: ds_p->sprbottomclip = negonearray;
                ds_p.silhouette = SIL_BOTTOM;
                ds_p.bsilheight = f32::MAX;
            }

            if backsector.floorheight >= frontsector.ceilingheight {
                // TODO: ds_p->sprtopclip = screenheightarray;
                ds_p.silhouette = SIL_TOP;
                ds_p.bsilheight = f32::MIN;
            }

            self.worldhigh = (backsector.ceilingheight - viewz) as i32;
            self.worldlow = (backsector.floorheight - viewz) as i32;

            // TODO: hack to allow height changes in outdoor areas
            //  if (frontsector->ceilingpic == skyflatnum && backsector->ceilingpic == skyflatnum)
            // 	{ worldtop = worldhigh; }

            // Checks to see if panes need updating?
            if self.worldlow != self.worldbottom as i32
                || backsector.floorpic != frontsector.floorpic
                || backsector.lightlevel != frontsector.lightlevel
            {
                self.markfloor = true;
            } else {
                self.markfloor = false;
            }
            //
            if self.worldhigh != self.worldtop as i32
                || backsector.ceilingpic != frontsector.ceilingpic
            {
                self.markceiling = true;
            } else {
                self.markceiling = false;
            }

            if self.worldhigh < self.worldtop as i32 {
                // TODO: texture stuff
                //  toptexture = texturetranslation[sidedef->toptexture];
                self.toptexture = 2;
            }

            if self.worldlow > self.worldbottom as i32 {
                // TODO: texture stuff
                //  bottomtexture = texturetranslation[sidedef->bottomtexture];
                self.bottomtexture = 3;
            }

            self.rw_toptexturemid += sidedef.rowoffset;
            self.rw_bottomtexturemid += sidedef.rowoffset;

            if sidedef.midtexture != 0 {
                self.maskedtexture = true;
                // TODO: ds_p->maskedtexturecol = maskedtexturecol = lastopening - rw_x;
                //  lastopening += rw_stopx - rw_x;
            }
        }

        // calculate rw_offset (only needed for textured lines)
        if self.midtexture
            | self.toptexture
            | self.bottomtexture
            | self.maskedtexture as i32
            != 0
        {
            self.segtextured = true;
        }

        if self.segtextured {
            offsetangle = self.rw_normalangle - rdata.rw_angle1;

            if offsetangle.rad() > PI {
                offsetangle = -offsetangle;
            } else if offsetangle.rad() > FRAC_PI_2 {
                offsetangle = Angle::new(FRAC_PI_2);
            }

            let sine = offsetangle.sin();
            self.rw_offset = hyp * sine;

            if self.rw_offset - rdata.rw_angle1.rad() < PI {
                self.rw_offset = -self.rw_offset;
            }

            self.rw_offset += sidedef.textureoffset + seg.offset;
            self.rw_centerangle =
                Angle::new(FRAC_PI_2) + view_angle - self.rw_normalangle;

            // TODO: calculate light table
            //  use different light tables
            //  for horizontal / vertical / diagonal
            // OPTIMIZE: get rid of LIGHTSEGSHIFT globally
            // if (!fixedcolormap)
            // {
            //     lightnum = (frontsector->lightlevel >> LIGHTSEGSHIFT) + extralight;
            //
            //     if (curline->v1->y == curline->v2->y)
            //     lightnum--;
            //     else if (curline->v1->x == curline->v2->x)
            //     lightnum++;
            //
            //     if (lightnum < 0)
            //     walllights = scalelight[0];
            //     else if (lightnum >= LIGHTLEVELS)
            //     walllights = scalelight[LIGHTLEVELS - 1];
            //     else
            //     walllights = scalelight[lightnum];
            // }
        }

        // if a floor / ceiling plane is on the wrong side
        //  of the view plane, it is definitely invisible
        //  and doesn't need to be marked.
        if frontsector.floorheight >= viewz {
            // above view plane
            self.markfloor = false;
        }
        // TDOD: if frontsector.ceilingheight <= viewz && frontsector.ceilingpic != skyflatnum
        if frontsector.ceilingheight <= viewz {
            // below view plane
            self.markceiling = false;
        }

        // TODO: 100 is half VIEWHEIGHT. Need to sort this stuff out
        self.topstep = -(self.worldtop as f32 * self.rw_scalestep);
        self.topfrac = 100.0 - (self.worldtop as f32 * self.rw_scale);

        self.bottomstep = -(self.worldbottom as f32 * self.rw_scalestep);
        self.bottomfrac = 100.0 - (self.worldbottom as f32 * self.rw_scale);

        if seg.backsector.is_some() {
            if self.worldhigh < self.worldtop as i32 {
                self.pixhigh = 100.0 - (self.worldhigh as f32 * self.rw_scale);
                self.pixhighstep = -(self.worldhigh as f32 * self.rw_scalestep);
            }

            if self.worldlow > self.worldbottom as i32 {
                self.pixlow = 100.0 - (self.worldlow as f32 * self.rw_scale);
                self.pixlowstep = -(self.worldlow as f32 * self.rw_scalestep);
            }
        }

        self.render_seg_loop(stop, seg, rdata, canvas);
    }

    fn render_seg_loop(
        &mut self,
        stop: i32,
        seg: &Segment,
        rdata: &mut RenderData,
        canvas: &mut Canvas<Surface>,
    ) {
        //
        // TESTING STUFF
        //
        let mut lightnum =
            seg.linedef.front_sidedef.sector.lightlevel as u8 >> 4;

        if (seg.v1.y() - seg.v2.y()).abs() < EPSILON {
            if lightnum > 5 {
                lightnum -= 5;
            }
        } else if (seg.v1.x() - seg.v2.x()).abs() < EPSILON && lightnum < 249 {
            lightnum += 5;
        }

        let z = seg.sidedef.sector.floorheight.abs() as u8 / 2;

        let colour = sdl2::pixels::Color::RGBA(
            150 + lightnum - (z >> 2) as u8,
            130 + lightnum - (z >> 2) as u8,
            130 + lightnum - (z >> 2) as u8,
            255,
        );
        canvas.set_draw_color(colour);

        // R_RenderSegLoop
        let mut yl;
        let mut yh;
        let mut top;
        let mut bottom;
        let mut mid;
        while self.rw_x <= stop {
            yl = (self.topfrac - 1.0) as i32;
            if yl < rdata.portal_clip.ceilingclip[self.rw_x as usize] + 1 {
                yl = rdata.portal_clip.ceilingclip[self.rw_x as usize] + 1;
            }

            if self.markceiling {
                top = rdata.portal_clip.ceilingclip[self.rw_x as usize] + 1;
                bottom = yl - 1;

                if bottom >= rdata.portal_clip.floorclip[self.rw_x as usize] {
                    bottom =
                        rdata.portal_clip.floorclip[self.rw_x as usize] - 1;
                }
                if top <= bottom {
                    // TODO: ceilingplane
                }
            }

            yh = self.bottomfrac as i32;
            if yh >= rdata.portal_clip.floorclip[self.rw_x as usize] - 1 {
                yh = rdata.portal_clip.floorclip[self.rw_x as usize] - 1;
            }

            if self.markfloor {
                top = yh + 1;
                bottom = rdata.portal_clip.floorclip[self.rw_x as usize] - 1;
                if top <= rdata.portal_clip.ceilingclip[self.rw_x as usize] {
                    top = rdata.portal_clip.ceilingclip[self.rw_x as usize] + 1;
                }
                if top <= bottom {
                    // TODO: floorplane
                }
            }

            if self.midtexture != 0 && yh > yl {
                let rect = Rect::new(
                    self.rw_x,
                    yl,
                    1,
                    (yh - yl) as u32, // WOAH! floating point rounding stuff
                );
                canvas.fill_rect(rect).unwrap();

                rdata.portal_clip.ceilingclip[self.rw_x as usize] =
                    SCREENHEIGHT as i32;
                rdata.portal_clip.floorclip[self.rw_x as usize] = -1;
            } else {
                if self.toptexture != 0 {
                    mid = self.pixhigh as i32;
                    self.pixhigh += self.pixhighstep;

                    if mid >= rdata.portal_clip.floorclip[self.rw_x as usize] {
                        mid =
                            rdata.portal_clip.floorclip[self.rw_x as usize] - 1;
                    }

                    if mid >= yl {
                        let rect = Rect::new(
                            self.rw_x,
                            yl,
                            1,
                            (mid - yl) as u32, // WOAH! floating point rounding stuff
                        );
                        canvas.fill_rect(rect).unwrap();

                        rdata.portal_clip.ceilingclip[self.rw_x as usize] = mid;
                    } else {
                        rdata.portal_clip.ceilingclip[self.rw_x as usize] =
                            yl - 1;
                    }
                } else if self.markceiling {
                    rdata.portal_clip.ceilingclip[self.rw_x as usize] = yl - 1;
                }

                if self.bottomtexture != 0 {
                    mid = self.pixlow as i32;
                    self.pixlow += self.pixlowstep;

                    if mid <= rdata.portal_clip.ceilingclip[self.rw_x as usize]
                    {
                        mid = rdata.portal_clip.ceilingclip[self.rw_x as usize]
                            + 1;
                    }

                    if mid <= yh {
                        let rect = Rect::new(
                            self.rw_x,
                            mid,
                            1,
                            (yh - mid) as u32, // WOAH! floating point rounding stuff
                        );
                        canvas.fill_rect(rect).unwrap();

                        rdata.portal_clip.floorclip[self.rw_x as usize] = mid;
                    } else {
                        rdata.portal_clip.floorclip[self.rw_x as usize] =
                            yh + 1;
                    }
                } else if self.markfloor {
                    rdata.portal_clip.floorclip[self.rw_x as usize] = yh + 1;
                }
            }

            self.rw_x += 1;
            self.rw_scale += self.rw_scalestep;
            self.topfrac += self.topstep;
            self.bottomfrac += self.bottomstep;
        }
    }
}
