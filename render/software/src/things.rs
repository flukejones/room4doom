use std::cmp;
use std::f32::consts::{FRAC_PI_2, TAU};

use gameplay::log::{error, warn};
use gameplay::{
    LineDefFlags, MapObjFlag, MapObject, PicData, Player, PspDef, Sector, p_random,
    point_to_angle_2,
};
use glam::Vec2;
use math::FixedPoint;
use render_trait::{PixelBuffer, RenderTrait};

use super::bsp::SoftwareRenderer;
use super::defs::DrawSeg;

const FF_FULLBRIGHT: u32 = 0x8000;
const FF_FRAMEMASK: u32 = 0x7FFF;
/// Offset in radians for player view rotation during frame rotation select
const FRAME_ROT_OFFSET: f32 = FRAC_PI_2 / 4.0;
/// Divisor for selecting which frame rotation to use
const FRAME_ROT_SELECT: f32 = 8.0 / TAU;

#[derive(Clone, Copy, PartialEq, Default)]
pub struct VisSprite {
    x1: FixedPoint,
    x2: FixedPoint,
    // Line side calc
    gx: FixedPoint,
    gy: FixedPoint,
    // Bottom and top for clipping
    gz: FixedPoint,
    gzt: FixedPoint,
    // horizontal position of x1
    start_frac: FixedPoint,
    scale: FixedPoint,
    // negative if flipped
    x_iscale: FixedPoint,
    texture_mid: FixedPoint,
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
            x1: FixedPoint::zero(),
            x2: FixedPoint::zero(),
            gx: FixedPoint::zero(),
            gy: FixedPoint::zero(),
            gz: FixedPoint::zero(),
            gzt: FixedPoint::zero(),
            start_frac: FixedPoint::zero(),
            scale: FixedPoint::zero(),
            x_iscale: FixedPoint::zero(),
            texture_mid: FixedPoint::zero(),
            patch: 0,
            light_level: 0,
            mobj_flags: 0,
        }
    }
}

impl SoftwareRenderer {
    pub(crate) fn add_sprites<'a>(
        &'a mut self,
        player: &Player,
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

