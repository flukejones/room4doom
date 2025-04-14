use crate::utilities::{point_to_dist_fixed, scale_from_view_angle_fixed, screen_to_angle};
#[cfg(feature = "hprof")]
use coarse_prof::profile;
use gameplay::log::warn;
use gameplay::tic_cmd::{LOOKDIRMAX, LOOKDIRMIN, LOOKDIRS};
use gameplay::{Angle, FlatPic, LineDefFlags, MapObject, PicData, Player, Segment};
use glam::Vec2;
use math::FixedPoint;
use render_trait::{PixelBuffer, RenderTrait, SOFT_PIXEL_CHANNELS};
use std::f32::consts::{FRAC_PI_2, PI, TAU};
use std::ptr::NonNull;
#[cfg(feature = "debug_draw")]
use std::thread::sleep;
#[cfg(feature = "debug_draw")]
use std::time::Duration;

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
    maskedtexturecol: FixedPoint,
    // Texture exists?
    toptexture: bool,
    bottomtexture: bool,
    midtexture: bool,
    //
    rw_normalangle: Angle,
    // regular wall
    rw_startx: FixedPoint,
    rw_stopx: FixedPoint,
    rw_centerangle: Angle,
    rw_offset: FixedPoint,
    rw_distance: FixedPoint, // In R_ScaleFromGlobalAngle? Compute when needed
    rw_scale: FixedPoint,
    rw_scalestep: FixedPoint,
    rw_midtexturemid: FixedPoint,
    rw_toptexturemid: FixedPoint,
    rw_bottomtexturemid: FixedPoint,

    pixhigh: FixedPoint,
    pixlow: FixedPoint,
    pixhighstep: FixedPoint,
    pixlowstep: FixedPoint,

    topfrac: FixedPoint,
    topstep: FixedPoint,
    bottomfrac: FixedPoint,
    bottomstep: FixedPoint,

    worldtop: FixedPoint,
    worldbottom: FixedPoint,
    worldhigh: FixedPoint,
    worldlow: FixedPoint,

    /// Stores the column number of the texture required for this opening
    pub(super) openings: Vec<FixedPoint>,
    lastopening: FixedPoint,
    /// Light level for the wall
    wall_lights: usize,
    pub yslopes: Vec<Vec<FixedPoint>>,
    pub look_yslope: usize,
    pub centery: FixedPoint,
    pub screen_x: Vec<f32>,
    pub screen_x_scale: Vec<f32>,
    pub fov: f32,
    pub fov_half: f32,
    pub wide_ratio: f32,

    sky_doubled: bool,
    sky_mid: FixedPoint,

    dc_iscale: FixedPoint,
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
            maskedtexturecol: FixedPoint::from(-1),
            toptexture: false,
            bottomtexture: false,
            midtexture: false,
            rw_normalangle: Angle::default(),
            rw_startx: FixedPoint::zero(),
            rw_stopx: FixedPoint::zero(),
            rw_centerangle: Angle::default(),
            rw_offset: FixedPoint::zero(),
            rw_distance: FixedPoint::zero(),
            rw_scale: FixedPoint::zero(),
            rw_scalestep: FixedPoint::zero(),
            rw_midtexturemid: FixedPoint::zero(),
            rw_toptexturemid: FixedPoint::zero(),
            rw_bottomtexturemid: FixedPoint::zero(),
            pixhigh: FixedPoint::zero(),
            pixlow: FixedPoint::zero(),
            pixhighstep: FixedPoint::zero(),
            pixlowstep: FixedPoint::zero(),
            topfrac: FixedPoint::zero(),
            topstep: FixedPoint::zero(),
            bottomfrac: FixedPoint::zero(),
            bottomstep: FixedPoint::zero(),
            worldtop: FixedPoint::zero(),
            worldbottom: FixedPoint::zero(),
            worldhigh: FixedPoint::zero(),
            worldlow: FixedPoint::zero(),
            wall_lights: 0,
            openings: vec![FixedPoint::max(); screen_width * screen_height],
            lastopening: FixedPoint::zero(),
            yslopes: (0..=screen_height + 1)
                .map(|y| unsafe {
                    (0..LOOKDIRS)
                        .map(|j| {
                            let dy =
                                y as f32 - (screen_height as f32 / 2.0 + (j - LOOKDIRMIN) as f32);
                            FixedPoint::from(screen_width as f32 / 2.0 / dy.abs())
                        })
                        .collect()
                })
                .collect(),
            look_yslope: 0,
            centery: FixedPoint::from(screen_height as f32 / 2.0),
            screen_x,
            screen_x_scale,
            fov,
            fov_half: fov / 2.0,
            wide_ratio,

            sky_doubled: screen_height != 200,
            sky_mid: FixedPoint::from(
                (screen_height / 2 - if screen_height != 200 { 12 } else { 6 }) as f32,
            ),

            dc_iscale: FixedPoint::zero(),
        }
    }

    pub const fn clear(&mut self) {
        self.lastopening = FixedPoint::zero();
    }

    /// # Safety
    /// Nothing else should be modifying `LOOKDIRMAX`
    pub unsafe fn set_view_pitch(&mut self, pitch: i16, half_screen_height: FixedPoint) {
        unsafe {
            self.look_yslope = (LOOKDIRMAX + pitch) as usize;
        }
        self.centery = half_screen_height + FixedPoint::from(pitch);
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
        start: FixedPoint,
        stop: FixedPoint,
        seg: &Segment,
        player: &Player,
        rdata: &mut RenderData,
        pic_data: &PicData,
        rend: &mut impl RenderTrait,
    ) {
        #[cfg(feature = "hprof")]
        profile!("store_wall_range");
        let size = rend.draw_buffer().size();

        // Check for invalid inputs
        if start < FixedPoint::zero() || start > FixedPoint::from(size.width_f32()) || start > stop
        {
            panic!("Bad R_RenderWallRange: {:?} to {:?}", start, stop);
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

        self.rw_normalangle = seg.angle + FRAC_PI_2; // widescreen: Leave as is
        let mut offsetangle = self.rw_normalangle - rdata.rw_angle1; // radians

        let mobj = unsafe { player.mobj_unchecked() };

        let distangle = Angle::new(FRAC_PI_2 - offsetangle.rad()); // widescreen: Leave as is
        // Calculate distance - convert to fixed point
        let hyp = point_to_dist_fixed(seg.v1.x, seg.v1.y, mobj.xy);
        self.rw_distance = hyp * FixedPoint::from(distangle.sin());

        ds_p.x1 = start;
        self.rw_startx = ds_p.x1;
        ds_p.x2 = stop;
        self.rw_stopx = stop + 1;

        // Calculate scale - convert to fixed point
        let angle_plus_screen = mobj.angle + self.screen_x[usize::from(start)];
        self.rw_scale = scale_from_view_angle_fixed(
            angle_plus_screen,
            self.rw_normalangle,
            self.rw_distance,
            mobj.angle,
            size.half_width().into(),
        ) * self.wide_ratio;

        ds_p.scale1 = self.rw_scale;

        if stop > start {
            // Calculate scale at end point
            let angle_plus_screen_stop = mobj.angle + self.screen_x[usize::from(stop)];
            ds_p.scale2 = scale_from_view_angle_fixed(
                angle_plus_screen_stop,
                self.rw_normalangle,
                self.rw_distance.into(),
                mobj.angle,
                size.half_width().into(),
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
        self.worldtop = FixedPoint::from(frontsector.ceilingheight - player.viewz);
        self.worldbottom = FixedPoint::from(frontsector.floorheight - player.viewz);

        self.midtexture = false;
        self.toptexture = false;
        self.bottomtexture = false;
        self.maskedtexture = false;
        self.maskedtexturecol = FixedPoint::from(-1);

        if seg.backsector.is_none() {
            // single sided line
            self.markfloor = true;
            self.markceiling = true;
            self.midtexture = sidedef.midtexture.is_some();
            if linedef.flags & LineDefFlags::UnpegBottom as u32 != 0 {
                if let Some(mid_tex) = sidedef.midtexture {
                    let texture_column = pic_data.wall_pic_column(mid_tex, 0);
                    let vtop = FixedPoint::from(
                        frontsector.floorheight as i32 + texture_column.len() as i32,
                    );
                    self.rw_midtexturemid = vtop - FixedPoint::from(player.viewz);
                }
            } else {
                // top of texture at top
                self.rw_midtexturemid = self.worldtop;
            }
            self.rw_midtexturemid = self.rw_midtexturemid + FixedPoint::from(sidedef.rowoffset);

            ds_p.silhouette = SIL_BOTH;
            ds_p.sprtopclip = Some(FixedPoint::zero()); // start of screenheightarray
            ds_p.sprbottomclip = Some(FixedPoint::zero()); // start of negonearray
            ds_p.bsilheight = FixedPoint::max();
            ds_p.tsilheight = FixedPoint::min();
        } else {
            let backsector = seg.backsector.as_ref().unwrap();
            // two sided line
            ds_p.sprtopclip = None;
            ds_p.sprbottomclip = None;
            ds_p.silhouette = SIL_NONE;

            if frontsector.floorheight > backsector.floorheight {
                ds_p.silhouette = SIL_BOTTOM;
                ds_p.bsilheight = FixedPoint::from(frontsector.floorheight);
            } else if backsector.floorheight >= player.viewz {
                ds_p.silhouette = SIL_BOTTOM;
                ds_p.bsilheight = FixedPoint::max();
            }

            if frontsector.ceilingheight < backsector.ceilingheight {
                ds_p.silhouette |= SIL_TOP;
                ds_p.tsilheight = FixedPoint::from(frontsector.ceilingheight);
            } else if backsector.ceilingheight < player.viewz {
                ds_p.silhouette |= SIL_TOP;
                ds_p.tsilheight = FixedPoint::min();
            }

            self.worldhigh = FixedPoint::from(backsector.ceilingheight - player.viewz);
            self.worldlow = FixedPoint::from(backsector.floorheight - player.viewz);

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
                    let vtop = FixedPoint::from(
                        backsector.ceilingheight as i32 + texture_column.len() as i32,
                    );
                    // texture bottom
                    self.rw_toptexturemid = vtop - FixedPoint::from(player.viewz);
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

            self.rw_toptexturemid = self.rw_toptexturemid + FixedPoint::from(sidedef.rowoffset);
            self.rw_bottomtexturemid =
                self.rw_bottomtexturemid + FixedPoint::from(sidedef.rowoffset);

            self.maskedtexture = true;
            self.maskedtexturecol = self.lastopening - self.rw_startx;
            ds_p.maskedtexturecol = self.maskedtexturecol;

            self.lastopening = self.lastopening + (self.rw_stopx - self.rw_startx);
        }

        // calculate rw_offset (only needed for textured lines)
        self.segtextured =
            self.midtexture || self.toptexture || self.bottomtexture || self.maskedtexture;

        if self.segtextured {
            offsetangle = self.rw_normalangle - rdata.rw_angle1;
            self.rw_offset = hyp * FixedPoint::from(offsetangle.sin());

            // Adjust offset direction
            self.rw_offset = -self.rw_offset;

            self.rw_offset = self.rw_offset + FixedPoint::from(sidedef.textureoffset + seg.offset);
            self.rw_centerangle = mobj.angle - self.rw_normalangle;
            self.wall_lights = (sidedef.sector.lightlevel >> 4) + player.extralight;
            if (seg.angle.rad().abs() == PI || seg.angle.rad() == 0.0) && self.wall_lights > 0 {
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

        // Convert calculations to fixed point
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

        let ds_p = &mut rdata.drawsegs[rdata.ds_p];
        if (ds_p.silhouette & SIL_TOP != 0 || self.maskedtexture) && ds_p.sprtopclip.is_none() {
            for (i, n) in rdata
                .portal_clip
                .ceilingclip
                .iter()
                .skip(usize::from(start))
                .enumerate()
            {
                let last = usize::from(self.lastopening);
                if last + i >= self.openings.len() {
                    break;
                }
                self.openings[last + i] = *n;
                if i as i32 >= i32::from(self.rw_stopx - start) {
                    break;
                }
            }
            ds_p.sprtopclip = Some(self.lastopening - start);
            self.lastopening = self.lastopening + (self.rw_stopx - start);
        }

        if (ds_p.silhouette & SIL_BOTTOM != 0 || self.maskedtexture) && ds_p.sprbottomclip.is_none()
        {
            for (i, n) in rdata
                .portal_clip
                .floorclip
                .iter()
                .skip(usize::from(start))
                .enumerate()
            {
                let last = usize::from(self.lastopening);
                if last + i >= self.openings.len() {
                    break;
                }
                self.openings[last + i] = *n;
                if i >= usize::from(self.rw_stopx - start) {
                    break;
                }
            }
            ds_p.sprbottomclip = Some(self.lastopening - start);
            self.lastopening = self.lastopening + (self.rw_stopx - start);
        }

        if ds_p.silhouette & SIL_TOP == 0 && self.maskedtexture {
            ds_p.silhouette |= SIL_TOP;
            ds_p.tsilheight = FixedPoint::min();
        }

        if ds_p.silhouette & SIL_BOTTOM == 0 && self.maskedtexture {
            ds_p.silhouette |= SIL_BOTTOM;
            ds_p.bsilheight = FixedPoint::max();
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
        let mut yl: FixedPoint;
        let mut yh: FixedPoint;
        let mut top: FixedPoint;
        let mut bottom: FixedPoint;
        let mut mid: FixedPoint;
        let mut angle;
        let mut texture_column = 0;
        let size = rend.draw_buffer().size().clone();
        let sidedef = seg.sidedef.clone();

        let flats_total_light = (seg.frontsector.lightlevel >> 4) + player.extralight;
        let ceil_height = FixedPoint::from((seg.frontsector.ceilingheight - player.viewz).abs());
        let ceil_tex = pic_data.get_flat(seg.frontsector.ceilingpic);
        let floor_height = FixedPoint::from((seg.frontsector.floorheight - player.viewz).abs());
        let floor_tex = pic_data.get_flat(seg.frontsector.floorpic);

        while self.rw_startx < self.rw_stopx {
            let clip_index = usize::from(self.rw_startx);

            // The yl and yh blocks are what affect wall clipping the most
            yl = self.topfrac.truncate();
            if yl <= rdata.portal_clip.ceilingclip[clip_index] {
                yl = rdata.portal_clip.ceilingclip[clip_index] + 1;
            }

            let x_angle = mobj.angle + self.screen_x[clip_index];
            let cos = FixedPoint::from_radian(x_angle.rad()).cos();
            let sin = FixedPoint::from_radian(x_angle.rad()).sin();
            let distscale = FixedPoint::from(self.screen_x_scale[usize::from(self.rw_startx)]);

            if self.markceiling {
                top = rdata.portal_clip.ceilingclip[clip_index] + 1;
                bottom = yl;
                if bottom >= rdata.portal_clip.floorclip[clip_index] {
                    bottom = rdata.portal_clip.floorclip[clip_index] - 1;
                }
                if top <= bottom {
                    if seg.frontsector.ceilingpic == pic_data.sky_num() {
                        let screen_x_degrees =
                            screen_to_angle(self.fov, self.rw_startx.into(), size.half_width_f32());
                        let sky_angle =
                            (mobj.angle.rad() + screen_x_degrees + TAU * 2.0).to_degrees() * 2.8444; // 2.8444 for correct skybox width
                        let sky_column = pic_data
                            .wall_pic_column(pic_data.sky_pic(), sky_angle.abs() as u32 as usize);

                        self.dc_iscale = FixedPoint::from(0.89);
                        self.draw_wall_column(
                            sky_column,
                            self.sky_mid,
                            top,
                            bottom,
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
                            usize::from(top),
                            usize::from(bottom),
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
                    rdata.portal_clip.ceilingclip[clip_index] = bottom - 1;
                }
            }

            yh = self.bottomfrac.truncate();
            if yh >= rdata.portal_clip.floorclip[clip_index] {
                yh = rdata.portal_clip.floorclip[clip_index] - 1;
            }

            if self.markfloor {
                top = yh + 1;
                bottom = rdata.portal_clip.floorclip[clip_index] - 1;
                if top <= rdata.portal_clip.ceilingclip[clip_index] {
                    top = rdata.portal_clip.ceilingclip[clip_index] + 1;
                }
                if top <= bottom {
                    // Must clip walls to floors if drawn
                    rdata.portal_clip.floorclip[clip_index] = top + 1;
                    self.draw_flat_column(
                        floor_tex,
                        mobj.xy,
                        floor_height,
                        flats_total_light,
                        cos,
                        sin,
                        distscale,
                        usize::from(top),
                        usize::from(bottom),
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
                angle = self.rw_centerangle + self.screen_x[usize::from(self.rw_startx)];

                // Calculate texture column - convert to fixed point
                texture_column = usize::from(
                    (self.rw_offset - angle.tan() * self.rw_distance), // without floor we get overflow in draw
                );

                self.dc_iscale = 1.0 / self.rw_scale;
            }

            if self.midtexture {
                if yl <= yh {
                    if let Some(mid_tex) = sidedef.midtexture {
                        let texture_column = pic_data.wall_pic_column(mid_tex, texture_column);
                        self.draw_wall_column(
                            texture_column,
                            self.rw_midtexturemid,
                            yl,
                            yh,
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
                    rdata.portal_clip.ceilingclip[clip_index] = FixedPoint::from(player.viewheight);
                    rdata.portal_clip.floorclip[clip_index] = FixedPoint::from(-1);
                }
            } else {
                if self.toptexture {
                    // floor vs ceil affects how things align in slightly off ways
                    mid = self.pixhigh;
                    self.pixhigh += self.pixhighstep;

                    if mid >= rdata.portal_clip.floorclip[clip_index] {
                        mid = rdata.portal_clip.floorclip[clip_index] - 1;
                    }
                    if mid >= yl {
                        if let Some(top_tex) = sidedef.toptexture {
                            let texture_column = pic_data.wall_pic_column(top_tex, texture_column);
                            self.draw_wall_column(
                                texture_column,
                                self.rw_toptexturemid,
                                yl,
                                mid,
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
                        rdata.portal_clip.ceilingclip[clip_index] = yl - 1;
                    }
                } else if self.markceiling {
                    rdata.portal_clip.ceilingclip[clip_index] = yl - 1;
                }

                if self.bottomtexture {
                    // floor vs ceil affects how things align in slightly off ways
                    mid = self.pixlow + 1;
                    self.pixlow += self.pixlowstep;

                    if mid <= rdata.portal_clip.ceilingclip[clip_index] {
                        mid = rdata.portal_clip.ceilingclip[clip_index] + 1;
                    }
                    if mid <= yh {
                        if let Some(bot_tex) = sidedef.bottomtexture {
                            let texture_column = pic_data.wall_pic_column(bot_tex, texture_column);
                            self.draw_wall_column(
                                texture_column,
                                self.rw_bottomtexturemid,
                                mid,
                                yh,
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
                        rdata.portal_clip.floorclip[clip_index] = yh + 1;
                    }
                } else if self.markfloor {
                    rdata.portal_clip.floorclip[clip_index] = yh + 1;
                }

                if self.maskedtexture {
                    let i = usize::from(self.maskedtexturecol + self.rw_startx);
                    if i < self.openings.len() {
                        self.openings[i] = FixedPoint::from(texture_column);
                    }
                }
            }

            self.rw_startx = self.rw_startx + 1;
            self.rw_scale = self.rw_scale + self.rw_scalestep;
            self.topfrac = self.topfrac + self.topstep;
            self.bottomfrac = self.bottomfrac + self.bottomstep;
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
        dc_texturemid: FixedPoint,
        y_start: FixedPoint,
        mut y_end: FixedPoint,
        sky: bool,
        pic_data: &PicData,
        pixels: &mut impl PixelBuffer,
    ) {
        #[cfg(feature = "hprof")]
        profile!("draw_wall_column");
        y_end = y_end.min_with_i32(pixels.size().height() - 1);

        let pal = pic_data.palette();
        let mut frac = dc_texturemid + (y_start - self.centery) * self.dc_iscale;

        let mut pos = pixels.get_buf_index(usize::from(self.rw_startx), usize::from(y_start));

        let colourmap = if !sky {
            pic_data.vert_light_colourmap(self.wall_lights, self.rw_scale.into())
        } else {
            pic_data.colourmap(0)
        };

        for _ in usize::from(y_start)..=usize::from(y_end) {
            let mut select = usize::from(frac) % texture_column.len();
            if sky && self.sky_doubled {
                select /= 2;
            }
            let tc = texture_column[select];
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
                pixels.set_pixel(usize::from(self.rw_startx), i as usize, &pal[colourmap[tc]]);
            }
            frac = frac + self.dc_iscale;
            pos += pixels.pitch();
            if pos + SOFT_PIXEL_CHANNELS >= pixels.buf_mut().len() {
                return;
            }
        }
    }

    #[inline]
    fn draw_flat_column(
        &mut self,
        texture: &FlatPic,
        viewxy: Vec2,
        plane_height: FixedPoint,
        total_light: usize,
        cos: FixedPoint,
        sin: FixedPoint,
        distscale: FixedPoint,
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
        let mut pos = pixels.get_buf_index(usize::from(self.rw_startx), y_start);

        for y_slope in self.yslopes[self.look_yslope][y_start..=y_end].iter() {
            let diminished_light = plane_height * *y_slope;
            // Calculate light colourmap for this position
            let colourmap =
                pic_data.flat_light_colourmap(total_light, usize::from(diminished_light) >> 4);

            // Calculate texture position
            let length = diminished_light * distscale;
            let xfrac = (cos * length) + FixedPoint::from(viewxy.x);
            let yfrac = (sin * length) + FixedPoint::from(viewxy.y);

            // Calculate texture coordinates
            let x_step = usize::from(xfrac) & tex_len;
            let y_step = usize::from(yfrac) & tex_len;

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
                let px = colourmap[texture.data[x_step][y_step]];
                pixels.set_pixel(fixed_to_float(self.rw_startx.0) as usize, y_start, &pal[px]);
            }
            pos += pixels.pitch();
        }
    }
}
