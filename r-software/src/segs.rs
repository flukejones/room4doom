use gameplay::{Angle, LineDefFlags, PicData, Player, Segment};
use rendering_traits::PixelBuf;
use std::{cell::RefCell, f32::consts::FRAC_PI_2, ptr::NonNull, rc::Rc};

use crate::utilities::{point_to_dist, scale_from_view_angle, CLASSIC_SCREEN_X_TO_VIEW};

use super::{
    defs::{DrawSeg, MAXDRAWSEGS, SCREENHEIGHT_HALF, SIL_BOTH, SIL_BOTTOM, SIL_NONE, SIL_TOP},
    RenderData,
};

const HEIGHTUNIT: f32 = 0.062485;

// angle_t rw_normalangle; // From global angle? R_ScaleFromGlobalAngle
// // angle to line origin
// int rw_angle1; // SHARED, PASS AS AN ARG to segs.c functions
// lighttable_t **walllights; // Set in R_SetupFrame?
// short *maskedtexturecol;

// TODO: possibly init this once then use a `clear` func when new is required
/// All of the state in this struct is unique to it as it is used once per seg
/// to be rendered.
pub(crate) struct SegRender {
    /// Current segment, e.g, `curline` in Doom src. We can use this to get the
    /// `sector_t *frontsector;` `sector_t *backsector;` shared variables between
    /// `r_bsp.c` and `r_seg.c`.

    /// True if any of the segs textures might be visible.
    segtextured: bool,
    /// False if the back side is the same plane.
    markfloor: bool,
    markceiling: bool,
    maskedtexture: bool,
    /// Index in to `openings` array
    maskedtexturecol: i32,
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

    worldtop: i32,
    worldbottom: i32,
    worldhigh: i32,
    worldlow: i32,

    /// Light level for the wall
    wall_lights: i32,

    texture_data: Rc<RefCell<PicData>>,
}

impl SegRender {
    pub fn new(texture_data: Rc<RefCell<PicData>>) -> Self {
        Self {
            segtextured: false,
            markfloor: false,
            markceiling: false,
            maskedtexture: false,
            maskedtexturecol: -1,
            toptexture: -1,
            bottomtexture: -1,
            midtexture: -1,
            rw_normalangle: Angle::default(),
            rw_x: 0,
            rw_stopx: 0,
            rw_centerangle: Angle::default(),
            rw_offset: 0.0,
            rw_distance: 0.0,
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
            wall_lights: 0,
            texture_data,
        }
    }

