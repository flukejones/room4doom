use std::cmp;

use gameplay::{MapObjFlag, MapObject, SectorExt};
use level::{LineDefFlags, Sector};
use log::{error, warn};
use math::{ANG45, FRACBITS, FRACUNIT, FixedT, r_point_to_angle};

const FINEANGLES: usize = 8192;
use pic_data::PicData;
use render_common::{DrawBuffer, FUZZ_TABLE, RenderPspDef, RenderView, fuzz_darken};

use super::bsp::Software25D;
use super::defs::DrawSeg;

const FF_FULLBRIGHT: u32 = 0x8000;
const FF_FRAMEMASK: u32 = 0x7FFF;

#[derive(Clone, Copy, PartialEq, Default)]
pub struct VisSprite {
    x1: FixedT,
    x2: FixedT,
    // Line side calc
    gx: FixedT,
    gy: FixedT,
    // Bottom and top for clipping
    gz: FixedT,
    gzt: FixedT,
    // horizontal position of x1
    start_frac: FixedT,
    /// Scale for depth sort comparison (projection / tz).
    scale: FixedT,
    /// Scale for vertical drawing (scale * wide_ratio for world sprites,
    /// same as scale for weapon sprites).
    draw_scale: FixedT,
    // negative if flipped
    x_iscale: FixedT,
    texture_mid: FixedT,
    /// The index in to patches array
    patch: usize,
    /// The index used to fetch colourmap for drawing
    light_level: usize,
    mobj_flags: u32,
}

impl PartialOrd for VisSprite {
    fn partial_cmp(&self, other: &VisSprite) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for VisSprite {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        if self.scale < other.scale {
            cmp::Ordering::Less
        } else if self.scale > other.scale {
            cmp::Ordering::Greater
        } else {
            cmp::Ordering::Equal
        }
    }
}

impl Eq for VisSprite {}

impl VisSprite {
    pub fn new() -> Self {
        Self {
            x1: FixedT::ZERO,
            x2: FixedT::ZERO,
            gx: FixedT::ZERO,
            gy: FixedT::ZERO,
            gz: FixedT::ZERO,
            gzt: FixedT::ZERO,
            start_frac: FixedT::ZERO,
            scale: FixedT::ZERO,
            draw_scale: FixedT::ZERO,
            x_iscale: FixedT::ZERO,
            texture_mid: FixedT::ZERO,
            patch: 0,
            light_level: 0,
            mobj_flags: 0,
        }
    }
}