        let light_level = (sector.lightlevel >> 4) + player.extralight;
        sector.run_func_on_thinglist(|thing| {
            self.project_sprite(player, thing, light_level, screen_width, pic_data)
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

    // R_ProjectSprite
    // Generates a vissprite for a thing
    //  if it might be visible.
    //
    fn project_sprite(
        &mut self,
        player: &Player,
        thing: &MapObject,
        light_level: usize,
        screen_width: u32,
        pic_data: &PicData,
    ) -> bool {
        if thing.player().is_some() {
            return true;
        }

        let player_mobj = unsafe { player.mobj_unchecked() };
        let view_cos = player_mobj.angle.cos();
        let view_sin = player_mobj.angle.sin();

        // transform the origin point
        let tr_x = FixedPoint::from(thing.xy.x - player_mobj.xy.x);
        let tr_y = FixedPoint::from(thing.xy.y - player_mobj.xy.y);
        let tz = (tr_x * view_cos) - -(tr_y * view_sin);

        // Is it behind the view?
        if tz < 10 {
            return true; // keep checking
        }

        let mut tx = (tr_x * view_sin) - (tr_y * view_cos);
        // too far off the side?
        // if tx.abs() >= tz.abs() * 2 {
        //     return true;
        // }

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
            let angle = point_to_angle_2(player_mobj.xy, thing.xy);
            let rot = ((angle - thing.angle + FRAME_ROT_OFFSET).rad()) * FRAME_ROT_SELECT;
            patch_index = sprite_frame.lump[rot as u32 as usize] as u32 as usize;
            patch = pic_data.sprite_patch(patch_index);
            flip = sprite_frame.flip[rot as u32 as usize];
        } else {
            patch_index = sprite_frame.lump[0] as u32 as usize;
            patch = pic_data.sprite_patch(patch_index);
            flip = sprite_frame.flip[0];
        }

        if flip > 0 {
            tx -= (patch.data.len() - patch.left_offset as u32 as usize) as f32;
        } else {
            tx -= patch.left_offset as f32;
        }

        let centerx = FixedPoint::from(screen_width / 2);
        // Projection does the X scaling in wide
        let x_scale = self.projection / tz;

        let x1 = (centerx + tx * x_scale) - 1;
        if x1 > screen_width {
            return true;
        }

        tx += patch.data.len() as f32;
        let x2 = centerx + tx * x_scale;
        if x2 < 0.0 {
            return true;
        }

        let y_scale = self.y_scale;
        let vis = self.new_vissprite();
        vis.mobj_flags = thing.flags;
        vis.scale = x_scale * y_scale; // Note: increase Y
        vis.gx = thing.xy.x.into();
        vis.gy = thing.xy.y.into();
        vis.gz = thing.z.into();
        vis.gzt = (thing.z + patch.top_offset as f32).into();
        vis.texture_mid = vis.gzt - player.viewz;
        vis.x1 = if x1 < 0.0 {
            FixedPoint::default()
        } else {
            x1.into()
        };
        vis.x2 = if x2 >= screen_width as f32 {
            (screen_width as f32 - 1.0).into()
        } else {
            x2.into()
        };
        let iscale = 1.0 / x_scale;
        if flip == 1 {
            vis.start_frac = (patch.data.len() - 1).into();
            vis.x_iscale = -iscale;
        } else {
            vis.start_frac = FixedPoint::default();
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
            vis.light_level = 255;
        } else {
            vis.light_level = light_level;
        }

        true
    }

    // R_DrawVisSprite
    fn draw_vissprite(
        &self,
        vis: &VisSprite,
        clip_bottom: &[FixedPoint],
        clip_top: &[FixedPoint],
        pic_data: &PicData,
        rend: &mut impl RenderTrait,
    ) {
        let patch = pic_data.sprite_patch(vis.patch);

        let spryscale = vis.scale;
        let dc_iscale = vis.x_iscale;
        let dc_texmid = vis.texture_mid;
        let mut frac = vis.start_frac;
        let colourmap = if vis.mobj_flags & MapObjFlag::Shadow as u32 != 0 {
            pic_data.colourmap(33)
        } else {
            pic_data.vert_light_colourmap(vis.light_level, vis.scale.into())
        };

        // Proportional to x1..x2
        let xfrac = vis.x_iscale * self.y_scale;

        for x in i32::from(vis.x1) as usize..=i32::from(vis.x2) as usize {
            let tex_column = i32::from(frac) as usize;
            if tex_column >= patch.data.len() {
                break;
            }

            let texture_column = &patch.data[tex_column];
            let mut top = (self.seg_renderer.centery - dc_texmid * spryscale) + 1.0;

            let mut bottom = top + (spryscale * FixedPoint::from(texture_column.len()));

            if bottom >= clip_bottom[x] {
                bottom = clip_bottom[x] - 1;
            }

            if top <= clip_top[x] {
                top = clip_top[x] + 1;
            }

            if top <= bottom {
                draw_masked_column(
                    texture_column,
                    colourmap,
                    vis.mobj_flags & MapObjFlag::Shadow as u32 != 0,
                    dc_iscale.into(),
                    self.seg_renderer.centery.into(),
                    x,
                    dc_texmid.into(),
                    top.into(),
                    bottom.into(),
                    pic_data,
                    rend.draw_buffer(),
                );
            }

            frac = frac + xfrac;
        }
    }

    /// Doom function name `R_DrawSprite`
    fn draw_sprite(
        &mut self,
        player: &Player,
        vis: &VisSprite,
        pic_data: &PicData,
        rend: &mut impl RenderTrait,
    ) {
        let size = rend.draw_buffer().size().clone();
        let mut clip_bottom = vec![FixedPoint::from(-2); size.width_usize()];
        let mut clip_top = vec![FixedPoint::from(-2); size.width_usize()];

        // Breaking lifetime to enable this loop
        let segs = unsafe { &*(&self.r_data.drawsegs as *const Vec<DrawSeg>) };
        for seg in segs.iter().rev() {
            if seg.x1 > vis.x2
                || seg.x2 < vis.x1
                || (seg.silhouette == 0 && seg.maskedtexturecol == FixedPoint::zero())
            {
                continue;
            }

            let r1 = if seg.x1 < vis.x1 { vis.x1 } else { seg.x1 };
            let r2 = if seg.x2 > vis.x2 { vis.x2 } else { seg.x2 };

            let (lowscale, scale) = if seg.scale1 > seg.scale2 {
                (seg.scale2, seg.scale1)
            } else {
                (seg.scale1, seg.scale2)
            };

            unsafe {
                // Check if sprite is behind this draw segment
                if scale <= vis.scale
                    || (lowscale < vis.scale
                        && seg
                            .curline
                            .as_ref()
                            .point_on_side(Vec2::new(vis.gx.into(), vis.gy.into()))
                            == 0)
                {
                    if seg.maskedtexturecol != FixedPoint::from(-1.0) {
                        self.render_masked_seg_range(player, seg, r1, r2, pic_data, rend);
                    }
                    // seg is behind sprite
                    continue;
                }
            }

            // Process sprite clipping
            for r in i32::from(r1) as usize..=i32::from(r2) as usize {
                if clip_bottom[r] == FixedPoint::from(-2.0) && seg.sprbottomclip.is_some() {
                    let i = u32::from(seg.sprbottomclip.unwrap() + r as i32) as usize;
                    if i < self.seg_renderer.openings.len() {
                        clip_bottom[r] = self.seg_renderer.openings[i];
                        if clip_bottom[r] < FixedPoint::zero() {
                            clip_bottom[r] = FixedPoint::zero();
                        }
                    }
                }
                if clip_top[r] == FixedPoint::from(-2.0) && seg.sprtopclip.is_some() {
                    let i = u32::from(seg.sprtopclip.unwrap() + r as i32) as usize;
                    if i < self.seg_renderer.openings.len() {
                        clip_top[r] = self.seg_renderer.openings[i];
                        if clip_top[r] >= FixedPoint::from(size.height()) {
                            clip_top[r] = FixedPoint::from(size.height());
                        }
                    }
                }
            }
        }

        // Set default clipping values if not set
        for x in i32::from(vis.x1) as usize..i32::from(vis.x2) as usize {
            if clip_bottom[x] == FixedPoint::from(-2) {
                clip_bottom[x] = FixedPoint::from(size.height());
            }
            if clip_top[x] == FixedPoint::from(-2) {
                clip_top[x] = FixedPoint::from(-1);
            }
        }

        self.draw_vissprite(vis, &clip_bottom, &clip_top, pic_data, rend);
    }

    fn draw_player_sprites(
        &mut self,
        player: &Player,
        pic_data: &PicData,
        rend: &mut impl RenderTrait,
    ) {
        if let Some(mobj) = player.mobj() {
            let light = mobj.subsector.sector.lightlevel;
            let light = (light >> 4) + player.extralight;

            for sprite in player.psprites.iter() {
                if sprite.state.is_some() {
                    self.draw_player_sprite(sprite, light, mobj.flags, pic_data, rend);
                }
            }
        }
    }

    /// R_DrawPSprite
    fn draw_player_sprite(
        &mut self,
        sprite: &PspDef,
        light: usize,
        flags: u32,
        pic_data: &PicData,
        rend: &mut impl RenderTrait,
    ) {
        let size = rend.draw_buffer().size().clone();
        let f = size.height() / 200;
        let pspriteiscale = FixedPoint::from(0.99 / f as f32);
        let pspritescale = FixedPoint::from(f);

        let def = pic_data.sprite_def(sprite.state.unwrap().sprite as u32 as usize);
        if def.frames.is_empty() {
            warn!("{:?} has no frames", sprite.state.unwrap().sprite);
            return;
        }

        let frame = def.frames[(sprite.state.unwrap().frame & FF_FRAMEMASK) as usize];
        let patch = pic_data.sprite_patch(frame.lump[0] as u32 as usize);
        let flip = frame.flip[0];

        // Calculate position - these are all in fixed point now
        let mut tx = FixedPoint::from(sprite.sx - 160.0 - patch.left_offset as f32);
        let x_offset = pspritescale / self.y_scale;
        let x1 = FixedPoint::from(size.half_width()) + (tx * x_offset);

        if x1 >= FixedPoint::from(size.width()) {
            return;
        }

        tx = tx + FixedPoint::from(patch.data.len() as f32);
        let x2 = FixedPoint::from(size.half_width()) + tx * x_offset;

        if x2 < FixedPoint::zero() {
            return;
        }

        let mut vis = VisSprite::new();
        vis.mobj_flags = flags;
        vis.patch = frame.lump[0] as u32 as usize;

        // Calculate texture mid - this is the vertical position reference
        vis.texture_mid = FixedPoint::from(100.0 - (sprite.sy - patch.top_offset as f32));
        let tmp = self.seg_renderer.centery - size.half_height();

        if size.hi_res() {
            vis.texture_mid = vis.texture_mid + tmp / 2.0;
        } else {
            vis.texture_mid = vis.texture_mid + tmp;
        }

        // Set screen coordinates with proper clipping
        vis.x1 = if x1 < FixedPoint::zero() {
            FixedPoint::zero()
        } else {
            x1
        };
        vis.x2 = if x2 >= FixedPoint::from(size.width()) {
            FixedPoint::from(size.width())
        } else {
            x2
        };

        vis.scale = pspritescale;
        vis.light_level = light + 2;

        // Handle flipping
        if flip != 0 {
            vis.x_iscale = -pspriteiscale;
            vis.start_frac = FixedPoint::from(patch.data[0].len() as f32);
        } else {
            vis.x_iscale = pspriteiscale;
            vis.start_frac = FixedPoint::zero();
        }

        // Adjust starting position if clipped
        if vis.x1 > x1 {
            vis.start_frac = vis.start_frac + vis.x_iscale * (vis.x1 - x1);
        }

        // Create clipping arrays
        let clip_bottom = vec![FixedPoint::zero(); size.width_usize()];
        let clip_top = vec![FixedPoint::from(size.height()); size.width_usize()];

        // Draw the sprite
        self.draw_vissprite(&vis, &clip_top, &clip_bottom, pic_data, rend)
    }

    pub(crate) fn draw_masked(
        &mut self,
        player: &Player,
        pic_data: &PicData,
        rend: &mut impl RenderTrait,
    ) {
        // Sort only the vissprites used
        self.vissprites[..self.next_vissprite].sort();
        // Need to break lifetime as a chain function call needs &mut on a separate item
        let vis = unsafe { &*(&self.vissprites as *const [VisSprite]) };
        for (i, vis) in vis.iter().enumerate() {
            self.draw_sprite(player, vis, pic_data, rend);
            if i == self.next_vissprite {
                break;
            }
        }

        let segs: Vec<DrawSeg> = self.r_data.drawsegs.to_vec();
        for ds in segs.iter().rev() {
            self.render_masked_seg_range(player, ds, ds.x1, ds.x2, pic_data, rend);
        }

        self.draw_player_sprites(player, pic_data, rend);
    }

    fn render_masked_seg_range(
        &mut self,
        player: &Player,
        ds: &DrawSeg,
        x1: FixedPoint,
        x2: FixedPoint,
        pic_data: &PicData,
        rend: &mut impl RenderTrait,
    ) {
        let size = rend.draw_buffer().size().clone();
        let seg = unsafe { ds.curline.as_ref() };
        let frontsector = seg.frontsector.clone();

        if let Some(backsector) = seg.backsector.as_ref() {
            if seg.sidedef.midtexture.is_none() {
                return;
            }
            let texnum = unsafe { seg.sidedef.midtexture.unwrap_unchecked() };

            let wall_lights = (seg.sidedef.sector.lightlevel >> 4) + player.extralight;

            let rw_scalestep = ds.scalestep;
            // Calculate scale for first pixel and adjust based on position
            let mut spryscale = ds.scale1 + (x1 - ds.x1) * rw_scalestep;

            let mut dc_texturemid;
            if seg.linedef.flags & LineDefFlags::UnpegBottom as u32 != 0 {
                // Bottom pegged
                dc_texturemid = if frontsector.floorheight > backsector.floorheight {
                    FixedPoint::from(frontsector.floorheight)
                } else {
                    FixedPoint::from(backsector.floorheight)
                };

                let texture_column = pic_data.wall_pic_column(texnum, 0);
                dc_texturemid = dc_texturemid + FixedPoint::from(texture_column.len() - 1)
                    - FixedPoint::from(player.viewz);
            } else {
                // Top pegged
                dc_texturemid = if frontsector.ceilingheight < backsector.ceilingheight {
                    FixedPoint::from(frontsector.ceilingheight)
                } else {
                    FixedPoint::from(backsector.ceilingheight)
                };
                dc_texturemid = dc_texturemid - FixedPoint::from(player.viewz);
            }
            dc_texturemid = dc_texturemid + FixedPoint::from(seg.sidedef.rowoffset);

            // Process masked column for each pixel in range
            for x in i32::from(x1)..=i32::from(x2) {
                if ds.maskedtexturecol + x < FixedPoint::zero() {
                    spryscale = spryscale + rw_scalestep;
                    continue;
                }
                let index = u32::from(ds.maskedtexturecol + x) as usize;

                if index != usize::MAX
                    && index < self.seg_renderer.openings.len()
                    && ds.sprbottomclip.is_some()
                    && ds.sprtopclip.is_some()
                    && self.seg_renderer.openings[index] != FixedPoint::max()
                    && seg.sidedef.midtexture.is_some()
                {
                    let texture_column = pic_data.wall_pic_column(
                        unsafe { seg.sidedef.midtexture.unwrap_unchecked() },
                        usize::from(self.seg_renderer.openings[index]),
                    );

                    // Get clipping values from openings
                    let i = usize::from(ds.sprtopclip.unwrap() + x);
                    if i >= self.seg_renderer.openings.len() {
                        continue;
                    }
                    let mut mceilingclip = self.seg_renderer.openings[i];

                    let i = usize::from(ds.sprbottomclip.unwrap() + x);
                    if i >= self.seg_renderer.openings.len() {
                        continue;
                    }
                    let mut mfloorclip = self.seg_renderer.openings[i];

                    // Apply bounds checking
                    if mceilingclip >= FixedPoint::from(size.height()) {
                        mceilingclip = FixedPoint::from(size.height());
                    }
                    if mfloorclip < FixedPoint::zero() {
                        mfloorclip = FixedPoint::zero();
                    }

                    // Calculate unclipped screen coordinates for post
                    let sprtopscreen = self.seg_renderer.centery - dc_texturemid * spryscale;
                    let mut top = sprtopscreen;
                    let mut bottom = top + 1 + (spryscale * FixedPoint::from(texture_column.len()));

                    // Apply clipping
                    if bottom >= mfloorclip {
                        bottom = mfloorclip - 1;
                    }
                    if top <= mceilingclip {
                        top = mceilingclip + 1;
                    }

                    // Draw the column if visible
                    draw_masked_column(
                        texture_column,
                        pic_data.vert_light_colourmap(wall_lights, f32::from(spryscale)),
                        false,
                        (1.0 / spryscale).into(),
                        self.seg_renderer.centery.into(),
                        x as usize,
                        dc_texturemid.into(),
                        top.into(),
                        bottom.into(),
                        pic_data,
                        rend.draw_buffer(),
                    );

                    // Mark this column as processed
                    self.seg_renderer.openings[index] = FixedPoint::max();
                }
                spryscale = spryscale + rw_scalestep;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_masked_column(
    texture_column: &[usize],
    colourmap: &[usize],
    fuzz: bool,
    fracstep: f32,
    centery: f32,
    dc_x: usize,
    dc_texturemid: f32,
    yl: f32,
    mut yh: f32,
    pic_data: &PicData,
    pixels: &mut impl PixelBuffer,
) {
    if yh >= pixels.size().height_f32() {
        yh = pixels.size().height_f32() - 1.0;
    }
    let pal = pic_data.palette();
    let mut frac = dc_texturemid + (yl - centery) * fracstep;
    for y in yl as u32 as usize..=yh as u32 as usize {
        let select = frac as u32 as usize;
        if select >= texture_column.len() {
            return;
        }
        // Transparency
        if texture_column[select] == usize::MAX || (fuzz && p_random() % 3 != 0) {
            frac += fracstep;
            continue;
        }
        let c = pal[colourmap[texture_column[select]]];
        pixels.set_pixel(dc_x, y, &c);
        frac += fracstep;
    }
}
