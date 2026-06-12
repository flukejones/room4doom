//! Lazily memoised asset thumbnails over [`EditorAssets`].
//! `WallPic`: column-major. `FlatPic`: row-major 64×64. `u16::MAX` = transparent.

pub mod palette;
pub mod patch;
pub mod sprite;
pub mod wall;

pub use patch::compose_texture_highlight;
pub use sprite::{SpriteSource, ensure_thing_sprite, sprite_present};
pub use wall::{FLAT_SIDE, WallBand, render_flat_square, render_wall};

use std::collections::HashMap;

use slint::{Rgba8Pixel, SharedPixelBuffer};
use wad::WadData;
use wad::types::WadPalette;

use crate::assets::palette::wad_color_to_rgba;
pub(super) use crate::assets::texture_compose::{
    MISSING_PATCH_INDEX, MISSING_PATCH_RGBA, TRANSPARENT_INDEX,
};
use crate::assets::{AssetGen, EditorAssets, decode_patch, resolve_patch_lump};

/// Write one palette-indexed texel as RGBA8. [`TRANSPARENT_INDEX`] → clear; [`MISSING_PATCH_INDEX`] → magenta.
pub(super) fn put_palette_color(out: &mut [u8], palette: &WadPalette, index: u16) {
    if index == TRANSPARENT_INDEX {
        out.copy_from_slice(&[0, 0, 0, 0]);
    } else if index == MISSING_PATCH_INDEX {
        out.copy_from_slice(&MISSING_PATCH_RGBA);
    } else {
        out.copy_from_slice(&wad_color_to_rgba(palette.0[index as usize & 0xff]));
    }
}

/// Memoised thumbnails over [`EditorAssets`].
pub struct GfxCache {
    tex_thumbs: Vec<Option<slint::Image>>,
    flat_thumbs: Vec<Option<slint::Image>>,
    patch_names: Vec<String>,
    patch_decoded: HashMap<String, Option<(slint::Image, u32, u32)>>,
    built_gen: AssetGen,
}

impl GfxCache {
    pub fn new(assets: &EditorAssets) -> Self {
        Self {
            tex_thumbs: vec![None; assets.textures().len()],
            flat_thumbs: vec![None; assets.iwad_flats().len()],
            patch_names: Vec::new(),
            patch_decoded: HashMap::new(),
            built_gen: assets.generation(),
        }
    }

    fn sync(&mut self, assets: &EditorAssets) {
        let now = assets.generation();
        if now.palette != self.built_gen.palette || now.textures != self.built_gen.textures {
            self.tex_thumbs.iter_mut().for_each(|t| *t = None);
        }
        if now.palette != self.built_gen.palette {
            self.flat_thumbs.iter_mut().for_each(|f| *f = None);
        }
        if now.patches != self.built_gen.patches || now.palette != self.built_gen.palette {
            self.patch_decoded.clear();
        }
        self.built_gen = now;
    }

    /// PNAMES patch names; loaded once on first call.
    pub fn patch_names(&mut self, wad: &WadData) -> &[String] {
        if self.patch_names.is_empty() {
            self.patch_names = wad.pnames_iter().collect();
        }
        &self.patch_names
    }

    /// Decoded patch image (imported before WAD); `None` if missing or undecodable.
    pub fn patch_image(
        &mut self,
        assets: &EditorAssets,
        wad: &WadData,
        name: &str,
    ) -> Option<slint::Image> {
        self.ensure_patch(assets, wad, name)
            .as_ref()
            .map(|(img, _, _)| img.clone())
    }

    /// Decoded dims of a patch.
    pub fn patch_size(
        &mut self,
        assets: &EditorAssets,
        wad: &WadData,
        name: &str,
    ) -> Option<(u32, u32)> {
        self.ensure_patch(assets, wad, name).map(|(_, w, h)| (w, h))
    }

    fn ensure_patch(
        &mut self,
        assets: &EditorAssets,
        wad: &WadData,
        name: &str,
    ) -> Option<(slint::Image, u32, u32)> {
        self.sync(assets);
        if !self.patch_decoded.contains_key(name) {
            let palette = *assets.palette();
            let decoded = resolve_patch_lump(name, assets.imported_patches(), wad)
                .and_then(decode_patch)
                .map(|patch| {
                    let mut buf = SharedPixelBuffer::<Rgba8Pixel>::new(
                        patch.width as u32,
                        patch.height as u32,
                    );
                    let bytes = buf.make_mut_bytes();
                    for (i, &index) in patch.data.iter().enumerate() {
                        put_palette_color(&mut bytes[i * 4..i * 4 + 4], &palette, index);
                    }
                    (
                        slint::Image::from_rgba8(buf),
                        patch.width as u32,
                        patch.height as u32,
                    )
                });
            self.patch_decoded.insert(name.to_owned(), decoded);
        }
        self.patch_decoded.get(name).cloned().flatten()
    }

    /// Composed thumbnail for the `num`th texture (memoised).
    pub fn texture_image(
        &mut self,
        assets: &EditorAssets,
        wad: &WadData,
        num: usize,
    ) -> slint::Image {
        self.sync(assets);
        if self.tex_thumbs[num].is_none() {
            let def = &assets.textures()[num];
            let buf = compose_texture_highlight(
                def,
                assets.imported_patches(),
                wad,
                assets.palette(),
                None,
                None,
            );
            self.tex_thumbs[num] = Some(slint::Image::from_rgba8(buf));
        }
        self.tex_thumbs[num]
            .clone()
            .expect("filled by the branch above")
    }

    /// Rendered thumbnail for the `num`th flat (memoised).
    pub fn flat_image(&mut self, assets: &EditorAssets, num: usize) -> slint::Image {
        self.sync(assets);
        if self.flat_thumbs[num].is_none() {
            let buf = palette::flat_to_rgba(&assets.iwad_flats()[num].flat, assets.palette());
            self.flat_thumbs[num] = Some(slint::Image::from_rgba8(buf));
        }
        self.flat_thumbs[num]
            .clone()
            .expect("filled by the branch above")
    }
}

#[cfg(test)]
mod tests;
