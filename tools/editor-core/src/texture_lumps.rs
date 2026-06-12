//! `TEXTURE<n>` + PNAMES lump encoding for projects with custom composite
//! textures. One lump per set: TEXTURE1, TEXTURE2, … in order.
//!
//! A PWAD's TEXTURE lump REPLACES the IWAD's, so these lumps must contain the
//! FULL texture set (everything imported from the IWAD plus edits), which is
//! exactly what a project's texture sets hold after `Project::create`.
//!
//! maptexture_t layout (little-endian): name (8 bytes), masked (i32, 0),
//! width (i16), height (i16), columndirectory (i32, obsolete 0),
//! patchcount (i16), then per patch: originx (i16), originy (i16),
//! patch (i16 PNAMES index), stepdir (i16), colormap (i16).
//! The lump is an i32 count, i32 offsets from lump start, then the records.
//! PNAMES is an i32 count followed by 8-byte names.

use std::fmt;

use wad::Lump;

use crate::dsp::TextureDef;
use crate::name8::Name8;

#[derive(Debug)]
pub enum TextureLumpError {
    FieldOutOfRange {
        texture: String,
        what: &'static str,
        value: i32,
    },
}

impl fmt::Display for TextureLumpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FieldOutOfRange {
                texture,
                what,
                value,
            } => {
                write!(f, "texture {texture}: {what} value {value} out of range")
            }
        }
    }
}

impl std::error::Error for TextureLumpError {}

/// Encode one `TEXTURE<n>` lump per set (TEXTURE1, TEXTURE2, …) plus the shared
/// PNAMES lump.
///
/// `extra_patch_names` are appended to PNAMES even when no texture references
/// them (so imported patches register), deduped against the
/// texture-referenced names.
pub fn encode_texture_lumps(
    texture_sets: &[Vec<TextureDef>],
    extra_patch_names: &[Name8],
) -> Result<(Vec<Lump>, Lump), TextureLumpError> {
    // PNAMES indices in first-use order across all sets.
    let mut pnames: Vec<[u8; 8]> = Vec::new();
    let mut patch_index = |name: [u8; 8]| -> i16 {
        if let Some(at) = pnames.iter().position(|n| *n == name) {
            return at as i16;
        }
        pnames.push(name);
        (pnames.len() - 1) as i16
    };

    let mut lumps = Vec::with_capacity(texture_sets.len());
    for (set_index, set) in texture_sets.iter().enumerate() {
        let mut records: Vec<Vec<u8>> = Vec::with_capacity(set.len());
        for tex in set {
            let mut rec = Vec::with_capacity(22 + tex.patches.len() * 10);
            rec.extend_from_slice(&tex.name.to_wad_bytes());
            rec.extend_from_slice(&0i32.to_le_bytes());
            rec.extend_from_slice(&field_i16(tex, "width", tex.width)?.to_le_bytes());
            rec.extend_from_slice(&field_i16(tex, "height", tex.height)?.to_le_bytes());
            rec.extend_from_slice(&0i32.to_le_bytes());
            rec.extend_from_slice(
                &field_i16(tex, "patch count", tex.patches.len() as i32)?.to_le_bytes(),
            );
            for patch in &tex.patches {
                rec.extend_from_slice(&field_i16(tex, "origin x", patch.origin_x)?.to_le_bytes());
                rec.extend_from_slice(&field_i16(tex, "origin y", patch.origin_y)?.to_le_bytes());
                rec.extend_from_slice(&patch_index(patch.patch.to_wad_bytes()).to_le_bytes());
                rec.extend_from_slice(&field_i16(tex, "stepdir", patch.step_dir)?.to_le_bytes());
                rec.extend_from_slice(&field_i16(tex, "colormap", patch.colormap)?.to_le_bytes());
            }
            records.push(rec);
        }

        let header = 4 + 4 * records.len();
        let total: usize = header + records.iter().map(Vec::len).sum::<usize>();
        let mut data = Vec::with_capacity(total);
        data.extend_from_slice(&(records.len() as i32).to_le_bytes());
        let mut offset = header as i32;
        for rec in &records {
            data.extend_from_slice(&offset.to_le_bytes());
            offset += rec.len() as i32;
        }
        for rec in &records {
            data.extend_from_slice(rec);
        }
        lumps.push(Lump {
            name: format!("TEXTURE{}", set_index + 1),
            data,
        });
    }

    // Register imported patches so they appear in PNAMES even unreferenced.
    for name in extra_patch_names {
        patch_index(name.to_wad_bytes());
    }

    let mut pnames_data = Vec::with_capacity(4 + pnames.len() * 8);
    pnames_data.extend_from_slice(&(pnames.len() as i32).to_le_bytes());
    for name in &pnames {
        pnames_data.extend_from_slice(name);
    }
    let pnames_lump = Lump {
        name: "PNAMES".to_owned(),
        data: pnames_data,
    };
    Ok((lumps, pnames_lump))
}