impl Software25D {
    pub(crate) fn add_sprites<'a>(
        &'a mut self,
        view: &RenderView,
        sector: &'a Sector,
        screen_width: u32,
        pic_data: &PicData,
    ) {
        // Need to track sectors as we recurse through BSP as the BSP
        // iteration is via subsectors, and sectors can be split in to
        // many subsectors
        if self.checked_sectors.contains(&sector.num) {
            return;
        }
        self.checked_sectors[self.checked_idx] = sector.num;
        if self.checked_idx < self.checked_sectors.len() {
            self.checked_idx += 1;
        }

        let light_level = ((sector.lightlevel >> 4) + view.extralight).min(15);
        <Sector as SectorExt>::run_func_on_thinglist(sector, |thing| {
            self.project_sprite(view, thing, light_level, screen_width, pic_data)
        });
    }

    const fn new_vissprite(&mut self) -> &mut VisSprite {
        let curr = self.next_vissprite;
        self.next_vissprite += 1;
        if curr >= self.vissprites.len() - 1 {
            // panic!("Exhausted vissprite allocation");
            self.next_vissprite -= 1;
        }
        &mut self.vissprites[curr]
    }

    /// Project a map object into screen space and create a `VisSprite` for it.
    ///
    /// - Transforms the thing's position into view-relative coordinates
    /// - Rejects things behind the viewer or too far off-screen
    /// - Resolves the sprite frame and rotation angle for the current view
    /// - Uses `viewangletox` LUT for X positioning to align with seg clipping
    /// - Populates a `VisSprite` with screen bounds, texture stepping, and
    ///   scale
    fn project_sprite(
        &mut self,
        view: &RenderView,
        thing: &MapObject,
        light_level: usize,
        screen_width: u32,
        pic_data: &PicData,
    ) -> bool {
        if thing.player().is_some() {
            return true;
        }

        let view_cos = view.angle.cos_fixedt();
        let view_sin = view.angle.sin_fixedt();

        // Sub-tic interpolation to match interpolated camera
        let lerp = |a: FixedT, b: FixedT| a + (b - a) * view.frac_fp;
        let sx = lerp(thing.prev_x, thing.x);
        let sy = lerp(thing.prev_y, thing.y);

        // transform the origin point
        let tr_x = sx - view.x;
        let tr_y = sy - view.y;
        let tz = (tr_x * view_cos) - -(tr_y * view_sin);

        // Is it behind the view?
        if tz < 10 {
            return true; // keep checking
        }

        let mut tx = (tr_x * view_sin) - (tr_y * view_cos);
        // too far off the side?
        // OG: abs(tx) > (tz << 2) — use saturating shift to avoid overflow
        if tx.doom_abs() > FixedT(tz.0.saturating_mul(4)) {
            return true;
        }

        // Find the sprite def to use
        let sprnum = thing.state.sprite;
        let sprite_def = pic_data.sprite_def(sprnum as u32 as usize);
        if sprite_def.frames.is_empty() {
            error!("No frames?, {:?}, {sprite_def:?}", thing.state);
            return true;
        }

        let frame = thing.frame & FF_FRAMEMASK;
        if frame & FF_FRAMEMASK > 28 {
            return true;
        }
        let sprite_frame = sprite_def.frames[frame as usize];
        let patch;
        let patch_index;
        let flip;
        if sprite_frame.rotate == 1 {
            // OG: ang = R_PointToAngle(thing->x, thing->y) - thing->angle;
            //     rot = (ang + (unsigned)(ANG45/2)*9) >> 29;
            let ang = r_point_to_angle(thing.x - view.x, thing.y - view.y)
                .wrapping_sub(thing.angle.inner().0);
            let rot = (ang.wrapping_add((ANG45 / 2).wrapping_mul(9)) >> 29) as usize;
            patch_index = sprite_frame.lump[rot] as u32 as usize;
            patch = pic_data.sprite_patch(patch_index);
            flip = sprite_frame.flip[rot];
        } else {
            patch_index = sprite_frame.lump[0] as u32 as usize;
            patch = pic_data.sprite_patch(patch_index);
            flip = sprite_frame.flip[0];
        }

        if flip > 0 {
            tx -= FixedT::from((patch.data.len() - patch.left_offset as u32 as usize) as i32);
        } else {
            tx -= FixedT::from(patch.left_offset as i32);
        }

        // focal_length for texture stepping
        let x_scale = self.focal_length / tz;
        // projection (half_width) for scale comparison with segs
        let spr_scale = self.projection / tz;

        // Use viewangletox LUT for sprite X positioning — same ceiling rounding
        // as seg column mapping. This ensures sprite columns align exactly with
        // seg clip columns.
        let ang1_bam = r_point_to_angle(tz, -tx);
        let ang1_fine = ((ang1_bam.wrapping_add(math::ANG90)) >> math::ANGLETOFINESHIFT) as usize;
        // LUT ceiling rounds 1 right vs OG sprite truncation. Offset left by 1
        // to match OG sprite placement; clipping still aligns with segs.
        let x1 = FixedT::from(self.viewangletox[ang1_fine & (FINEANGLES / 2 - 1)] - 1);
        if x1 > FixedT::from(screen_width as i32) {
            return true;
        }

        tx += FixedT::from(patch.data.len() as i32);
        let ang2_bam = r_point_to_angle(tz, -tx);
        let ang2_fine = ((ang2_bam.wrapping_add(math::ANG90)) >> math::ANGLETOFINESHIFT) as usize;
        let x2 = FixedT::from(self.viewangletox[ang2_fine & (FINEANGLES / 2 - 1)] - 1);
        if x2 < 0 {
            return true;
        }

        let y_scale = self.y_scale;
        let wide_ratio = self.seg_renderer.wide_ratio;
        let vis = self.new_vissprite();
        vis.mobj_flags = thing.flags.bits();
        vis.scale = spr_scale * y_scale * wide_ratio;
        vis.draw_scale = vis.scale;
        let sz = lerp(thing.prev_z, thing.z);
        vis.gx = sx;
        vis.gy = sy;
        vis.gz = sz;
        vis.gzt = sz + FixedT::from(patch.top_offset as i32);
        vis.texture_mid = vis.gzt - view.viewz;
        vis.x1 = if x1 < 0 { FixedT::ZERO } else { x1 };
        vis.x2 = if x2 >= FixedT::from(screen_width as i32) {
            FixedT::from(screen_width as i32) - 1
        } else {
            x2
        };
        let iscale = 1 / x_scale;
        if flip == 1 {
            vis.start_frac = FixedT::from((patch.data.len() - 1) as i32);
            vis.x_iscale = -iscale;
        } else {
            vis.start_frac = FixedT::ZERO;
            vis.x_iscale = iscale;
        }
        vis.x_iscale /= y_scale; // Note: proportion to x_scale

        // Catches certain orientations
        if vis.x1 > x1 {
            vis.start_frac += vis.x_iscale * (vis.x1 - x1);
        }

        vis.patch = patch_index;
        if thing.frame & FF_FULLBRIGHT != 0 {
            // full bright
            vis.light_level = 15;
        } else {
            vis.light_level = light_level;
        }

        true
    }

    /// Rasterize a single `VisSprite` column-by-column within its clip bounds.
    ///
    /// - Iterates columns from `vis.x1` to `vis.x2`, stepping through the patch
    ///   texture
    /// - Clips each column vertically against `clip_bottom` and `clip_top`
    ///   arrays
    /// - Dispatches to `draw_fuzz_column` for spectre/shadow things, otherwise
    ///   `draw_masked_column`
    fn draw_vissprite(
        &mut self,
        vis: &VisSprite,
        clip_bottom: &[FixedT],
        clip_top: &[FixedT],
        pic_data: &PicData,
        rend: &mut impl DrawBuffer,
    ) {
        let patch = pic_data.sprite_patch(vis.patch);

        let spryscale = vis.draw_scale;
        let dc_iscale = vis.x_iscale.doom_abs();
        let dc_texmid = vis.texture_mid;
        let mut frac = vis.start_frac;
        let is_shadow = vis.mobj_flags & MapObjFlag::Shadow.bits() != 0;
        let colourmap = if is_shadow {
            pic_data.colourmap(33) // unused for fuzz, but needed for signature
        } else {
            pic_data.vert_light_colourmap(vis.light_level, vis.scale.to_f32())
        };

        let xfrac = vis.x_iscale * self.y_scale; // proportional to x1..x2
        for x in vis.x1.to_i32() as usize..=vis.x2.to_i32() as usize {
            let tex_column = frac.to_i32() as usize;
            if tex_column >= patch.data.len() {
                break;
            }

            let texture_column = &patch.data[tex_column];
            let sprtopscreen = self.seg_renderer.centery - dc_texmid * spryscale;
            let bottomscreen = sprtopscreen + spryscale * FixedT::from(texture_column.len() as i32);
            let mut top = FixedT::from(((sprtopscreen.0 + FRACUNIT - 1) >> FRACBITS) as i32);
            let mut bottom = FixedT::from(((bottomscreen.0 - 1) >> FRACBITS) as i32);

            if bottom >= clip_bottom[x] {
                bottom = clip_bottom[x] - 1;
            }

            if top <= clip_top[x] {
                top = clip_top[x] + 1;
            }

            if top <= bottom {
                if is_shadow {
                    draw_fuzz_column(
                        texture_column,
                        dc_iscale,
                        self.seg_renderer.centery,
                        x,
                        dc_texmid,
                        top,
                        bottom,
                        rend,
                        &mut self.fuzz_pos,
                    );
                } else {
                    draw_masked_column(
                        texture_column,
                        colourmap,
                        dc_iscale,
                        self.seg_renderer.centery,
                        x,
                        dc_texmid,
                        top,
                        bottom,
                        pic_data,
                        rend,
                    );
                }
            }

            frac += xfrac;
        }
    }

    /// Build per-column clip arrays from overlapping drawsegs, then draw the
    /// sprite.
    ///
    /// - Iterates drawsegs back-to-front, comparing scale to determine
    ///   sprite-wall ordering
    /// - For segs behind the sprite: renders any masked (transparent) wall
    ///   texture first
    /// - For segs in front: writes floor/ceiling clip values into per-column
    ///   arrays
    /// - Unclipped columns default to full screen height
    /// - Finally delegates to `draw_vissprite` with the computed clip arrays
    fn draw_sprite(
        &mut self,
        view: &RenderView,
        vis: &VisSprite,
        pic_data: &PicData,
        rend: &mut impl DrawBuffer,
    ) {
        let size = rend.size().clone();
        let mut clip_bottom = vec![FixedT::from(-2); size.width_usize()];
        let mut clip_top = vec![FixedT::from(-2); size.width_usize()];

        // Breaking liftime to enable this loop
        let segs = unsafe { &*(&self.r_data.drawsegs as *const Vec<DrawSeg>) };
        for seg in segs.iter().rev() {
            if seg.x1 > vis.x2
                || seg.x2 < vis.x1
                || (seg.silhouette == 0 && seg.maskedtexturecol == FixedT::ZERO)
            {
                continue;
            }

            let r1 = if (seg.x1) < vis.x1 { vis.x1 } else { seg.x1 };
            let r2 = if (seg.x2) > vis.x2 { vis.x2 } else { seg.x2 };

            let (lowscale, scale) = if seg.scale1 > seg.scale2 {
                (seg.scale2, seg.scale1)
            } else {
                (seg.scale1, seg.scale2)
            };

            unsafe {
                if scale <= vis.scale
                    || (lowscale < vis.scale && {
                        let seg = seg.curline.as_ref();
                        let dx = vis.gx - seg.v1.x_fp;
                        let dy = vis.gy - seg.v1.y_fp;
                        let ddx = seg.v2.x_fp - seg.v1.x_fp;
                        let ddy = seg.v2.y_fp - seg.v1.y_fp;
                        (dy * ddx) <= (ddy * dx)
                    })
                {
                    if seg.maskedtexturecol != FixedT::from(-1) {
                        self.render_masked_seg_range(view, seg, r1, r2, pic_data, rend);
                    }
                    // seg is behind sprite
                    continue;
                }
            }

            for r in r1.to_i32() as usize..=r2.to_i32() as usize {
                if clip_bottom[r] == FixedT::from(-2) && seg.sprbottomclip.is_some() {
                    let i = (seg.sprbottomclip.unwrap() + FixedT::from(r as i32)).to_i32() as usize;
                    if i < self.seg_renderer.openings.len() {
                        clip_bottom[r] = self.seg_renderer.openings[i];
                        if clip_bottom[r] < 0 {
                            clip_bottom[r] = FixedT::ZERO;
                        }
                    }
                }
                if clip_top[r] == FixedT::from(-2) && seg.sprtopclip.is_some() {
                    let i = (seg.sprtopclip.unwrap() + FixedT::from(r as i32)).to_i32() as usize;
                    if i < self.seg_renderer.openings.len() {
                        clip_top[r] = self.seg_renderer.openings[i];
                        if clip_top[r] >= FixedT::from(size.view_height()) {
                            clip_top[r] = FixedT::from(size.view_height());
                        }
                    }
                }
            }
        }

        for x in vis.x1.to_i32() as usize..=vis.x2.to_i32() as usize {
            if clip_bottom[x] == FixedT::from(-2) {
                clip_bottom[x] = FixedT::from(size.view_height());
            }
            if clip_top[x] == FixedT::from(-2) {
                clip_top[x] = FixedT::from(-1);
            }
        }

        self.draw_vissprite(vis, &clip_bottom, &clip_top, pic_data, rend);
    }

    fn draw_player_sprites(
        &mut self,
        view: &RenderView,
        pic_data: &PicData,
        rend: &mut impl DrawBuffer,
    ) {
        let light = (view.sector_lightlevel >> 4) + view.extralight;
        for sprite in view.psprites.iter() {
            if sprite.active {
                self.draw_player_sprite(sprite, light, view.is_shadow, pic_data, rend);
            }
        }
    }

    /// Render a player weapon (psprite) overlay.
    ///
    /// - Computes screen-resolution scale from native 200px height
    /// - Centers the weapon horizontally at screen midpoint with offset
    /// - Builds a `VisSprite` with no `wide_ratio` correction (weapon fills
    ///   screen width)
    /// - Uses unclipped full-screen clip bounds (weapon always draws on top)
    fn draw_player_sprite(
        &mut self,
        sprite: &RenderPspDef,
        light: usize,
        is_shadow: bool,
        pic_data: &PicData,
        rend: &mut impl DrawBuffer,
    ) {
        let size = rend.size().clone();
        let f = size.height() / 200;
        let pspriteiscale = FixedT::ONE / FixedT::from(f);
        let pspritescale = FixedT::from(f);

        let def = pic_data.sprite_def(sprite.sprite);
        if def.frames.is_empty() {
            warn!("sprite {} has no frames", sprite.sprite);
            return;
        }
        let frame = def.frames[(sprite.frame & FF_FRAMEMASK) as usize];
        let patch = pic_data.sprite_patch(frame.lump[0] as u32 as usize);
        let flip = frame.flip[0];
        // 160 is pretty much a hardcoded number to center the weapon always
        let mut tx = FixedT::from_f32(sprite.sx) - 160 - FixedT::from(patch.left_offset as i32);
        let x_offset = pspritescale / self.y_scale;
        let x1 = FixedT::from(size.half_width()) + (tx * x_offset);

        if x1 >= FixedT::from(size.width()) {
            return;
        }
        tx += FixedT::from(patch.data.len() as i32);
        let x2 = FixedT::from(size.half_width()) + tx * x_offset;

        if x2 < 0 {
            return;
        }

        let mut vis = VisSprite::new();
        vis.mobj_flags = if is_shadow {
            MapObjFlag::Shadow.bits()
        } else {
            0
        };
        vis.patch = frame.lump[0] as u32 as usize;
        vis.texture_mid = FixedT::from(100)
            - (FixedT::from_f32(sprite.sy) - FixedT::from(patch.top_offset as i32));
        let tmp = self.seg_renderer.centery - FixedT::from(size.view_height() / 2);
        if size.hi_res() {
            vis.texture_mid += tmp / 2;
        } else {
            vis.texture_mid += tmp;
        }
        vis.x1 = if x1 < 0 { FixedT::ZERO } else { x1 };
        vis.x2 = if x2 >= FixedT::from(size.width()) {
            FixedT::from(size.width())
        } else {
            x2
        };
        vis.scale = pspritescale;
        vis.draw_scale = pspritescale; // weapon: no wide_ratio correction
        vis.light_level = (light + 2).min(15);

        if flip != 0 {
            vis.x_iscale = -pspriteiscale;
            vis.start_frac = FixedT::from(patch.data[0].len() as i32);
        } else {
            vis.x_iscale = pspriteiscale;
            vis.start_frac = FixedT::ZERO;
        }

        if vis.x1 > x1 {
            vis.start_frac += vis.x_iscale * (vis.x1 - x1);
        }

        let clip_bottom = vec![FixedT::ZERO; size.width_usize()];
        let clip_top = vec![FixedT::from(size.view_height()); size.width_usize()];
        self.draw_vissprite(&vis, &clip_top, &clip_bottom, pic_data, rend)
    }

    /// Sort vissprites by depth and draw all masked (transparent) geometry.
    ///
    /// - Sorts vissprites back-to-front (painter's order) by scale
    /// - Draws each world sprite with wall-occlusion clipping
    /// - Draws remaining masked wall textures (e.g., midtextures on 2-sided
    ///   lines)
    /// - Draws player weapon sprites last (always on top)
    pub(crate) fn draw_masked(
        &mut self,
        view: &RenderView,
        pic_data: &PicData,
        rend: &mut impl DrawBuffer,
    ) {
        // Sort only the vissprites used
        self.vissprites[..self.next_vissprite].sort();
        // Need to break lifetime as a chain function call needs &mut on a separate item
        let vis = unsafe { &*(&self.vissprites as *const [VisSprite]) };
        for (i, vis) in vis.iter().enumerate() {
            self.draw_sprite(view, vis, pic_data, rend);
            if i == self.next_vissprite {
                break;
            }
        }

        let segs: Vec<DrawSeg> = self.r_data.drawsegs.to_vec();
        for ds in segs.iter().rev() {
            self.render_masked_seg_range(view, ds, ds.x1, ds.x2, pic_data, rend);
        }

        self.draw_player_sprites(view, pic_data, rend);
    }

    /// Render the masked (transparent) midtexture of a two-sided linedef across
    /// columns `x1..=x2`.
    ///
    /// - Computes texture anchor from `UnpegBottom` flag (floor-anchored vs
    ///   ceiling-anchored)
    /// - Steps through columns, looking up texture column indices from the
    ///   openings array
    /// - Clips each column against floor and ceiling clip values from the
    ///   drawseg
    /// - Marks rendered openings as `MAX` to prevent double-drawing
    fn render_masked_seg_range(
        &mut self,
        view: &RenderView,
        ds: &DrawSeg,
        x1: FixedT,
        x2: FixedT,
        pic_data: &PicData,
        rend: &mut impl DrawBuffer,
    ) {
        let size = rend.size().clone();
        let seg = unsafe { ds.curline.as_ref() };
        let frontsector = seg.frontsector.clone();

        if let Some(backsector) = seg.backsector.as_ref() {
            if seg.sidedef.midtexture.is_none() {
                return;
            }
            let texnum = unsafe { seg.sidedef.midtexture.unwrap_unchecked() };

            let wall_lights = ((seg.sidedef.sector.lightlevel >> 4) + view.extralight).min(15);

            let rw_scalestep = ds.scalestep;
            let mut spryscale = ds.scale1 + (x1 - ds.x1) * rw_scalestep;

            let mut dc_texturemid;
            if seg.linedef.flags.contains(LineDefFlags::UnpegBottom) {
                dc_texturemid = if frontsector.floorheight > backsector.floorheight {
                    frontsector.floorheight
                } else {
                    backsector.floorheight
                };

                let texture_column = pic_data.wall_pic_column(texnum, 0);
                dc_texturemid += FixedT::from((texture_column.len() - 1) as i32) - view.viewz;
            } else {
                dc_texturemid = if frontsector.ceilingheight < backsector.ceilingheight {
                    frontsector.ceilingheight
                } else {
                    backsector.ceilingheight
                };
                dc_texturemid -= view.viewz;
            }
            dc_texturemid += seg.sidedef.rowoffset;

            for x in x1.to_i32() as usize..=x2.to_i32() as usize {
                if ds.maskedtexturecol + FixedT::from(x as i32) < 0 {
                    spryscale += rw_scalestep;
                    continue;
                }
                let index = (ds.maskedtexturecol + FixedT::from(x as i32)).to_i32() as usize;

                if index != usize::MAX
                    && index < self.seg_renderer.openings.len()
                    && ds.sprbottomclip.is_some()
                    && ds.sprtopclip.is_some()
                    && self.seg_renderer.openings[index] != FixedT::MAX
                    && seg.sidedef.midtexture.is_some()
                {
                    let texture_column = pic_data.wall_pic_column(
                        unsafe { seg.sidedef.midtexture.unwrap_unchecked() },
                        self.seg_renderer.openings[index].doom_abs().to_i32() as usize,
                    );

                    let i = (ds.sprtopclip.unwrap() + FixedT::from(x as i32)).to_i32() as usize;
                    if i >= self.seg_renderer.openings.len() {
                        continue;
                    }
                    let mut mceilingclip = self.seg_renderer.openings[i];
                    let i = (ds.sprbottomclip.unwrap() + FixedT::from(x as i32)).to_i32() as usize;
                    if i >= self.seg_renderer.openings.len() {
                        continue;
                    }
                    let mut mfloorclip = self.seg_renderer.openings[i];
                    if mceilingclip >= FixedT::from(size.view_height()) {
                        mceilingclip = FixedT::from(size.view_height());
                    }
                    if mfloorclip < 0 {
                        mfloorclip = FixedT::ZERO;
                    }

                    // calculate unclipped screen coordinates for post
                    let sprtopscreen = self.seg_renderer.centery - dc_texturemid * spryscale;
                    let bottomscreen =
                        sprtopscreen + spryscale * FixedT::from(texture_column.len() as i32);
                    let mut top =
                        FixedT::from(((sprtopscreen.0 + FRACUNIT - 1) >> FRACBITS) as i32);
                    let mut bottom = FixedT::from(((bottomscreen.0 - 1) >> FRACBITS) as i32);

                    if bottom >= mfloorclip {
                        bottom = mfloorclip - 1;
                    }
                    if top <= mceilingclip {
                        top = mceilingclip + 1;
                    }

                    draw_masked_column(
                        texture_column,
                        pic_data.vert_light_colourmap(wall_lights, spryscale.to_f32()),
                        1 / spryscale,
                        self.seg_renderer.centery,
                        x,
                        dc_texturemid,
                        top,
                        bottom,
                        pic_data,
                        rend,
                    );

                    self.seg_renderer.openings[index] = FixedT::MAX;
                }
                spryscale += rw_scalestep;
            }
        }
    }
}

