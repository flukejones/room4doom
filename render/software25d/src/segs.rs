#[cfg(feature = "hprof")]
use coarse_prof::profile;
use game_config::tic_cmd::{LOOKDIRMAX, LOOKDIRS};
use level::{LineDefFlags, Segment};
use log::warn;
use math::{ANG90, ANG180, ANGLETOFINESHIFT, Angle, Bam, FixedT, fine_tan};
use pic_data::{FlatPic, PicData};
use render_common::{DrawBuffer, RenderView};
use std::ptr::NonNull;

use crate::utilities::scale_from_view_angle;

use super::RenderData;
use super::defs::{DrawSeg, MAXDRAWSEGS, SIL_BOTH, SIL_BOTTOM, SIL_NONE, SIL_TOP};

/// Flat texture interpolation interval — exact samples every N rows,
/// linear stepping between. Must be power of 2.
const FLAT_INTERP_INTERVAL: usize = 4;
const FLAT_INTERP_SHIFT: i32 = 2; // log2(FLAT_INTERP_INTERVAL)

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
    maskedtexturecol: FixedT,
    // Texture exists?
    toptexture: bool,
    bottomtexture: bool,
    midtexture: bool,
    //
    rw_normalangle: Angle<Bam>,
    // regular wall
    rw_startx: FixedT,
    rw_stopx: FixedT,
    rw_centerangle: Angle<Bam>,
    rw_offset: FixedT,
    rw_distance: FixedT, // In R_ScaleFromGlobalAngle? Compute when needed
    rw_scale: FixedT,
    rw_scalestep: FixedT,
    rw_midtexturemid: FixedT,
    rw_toptexturemid: FixedT,
    rw_bottomtexturemid: FixedT,

    pixhigh: FixedT,
    pixlow: FixedT,
    pixhighstep: FixedT,
    pixlowstep: FixedT,

    topfrac: FixedT,
    topstep: FixedT,
    bottomfrac: FixedT,
    bottomstep: FixedT,

    worldtop: FixedT,
    worldbottom: FixedT,
    worldhigh: FixedT,
    worldlow: FixedT,

    /// Stores the column number of the texture required for this opening
    pub(super) openings: Vec<FixedT>,
    lastopening: FixedT,
    /// Light level for the wall
    wall_lights: usize,
    pub yslopes: Vec<Vec<FixedT>>,
    pub look_yslope: usize,
    pub centery: FixedT,
    pub screen_x: Vec<Angle<Bam>>,
    pub screen_x_scale: Vec<FixedT>,
    pub wide_ratio: FixedT,

    sky_doubled: bool,
    sky_mid: FixedT,

    dc_iscale: FixedT,
}

impl SegRender {
    pub fn new(screen_width: usize, screen_height: usize, xtoviewangle: Vec<u32>) -> Self {
        // OG Doom: xtoviewangle built from integer viewangletox inversion.
        // No float — pure BAM angles.
        let screen_x: Vec<Angle<Bam>> = xtoviewangle
            .iter()
            .map(|&bam| Angle::<Bam>::from_bam(bam))
            .collect();

        let wide_ratio = FixedT::from_f32(screen_height as f32 / screen_width as f32 * 1.6);
        let screen_x_scale: Vec<FixedT> = screen_x
            .iter()
            .map(|x| 1 / x.cos_fixedt() * wide_ratio)
            .collect();

        let mut s = Self {
            segtextured: false,
            markfloor: false,
            markceiling: false,
            maskedtexture: false,
            maskedtexturecol: FixedT::from(-1),
            toptexture: false,
            bottomtexture: false,
            midtexture: false,
            rw_normalangle: Angle::<Bam>::default(),
            rw_startx: FixedT::ZERO,
            rw_stopx: FixedT::ZERO,
            rw_centerangle: Angle::<Bam>::default(),
            rw_offset: FixedT::ZERO,
            rw_distance: FixedT::ZERO,
            rw_scale: FixedT::ZERO,
            rw_scalestep: FixedT::ZERO,
            rw_midtexturemid: FixedT::ZERO,
            rw_toptexturemid: FixedT::ZERO,
            rw_bottomtexturemid: FixedT::ZERO,
            pixhigh: FixedT::ZERO,
            pixlow: FixedT::ZERO,
            pixhighstep: FixedT::ZERO,
            pixlowstep: FixedT::ZERO,
            topfrac: FixedT::ZERO,
            topstep: FixedT::ZERO,
            bottomfrac: FixedT::ZERO,
            bottomstep: FixedT::ZERO,
            worldtop: FixedT::ZERO,
            worldbottom: FixedT::ZERO,
            worldhigh: FixedT::ZERO,
            worldlow: FixedT::ZERO,
            wall_lights: 0,
            openings: {
                let mut o = vec![FixedT::MAX; screen_width * screen_height];
                for i in 0..screen_width {
                    o[i] = FixedT::from(-1);
                }
                o
            },
            lastopening: FixedT::from((screen_width * 2) as i32),
            yslopes: unsafe {
                (0..LOOKDIRS)
                    .map(|_| vec![FixedT::ZERO; screen_height + 2])
                    .collect()
            },
            look_yslope: 0,
            centery: FixedT::ZERO,
            screen_x,
            screen_x_scale,
            wide_ratio,
            sky_doubled: false,
            sky_mid: FixedT::ZERO,
            dc_iscale: FixedT::ZERO,
        };
        s.set_view_height(screen_height);
        s
    }

