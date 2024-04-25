use crate::utilities::screen_to_x_view;
use gameplay::{Angle, FlatPic, LineDefFlags, PicData, Player, Segment};
use glam::Vec2;
use render_target::PixelBuffer;
use std::{f32::consts::FRAC_PI_2, ptr::NonNull};

use crate::utilities::{point_to_dist, scale_from_view_angle};

use super::{
    defs::{DrawSeg, MAXDRAWSEGS, SIL_BOTH, SIL_BOTTOM, SIL_NONE, SIL_TOP},
    RenderData,
};

//const HEIGHTUNIT: f32 = 0.062485;

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
    maskedtexturecol: f32,
    // Texture exists?
    toptexture: bool,
    bottomtexture: bool,
    midtexture: bool,
    //
    rw_normalangle: Angle,
    // regular wall
    rw_startx: f32,
    rw_stopx: f32,
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

    /// Stores the column number of the texture required for this opening
    pub(super) openings: Vec<f32>,
    lastopening: f32,
    /// Light level for the wall
    wall_lights: i32,
    doubled: bool,
}

impl SegRender {
    pub fn new(screen_width: usize, screen_height: usize) -> Self {
        Self {
            segtextured: false,
            markfloor: false,
            markceiling: false,
            maskedtexture: false,
            maskedtexturecol: -1.0,
            toptexture: false,
            bottomtexture: false,
            midtexture: false,
            rw_normalangle: Angle::default(),
            rw_startx: 0.0,
            rw_stopx: 0.0,
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
            worldtop: 0.0,
            worldbottom: 0.0,
            worldhigh: 0.0,
            worldlow: 0.0,
            wall_lights: 0,
            openings: vec![f32::MAX; screen_width * screen_height], // TODO: find a good limit
            lastopening: 0.0,
            doubled: false,
        }
    }

    pub fn clear(&mut self) {
        self.lastopening = 0.0;
    }

