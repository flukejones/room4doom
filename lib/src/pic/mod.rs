//! All data and functions related to pictures in Doom.
//! These are:
//! - Wall textures
//! - Flat/span textures
//! - Palettes
//! - Coloumaps and light scaling
//! - Sprites (patches and frame sets)

mod animations;
pub use animations::*;
mod switches;
pub use switches::*;
mod sprites;

use std::mem::{size_of, size_of_val};

use glam::Vec2;
use log::debug;
use wad::{
    lumps::{WadColour, WadPalette, WadPatch, WadTexture},
    WadData,
};

use crate::{doom_def::GameMode, info::SPRNAMES, pic::sprites::init_spritedefs};

use self::sprites::SpriteDef;

const MAXLIGHTZ: i32 = 128;
const LIGHTLEVELS: i32 = 16;
const NUMCOLORMAPS: i32 = 32;
const MAXLIGHTSCALE: i32 = 48;

#[derive(Debug)]
pub struct FlatPic {
    pub name: String,
    pub data: [[u8; 64]; 64],
}

#[derive(Debug)]
pub struct WallPic {
    pub name: String,
    pub data: Vec<Vec<usize>>,
}

#[derive(Debug)]
pub struct SpritePic {
    pub name: String,
    pub left_offset: i32,
    pub top_offset: i32,
    pub data: Vec<Vec<usize>>,
}

#[derive(Debug)]
pub struct PicData {
    /// Colours for pixels
    palettes: Vec<WadPalette>,
    // Usually 34 blocks of 256, each u8 being an index in to the palette
    colourmap: Vec<Vec<usize>>,
    light_scale: Vec<Vec<Vec<usize>>>,
    zlight_scale: Vec<Vec<Vec<usize>>>,
    walls: Vec<WallPic>,
    /// Used in animations
    wall_translation: Vec<usize>,
    flats: Vec<FlatPic>,
    /// Used in animations
    flat_translation: Vec<usize>,
    /// The number flats use to signify a sky should be drawn
    sky_num: usize,
    /// The index number of the texture to use for skybox
    sky_pic: usize,
    //
    sprite_patches: Vec<SpritePic>,
    sprite_defs: Vec<SpriteDef>,
}

impl PicData {
    pub fn init(wad: &WadData) -> Self {
        print!("Init image data  [");

        let colourmap = Self::init_colourmap(wad);
        let palettes = Self::init_palette(wad);
        let light_scale = Self::init_light_scales(&colourmap);
        let zlight_scale = Self::init_zlight_scales(&colourmap);

        let (walls, sky_pic) = Self::init_wall_pics(wad);
        let wall_translation = (0..walls.len()).collect();

        let (flats, sky_num) = Self::init_flat_pics(wad);
        let flat_translation = (0..flats.len()).collect();

        let sprite_patches: Vec<SpritePic> = wad
            .sprites_iter()
            .enumerate()
            .map(|(i, patch)| {
                if i % 64 == 0 {
                    print!(".");
                }

                let mut x_pos = 0;
                let mut compose =
                    vec![vec![usize::MAX; patch.height as usize]; patch.width as usize];
                for c in patch.columns.iter() {
                    if x_pos == patch.width as i32 {
                        break;
                    }
                    for (y, p) in c.pixels.iter().enumerate() {
                        let y_pos = y as i32 + c.y_offset as i32;
                        if y_pos >= 0 && y_pos < patch.height as i32 && x_pos >= 0 {
                            compose[x_pos as usize][y_pos as usize] = *p;
                        }
                    }
                    if c.y_offset == 255 {
                        x_pos += 1;
                    }
                }

                SpritePic {
                    name: patch.name,
                    top_offset: patch.top_offset as i32,
                    left_offset: patch.left_offset as i32,
                    data: compose,
                }
            })
            .collect();
        let sprite_defs = init_spritedefs(&SPRNAMES, &sprite_patches);

        print!(".]\n");

        Self {
            walls,
            wall_translation,
            sky_num,
            sky_pic,
            flats,
            flat_translation,
            palettes,
            light_scale,
            zlight_scale,
            colourmap,
            sprite_patches,
            sprite_defs,
        }
    }

    fn init_palette(wad: &WadData) -> Vec<WadPalette> {
        print!(".");
        wad.playpal_iter().collect()
    }

    fn init_colourmap(wad: &WadData) -> Vec<Vec<usize>> {
        print!(".");
        wad.colourmap_iter()
            .map(|i| i as usize)
            .collect::<Vec<usize>>()
            .chunks(256)
            .map(|v| v.to_owned())
            .collect()
    }

