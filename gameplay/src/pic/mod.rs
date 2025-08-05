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

use log::{debug, warn};
use wad::WadData;
use wad::types::{WadColour, WadPalette, WadPatch, WadTexture};

use crate::Player;
use crate::doom_def::{GameMode, PowerType};
use crate::info::SPRNAMES;
use crate::pic::sprites::init_spritedefs;

use self::sprites::SpriteDef;

const MAXLIGHTZ: usize = 128;
const LIGHTLEVELS: i32 = 16;
const NUMCOLORMAPS: i32 = 32;
const MAXLIGHTSCALE: i32 = 48;
const LIGHTMAP_LEN: usize = 48 * 16;
pub const INVERSECOLORMAP: i32 = 32;
const STARTREDPALS: usize = 1;
const NUMREDPALS: usize = 8;
const STARTBONUSPALS: usize = 9;
const NUMBONUSPALS: usize = 4;
const RADIATIONPAL: usize = 13;

#[derive(Debug)]
pub struct FlatPic {
    pub name: String,
    pub data: [usize; 64 * 64],
    pub width: usize,
    pub height: usize,
    pub mip_levels: Vec<MipLevel>,
}

#[derive(Debug)]
pub struct WallPic {
    pub name: String,
    pub data: Vec<usize>,
    pub width: usize,
    pub height: usize,
    pub mip_levels: Vec<MipLevel>,
}

#[derive(Debug)]
pub struct MipLevel {
    pub data: Vec<usize>,
    pub width: usize,
    pub height: usize,
}

#[derive(Debug)]
pub struct SpritePic {
    pub name: String,
    pub left_offset: i32,
    pub top_offset: i32,
    pub data: Vec<Vec<usize>>,
}

type Colourmap = [usize; 256];
const PALLETE_LEN: usize = 14;
const COLOURMAP_LEN: usize = 34;

#[derive(Debug)]
pub struct PicData {
    /// Colours for pixels
    palettes: [WadPalette; PALLETE_LEN],
    // Usually 34 blocks of 256, each u8 being an index in to the palette
    colourmap: [Colourmap; COLOURMAP_LEN],
    // 16 groups of 48 sets of indexes to colourmap
    light_scale: [usize; LIGHTMAP_LEN],
    lightscale_colourmap: Vec<Colourmap>,
    // 16 groups of 128 sets of palette
    zlight_scale: [[usize; 128]; 16],
    use_fixed_colourmap: usize,
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
    /// The pallette to be used. Can be set with `set_pallette()` or
    /// `set_player_palette()`, typically done on frame start to set effects
    /// like take-damage.
    use_pallette: usize,
}

impl Default for PicData {
    fn default() -> Self {
        Self {
            palettes: Default::default(),
            colourmap: [[0; 256]; COLOURMAP_LEN],
            light_scale: [0; LIGHTMAP_LEN],
            zlight_scale: [[0usize; 128]; 16],
            use_fixed_colourmap: Default::default(),
            walls: Default::default(),
            wall_translation: Default::default(),
            flats: Default::default(),
            flat_translation: Default::default(),
            sky_num: Default::default(),
            sky_pic: Default::default(),
            sprite_patches: Default::default(),
            sprite_defs: Default::default(),
            use_pallette: Default::default(),
            lightscale_colourmap: vec![[0usize; 256]; LIGHTMAP_LEN],
        }
    }
}

impl PicData {
    pub fn init(wad: &WadData) -> Self {
        print!("Init image data  [");

        let colourmap = Self::init_colourmap(wad);
        let palettes = Self::init_palette(wad);
        let light_scale = Self::init_light_scales();
        let zlight_scale = Self::init_zlight_scales();

        // Precompute lightscale_colourmap merging light_scale and colourmap entries by clone
        let mut lightscale_colourmap = vec![[0usize; 256]; LIGHTMAP_LEN];
        for (i, &light_scale_idx) in light_scale.iter().enumerate() {
            lightscale_colourmap[i].clone_from_slice(&colourmap[light_scale_idx]);
        }

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
                        let y_pos = y as i32 + c.y_offset;
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

        println!(".]");

        Self {
            walls,
            wall_translation,
            sky_num,
            sky_pic,
            flats,
            flat_translation,
            palettes,
            light_scale,
            lightscale_colourmap,
            zlight_scale,
            colourmap,
            use_fixed_colourmap: 0,
            sprite_patches,
            sprite_defs,
            use_pallette: 0,
        }
    }