    /// R_StoreWallRange - r_segs
    pub fn store_wall_range(
        &mut self,
        start: f32,
        stop: f32,
        seg: &Segment,
        player: &Player,
        rdata: &mut RenderData,
        pic_data: &PicData,
        pixels: &mut dyn PixelBuffer,
    ) {
        // Keep original Doom behaviour here
        if rdata.drawsegs.len() >= MAXDRAWSEGS {
            return;
        }

        // bounds check before getting ref
        if rdata.ds_p >= rdata.drawsegs.len() {
            rdata.drawsegs.push(DrawSeg::new(NonNull::from(seg)));
        }

        let ds_p = &mut rdata.drawsegs[rdata.ds_p];

        if start < 0.0 || pixels.size().width_f32() <= start || start > stop {
            // return;
            panic!("Bad R_RenderWallRange: {} to {}", start, stop);
        }

        // These need only be locally defined to make some things easier
        let sidedef = seg.sidedef.clone();
        let mut linedef = seg.linedef.clone();

        // mark the segment as visible for automap
        linedef.flags |= LineDefFlags::Mapped as u32;

        self.rw_normalangle = seg.angle + FRAC_PI_2;
        let mut offsetangle = self.rw_normalangle - rdata.rw_angle1; // radians

        let mobj = unsafe { player.mobj_unchecked() };

        let distangle = Angle::new(FRAC_PI_2 - offsetangle.rad());
        let hyp = point_to_dist(seg.v1.x, seg.v1.y, mobj.xy); // verified correct
        self.rw_distance = hyp * distangle.sin(); // Correct??? Seems to be...

        // TODO: doublecheck the angles and bounds
        let visangle = mobj.angle + screen_to_x_view(start, pixels.size().width_f32());
        self.rw_scale = scale_from_view_angle(
            visangle,
            self.rw_normalangle,
            self.rw_distance,
            mobj.angle,
            pixels.size().width_f32(),
        );

        let visangle = mobj.angle + screen_to_x_view(stop, pixels.size().width_f32());

        ds_p.scale1 = self.rw_scale;
        ds_p.x1 = start;
        self.rw_startx = ds_p.x1;
        ds_p.x2 = stop;
        self.rw_stopx = stop + 1.0;

        if stop > start {
            // scale2 and rw_scale appears corrrect
            ds_p.scale2 = scale_from_view_angle(
                visangle,
                self.rw_normalangle,
                self.rw_distance,
                mobj.angle,
                pixels.size().width_f32(),
            );

            self.rw_scalestep = (ds_p.scale2 - self.rw_scale) / (stop - start);
            ds_p.scalestep = self.rw_scalestep;
        } else {
            ds_p.scale2 = ds_p.scale1;
        }

        // calculate texture boundaries
        //  and decide if floor / ceiling marks are needed
        // `seg.sidedef.sector` is the front sector
        let frontsector = &seg.frontsector;
        self.worldtop = frontsector.ceilingheight - player.viewz;
        self.worldbottom = frontsector.floorheight - player.viewz;

        self.midtexture = false;
        self.toptexture = false;
        self.bottomtexture = false;
        self.maskedtexture = false;
        self.maskedtexturecol = -1.0;

        if seg.backsector.is_none() {
            self.markfloor = true;
            self.markceiling = true;
            // single sided line
            self.midtexture = seg.sidedef.midtexture.is_some();

            // top of texture at top
            self.rw_midtexturemid = self.worldtop;
            if linedef.flags & LineDefFlags::UnpegBottom as u32 != 0 {
                if let Some(mid_tex) = seg.sidedef.midtexture {
                    let texture_column = pic_data.wall_pic_column(mid_tex, 0);
                    let vtop = frontsector.floorheight + texture_column.len() as f32;
                    self.rw_midtexturemid = vtop - player.viewz;
                }
            }
            self.rw_midtexturemid += seg.sidedef.rowoffset;

            ds_p.silhouette = SIL_BOTH;
            ds_p.sprtopclip = Some(0.0); // start of screenheightarray
            ds_p.sprbottomclip = Some(0.0); // start of negonearray
            ds_p.bsilheight = f32::MAX;
            ds_p.tsilheight = f32::MIN;
        } else {
            let backsector = seg.backsector.as_ref().unwrap();
            // two sided line
            // TODO: when thing render started
            ds_p.sprtopclip = None;
            ds_p.sprbottomclip = None;
            ds_p.silhouette = SIL_NONE;

            if frontsector.floorheight > backsector.floorheight {
                ds_p.silhouette = SIL_BOTTOM;
                ds_p.bsilheight = frontsector.floorheight;
            } else if backsector.floorheight >= player.viewz {
                ds_p.silhouette = SIL_BOTTOM;
                ds_p.bsilheight = f32::MAX;
            }

            if frontsector.ceilingheight < backsector.ceilingheight {
                ds_p.silhouette |= SIL_TOP;
                ds_p.tsilheight = frontsector.ceilingheight;
            } else if backsector.ceilingheight <= player.viewz {
                ds_p.silhouette |= SIL_TOP;
                ds_p.tsilheight = f32::MIN;
            }

            // Commented out as this seems to fix the incorrect clipping of
            // sprites lower/higher than player and blocked by lower or upper
            // part of portal
            // if backsector.ceilingheight <= frontsector.floorheight {
            //     ds_p.sprbottomclip = Some(0.0); // start of negonearray
            //     ds_p.silhouette |= SIL_BOTTOM;
            //     ds_p.bsilheight = f32::MAX;
            // }

            // if backsector.floorheight >= frontsector.ceilingheight {
            //     ds_p.sprtopclip = Some(0.0);
            //     ds_p.silhouette |= SIL_TOP;
            //     ds_p.tsilheight = f32::MIN;
            // }

            self.worldhigh = backsector.ceilingheight - player.viewz;
            self.worldlow = backsector.floorheight - player.viewz;

            if frontsector.ceilingpic == pic_data.sky_num()
                && backsector.ceilingpic == pic_data.sky_num()
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
                self.toptexture = sidedef.toptexture.is_some();
                if linedef.flags & LineDefFlags::UnpegTop as u32 != 0 {
                    self.rw_toptexturemid = self.worldtop;
                } else if let Some(top_tex) = seg.sidedef.toptexture {
                    let texture_column = pic_data.wall_pic_column(top_tex, 0);
                    let vtop = backsector.ceilingheight + texture_column.len() as f32;
                    self.rw_toptexturemid = vtop - player.viewz;
                }
            }

            if self.worldlow > self.worldbottom {
                self.bottomtexture = sidedef.bottomtexture.is_some();
                if linedef.flags & LineDefFlags::UnpegBottom as u32 != 0 {
                    self.rw_bottomtexturemid = self.worldtop;
                } else {
                    self.rw_bottomtexturemid = self.worldlow;
                }
            }

            self.rw_toptexturemid += sidedef.rowoffset;
            self.rw_bottomtexturemid += sidedef.rowoffset;

            // if sidedef.midtexture.is_some() {
            self.maskedtexture = true;
            // Set the indexes in to visplanes.openings
            self.maskedtexturecol = self.lastopening - self.rw_startx;
            ds_p.maskedtexturecol = self.maskedtexturecol;

            self.lastopening += self.rw_stopx - self.rw_startx;
            // }
        }