    /// R_StoreWallRange - r_segs
    pub fn store_wall_range(
        &mut self,
        start: i32,
        stop: i32,
        seg: &Segment,
        player: &Player,
        rdata: &mut RenderData,
        pixels: &mut PixelBuf,
    ) {
        // bounds check before getting ref
        if rdata.ds_p >= rdata.drawsegs.len() {
            rdata.drawsegs.push(DrawSeg::new(NonNull::from(seg)));
        }

        // Keep original Doom behaviour here
        if rdata.drawsegs.len() - 1 > MAXDRAWSEGS {
            return;
        }

        let mut ds_p = &mut rdata.drawsegs[rdata.ds_p];

        if !(0..320).contains(&start) || start > stop {
            panic!("Bad R_RenderWallRange: {} to {}", start, stop);
        }

        // These need only be locally defined to make some things easier
        let sidedef = seg.sidedef.clone();
        let mut linedef = seg.linedef.clone();

        // mark the segment as visible for automap
        linedef.flags |= LineDefFlags::Mapped as u32;

        self.rw_normalangle = seg.angle;
        self.rw_normalangle += FRAC_PI_2;
        let mut offsetangle = self.rw_normalangle - rdata.rw_angle1; // radians

        // Unrequired with full angle range
        // if offsetangle > FRAC_PI_2 {
        //     offsetangle = FRAC_PI_2;
        // }
        let mobj = unsafe { &*(player.mobj.unwrap()) };

        let distangle = Angle::new(FRAC_PI_2 - offsetangle.rad());
        let hyp = point_to_dist(seg.v1.x(), seg.v1.y(), mobj.xy); // verified correct
        self.rw_distance = hyp * distangle.sin(); // Correct??? Seems to be...

        // viewangle = player->mo->angle + viewangleoffset; // offset can be 0, 90, 270
        let view_angle = mobj.angle;

        // TODO: doublecheck the angles and bounds
        let visangle = view_angle + CLASSIC_SCREEN_X_TO_VIEW[start as usize].to_radians();
        self.rw_scale =
            scale_from_view_angle(visangle, self.rw_normalangle, self.rw_distance, view_angle);

        let visangle = view_angle + CLASSIC_SCREEN_X_TO_VIEW[stop as usize].to_radians();

        ds_p.scale1 = self.rw_scale;
        ds_p.x1 = start;
        self.rw_x = ds_p.x1;
        ds_p.x2 = stop;
        self.rw_stopx = stop + 1;

        if stop > start {
            // scale2 and rw_scale appears corrrect
            ds_p.scale2 =
                scale_from_view_angle(visangle, self.rw_normalangle, self.rw_distance, view_angle);

            self.rw_scalestep = (ds_p.scale2 - self.rw_scale) / (stop - start) as f32;
            ds_p.scalestep = self.rw_scalestep;
        } else {
            ds_p.scale2 = ds_p.scale1;
        }

        // calculate texture boundaries
        //  and decide if floor / ceiling marks are needed
        // `seg.sidedef.sector` is the front sector
        let frontsector = &seg.frontsector;
        let viewz = player.viewz;
        self.worldtop = (frontsector.ceilingheight - viewz) as i32;
        self.worldbottom = (frontsector.floorheight - viewz) as i32;

        self.midtexture = -1;
        self.toptexture = -1;
        self.bottomtexture = -1;
        self.maskedtexture = false;
        self.maskedtexturecol = -1;

        if seg.backsector.is_none() {
            let textures = &self.texture_data.borrow();
            // single sided line
            self.midtexture = sidedef.midtexture as i32;
            self.markfloor = true;
            self.markceiling = true;
            if linedef.flags & LineDefFlags::UnpegBottom as u32 != 0
                && seg.sidedef.midtexture != usize::MAX
            {
                let texture_column = textures.wall_pic_column(seg.sidedef.midtexture, 0);
                let vtop = frontsector.floorheight + texture_column.len() as f32 - 1.0;
                self.rw_midtexturemid = vtop - viewz;
            } else {
                // top of texture at top
                self.rw_midtexturemid = self.worldtop as f32;
            }
            self.rw_midtexturemid += seg.sidedef.rowoffset;

            ds_p.silhouette = SIL_BOTH;
            ds_p.sprtopclip = Some(0); // start of screenheightarray
            ds_p.sprbottomclip = Some(0); // start of negonearray
            ds_p.bsilheight = f32::MAX;
            ds_p.tsilheight = f32::MIN;
        } else {
            let textures = &self.texture_data.borrow();
            let backsector = seg.backsector.as_ref().unwrap();
            // two sided line
            // TODO: when thing render started
            ds_p.sprtopclip = None;
            ds_p.sprbottomclip = None;
            ds_p.silhouette = SIL_NONE;

            if frontsector.floorheight > backsector.floorheight {
                ds_p.silhouette = SIL_BOTTOM;
                ds_p.bsilheight = frontsector.floorheight;
            } else if backsector.floorheight > viewz {
                ds_p.silhouette = SIL_BOTTOM;
                ds_p.bsilheight = f32::MAX;
            }

            if frontsector.ceilingheight < backsector.ceilingheight {
                ds_p.silhouette |= SIL_TOP;
                ds_p.tsilheight = frontsector.ceilingheight;
            } else if backsector.ceilingheight < viewz {
                ds_p.silhouette |= SIL_TOP;
                ds_p.tsilheight = f32::MIN;
            }

            // Commented out as this seems to fix the incorrect clipping of
            // sprites lower/higher than player and blocked by lower or upper
            // part of portal
            // if backsector.ceilingheight <= frontsector.floorheight {
            //     ds_p.sprbottomclip = Some(0); // start of negonearray
            //     ds_p.silhouette |= SIL_BOTTOM;
            //     ds_p.bsilheight = f32::MAX;
            // }

            // if backsector.floorheight >= frontsector.ceilingheight {
            //     ds_p.sprtopclip = Some(0);
            //     ds_p.silhouette |= SIL_TOP;
            //     ds_p.tsilheight = f32::MIN;
            // }

            self.worldhigh = (backsector.ceilingheight - viewz).floor() as i32;
            self.worldlow = (backsector.floorheight - viewz).floor() as i32;

            // TODO: hack to allow height changes in outdoor areas
            if frontsector.ceilingpic == textures.sky_num()
                && backsector.ceilingpic == textures.sky_num()
            {
                self.worldtop = self.worldhigh;
            }

            // Checks to see if panes need updating?
            if self.worldlow != self.worldbottom
                || backsector.floorpic != frontsector.floorpic
                || backsector.lightlevel != frontsector.lightlevel
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
                self.toptexture = sidedef.toptexture as i32;
                if linedef.flags & LineDefFlags::UnpegTop as u32 != 0 {
                    self.rw_toptexturemid = self.worldtop as f32;
                } else if seg.sidedef.toptexture != usize::MAX {
                    let texture_column = textures.wall_pic_column(seg.sidedef.toptexture, 0);
                    let vtop = backsector.ceilingheight + texture_column.len() as f32 - 1.0;
                    self.rw_toptexturemid = vtop - viewz;
                }
            }

            if self.worldlow > self.worldbottom {
                self.bottomtexture = sidedef.bottomtexture as i32;
                if linedef.flags & LineDefFlags::UnpegBottom as u32 != 0 {
                    self.rw_bottomtexturemid = self.worldtop as f32;
                } else {
                    self.rw_bottomtexturemid = self.worldlow as f32;
                }
            }

            // TODO: how to deal with negative rowoffset
            self.rw_toptexturemid += sidedef.rowoffset;
            self.rw_bottomtexturemid += sidedef.rowoffset;

            if sidedef.midtexture != 0 {
                self.maskedtexture = true;
                // Set the indexes in to visplanes.openings
                self.maskedtexturecol = rdata.visplanes.lastopening - self.rw_x;
                ds_p.maskedtexturecol = self.maskedtexturecol;

                rdata.visplanes.lastopening += self.rw_stopx - self.rw_x;
            }
        }

