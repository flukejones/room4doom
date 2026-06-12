//! Doom picture codec and texture composer — pure palette-index data, no UI; `gfx` builds Slint images on top, no Slint dep here avoids a module cycle.

use editor_core::{ImportedPatch, TextureDef};
use wad::WadData;

use super::WallPic;

/// `u16::MAX` — no post covers this texel.
pub const TRANSPARENT_INDEX: u16 = u16::MAX;
/// Texel covered by a missing patch lump; distinct from [`TRANSPARENT_INDEX`] so the atlas packer shows magenta instead of see-through, out of palette range.
pub const MISSING_PATCH_INDEX: u16 = u16::MAX - 1;
/// Opaque magenta for [`MISSING_PATCH_INDEX`] — shared by atlas packer and preview.
pub const MISSING_PATCH_RGBA: [u8; 4] = [0xff, 0x00, 0xff, 0xff];
/// `topdelta` is a u8 and vanilla engines stop posts at 128.
const MAX_POST_LEN: usize = 128;
/// `topdelta` is a u8; 0xFF ends the column, leaving 254 addressable rows.
pub const MAX_PATCH_HEIGHT: usize = 254;

/// Decoded Doom picture (patch lump).
pub struct PatchImage {
    pub width: usize,
    pub height: usize,
    /// Row-major palette indices; [`TRANSPARENT_INDEX`] where no post covers the texel.
    pub data: Vec<u16>,
}

/// Decode Doom picture format: `{width, height, offsets i16}`, u32 column offsets, per-column posts `{topdelta u8 (0xFF=end), length u8, pad, pixels, pad}`.
pub fn decode_patch(lump: &[u8]) -> Option<PatchImage> {
    let rd16 = |at: usize| -> Option<usize> {
        Some(i16::from_le_bytes([*lump.get(at)?, *lump.get(at + 1)?]) as usize)
    };
    let width = rd16(0)?;
    let height = rd16(2)?;
    if width == 0 || height == 0 || width > 4096 || height > 4096 {
        return None;
    }
    let mut data = vec![TRANSPARENT_INDEX; width * height];
    for x in 0..width {
        let off_at = 8 + x * 4;
        let col_start = u32::from_le_bytes([
            *lump.get(off_at)?,
            *lump.get(off_at + 1)?,
            *lump.get(off_at + 2)?,
            *lump.get(off_at + 3)?,
        ]) as usize;
        let mut at = col_start;
        loop {
            let topdelta = *lump.get(at)? as usize;
            if topdelta == 0xff {
                break;
            }
            let length = *lump.get(at + 1)? as usize;
            for i in 0..length {
                let y = topdelta + i;
                if y < height {
                    data[y * width + x] = u16::from(*lump.get(at + 3 + i)?);
                }
            }
            at += 4 + length;
        }
    }
    Some(PatchImage {
        width,
        height,
        data,
    })
}

/// Encode row-major palette indices as a Doom picture lump; heights over [`MAX_PATCH_HEIGHT`] are capped, tall-patch not implemented.
pub fn encode_patch(width: usize, height: usize, data: &[u16]) -> Vec<u8> {
    let height = if height > MAX_PATCH_HEIGHT {
        log::warn!("patch height {height} exceeds {MAX_PATCH_HEIGHT}; capping");
        MAX_PATCH_HEIGHT
    } else {
        height
    };

    let mut columns: Vec<Vec<u8>> = Vec::with_capacity(width);
    for x in 0..width {
        let mut col = Vec::new();
        let mut y = 0;
        while y < height {
            if data[y * width + x] == TRANSPARENT_INDEX {
                y += 1;
                continue;
            }
            let top = y;
            while y < height && data[y * width + x] != TRANSPARENT_INDEX {
                y += 1;
            }
            let mut run = top;
            while run < y {
                let len = (y - run).min(MAX_POST_LEN);
                col.push(run as u8);
                col.push(len as u8);
                col.push(0); // pad
                for row in run..run + len {
                    col.push(data[row * width + x] as u8);
                }
                col.push(0); // pad
                run += len;
            }
        }
        col.push(0xFF); // end-of-column
        columns.push(col);
    }

    let table_start = 8 + width * 4;
    let total = table_start + columns.iter().map(Vec::len).sum::<usize>();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&(width as i16).to_le_bytes());
    out.extend_from_slice(&(height as i16).to_le_bytes());
    out.extend_from_slice(&0i16.to_le_bytes()); // left origin
    out.extend_from_slice(&0i16.to_le_bytes()); // top origin
    let mut offset = table_start as u32;
    for col in &columns {
        out.extend_from_slice(&offset.to_le_bytes());
        offset += col.len() as u32;
    }
    for col in &columns {
        out.extend_from_slice(col);
    }
    out
}