    fn init_light_scales(colourmap: &[Vec<usize>]) -> Vec<Vec<Vec<usize>>> {
        print!(".");
        (0..LIGHTLEVELS)
            .map(|i| {
                let startmap = ((LIGHTLEVELS - 1 - i) * 2) * NUMCOLORMAPS / LIGHTLEVELS;
                (0..MAXLIGHTSCALE)
                    .map(|j| {
                        let mut level = startmap - j / 2;
                        if level < 0 {
                            level = 0;
                        }
                        if level >= NUMCOLORMAPS {
                            level = NUMCOLORMAPS - 1;
                        }
                        colourmap[level as usize].to_owned()
                    })
                    .collect()
            })
            .collect()
    }

    fn init_zlight_scales(colourmap: &[Vec<usize>]) -> Vec<Vec<Vec<usize>>> {
        print!(".");
        (0..LIGHTLEVELS)
            .map(|i| {
                let startmap = ((LIGHTLEVELS - 1 - i) * 2) * NUMCOLORMAPS / LIGHTLEVELS;
                (0..MAXLIGHTZ)
                    .map(|j| {
                        let scale = 160.0 / (j + 1) as f32;
                        let mut level = (startmap as f32 - scale / 2.0) as i32;
                        if level < 0 {
                            level = 0;
                        }
                        if level >= NUMCOLORMAPS {
                            level = NUMCOLORMAPS - 1;
                        }
                        colourmap[level as usize].to_owned()
                    })
                    .collect()
            })
            .collect()
    }

    fn init_wall_pics(wad: &WadData) -> (Vec<WallPic>, usize) {
        print!(".");
        let patches: Vec<WadPatch> = wad.patches_iter().collect();
        print!(".");
        // info!("Init wall textures.");
        let mut skytexture = 0;
        let mut texture_alloc_size = 0;

        let mut pic_func = |(i, tex)| {
            let pic = Self::build_wall_pic(tex, &patches);
            if pic.name == "SKY1" {
                print!(".");
                skytexture = i;
            }
            texture_alloc_size += size_of_val(&pic.name);
            for y in &pic.data {
                texture_alloc_size += size_of::<usize>() * y.len();
            }
            if i % 64 == 0 {
                print!(".");
            }
            pic
        };

        let mut wall_pic: Vec<WallPic> = wad
            .texture_iter("TEXTURE1")
            .enumerate()
            .map(&mut pic_func)
            .collect();

        if wad.lump_exists("TEXTURE2") {
            let mut textures2: Vec<WallPic> = wad
                .texture_iter("TEXTURE2")
                .enumerate()
                .map(&mut pic_func)
                .collect();
            wall_pic.append(&mut textures2);
        };

        let tmp = (texture_alloc_size / 1024).to_string();
        let size = tmp.split_at(2);
        debug!("Total memory used for textures: {},{} KiB", size.0, size.1);

        (wall_pic, skytexture)
    }

    fn init_flat_pics(wad: &WadData) -> (Vec<FlatPic>, usize) {
        print!(".");
        let mut skynum = 256;
        // info!("Init flats.");
        let mut flats = Vec::with_capacity(wad.flats_iter().count());
        print!(".");

        let mut flat_alloc_size = 0;
        for (i, wf) in wad.flats_iter().enumerate() {
            let mut flat = FlatPic {
                name: wf.name,
                data: [[0; 64]; 64],
            };
            for (y, col) in wf.data.chunks(64).enumerate() {
                for (x, px) in col.iter().enumerate() {
                    flat.data[x][y] = *px;
                }
            }
            if flat.name == "F_SKY1" {
                skynum = flats.len();
            }

            flat_alloc_size += size_of_val(&flat.name);
            flat_alloc_size += flat.data.len() * flat.data[0].len() * size_of::<u8>();
            if i % 32 == 0 {
                print!(".");
            }

            flats.push(flat);
        }

        debug!(
            "Total memory used for flats: {} KiB",
            flat_alloc_size / 1024
        );

        (flats, skynum)
    }

    /// Build a texture out of patches and return it
    fn build_wall_pic(texture: WadTexture, patches: &[WadPatch]) -> WallPic {
        let mut compose = vec![vec![usize::MAX; texture.height as usize]; texture.width as usize];

        for patch_pos in &texture.patches {
            let patch = &patches[patch_pos.patch_index];
            // draw patch
            let mut x_pos = patch_pos.origin_x;
            for c in patch.columns.iter() {
                if x_pos == texture.width as i32 {
                    break;
                }
                for (y, p) in c.pixels.iter().enumerate() {
                    let y_pos = y as i32 + patch_pos.origin_y + c.y_offset as i32;
                    if y_pos >= 0 && y_pos < texture.height as i32 && x_pos >= 0 {
                        compose[x_pos as usize][y_pos as usize] = *p;
                    }
                }
                if c.y_offset == 255 {
                    x_pos += 1;
                }
            }
        }
        debug!("Built texture: {}", &texture.name);
        WallPic {
            name: texture.name.clone(),
            data: compose,
        }
    }