        // calculate rw_offset (only needed for textured lines)
        if self.midtexture | self.toptexture | self.bottomtexture | self.maskedtexture as i32 != 0 {
            self.segtextured = true;
        }

        if self.segtextured {
            offsetangle = self.rw_normalangle - rdata.rw_angle1;
            self.rw_offset = hyp * offsetangle.sin();
            // if self.rw_normalangle.rad() - rdata.rw_angle1.rad() < PI * 2.0 {
            self.rw_offset = -self.rw_offset;
            //  }
            self.rw_offset += sidedef.textureoffset + seg.offset;
            self.rw_centerangle = view_angle - self.rw_normalangle;
            self.wall_lights = (seg.sidedef.sector.lightlevel >> 4) + player.extralight;
        }

        // if a floor / ceiling plane is on the wrong side
        //  of the view plane, it is definitely invisible
        //  and doesn't need to be marked.
        if frontsector.floorheight >= viewz {
            // above view plane
            self.markfloor = false;
        }

        if frontsector.ceilingheight <= viewz
            && frontsector.ceilingpic != self.texture_data.borrow().sky_num()
        {
            // below view plane
            self.markceiling = false;
        }

        self.topstep = -(self.worldtop as f32 * self.rw_scalestep);
        self.topfrac = 100.0 - (self.worldtop as f32 * self.rw_scale);