    fn generate_mip_levels(data: &[usize], width: usize, height: usize) -> Vec<MipLevel> {
        let mut mips = Vec::new();

        // Don't create mipmaps for textures smaller than 32x32
        if width < 32 || height < 32 {
            return mips;
        }

        // Don't create mipmaps for very narrow textures (common in Doom walls)
        let aspect_ratio = width.max(height) as f32 / width.min(height) as f32;
        if aspect_ratio > 8.0 {
            return mips;
        }

        let mut current_width = width;
        let mut current_height = height;
        let mut current_data = data.to_vec();

        // Generate mip levels, stopping at 4x4 minimum
        while current_width > 4 && current_height > 4 {
            current_width /= 2;
            current_height /= 2;

            let mut new_data = vec![0usize; current_width * current_height];

            for y in 0..current_height {
                for x in 0..current_width {
                    let src_x = x * 2;
                    let src_y = y * 2;
                    let src_width = current_width * 2;

                    // Sample 2x2 block
                    let samples = [
                        current_data[src_y * src_width + src_x],
                        current_data[src_y * src_width + src_x.min(src_width - 1)],
                        current_data[(src_y + 1).min(current_height * 2 - 1) * src_width + src_x],
                        current_data[(src_y + 1).min(current_height * 2 - 1) * src_width
                            + src_x.min(src_width - 1)],
                    ];

                    // Find most common non-transparent pixel
                    let mut opaque_samples = Vec::new();
                    let mut transparent_count = 0;

                    for &sample in &samples {
                        if sample == usize::MAX {
                            transparent_count += 1;
                        } else {
                            opaque_samples.push(sample);
                        }
                    }

                    // If majority is transparent, use transparency
                    // Otherwise use first opaque sample
                    new_data[y * current_width + x] = if transparent_count >= 2 {
                        usize::MAX
                    } else if !opaque_samples.is_empty() {
                        opaque_samples[0]
                    } else {
                        usize::MAX
                    };
                }
            }

            mips.push(MipLevel {
                data: new_data.clone(),
                width: current_width,
                height: current_height,
            });

            current_data = new_data;
        }

        mips
    }

    fn generate_flat_mip_levels(data: &[usize; 64 * 64]) -> Vec<MipLevel> {
        let mut mips = Vec::new();
        let mut current_width = 64;
        let mut current_height = 64;
        let mut current_data: Vec<usize> = data.to_vec();

        // Generate mip levels for 64x64 flats: 32x32, 16x16, 8x8, 4x4
        while current_width > 4 && current_height > 4 {
            current_width /= 2;
            current_height /= 2;

            let mut new_data = vec![0usize; current_width * current_height];

            for y in 0..current_height {
                for x in 0..current_width {
                    let src_x = x * 2;
                    let src_y = y * 2;

                    // Sample 2x2 block from 64x64 data
                    let samples = [
                        current_data[src_y * (current_width * 2) + src_x],
                        current_data[src_y * (current_width * 2) + src_x + 1],
                        current_data[(src_y + 1) * (current_width * 2) + src_x],
                        current_data[(src_y + 1) * (current_width * 2) + src_x + 1],
                    ];

                    // For floors, just use first sample (floors are usually solid)
                    new_data[y * current_width + x] = samples[0];
                }
            }

            mips.push(MipLevel {
                data: new_data.clone(),
                width: current_width,
                height: current_height,
            });

            current_data = new_data;
        }

        mips
    }

    fn init_palette(wad: &WadData) -> [WadPalette; PALLETE_LEN] {
        print!(".");
        let mut tmp = [WadPalette::default(); PALLETE_LEN];
        for (i, p) in wad.playpal_iter().enumerate() {
            tmp[i] = p;
        }
        tmp
    }

    fn init_colourmap(wad: &WadData) -> [Colourmap; COLOURMAP_LEN] {
        print!(".");
        let mut tmp = [[0; 256]; COLOURMAP_LEN];
        let maps: Vec<Colourmap> = wad
            .colourmap_iter()
            .map(|i| i as usize)
            .collect::<Vec<usize>>()
            .chunks(256)
            .map(|v| {
                let mut tmp: Colourmap = [0; 256];
                tmp.copy_from_slice(v);
                tmp
            })
            .collect();
        tmp.copy_from_slice(&maps);
        tmp
    }

