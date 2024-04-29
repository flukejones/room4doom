//! Functions used for various graphical and other tests

use gameplay::WallPic;
use gamestate::Game;

use render_target::PixelBuffer;
use wad::lumps::{WadFlat, WadPalette, WadPatch};

pub(crate) fn image_test(name: &str, game: &Game, pixels: &mut dyn PixelBuffer) {
    let lump = game.wad_data.get_lump(name).unwrap();
    let image = WadPatch::from_lump(lump);
    let pals: Vec<WadPalette> = game.wad_data.playpal_iter().collect();

    let xs = (pixels.size().width_usize() - image.width as usize) / 2;
    let ys = (pixels.size().height_usize() - image.height as usize) / 2;

    let mut x = 0;
    for c in image.columns.iter() {
        for (y, p) in c.pixels.iter().enumerate() {
            let colour = pals[0].0[*p];

            pixels.set_pixel(
                xs + x,                         // - (image.left_offset as i32),
                (ys + y) + c.y_offset as usize, // - image.top_offset as i32 - 30,
                &colour.0,
            );
        }
        if c.y_offset == 255 {
            x += 1;
        }
    }
}

pub(crate) fn patch_select_test(image: &WadPatch, game: &Game, pixels: &mut dyn PixelBuffer) {
    let pals: Vec<WadPalette> = game.wad_data.playpal_iter().collect();

    let xs = (pixels.size().width_usize() - image.width as usize) / 2;
    let ys = (pixels.size().height_usize() - image.height as usize) / 2;

    let mut x = 0;
    for c in image.columns.iter() {
        for (y, p) in c.pixels.iter().enumerate() {
            let colour = pals[0].0[*p];
            pixels.set_pixel(
                xs + x,                       // - (image.left_offset as i32),
                ys + y + c.y_offset as usize, // - image.top_offset as i32 - 30,
                &colour.0,
            );
        }
        if c.y_offset == 255 {
            x += 1;
        }
    }
}

pub(crate) fn texture_select_test(texture: &WallPic, game: &Game, pixels: &mut dyn PixelBuffer) {
    let width = texture.data.len();
    let height = texture.data[0].len();
    let pals: Vec<WadPalette> = game.wad_data.playpal_iter().collect();

    let xs = (pixels.size().width_usize() - width) / 2;
    let ys = (pixels.size().height_usize() - height) / 2;
    let pal = pals[0].0;

    for (x_pos, column) in texture.data.iter().enumerate() {
        for (y_pos, idx) in column.iter().enumerate() {
            if *idx >= pal.len() {
                continue;
            }
            let colour = pal[*idx];
            pixels.set_pixel(xs + x_pos, ys + y_pos, &colour.0);
        }
    }
}

pub(crate) fn flat_select_test(flat: &WadFlat, game: &Game, pixels: &mut dyn PixelBuffer) {
    let pals: Vec<WadPalette> = game.wad_data.playpal_iter().collect();

    let xs = (pixels.size().width_usize() - 64) / 2;
    let ys = (pixels.size().height_usize() - 64) / 2;
    let pal = pals[0].0;

    for (y, col) in flat.data.chunks(64).enumerate() {
        for (x, px) in col.iter().enumerate() {
            if *px as usize >= pal.len() {
                continue;
            }
            let colour = pal[*px as usize];
            pixels.set_pixel(xs + x, ys + y, &colour.0);
        }
    }
}