    /// Recompute view-height-dependent values (R_ExecuteSetViewSize
    /// equivalent).
    pub fn set_view_height(&mut self, view_height: usize) {
        let screen_width = self.screen_x.len() - 1;
        // Screenheightarray (openings[width..2*width])
        for i in 0..screen_width {
            self.openings[screen_width + i] = FixedT::from(view_height as i32);
        }
        // Yslopes — stored as [look_dir][y] for contiguous scanline slicing.
        // Center at view_height/2 + (j - LOOKDIRMAX) so that at pitch=0
        // (look_yslope = LOOKDIRMAX), the yslope center aligns with centery.
        let num_y = self.yslopes[0].len();
        unsafe {
            for j in 0..LOOKDIRS {
                for y in 0..num_y {
                    let dy = y as f32 - (view_height as f32 / 2.0 + (j - LOOKDIRMAX) as f32) + 0.5;
                    self.yslopes[j as usize][y] =
                        FixedT::from_f32(screen_width as f32 / 2.0 / dy.abs());
                }
            }
        }
        self.centery = FixedT::from(view_height as i32 / 2);
        self.sky_doubled = view_height != 200;
        self.sky_mid =
            FixedT::from((view_height / 2 - if view_height != 200 { 12 } else { 6 }) as i32);
    }

    pub fn clear(&mut self) {
        let width = self.screen_x.len() - 1;
        self.lastopening = FixedT::from((width * 2) as i32);
    }

    pub fn set_view_pitch(&mut self, pitch: i16, half_screen_height: FixedT) {
        unsafe {
            self.look_yslope = (LOOKDIRMAX as i16 + pitch) as usize;
        }
        self.centery = half_screen_height + FixedT::from(pitch as i32);
    }

