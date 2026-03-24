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
pub use switches::Switches;
pub mod sprites;

use std::collections::HashSet;
use std::mem::{size_of, size_of_val};

use log::{debug, warn};
use wad::WadData;
use wad::types::{WadColour, WadPalette, WadPatch, WadTexture};

use self::sprites::{SpriteDef, init_spritedefs};
use game_config::GameMode;

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
}

#[derive(Debug)]
pub struct WallPic {
    pub name: String,
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

/// CRT phosphor response simulation parameters.
/// Operates in luminance space to avoid over-saturating colours.
#[derive(Debug, Clone)]
pub struct CrtGamma {
    pub brightness: f32,
    pub black_crush: f32,
    pub highlight_boost: f32,
    pub saturation: f32,
    pub enabled: bool,
}

impl Default for CrtGamma {
    fn default() -> Self {
        Self {
            brightness: 1.0,
            black_crush: 0.32,
            highlight_boost: 1.8,
            saturation: 0.9,
            enabled: true,
        }
    }
}

/// Build a luminance-only tone curve LUT that simulates CRT phosphor response.
/// Applied to each palette entry in luminance space to preserve colour ratios.
fn build_crt_tone_lut(brightness: f32, black_crush: f32, highlight_boost: f32) -> [u8; 256] {
    let mut lut = [0u8; 256];
    for i in 0..256 {
        let v = i as f32 / 255.0;
        // S-curve: crush blacks, boost highlights, slightly wash midtones.
        // Approximates CRT phosphor response vs LCD backlight.
        // black_crush controls how aggressively darks are pushed down (0.0-1.0)
        // highlight_boost lifts the upper range
        let crushed = v.powf(1.0 + black_crush); // darks get darker
        let boosted = 1.0 - (1.0 - crushed).powf(1.0 + highlight_boost); // highlights lift
        let out = (boosted * brightness).clamp(0.0, 1.0);
        lut[i] = (out * 255.0) as u8;
    }
    lut
}

/// Apply CRT tone curve to a single palette colour.
/// Works in luminance space to avoid over-saturating.
fn apply_crt_tone(color: u32, tone_lut: &[u8; 256], saturation: f32) -> u32 {
    let r = ((color >> 16) & 0xFF) as f32;
    let g = ((color >> 8) & 0xFF) as f32;
    let b = (color & 0xFF) as f32;

    // Perceptual luminance
    let lum = 0.299 * r + 0.587 * g + 0.114 * b;
    let lum_i = (lum as u8).min(255);
    let new_lum = tone_lut[lum_i as usize] as f32;

    // Scale channels by luminance ratio (preserves colour ratios)
    let scale = if lum > 0.5 {
        new_lum / lum
    } else {
        new_lum / 128.0
    };

    let mut nr = r * scale;
    let mut ng = g * scale;
    let mut nb = b * scale;

    // Desaturate slightly toward the new luminance (CRT phosphor bleed)
    nr = new_lum + (nr - new_lum) * saturation;
    ng = new_lum + (ng - new_lum) * saturation;
    nb = new_lum + (nb - new_lum) * saturation;

    let nr = (nr.clamp(0.0, 255.0)) as u32;
    let ng = (ng.clamp(0.0, 255.0)) as u32;
    let nb = (nb.clamp(0.0, 255.0)) as u32;

    0xFF000000 | (nr << 16) | (ng << 8) | nb
}

#[derive(Debug)]
pub struct PicData {
    /// Original palettes from WAD (never modified after load)
    palettes_raw: [WadPalette; PALLETE_LEN],
    /// Active palettes (tone-corrected if CRT gamma enabled)
    palettes: [WadPalette; PALLETE_LEN],
    crt_gamma: CrtGamma,
    crt_tone_lut: [u8; 256],
    // Usually 34 blocks of 256, each u8 being an index in to the palette
    colourmap: [Colourmap; COLOURMAP_LEN],
    /// Precomputed wall light colourmaps (16 light levels × 48 scales)
    lightscale_colourmap: Vec<Colourmap>,
    /// Precomputed flat light colourmaps (16 light levels × 128 distances)
    zlight_colourmap: Vec<Colourmap>,
    use_fixed_colourmap: usize,
    walls: Vec<WallPic>,
    /// Used in animations
    pub(crate) wall_translation: Vec<usize>,
    flats: Vec<FlatPic>,
    /// Used in animations
    pub(crate) flat_translation: Vec<usize>,
    /// The number flats use to signify a sky should be drawn
    sky_num: usize,
    /// The index number of the texture to use for skybox
    sky_pic: usize,
    //
    sprite_patches: Vec<SpritePic>,
    sprite_defs: Vec<SpriteDef>,
    /// 4-char sprite prefixes that were overridden by a PWAD.
    /// Voxel models should not replace these.
    pwad_sprite_overrides: std::collections::HashSet<String>,
    /// The pallette to be used. Can be set with `set_pallette()` or
    /// `set_player_palette()`, typically done on frame start to set effects
    /// like take-damage.
    use_pallette: usize,
}

impl Default for PicData {
    fn default() -> Self {
        Self {
            palettes_raw: Default::default(),
            palettes: Default::default(),
            crt_gamma: CrtGamma::default(),
            crt_tone_lut: [0; 256],
            colourmap: [[0; 256]; COLOURMAP_LEN],
            use_fixed_colourmap: Default::default(),
            walls: Default::default(),
            wall_translation: Default::default(),
            flats: Default::default(),
            flat_translation: Default::default(),
            sky_num: Default::default(),
            sky_pic: Default::default(),
            sprite_patches: Default::default(),
            sprite_defs: Default::default(),
            pwad_sprite_overrides: Default::default(),
            use_pallette: Default::default(),
            lightscale_colourmap: vec![[0usize; 256]; LIGHTMAP_LEN],
            zlight_colourmap: vec![[0usize; 256]; 16 * 128],
        }
    }
}

impl PicData {
    pub fn init(wad: &WadData, sprite_names: &[&str]) -> Self {
        Self::init_with_crt_gamma(wad, sprite_names, CrtGamma::default())
    }

