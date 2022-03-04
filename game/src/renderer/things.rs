use doom_lib::ML_DONTPEGBOTTOM;
use sdl2::{rect::Rect, render::Canvas, surface::Surface};

use super::{bsp::BspRender, defs::DrawSeg, plane::VisPlaneRender, segs::get_column, RenderData};

impl BspRender {
    pub fn draw_masked(
        &self,
        viewz: f32,
        visplanes: &mut VisPlaneRender,
        rdata: &mut RenderData,
        canvas: &mut Canvas<Surface>,
    ) {
        // todo: R_SortVisSprites
        // todo: R_DrawSprite

        let segs: Vec<DrawSeg> = (&rdata.drawsegs).to_vec();
        for ds in segs.iter().rev() {
            self.render_masked_seg_range(viewz, ds, ds.x1, ds.x2, visplanes, rdata, canvas);
        }

        // todo: R_DrawPlayerSprites ();
    }

    pub fn render_masked_seg_range(
        &self,
        viewz: f32,
        ds: &DrawSeg,
        x1: i32,
        x2: i32,
        visplanes: &mut VisPlaneRender,
        rdata: &mut RenderData,
        canvas: &mut Canvas<Surface>,
    ) {
        let seg = unsafe { ds.curline.as_ref() };
        let frontsector = seg.frontsector.clone();

        if let Some(backsector) = seg.backsector.as_ref() {
            let texnum = seg.sidedef.midtexture;
            if texnum == usize::MAX {
                return;
            }

            // Find a suitable light-table
            let mut lightnum = seg.sidedef.sector.lightlevel as u8 >> 4;
            if seg.v1.y() == seg.v2.y() {
                if lightnum > 1 {
                    lightnum -= 1;
                }
            } else if (seg.v1.x() == seg.v2.x()) && lightnum < 15 {
                lightnum += 1;
            }
            let wall_lights = lightnum as usize;

            let rw_scalestep = ds.scalestep;
            let mut spryscale = ds.scale1 + (x1 - ds.x1) as f32 * rw_scalestep;

            // Select colourmap to use (max should be 48)
            let mut colourmap = (spryscale * 17.0) as usize;
            if colourmap > 47 {
                colourmap = 47;
            }

            let mut dc_texturemid;
            if seg.linedef.flags as u32 & ML_DONTPEGBOTTOM != 0 {
                dc_texturemid = if frontsector.floorheight > backsector.floorheight {
                    frontsector.floorheight
                } else {
                    backsector.floorheight
                };

                let texture = &rdata.textures[texnum];
                let texture_column = get_column(texture, 0.0);
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

            // TESTING
            // TODO: missing column? Offset?
            for x in x1..=x2 {
                if ds.maskedtexturecol + x < 0 {
                    spryscale += rw_scalestep;
                    continue;
                }
                let index = (ds.maskedtexturecol + x) as usize;

                if index != usize::MAX && ds.sprbottomclip.is_some() && ds.sprtopclip.is_some() {
                    if visplanes.openings[index] != f32::MAX && seg.sidedef.midtexture != usize::MAX
                    {
                        let texture = &rdata.textures[seg.sidedef.midtexture];
                        let texture_column = get_column(texture, visplanes.openings[index]); // - 3???

                        let mceilingclip =
                            visplanes.openings[(ds.sprtopclip.unwrap() + x) as usize] as i32;
                        let mfloorclip =
                            visplanes.openings[(ds.sprbottomclip.unwrap() + x) as usize] as i32;

                        // // calculate unclipped screen coordinates for post
                        let sprtopscreen = 100.0 - dc_texturemid * spryscale;
                        let top = sprtopscreen as i32;
                        let bottom = top + (spryscale * texture[0].len() as f32) as i32;
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
                            &rdata.get_lightscale(wall_lights)[colourmap],
                            1.0 / spryscale,
                            x,
                            dc_texturemid,
                            yl,
                            yh,
                            rdata,
                            canvas,
                        );

                        visplanes.openings[index] = f32::MAX;
                    } else {
                        dbg!(x, visplanes.openings[index]);
                    }
                }
                spryscale += rw_scalestep;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_masked_column(
    texture_column: &[usize],
    colourmap: &[usize],
    fracstep: f32,
    dc_x: i32,
    dc_texturemid: f32,
    yl: i32,
    yh: i32,
    rdata: &RenderData,
    canvas: &mut Canvas<Surface>,
) {
    let mut frac = dc_texturemid + (yl as f32 - 100.0) * fracstep;
    for n in yl..=yh {
        let mut select = frac as i32 & 127;

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
            let colour = &rdata.get_palette(0)[px];
            sdl2::pixels::Color::RGBA(colour.r, colour.g, colour.b, 255)
        };

        canvas.set_draw_color(colour);
        canvas.fill_rect(Rect::new(dc_x, n, 1, 1)).unwrap();
        frac += fracstep;
    }
}

/*
TODO:
short negonearray[SCREENWIDTH];
short screenheightarray[SCREENWIDTH];
and drawseg sprtopclip needs to index in to these

short *mfloorclip;
short *mceilingclip;

fixed_t spryscale;
fixed_t sprtopscreen;

Are you fucking serious C? ds->sprbottomclip is a pointer flipping between
negonearray[] and *lastopening which is openings[MAXOPENINGS]
  mfloorclip = ds->sprbottomclip;
  mceilingclip = ds->sprtopclip;
*/
