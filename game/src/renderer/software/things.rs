use std::{
    f32::consts::{FRAC_PI_2, PI},
    ptr::null_mut,
};

use doom_lib::{Angle, LineDefFlags, MapObject, PicData, Player, Sector};
use glam::Vec2;
use sdl2::{rect::Rect, render::Canvas, surface::Surface};

use super::{
    bsp::SoftwareRenderer,
    defs::{DrawSeg, SCREENHEIGHT_HALF, SCREENWIDTH},
};

const FF_FULLBRIGHT: u32 = 0x8000;

pub fn point_to_angle_2(point1: Vec2, point2: Vec2) -> Angle {
    let x = point1.x() - point2.x();
    let y = point1.y() - point2.y();
    Angle::new(y.atan2(x))
}

#[derive(Clone, Copy)]
pub struct VisSprite {
    prev: *mut VisSprite,
    next: *mut VisSprite,
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

impl VisSprite {
    pub fn new() -> Self {
        Self {
            prev: null_mut(),
            next: null_mut(),
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
        self.prev = null_mut();
        self.next = null_mut();
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
    pub fn add_sprites<'a>(&'a mut self, player: &Player, sector: &'a Sector) {
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
        if thing.frame > 28 {
            return true;
        }
        let sprite_frame = sprite_def.frames[thing.frame as usize];
        let patch;
        let patch_index;
        let flip;
        if sprite_frame.rotate == 1 {
            let angle = point_to_angle_2(player_mobj.xy, thing.xy);
            let rot = ((angle - thing.angle + FRAC_PI_2 / 3.0).rad()) * 7.0 / (PI * 2.0);
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
        //  - full-bright ( 0 )
        if thing.frame & FF_FULLBRIGHT != 0 {
            // full bright
            vis.light_level = 0;
        } else {
            vis.light_level = light_level as usize;
        }

        true
    }

    pub fn draw_vissprite(&self, vis: &VisSprite, canvas: &mut Canvas<Surface>) {
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

            let sprtopscreen = (SCREENHEIGHT_HALF as f32 - dc_texmid * spryscale).ceil();
            let texture_column = &patch.data[tex_column];

            let top = sprtopscreen as i32;
            let bottom = top + (spryscale * texture_column.len() as f32).floor() as i32 - 2;

            draw_masked_column(
                &texture_column,
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

    pub fn draw_masked(&mut self, viewz: f32, canvas: &mut Canvas<Surface>) {
        // todo: R_SortVisSprites
        // todo: R_DrawSprite
        for vis in self.vissprites.iter() {
            self.draw_vissprite(vis, canvas);
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

                        let mceilingclip = self.r_data.visplanes.openings
                            [(ds.sprtopclip.unwrap() + x) as usize]
                            as i32;
                        let mfloorclip = self.r_data.visplanes.openings
                            [(ds.sprbottomclip.unwrap() + x) as usize]
                            as i32;

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
                        //dbg!(x, self.r_data.visplanes.openings[index]);
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
    let mut frac = dc_texturemid + (yl as f32 - SCREENHEIGHT_HALF as f32) * fracstep;
    for n in yl..=yh {
        let mut select = frac.round() as i32 & 127;

        if select >= texture_column.len() as i32 {
            select %= texture_column.len() as i32;
        }

        if texture_column[select as usize] as usize == usize::MAX {
            frac += fracstep;
            continue;
        }

        let px = colourmap[texture_column[select as usize]];
        let colour = if px == usize::MAX {
            // ERROR COLOUR
            sdl2::pixels::Color::RGBA(255, 0, 0, 255)
        } else {
            let colour = &textures.palette(0)[px];
            sdl2::pixels::Color::RGBA(colour.r, colour.g, colour.b, 255)
        };

        canvas.set_draw_color(colour);
        canvas.fill_rect(Rect::new(dc_x, n, 1, 1)).unwrap();
        frac += fracstep;
    }
}