    pub fn init_with_crt_gamma(wad: &WadData, sprite_names: &[&str], crt_gamma: CrtGamma) -> Self {
        print!("Init image data  [");

        let colourmap = Self::init_colourmap(wad);
        let palettes = Self::init_palette(wad);
        let light_scale = Self::init_light_scales();
        let zlight_scale = Self::init_zlight_scales();

        // Precompute lightscale_colourmap: flatten light_scale indirection
        let mut lightscale_colourmap = vec![[0usize; 256]; LIGHTMAP_LEN];
        for (i, &light_scale_idx) in light_scale.iter().enumerate() {
            lightscale_colourmap[i].clone_from_slice(&colourmap[light_scale_idx]);
        }

        // Precompute zlight_colourmap: flatten zlight_scale indirection
        let mut zlight_colourmap = vec![[0usize; 256]; 16 * 128];
        for i in 0..16 {
            for j in 0..128 {
                zlight_colourmap[i * 128 + j].clone_from_slice(&colourmap[zlight_scale[i][j]]);
            }
        }

        let (walls, sky_pic) = Self::init_wall_pics(wad);
        let wall_translation = (0..walls.len()).collect();

        let (flats, sky_num) = Self::init_flat_pics(wad);
        let flat_translation = (0..flats.len()).collect();

        let mut sprite_patches: Vec<SpritePic> = Vec::new();
        let mut seen_names = HashSet::new();
        let mut pwad_sprite_overrides = HashSet::new();
        for (i, patch) in wad.sprites_iter().enumerate() {
            if i % 64 == 0 {
                print!(".");
            }

            // PWAD sprites are iterated first. Skip IWAD duplicates so
            // PWAD replacements take priority.
            if !seen_names.insert(patch.name.clone()) {
                // This name was already seen (from PWAD) — record the
                // 4-char prefix as a PWAD override so voxels don't replace it.
                if patch.name.len() >= 4 {
                    pwad_sprite_overrides.insert(patch.name[..4].to_string());
                }
                continue;
            }

            let mut x_pos = 0;
            let mut compose = vec![vec![usize::MAX; patch.height as usize]; patch.width as usize];
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

            sprite_patches.push(SpritePic {
                name: patch.name,
                top_offset: patch.top_offset as i32,
                left_offset: patch.left_offset as i32,
                data: compose,
            });
        }
        let sprite_defs = init_spritedefs(sprite_names, &sprite_patches);

        println!(".]");

        let crt_tone_lut = build_crt_tone_lut(
            crt_gamma.brightness,
            crt_gamma.black_crush,
            crt_gamma.highlight_boost,
        );

        let mut s = Self {
            walls,
            wall_translation,
            sky_num,
            sky_pic,
            flats,
            flat_translation,
            palettes_raw: palettes,
            palettes,
            crt_gamma,
            crt_tone_lut,
            lightscale_colourmap,
            zlight_colourmap,
            colourmap,
            use_fixed_colourmap: 0,
            sprite_patches,
            sprite_defs,
            pwad_sprite_overrides,
            use_pallette: 0,
        };
        s.apply_crt_gamma();
        s
    }

    fn init_palette(wad: &WadData) -> [WadPalette; PALLETE_LEN] {
        print!(".");
        let mut tmp = [WadPalette::default(); PALLETE_LEN];
        for (i, p) in wad.lump_iter::<WadPalette>("PLAYPAL").enumerate() {
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

    /// Precompute the wall light scale LUT: maps (light level, scale) to
    /// colourmap index.
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
                tmp[i as usize * 48 + j as usize] = level as usize;
            }
        }
        tmp
    }