        // calculate rw_offset (only needed for textured lines)
        self.segtextured =
            self.midtexture || self.toptexture || self.bottomtexture || self.maskedtexture;

        if self.segtextured {
            offsetangle = self.rw_normalangle - rdata.rw_angle1;
            self.rw_offset = hyp * offsetangle.sin();
            // if self.rw_normalangle.rad() - rdata.rw_angle1.rad() < PI * 2.0 {
            self.rw_offset = -self.rw_offset;
            //  }
            self.rw_offset += sidedef.textureoffset + seg.offset;
            self.rw_centerangle = mobj.angle - self.rw_normalangle;
            self.wall_lights = (seg.sidedef.sector.lightlevel >> 4) + player.extralight;
        }

        // if a floor / ceiling plane is on the wrong side
        //  of the view plane, it is definitely invisible
        //  and doesn't need to be marked.
        if frontsector.floorheight > player.viewz {
            // above view plane
            self.markfloor = false;
        }

        if frontsector.ceilingheight < player.viewz && frontsector.ceilingpic != pic_data.sky_num()
        {
            // below view plane
            self.markceiling = false;
        }

        let half_height = pixels.size().half_height_f32(); // TODO: hmmm, - 0.5;
        self.topstep = -(self.worldtop * self.rw_scalestep);
        self.topfrac = half_height - (self.worldtop * self.rw_scale);

        self.bottomstep = -(self.worldbottom * self.rw_scalestep);
        self.bottomfrac = half_height - (self.worldbottom * self.rw_scale);

        if seg.backsector.is_some() {
            if self.worldhigh < self.worldtop {
                self.pixhigh = half_height - (self.worldhigh * self.rw_scale);
                self.pixhighstep = -(self.worldhigh * self.rw_scalestep);
            }

            if self.worldlow > self.worldbottom {
                self.pixlow = half_height - (self.worldlow * self.rw_scale);
                self.pixlowstep = -(self.worldlow * self.rw_scalestep);
            }
        }

        // render it
        if self.markceiling {
            rdata.visplane_render.ceilingplane = rdata.visplane_render.check_plane(
                self.rw_startx,
                self.rw_stopx,
                rdata.visplane_render.ceilingplane,
            );
        }

        if self.markfloor {
            rdata.visplane_render.floorplane = rdata.visplane_render.check_plane(
                self.rw_startx,
                self.rw_stopx,
                rdata.visplane_render.floorplane,
            );
        }

        self.doubled = pixels.size().height() > 200;
        self.render_seg_loop(seg, player.viewheight, rdata, pic_data, pixels);