/// Resolve a patch lump; imported patches shadow WAD lumps.
pub fn resolve_patch_lump<'a>(
    name: &str,
    imported: &'a [ImportedPatch],
    wad: &'a WadData,
) -> Option<&'a [u8]> {
    if let Some(patch) = imported
        .iter()
        .find(|p| p.name.as_str().eq_ignore_ascii_case(name))
    {
        return Some(&patch.lump);
    }
    wad.get_lump(name).map(|l| l.data.as_slice())
}

/// Decoded dims of a patch lump (imported before WAD).
pub fn patch_dims(imported: &[ImportedPatch], wad: &WadData, name: &str) -> Option<(usize, usize)> {
    let bytes = resolve_patch_lump(name, imported, wad)?;
    let patch = decode_patch(bytes)?;
    Some((patch.width, patch.height))
}

/// Compose a texture to palette indices, column-major (`data[x*h+y]`); cached by [`super::EditorAssets::ensure_composed`]. If any patch lump is missing (dims unknowable), all uncovered texels become [`MISSING_PATCH_INDEX`] (magenta) instead of [`TRANSPARENT_INDEX`] — present patches draw normally, genuine transparency only survives if no patch is missing.
pub fn compose_texture_indices(
    def: &TextureDef,
    imported: &[ImportedPatch],
    wad: &WadData,
) -> WallPic {
    let (w, h) = (def.width.max(1) as usize, def.height.max(1) as usize);
    let mut data = vec![TRANSPARENT_INDEX; w * h];
    let mut any_missing = false;
    for placement in &def.patches {
        let patch =
            resolve_patch_lump(placement.patch.as_str(), imported, wad).and_then(decode_patch);
        let Some(patch) = patch else {
            any_missing = true;
            continue;
        };
        for py in 0..patch.height {
            let ty = placement.origin_y + py as i32;
            if ty < 0 || ty as usize >= h {
                continue;
            }
            for px in 0..patch.width {
                let tx = placement.origin_x + px as i32;
                if tx < 0 || tx as usize >= w {
                    continue;
                }
                let index = patch.data[py * patch.width + px];
                if index != TRANSPARENT_INDEX {
                    data[tx as usize * h + ty as usize] = index;
                }
            }
        }
    }
    if any_missing {
        for texel in &mut data {
            if *texel == TRANSPARENT_INDEX {
                *texel = MISSING_PATCH_INDEX;
            }
        }
    }
    WallPic {
        data,
        width: w,
        height: h,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use editor_core::{Name8, PatchPlacement};

    #[test]
    fn encode_patch_round_trips_with_gap() {
        let t = TRANSPARENT_INDEX;
        // 2 wide, 4 tall, row-major. Column 0 has a gap at row 2.
        let data = vec![
            1, 5, // row 0
            1, 5, // row 1
            t, 5, // row 2 (col 0 transparent → gap)
            2, 5, // row 3
        ];
        let lump = encode_patch(2, 4, &data);
        let patch = decode_patch(&lump).expect("decodes");
        assert_eq!((patch.width, patch.height), (2, 4));
        assert_eq!(patch.data, data);
    }

    #[test]
    fn encode_patch_splits_long_run() {
        let height = 200;
        let data = vec![7u16; height]; // 1 wide, fully opaque
        let lump = encode_patch(1, height, &data);
        let patch = decode_patch(&lump).expect("decodes");
        assert_eq!(patch.height, height);
        assert!(patch.data.iter().all(|&i| i == 7));
    }

    #[test]
    fn encode_patch_caps_tall_height() {
        let data = vec![3u16; 260];
        let lump = encode_patch(1, 260, &data);
        let patch = decode_patch(&lump).expect("decodes");
        assert_eq!(patch.height, MAX_PATCH_HEIGHT);
    }

    #[test]
    fn missing_patch_fills_uncovered_texels_with_sentinel() {
        let present = encode_patch(2, 2, &[1, 1, 1, 1]);
        let imported = vec![ImportedPatch {
            name: Name8::new("HAVE").expect("valid"),
            lump: present,
        }];
        let placement = |patch: &str, x, y| PatchPlacement {
            patch: Name8::new(patch).expect("valid"),
            origin_x: x,
            origin_y: y,
            step_dir: 0,
            colormap: 0,
        };
        let def = TextureDef {
            name: Name8::new("BROKEN").expect("valid"),
            width: 4,
            height: 4,
            patches: vec![placement("HAVE", 0, 0), placement("GONE", 2, 0)],
        };
        let wad = WadData::new(&test_utils::doom1_wad_path());
        let pic = compose_texture_indices(&def, &imported, &wad);

        // data is column-major: data[x * h + y].
        for x in 0..2usize {
            for y in 0..2usize {
                assert_eq!(pic.data[x * 4 + y], 1, "present patch texel ({x},{y})");
            }
        }
        assert_eq!(pic.data[2 * 4], MISSING_PATCH_INDEX, "uncovered → sentinel");
        assert!(
            pic.data.contains(&MISSING_PATCH_INDEX),
            "a missing patch yields sentinel texels"
        );
        assert!(
            !pic.data.contains(&TRANSPARENT_INDEX),
            "no texel stays transparent once a patch is missing"
        );
    }

    #[test]
    fn all_patches_present_never_marks_missing() {
        let lump = encode_patch(2, 2, &[2, 2, 2, TRANSPARENT_INDEX]);
        let imported = vec![ImportedPatch {
            name: Name8::new("HAVE").expect("valid"),
            lump,
        }];
        let def = TextureDef {
            name: Name8::new("OK").expect("valid"),
            width: 2,
            height: 2,
            patches: vec![PatchPlacement {
                patch: Name8::new("HAVE").expect("valid"),
                origin_x: 0,
                origin_y: 0,
                step_dir: 0,
                colormap: 0,
            }],
        };
        let wad = WadData::new(&test_utils::doom1_wad_path());
        let pic = compose_texture_indices(&def, &imported, &wad);
        assert!(
            !pic.data.contains(&MISSING_PATCH_INDEX),
            "no missing patch → no sentinel; genuine transparency survives"
        );
        assert!(
            pic.data.contains(&TRANSPARENT_INDEX),
            "the genuine transparent corner is preserved"
        );
    }

    #[test]
    fn resolve_patch_lump_prefers_imported() {
        let wad = WadData::new(&test_utils::doom1_wad_path());
        let imported = vec![ImportedPatch {
            name: Name8::new("TITLEPIC").expect("valid"),
            lump: vec![0xde, 0xad, 0xbe, 0xef],
        }];
        let bytes = resolve_patch_lump("TITLEPIC", &imported, &wad).expect("found");
        assert_eq!(bytes, &[0xde, 0xad, 0xbe, 0xef]);
        assert!(resolve_patch_lump("PLAYPAL", &[], &wad).is_some());
    }
}
