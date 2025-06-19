use crate::utilities::screen_to_angle;
#[cfg(feature = "hprof")]
use coarse_prof::profile;
use gameplay::log::warn;
use gameplay::tic_cmd::{LOOKDIRMAX, LOOKDIRMIN, LOOKDIRS};
use gameplay::{Angle, FlatPic, LineDefFlags, MapObject, PicData, Player, Segment};
use glam::Vec2;
use render_trait::{PixelBuffer, RenderTrait, SOFT_PIXEL_CHANNELS};
use std::f32::consts::{FRAC_PI_2, TAU};
use std::ptr::NonNull;
#[cfg(feature = "debug_draw")]
use std::thread::sleep;
#[cfg(feature = "debug_draw")]
use std::time::Duration;

use crate::utilities::{point_to_dist, scale_from_view_angle};

use super::RenderData;
use super::defs::{DrawSeg, MAXDRAWSEGS, SIL_BOTH, SIL_BOTTOM, SIL_NONE, SIL_TOP};

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
    /// `sector_t *frontsector;` `sector_t *backsector;` shared variables
    /// between `r_bsp.c` and `r_seg.c`.

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
    wall_lights: usize,
    pub yslopes: Vec<Vec<f32>>,
    pub look_yslope: usize,
    pub centery: f32,
    pub screen_x: Vec<f32>,
    pub screen_x_scale: Vec<f32>,
    pub fov: f32,
    pub fov_half: f32,
    pub wide_ratio: f32,

    sky_doubled: bool,
    sky_mid: f32,

    dc_iscale: f32,
}