        let ds_p = &mut rdata.drawsegs[rdata.ds_p];
        if (ds_p.silhouette & SIL_TOP != 0 || self.maskedtexture) && ds_p.sprtopclip.is_none() {
            for (i, n) in rdata
                .portal_clip
                .ceilingclip
                .iter()
                .skip(start as usize)
                .enumerate()
            {
                let last = self.lastopening as usize;
                self.openings[last + i] = *n;
                if i as f32 >= self.rw_stopx - start {
                    break;
                }
            }
            ds_p.sprtopclip = Some(self.lastopening - start);
            self.lastopening += self.rw_stopx - start;
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
                let last = self.lastopening as usize;
                self.openings[last + i] = *n;
                if i as f32 >= self.rw_stopx - start {
                    break;
                }
            }
            ds_p.sprbottomclip = Some(self.lastopening - start);
            self.lastopening += self.rw_stopx - start;
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
        pic_data: &PicData,
        pixels: &mut dyn PixelBuffer,
    ) {
        // TODO: yh/bottomfrac is sometimes negative?
        if self.bottomfrac.is_sign_negative() {
            return;
        }
        // yl is the pixel location, it is the result of converting the topfrac to int
        let mut yl: f32;
        let mut yh: f32;
        let mut top: f32;
        let mut bottom: f32;
        let mut mid: f32;
        let mut angle;
        let mut texture_column = 0;

        while self.rw_startx < self.rw_stopx.ceil() {
            let clip_index = self.rw_startx as usize;
            if rdata.portal_clip.floorclip[clip_index] < 0.0 {
                // TODO: shouldn't be happening, early out?
                return;
            }

            // The yl and yh blocks are what affect wall clipping the most. You can make shorter/taller.
            // topfrac here is calulated in previous function and is the
            // starting point that topstep is added to
            yl = self.topfrac.floor();
            if yl <= rdata.portal_clip.ceilingclip[clip_index] + 1.0 {
                yl = rdata.portal_clip.ceilingclip[clip_index] + 1.0;
            }

            if self.markceiling {
                top = rdata.portal_clip.ceilingclip[clip_index] + 1.0;
                bottom = yl;

                if bottom >= rdata.portal_clip.floorclip[clip_index] {
                    bottom = rdata.portal_clip.floorclip[clip_index] - 1.0;
                }
                if top <= bottom {
                    // was a plane selection already set?
                    let ceil = rdata.visplane_render.ceilingplane;
                    rdata.visplane_render.visplanes[ceil].top[clip_index] = top;
                    rdata.visplane_render.visplanes[ceil].bottom[clip_index] = bottom - 1.0;
                }
            }

            yh = self.bottomfrac.floor();
            if yh >= rdata.portal_clip.floorclip[clip_index] - 1.0 {
                yh = rdata.portal_clip.floorclip[clip_index] - 1.0;
            }

            if self.markfloor {
                top = yh;
                bottom = rdata.portal_clip.floorclip[clip_index] - 1.0;

                if top <= rdata.portal_clip.ceilingclip[clip_index] {
                    top = rdata.portal_clip.ceilingclip[clip_index] + 1.0;
                }
                if top <= bottom {
                    let floor = rdata.visplane_render.floorplane;
                    rdata.visplane_render.visplanes[floor].top[clip_index] = top + 1.0;
                    rdata.visplane_render.visplanes[floor].bottom[clip_index] = bottom;
                }
            }

            let mut dc_iscale = 0.0;
            if self.segtextured {
                angle = self.rw_centerangle
                    + screen_to_x_view(self.rw_startx, pixels.size().width_f32());
                texture_column = (self.rw_offset - angle.tan() * self.rw_distance).abs() as usize;

                dc_iscale = 1.0 / self.rw_scale;
            }

            if self.midtexture {
                if let Some(mid_tex) = seg.sidedef.midtexture {
                    let texture_column = pic_data.wall_pic_column(mid_tex, texture_column);
                    draw_wall_column(
                        texture_column,
                        pic_data.wall_light_colourmap(
                            &seg.v1,
                            &seg.v2,
                            self.wall_lights,
                            self.rw_scale,
                        ),
                        dc_iscale,
                        self.rw_startx,
                        self.rw_midtexturemid,
                        yl,
                        yh,
                        pic_data,
                        false,
                        pixels,
                    );
                };

                rdata.portal_clip.ceilingclip[clip_index] = view_height;
                rdata.portal_clip.floorclip[clip_index] = -1.0;
            } else {
                if self.toptexture {
                    // floor vs ceil affects how things align in slightly off ways
                    mid = self.pixhigh.floor();
                    self.pixhigh += self.pixhighstep;

                    if mid >= rdata.portal_clip.floorclip[clip_index] {
                        mid = rdata.portal_clip.floorclip[clip_index] - 1.0;
                    }

                    if mid >= yl {
                        if let Some(top_tex) = seg.sidedef.toptexture {
                            let texture_column = pic_data.wall_pic_column(top_tex, texture_column);
                            draw_wall_column(
                                texture_column,
                                pic_data.wall_light_colourmap(
                                    &seg.v1,
                                    &seg.v2,
                                    self.wall_lights,
                                    self.rw_scale,
                                ),
                                dc_iscale,
                                self.rw_startx,
                                self.rw_toptexturemid,
                                yl,
                                mid,
                                pic_data,
                                false,
                                pixels,
                            );
                        }

                        rdata.portal_clip.ceilingclip[clip_index] = mid;
                    } else {
                        rdata.portal_clip.ceilingclip[clip_index] = yl - 1.0;
                    }
                } else if self.markceiling {
                    rdata.portal_clip.ceilingclip[clip_index] = yl - 1.0;
                }

                if self.bottomtexture {
                    // floor vs ceil affects how things align in slightly off ways
                    mid = (self.pixlow + 1.0).floor();
                    self.pixlow += self.pixlowstep;

                    if mid <= rdata.portal_clip.ceilingclip[clip_index] {
                        mid = rdata.portal_clip.ceilingclip[clip_index] + 1.0;
                    }

                    if mid <= yh {
                        if let Some(bot_tex) = seg.sidedef.bottomtexture {
                            let texture_column = pic_data.wall_pic_column(bot_tex, texture_column);
                            draw_wall_column(
                                texture_column,
                                pic_data.wall_light_colourmap(
                                    &seg.v1,
                                    &seg.v2,
                                    self.wall_lights,
                                    self.rw_scale,
                                ),
                                dc_iscale,
                                self.rw_startx,
                                self.rw_bottomtexturemid,
                                mid - 1.0,
                                yh,
                                pic_data,
                                false,
                                pixels,
                            );
                        }
                        rdata.portal_clip.floorclip[clip_index] = mid;
                    } else {
                        rdata.portal_clip.floorclip[clip_index] = yh + 1.0;
                    }
                } else if self.markfloor {
                    rdata.portal_clip.floorclip[clip_index] = yh + 1.0;
                }

                if self.maskedtexture {
                    self.openings[(self.maskedtexturecol + self.rw_startx) as usize] =
                        texture_column as f32;
                }
            }

            self.rw_startx += 1.0;
            self.rw_scale += self.rw_scalestep;
            self.topfrac += self.topstep;
            self.bottomfrac += self.bottomstep;
        }
    }
}