    /// R_StoreWallRange - r_segs
    pub(crate) fn store_wall_range(
        &mut self,
        start: FixedT,
        stop: FixedT,
        seg: &Segment,
        view: &RenderView,
        rdata: &mut RenderData,
        pic_data: &PicData,
        rend: &mut impl DrawBuffer,
    ) {
        #[cfg(feature = "hprof")]
        profile!("store_wall_range");
        let size = rend.size();
        if start < FixedT::ZERO || start > FixedT::from(size.width()) || start > stop {
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
        linedef.flags.insert(LineDefFlags::Mapped);
        // TODO: return if in automap

        let seg_angle_bam: Angle<Bam> = seg.angle.convert();
        let ang90 = Angle::<Bam>::from_bam(ANG90);
        self.rw_normalangle = seg_angle_bam + ang90;
        let mut offsetangle = self.rw_normalangle - rdata.rw_angle1;

        let distangle: Angle<Bam> = ang90 - offsetangle;
        let hyp = math::r_point_to_dist(seg.v1.x_fp, seg.v1.y_fp, view.x, view.y);
        let sin_val = distangle.sin_fixedt();
        self.rw_distance = hyp * sin_val;

        ds_p.x1 = start;
        self.rw_startx = ds_p.x1;
        ds_p.x2 = stop;
        self.rw_stopx = stop + 1;
        let half_width = FixedT::from(size.half_width());
        self.rw_scale = scale_from_view_angle(
            view.angle + self.screen_x[start.to_i32() as usize],
            self.rw_normalangle,
            self.rw_distance,
            view.angle,
            half_width,
        ) * self.wide_ratio;
        ds_p.scale1 = self.rw_scale;

        if stop > start {
            let scale2 = scale_from_view_angle(
                view.angle + self.screen_x[stop.to_i32() as usize],
                self.rw_normalangle,
                self.rw_distance,
                view.angle,
                half_width,
            ) * self.wide_ratio;
            ds_p.scale2 = scale2;

            let count = (stop - start).to_i32() as math::Inner;
            self.rw_scalestep = FixedT((scale2.0 - self.rw_scale.0) / count);
            ds_p.scalestep = self.rw_scalestep;
        } else {
            ds_p.scale2 = ds_p.scale1;
        }

        // calculate texture boundaries
        //  and decide if floor / ceiling marks are needed
        // `seg.sidedef.sector` is the front sector
        let frontsector = &seg.frontsector;
        self.worldtop = frontsector.ceilingheight - view.viewz;
        self.worldbottom = frontsector.floorheight - view.viewz;

        self.midtexture = false;
        self.toptexture = false;
        self.bottomtexture = false;
        self.maskedtexture = false;
        self.maskedtexturecol = FixedT::from(-1);

        if seg.backsector.is_none() {
            // single sided line
            self.markfloor = true;
            self.markceiling = true;
            self.midtexture = sidedef.midtexture.is_some();
            if linedef.flags.contains(LineDefFlags::UnpegBottom) {
                if let Some(mid_tex) = sidedef.midtexture {
                    let texture_column = pic_data.wall_pic_column(mid_tex, 0);
                    let vtop = frontsector.floorheight + FixedT::from(texture_column.len() as i32);
                    self.rw_midtexturemid = vtop - view.viewz;
                }
            } else {
                // top of texture at top
                self.rw_midtexturemid = self.worldtop;
            }
            self.rw_midtexturemid += sidedef.rowoffset;

            ds_p.silhouette = SIL_BOTH;
            // negonearray at [0..width], screenheightarray at [width..2*width]
            let width = FixedT::from(rend.size().width() as i32);
            ds_p.sprtopclip = Some(width); // screenheightarray
            ds_p.sprbottomclip = Some(FixedT::ZERO); // negonearray
            ds_p.bsilheight = FixedT::MAX;
            ds_p.tsilheight = FixedT::MIN;
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
            } else if backsector.floorheight >= view.viewz {
                ds_p.silhouette = SIL_BOTTOM;
                ds_p.bsilheight = FixedT::MAX;
            }

            if frontsector.ceilingheight < backsector.ceilingheight {
                ds_p.silhouette |= SIL_TOP;
                ds_p.tsilheight = frontsector.ceilingheight;
            } else if backsector.ceilingheight < view.viewz {
                ds_p.silhouette |= SIL_TOP;
                ds_p.tsilheight = FixedT::MIN;
            }

            // Commented out as this seems to fix the incorrect clipping of
            // sprites lower/higher than player and blocked by lower or upper
            // part of portal
            // if backsector.ceilingheight <= frontsector.floorheight {
            //     ds_p.sprbottomclip = Some(FixedT::ZERO); // start of negonearray
            //     ds_p.silhouette |= SIL_BOTTOM;
            //     ds_p.bsilheight = FixedT::MAX;
            // }

            // if backsector.floorheight >= frontsector.ceilingheight {
            //     ds_p.sprtopclip = Some(FixedT::ZERO);
            //     ds_p.silhouette |= SIL_TOP;
            //     ds_p.tsilheight = FixedT::MIN;
            // }

            self.worldhigh = backsector.ceilingheight - view.viewz;
            self.worldlow = backsector.floorheight - view.viewz;

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
                if linedef.flags.contains(LineDefFlags::UnpegTop) {
                    // texture top
                    self.rw_toptexturemid = self.worldtop;
                } else if let Some(top_tex) = sidedef.toptexture {
                    let texture_column = pic_data.wall_pic_column(top_tex, 0);
                    let vtop = backsector.ceilingheight + FixedT::from(texture_column.len() as i32);
                    // texture bottom
                    self.rw_toptexturemid = vtop - view.viewz;
                }
            }

            if self.worldlow > self.worldbottom {
                self.bottomtexture = sidedef.bottomtexture.is_some();
                if linedef.flags.contains(LineDefFlags::UnpegBottom) {
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
            self.rw_offset = hyp * offsetangle.sin_fixedt();
            // if self.rw_normalangle.rad() - rdata.rw_angle1.rad() < PI * 2.0 {
            self.rw_offset = -self.rw_offset;
            //  }
            self.rw_offset += sidedef.textureoffset + seg.offset;
            self.rw_centerangle = Angle::<Bam>::from_bam(ANG90) + view.angle - self.rw_normalangle;
            self.wall_lights = ((sidedef.sector.lightlevel >> 4) + view.extralight).min(15);
            // OG Doom fake contrast: horizontal segs (E-W) darken, vertical (N-S) lighten
            let seg_bam = seg.angle.to_bam();
            if seg_bam == 0 || seg_bam == ANG180 {
                if self.wall_lights > 0 {
                    self.wall_lights -= 1;
                }
            } else if seg_bam == ANG90 || seg_bam == math::ANG270 {
                if self.wall_lights < 15 {
                    self.wall_lights += 1;
                }
            }
        }

        // if a floor / ceiling plane is on the wrong side
        //  of the view plane, it is definitely invisible
        //  and doesn't need to be marked.
        if frontsector.floorheight > view.viewz {
            // above view plane
            self.markfloor = false;
        }

        if frontsector.ceilingheight <= view.viewz && frontsector.ceilingpic != pic_data.sky_num() {
            // below view plane
            self.markceiling = false;
        }

        // OG Doom shifts worldtop/worldbottom AND centeryfrac by >> 4 before
        // the multiply. This puts topfrac/bottomfrac in 20.12 format. Extract
        // screen rows with >> 12. The >> 4 on world heights provides 4 extra
        // integer bits to prevent overflow in the FixedMul.
        let half_height = FixedT(self.centery.0 >> 4);
        let wt = FixedT(self.worldtop.0 >> 4);
        let wb = FixedT(self.worldbottom.0 >> 4);
        self.topstep = -(wt * self.rw_scalestep);
        self.topfrac = half_height - (wt * self.rw_scale);

        self.bottomstep = -(wb * self.rw_scalestep);
        self.bottomfrac = half_height - (wb * self.rw_scale);

        if seg.backsector.is_some() {
            if self.worldhigh < self.worldtop {
                let wh = FixedT(self.worldhigh.0 >> 4);
                self.pixhigh = half_height - (wh * self.rw_scale);
                self.pixhighstep = -(wh * self.rw_scalestep);
            }

            if self.worldlow > self.worldbottom {
                let wl = FixedT(self.worldlow.0 >> 4);
                self.pixlow = half_height - (wl * self.rw_scale);
                self.pixlowstep = -(wl * self.rw_scalestep);
            }
        }

        self.render_seg_loop(seg, view, rdata, pic_data, rend);

        let ds_p = &mut rdata.drawsegs[rdata.ds_p];
        // OG: memcpy(lastopening, ceilingclip+start,
        // sizeof(*lastopening)*(rw_stopx-start))
        let count = (self.rw_stopx - start).to_i32() as usize;
        if (ds_p.silhouette & SIL_TOP != 0 || self.maskedtexture) && ds_p.sprtopclip.is_none() {
            let last = self.lastopening.to_i32() as usize;
            let src_start = start.to_i32() as usize;
            for i in 0..count {
                if last + i >= self.openings.len() {
                    break;
                }
                self.openings[last + i] = rdata.portal_clip.ceilingclip[src_start + i];
            }
            ds_p.sprtopclip = Some(self.lastopening - start);
            self.lastopening += self.rw_stopx - start;
        }

        if (ds_p.silhouette & SIL_BOTTOM != 0 || self.maskedtexture) && ds_p.sprbottomclip.is_none()
        {
            let last = self.lastopening.to_i32() as usize;
            let src_start = start.to_i32() as usize;
            for i in 0..count {
                if last + i >= self.openings.len() {
                    break;
                }
                self.openings[last + i] = rdata.portal_clip.floorclip[src_start + i];
            }
            ds_p.sprbottomclip = Some(self.lastopening - start);
            self.lastopening += self.rw_stopx - start;
        }

        if ds_p.silhouette & SIL_TOP == 0 && self.maskedtexture {
            ds_p.silhouette |= SIL_TOP;
            ds_p.tsilheight = FixedT::MIN;
        }

        if ds_p.silhouette & SIL_BOTTOM == 0 && self.maskedtexture {
            ds_p.silhouette |= SIL_BOTTOM;
            ds_p.bsilheight = FixedT::MAX;
        }
        rdata.ds_p += 1;
    }

    /// Per-column wall and flat rendering loop (OG: `R_RenderSegLoop`).
    ///
    /// Iterates each screen column in `[rw_startx, rw_stopx)`:
    /// - Clips ceiling/floor spans against portal clip arrays
    /// - Draws sky or ceiling flat above the wall
    /// - Draws floor flat below the wall
    /// - Computes texture column from view angle and seg offset
    /// - Draws mid/top/bottom wall textures for one-sided and two-sided lines
    /// - Updates portal clip arrays for subsequent rendering passes
    /// - Stores masked texture column indices in the openings array
    fn render_seg_loop(
        &mut self,
        seg: &Segment,
        view: &RenderView,
        rdata: &mut RenderData,
        pic_data: &PicData,
        rend: &mut impl DrawBuffer,
    ) {
        #[cfg(feature = "hprof")]
        profile!("render_seg_loop");
        // yl is the pixel location, it is the result of converting the topfrac to int
        let mut yl: FixedT;
        let mut yh: FixedT;
        let mut top: FixedT;
        let mut bottom: FixedT;
        let mut mid: FixedT;
        let mut angle;
        let mut texture_column = 0;
        let sidedef = seg.sidedef.clone();

        let flats_total_light = ((seg.frontsector.lightlevel >> 4) + view.extralight).min(15);
        let ceil_height = (seg.frontsector.ceilingheight - view.viewz).doom_abs();
        let ceil_tex = pic_data.get_flat(seg.frontsector.ceilingpic);
        let floor_height = (seg.frontsector.floorheight - view.viewz).doom_abs();
        let floor_tex = pic_data.get_flat(seg.frontsector.floorpic);

        while self.rw_startx < self.rw_stopx {
            let clip_index = self.rw_startx.to_i32() as usize;
            // The yl and yh blocks are what affect wall clipping the most. You can make
            // shorter/taller. topfrac here is calulated in previous function
            // and is the starting point that topstep is added to
            top = rdata.portal_clip.ceilingclip[clip_index] + 1;
            // OG: yl = (topfrac + HEIGHTUNIT - 1) >> HEIGHTBITS (ceiling in 20.12)
            yl = FixedT::from((self.topfrac.0 + 0xFFF) >> 12);
            if yl < top {
                yl = top;
            }

            let x_angle = view.angle + self.screen_x[clip_index];
            let cos = x_angle.cos_fixedt();
            let sin = x_angle.sin_fixedt();
            let distscale = self.screen_x_scale[self.rw_startx.to_i32() as usize];

            if self.markceiling {
                bottom = yl - 1;
                if bottom >= rdata.portal_clip.floorclip[clip_index] {
                    bottom = rdata.portal_clip.floorclip[clip_index] - 1;
                }
                if top <= bottom {
                    if seg.frontsector.ceilingpic == pic_data.sky_num() {
                        let screen_x_angle = self.screen_x[self.rw_startx.to_i32() as usize];
                        let sky_bam = (view.angle + screen_x_angle).inner().0;
                        // BAM >> 22 gives ~1024 values per full rotation (4x wrap for 256-wide sky)
                        let sky_col_idx = (sky_bam >> 22) as usize;
                        let sky_column = pic_data.wall_pic_column(pic_data.sky_pic(), sky_col_idx);

                        self.dc_iscale = FixedT(58327); // 0.89 in 16.16
                        self.draw_wall_column(
                            sky_column,
                            self.sky_mid,
                            top.to_i32(),
                            bottom.to_i32(),
                            true,
                            pic_data,
                            rend,
                        );
                    } else {
                        let y0 = top.to_i32().max(0) as usize;
                        let y1 = bottom.to_i32().max(0) as usize;
                        self.draw_flat_column(
                            ceil_tex,
                            view.x,
                            view.y,
                            ceil_height,
                            flats_total_light,
                            cos,
                            sin,
                            distscale,
                            y0,
                            y1,
                            pic_data,
                            rend,
                        );
                    }
                    rdata.portal_clip.ceilingclip[clip_index] = bottom;
                }
            }

            bottom = rdata.portal_clip.floorclip[clip_index] - 1;
            // OG: yh = bottomfrac >> HEIGHTBITS (floor in 20.12)
            yh = FixedT::from(self.bottomfrac.0 >> 12);
            if yh > bottom {
                yh = bottom;
            }

            if self.markfloor {
                top = yh + 1;
                if top <= rdata.portal_clip.ceilingclip[clip_index] {
                    top = rdata.portal_clip.ceilingclip[clip_index] + 1;
                }
                if top <= bottom {
                    // Must clip walls to floors if drawn
                    rdata.portal_clip.floorclip[clip_index] = top;
                    let y0 = top.to_i32().max(0) as usize;
                    let y1 = bottom.to_i32().max(0) as usize;
                    self.draw_flat_column(
                        floor_tex,
                        view.x,
                        view.y,
                        floor_height,
                        flats_total_light,
                        cos,
                        sin,
                        distscale,
                        y0,
                        y1,
                        pic_data,
                        rend,
                    );
                }
            }

            if self.segtextured {
                angle = self.rw_centerangle + self.screen_x[self.rw_startx.to_i32() as usize];
                let fine_idx = (angle.inner().0 >> ANGLETOFINESHIFT) as usize;
                let tan_val = fine_tan(fine_idx);
                texture_column = (self.rw_offset - self.rw_distance * tan_val)
                    .doom_abs()
                    .to_i32() as usize;

                self.dc_iscale = if self.rw_scale.doom_abs() < FixedT(1) {
                    FixedT::MAX
                } else {
                    1 / self.rw_scale
                };
            }

            if self.midtexture {
                if yl <= yh {
                    if let Some(mid_tex) = sidedef.midtexture {
                        let texture_column = pic_data.wall_pic_column(mid_tex, texture_column);
                        self.draw_wall_column(
                            texture_column,
                            self.rw_midtexturemid,
                            yl.to_i32(),
                            yh.to_i32(),
                            false,
                            pic_data,
                            rend,
                        );
                    };
                    rdata.portal_clip.ceilingclip[clip_index] = view.viewheight;
                    rdata.portal_clip.floorclip[clip_index] = FixedT::from(-1);
                }
            } else {
                if self.toptexture {
                    // floor vs ceil affects how things align in slightly off ways
                    // OG: mid = pixhigh >> HEIGHTBITS
                    mid = FixedT::from(self.pixhigh.0 >> 12);
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
                                yl.to_i32(),
                                mid.to_i32(),
                                false,
                                pic_data,
                                rend,
                            );
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
                    // OG: mid = (pixlow + HEIGHTUNIT - 1) >> HEIGHTBITS
                    mid = FixedT::from((self.pixlow.0 + 0xFFF) >> 12);
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
                                mid.to_i32(),
                                yh.to_i32(),
                                false,
                                pic_data,
                                rend,
                            );
                            rdata.portal_clip.floorclip[clip_index] = mid;
                        }
                    } else {
                        rdata.portal_clip.floorclip[clip_index] = yh + 1;
                    }
                } else if self.markfloor {
                    rdata.portal_clip.floorclip[clip_index] = yh + 1;
                }