        self.bottomstep = -(self.worldbottom as f32 * self.rw_scalestep);
        self.bottomfrac = 100.0 - (self.worldbottom as f32 * self.rw_scale);

        if seg.backsector.is_some() {
            if self.worldhigh < self.worldtop {
                self.pixhigh = 100.0 + HEIGHTUNIT - (self.worldhigh as f32 * self.rw_scale);
                self.pixhighstep = -(self.worldhigh as f32 * self.rw_scalestep);
            }

            if self.worldlow > self.worldbottom {
                self.pixlow = 100.0 + HEIGHTUNIT - (self.worldlow as f32 * self.rw_scale);
                self.pixlowstep = -(self.worldlow as f32 * self.rw_scalestep);
            }
        }

        // render it
        if self.markceiling {
            rdata.visplanes.ceilingplane =
                rdata
                    .visplanes
                    .check_plane(self.rw_x, self.rw_stopx, rdata.visplanes.ceilingplane);
        }

        if self.markfloor {
            rdata.visplanes.floorplane =
                rdata
                    .visplanes
                    .check_plane(self.rw_x, self.rw_stopx, rdata.visplanes.floorplane);
        }

        self.render_seg_loop(seg, player.viewheight, rdata, pixels);

        let ds_p = &mut rdata.drawsegs[rdata.ds_p];
        if (ds_p.silhouette & SIL_TOP != 0 || self.maskedtexture) && ds_p.sprtopclip.is_none() {
            for (i, n) in rdata
                .portal_clip
                .ceilingclip
                .iter()
                .skip(start as usize)
                .enumerate()
            {
                let last = rdata.visplanes.lastopening as usize;
                rdata.visplanes.openings[last + i] = *n;
                if i as i32 > self.rw_stopx - start {
                    break;
                }
            }
            ds_p.sprtopclip = Some(rdata.visplanes.lastopening - start);
            rdata.visplanes.lastopening += self.rw_stopx - start;
        }

        if (ds_p.silhouette & SIL_BOTTOM != 0 || self.maskedtexture) && ds_p.sprbottomclip.is_none()
        {
            for (i, n) in rdata
                .portal_clip
                .floorclip
                .iter()
                .skip(start as usize)
                .enumerate()
            {
                let last = rdata.visplanes.lastopening as usize;
                rdata.visplanes.openings[last + i] = *n;
                if i as i32 > self.rw_stopx - start {
                    break;
                }
            }
            ds_p.sprbottomclip = Some(rdata.visplanes.lastopening - start);
            rdata.visplanes.lastopening += self.rw_stopx - start;
        }

        if ds_p.silhouette & SIL_TOP == 0 && self.maskedtexture {
            ds_p.silhouette |= SIL_TOP;
            ds_p.tsilheight = f32::MIN;
        }