/// Provides an easy way to draw a column in an `dc_x` location, starting and ending at `yl` and `yh`

/// A column is a vertical slice/span from a wall texture that,
///  given the DOOM style restrictions on the view orientation,
///  will always have constant z depth.
/// Thus a special case loop for very fast rendering can
///  be used. It has also been used with Wolfenstein 3D.
pub fn draw_wall_column(
    texture_column: &[usize],
    colourmap: &[usize],
    fracstep: f32,
    dc_x: f32,
    dc_texturemid: f32,
    yl: f32,
    yh: f32,
    pic_data: &PicData,
    doubled: bool,
    pixels: &mut dyn PixelBuffer,
) {
    let mut frac = dc_texturemid + (yl - pixels.size().half_height_f32()) * fracstep;

    let dc_x = dc_x as usize;
    let pal = pic_data.palette();
    for y in yl as i32..=yh as i32 {
        let mut select = frac.abs() as usize;
        if doubled {
            select /= 2;
        }
        select %= texture_column.len();

        let tc = texture_column[select];
        if tc >= colourmap.len() {
            return;
        }
        let cm = colourmap[tc];
        let c = pal[cm];
        pixels.set_pixel(dc_x, y as usize, (c.r, c.g, c.b, 255));
        frac += fracstep;
    }
}

pub fn draw_column_style_flats(
    texture: &FlatPic,
    viewxy: Vec2,
    plane_height: f32,
    total_light: i32,
    dc_x: f32,
    angle: Angle,
    mut yl: f32,
    yh: f32,
    pic_data: &PicData,
    pixels: &mut dyn PixelBuffer,
) {
    let angle = angle + screen_to_x_view(dc_x, pixels.size().width_f32());
    let distscale = 1.0 / screen_to_x_view(dc_x, pixels.size().width_f32()).cos();
    let cos = angle.cos();
    let sin = angle.sin();

    let dc_x = dc_x as usize;
    let pal = pic_data.palette();
    for y in yl as usize..=yh as usize {
        let dy = yl - pixels.size().half_height_f32();
        let yslope = pixels.size().half_width_f32() / dy.abs();
        let distance = plane_height * yslope;

        let length = distance * distscale;
        let ds_xfrac = viewxy.x + cos * length;
        let ds_yfrac = -viewxy.y - sin * length;

        let x_step = ds_xfrac.abs() as usize % (texture.data.len());
        let y_step = ds_yfrac.abs() as usize % (texture.data[0].len());

        let colourmap = pic_data.flat_light_colourmap(total_light, distance as u32);
        let px = colourmap[texture.data[x_step][y_step]];
        let c = pal[px];
        pixels.set_pixel(dc_x, y, (c.r, c.g, c.b, 255));

        yl += 1.0;
    }
}
