use std::{
    cmp,
    f32::consts::{FRAC_PI_2, PI},
};

use gameplay::{Angle, LineDefFlags, MapObject, PicData, Player, Sector};
use glam::Vec2;
use sdl2::{rect::Rect, render::Canvas, surface::Surface};

use super::{
    bsp::SoftwareRenderer,
    defs::{DrawSeg, SCREENHEIGHT, SCREENHEIGHT_HALF, SCREENWIDTH, SIL_BOTTOM, SIL_TOP},
};

const FF_FULLBRIGHT: u32 = 0x8000;
const FF_FRAMEMASK: u32 = 0x7fff;
/// Offset in radians for player view rotation during frame rotation select
const FRAME_ROT_OFFSET: f32 = FRAC_PI_2 / 4.0;
/// Divisor for selecting which frame rotation to use
const FRAME_ROT_SELECT: f32 = 8.0 / (PI * 2.0);

pub fn point_to_angle_2(point1: Vec2, point2: Vec2) -> Angle {
    let x = point1.x() - point2.x();
    let y = point1.y() - point2.y();
    Angle::new(y.atan2(x))
}

#[derive(Clone, Copy, PartialEq)]
pub struct VisSprite {
    x1: i32,
    x2: i32,
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
            x1: 0,
            x2: 0,
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

    pub fn clear(&mut self) {
        self.x1 = 0;
        self.x2 = 0;
        self.gx = 0.0;
        self.gy = 0.0;
        self.gz = 0.0;
        self.gzt = 0.0;
        self.start_frac = 0.0;
        self.scale = 0.0;
        self.x_iscale = 0.0;
        self.texture_mid = 0.0;
        self.patch = 0;
        self.light_level = 0;
        self.mobj_flags = 0;
    }
}

impl SoftwareRenderer {
    pub(crate) fn add_sprites<'a>(&'a mut self, player: &Player, sector: &'a Sector) {
        // Need to track sectors as we recurse through BSP as the BSP
        // iteration is via subsectors, and sectors can be split in to
        // many subsectors
        if self.checked_sectors.contains(&sector.num) {
            return;
        }
        self.checked_sectors.push(sector.num);

        let light_level = sector.lightlevel; // TODO: extralight

        // TODO: sprite lights
        // let sprite_light;
        // if light_level < 0 {

        // }