    pub fn palette(&self, num: usize) -> &[WadColour] {
        &self.palettes[num].0
    }

    /// Get the number of the flat used for the sky texture. Sectors using this number
    /// for the flat will be rendered witht eh skybox.
    pub fn sky_num(&self) -> usize {
        self.sky_num
    }

    /// Get the index used by `get_texture()` to return a texture.
    pub fn sky_pic(&self) -> usize {
        self.sky_pic
    }

    /// Set the correct skybox for the map/episode currently playing
    pub fn set_sky_pic(&mut self, mode: GameMode, episode: i32, map: i32) {
        if mode == GameMode::Commercial {
            self.sky_pic = self.wallpic_num_for_name("SKY3").expect("SKY3 is missing");
            if map < 12 {
                self.sky_pic = self.wallpic_num_for_name("SKY1").expect("SKY1 is missing");
            } else if map < 21 {
                self.sky_pic = self.wallpic_num_for_name("SKY2").expect("SKY2 is missing");
            }
        } else {
            match episode {
                2 => {
                    self.sky_pic = self.wallpic_num_for_name("SKY2").expect("SKY2 is missing");
                }
                3 => {
                    self.sky_pic = self.wallpic_num_for_name("SKY3").expect("SKY3 is missing");
                }
                4 => {
                    self.sky_pic = self.wallpic_num_for_name("SKY4").expect("SKY4 is missing");
                }
                _ => {
                    self.sky_pic = self.wallpic_num_for_name("SKY1").expect("SKY1 is missing");
                }
            }
        }
    }

    pub fn colourmap(&self, index: usize) -> &[usize] {
        &self.colourmap[index]
    }

    /// Get the correct colourmapping for a light level. The colourmap is indexed by the Y coordinate
    /// of a texture column.
    pub fn wall_light_colourmap(
        &self,
        v1: &Vec2,
        v2: &Vec2,
        light_level: i32,
        wall_scale: f32,
    ) -> &[usize] {
        let mut light_level = light_level >> 4;
        if v1.y() == v2.y() {
            if light_level > 1 {
                light_level -= 1;
            }
        } else if (v1.x() == v2.x()) && light_level < 15 {
            light_level += 1;
        }

        let mut colourmap = (wall_scale * 15.8).round() as u32;
        if colourmap >= MAXLIGHTSCALE as u32 - 1 {
            colourmap = MAXLIGHTSCALE as u32 - 1;
        }

        &self.light_scale[light_level as usize][colourmap as usize]
    }

    pub fn flat_light_colourmap(&self, light_level: i32, wall_scale: f32) -> &[usize] {
        let mut dist = (wall_scale as i32 >> 4) as u32;
        let light_level = light_level >> 4;

        if dist >= MAXLIGHTZ as u32 - 1 {
            dist = MAXLIGHTZ as u32 - 1;
        }

        &self.zlight_scale[light_level as usize][dist as usize]
    }

    pub fn get_texture(&self, num: usize) -> &WallPic {
        let num = self.wall_translation[num];
        &self.walls[num]
    }

    pub fn get_flat(&self, num: usize) -> &FlatPic {
        let num = self.flat_translation[num];
        &self.flats[num]
    }

    pub fn wallpic_num_for_name(&self, name: &str) -> Option<usize> {
        for (i, tex) in self.walls.iter().enumerate() {
            if tex.name == name {
                return Some(i);
            }
        }
        None
    }

    pub fn flat_num_for_name(&self, name: &str) -> Option<usize> {
        for (i, tex) in self.flats.iter().enumerate() {
            if tex.name == name {
                return Some(i);
            }
        }
        None
    }

    /// Return a ref to the specified column of the requested texture
    pub fn wall_pic_column(&self, texture: usize, texture_column: i32) -> &[usize] {
        let texture = &self.walls[self.wall_translation[texture]];
        let mut col = texture_column;
        if col >= texture.data.len() as i32 {
            col -= 1;
        }
        let index = col & (texture.data.len() as i32 - 1);
        &texture.data[index as usize]
    }

    pub fn num_textures(&self) -> usize {
        self.walls.len()
    }

    pub fn sprite_def(&self, sprite_num: usize) -> &SpriteDef {
        &self.sprite_defs[sprite_num]
    }

    pub fn sprite_patch(&self, patch_num: usize) -> &SpritePic {
        &self.sprite_patches[patch_num]
    }
}