fn field_i16(tex: &TextureDef, what: &'static str, value: i32) -> Result<i16, TextureLumpError> {
    i16::try_from(value).map_err(|_| TextureLumpError::FieldOutOfRange {
        texture: tex.name.as_str().to_owned(),
        what,
        value,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::PatchPlacement;
    use crate::name8::Name8;

    fn def() -> TextureDef {
        TextureDef {
            name: Name8::new("STARTAN3").expect("valid"),
            width: 128,
            height: 64,
            patches: vec![PatchPlacement {
                origin_x: 0,
                origin_y: 16,
                patch: Name8::new("SW17_4").expect("valid"),
                step_dir: 1,
                colormap: 0,
            }],
        }
    }

    #[test]
    fn one_patch_texture_golden_bytes() {
        let (lumps, pnames) = encode_texture_lumps(&[vec![def()]], &[]).expect("encodes");
        assert_eq!(lumps.len(), 1);
        assert_eq!(lumps[0].name, "TEXTURE1");
        let d = &lumps[0].data;
        assert_eq!(i32::from_le_bytes(d[0..4].try_into().expect("4")), 1);
        assert_eq!(i32::from_le_bytes(d[4..8].try_into().expect("4")), 8);
        assert_eq!(&d[8..16], b"STARTAN3");
        // masked 0, width 128, height 64
        assert_eq!(i16::from_le_bytes(d[20..22].try_into().expect("2")), 128);
        assert_eq!(i16::from_le_bytes(d[22..24].try_into().expect("2")), 64);
        // patchcount 1 after the obsolete dword
        assert_eq!(i16::from_le_bytes(d[28..30].try_into().expect("2")), 1);
        // patch record: x 0, y 16, pnames idx 0, stepdir 1, colormap 0
        assert_eq!(i16::from_le_bytes(d[32..34].try_into().expect("2")), 16);
        assert_eq!(i16::from_le_bytes(d[34..36].try_into().expect("2")), 0);

        assert_eq!(pnames.name, "PNAMES");
        assert_eq!(
            i32::from_le_bytes(pnames.data[0..4].try_into().expect("4")),
            1
        );
        assert_eq!(&pnames.data[4..12], b"SW17_4\0\0");
    }

    #[test]
    fn shared_patches_dedupe_in_pnames() {
        let mut second = def();
        second.name = Name8::new("OTHER").expect("valid");
        let (_, pnames) = encode_texture_lumps(&[vec![def(), second]], &[]).expect("encodes");
        assert_eq!(
            i32::from_le_bytes(pnames.data[0..4].try_into().expect("4")),
            1
        );
    }

    #[test]
    fn n_sets_encode_to_numbered_lumps() {
        let sets = vec![vec![def()], vec![def()], vec![def()]];
        let (lumps, _) = encode_texture_lumps(&sets, &[]).expect("encodes N sets");
        let names: Vec<&str> = lumps.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["TEXTURE1", "TEXTURE2", "TEXTURE3"]);
    }

    #[test]
    fn extra_names_appear_in_pnames() {
        let extra = [Name8::new("EXTRAPCH").expect("valid")];
        let (_, pnames) = encode_texture_lumps(&[vec![def()]], &extra).expect("encodes");
        assert_eq!(
            i32::from_le_bytes(pnames.data[0..4].try_into().expect("4")),
            2
        );
        assert_eq!(&pnames.data[4..12], b"SW17_4\0\0");
        assert_eq!(&pnames.data[12..20], b"EXTRAPCH");
    }

    #[test]
    fn extra_names_dedup_with_texture_refs() {
        // The extra name is already referenced by the texture's patch.
        let extra = [Name8::new("SW17_4").expect("valid")];
        let (_, pnames) = encode_texture_lumps(&[vec![def()]], &extra).expect("encodes");
        assert_eq!(
            i32::from_le_bytes(pnames.data[0..4].try_into().expect("4")),
            1
        );
    }
}