    /// Populate the indexes to colourmaps
    fn init_light_scales() -> [usize; LIGHTMAP_LEN] {
        print!(".");
        let mut tmp = [0; LIGHTMAP_LEN];
        for i in 0..LIGHTLEVELS {
            let startmap = ((LIGHTLEVELS - 1 - i) * 2) * NUMCOLORMAPS / LIGHTLEVELS;
            for j in 0..MAXLIGHTSCALE {
                let mut level = startmap - j / 2;
                if level < 0 {
                    level = 0;
                }
                level = level.min(NUMCOLORMAPS - 1);
                // TODO: maybe turn this in to indexing? of colourmaps
                // tmp[i as usize][j as usize].copy_from_slice(&colourmap[level as usize]);
                tmp[i as usize * 48 + j as usize] = level as usize;
            }
        }
        tmp
    }

    /// A non-zero value is the the colourmap number forced to use for all
    /// light-levels
    pub const fn set_fixed_lightscale(&mut self, colourmap: usize) {
        self.use_fixed_colourmap = colourmap
    }

    fn init_zlight_scales() -> [[usize; 128]; 16] {
        print!(".");
        let mut tmp = [[0usize; 128]; 16];
        for i in 0..LIGHTLEVELS {
            let startmap = ((LIGHTLEVELS - 1 - i) * 2) * NUMCOLORMAPS / LIGHTLEVELS;
            for j in 0..MAXLIGHTZ {
                let scale = (160 / (j + 1)) as f32;
                let mut level = startmap - (scale / 2.0) as i32;
                if level < 0 {
                    level = 0;
                }
                level = level.min(NUMCOLORMAPS - 1);
                tmp[i as usize][j] = level as usize;
            }
        }
        tmp
    }

    fn init_wall_pics(wad: &WadData) -> (Vec<WallPic>, usize) {
        print!(".");
        let patches: Vec<WadPatch> = wad.patches_iter().collect();
        // Need to include flats
        let pnames: Vec<String> = wad.pnames_iter().collect();
        let mut sorted_patches: Vec<WadPatch> = Vec::with_capacity(pnames.len());
        for name in &pnames {
            let mut log = true;
            for patch in &patches {
                if &patch.name == name {
                    sorted_patches.push(patch.clone());
                    log = false;
                    break;
                }
            }
            if log {
                // Try to find missing patches
                if let Some(lump) = wad.get_lump(name) {
                    sorted_patches.push(WadPatch::from_lump(lump));
                } else {
                    warn!("Mising: {name}");
                }
            }
        }
        print!(".");
        // info!("Init wall textures.");
        let mut skytexture = 0;
        let mut texture_alloc_size = 0;

        let mut pic_func = |(i, tex)| {
            let pic = Self::build_wall_pic(tex, &sorted_patches);
            if pic.name == "SKY1" {
                print!(".");
                skytexture = i;
            }
            texture_alloc_size += size_of_val(&pic.name) + size_of::<usize>() * pic.data.len();
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
                name: wf.name.clone(),
                data: [0; 64 * 64],
                width: 64,
                height: 64,
                mip_levels: Vec::new(),
            };
            let mut outofbounds = false;
            for (x, col) in wf.data.chunks(64).enumerate() {
                if x >= 64 || outofbounds {
                    outofbounds = true;
                    break;
                }
                for (y, px) in col.iter().enumerate() {
                    if y >= 64 || outofbounds {
                        outofbounds = true;
                        break;
                    }
                    flat.data[x * flat.height + y] = *px as usize;
                }
            }
            if outofbounds {
                warn!("Flat {} was not 64x64 in size", wf.name);
            }
            if flat.name == "F_SKY1" {
                skynum = flats.len();
            }

            flat_alloc_size += size_of_val(&flat.name);
            flat_alloc_size += flat.data.len() * size_of::<usize>();
            if i % 32 == 0 {
                print!(".");
            }

            // Generate mipmaps for the flat
            flat.mip_levels = Self::generate_flat_mip_levels(&flat.data);
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
        let mut compose = vec![usize::MAX; texture.height as usize * texture.width as usize];
        for wad_tex_patch in texture.patches.iter() {
            let wad_patch = &patches[wad_tex_patch.patch_index];
            // draw patch
            let mut x_pos = wad_tex_patch.origin_x;
            if x_pos.is_negative() {
                // OG Doom sets the start to 0 if less than 0
                // skip = x_pos.abs() as usize;
                x_pos = 0;
            }

            for patch_column in wad_patch.columns.iter() {
                if patch_column.y_offset == 255 {
                    x_pos += 1;
                    continue;
                }
                if x_pos == texture.width as i32 {
                    break;
                }

                for (y, p) in patch_column.pixels.iter().enumerate() {
                    let y_pos = y as i32 + wad_tex_patch.origin_y + patch_column.y_offset;
                    let pos = x_pos * texture.height as i32 + y_pos;
                    if y_pos >= 0 && pos < compose.len() as i32 {
                        compose[pos as usize] = *p;
                    }
                }
            }
        }

        debug!("Built texture: {}", &texture.name);
        let mip_levels =
            Self::generate_mip_levels(&compose, texture.width as usize, texture.height as usize);
        WallPic {
            name: texture.name,
            width: texture.width as usize,
            height: texture.height as usize,
            data: compose,
            mip_levels,
        }
    }