        sector.run_rfunc_on_thinglist(|thing| self.project_sprite(player, thing, light_level));
    }

    fn new_vissprite(&mut self) -> &mut VisSprite {
        let curr = self.next_vissprite;
        self.next_vissprite += 1;
        if curr == self.vissprites.len() {
            panic!("Exhausted vissprite allocation");
        }
        &mut self.vissprites[curr]
    }

    fn project_sprite(&mut self, player: &Player, thing: &MapObject, light_level: i32) -> bool {
        if thing.player.is_some() {
            return true;
        }

        let player_mobj = unsafe { &*player.mobj.unwrap() };
        let view_cos = player_mobj.angle.cos();
        let view_sin = player_mobj.angle.sin();

        // transform the origin point
        let tr_x = thing.xy.x() - player_mobj.xy.x();
        let tr_y = thing.xy.y() - player_mobj.xy.y();
        let gxt = tr_x * view_cos;
        let gyt = -(tr_y * view_sin);
        let tz = gxt - gyt;

        // Is it behind the view?
        if tz < 4.0 {
            return true; // keep checking
        }

        let x_scale = (SCREENWIDTH / 2) as f32 / tz;

        let gxt = -(tr_x * view_sin);
        let gyt = tr_y * view_cos;
        let mut tx = -(gyt + gxt);

        // too far off the side?
        if tx.abs() as i32 > (tz.abs() as i32) << 2 {
            return true;
        }

        // Find the sprite def to use
        let naff = self.texture_data.clone(); // Need to separate lifetimes
        let texture_data = naff.borrow();
        let sprnum = thing.state.sprite;
        let sprite_def = texture_data.sprite_def(sprnum as usize);

        let frame = thing.frame & FF_FRAMEMASK;
        if frame & FF_FRAMEMASK > 28 {
            return true;
        }
        let sprite_frame = sprite_def.frames[(frame) as usize];
        let patch;
        let patch_index;
        let flip;
        if sprite_frame.rotate == 1 {
            let angle = point_to_angle_2(player_mobj.xy, thing.xy);
            let rot = ((angle - thing.angle + FRAME_ROT_OFFSET).rad()) * FRAME_ROT_SELECT;
            let rot = rot.floor();
            patch_index = sprite_frame.lump[rot as usize] as usize;
            patch = texture_data.sprite_patch(patch_index);
            flip = sprite_frame.flip[rot as usize];
        } else {
            patch_index = sprite_frame.lump[0] as usize;
            patch = texture_data.sprite_patch(patch_index);
            flip = sprite_frame.flip[0];
        }

        tx -= patch.left_offset as f32;
        let x1 = ((SCREENWIDTH as f32 / 2.0) + tx * x_scale) as i32;
        if x1 > SCREENWIDTH as i32 {
            return true;
        }

        tx += patch.data.len() as f32;
        let x2 = (((SCREENWIDTH as f32 / 2.0) + tx * x_scale) - 1.0) as i32;
        if x2 < 0 {
            return true;
        }

        let vis = self.new_vissprite();
        vis.mobj_flags = thing.flags;
        vis.scale = x_scale;
        vis.gx = thing.xy.x();
        vis.gy = thing.xy.y();
        vis.gz = thing.z;
        vis.gzt = thing.z + patch.top_offset as f32;
        vis.texture_mid = vis.gzt - player.viewz;
        vis.x1 = if x1 < 0 { 0 } else { x1 };
        vis.x2 = if x2 >= SCREENWIDTH as i32 {
            SCREENWIDTH as i32 - 1
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
        // Catches certain orientations
        if vis.x1 > x1 {
            vis.start_frac += vis.x_iscale * (vis.x1 - x1) as f32;
        }

        vis.patch = patch_index;
        // TODO: colourmap index
        //  - shadow
        //  - fixed
        if thing.frame & FF_FULLBRIGHT != 0 {
            // full bright
            vis.light_level = 255;
        } else {
            vis.light_level = light_level as usize;
        }

        true
    }

    fn draw_vissprite(
        &self,
        vis: &VisSprite,
        clip_bottom: &[i32],
        clip_top: &[i32],
        canvas: &mut Canvas<Surface>,
    ) {
        let naff = self.texture_data.clone(); // Need to separate lifetimes
        let texture_data = naff.borrow();
        let patch = texture_data.sprite_patch(vis.patch);

        let dc_iscale = vis.x_iscale.abs();
        let dc_texmid = vis.texture_mid;
        let mut frac = vis.start_frac;
        let spryscale = vis.scale;
        let colourmap = texture_data.sprite_light_colourmap(vis.light_level, vis.scale);

        for x in vis.x1..=vis.x2 {
            frac += vis.x_iscale;

            let tex_column = frac.floor() as usize;
            if tex_column >= patch.data.len() {
                break;
                // tex_column %= patch.data.len();
            }

            let sprtopscreen = (SCREENHEIGHT_HALF as f32 + 1.0 - dc_texmid * spryscale).floor();
            let texture_column = &patch.data[tex_column];

            let mut top = sprtopscreen as i32;
            let mut bottom = top + (spryscale * (texture_column.len() as f32 - 1.5)).floor() as i32;

            if bottom >= clip_bottom[x as usize] {
                bottom = clip_bottom[x as usize] - 1;
            }

            if top <= clip_top[x as usize] {
                top = clip_top[x as usize] + 1;
            }

            if top < bottom {
                draw_masked_column(
                    texture_column,
                    colourmap,
                    dc_iscale,
                    x,
                    dc_texmid,
                    top,
                    bottom,
                    &texture_data,
                    canvas,
                );
            }
        }
    }

    /// Doom function name `R_DrawSprite`
    fn draw_sprite(
        &mut self,
        viewz: f32,
        _viewheight: f32,
        vis: &VisSprite,
        canvas: &mut Canvas<Surface>,
    ) {
        let mut clip_bottom = [-2i32; SCREENWIDTH];
        let mut clip_top = [-2i32; SCREENWIDTH];

        // Breaking liftime to enable this loop
        let segs = unsafe { &*(&self.r_data.drawsegs as *const Vec<DrawSeg>) };
        for seg in segs.iter().rev() {
            if seg.x1 > vis.x2
                || seg.x2 < vis.x1
                || (seg.silhouette == 0 && seg.maskedtexturecol == 0)
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
                if scale < vis.scale
                    || (lowscale < vis.scale
                        && seg
                            .curline
                            .as_ref()
                            .point_on_side(&Vec2::new(vis.gx, vis.gy))
                            == 0)
                {
                    if seg.maskedtexturecol != 0 {
                        self.render_masked_seg_range(viewz, seg, r1, r2, canvas);
                    }
                    // seg is behind sprite
                    continue;
                }
            }

            let mut sil = seg.silhouette;
            if vis.gz > seg.bsilheight {
                sil &= !SIL_BOTTOM;
            }
            if vis.gzt < seg.tsilheight {
                sil &= !SIL_TOP;
            }

            if sil == 1 {
                // bottom sil
                for r in r1..=r2 {
                    if clip_bottom[r as usize] == -2 && seg.sprbottomclip.is_some() {
                        clip_bottom[r as usize] = self.r_data.visplanes.openings
                            [(seg.sprbottomclip.unwrap() + r) as usize];
                        if clip_bottom[r as usize] <= 0 {
                            clip_bottom[r as usize] = 0;
                        }
                    }
                }
            } else if sil == 2 {
                // top sil
                for r in r1..=r2 {
                    if clip_top[r as usize] == -2 && seg.sprtopclip.is_some() {
                        clip_top[r as usize] =
                            self.r_data.visplanes.openings[(seg.sprtopclip.unwrap() + r) as usize];
                        if clip_top[r as usize] >= SCREENHEIGHT as i32 {
                            clip_top[r as usize] = SCREENHEIGHT as i32;
                        }
                    }
                }
            } else {
                // both
                for r in r1..=r2 {
                    if clip_bottom[r as usize] == -2 && seg.sprbottomclip.is_some() {
                        clip_bottom[r as usize] = self.r_data.visplanes.openings
                            [(seg.sprbottomclip.unwrap() + r) as usize];
                        if clip_bottom[r as usize] <= 0 {
                            clip_bottom[r as usize] = 0;
                        }
                    }
                    if clip_top[r as usize] == -2 && seg.sprtopclip.is_some() {
                        clip_top[r as usize] =
                            self.r_data.visplanes.openings[(seg.sprtopclip.unwrap() + r) as usize];
                        if clip_top[r as usize] >= SCREENHEIGHT as i32 {
                            clip_top[r as usize] = SCREENHEIGHT as i32;
                        }
                    }
                }
            }
        }

        for x in vis.x1..=vis.x2 {
            if clip_bottom[x as usize] == -2 {
                clip_bottom[x as usize] = SCREENHEIGHT as i32;
            }
            if clip_top[x as usize] == -2 {
                clip_top[x as usize] = -1;
            }
        }

        self.draw_vissprite(vis, &clip_bottom, &clip_top, canvas);
    }

    pub(crate) fn draw_masked(
        &mut self,
        viewz: f32,
        viewheight: f32,
        canvas: &mut Canvas<Surface>,
    ) {
        // Sort only the vissprites used
        self.vissprites[..self.next_vissprite].sort_by(|a, b| a.cmp(b));
        // Need to break lifetime as a chain function call needs &mut on a separate item
        let vis = unsafe { &*(&self.vissprites as *const [VisSprite]) };
        for (i, vis) in vis.iter().enumerate() {
            self.draw_sprite(viewz, viewheight, vis, canvas);
            if i == self.next_vissprite {
                break;
            }
        }

        let segs: Vec<DrawSeg> = (&self.r_data.drawsegs).to_vec();
        for ds in segs.iter().rev() {
            self.render_masked_seg_range(viewz, ds, ds.x1, ds.x2, canvas);
        }

        // todo: R_DrawPlayerSprites ();
    }

    fn render_masked_seg_range(
        &mut self,
        viewz: f32,
        ds: &DrawSeg,
        x1: i32,
        x2: i32,

        canvas: &mut Canvas<Surface>,
    ) {
        let seg = unsafe { ds.curline.as_ref() };
        let frontsector = seg.frontsector.clone();

        if let Some(backsector) = seg.backsector.as_ref() {
            let textures = self.texture_data.borrow();
            let texnum = seg.sidedef.midtexture;
            if texnum == usize::MAX {
                return;
            }

            let wall_lights = seg.sidedef.sector.lightlevel;

            let rw_scalestep = ds.scalestep;
            let mut spryscale = ds.scale1 + (x1 - ds.x1) as f32 * rw_scalestep;

            let mut dc_texturemid;
            if seg.linedef.flags & LineDefFlags::UnpegBottom as u32 != 0 {
                dc_texturemid = if frontsector.floorheight > backsector.floorheight {
                    frontsector.floorheight
                } else {
                    backsector.floorheight
                };

                let texture_column = textures.wall_pic_column(texnum, 0);
                dc_texturemid += texture_column.len() as f32 - viewz;
            } else {
                dc_texturemid = if frontsector.ceilingheight < backsector.ceilingheight {
                    frontsector.ceilingheight
                } else {
                    backsector.ceilingheight
                };
                dc_texturemid -= viewz;
            }
            dc_texturemid += seg.sidedef.rowoffset;

            for x in x1..=x2 {
                if ds.maskedtexturecol + x < 0 {
                    spryscale += rw_scalestep;
                    continue;
                }
                let index = (ds.maskedtexturecol + x) as usize;

                if index != usize::MAX && ds.sprbottomclip.is_some() && ds.sprtopclip.is_some() {
                    if self.r_data.visplanes.openings[index] != i32::MAX
                        && seg.sidedef.midtexture != usize::MAX
                    {
                        let texture_column = textures.wall_pic_column(
                            seg.sidedef.midtexture,
                            self.r_data.visplanes.openings[index],
                        );

                        let mut mceilingclip = self.r_data.visplanes.openings
                            [(ds.sprtopclip.unwrap() + x) as usize]
                            as i32;
                        let mut mfloorclip = self.r_data.visplanes.openings
                            [(ds.sprbottomclip.unwrap() + x) as usize]
                            as i32;
                        if mceilingclip >= SCREENHEIGHT as i32 {
                            mceilingclip = SCREENHEIGHT as i32;
                        }
                        if mfloorclip <= 0 {
                            mfloorclip = 0;
                        }

                        // // calculate unclipped screen coordinates for post
                        let sprtopscreen = SCREENHEIGHT_HALF as f32 - dc_texturemid * spryscale;
                        let top = sprtopscreen as i32;
                        let bottom = top + (spryscale * texture_column.len() as f32) as i32;
                        let mut yl = top;
                        let mut yh = bottom;

                        if bottom >= mfloorclip {
                            yh = mfloorclip - 1;
                        }
                        if top <= mceilingclip {
                            yl = mceilingclip + 1;
                        }

                        draw_masked_column(
                            texture_column,
                            textures.wall_light_colourmap(&seg.v1, &seg.v2, wall_lights, spryscale),
                            1.0 / spryscale,
                            x,
                            dc_texturemid,
                            yl,
                            yh,
                            &textures,
                            canvas,
                        );

                        self.r_data.visplanes.openings[index] = i32::MAX;
                    } else {
                    }
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
    fracstep: f32,
    dc_x: i32,
    dc_texturemid: f32,
    yl: i32,
    yh: i32,
    textures: &PicData,

    canvas: &mut Canvas<Surface>,
) {
    let pal = &textures.palette(0);
    let mut frac = dc_texturemid + (yl as f32 - SCREENHEIGHT_HALF as f32) * fracstep;
    for n in yl..=yh {
        let mut select = frac.floor() as i32 & 127;

        if select >= texture_column.len() as i32 {
            select %= texture_column.len() as i32;
        }

        if texture_column[select as usize] as usize == usize::MAX {
            frac += fracstep;
            continue;
        }

        let px = colourmap[texture_column[select as usize]];
        let colour = pal[px];
        let colour = sdl2::pixels::Color::RGBA(colour.r, colour.g, colour.b, 255);

        canvas.set_draw_color(colour);
        canvas.fill_rect(Rect::new(dc_x, n, 1, 1)).unwrap();
        frac += fracstep;
    }
}