                if self.maskedtexture {
                    let i = (self.maskedtexturecol + self.rw_startx).to_i32() as usize;
                    if self.openings.len() > i {
                        self.openings[i] = FixedT::from(texture_column as i32);
                    }
                }
            }

            self.rw_startx += 1;
            self.rw_scale += self.rw_scalestep;
            self.topfrac += self.topstep;
            self.bottomfrac += self.bottomstep;
        }
    }

    /// Rasterize one textured wall column. In sky mode, texel indices are
    /// halved and dc_iscale is caller-supplied.
    #[inline]
    fn draw_wall_column(
        &mut self,
        texture_column: &[usize],
        dc_texturemid: FixedT,
        y_start: i32,
        mut y_end: i32,
        sky: bool,
        pic_data: &PicData,
        pixels: &mut impl DrawBuffer,
    ) {
        #[cfg(feature = "hprof")]
        profile!("draw_wall_column");
        let dc_x = self.rw_startx.to_i32();
        let y_start = y_start.max(0);
        y_end = y_end.clamp(0, pixels.size().height() - 1);
        if y_start > y_end || dc_x < 0 || dc_x >= pixels.size().width() {
            return;
        }

        let pal = pic_data.palette();
        let mut frac = dc_texturemid + (FixedT::from(y_start) - self.centery) * self.dc_iscale;

        let mut pos = pixels.get_buf_index(self.rw_startx.to_i32() as usize, y_start as usize);

        let colourmap = if !sky {
            pic_data.vert_light_colourmap(self.wall_lights, self.rw_scale.to_f32())
        } else {
            pic_data.colourmap(0)
        };

        for _ in y_start..=y_end {
            let mut select = (frac.to_i32() as usize) & 127;
            if sky && self.sky_doubled {
                select /= 2;
            }
            if select >= texture_column.len() {
                return;
            }
            let tc = texture_column[select];
            if tc < colourmap.len() {
                unsafe {
                    let c = *pal.get_unchecked(*colourmap.get_unchecked(tc));
                    *pixels.buf_mut().get_unchecked_mut(pos) = c;
                }
            }
            #[cfg(any())] // disabled
            {
                pixels.set_pixel(dc_x, i as u32 as usize, pal[colourmap[tc]]);
            }
            frac += self.dc_iscale;
            pos += pixels.pitch();
        }
    }

    /// Draws a vertical span of a floor or ceiling flat texture.
    ///
    /// Uses piecewise-linear interpolation with exact samples every
    /// `FLAT_INTERP_INTERVAL` rows and linear stepping between them.
    /// The tail (fewer than `FLAT_INTERP_INTERVAL` remaining rows) computes
    /// each texel exactly. Texture coordinates are derived from yslope-based
    /// perspective distance scaled by view angle cosine/sine.
    #[inline]
    fn draw_flat_column(
        &mut self,
        texture: &FlatPic,
        view_x: FixedT,
        view_y: FixedT,
        plane_height: FixedT,
        total_light: usize,
        cos: FixedT,
        sin: FixedT,
        distscale: FixedT,
        y_start: usize,
        mut y_end: usize,
        pic_data: &PicData,
        pixels: &mut impl DrawBuffer,
    ) {
        #[cfg(feature = "hprof")]
        profile!("draw_flat_column");
        let dc_x = self.rw_startx.to_i32();
        if dc_x < 0 || dc_x >= pixels.size().width() {
            return;
        }
        y_end = y_end.min(pixels.size().height_usize() - 1);
        if y_start > y_end {
            return;
        }

        let pal = pic_data.palette();
        let tex_len = texture.height - 1; // always square
        let tex_w = texture.width;
        let mut pos = pixels.get_buf_index(dc_x as usize, y_start);
        let pitch = pixels.pitch();
        let yslopes = &self.yslopes[self.look_yslope][y_start..=y_end];
        let total = yslopes.len();

        // Compute exact texture coords from a y_slope value
        #[inline(always)]
        fn sample_coords(
            plane_height: FixedT,
            distscale: FixedT,
            cos: FixedT,
            sin: FixedT,
            view_x: FixedT,
            neg_view_y: FixedT,
            y_slope: FixedT,
        ) -> (FixedT, FixedT) {
            let length = plane_height * y_slope * distscale;
            (view_x + cos * length, neg_view_y - sin * length)
        }

        let neg_view_y = -view_y;
        let mut i = 0;

        while i < total {
            let remaining = total - i;

            // Exact sample at current position
            let (x0, y0) = sample_coords(
                plane_height,
                distscale,
                cos,
                sin,
                view_x,
                neg_view_y,
                yslopes[i],
            );

            if remaining > FLAT_INTERP_INTERVAL {
                // Exact sample N pixels ahead
                let (x1, y1) = sample_coords(
                    plane_height,
                    distscale,
                    cos,
                    sin,
                    view_x,
                    neg_view_y,
                    yslopes[i + FLAT_INTERP_INTERVAL],
                );

                // Linear steps (wrapping: OG Doom relies on i32 wrap)
                let dx = FixedT(x1.0.wrapping_sub(x0.0) >> FLAT_INTERP_SHIFT);
                let dy = FixedT(y1.0.wrapping_sub(y0.0) >> FLAT_INTERP_SHIFT);

                let mut xfrac = x0;
                let mut yfrac = y0;

                for j in 0..FLAT_INTERP_INTERVAL {
                    let diminished_light = plane_height * yslopes[i + j];
                    let colourmap = pic_data.flat_light_colourmap(
                        total_light,
                        (diminished_light.to_i32() as u32 as usize) >> 4,
                    );

                    let x_step = (xfrac.doom_abs().to_i32() as usize) & tex_len;
                    let y_step = (yfrac.doom_abs().to_i32() as usize) & tex_len;

                    unsafe {
                        let tc = texture.data[y_step * tex_w + x_step];
                        let c = *pal.get_unchecked(*colourmap.get_unchecked(tc));
                        *pixels.buf_mut().get_unchecked_mut(pos) = c;
                    }
                    pos += pitch;
                    xfrac += dx;
                    yfrac += dy;
                }
                i += FLAT_INTERP_INTERVAL;
            } else {
                // Tail: fewer than N pixels, compute each exactly
                for j in 0..remaining {
                    let (xfrac, yfrac) = sample_coords(
                        plane_height,
                        distscale,
                        cos,
                        sin,
                        view_x,
                        neg_view_y,
                        yslopes[i + j],
                    );
                    let diminished_light = plane_height * yslopes[i + j];
                    let colourmap = pic_data.flat_light_colourmap(
                        total_light,
                        (diminished_light.to_i32() as u32 as usize) >> 4,
                    );

                    let x_step = (xfrac.doom_abs().to_i32() as usize) & tex_len;
                    let y_step = (yfrac.doom_abs().to_i32() as usize) & tex_len;

                    unsafe {
                        let tc = texture.data[y_step * tex_w + x_step];
                        let c = *pal.get_unchecked(*colourmap.get_unchecked(tc));
                        *pixels.buf_mut().get_unchecked_mut(pos) = c;
                    }
                    pos += pitch;
                }
                i += remaining;
            }
        }
    }

    #[cfg(feature = "debug_seg_clip")]
    pub(crate) fn draw_debug_clipping(&self, rdata: &RenderData, pixels: &mut impl DrawBuffer) {
        // Draw ceiling clip line in red
        for x in 0..pixels.size().width_usize() {
            let ceiling_y = rdata.portal_clip.ceilingclip[x].to_i32() as usize;
            if ceiling_y < pixels.size().height_usize() {
                pixels.set_pixel(x, ceiling_y, 0xFFFF0000); // Red
                if ceiling_y + 1 < pixels.size().height_usize() {
                    pixels.set_pixel(x, ceiling_y + 1, 0xFFFF0000);
                }
            }

            // Draw floor clip line in blue
            let floor_y = rdata.portal_clip.floorclip[x].to_i32() as usize;
            if floor_y < pixels.size().height_usize() {
                pixels.set_pixel(x, floor_y, 0xFF0000FF); // Blue
                if floor_y > 0 {
                    pixels.set_pixel(x, floor_y - 1, 0xFF0000FF);
                }
            }
        }

        // Draw current segment bounds in green
        if self.rw_startx < self.rw_stopx {
            for x in (self.rw_startx.to_i32() as usize)
                ..=(self
                    .rw_stopx
                    .clamp(FixedT::ZERO, FixedT::from(pixels.size().width() as i32 - 1))
                    .to_i32() as usize)
            {
                // Draw top of seg
                let top_y = self.topfrac.to_i32() as usize;
                if top_y < pixels.size().height_usize() {
                    pixels.set_pixel(x, top_y, 0xFF00FF00); // Green
                }

                // Draw bottom of seg
                let bottom_y = (self.bottomfrac.0 >> 12) as usize;
                if bottom_y < pixels.size().height_usize() {
                    pixels.set_pixel(x, bottom_y, 0xFF00FF00); // Green
                }
            }
        }

        // Highlight any problem areas where ceiling > floor
        for x in 0..pixels.size().width_usize() {
            if rdata.portal_clip.ceilingclip[x] >= rdata.portal_clip.floorclip[x] {
                // This is an error condition - draw a yellow vertical line
                for y in 0..pixels.size().height_usize() {
                    pixels.set_pixel(x, y, 0xFFFFFF00); // Semi-transparent yellow
                }
            }
        }
    }

    #[cfg(feature = "debug_seg_invert")]
    fn highlight_inverted_clips(&self, rdata: &RenderData, pixels: &mut impl DrawBuffer) {
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
                    let existing = pixels.read_pixel(x, y);
                    let er = (existing >> 16) as u8;
                    let eg = (existing >> 8) as u8;
                    let eb = existing as u8;
                    let r = 255u8 / 2 + er / 2;
                    let g = eg / 2;
                    let b = 255u8 / 2 + eb / 2;
                    let pixel = (r as u32) << 16 | (g as u32) << 8 | b as u32;
                    pixels.set_pixel(x, y, pixel);
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