    #[inline(always)]
    pub const fn palette(&self) -> &[WadColour] {
        &self.palettes[self.use_pallette].0
    }

    #[inline(always)]
    pub fn set_palette(&mut self, num: usize) {
        self.use_pallette = num.min(self.palettes.len() - 1);
    }

    /// Used to set effects for the player visually, such as damage
    pub fn set_player_palette(&mut self, player: &Player) {
        let mut damagecount = player.status.damagecount;
        let berkers;

        if player.status.powers[PowerType::Strength as usize] != 0 {
            // slowly fade the berzerk out
            berkers = 12 - (player.status.powers[PowerType::Strength as usize] >> 6);

            if berkers > damagecount {
                damagecount = berkers;
            }
        }

        if damagecount != 0 {
            self.use_pallette = ((damagecount + 7) >> 3) as usize;
            self.use_pallette = self.use_pallette.min(NUMREDPALS - 1);
            self.use_pallette += STARTREDPALS;
        } else if player.status.bonuscount != 0 {
            self.use_pallette = ((player.status.bonuscount + 7) >> 3) as usize;
            self.use_pallette = self.use_pallette.min(NUMBONUSPALS - 1);
            self.use_pallette += STARTBONUSPALS;
        } else if player.status.powers[PowerType::IronFeet as usize] > 4 * 32
            || player.status.powers[PowerType::IronFeet as usize] & 8 != 0
        {
            self.use_pallette = RADIATIONPAL;
        } else {
            self.use_pallette = 0;
        }

        if self.use_pallette >= self.palettes.len() {
            self.use_pallette = self.palettes.len() - 1;
        }
    }

    /// Get the number of the flat used for the sky texture. Sectors using this
    /// number for the flat will be rendered with the skybox.
    #[inline(always)]
    pub const fn sky_num(&self) -> usize {
        self.sky_num
    }

    /// Get the index used by `get_texture()` to return a texture.
    #[inline(always)]
    pub const fn sky_pic(&self) -> usize {
        self.sky_pic
    }