impl SegRender {
    pub fn new(fov: f32, screen_width: usize, screen_height: usize) -> Self {
        let screen_x: Vec<f32> = (0..=screen_width)
            .map(|x| screen_to_angle(fov, x as f32, (screen_width / 2) as f32))
            .collect();

        let wide_ratio = screen_height as f32 / screen_width as f32 * 1.6;
        let screen_x_scale = screen_x
            .iter()
            .map(|x| 1.0 / x.cos() * wide_ratio)
            .collect();

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
            openings: vec![f32::MAX; screen_width * screen_height],
            lastopening: 0.0,
            yslopes: (0..=screen_height + 1)
                .map(|y| unsafe {
                    (0..LOOKDIRS)
                        .map(|j| {
                            let dy =
                                y as f32 - (screen_height as f32 / 2.0 + (j - LOOKDIRMIN) as f32);
                            screen_width as f32 / 2.0 / dy.abs()
                        })
                        .collect()
                })
                .collect(),
            look_yslope: 0,
            centery: screen_height as f32 / 2.0,
            screen_x,
            screen_x_scale,
            fov,
            fov_half: fov / 2.0,
            wide_ratio,

            sky_doubled: screen_height != 200,
            sky_mid: (screen_height / 2 - if screen_height != 200 { 12 } else { 6 }) as f32,

            dc_iscale: 0.0,
        }
    }

    pub const fn clear(&mut self) {
        self.lastopening = 0.0;
    }

    /// # Safety
    /// Nothing else should be modifying `LOOKDIRMAX`
    pub const unsafe fn set_view_pitch(&mut self, pitch: i16, half_screen_height: f32) {
        unsafe {
            self.look_yslope = (LOOKDIRMAX as i16 + pitch) as usize;
        }
        self.centery = half_screen_height as f32 + pitch as f32;
    }

    /// R_StoreWallRange - r_segs
    /// This is called by the BSP clipping functions. The incoming `start` and
    /// `stop` have already been `.floor()`ed by `angle_to_screen()` function
    /// called on the segs during BSP traversal.
    ///
    /// # Note
    ///
    /// This can be a source of bugs such as missing clip ranges
    pub(crate) fn store_wall_range(
        &mut self,
        start: f32,
        stop: f32,
        seg: &Segment,
        player: &Player,
        rdata: &mut RenderData,
        pic_data: &PicData,
        rend: &mut impl RenderTrait,
    ) {
        #[cfg(feature = "hprof")]
        profile!("store_wall_range");
        let size = rend.draw_buffer().size();
        // //seg:, x:496.000000, y:-1072.000000
        // //seg:, x:496.000000, y:-1040.000000
        // if seg.v1 == Vec2::new(496.0, -1072.0) && seg.v2 == Vec2::new(496.0, -1040.0)
        // {     dbg!(&seg.sidedef);
        // }
        if start < 0.0 || start > size.width_f32() || start > stop {
            // panic!("Bad R_RenderWallRange: {} to {}", start, stop);
        }

        // bounds check before getting ref
        if rdata.ds_p >= rdata.drawsegs.capacity() {
            rdata.drawsegs.reserve(MAXDRAWSEGS);
            warn!(
                "Maxxed out drawsegs. Expanded to {}",
                rdata.drawsegs.capacity()
            );
        }
        if rdata.ds_p >= rdata.drawsegs.len() {
            rdata.drawsegs.push(DrawSeg::new(NonNull::from(seg)));
        }

        let ds_p = &mut rdata.drawsegs[rdata.ds_p];

        // These need only be locally defined to make some things easier
        let sidedef = seg.sidedef.clone();
        let mut linedef = seg.linedef.clone();

        // mark the segment as visible for automap
        linedef.flags |= LineDefFlags::Mapped as u32;
        // TODO: return if in automap

        self.rw_normalangle = seg.angle + FRAC_PI_2; // widescreen: Leave as is
        let mut offsetangle = self.rw_normalangle - rdata.rw_angle1; // radians

        let mobj = unsafe { player.mobj_unchecked() };

        let distangle = Angle::new(FRAC_PI_2 - offsetangle.rad()); // widescreen: Leave as is
        let hyp = point_to_dist(seg.v1.x, seg.v1.y, mobj.xy); // verified correct
        let sin_val = distangle.sin();
        const MIN_DISTANCE: f32 = 0.001;
        self.rw_distance = (hyp * sin_val).max(MIN_DISTANCE);

        ds_p.x1 = start;
        self.rw_startx = ds_p.x1;
        ds_p.x2 = stop;
        self.rw_stopx = stop + 1.0;
        self.rw_scale = scale_from_view_angle(
            mobj.angle + self.screen_x[start as u32 as usize],
            self.rw_normalangle,
            self.rw_distance,
            mobj.angle,
            size.half_width_f32(),
        ) * self.wide_ratio;
        ds_p.scale1 = self.rw_scale;

        if stop >= start {
            ds_p.scale2 = scale_from_view_angle(
                mobj.angle + self.screen_x[stop as u32 as usize],
                self.rw_normalangle,
                self.rw_distance,
                mobj.angle,
                size.half_width_f32(),
            ) * self.wide_ratio;

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

        // //map20
        // if seg.v2 == Vec2::new(-560.000000, -3920.000000)
        //     && seg.v1 == Vec2::new(-560.000000, -3952.000000)
        // {
        //     dbg!(seg);
        // }

        if seg.backsector.is_none() {
            // single sided line
            self.markfloor = true;
            self.markceiling = true;
            self.midtexture = sidedef.midtexture.is_some();
            if linedef.flags & LineDefFlags::UnpegBottom as u32 != 0 {
                if let Some(mid_tex) = sidedef.midtexture {
                    let texture_column = pic_data.wall_pic_column(mid_tex, 0);
                    let vtop = frontsector.floorheight + texture_column.len() as f32;
                    self.rw_midtexturemid = vtop - player.viewz;
                }
            } else {
                // top of texture at top
                self.rw_midtexturemid = self.worldtop;
            }
            self.rw_midtexturemid += sidedef.rowoffset;

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
            } else if backsector.ceilingheight < player.viewz {
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
                    // texture top
                    self.rw_toptexturemid = self.worldtop;
                } else if let Some(top_tex) = sidedef.toptexture {
                    let texture_column = pic_data.wall_pic_column(top_tex, 0);
                    let vtop = backsector.ceilingheight + texture_column.len() as f32;
                    // texture bottom
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

            // TODO: fix this. Enabed causes sprites to clip throguh some places
            // if sidedef.midtexture.is_some() {
            self.maskedtexture = true;
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
            self.wall_lights = (sidedef.sector.lightlevel >> 4) + player.extralight;
            if (seg.angle.rad().abs() == std::f32::consts::PI || seg.angle.rad() == 0.0)
                && self.wall_lights > 0
            {
                self.wall_lights -= 1;
            }
        }

        // if a floor / ceiling plane is on the wrong side
        //  of the view plane, it is definitely invisible
        //  and doesn't need to be marked.
        if frontsector.floorheight > player.viewz {
            // above view plane
            self.markfloor = false;
        }

        if frontsector.ceilingheight <= player.viewz && frontsector.ceilingpic != pic_data.sky_num()
        {
            // below view plane
            self.markceiling = false;
        }

        let half_height = self.centery;
        self.topstep = -(self.worldtop * self.rw_scalestep);
        self.topfrac = half_height - (self.worldtop * self.rw_scale) + 1.0;

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

        self.render_seg_loop(seg, player, mobj, rdata, pic_data, rend);

        #[cfg(feature = "debug_seg_clip")]
        {
            self.draw_debug_clipping(rdata, rend.draw_buffer());
            rend.debug_blit_draw_buffer();
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        let ds_p = &mut rdata.drawsegs[rdata.ds_p];
        if (ds_p.silhouette & SIL_TOP != 0 || self.maskedtexture) && ds_p.sprtopclip.is_none() {
            for (i, n) in rdata
                .portal_clip
                .ceilingclip
                .iter()
                .skip(start as u32 as usize)
                .enumerate()
            {
                let last = self.lastopening as u32 as usize;
                if last + i >= self.openings.len() {
                    break;
                }
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
                .skip(start as u32 as usize)
                .enumerate()
            {
                let last = self.lastopening as u32 as usize;
                if last + i >= self.openings.len() {
                    break;
                }
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
        player: &Player,
        mobj: &MapObject,
        rdata: &mut RenderData,
        pic_data: &PicData,
        rend: &mut impl RenderTrait,
    ) {
        #[cfg(feature = "hprof")]
        profile!("render_seg_loop");
        // yl is the pixel location, it is the result of converting the topfrac to int
        let mut yl: f32;
        let mut yh: f32;
        let mut top: f32;
        let mut bottom: f32;
        let mut mid: f32;
        let mut angle;
        let mut texture_column = 0;
        let size = rend.draw_buffer().size().clone();
        let sidedef = seg.sidedef.clone();

        let flats_total_light = (seg.frontsector.lightlevel >> 4) + player.extralight;
        let ceil_height = (seg.frontsector.ceilingheight - player.viewz).abs();
        let ceil_tex = pic_data.get_flat(seg.frontsector.ceilingpic);
        let floor_height = (seg.frontsector.floorheight - player.viewz).abs();
        let floor_tex = pic_data.get_flat(seg.frontsector.floorpic);

        while self.rw_startx < self.rw_stopx {
            let clip_index = self.rw_startx as u32 as usize;
            // The yl and yh blocks are what affect wall clipping the most. You can make
            // shorter/taller. topfrac here is calulated in previous function
            // and is the starting point that topstep is added to
            top = rdata.portal_clip.ceilingclip[clip_index] + 1.0;
            yl = self.topfrac.floor();
            if yl < top {
                yl = top;
            }

            let x_angle = mobj.angle + self.screen_x[clip_index];
            let cos = x_angle.cos();
            let sin = x_angle.sin();
            let distscale = self.screen_x_scale[self.rw_startx as u32 as usize];

            if self.markceiling {
                bottom = yl - 1.0;
                if bottom >= rdata.portal_clip.floorclip[clip_index] {
                    bottom = rdata.portal_clip.floorclip[clip_index] - 1.0;
                }
                if top <= bottom {
                    if seg.frontsector.ceilingpic == pic_data.sky_num() {
                        let screen_x_degrees =
                            screen_to_angle(self.fov, self.rw_startx, size.half_width_f32());
                        let sky_angle =
                            (mobj.angle.rad() + screen_x_degrees + TAU * 2.).to_degrees() * 2.8444; // 2.8444 seems to give the corect skybox width
                        let sky_column = pic_data
                            .wall_pic_column(pic_data.sky_pic(), sky_angle.abs() as u32 as usize);

                        self.dc_iscale = 0.89;
                        self.draw_wall_column(
                            sky_column,
                            self.sky_mid,
                            top as i32,
                            bottom as i32,
                            true,
                            pic_data,
                            rend.draw_buffer(),
                        );
                        #[cfg(feature = "debug_draw")]
                        {
                            rend.debug_blit_draw_buffer();
                            sleep(Duration::from_millis(1));
                        }
                    } else {
                        self.draw_flat_column(
                            ceil_tex,
                            mobj.xy,
                            ceil_height,
                            flats_total_light,
                            cos,
                            sin,
                            distscale,
                            top as u32 as usize,
                            bottom as u32 as usize,
                            pic_data,
                            rend.draw_buffer(),
                        );
                        #[cfg(feature = "debug_draw")]
                        {
                            rend.debug_blit_draw_buffer();
                            sleep(Duration::from_millis(1));
                        }
                    }
                    // Must clip walls to floors if drawn
                    rdata.portal_clip.ceilingclip[clip_index] = bottom;
                }
            }

            bottom = rdata.portal_clip.floorclip[clip_index] - 1.0;
            yh = self.bottomfrac.floor();
            if yh > bottom {
                yh = bottom;
            }

            if self.markfloor {
                top = yh + 1.0;
                if top < rdata.portal_clip.ceilingclip[clip_index] {
                    top = rdata.portal_clip.ceilingclip[clip_index];
                }
                if top <= bottom {
                    // Must clip walls to floors if drawn
                    rdata.portal_clip.floorclip[clip_index] = top;
                    self.draw_flat_column(
                        floor_tex,
                        mobj.xy,
                        floor_height,
                        flats_total_light,
                        cos,
                        sin,
                        distscale,
                        top as u32 as usize,
                        bottom as u32 as usize,
                        pic_data,
                        rend.draw_buffer(),
                    );
                    #[cfg(feature = "debug_draw")]
                    {
                        rend.debug_blit_draw_buffer();
                        sleep(Duration::from_millis(1));
                    }
                }
            }

            if self.segtextured {
                angle = self.rw_centerangle + self.screen_x[self.rw_startx as u32 as usize]; // screen_to_x_view(self.fov, self.rw_startx, size.half_width_f32());
                // TODO: horizontal position of texture isn't quite right
                let tan_val = angle.tan().clamp(-2048.0, 2048.0);
                texture_column =
                    (self.rw_offset - tan_val * self.rw_distance).abs().floor() as u32 as usize;

                const MIN_SCALE: f32 = 0.00001;
                const MAX_ISCALE: f32 = 65536.0;
                self.dc_iscale = if self.rw_scale.abs() < MIN_SCALE {
                    MAX_ISCALE
                } else {
                    (1.0 / self.rw_scale).clamp(0.0, MAX_ISCALE)
                };
            }

            if self.midtexture {
                if yl <= yh {
                    if let Some(mid_tex) = sidedef.midtexture {
                        let texture_column = pic_data.wall_pic_column(mid_tex, texture_column);
                        self.draw_wall_column(
                            texture_column,
                            self.rw_midtexturemid,
                            yl as i32,
                            yh as i32,
                            false,
                            pic_data,
                            rend.draw_buffer(),
                        );
                        #[cfg(feature = "debug_draw")]
                        {
                            rend.debug_blit_draw_buffer();
                            sleep(Duration::from_millis(1));
                        }
                    };
                    rdata.portal_clip.ceilingclip[clip_index] = player.viewheight;
                    rdata.portal_clip.floorclip[clip_index] = -1.0;
                }
            } else {
                if self.toptexture {
                    // floor vs ceil affects how things align in slightly off ways
                    mid = self.pixhigh.floor();
                    self.pixhigh += self.pixhighstep;

                    if mid >= rdata.portal_clip.floorclip[clip_index] {
                        mid = rdata.portal_clip.floorclip[clip_index] - 1.0;
                    }
                    if mid >= yl {
                        if let Some(top_tex) = sidedef.toptexture {
                            let texture_column = pic_data.wall_pic_column(top_tex, texture_column);
                            self.draw_wall_column(
                                texture_column,
                                self.rw_toptexturemid,
                                yl as i32,
                                mid as i32,
                                false,
                                pic_data,
                                rend.draw_buffer(),
                            );
                            #[cfg(feature = "debug_draw")]
                            {
                                rend.debug_blit_draw_buffer();
                                sleep(Duration::from_millis(1));
                            }
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
                    mid = self.pixlow.floor() + 1.0;
                    self.pixlow += self.pixlowstep;

                    if mid <= rdata.portal_clip.ceilingclip[clip_index] {
                        mid = rdata.portal_clip.ceilingclip[clip_index] + 1.0;
                    }
                    if mid <= yh {
                        if let Some(bot_tex) = sidedef.bottomtexture {
                            let texture_column = pic_data.wall_pic_column(bot_tex, texture_column);
                            self.draw_wall_column(
                                texture_column,
                                self.rw_bottomtexturemid,
                                mid as i32,
                                yh as i32,
                                false,
                                pic_data,
                                rend.draw_buffer(),
                            );
                            #[cfg(feature = "debug_draw")]
                            {
                                rend.debug_blit_draw_buffer();
                                sleep(Duration::from_millis(1));
                            }
                            rdata.portal_clip.floorclip[clip_index] = mid;
                        }
                    } else {
                        rdata.portal_clip.floorclip[clip_index] = yh + 1.0;
                    }
                } else if self.markfloor {
                    rdata.portal_clip.floorclip[clip_index] = yh + 1.0;
                }

                if self.maskedtexture {
                    let i = (self.maskedtexturecol + self.rw_startx) as u32 as usize;
                    if self.openings.len() > i {
                        self.openings[i] = texture_column as f32;
                    }
                }
            }

            self.rw_startx += 1.0;
            self.rw_scale += self.rw_scalestep;
            self.topfrac += self.topstep;
            self.bottomfrac += self.bottomstep;
        }
    }

    /// Provides an easy way to draw a column in an `dc_x` location, starting
    /// and ending at `yl` and `yh`

    /// A column is a vertical slice/span from a wall texture that,
    ///  given the DOOM style restrictions on the view orientation,
    ///  will always have constant z depth.
    /// Thus a special case loop for very fast rendering can
    ///  be used. It has also been used with Wolfenstein 3D.
    #[inline]
    fn draw_wall_column(
        &mut self,
        texture_column: &[usize],
        dc_texturemid: f32,
        y_start: i32,
        mut y_end: i32,
        sky: bool,
        pic_data: &PicData,
        pixels: &mut impl PixelBuffer,
    ) {
        #[cfg(feature = "hprof")]
        profile!("draw_wall_column");
        y_end = y_end.min(pixels.size().height() - 1);

        let pal = pic_data.palette();
        let mut frac = dc_texturemid + (y_start as f32 - self.centery) * self.dc_iscale;

        let mut pos = pixels.get_buf_index(self.rw_startx as u32 as usize, y_start as usize);

        let colourmap = if !sky {
            pic_data.vert_light_colourmap(self.wall_lights, self.rw_scale)
        } else {
            pic_data.colourmap(0)
        };

        for _ in y_start..=y_end {
            let mut select = (frac.floor() as i32 as usize) & 127;
            if sky && self.sky_doubled {
                select /= 2;
            }
            if select >= texture_column.len() {
                return;
            }
            let tc = texture_column[select];
            #[cfg(feature = "safety_check")]
            if tc >= colourmap.len() {
                return;
            }
            #[cfg(not(feature = "safety_check"))]
            unsafe {
                let c = pal.get_unchecked(*colourmap.get_unchecked(tc));
                pixels
                    .buf_mut()
                    .get_unchecked_mut(pos..pos + SOFT_PIXEL_CHANNELS)
                    .copy_from_slice(c);
            }
            #[cfg(feature = "safety_check")]
            {
                pixels.set_pixel(dc_x, i as u32 as usize, &pal[colourmap[tc]].0);
            }
            frac += self.dc_iscale;
            pos += pixels.pitch();
        }
    }

    #[inline]
    fn draw_flat_column(
        &mut self,
        texture: &FlatPic,
        viewxy: Vec2,
        plane_height: f32,
        total_light: usize,
        cos: f32,
        sin: f32,
        distscale: f32,
        y_start: usize,
        mut y_end: usize,
        pic_data: &PicData,
        pixels: &mut impl PixelBuffer,
    ) {
        #[cfg(feature = "hprof")]
        profile!("draw_flat_column");
        y_end = y_end.min(pixels.size().height_usize() - 1);

        let pal = pic_data.palette();
        let tex_len = texture.data.len() - 1; // always square
        let mut pos = pixels.get_buf_index(self.rw_startx as u32 as usize, y_start);

        for y_slope in self.yslopes[self.look_yslope][y_start..=y_end].iter() {
            let diminished_light = plane_height * y_slope;
            // TODO: move this out and make a way to "step" it
            let colourmap =
                pic_data.flat_light_colourmap(total_light, (diminished_light as u32 as usize) >> 4);

            let length = diminished_light * distscale;
            let xfrac = viewxy.x + cos * length;
            let yfrac = viewxy.y + sin * length;
            // flats are 64x64 so a bitwise op works here
            let x_step = (xfrac.abs() as u32 as usize) & tex_len;
            let y_step = (yfrac.abs() as u32 as usize) & tex_len;

            #[cfg(not(feature = "safety_check"))]
            unsafe {
                let tc = *texture.data.get_unchecked(x_step).get_unchecked(y_step);
                let c = pal.get_unchecked(*colourmap.get_unchecked(tc));
                pixels
                    .buf_mut()
                    .get_unchecked_mut(pos..pos + SOFT_PIXEL_CHANNELS)
                    .copy_from_slice(c);
            }
            #[cfg(feature = "safety_check")]
            {
                let px = colourmap[texture.data[x_step][y_pos]];
                pixels.set_pixel(dc_x, y_pos, &pal[px].0);
            }
            pos += pixels.pitch();
        }
    }

    #[cfg(feature = "debug_seg_clip")]
    fn draw_debug_clipping(&self, rdata: &RenderData, pixels: &mut impl PixelBuffer) {
        // Draw ceiling clip line in red
        for x in 0..pixels.size().width_usize() {
            let ceiling_y = (rdata.portal_clip.ceilingclip[x] as u32 as usize);
            if ceiling_y < pixels.size().height_usize() {
                pixels.set_pixel(x, ceiling_y, &[255, 0, 0, 255]); // Red
                // Draw a second pixel to make it more visible
                if ceiling_y + 1 < pixels.size().height_usize() {
                    pixels.set_pixel(x, ceiling_y + 1, &[255, 0, 0, 255]);
                }
            }

            // Draw floor clip line in blue
            let floor_y = (rdata.portal_clip.floorclip[x] as u32 as usize);
            if floor_y < pixels.size().height_usize() {
                pixels.set_pixel(x, floor_y, &[0, 0, 255, 255]); // Blue
                // Draw a second pixel to make it more visible
                if floor_y > 0 {
                    pixels.set_pixel(x, floor_y - 1, &[0, 0, 255, 255]);
                }
            }
        }

        // Draw current segment bounds in green
        if self.rw_startx < self.rw_stopx {
            for x in (self.rw_startx as u32 as usize)
                ..=(self.rw_stopx.min(pixels.size().width() as f32 - 1.0) as u32 as usize)
            {
                // Draw top of seg
                let top_y = (self.topfrac as u32 as usize);
                if top_y < pixels.size().height_usize() {
                    pixels.set_pixel(x, top_y, &[0, 255, 0, 255]); // Green
                }

                // Draw bottom of seg
                let bottom_y = (self.bottomfrac as u32 as usize);
                if bottom_y < pixels.size().height_usize() {
                    pixels.set_pixel(x, bottom_y, &[0, 255, 0, 255]); // Green
                }
            }
        }

        // Highlight any problem areas where ceiling > floor
        for x in 0..pixels.size().width_usize() {
            if rdata.portal_clip.ceilingclip[x] >= rdata.portal_clip.floorclip[x] {
                // This is an error condition - draw a yellow vertical line
                for y in 0..pixels.size().height_usize() {
                    pixels.set_pixel(x, y, &[255, 255, 0, 128]); // Semi-transparent yellow
                }
            }
        }
    }

    #[cfg(feature = "debug_seg_invert")]
    fn highlight_inverted_clips(&self, rdata: &RenderData, pixels: &mut impl PixelBuffer) {
        let width = pixels.size().width_usize();
        let height = pixels.size().height_usize();

        let mut inverted_count = 0;
        let mut first_inverted = None;

        for x in 0..width {
            let ceiling = rdata.portal_clip.ceilingclip[x];
            let floor = rdata.portal_clip.floorclip[x];

            if ceiling >= floor {
                inverted_count += 1;
                if first_inverted.is_none() {
                    first_inverted = Some(x);
                }

                // Draw a vertical magenta line at each inverted column
                for y in 0..height {
                    let mut pixel = [255, 0, 255, 50]; // Magenta
                    let existing = pixels.read_pixel(x, y);
                    pixel[0] = pixel[0] / 2 + existing[0] / 2;
                    pixel[1] = pixel[1] / 2 + existing[1] / 2;
                    pixel[2] = pixel[2] / 2 + existing[2] / 2;

                    pixels.set_pixel(x, y, &pixel);
                }
            }
        }

        if inverted_count > 0 {
            warn!(
                "CLIP INVERSION: Found {} columns with ceiling >= floor. First at x={}",
                inverted_count,
                first_inverted.unwrap()
            );
        }
    }
}