/// Draw a single vertical column of a masked (transparent) texture.
///
/// Iterates texels top-to-bottom, skipping transparent pixels (`usize::MAX`),
/// mapping through the colourmap for lighting, and writing to the framebuffer.
#[allow(clippy::too_many_arguments)]
fn draw_masked_column(
    texture_column: &[usize],
    colourmap: &[usize],
    fracstep: FixedT,
    centery: FixedT,
    dc_x: usize,
    dc_texturemid: FixedT,
    yl: FixedT,
    mut yh: FixedT,
    pic_data: &PicData,
    pixels: &mut impl DrawBuffer,
) {
    if yh >= FixedT::from(pixels.size().height()) {
        yh = FixedT::from(pixels.size().height()) - 1;
    }
    let pal = pic_data.palette();
    let mut frac = dc_texturemid + (yl - centery) * fracstep;
    for y in yl.to_i32() as usize..=yh.to_i32() as usize {
        let select = frac.to_i32() as usize;
        if select >= texture_column.len() {
            return;
        }
        if texture_column[select] == usize::MAX {
            frac += fracstep;
            continue;
        }
        let c = pal[colourmap[texture_column[select]]];
        pixels.set_pixel(dc_x, y, c);
        frac += fracstep;
    }
}

/// Draw a single column using the fuzz/spectre effect.
///
/// Instead of texturing, darkens a neighbouring pixel from the framebuffer
/// using a pseudo-random offset table (`FUZZ_TABLE`) to produce the
/// partial-invisibility shimmer.
#[allow(clippy::too_many_arguments)]
fn draw_fuzz_column(
    texture_column: &[usize],
    fracstep: FixedT,
    centery: FixedT,
    dc_x: usize,
    dc_texturemid: FixedT,
    yl: FixedT,
    mut yh: FixedT,
    pixels: &mut impl DrawBuffer,
    fuzz_pos: &mut usize,
) {
    let height = pixels.size().height_usize();
    if yh >= FixedT::from(pixels.size().height()) {
        yh = FixedT::from(pixels.size().height()) - 1;
    }
    let pitch = pixels.pitch();
    let mut frac = dc_texturemid + (yl - centery) * fracstep;
    for y in yl.to_i32() as usize..=yh.to_i32() as usize {
        let select = frac.to_i32() as usize;
        if select >= texture_column.len() {
            return;
        }
        if texture_column[select] == usize::MAX {
            frac += fracstep;
            continue;
        }
        let buf = pixels.buf_mut();
        let offset = FUZZ_TABLE[*fuzz_pos % FUZZ_TABLE.len()];
        let src_y = (y as i32 + offset).clamp(0, height as i32 - 1) as usize;
        buf[y * pitch + dc_x] = fuzz_darken(buf[src_y * pitch + dc_x]);
        *fuzz_pos += 1;
        frac += fracstep;
    }
}