    /// Set the correct skybox for the map/episode currently playing
    #[inline(always)]
    pub fn set_sky_pic(&mut self, mode: GameMode, episode: usize, map: usize) {
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

    #[inline(always)]
    pub fn colourmap(&self, index: usize) -> &[usize] {
        &self.colourmap[index]
    }

    #[inline(always)]
    fn colourmap_for_scale(&self, scale: f32) -> usize {
        // let colourmap = if self.double_res {
        //     (scale * 7.9) as u32
        // } else {
        let colourmap = (scale * 15.8) as u32;
        // };
        colourmap.min(MAXLIGHTSCALE as u32 - 1) as usize
    }

    #[inline(always)]
    pub fn base_colourmap(&self, light_level: usize, wall_scale: f32) -> &[usize] {
        let colourmap = (wall_scale as u32).min(47) as usize;
        unsafe {
            self.lightscale_colourmap
                .get_unchecked(light_level * 48 + colourmap)
        }
    }

    /// Get the correct colourmapping for a light level. The colourmap is
    /// indexed by the Y coordinate of a texture column.
    #[inline(always)]
    pub fn vert_light_colourmap(&self, light_level: usize, wall_scale: f32) -> &[usize] {
        if self.use_fixed_colourmap != 0 {
            return &self.colourmap[self.use_fixed_colourmap];
        }

        let colourmap = self.colourmap_for_scale(wall_scale);
        #[cfg(not(feature = "safety_check"))]
        unsafe {
            // unchecked reduces instruction count from ~8 down to 1
            let i = self.light_scale.get_unchecked(light_level * 48 + colourmap);
            self.colourmap.get_unchecked(*i)
        }
        #[cfg(feature = "safety_check")]
        &self
            .colourmap
            .get_unchecked(self.light_scale[light_level.min(15) * 48 + colourmap])
    }

    #[inline(always)]
    pub fn flat_light_colourmap(&self, mut light_level: usize, mut scale: usize) -> &[usize] {
        if self.use_fixed_colourmap != 0 {
            #[cfg(not(feature = "safety_check"))]
            unsafe {
                return self.colourmap.get_unchecked(self.use_fixed_colourmap);
            }
            #[cfg(feature = "safety_check")]
            return &self.colourmap[self.use_fixed_colourmap];
        }

        // scale = scale >> 4;
        scale &= MAXLIGHTZ - 1;
        light_level = light_level.min(self.zlight_scale.len() - 1);

        #[cfg(not(feature = "safety_check"))]
        unsafe {
            let i = self
                .zlight_scale
                .get_unchecked(light_level)
                .get_unchecked(scale);
            self.colourmap.get_unchecked(*i)
        }
        #[cfg(feature = "safety_check")]
        &self.colourmap[self.zlight_scale[light_level][scale]]
    }

    #[inline(always)]
    pub fn get_texture(&self, num: usize) -> &WallPic {
        #[cfg(not(feature = "safety_check"))]
        unsafe {
            let num = self.wall_translation.get_unchecked(num);
            self.walls.get_unchecked(*num)
        }
        #[cfg(feature = "safety_check")]
        {
            let num = self.wall_translation[num];
            &self.walls[num]
        }
    }

    #[inline(always)]
    pub fn get_flat(&self, num: usize) -> &FlatPic {
        if num >= self.flat_translation.len() || num >= self.flats.len() {
            panic!()
        }
        #[cfg(not(feature = "safety_check"))]
        unsafe {
            let num = self.flat_translation.get_unchecked(num);
            self.flats.get_unchecked(*num)
        }
        #[cfg(feature = "safety_check")]
        {
            let num = self.flat_translation[num];
            &self.flats[num]
        }
    }

    #[inline(always)]
    pub fn wallpic_num_for_name(&self, name: &str) -> Option<usize> {
        for (i, tex) in self.walls.iter().enumerate() {
            if tex.name == name {
                return Some(i);
            }
        }
        None
    }

    #[inline(always)]
    pub fn flat_num_for_name(&self, name: &str) -> Option<usize> {
        for (i, tex) in self.flats.iter().enumerate() {
            if tex.name == name {
                return Some(i);
            }
        }
        None
    }

    /// Return a ref to the specified column of the requested texture
    #[inline(always)]
    pub fn wall_pic(&self, texture: usize) -> &WallPic {
        #[cfg(not(feature = "safety_check"))]
        unsafe {
            self.walls
                .get_unchecked(*self.wall_translation.get_unchecked(texture))
        }
        #[cfg(feature = "safety_check")]
        &self.walls[self.wall_translation[texture]]
    }

    /// Return a ref to the specified column of the requested texture
    #[inline(always)]
    pub fn wall_pic_column(&self, texture: usize, mut texture_column: usize) -> &[usize] {
        #[cfg(not(feature = "safety_check"))]
        let texture = unsafe {
            self.walls
                .get_unchecked(*self.wall_translation.get_unchecked(texture))
        };
        #[cfg(feature = "safety_check")]
        let texture = &self.walls[self.wall_translation[texture]];

        texture_column &= texture.width - 1;
        let column_start = texture_column * texture.height;
        let column_end = column_start + texture.height;

        #[cfg(not(feature = "safety_check"))]
        unsafe {
            texture.data.get_unchecked(column_start..column_end)
        }
        #[cfg(feature = "safety_check")]
        &texture.data[column_start..column_end]
    }

    #[inline(always)]
    pub fn num_textures(&self) -> usize {
        self.walls.len()
    }

    #[inline(always)]
    pub fn sprite_def(&self, sprite_num: usize) -> &SpriteDef {
        #[cfg(not(feature = "safety_check"))]
        unsafe {
            self.sprite_defs.get_unchecked(sprite_num)
        }
        #[cfg(feature = "safety_check")]
        &self.sprite_defs[sprite_num]
    }

    #[inline(always)]
    pub fn sprite_patch(&self, num: usize) -> &SpritePic {
        #[cfg(not(feature = "safety_check"))]
        unsafe {
            self.sprite_patches.get_unchecked(num)
        }
        #[cfg(feature = "safety_check")]
        &self.sprite_patches[num]
    }

    /// Get an average color sample from a texture using the colourmap.
    /// This samples multiple points from the texture data and returns
    /// the average color from the palette.
    pub fn get_texture_average_color(
        &self,
        light: usize,
        scale: f32,
        texture_num: usize,
    ) -> WadColour {
        let texture = self.get_texture(texture_num);

        // Sample points from the texture
        let mut r_sum = 0u32;
        let mut g_sum = 0u32;
        let mut b_sum = 0u32;
        let mut sample_count = 0u32;

        // Sample evenly across the texture
        let width = texture.width;
        let height = texture.height;

        // Sample every few pixels to get a good average
        let x_step = (width / 8).max(1);
        let y_step = (height / 8).max(1);

        for x in (0..width).step_by(x_step) {
            for y in (0..height).step_by(y_step) {
                #[cfg(not(feature = "safety_check"))]
                unsafe {
                    let c = texture.data.get_unchecked(x * texture.height + y);
                    let colourmap = self.vert_light_colourmap(light, scale);
                    // TODO: fix c being out of range of colourmap sometimes
                    if let Some(cm) = colourmap.get(*c as usize) {
                        if let Some(color) = self.palette().get(*cm) {
                            r_sum += color[0] as u32;
                            g_sum += color[1] as u32;
                            b_sum += color[2] as u32;
                        }
                    }
                }
                #[cfg(feature = "safety_check")]
                {
                    if let Some(column) = texture.data.get(x) {
                        if let Some(&c) = column.get(y) {
                            let colourmap = self.vert_light_colourmap(light, scale);
                            if let Some(&cm) = colourmap.get(c as usize) {
                                if let Some(color) = self.palette().get(cm) {
                                    r_sum += color[0] as u32;
                                    g_sum += color[1] as u32;
                                    b_sum += color[2] as u32;
                                }
                            }
                        }
                    }
                }
                sample_count += 1;
            }
        }

        if sample_count == 0 {
            return [0, 0, 0, 0];
        }

        // Calculate average
        [
            (r_sum / sample_count) as u8,
            (g_sum / sample_count) as u8,
            (b_sum / sample_count) as u8,
            255,
        ]
    }

    /// Get an average color sample from a flat using the colourmap.
    /// This samples multiple points from the flat data and returns
    /// the average color from the palette.
    pub fn get_flat_average_color(&self, light: usize, scale: usize, flat_num: usize) -> WadColour {
        let flat = self.get_flat(flat_num);

        // Sample points from the flat
        let mut r_sum = 0u32;
        let mut g_sum = 0u32;
        let mut b_sum = 0u32;
        let mut sample_count = 0u32;

        // Sample evenly across the 64x64 flat
        let sample_step = 8; // Sample every 8th pixel

        for x in (0..64).step_by(sample_step) {
            for y in (0..64).step_by(sample_step) {
                #[cfg(not(feature = "safety_check"))]
                unsafe {
                    let c = flat.data.get_unchecked(y * 64 + x);
                    let cm = self.flat_light_colourmap(light, scale).get_unchecked(*c);
                    let color = self.palette().get_unchecked(*cm);
                    r_sum += color[0] as u32;
                    g_sum += color[1] as u32;
                    b_sum += color[2] as u32;
                }
                #[cfg(feature = "safety_check")]
                {
                    if let Some(row) = flat.data.get(y) {
                        if let Some(&c) = row.get(x) {
                            if let Some(colourmap_row) = self.colourmap.get(1) {
                                if let Some(&cm) = colourmap_row.get(c) {
                                    if let Some(color) = self.palette().get(cm) {
                                        r_sum += color[0] as u32;
                                        g_sum += color[1] as u32;
                                        b_sum += color[2] as u32;
                                    }
                                }
                            }
                        }
                    }
                }
                sample_count += 1;
            }
        }

        if sample_count == 0 {
            return [0, 0, 0, 0];
        }

        // Calculate average
        [
            (r_sum / sample_count) as u8,
            (g_sum / sample_count) as u8,
            (b_sum / sample_count) as u8,
            255,
        ]
    }
}
