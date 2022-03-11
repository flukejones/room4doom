mod anims;
pub use anims::*;
mod switches;
pub use switches::*;

use std::mem::{size_of, size_of_val};

use glam::Vec2;
use log::debug;
use wad::{
    lumps::{WadColour, WadPalette, WadPatch, WadTexture},
    WadData,
};

const LIGHTLEVELS: i32 = 16;
const NUMCOLORMAPS: i32 = 32;
const MAXLIGHTSCALE: i32 = 48;

#[derive(Debug, Default)]
pub struct Texture {
    pub name: String,
    pub data: Vec<Vec<usize>>,
}

#[derive(Debug, Default)]
pub struct TextureData {
    /// Colours for pixels
    palettes: Vec<WadPalette>,
    // Usually 34 blocks of 256, each u8 being an index in to the palette
    _colourmap: Vec<Vec<usize>>,
    lightscale: Vec<Vec<Vec<usize>>>,
    /// Indexing is [texture num][x][y]
    walls: Vec<Texture>,
    wall_translation: Vec<usize>,
    skyflatnum: i16,
}

impl TextureData {
    pub fn new(wad: &WadData) -> Self {
        let palettes = wad.playpal_iter().collect();
        let colourmap: Vec<Vec<usize>> = wad
            .colourmap_iter()
            .map(|i| i as usize)
            .collect::<Vec<usize>>()
            .chunks(256)
            .map(|v| v.to_owned())
            .collect();

        let lightscale = (0..LIGHTLEVELS)
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
            .collect();

        // for i in 0..LIGHTLEVELS {
        //     // TODO: const LIGHTLEVELS
        //     let startmap = ((LIGHTLEVELS - 1 - i) * 2) * NUMCOLORMAPS / LIGHTLEVELS;
        //     for j in 0..MAXLIGHTSCALE {
        //         let mut level = startmap - j / 2;
        //         if level < 0 {
        //             level = 0;
        //         }
        //         if level >= NUMCOLORMAPS {
        //             level = NUMCOLORMAPS - 1;
        //         }
        //     }
        // }

        let patches: Vec<WadPatch> = wad.patches_iter().collect();
        let mut textures: Vec<Texture> = wad
            .texture_iter("TEXTURE1")
            .map(|tex| Self::compose_texture(tex, &patches))
            .collect();
        let mut texture_translation = (0..textures.len()).collect();

        if wad.lump_exists("TEXTURE2") {
            let mut textures2: Vec<Texture> = wad
                .texture_iter("TEXTURE2")
                .map(|tex| Self::compose_texture(tex, &patches))
                .collect();
            textures.append(&mut textures2);
            texture_translation = (0..textures.len()).collect();
        }

        let mut size = 0;
        for x in &textures {
            size += size_of_val(&x.name);
            for y in &x.data {
                for _ in y {
                    size += size_of::<usize>();
                }
            }
        }
        debug!("Total memory used for textures: {}KiB", size / 1024);

        Self {
            palettes,
            _colourmap: colourmap,
            lightscale,
            walls: textures,
            wall_translation: texture_translation,
            skyflatnum: 256, // TODO: find index number from parsed flats
        }
    }

    fn compose_texture(texture: WadTexture, patches: &[WadPatch]) -> Texture {
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
        Texture {
            name: texture.name.clone(),
            data: compose,
        }
    }

    pub fn palette(&self, num: usize) -> &[WadColour] {
        &self.palettes[num].0
    }

    pub fn skyflatnum(&self) -> i16 {
        self.skyflatnum
    }

    // pub fn get_colourmap(&self, index: usize) -> &[usize] {
    //     &self.colourmap[index]
    // }

    // pub fn get_lightscale(&self, index: usize) -> &Vec<Vec<usize>> {
    //     &self.lightscale[index]
    // }

    /// Get the correct colourmapping for a light level. The colourmap is indexed by the Y coordinate
    /// of a texture column.
    pub fn get_light_colourmap(
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

        let mut colourmap = (wall_scale * 15.8).round() as usize;
        if colourmap > 47 {
            colourmap = 47;
        }

        &self.lightscale[light_level as usize][colourmap]
    }

    pub fn get_texture(&self, num: usize) -> &Texture {
        let num = self.wall_translation[num];
        &self.walls[num]
    }

    pub fn texture_num_for_name(&self, name: &str) -> Option<usize> {
        for (i, tex) in self.walls.iter().enumerate() {
            if tex.name == name {
                return Some(i);
            }
        }
        None
    }

    pub fn get_column(&self, texture: usize, texture_column: f32) -> &[usize] {
        let texture = &self.walls[self.wall_translation[texture]];
        let mut col = texture_column.ceil() as i32;
        if col >= texture.data.len() as i32 {
            col -= 1;
        }
        let index = col & (texture.data.len() as i32 - 1);
        &texture.data[index as usize]
    }

    pub fn num_textures(&self) -> usize {
        self.walls.len()
    }
}
