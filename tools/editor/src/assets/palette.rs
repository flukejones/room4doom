//! Raw `PLAYPAL`/`COLORMAP` loading and nearest-index quantiser for PNG import.

use wad::WadData;
use wad::types::WadPalette;

const COLORMAP_ENTRIES: usize = 256;
/// The 32 light-fade maps (WAD may carry extra invuln maps after; not exposed).
pub const COLORMAP_LEVELS: usize = 32;

/// Parse PLAYPAL palette 0 (256 RGB triples) into a [`WadPalette`], gamma-free.
pub fn load_palette(wad: &WadData) -> Option<WadPalette> {
    let lump = wad.get_lump("PLAYPAL")?;
    let mut pal = [wad::types::BLACK; 256];
    for (slot, rgb) in pal.iter_mut().zip(lump.data.chunks_exact(3).take(256)) {
        *slot =
            0xff00_0000 | (u32::from(rgb[0]) << 16) | (u32::from(rgb[1]) << 8) | u32::from(rgb[2]);
    }
    Some(WadPalette(pal))
}

/// Load 32 COLORMAP light-fade tables as raw index bytes.
pub fn load_colormaps(wad: &WadData) -> Vec<[u8; 256]> {
    let Some(lump) = wad.get_lump("COLORMAP") else {
        return Vec::new();
    };
    let mut maps = Vec::with_capacity(COLORMAP_LEVELS);
    for level in 0..COLORMAP_LEVELS {
        let start = level * COLORMAP_ENTRIES;
        let Some(slice) = lump.data.get(start..start + COLORMAP_ENTRIES) else {
            break;
        };
        let mut map = [0u8; 256];
        map.copy_from_slice(slice);
        maps.push(map);
    }
    maps
}

/// Opaque RGBA8 from a `0xAARRGGBB` palette entry.
pub fn wad_color_to_rgba(c: u32) -> [u8; 4] {
    [
        ((c >> 16) & 0xff) as u8,
        ((c >> 8) & 0xff) as u8,
        (c & 0xff) as u8,
        0xff,
    ]
}

/// Nearest palette index for `0xFFRRGGBB` via RGB squared distance; 0 for transparent.
/// Duplicated from the engine's sky quantiser — editor has no `pic-data` dep.
pub fn nearest_palette_index(colour: u32, palette: &[u32]) -> u8 {
    if colour == 0 {
        return 0;
    }
    let r = ((colour >> 16) & 0xFF) as i32;
    let g = ((colour >> 8) & 0xFF) as i32;
    let b = (colour & 0xFF) as i32;
    let mut best_dist = i32::MAX;
    let mut best = 0u8;
    for (i, &p) in palette.iter().take(256).enumerate() {
        let dr = r - ((p >> 16) & 0xFF) as i32;
        let dg = g - ((p >> 8) & 0xFF) as i32;
        let db = b - (p & 0xFF) as i32;
        let dist = dr * dr + dg * dg + db * db;
        if dist < best_dist {
            best_dist = dist;
            best = i as u8;
        }
    }
    best
}