    /// Force a fixed colourmap for all light levels. Pass 0 to disable.
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
                if let Some(lump) = wad.get_lump(name) {
                    sorted_patches.push(WadPatch::from_lump(lump));
                } else {
                    warn!("Mising: {name}");
                }
            }
        }
        print!(".");
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
        let mut flats = Vec::with_capacity(wad.flats_iter().count());
        let mut seen_names = HashSet::new();
        print!(".");

        let mut flat_alloc_size = 0;
        for (i, wf) in wad.flats_iter().enumerate() {
            // PWAD flats are iterated first. Skip IWAD duplicates so
            // PWAD replacements take priority.
            if !seen_names.insert(wf.name.clone()) {
                continue;
            }

            let mut flat = FlatPic {
                name: wf.name.clone(),
                data: [0; 64 * 64],
                width: 64,
                height: 64,
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

            flats.push(flat);
        }

        debug!(
            "Total memory used for flats: {} KiB",
            flat_alloc_size / 1024
        );

        (flats, skynum)
    }

    fn build_wall_pic(texture: WadTexture, patches: &[WadPatch]) -> WallPic {
        let mut compose = vec![usize::MAX; texture.height as usize * texture.width as usize];
        for wad_tex_patch in texture.patches.iter() {
            let wad_patch = &patches[wad_tex_patch.patch_index];
            let mut x_pos = wad_tex_patch.origin_x;
            if x_pos.is_negative() {
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
        WallPic {
            name: texture.name,
            width: texture.width as usize,
            height: texture.height as usize,
            data: compose,
        }
    }

    #[inline(always)]
    pub const fn palette(&self) -> &[WadColour] {
        &self.palettes[self.use_pallette].0
    }

    #[inline(always)]
    pub const fn wad_palette(&self) -> &WadPalette {
        &self.palettes[self.use_pallette]
    }

    #[inline(always)]
    pub fn set_palette(&mut self, num: usize) {
        self.use_pallette = num.min(self.palettes.len() - 1);
    }

    /// Enable or disable CRT phosphor tone correction.
    /// Rebuilds LUT and re-applies to all palettes.
    pub fn set_crt_gamma(&mut self, enabled: bool) {
        self.crt_gamma.enabled = enabled;
        self.crt_tone_lut = build_crt_tone_lut(
            self.crt_gamma.brightness,
            self.crt_gamma.black_crush,
            self.crt_gamma.highlight_boost,
        );
        self.apply_crt_gamma();
    }

    fn apply_crt_gamma(&mut self) {
        self.palettes = self.palettes_raw;
        if !self.crt_gamma.enabled {
            return;
        }
        let lut = self.crt_tone_lut;
        let sat = self.crt_gamma.saturation;
        for palette in &mut self.palettes {
            for color in &mut palette.0 {
                *color = apply_crt_tone(*color, &lut, sat);
            }
        }
    }

    /// Set palette based on player damage/bonus/power state.
    /// Arguments are extracted from Player to avoid depending on gameplay
    /// types.
    pub fn set_player_palette(
        &mut self,
        damagecount: i32,
        bonuscount: i32,
        strength_power: i32,
        ironfeet_power: i32,
    ) {
        let mut damagecount = damagecount;

        if strength_power != 0 {
            // slowly fade the berzerk out
            let berkers = 12 - (strength_power >> 6);
            if berkers > damagecount {
                damagecount = berkers;
            }
        }

        if damagecount != 0 {
            self.use_pallette = ((damagecount + 7) >> 3) as usize;
            self.use_pallette = self.use_pallette.min(NUMREDPALS - 1);
            self.use_pallette += STARTREDPALS;
        } else if bonuscount != 0 {
            self.use_pallette = ((bonuscount + 7) >> 3) as usize;
            self.use_pallette = self.use_pallette.min(NUMBONUSPALS - 1);
            self.use_pallette += STARTBONUSPALS;
        } else if ironfeet_power > 4 * 32 || ironfeet_power & 8 != 0 {
            self.use_pallette = RADIATIONPAL;
        } else {
            self.use_pallette = 0;
        }

        if self.use_pallette >= self.palettes.len() {
            self.use_pallette = self.palettes.len() - 1;
        }
    }

    #[inline(always)]
    pub const fn sky_num(&self) -> usize {
        self.sky_num
    }

    #[inline(always)]
    pub const fn sky_pic(&self) -> usize {
        self.sky_pic
    }

    pub fn pwad_sprite_overrides(&self) -> &std::collections::HashSet<String> {
        &self.pwad_sprite_overrides
    }

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

    pub fn set_sky_pic_by_name(&mut self, name: &str) {
        if let Some(idx) = self.wallpic_num_for_name(name) {
            self.sky_pic = idx;
        } else {
            log::warn!("UMAPINFO sky texture '{}' not found, keeping default", name);
        }
    }

    #[inline(always)]
    pub fn colourmap(&self, index: usize) -> &[usize] {
        &self.colourmap[index]
    }

    #[inline(always)]
    pub fn base_colourmap(&self, light_level: usize, wall_scale: f32) -> &[usize] {
        if self.use_fixed_colourmap != 0 {
            return &self.colourmap[self.use_fixed_colourmap];
        }
        let colourmap = (wall_scale as u32).min(47) as usize;
        unsafe {
            self.lightscale_colourmap
                .get_unchecked(light_level * 48 + colourmap)
        }
    }

    #[inline(always)]
    pub fn vert_light_colourmap(&self, light_level: usize, wall_scale: f32) -> &[usize] {
        if self.use_fixed_colourmap != 0 {
            return &self.colourmap[self.use_fixed_colourmap];
        }

        let colourmap = ((wall_scale * 15.8) as u32).min(MAXLIGHTSCALE as u32 - 1) as usize;
        unsafe {
            self.lightscale_colourmap
                .get_unchecked(light_level * 48 + colourmap)
        }
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

        scale &= MAXLIGHTZ - 1;
        light_level = light_level.min(15);

        unsafe {
            self.zlight_colourmap
                .get_unchecked(light_level * 128 + scale)
        }
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

    pub fn get_texture_average_color(
        &self,
        light: usize,
        scale: f32,
        texture_num: usize,
    ) -> WadColour {
        let texture = self.get_texture(texture_num);
        let mut r_sum = 0u32;
        let mut g_sum = 0u32;
        let mut b_sum = 0u32;
        let mut sample_count = 0u32;
        let width = texture.width;
        let height = texture.height;
        let x_step = (width / 8).max(1);
        let y_step = (height / 8).max(1);

        for x in (0..width).step_by(x_step) {
            for y in (0..height).step_by(y_step) {
                #[cfg(not(feature = "safety_check"))]
                unsafe {
                    let c = texture.data.get_unchecked(x * texture.height + y);
                    let colourmap = self.vert_light_colourmap(light, scale);
                    if let Some(cm) = colourmap.get(*c as usize) {
                        if let Some(&color) = self.palette().get(*cm) {
                            r_sum += (color >> 16) & 0xFF;
                            g_sum += (color >> 8) & 0xFF;
                            b_sum += color & 0xFF;
                        }
                    }
                }
                #[cfg(feature = "safety_check")]
                {
                    if let Some(column) = texture.data.get(x) {
                        if let Some(&c) = column.get(y) {
                            let colourmap = self.vert_light_colourmap(light, scale);
                            if let Some(&cm) = colourmap.get(c as usize) {
                                if let Some(&color) = self.palette().get(cm) {
                                    r_sum += (color >> 16) & 0xFF;
                                    g_sum += (color >> 8) & 0xFF;
                                    b_sum += color & 0xFF;
                                }
                            }
                        }
                    }
                }
                sample_count += 1;
            }
        }

        if sample_count == 0 {
            return 0;
        }

        ((r_sum / sample_count) << 16) | ((g_sum / sample_count) << 8) | (b_sum / sample_count)
    }

    pub fn get_flat_average_color(&self, light: usize, scale: usize, flat_num: usize) -> WadColour {
        let flat = self.get_flat(flat_num);
        let mut r_sum = 0u32;
        let mut g_sum = 0u32;
        let mut b_sum = 0u32;
        let mut sample_count = 0u32;
        let sample_step = 8;

        for x in (0..64).step_by(sample_step) {
            for y in (0..64).step_by(sample_step) {
                #[cfg(not(feature = "safety_check"))]
                unsafe {
                    let c = flat.data.get_unchecked(y * 64 + x);
                    let cm = self.flat_light_colourmap(light, scale).get_unchecked(*c);
                    let color = *self.palette().get_unchecked(*cm);
                    r_sum += (color >> 16) & 0xFF;
                    g_sum += (color >> 8) & 0xFF;
                    b_sum += color & 0xFF;
                }
                #[cfg(feature = "safety_check")]
                {
                    if let Some(row) = flat.data.get(y) {
                        if let Some(&c) = row.get(x) {
                            if let Some(colourmap_row) = self.colourmap.get(1) {
                                if let Some(&cm) = colourmap_row.get(c) {
                                    if let Some(&color) = self.palette().get(cm) {
                                        r_sum += (color >> 16) & 0xFF;
                                        g_sum += (color >> 8) & 0xFF;
                                        b_sum += color & 0xFF;
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
            return 0;
        }

        ((r_sum / sample_count) << 16) | ((g_sum / sample_count) << 8) | (b_sum / sample_count)
    }
}
