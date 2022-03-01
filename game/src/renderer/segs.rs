use doom_lib::{Angle, Player, Segment, ML_DONTPEGBOTTOM, ML_DONTPEGTOP, ML_MAPPED};
use sdl2::{rect::Rect, render::Canvas, surface::Surface};
use std::{
    f32::consts::{FRAC_PI_2, PI},
    ptr::NonNull,
};

use crate::{
    renderer::{
        defs::{DrawSeg, MAXDRAWSEGS, SCREENHEIGHT, SIL_BOTH, SIL_BOTTOM, SIL_NONE, SIL_TOP},
        RenderData,
    },
    utilities::{point_to_dist, scale_from_view_angle, CLASSIC_SCREEN_X_TO_VIEW},
};

// angle_t rw_normalangle; // From global angle? R_ScaleFromGlobalAngle
// // angle to line origin
// int rw_angle1; // SHARED, PASS AS AN ARG to segs.c functions
// lighttable_t **walllights; // Set in R_SetupFrame?
// short *maskedtexturecol;

// TODO: possibly init this once then use a `clear` func when new is required
/// All of the state in this struct is unique to it as it is used once per seg
/// to be rendered.
#[derive(Default)]
pub struct SegRender {
    /// Current segment, e.g, `curline` in Doom src. We can use this to get the
    /// `sector_t *frontsector;` `sector_t *backsector;` shared variables between
    /// `r_bsp.c` and `r_seg.c`.

    /// True if any of the segs textures might be visible.
    segtextured: bool,
    /// False if the back side is the same plane.
    markfloor: bool,
    markceiling: bool,
    maskedtexture: bool,
    // Texture ID's
    toptexture: i32,
    bottomtexture: i32,
    midtexture: i32,
    //
    rw_normalangle: Angle,
    // regular wall
    rw_x: i32,
    rw_stopx: i32,
    rw_centerangle: Angle,
    rw_offset: f32,
    rw_distance: f32, // In R_ScaleFromGlobalAngle? Compute when needed
    rw_scale: f32,
    rw_scalestep: f32,
    rw_midtexturemid: f32,
    rw_toptexturemid: f32,
    rw_bottomtexturemid: f32,

    pixhigh: f32,
    pixlow: f32,
    pixhighstep: f32,
    pixlowstep: f32,

    topfrac: f32,
    topstep: f32,
    bottomfrac: f32,
    bottomstep: f32,

    worldtop: f32,
    worldbottom: f32,
    worldhigh: f32,
    worldlow: f32,

    /// Lightmap index
    wall_lights: usize,
    /// Index to the colourmap sof wall_lights to use
    colourmap: usize,
}

impl SegRender {
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
        if rdata.drawsegs.len() > MAXDRAWSEGS {
            return;
        }

