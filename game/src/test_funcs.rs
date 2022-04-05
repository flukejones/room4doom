//! Functions used for various graphical and other tests

use gameplay::WallPic;

use sdl2::{pixels::Color, rect::Rect, render::Canvas, surface::Surface};
use wad::lumps::{WadFlat, WadPalette, WadPatch};

use crate::game::Game;

pub(crate) fn palette_test(pal_num: usize, game: &mut Game, canvas: &mut Canvas<Surface>) {
    let height = canvas.surface().height();

    let row_count: i32 = 16;
    let block_size = height as i32 / row_count;

    let x_start = (canvas.surface().width() / 2) as i32 - block_size * row_count / 2;
    let y_start = (canvas.surface().height() / 2) as i32 - block_size * row_count / 2;

    let pals: Vec<WadPalette> = game.wad_data.playpal_iter().collect();

    for (i, c) in pals[pal_num].0.iter().enumerate() {
        canvas.set_draw_color(Color::RGB(c.r, c.g, c.b));
        canvas
            .fill_rect(Rect::new(
                i as i32 % row_count * block_size + x_start,
                i as i32 / row_count * block_size + y_start,
                block_size as u32,
                block_size as u32,
            ))
            .unwrap();
    }
}

pub(crate) fn image_test(name: &str, game: &Game, canvas: &mut Canvas<Surface>) {
    let lump = game.wad_data.get_lump(name).unwrap();
    let image = WadPatch::from_lump(lump);
    let pals: Vec<WadPalette> = game.wad_data.playpal_iter().collect();

    let xs = ((canvas.surface().width() - image.width as u32) / 2) as i32;
    let ys = ((canvas.surface().height() - image.height as u32) / 2) as i32;

    let mut x = 0;
    for c in image.columns.iter() {
        for (y, p) in c.pixels.iter().enumerate() {
            let colour = pals[0].0[*p];
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

pub(crate) fn patch_select_test(image: &WadPatch, game: &Game, canvas: &mut Canvas<Surface>) {
    let pals: Vec<WadPalette> = game.wad_data.playpal_iter().collect();

    let xs = ((canvas.surface().width() - image.width as u32) / 2) as i32;
    let ys = ((canvas.surface().height() - image.height as u32) / 2) as i32;

    let mut x = 0;
    for c in image.columns.iter() {
        for (y, p) in c.pixels.iter().enumerate() {
            let colour = pals[0].0[*p];
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

pub(crate) fn texture_select_test(texture: &WallPic, game: &Game, canvas: &mut Canvas<Surface>) {
    let width = texture.data.len() as u32;
    let height = texture.data[0].len() as u32;
    let pals: Vec<WadPalette> = game.wad_data.playpal_iter().collect();

    let xs = ((canvas.surface().width() - width) / 2) as i32;
    let ys = ((canvas.surface().height() - height) / 2) as i32;
    let pal = pals[0].0;

    for (x_pos, column) in texture.data.iter().enumerate() {
        for (y_pos, idx) in column.iter().enumerate() {
            if *idx >= pal.len() {
                continue;
            }
            let colour = pal[*idx];
            canvas.set_draw_color(Color::RGB(colour.r, colour.g, colour.b));
            canvas
                .fill_rect(Rect::new(xs + x_pos as i32, ys + y_pos as i32, 1, 1))
                .unwrap();
        }
    }
}

pub(crate) fn flat_select_test(flat: &WadFlat, game: &Game, canvas: &mut Canvas<Surface>) {
    let pals: Vec<WadPalette> = game.wad_data.playpal_iter().collect();

    let xs = ((canvas.surface().width() - 64) / 2) as i32;
    let ys = ((canvas.surface().height() - 64) / 2) as i32;
    let pal = pals[0].0;

    for (y, col) in flat.data.chunks(64).enumerate() {
        for (x, px) in col.iter().enumerate() {
            if *px as usize >= pal.len() {
                continue;
            }
            let colour = pal[*px as usize];
            canvas.set_draw_color(Color::RGB(colour.r, colour.g, colour.b));
            canvas
                .fill_rect(Rect::new(xs + x as i32, ys + y as i32, 1, 1))
                .unwrap();
        }
    }
}