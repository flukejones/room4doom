use sdl2::{rect::Rect, render::Canvas, surface::Surface};
use std::{
    f32::consts::{FRAC_PI_2, PI},
    ptr::NonNull,
};

use crate::angle::{Angle, CLASSIC_SCREEN_X_TO_VIEW};
use crate::doom_def::{ML_DONTPEGBOTTOM, ML_MAPPED};
use crate::level_data::map_defs::Segment;
use crate::p_map_object::MapObject;
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
    worldhigh:   f32,
    worldlow:    f32,
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
            worldhigh:   0.0,
            worldlow:    0.0,
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

        // mark the segment as visible for automap
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
            self.midtexture = sidedef.midtexture as i32;
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

            self.worldhigh = backsector.ceilingheight - viewz;
            self.worldlow = backsector.floorheight - viewz;

            // TODO: hack to allow height changes in outdoor areas
            //  if (frontsector->ceilingpic == skyflatnum && backsector->ceilingpic == skyflatnum)
            // 	{ worldtop = worldhigh; }

            // Checks to see if panes need updating?
            if self.worldlow != self.worldbottom
                || backsector.floorpic != frontsector.floorpic
            {
                self.markfloor = true;
            } else {
                // same plane on both sides
                self.markfloor = false;
            }
            //
            if self.worldhigh != self.worldtop
                || backsector.ceilingpic != frontsector.ceilingpic
                || backsector.lightlevel != frontsector.lightlevel
            {
                self.markceiling = true;
            } else {
                // same plane on both sides
                self.markceiling = false;
            }

            if backsector.ceilingheight <= frontsector.floorheight
                || backsector.floorheight >= frontsector.ceilingheight
            {
                // closed door
                self.markceiling = true;
                self.markfloor = true;
            }

            if self.worldhigh < self.worldtop {
                // TODO: texture stuff
                //  toptexture = texturetranslation[sidedef->toptexture];
                self.toptexture = sidedef.toptexture as i32;
            }

            if self.worldlow > self.worldbottom {
                // TODO: texture stuff
                //  bottomtexture = texturetranslation[sidedef->bottomtexture];
                self.bottomtexture = sidedef.bottomtexture as i32;
            }

            self.rw_toptexturemid += sidedef.rowoffset;
            self.rw_bottomtexturemid += sidedef.rowoffset;

            if sidedef.midtexture != 0 {
                self.maskedtexture = true;
                ds_p.maskedtexturecol = sidedef.midtexture;
                // TODO: ds_p->maskedtexturecol = maskedtexturecol = lastopening - rw_x;
                // lastopening += rw_stopx - rw_x;
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
        self.topstep = -(self.worldtop * self.rw_scalestep);
        self.topfrac = 100.0 - (self.worldtop * self.rw_scale);

        self.bottomstep = -(self.worldbottom * self.rw_scalestep);
        self.bottomfrac = 100.0 - (self.worldbottom * self.rw_scale);

        if seg.backsector.is_some() {
            if self.worldhigh < self.worldtop {
                self.pixhigh = 100.0 - (self.worldhigh * self.rw_scale);
                self.pixhighstep = -(self.worldhigh * self.rw_scalestep);
            }

            if self.worldlow > self.worldbottom {
                self.pixlow = 100.0 - (self.worldlow * self.rw_scale);
                self.pixlowstep = -(self.worldlow * self.rw_scalestep);
            }
        }

        self.render_seg_loop(object, seg, rdata, canvas);
    }

    fn render_seg_loop(
        &mut self,
        player: &Player,
        seg: &Segment,
        rdata: &mut RenderData,
        canvas: &mut Canvas<Surface>,
    ) {
        //
        // TESTING STUFF
        //
        let mut lightnum =
            seg.linedef.front_sidedef.sector.lightlevel as u8 >> 2;

        if (seg.v1.y() - seg.v2.y()).abs() < EPSILON {
            if lightnum > 5 {
                lightnum -= 5;
            }
        } else if (seg.v1.x() - seg.v2.x()).abs() < EPSILON && lightnum < 249 {
            lightnum += 5;
        }

        let z = seg.sidedef.sector.floorheight.abs() as u8 / 2;

        let colour = sdl2::pixels::Color::RGBA(
            100 + (self.midtexture * 5) as u8 + lightnum - (z >> 2) as u8,
            100 + (self.toptexture * 5) as u8 + lightnum - (z >> 2) as u8,
            100 + (self.bottomtexture * 5) as u8 + lightnum - (z >> 2) as u8,
            255,
        );
        canvas.set_draw_color(colour);

        // R_RenderSegLoop
        let mut yl;
        let mut yh;
        let mut top;
        let mut bottom;
        let mut mid;
        while self.rw_x < self.rw_stopx {
            yl = self.topfrac + 1.0;
            if yl < rdata.portal_clip.ceilingclip[self.rw_x as usize] + 1.0 {
                yl = rdata.portal_clip.ceilingclip[self.rw_x as usize] + 1.0;
            }

            if self.markceiling {
                top = rdata.portal_clip.ceilingclip[self.rw_x as usize] + 1.0;
                bottom = yl - 1.0;

                if bottom >= rdata.portal_clip.floorclip[self.rw_x as usize] {
                    bottom =
                        rdata.portal_clip.floorclip[self.rw_x as usize] - 1.0;
                }
                if top <= bottom {
                    // TODO: ceilingplane
                }
            }

            yh = self.bottomfrac;
            if yh >= rdata.portal_clip.floorclip[self.rw_x as usize] - 1.0 {
                yh = rdata.portal_clip.floorclip[self.rw_x as usize] - 1.0;
            }

            if self.markfloor {
                top = yh + 1.0;
                bottom = rdata.portal_clip.floorclip[self.rw_x as usize] - 1.0;
                if top <= rdata.portal_clip.ceilingclip[self.rw_x as usize] {
                    top =
                        rdata.portal_clip.ceilingclip[self.rw_x as usize] + 1.0;
                }
                if top <= bottom {
                    // TODO: floorplane
                }
            }

            if !self.segtextured {
                continue;
            }

            if self.midtexture != 0 && yh > yl {
                canvas
                    .draw_line((self.rw_x, yl as i32), (self.rw_x, yh as i32))
                    .unwrap();

                rdata.portal_clip.ceilingclip[self.rw_x as usize] =
                    SCREENHEIGHT as f32;
                rdata.portal_clip.floorclip[self.rw_x as usize] = -1.0;
            } else {
                if self.toptexture != 0 {
                    mid = self.pixhigh;
                    self.pixhigh += self.pixhighstep;

                    if mid >= rdata.portal_clip.floorclip[self.rw_x as usize] {
                        mid = rdata.portal_clip.floorclip[self.rw_x as usize]
                            - 1.0;
                    }

                    if mid >= yl {
                        // TODO: temporary?
                        if seg.linedef.point_on_side(
                            &player.mobj.as_ref().unwrap().obj.xy,
                        ) == 0
                        {
                            canvas
                                .draw_line(
                                    (self.rw_x, yl as i32),
                                    (self.rw_x, mid as i32),
                                )
                                .unwrap();
                        }

                        rdata.portal_clip.ceilingclip[self.rw_x as usize] = mid;
                    } else {
                        rdata.portal_clip.ceilingclip[self.rw_x as usize] =
                            yl - 1.0;
                    }
                } else if self.markceiling {
                    rdata.portal_clip.ceilingclip[self.rw_x as usize] =
                        yl - 1.0;
                }

                if self.bottomtexture != 0 {
                    mid = self.pixlow;
                    self.pixlow += self.pixlowstep;

                    if mid <= rdata.portal_clip.ceilingclip[self.rw_x as usize]
                    {
                        mid = rdata.portal_clip.ceilingclip[self.rw_x as usize]
                            + 1.0;
                    }

                    if mid <= yh {
                        // TODO: temporary?
                        if seg.linedef.point_on_side(
                            &player.mobj.as_ref().unwrap().obj.xy,
                        ) == 0
                        {
                            canvas
                                .draw_line(
                                    (self.rw_x, yh as i32),
                                    (self.rw_x, mid as i32),
                                )
                                .unwrap();
                        }

                        rdata.portal_clip.floorclip[self.rw_x as usize] = mid;
                    } else {
                        rdata.portal_clip.floorclip[self.rw_x as usize] =
                            yh + 1.0;
                    }
                } else if self.markfloor {
                    rdata.portal_clip.floorclip[self.rw_x as usize] = yh + 1.0;
                }
            }

            self.rw_x += 1;
            self.rw_scale += self.rw_scalestep;
            self.topfrac += self.topstep;
            self.bottomfrac += self.bottomstep;
        }
    }

    /// A column is a vertical slice/span from a wall texture that,
    ///  given the DOOM style restrictions on the view orientation,
    ///  will always have constant z depth.
    /// Thus a special case loop for very fast rendering can
    ///  be used. It has also been used with Wolfenstein 3D.
    fn draw_column(&self, yh: i32, yl: i32, canvas: &mut Canvas<Surface>) {
        // let mut count = yh - yl;
        // let mut frac = 0.0;
        // let mut fracstep;
        //
        // while count != 0 {
        //     canvas.draw_line((self.rw_x, yl), (self.rw_x, yh)).unwrap();
        //     count -= 1;
        // }
    }
}