        if start >= 320 || start > stop {
            panic!("Bad R_RenderWallRange: {} to {}", start, stop);
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
        let mobj = unsafe { object.mobj.as_ref().unwrap().as_ref() };

        let distangle = Angle::new(FRAC_PI_2 - offsetangle.rad());
        let hyp = point_to_dist(seg.v1.x(), seg.v1.y(), mobj.xy); // verified correct
        self.rw_distance = hyp * distangle.sin(); // Correct??? Seems to be...

        let mut ds_p = DrawSeg::new(NonNull::from(seg));

        // viewangle = player->mo->angle + viewangleoffset; // offset can be 0, 90, 270
        let view_angle = mobj.angle;

        // TODO: doublecheck the angles and bounds
        let visangle = view_angle + CLASSIC_SCREEN_X_TO_VIEW[start as usize] * PI / 180.0; // degrees not rads
        self.rw_scale =
            scale_from_view_angle(visangle, self.rw_normalangle, self.rw_distance, view_angle);

        let visangle = view_angle + CLASSIC_SCREEN_X_TO_VIEW[stop as usize] * PI / 180.0;
        ds_p.scale2 =
            scale_from_view_angle(visangle, self.rw_normalangle, self.rw_distance, view_angle);

        ds_p.scale1 = self.rw_scale;
        ds_p.x1 = start;
        self.rw_x = start;
        ds_p.x2 = stop;
        self.rw_stopx = stop;

        // testing draws
        self.rw_scalestep = (ds_p.scale2 - self.rw_scale) / (stop - start) as f32;
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

        if seg.backsector.is_none() {
            // single sided line
            self.midtexture = sidedef.midtexture as i32;
            self.markfloor = true;
            self.markceiling = true;
            if linedef.flags as u32 & ML_DONTPEGBOTTOM != 0 && seg.sidedef.midtexture != usize::MAX
            {
                let texture = &rdata.textures[seg.sidedef.midtexture];
                let texture_column = get_column(texture, 0);
                let mut vtop = frontsector.floorheight + texture_column.len() as f32;
                if vtop < frontsector.floorheight + frontsector.ceilingheight {
                    vtop = frontsector.floorheight + frontsector.ceilingheight
                }
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
            if self.worldlow != self.worldbottom || backsector.floorpic != frontsector.floorpic {
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

                if linedef.flags as u32 & ML_DONTPEGTOP != 0 {
                    self.rw_toptexturemid = self.worldtop;
                } else if seg.sidedef.toptexture != usize::MAX {
                    let texture = &rdata.textures[seg.sidedef.toptexture];
                    let texture_column = get_column(texture, 0);
                    let vtop = backsector.ceilingheight + texture_column.len() as f32;
                    self.rw_toptexturemid = vtop - viewz;
                } else {
                    self.rw_toptexturemid = self.worldtop;
                }
            }

            if self.worldlow > self.worldbottom {
                // TODO: texture stuff
                //  bottomtexture = texturetranslation[sidedef->bottomtexture];
                self.bottomtexture = sidedef.bottomtexture as i32;

                if linedef.flags as u32 & ML_DONTPEGBOTTOM != 0 {
                    self.rw_bottomtexturemid = self.worldtop;
                } else {
                    self.rw_bottomtexturemid = self.worldlow;
                }
            }

            self.rw_toptexturemid += sidedef.rowoffset;
            self.rw_bottomtexturemid += sidedef.rowoffset;

            if sidedef.midtexture != 0 {
                self.maskedtexture = true;
                ds_p.maskedtexturecol = sidedef.midtexture as i16;
                // TODO: ds_p->maskedtexturecol = maskedtexturecol = lastopening - rw_x;
                // lastopening += rw_stopx - rw_x;
            }
        }

        // calculate rw_offset (only needed for textured lines)
        if self.midtexture | self.toptexture | self.bottomtexture | self.maskedtexture as i32 != 0 {
            self.segtextured = true;
        }

        if self.segtextured {
            offsetangle = self.rw_normalangle - rdata.rw_angle1;

            // if offsetangle.rad() > PI {
            //     offsetangle = -offsetangle;
            // }
            // dbg!(offsetangle);

            self.rw_offset = hyp * offsetangle.sin();

            //if self.rw_normalangle.rad() - rdata.rw_angle1.rad() < PI * 2.0 {
            self.rw_offset = -self.rw_offset;
            //}

            self.rw_offset += sidedef.textureoffset + seg.offset;
            self.rw_centerangle = view_angle - self.rw_normalangle;

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
            let mut lightnum = seg.sidedef.sector.lightlevel as u8 >> 4;

            if seg.v1.y() == seg.v2.y() {
                if lightnum > 1 {
                    lightnum -= 1;
                }
            } else if (seg.v1.x() == seg.v2.x()) && lightnum < 15 {
                lightnum += 1;
                // walllights = scalelight[lightnum];, where scalelight = lighttable_t *[16][48]
            }
            //wall_lights = rdata.get_lightscale(lightnum as usize).to_owned();
            self.wall_lights = lightnum as usize;
        }

        // if a floor / ceiling plane is on the wrong side
        //  of the view plane, it is definitely invisible
        //  and doesn't need to be marked.
        if frontsector.floorheight >= viewz {
            // above view plane
            self.markfloor = false;
        }
        // TODO: if frontsector.ceilingheight <= viewz && frontsector.ceilingpic != skyflatnum
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

        // render it
        if self.markceiling {
            // ceilingplane = R_CheckPlane(ceilingplane, self.rw_x, self.rw_stopx - 1);
        }

        if self.markfloor {
            // floorplane = R_CheckPlane(floorplane, self.rw_x, self.rw_stopx - 1);
        }

        self.render_seg_loop(object, seg, rdata, canvas);
    }

    /// Doom function name `R_RenderSegLoop`
    fn render_seg_loop(
        &mut self,
        player: &Player,
        seg: &Segment,
        rdata: &mut RenderData,
        canvas: &mut Canvas<Surface>,
    ) {
        let mobj = unsafe { player.mobj.as_ref().unwrap().as_ref() };

        // R_RenderSegLoop
        let mut yl;
        let mut yh;
        let mut top;
        let mut bottom;
        let mut mid;
        let mut angle;
        let mut texture_column = 0;
        while self.rw_x <= self.rw_stopx {
            yl = self.topfrac - 1.0;
            if yl < rdata.portal_clip.ceilingclip[self.rw_x as usize] + 1.0 {
                yl = rdata.portal_clip.ceilingclip[self.rw_x as usize] + 1.0;
            }

            if self.markceiling {
                top = rdata.portal_clip.ceilingclip[self.rw_x as usize] + 1.0;
                bottom = yl - 1.0;

                if bottom >= rdata.portal_clip.floorclip[self.rw_x as usize] {
                    bottom = rdata.portal_clip.floorclip[self.rw_x as usize] - 1.0;
                }
                if top <= bottom {
                    // TODO: ceilingplane
                }
            }

            yh = self.bottomfrac;
            if yh >= rdata.portal_clip.floorclip[self.rw_x as usize] {
                yh = rdata.portal_clip.floorclip[self.rw_x as usize];
            }

            if self.markfloor {
                top = yh + 1.0;
                bottom = rdata.portal_clip.floorclip[self.rw_x as usize] - 1.0;
                if top <= rdata.portal_clip.ceilingclip[self.rw_x as usize] {
                    top = rdata.portal_clip.ceilingclip[self.rw_x as usize] + 1.0;
                }
                if top <= bottom {
                    // TODO: floorplane
                }
            }

            let mut dc_iscale = 0.0;
            if self.segtextured {
                angle =
                    self.rw_centerangle + CLASSIC_SCREEN_X_TO_VIEW[self.rw_x as usize] * PI / 180.0;
                texture_column = (self.rw_offset - angle.tan() * self.rw_distance) as usize;

                // Select colourmap to use (max should be 48)
                let mut index = (self.rw_scale * 17.0) as usize;
                dbg!(index);
                if index > 47 {
                    index = 47;
                }
                self.colourmap = index;

                dc_iscale = 1.0 / self.rw_scale;
            }

            if self.midtexture != 0 {
                if seg.sidedef.midtexture != usize::MAX {
                    let texture = &rdata.textures[seg.sidedef.midtexture];
                    let texture_column = get_column(texture, texture_column);
                    basic_draw_test(
                        texture_column,
                        &rdata.get_lightscale(self.wall_lights)[self.colourmap],
                        dc_iscale,
                        self.rw_x,
                        self.rw_midtexturemid,
                        yl as i32,
                        yh as i32,
                        rdata,
                        canvas,
                    );
                };

                rdata.portal_clip.ceilingclip[self.rw_x as usize] = SCREENHEIGHT as f32;
                rdata.portal_clip.floorclip[self.rw_x as usize] = -1.0;
            } else {
                if self.toptexture != 0 {
                    mid = self.pixhigh;
                    self.pixhigh += self.pixhighstep;

                    if mid >= rdata.portal_clip.floorclip[self.rw_x as usize] {
                        mid = rdata.portal_clip.floorclip[self.rw_x as usize];
                    }

                    if mid >= yl {
                        if seg.linedef.point_on_side(&mobj.xy) == 0
                            && seg.sidedef.toptexture != usize::MAX
                        {
                            let texture = &rdata.textures[seg.sidedef.toptexture];
                            let texture_column = get_column(texture, texture_column);
                            basic_draw_test(
                                texture_column,
                                &rdata.get_lightscale(self.wall_lights)[self.colourmap],
                                dc_iscale,
                                self.rw_x,
                                self.rw_toptexturemid,
                                yl as i32,
                                mid as i32,
                                rdata,
                                canvas,
                            );
                        }

                        rdata.portal_clip.ceilingclip[self.rw_x as usize] = mid;
                    } else {
                        rdata.portal_clip.ceilingclip[self.rw_x as usize] = yl; // - 1.0;
                    }
                } else if self.markceiling {
                    rdata.portal_clip.ceilingclip[self.rw_x as usize] = yl; // - 1.0;
                }

                if self.bottomtexture != 0 {
                    mid = self.pixlow;
                    self.pixlow += self.pixlowstep;

                    if mid <= rdata.portal_clip.ceilingclip[self.rw_x as usize] {
                        mid = rdata.portal_clip.ceilingclip[self.rw_x as usize];
                    }

                    if mid <= yh {
                        if seg.linedef.point_on_side(&mobj.xy) == 0
                            && seg.sidedef.bottomtexture != usize::MAX
                        {
                            let texture = &rdata.textures[seg.sidedef.bottomtexture];
                            let texture_column = get_column(texture, texture_column);
                            basic_draw_test(
                                texture_column,
                                &rdata.get_lightscale(self.wall_lights)[self.colourmap],
                                dc_iscale,
                                self.rw_x,
                                self.rw_bottomtexturemid,
                                mid as i32,
                                yh as i32,
                                rdata,
                                canvas,
                            );
                        }

                        rdata.portal_clip.floorclip[self.rw_x as usize] = mid;
                    } else {
                        rdata.portal_clip.floorclip[self.rw_x as usize] = yh;
                    }
                } else if self.markfloor {
                    rdata.portal_clip.floorclip[self.rw_x as usize] = yh;
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

fn get_column(texture: &[Vec<usize>], texture_column: usize) -> &[usize] {
    &texture[texture_column & (texture.len() - 1)]
}

#[allow(clippy::too_many_arguments)]
fn basic_draw_test(
    texture_column: &[usize],
    colourmap: &[usize],
    fracstep: f32,
    dc_x: i32,
    dc_texturemid: f32,
    yl: i32,
    yh: i32,
    rdata: &RenderData,
    canvas: &mut Canvas<Surface>,
) {
    //let scale = lightnum as f32 / 255.0;
    let mut frac = dc_texturemid + (yl as f32 - 100.0) * fracstep;

    for n in yl..=yh {
        if frac as usize & 127 > texture_column.len() - 1 {
            return;
        }
        let px = colourmap[texture_column[frac as usize & 127]];
        let colour = if px == usize::MAX {
            // ERROR COLOUR
            sdl2::pixels::Color::RGBA(255, 0, 0, 255)
        } else {
            let colour = &rdata.get_palette(0)[px];
            sdl2::pixels::Color::RGBA(colour.r, colour.g, colour.b, 255)
        };

        canvas.set_draw_color(colour);
        canvas.fill_rect(Rect::new(dc_x, n, 1, 1)).unwrap();

        frac += fracstep;
    }
}