        if ds_p.silhouette & SIL_BOTTOM == 0 && self.maskedtexture {
            ds_p.silhouette |= SIL_BOTTOM;
            ds_p.bsilheight = f32::MAX;
        }
        rdata.ds_p += 1;
    }

    /// Doom function name `R_RenderSegLoop`
    fn render_seg_loop(
        &mut self,
        seg: &Segment,
        view_height: f32,
        rdata: &mut RenderData,
        pixels: &mut PixelBuf,
    ) {
        // R_RenderSegLoop
        let mut yl: i32;
        let mut yh: i32;
        let mut top;
        let mut bottom;
        let mut mid;
        let mut angle;
        let mut texture_column = 0;
        while self.rw_x < self.rw_stopx {
            // yl = (topfrac + HEIGHTUNIT - 1) >> HEIGHTBITS;
            // Whaaaat?
            yl = (self.topfrac + HEIGHTUNIT) as i32 + 1;
            if yl < rdata.portal_clip.ceilingclip[self.rw_x as usize] + 1 {
                yl = rdata.portal_clip.ceilingclip[self.rw_x as usize] + 1;
            }

            if self.markceiling {
                top = rdata.portal_clip.ceilingclip[self.rw_x as usize] + 1;
                bottom = yl - 1;

                if bottom >= rdata.portal_clip.floorclip[self.rw_x as usize] {
                    bottom = rdata.portal_clip.floorclip[self.rw_x as usize] - 1;
                }
                if top <= bottom {
                    let ceil = rdata.visplanes.ceilingplane;
                    rdata.visplanes.visplanes[ceil].top[self.rw_x as usize] = top as u8;
                    rdata.visplanes.visplanes[ceil].bottom[self.rw_x as usize] = bottom as u8;
                }
            }

            yh = self.bottomfrac as i32;

            if yh >= rdata.portal_clip.floorclip[self.rw_x as usize] {
                yh = rdata.portal_clip.floorclip[self.rw_x as usize] - 1;
            }

            if self.markfloor {
                top = yh + 1;
                bottom = rdata.portal_clip.floorclip[self.rw_x as usize] - 1;

                if top <= rdata.portal_clip.ceilingclip[self.rw_x as usize] {
                    top = rdata.portal_clip.ceilingclip[self.rw_x as usize] + 1;
                }
                if top <= bottom {
                    let floor = rdata.visplanes.floorplane;
                    rdata.visplanes.visplanes[floor].top[self.rw_x as usize] = top as u8;
                    rdata.visplanes.visplanes[floor].bottom[self.rw_x as usize] = bottom as u8;
                }
            }

            let mut dc_iscale = 0.0;
            if self.segtextured {
                angle =
                    self.rw_centerangle + CLASSIC_SCREEN_X_TO_VIEW[self.rw_x as usize].to_radians();
                texture_column = (self.rw_offset - angle.tan() * self.rw_distance) as i32;

                dc_iscale = 1.0 / self.rw_scale;
            }

            if self.midtexture != -1 {
                if seg.sidedef.midtexture != usize::MAX {
                    let textures = &self.texture_data.borrow();
                    let texture_column =
                        textures.wall_pic_column(seg.sidedef.midtexture, texture_column);
                    let mut dc = DrawColumn::new(
                        texture_column,
                        textures.wall_light_colourmap(
                            &seg.v1,
                            &seg.v2,
                            self.wall_lights,
                            self.rw_scale,
                        ),
                        dc_iscale,
                        self.rw_x,
                        self.rw_midtexturemid,
                        yl as i32,
                        yh as i32,
                    );
                    dc.draw_column(textures, pixels);
                };

                rdata.portal_clip.ceilingclip[self.rw_x as usize] = view_height as i32;
                rdata.portal_clip.floorclip[self.rw_x as usize] = -1;
            } else {
                let textures = &self.texture_data.borrow();
                if self.toptexture != -1 {
                    mid = self.pixhigh as i32; // - HEIGHTUNIT;
                    self.pixhigh += self.pixhighstep;

                    if mid > rdata.portal_clip.floorclip[self.rw_x as usize] {
                        mid = rdata.portal_clip.floorclip[self.rw_x as usize] - 1;
                    }

                    if mid >= yl {
                        if seg.sidedef.toptexture != usize::MAX {
                            let texture_column =
                                textures.wall_pic_column(seg.sidedef.toptexture, texture_column);
                            let mut dc = DrawColumn::new(
                                texture_column,
                                textures.wall_light_colourmap(
                                    &seg.v1,
                                    &seg.v2,
                                    self.wall_lights,
                                    self.rw_scale,
                                ),
                                dc_iscale,
                                self.rw_x,
                                self.rw_toptexturemid,
                                yl as i32,
                                mid as i32,
                            );
                            dc.draw_column(textures, pixels);
                        }

                        rdata.portal_clip.ceilingclip[self.rw_x as usize] = mid;
                    } else {
                        rdata.portal_clip.ceilingclip[self.rw_x as usize] = yl - 1;
                    }
                } else if self.markceiling {
                    rdata.portal_clip.ceilingclip[self.rw_x as usize] = yl - 1;
                }

                if self.bottomtexture != -1 {
                    // TODO: this affects some placement
                    mid = self.pixlow as i32 + 1;
                    self.pixlow += self.pixlowstep;

                    if mid < rdata.portal_clip.ceilingclip[self.rw_x as usize] {
                        mid = rdata.portal_clip.ceilingclip[self.rw_x as usize] + 1;
                    }

                    if mid <= yh {
                        if seg.sidedef.bottomtexture != usize::MAX {
                            let texture_column =
                                textures.wall_pic_column(seg.sidedef.bottomtexture, texture_column);
                            let mut dc = DrawColumn::new(
                                texture_column,
                                textures.wall_light_colourmap(
                                    &seg.v1,
                                    &seg.v2,
                                    self.wall_lights,
                                    self.rw_scale,
                                ),
                                dc_iscale,
                                self.rw_x,
                                self.rw_bottomtexturemid,
                                mid as i32,
                                yh as i32,
                            );
                            dc.draw_column(textures, pixels);
                        }
                        rdata.portal_clip.floorclip[self.rw_x as usize] = mid;
                    } else {
                        rdata.portal_clip.floorclip[self.rw_x as usize] = yh + 1;
                    }
                } else if self.markfloor {
                    rdata.portal_clip.floorclip[self.rw_x as usize] = yh + 1;
                }

                if self.maskedtexture {
                    rdata.visplanes.openings[(self.maskedtexturecol + self.rw_x) as usize] =
                        texture_column;
                }
            }

            self.rw_x += 1;
            self.rw_scale += self.rw_scalestep;
            self.topfrac += self.topstep;
            self.bottomfrac += self.bottomstep;
        }
    }
}

