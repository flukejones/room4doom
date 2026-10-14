//! PNG import into the shared editable asset set.

use std::{fs::File, path::Path};

use editor_core::{ImportedPatch, Name8};
use gpui::{Context, PathPromptOptions};

use super::TextureEditor;
use crate::assets::{encode_patch, palette::nearest_palette_index};

impl TextureEditor {
    pub(super) fn import_patch(&self, cx: &Context<Self>) {
        let generation = self.shared.borrow().resource_generation;
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Import PNG patch".into()),
        });
        cx.spawn(async move |tab, cx| {
            let Ok(Ok(Some(paths))) = picker.await else {
                return;
            };
            let Some(path) = paths.first() else { return };
            tab.update(cx, |this, cx| {
                if this.shared.borrow().resource_generation != generation {
                    return;
                }
                match this.import_png(path) {
                    Ok(status) => this.status = status,
                    Err(status) => this.status = status.into(),
                }
                this.refresh_preview(cx);
            })
            .ok();
        })
        .detach();
    }

    fn import_png(&self, path: &Path) -> Result<String, &'static str> {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or("PNG name is not valid text")?;
        let name = Name8::new(stem).map_err(|_| "PNG name is not a valid 8-char patch name")?;
        let (width, height, rgba) = decode_png_rgba8(path).map_err(|error| {
            log::warn!("PNG import {}: {error}", path.display());
            "Could not decode PNG"
        })?;
        let mut state = self.shared.borrow_mut();
        if !state.ensure_assets() {
            return Err("No WAD palette to quantize against");
        }
        if state
            .wad_data
            .as_ref()
            .and_then(|wad| wad.get_lump(name.as_str()))
            .is_some()
        {
            log::warn!("Imported patch {} shadows an IWAD lump", name.as_str());
        }
        let assets = state
            .assets
            .as_mut()
            .ok_or("No WAD loaded to import against")?;
        let indices = quantize_to_indices(&rgba, assets.palette());
        assets.import_patch(ImportedPatch {
            name,
            lump: encode_patch(width, height, &indices),
        })?;
        state.assets_dirty = true;
        Ok(format!("Imported {} ({width}x{height})", name.as_str()))
    }
}

fn decode_png_rgba8(path: &Path) -> Result<(usize, usize, Vec<u8>), png::DecodingError> {
    let mut decoder = png::Decoder::new(File::open(path)?);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info()?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf)?;
    let (width, height) = (info.width as usize, info.height as usize);
    let channels = info.color_type.samples();
    buf.truncate(info.buffer_size());
    let mut rgba = vec![0u8; width * height * 4];
    for (pixel, src) in buf.chunks_exact(channels).enumerate() {
        let at = pixel * 4;
        match info.color_type {
            png::ColorType::Grayscale | png::ColorType::Indexed => {
                rgba[at..at + 3].fill(src[0]);
                rgba[at + 3] = 0xff;
            }
            png::ColorType::GrayscaleAlpha => {
                rgba[at..at + 3].fill(src[0]);
                rgba[at + 3] = src[1];
            }
            png::ColorType::Rgb => {
                rgba[at..at + 3].copy_from_slice(&src[..3]);
                rgba[at + 3] = 0xff;
            }
            png::ColorType::Rgba => rgba[at..at + 4].copy_from_slice(&src[..4]),
        }
    }
    Ok((width, height, rgba))
}

fn quantize_to_indices(rgba: &[u8], palette: &wad::types::WadPalette) -> Vec<u16> {
    rgba.as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| {
            if pixel[3] == 0 {
                return u16::MAX;
            }
            let colour = 0xff00_0000
                | (u32::from(pixel[0]) << 16)
                | (u32::from(pixel[1]) << 8)
                | u32::from(pixel[2]);
            u16::from(nearest_palette_index(colour, &palette.0))
        })
        .collect()
}
