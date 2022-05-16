//! Functions used for various graphical and other tests

use gamestate::Game;
use gameplay::WallPic;

use render_traits::PixelBuf;
use wad::lumps::{WadFlat, WadPalette, WadPatch};

pub(crate) fn palette_test(pal_num: usize, game: &mut Game, pixels: &mut PixelBuf) {
    let height = pixels.height();

    let row_count: i32 = 16;
    let block_size = height as i32 / row_count;

    let x_start = (pixels.width() / 2) as i32 - block_size * row_count / 2;
    let y_start = (pixels.height() / 2) as i32 - block_size * row_count / 2;

    let pals: Vec<WadPalette> = game.wad_data.playpal_iter().collect();

    for (i, c) in pals[pal_num].0.iter().enumerate() {
        pixels.set_pixel(
            (i as i32 % row_count * block_size + x_start) as usize,
            (i as i32 / row_count * block_size + y_start) as usize,
            c.r,
            c.g,
            c.b,
            255,
        );
    }
}

pub(crate) fn image_test(name: &str, game: &Game, pixels: &mut PixelBuf) {
    let lump = game.wad_data.get_lump(name).unwrap();
    let image = WadPatch::from_lump(lump);
    let pals: Vec<WadPalette> = game.wad_data.playpal_iter().collect();

    let xs = ((pixels.width() - image.width as u32) / 2) as i32;
    let ys = ((pixels.height() - image.height as u32) / 2) as i32;

    let mut x = 0;
    for c in image.columns.iter() {
        for (y, p) in c.pixels.iter().enumerate() {
            let colour = pals[0].0[*p];

            pixels.set_pixel(
                (xs + x as i32) as usize, // - (image.left_offset as i32),
                (ys + y as i32 + c.y_offset as i32) as usize, // - image.top_offset as i32 - 30,
                colour.r,
                colour.g,
                colour.b,
                255,
            );
        }
        if c.y_offset == 255 {
            x += 1;
        }
    }
}

pub(crate) fn patch_select_test(image: &WadPatch, game: &Game, pixels: &mut PixelBuf) {
    let pals: Vec<WadPalette> = game.wad_data.playpal_iter().collect();

    let xs = ((pixels.width() - image.width as u32) / 2) as i32;
    let ys = ((pixels.height() - image.height as u32) / 2) as i32;

    let mut x = 0;
    for c in image.columns.iter() {
        for (y, p) in c.pixels.iter().enumerate() {
            let colour = pals[0].0[*p];
            pixels.set_pixel(
                (xs + x as i32) as usize, // - (image.left_offset as i32),
                (ys + y as i32 + c.y_offset as i32) as usize, // - image.top_offset as i32 - 30,
                colour.r,
                colour.g,
                colour.b,
                255,
            );
        }
        if c.y_offset == 255 {
            x += 1;
        }
    }
}

pub(crate) fn texture_select_test(texture: &WallPic, game: &Game, pixels: &mut PixelBuf) {
    let width = texture.data.len() as u32;
    let height = texture.data[0].len() as u32;
    let pals: Vec<WadPalette> = game.wad_data.playpal_iter().collect();

    let xs = ((pixels.width() - width) / 2) as i32;
    let ys = ((pixels.height() - height) / 2) as i32;
    let pal = pals[0].0;

    for (x_pos, column) in texture.data.iter().enumerate() {
        for (y_pos, idx) in column.iter().enumerate() {
            if *idx >= pal.len() {
                continue;
            }
            let colour = pal[*idx];
            pixels.set_pixel(
                (xs + x_pos as i32) as usize,
                (ys + y_pos as i32) as usize,
                colour.r,
                colour.g,
                colour.b,
                255,
            );
        }
    }
}

pub(crate) fn flat_select_test(flat: &WadFlat, game: &Game, pixels: &mut PixelBuf) {
    let pals: Vec<WadPalette> = game.wad_data.playpal_iter().collect();

    let xs = ((pixels.width() - 64) / 2) as i32;
    let ys = ((pixels.height() - 64) / 2) as i32;
    let pal = pals[0].0;

    for (y, col) in flat.data.chunks(64).enumerate() {
        for (x, px) in col.iter().enumerate() {
            if *px as usize >= pal.len() {
                continue;
            }
            let colour = pal[*px as usize];
            pixels.set_pixel(
                (xs + x as i32) as usize,
                (ys + y as i32) as usize,
                colour.r,
                colour.g,
                colour.b,
                255,
            );
        }
    }
}