/// Provides an easy way to draw a column in an `dc_x` location, starting and ending at `yl` and `yh`
pub struct DrawColumn<'a> {
    texture_column: &'a [usize],
    colourmap: &'a [usize],
    fracstep: f32,
    dc_x: i32,
    dc_texturemid: f32,
    yl: i32,
    yh: i32,
}

impl<'a> DrawColumn<'a> {
    pub fn new(
        texture_column: &'a [usize],
        colourmap: &'a [usize],
        fracstep: f32,
        dc_x: i32,
        dc_texturemid: f32,
        yl: i32,
        yh: i32,
    ) -> Self {
        Self {
            texture_column,
            colourmap,
            fracstep,
            dc_x,
            dc_texturemid,
            yl,
            yh,
        }
    }

    /// A column is a vertical slice/span from a wall texture that,
    ///  given the DOOM style restrictions on the view orientation,
    ///  will always have constant z depth.
    /// Thus a special case loop for very fast rendering can
    ///  be used. It has also been used with Wolfenstein 3D.
    pub fn draw_column(&mut self, textures: &PicData, pixels: &mut PixelBuf) {
        let pal = textures.palette(0);
        let mut frac =
            self.dc_texturemid + (self.yl as f32 - SCREENHEIGHT_HALF as f32) * self.fracstep;

        for n in self.yl..=self.yh {
            let mut select = frac.round() as i32 & 127;
            if select >= self.texture_column.len() as i32 {
                select %= self.texture_column.len() as i32;
            }
            if self.texture_column[select as usize] as usize == usize::MAX {
                frac += self.fracstep;
                continue;
            }

            let px = self.colourmap[self.texture_column[select as usize]];
            let c = pal[px];
            pixels.set_pixel(self.dc_x as usize, n as usize, c.r, c.g, c.b, 255);

            frac += self.fracstep;
        }
    }
}
