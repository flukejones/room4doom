use std::cmp;
use std::f32::consts::{FRAC_PI_2, TAU};

use gameplay::log::{error, warn};
use gameplay::{
    p_random, point_to_angle_2, LineDefFlags, MapObjFlag, MapObject, PicData, Player, PspDef, Sector
};
use glam::Vec2;
use render_target::PixelBuffer;

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
    x1: f32,
    x2: f32,
    // Line side calc
    gx: f32,
    gy: f32,
    // Bottom and top for clipping
    gz: f32,
    gzt: f32,
    // horizontal position of x1
    start_frac: f32,
    scale: f32,
    // negative if flipped
    x_iscale: f32,
    texture_mid: f32,
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
            x1: 0.0,
            x2: 0.0,
            gx: 0.0,
            gy: 0.0,
            gz: 0.0,
            gzt: 0.0,
            start_frac: 0.0,
            scale: 0.0,
            x_iscale: 0.0,
            texture_mid: 0.0,
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
        self.checked_sectors.push(sector.num);

        let light_level = (sector.lightlevel >> 4) + player.extralight;
        sector.run_func_on_thinglist(|thing| {
            self.project_sprite(player, thing, light_level, screen_width, pic_data)
        });
    }

    fn new_vissprite(&mut self) -> &mut VisSprite {
        let curr = self.next_vissprite;
        self.next_vissprite += 1;
        if curr >= self.vissprites.len() - 1 {
            // panic!("Exhausted vissprite allocation");
            self.next_vissprite -= 1;
        }
        &mut self.vissprites[curr]
    }

    // R_ProjectSprite
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
        let tr_x = thing.xy.x - player_mobj.xy.x;
        let tr_y = thing.xy.y - player_mobj.xy.y;
        let tz = (tr_x * view_cos) + (tr_y * view_sin);

        // Is it behind the view?
        if tz < 4.0 {
            return true; // keep checking
        }

        let mut tx = (tr_x * view_sin) - (tr_y * view_cos);
        // too far off the side?
        if tx.abs() as i32 > (tz.abs() as i32) << 2 {
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

        let centerx = screen_width as f32 / 2.0;
        // Projection does the X scaling in wide
        let x_scale = self.projection / tz;

        let x1 = (centerx + tx * x_scale) - 1.0;
        if x1 > screen_width as f32 {
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
        vis.gx = thing.xy.x;
        vis.gy = thing.xy.y;
        vis.gz = thing.z;
        vis.gzt = thing.z + patch.top_offset as f32;
        vis.texture_mid = vis.gzt - player.viewz;
        vis.x1 = if x1 < 0.0 { 0.0 } else { x1 };
        vis.x2 = if x2 >= screen_width as f32 {
            screen_width as f32 - 1.0
        } else {
            x2
        };
        let iscale = 1.0 / x_scale;
        if flip == 1 {
            vis.start_frac = (patch.data.len() - 1) as f32;
            vis.x_iscale = -iscale;
        } else {
            vis.start_frac = 0.0;
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
        clip_bottom: &[f32],
        clip_top: &[f32],
        pic_data: &PicData,
        pixels: &mut dyn PixelBuffer,
    ) {
        let patch = pic_data.sprite_patch(vis.patch);

        let spryscale = vis.scale;
        let dc_iscale = vis.x_iscale.abs();
        let dc_texmid = vis.texture_mid;
        let mut frac = vis.start_frac;
        let colourmap = if vis.mobj_flags & MapObjFlag::Shadow as u32 != 0 {
            pic_data.colourmap(33)
        } else {
            pic_data.vert_light_colourmap(vis.light_level, vis.scale)
        };

        let xfrac = vis.x_iscale * self.y_scale; // proportional to x1..x2
        for x in vis.x1.ceil() as u32 as usize..=vis.x2.floor() as u32 as usize {
            let tex_column = frac as u32 as usize;
            if tex_column >= patch.data.len() {
                break;
            }

            let texture_column = &patch.data[tex_column];
            let mut top = ((self.seg_renderer.centery - dc_texmid * spryscale) + 1.0).round();
            let mut bottom = top + (spryscale * texture_column.len() as f32).round();

            if bottom >= clip_bottom[x] {
                bottom = clip_bottom[x] - 1.0;
            }

            if top <= clip_top[x] {
                top = clip_top[x] + 1.0;
            }

            if top <= bottom {
                draw_masked_column(
                    texture_column,
                    colourmap,
                    vis.mobj_flags & MapObjFlag::Shadow as u32 != 0,
                    dc_iscale,
                    self.seg_renderer.centery,
                    x,
                    dc_texmid,
                    top,
                    bottom,
                    pic_data,
                    pixels,
                );
            }

            frac += xfrac;
        }
    }

    /// Doom function name `R_DrawSprite`
    fn draw_sprite(
        &mut self,
        player: &Player,
        vis: &VisSprite,
        pic_data: &PicData,
        pixels: &mut dyn PixelBuffer,
    ) {
        let mut clip_bottom = vec![-2.0; pixels.size().width_usize()];
        let mut clip_top = vec![-2.0; pixels.size().width_usize()];

        // Breaking liftime to enable this loop
        let segs = unsafe { &*(&self.r_data.drawsegs as *const Vec<DrawSeg>) };
        for seg in segs.iter().rev() {
            if seg.x1 > vis.x2
                || seg.x2 < vis.x1
                || (seg.silhouette == 0 && seg.maskedtexturecol == 0.0)
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
                    || (lowscale < vis.scale
                        && seg
                            .curline
                            .as_ref()
                            .point_on_side(&Vec2::new(vis.gx, vis.gy))
                            == 0)
                {
                    if seg.maskedtexturecol != -1.0 {
                        self.render_masked_seg_range(player, seg, r1, r2, pic_data, pixels);
                    }
                    // seg is behind sprite
                    continue;
                }
            }

            for r in r1 as u32 as usize..=r2 as u32 as usize {
                if clip_bottom[r] == -2.0 && seg.sprbottomclip.is_some() {
                    let i = (seg.sprbottomclip.unwrap() + r as f32) as u32 as usize;
                    if i < self.seg_renderer.openings.len() {
                        clip_bottom[r] = self.seg_renderer.openings[i];
                        if clip_bottom[r] < 0.0 {
                            clip_bottom[r] = 0.0;
                        }
                    }
                }
                if clip_top[r] == -2.0 && seg.sprtopclip.is_some() {
                    let i = (seg.sprtopclip.unwrap() + r as f32) as u32 as usize;
                    if i < self.seg_renderer.openings.len() {
                        clip_top[r] = self.seg_renderer.openings[i];
                        if clip_top[r] >= pixels.size().height_f32() {
                            clip_top[r] = pixels.size().height_f32();
                        }
                    }
                }
            }
        }

        for x in vis.x1 as u32 as usize..=vis.x2 as u32 as usize {
            if clip_bottom[x] == -2.0 {
                clip_bottom[x] = pixels.size().height_f32();
            }
            if clip_top[x] == -2.0 {
                clip_top[x] = -1.0;
            }
        }

        self.draw_vissprite(vis, &clip_bottom, &clip_top, pic_data, pixels);
    }

    fn draw_player_sprites(
        &mut self,
        player: &Player,
        pic_data: &PicData,
        pixels: &mut dyn PixelBuffer,
    ) {
        if let Some(mobj) = player.mobj() {
            let light = mobj.subsector.sector.lightlevel;
            let light = (light >> 4) + player.extralight;

            for sprite in player.psprites.iter() {
                if sprite.state.is_some() {
                    self.draw_player_sprite(sprite, light, mobj.flags, pic_data, pixels);
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
        pixels: &mut dyn PixelBuffer,
    ) {
        let f = pixels.size().height() / 200;
        let pspriteiscale = 0.99 / f as f32;
        let pspritescale = f as f32;

        let def = pic_data.sprite_def(sprite.state.unwrap().sprite as u32 as usize);
        if def.frames.is_empty() {
            warn!("{:?} has no frames", sprite.state.unwrap().sprite);
            return;
        }
        // TODO: WARN: SHT2 has no frames
        // thread 'main' panicked at 'index out of bounds: the len is 0 but the index is
        // 0', render/software/src/things.rs:423:21
        let frame = def.frames[(sprite.state.unwrap().frame & FF_FRAMEMASK) as usize];
        let patch = pic_data.sprite_patch(frame.lump[0] as u32 as usize);
        let flip = frame.flip[0];
        // 160.0 is pretty much a hardcoded number to center the weapon always
        let mut tx = sprite.sx - 160.0 - patch.left_offset as f32;
        let x_offset = pspritescale / self.y_scale;
        let x1 = pixels.size().half_width_f32() + (tx * x_offset);

        if x1 >= pixels.size().width_f32() {
            return;
        }
        tx += patch.data.len() as f32;
        let x2 = pixels.size().half_width_f32() + tx * x_offset;

        if x2 < 0.0 {
            return;
        }

        let mut vis = VisSprite::new();
        vis.mobj_flags = flags;
        vis.patch = frame.lump[0] as u32 as usize;
        // -(sprite.sy.floor() - patch.top_offset as f32);
        vis.texture_mid = 100.0 - (sprite.sy - patch.top_offset as f32);
        let tmp = self.seg_renderer.centery - pixels.size().half_height_f32();
        if pixels.size().hi_res() {
            vis.texture_mid += tmp / 2.0;
        } else {
            vis.texture_mid += tmp;
        }
        vis.x1 = if x1 < 0.0 { 0.0 } else { x1 };
        vis.x2 = if x2 >= pixels.size().width_f32() {
            pixels.size().width_f32()
        } else {
            x2
        };
        vis.scale = pspritescale;
        vis.light_level = light + 2;

        if flip != 0 {
            vis.x_iscale = -pspriteiscale;
            vis.start_frac = patch.data[0].len() as f32;
        } else {
            vis.x_iscale = pspriteiscale;
            vis.start_frac = 0.0;
        }

        if vis.x1 > x1 {
            vis.start_frac += vis.x_iscale * (vis.x1 - x1);
        }

        let clip_bottom = vec![0.0; pixels.size().width_usize()];
        let clip_top = vec![pixels.size().height_f32(); pixels.size().width_usize()];
        self.draw_vissprite(&vis, &clip_top, &clip_bottom, pic_data, pixels)
    }

    pub(crate) fn draw_masked(
        &mut self,
        player: &Player,
        pic_data: &PicData,
        pixels: &mut dyn PixelBuffer,
    ) {
        // Sort only the vissprites used
        self.vissprites[..self.next_vissprite].sort();
        // Need to break lifetime as a chain function call needs &mut on a separate item
        let vis = unsafe { &*(&self.vissprites as *const [VisSprite]) };
        for (i, vis) in vis.iter().enumerate() {
            self.draw_sprite(player, vis, pic_data, pixels);
            if i == self.next_vissprite {
                break;
            }
        }

        let segs: Vec<DrawSeg> = self.r_data.drawsegs.to_vec();
        for ds in segs.iter().rev() {
            self.render_masked_seg_range(player, ds, ds.x1, ds.x2, pic_data, pixels);
        }

        self.draw_player_sprites(player, pic_data, pixels);
    }

    fn render_masked_seg_range(
        &mut self,
        player: &Player,
        ds: &DrawSeg,
        x1: f32,
        x2: f32,
        pic_data: &PicData,
        pixels: &mut dyn PixelBuffer,
    ) {
        let seg = unsafe { ds.curline.as_ref() };
        let frontsector = seg.frontsector.clone();

        if let Some(backsector) = seg.backsector.as_ref() {
            if seg.sidedef.midtexture.is_none() {
                return;
            }
            let texnum = unsafe { seg.sidedef.midtexture.unwrap_unchecked() };

            let wall_lights = (seg.sidedef.sector.lightlevel >> 4) + player.extralight;

            let rw_scalestep = ds.scalestep;
            // TODO: hmmmm 0.05
            let mut spryscale = ds.scale1 + (x1 - ds.x1) * rw_scalestep;

            let mut dc_texturemid;
            if seg.linedef.flags & LineDefFlags::UnpegBottom as u32 != 0 {
                dc_texturemid = if frontsector.floorheight > backsector.floorheight {
                    frontsector.floorheight
                } else {
                    backsector.floorheight
                };

                let texture_column = pic_data.wall_pic_column(texnum, 0);
                dc_texturemid += (texture_column.len() - 1) as f32 - player.viewz;
            } else {
                dc_texturemid = if frontsector.ceilingheight < backsector.ceilingheight {
                    frontsector.ceilingheight
                } else {
                    backsector.ceilingheight
                };
                dc_texturemid -= player.viewz;
            }
            dc_texturemid += seg.sidedef.rowoffset;

            for x in x1.floor() as u32 as usize..=x2.floor() as u32 as usize {
                if ds.maskedtexturecol + (x as f32) < 0.0 {
                    spryscale += rw_scalestep;
                    continue;
                }
                let index = (ds.maskedtexturecol + x as f32) as u32 as usize;

                if index != usize::MAX
                    && index < self.seg_renderer.openings.len()
                    && ds.sprbottomclip.is_some()
                    && ds.sprtopclip.is_some()
                    && self.seg_renderer.openings[index] != f32::MAX
                    && seg.sidedef.midtexture.is_some()
                {
                    let texture_column = pic_data.wall_pic_column(
                        unsafe { seg.sidedef.midtexture.unwrap_unchecked() },
                        self.seg_renderer.openings[index].abs() as u32 as usize,
                    );

                    let i = (ds.sprtopclip.unwrap() + x as f32) as u32 as usize;
                    if i >= self.seg_renderer.openings.len() {
                        continue;
                    }
                    let mut mceilingclip = self.seg_renderer.openings[i];
                    let i = (ds.sprbottomclip.unwrap() + x as f32) as u32 as usize;
                    if i >= self.seg_renderer.openings.len() {
                        continue;
                    }
                    let mut mfloorclip = self.seg_renderer.openings[i];
                    if mceilingclip >= pixels.size().height_f32() {
                        mceilingclip = pixels.size().height_f32();
                    }
                    if mfloorclip < 0.0 {
                        mfloorclip = 0.0;
                    }

                    // calculate unclipped screen coordinates for post
                    let sprtopscreen = self.seg_renderer.centery - dc_texturemid * spryscale;
                    let mut top = sprtopscreen.round(); // TODO: possible glitch
                    let mut bottom = top + 1.0 + (spryscale * texture_column.len() as f32).round();

                    if bottom >= mfloorclip {
                        bottom = mfloorclip - 1.0;
                    }
                    if top <= mceilingclip {
                        top = mceilingclip + 1.0;
                    }

                    draw_masked_column(
                        texture_column,
                        pic_data.vert_light_colourmap(wall_lights, spryscale),
                        false,
                        1.0 / spryscale,
                        self.seg_renderer.centery,
                        x,
                        dc_texturemid,
                        top,
                        bottom,
                        pic_data,
                        pixels,
                    );

                    self.seg_renderer.openings[index] = f32::MAX;
                }
                spryscale += rw_scalestep;
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
    pixels: &mut dyn PixelBuffer,
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
