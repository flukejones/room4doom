//! Thing-icon sprite decode into [`ThingSpriteCache`].

use wad::WadData;
use wad::types::WadPalette;

use super::put_palette_color;
use crate::assets::decode_patch;
use crate::render::sprites::{SpriteRgba, ThingSpriteCache};

/// Source of a thing's icon sprite.
pub enum SpriteSource<'a> {
    /// Full patch lump name (project `things.dsp` icon).
    Patch(&'a str),
    /// 4-char sprite prefix; frame `A` is used.
    Prefix(&'a str),
    /// No sprite; caller draws a colour square.
    None,
}

/// Decode icon for `kind` into [`ThingSpriteCache`] (idempotent).
pub fn ensure_thing_sprite(
    cache: &mut ThingSpriteCache,
    wad: &WadData,
    palette: &WadPalette,
    kind: i32,
    source: SpriteSource,
) {
    if cache.contains(kind) {
        return;
    }
    let sprite = match source {
        SpriteSource::None => None,
        SpriteSource::Patch(name) => wad
            .get_lump(name)
            .and_then(|l| sprite_from_lump(l, palette)),
        SpriteSource::Prefix(prefix) => {
            find_sprite_lump(wad, prefix).and_then(|l| sprite_from_lump(l, palette))
        }
    };
    cache.insert(kind, sprite);
}

fn sprite_from_lump(lump: &wad::Lump, palette: &WadPalette) -> Option<SpriteRgba> {
    let patch = decode_patch(&lump.data)?;
    let mut rgba = vec![0u8; patch.width * patch.height * 4];
    for (i, &index) in patch.data.iter().enumerate() {
        put_palette_color(&mut rgba[i * 4..i * 4 + 4], palette, index);
    }
    Some(SpriteRgba {
        width: patch.width as u32,
        height: patch.height as u32,
        rgba,
    })
}

/// True if the WAD has any frame-`A` lump for `prefix`.
pub fn sprite_present(wad: &WadData, prefix: &str) -> bool {
    find_sprite_lump(wad, prefix).is_some()
}

/// First frame-`A` sprite lump for `prefix` (prefers `A1`, `A0`, then any `A`).
fn find_sprite_lump<'w>(wad: &'w WadData, prefix: &str) -> Option<&'w wad::Lump> {
    let lumps = wad.lumps();
    let start = lumps.iter().position(|l| l.name == "S_START")?;
    let end = lumps.iter().position(|l| l.name == "S_END")?;
    if end <= start + 1 {
        return None;
    }
    let span = &lumps[start + 1..end];
    let frame_a = format!("{prefix}A");
    let pick = |suffix: &str| {
        let want = format!("{frame_a}{suffix}");
        span.iter().find(|l| l.name == want)
    };
    pick("1")
        .or_else(|| pick("0"))
        .or_else(|| span.iter().find(|l| l.name.starts_with(frame_a.as_str())))
}
