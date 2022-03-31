use doom_lib::{LineDefFlags, MapObject, MapObjectType, PicData, Player, Sector};
use sdl2::{pixels::Color, rect::Rect, render::Canvas, surface::Surface};

use super::{
    bsp::SoftwareRenderer,
    defs::{DrawSeg, SCREENHEIGHT_HALF, SCREENWIDTH},
};

impl SoftwareRenderer {
    pub fn add_sprites<'a>(
        &'a mut self,
        player: &Player,
        sector: &'a Sector,
        canvas: &mut Canvas<Surface>,
    ) {
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

        sector.run_rfunc_on_thinglist(|thing| self.project_sprite(player, thing, canvas));
    }

    fn project_sprite(
        &mut self,
        player: &Player,
        thing: &MapObject,
        canvas: &mut Canvas<Surface>,
    ) -> bool {
        if thing.player.is_some() {
            return true;
        }
        // transform the origin point
        let player_mobj = unsafe { &*player.mobj.unwrap() };
        let view_cos = player_mobj.angle.cos();
        let view_sin = player_mobj.angle.sin();

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
        let tx = -(gyt + gxt);

        // too far off the side?
        if tx.abs() as i32 > (tz.abs() as i32) << 2 {
            return true;
        }

        // Find the sprite def to use
        let texture_data = self.texture_data.borrow();
        let sprnum = thing.state.sprite;
        let sprite_def = texture_data.sprite_def(sprnum as usize);
        if thing.frame > 28 {
            return true;
        }
        let sprite_frame = sprite_def.frames[0];

        // TODO: TEMPORARY TEST BLOCK HERE
        {
            let image = texture_data.sprite_patch(sprite_frame.lump[0] as usize);
            let pal = texture_data.palette(0);

            let xs = ((canvas.surface().width() - image.width as u32) / 2) as i32;
            let ys = ((canvas.surface().height() - image.height as u32) / 2) as i32;

            let mut x = 0;
            for c in image.columns.iter() {
                for (y, p) in c.pixels.iter().enumerate() {
                    let colour = pal[*p];
                    canvas.set_draw_color(Color::RGB(colour.r, colour.g, colour.b));
                    canvas
                        .fill_rect(Rect::new(
                            xs + x as i32,                     // - (image.left_offset as i32),
                            ys + y as i32 + c.y_offset as i32, // - image.top_offset as i32 - 30,
                            1,
                            1,
                        ))
                        .unwrap();
                }
                if c.y_offset == 255 {
                    x += 1;
                }
            }
        }

        true
    }

    pub fn draw_masked(&mut self, viewz: f32, canvas: &mut Canvas<Surface>) {
        // todo: R_SortVisSprites
        // todo: R_DrawSprite

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

        while select >= texture_column.len() as i32 {
            select -= texture_column.len() as i32;
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
