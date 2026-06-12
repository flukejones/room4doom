//! GPU atlas textures + lookup maps for the wgpu canvas. Three atlases packed once per map/asset change: **Flat atlas** — one 64×64 RGBA tile per distinct flat, palette indices baked to RGBA; **Wall atlas** — shelf-packed RGBA wall textures, transparent texels (`u16::MAX`) → alpha 0; **Sprite atlas** — one RGBA rect per thing-kind icon, maps to [`SpriteSlot`] normalised UVs.

use std::collections::{HashMap, HashSet};

use editor_core::{EditorMap, Name8, SectorKey};

use crate::assets::palette::wad_color_to_rgba;
use crate::assets::{
    self, EditorAssets, MISSING_PATCH_INDEX, MISSING_PATCH_RGBA, TRANSPARENT_INDEX,
};
use crate::render::frame::SpriteSlot;
use crate::render::sprites::{SpriteRgba, ThingSpriteCache};
use crate::render::wgpu::{AtlasData, WallRect};
use crate::render::{FNV_OFFSET, fnv_fold};

/// [`assets::FLAT_SIDE`] in the atlas's u32 texel arithmetic.
const FLAT_SIDE: u32 = assets::FLAT_SIDE as u32;
/// Sprite atlas padding (texels) — prevents neighbour bleed under nearest sampling.
const SPRITE_PAD: u32 = 1;
/// Wall shelf padding (texels) between packed textures.
const WALL_PAD: u32 = 1;
/// Maximum wall atlas dimension; textures wider than this are skipped.
const WALL_ATLAS_MAX: u32 = 4096;

/// Atlas origin lookup maps. `*_tile` maps rebuilt per sector edit; rest rebuilt when used-name set changes.
#[derive(Default)]
pub struct AtlasMaps {
    /// Sector → floor flat tile origin (atlas texels); absent = unknown flat.
    pub sector_tile: HashMap<SectorKey, [f32; 2]>,
    /// Sector → ceil flat tile origin (atlas texels); absent = unknown flat.
    pub sector_ceil_tile: HashMap<SectorKey, [f32; 2]>,
    /// Flat name → atlas tile origin (layout; drives the `*_tile` vecs).
    pub flat_name_tile: HashMap<Name8, [f32; 2]>,
    /// Wall texture → atlas origin + intrinsic size.
    pub wall_rects: HashMap<Name8, WallRect>,
    /// Thing kind → sprite atlas slot.
    pub sprite_slots: HashMap<i32, SpriteSlot>,
}

/// Build all atlases + lookup maps. `generation` is bumped by the caller on change.
pub fn build(
    assets: &EditorAssets,
    map: &EditorMap,
    sprites: &ThingSpriteCache,
    generation: u64,
) -> (AtlasData, AtlasMaps) {
    let (flat_rgba, flat_w, flat_h, flat_name_tile) = pack_flat_atlas(assets, map);
    let (wall_rgba, wall_atlas_w, wall_atlas_h, wall_rects) = pack_wall_atlas(assets, map);
    let (sprite_rgba, sprite_w, sprite_h, sprite_slots) = pack_sprite_atlas(sprites, map);

    let data = AtlasData {
        flat_rgba,
        flat_atlas_w: flat_w,
        flat_atlas_h: flat_h,
        wall_rgba,
        wall_atlas_w,
        wall_atlas_h,
        sprite_rgba,
        sprite_w,
        sprite_h,
        generation,
    };
    let mut maps = AtlasMaps {
        sector_tile: HashMap::new(),
        sector_ceil_tile: HashMap::new(),
        flat_name_tile,
        wall_rects,
        sprite_slots,
    };
    remap_sector_tiles(map, &mut maps);
    (data, maps)
}

/// Recompute every sector's tile origins from `flat_name_tile` (atlas build / map load).
pub fn remap_sector_tiles(map: &EditorMap, maps: &mut AtlasMaps) {
    maps.sector_tile.clear();
    maps.sector_ceil_tile.clear();
    let all: Vec<SectorKey> = map.sectors.keys().collect();
    remap_sector_tiles_for(map, maps, &all);
}

/// Refresh tile origins for `sectors` only; removed or unresolved keys drop their entries.
pub fn remap_sector_tiles_for(map: &EditorMap, maps: &mut AtlasMaps, sectors: &[SectorKey]) {
    for &key in sectors {
        let tiles = map.sectors.get(key).map(|s| {
            (
                maps.flat_name_tile.get(&s.floor_flat).copied(),
                maps.flat_name_tile.get(&s.ceil_flat).copied(),
            )
        });
        let (floor, ceil) = tiles.unwrap_or((None, None));
        match floor {
            Some(t) => {
                maps.sector_tile.insert(key, t);
            }
            None => {
                maps.sector_tile.remove(&key);
            }
        }
        match ceil {
            Some(t) => {
                maps.sector_ceil_tile.insert(key, t);
            }
            None => {
                maps.sector_ceil_tile.remove(&key);
            }
        }
    }
}

/// Hash of the atlas inputs: used flat/wall names, thing kinds, asset generation; unchanged key → atlas RGBA is identical, skip pack+upload, only [`remap_sector_tiles`] needed.
pub fn content_key(
    assets: &EditorAssets,
    map: &EditorMap,
    sprites: &ThingSpriteCache,
    walls: &[Name8],
) -> u64 {
    let mut h = FNV_OFFSET;
    let g = assets.generation();
    for v in [g.textures, g.patches, g.animations] {
        h = fnv_fold(h, v);
    }

    let mut flats: Vec<Name8> = Vec::new();
    for s in map.sectors.values() {
        flats.push(s.floor_flat);
        flats.push(s.ceil_flat);
    }
    flats.sort_unstable_by(|a, b| a.as_str().cmp(b.as_str()));
    flats.dedup();
    for n in &flats {
        h = fold_name(h, n, 0xff);
    }

    let mut walls = walls.to_vec();
    walls.sort_unstable_by(|a, b| a.as_str().cmp(b.as_str()));
    for n in &walls {
        h = fold_name(h, n, 0xfe);
    }

    let mut kinds: Vec<i32> = map.things.values().map(|t| t.kind).collect();
    kinds.sort_unstable();
    kinds.dedup();
    for k in kinds {
        h = fnv_fold(h, k as u64);
        if sprites.get(k).is_some() {
            h = fnv_fold(h, 1);
        }
    }
    h
}

fn fold_name(mut h: u64, n: &Name8, sep: u64) -> u64 {
    for b in n.as_str().bytes() {
        h = fnv_fold(h, b as u64);
    }
    fnv_fold(h, sep)
}

/// Pack distinct flats into a horizontal strip of 64×64 RGBA tiles.
fn pack_flat_atlas(
    assets: &EditorAssets,
    map: &EditorMap,
) -> (Vec<u8>, u32, u32, HashMap<Name8, [f32; 2]>) {
    let mut names: Vec<Name8> = Vec::new();
    let mut seen: HashSet<Name8> = HashSet::new();
    for s in map.sectors.values() {
        for flat in [&s.floor_flat, &s.ceil_flat] {
            if flat.is_empty() || assets.iwad_flat_num(flat).is_none() {
                continue;
            }
            if seen.insert(*flat) {
                names.push(*flat);
            }
        }
    }
    if names.is_empty() {
        return (Vec::new(), 0, 0, HashMap::new());
    }
    let pal = assets.palette();
    let cols = names.len() as u32;
    let atlas_w = cols * FLAT_SIDE;
    let atlas_h = FLAT_SIDE;
    let mut rgba = vec![0u8; (atlas_w * atlas_h * 4) as usize];
    let mut tile_of = HashMap::new();
    for (i, name) in names.iter().enumerate() {
        let Some(num) = assets.iwad_flat_num(name) else {
            continue;
        };
        let pic = &assets.iwad_flats()[num].flat;
        let ox = i as u32 * FLAT_SIDE;
        for ty in 0..FLAT_SIDE {
            for tx in 0..FLAT_SIDE {
                let idx = pic.data[(ty * FLAT_SIDE + tx) as usize] as usize;
                let c = pal.0[idx];
                let dst = (((ty * atlas_w) + ox + tx) * 4) as usize;
                let px = wad_color_to_rgba(c);
                rgba[dst..dst + 4].copy_from_slice(&px);
            }
        }
        tile_of.insert(*name, [ox as f32, 0.0]);
    }
    (rgba, atlas_w, atlas_h, tile_of)
}

/// Shelf-pack all map wall textures into one RGBA atlas from the composed cache.
fn pack_wall_atlas(
    assets: &EditorAssets,
    map: &EditorMap,
) -> (Vec<u8>, u32, u32, HashMap<Name8, WallRect>) {
    let names = collect_wall_names(assets, map);
    if names.is_empty() {
        return (Vec::new(), 0, 0, HashMap::new());
    }

    let mut rects: HashMap<Name8, WallRect> = HashMap::new();
    let mut baked: Vec<(Name8, Vec<u8>, u32, u32)> = Vec::new();

    for name in &names {
        let Some(pic) = assets.composed(name) else {
            continue;
        };
        let w = pic.width as u32;
        let h = pic.height as u32;
        if w == 0 || h == 0 || w > WALL_ATLAS_MAX || h > WALL_ATLAS_MAX {
            log::warn!(
                "wall texture {:?} ({}×{}) unusable (empty or exceeds atlas max); skipped",
                name.as_str(),
                w,
                h
            );
            continue;
        }
        let pal = assets.palette();
        let mut tile = vec![0u8; (w * h * 4) as usize];
        for ty in 0..h {
            for tx in 0..w {
                let idx = pic.data[(tx as usize * h as usize) + ty as usize];
                let dst = ((ty * w + tx) * 4) as usize;
                if idx == MISSING_PATCH_INDEX {
                    tile[dst..dst + 4].copy_from_slice(&MISSING_PATCH_RGBA);
                } else if idx != TRANSPARENT_INDEX {
                    tile[dst..dst + 4].copy_from_slice(&wad_color_to_rgba(pal.0[idx as usize]));
                }
            }
        }
        baked.push((*name, tile, w, h));
    }

    if baked.is_empty() {
        return (Vec::new(), 0, 0, HashMap::new());
    }

    let atlas_w = WALL_ATLAS_MAX;
    let mut shelves: Vec<(u32, u32, u32)> = Vec::new(); // (x_cursor, shelf_y, shelf_h)
    shelves.push((0, 0, 0));

    for (_, _, w, h) in &baked {
        let shelf = shelves.last_mut().unwrap();
        let needed_w = w + WALL_PAD;
        let needed_h = *h;
        if shelf.0 + needed_w > atlas_w && shelf.0 > 0 {
            let next_y = shelf.1 + shelf.2 + WALL_PAD;
            shelves.push((0, next_y, 0));
        }
        let shelf = shelves.last_mut().unwrap();
        if needed_h > shelf.2 {
            shelf.2 = needed_h;
        }
        shelf.0 += needed_w;
    }
    let last = shelves.last().unwrap();
    let atlas_h = last.1 + last.2;
    if atlas_h == 0 {
        return (Vec::new(), 0, 0, HashMap::new());
    }

    let mut rgba = vec![0u8; (atlas_w * atlas_h * 4) as usize];

    let mut shelf_idx = 0usize;
    let mut cx = 0u32;
    for (name, tile, w, h) in &baked {
        let needed_w = w + WALL_PAD;
        if cx + needed_w > atlas_w && cx > 0 {
            shelf_idx += 1;
            cx = 0;
        }
        let sy = shelves[shelf_idx].1;
        for ty in 0..*h {
            let src_row = (ty * w * 4) as usize;
            let dst_row = ((sy + ty) * atlas_w + cx) as usize * 4;
            rgba[dst_row..dst_row + (w * 4) as usize]
                .copy_from_slice(&tile[src_row..src_row + (w * 4) as usize]);
        }
        rects.insert(
            *name,
            WallRect {
                x: cx,
                y: sy,
                w: *w,
                h: *h,
            },
        );
        cx += needed_w;
    }

    (rgba, atlas_w, atlas_h, rects)
}

/// Unique wall-texture names defined in the active WAD. Shared by atlas packer and composed-cache prefill.
pub fn collect_wall_names(assets: &EditorAssets, map: &EditorMap) -> Vec<Name8> {
    let mut seen: HashSet<Name8> = HashSet::new();
    let mut names: Vec<Name8> = Vec::new();
    for line in map.lines.values() {
        for side in line.sides() {
            for tex in [&side.top_tex, &side.middle_tex, &side.bottom_tex] {
                if tex.is_empty() {
                    continue;
                }
                let s = tex.as_str();
                if s == "-" {
                    continue;
                }
                if !assets.map_texture_exists(tex) {
                    continue;
                }
                if seen.insert(*tex) {
                    names.push(*tex);
                }
            }
        }
    }
    names
}

/// Shelf-pack thing icons into one RGBA atlas; returns bytes, dims, and kind→slot map.
fn pack_sprite_atlas(
    sprites: &ThingSpriteCache,
    map: &EditorMap,
) -> (Vec<u8>, u32, u32, HashMap<i32, SpriteSlot>) {
    let mut kinds: Vec<i32> = map.things.values().map(|t| t.kind).collect();
    kinds.sort_unstable();
    kinds.dedup();
    let entries: Vec<(i32, &SpriteRgba)> = kinds
        .iter()
        .filter_map(|&k| sprites.get(k).map(|s| (k, s)))
        .collect();
    if entries.is_empty() {
        return (Vec::new(), 0, 0, HashMap::new());
    }

    let atlas_h = entries.iter().map(|(_, s)| s.height).max().unwrap_or(1) + SPRITE_PAD * 2;
    let atlas_w: u32 = entries
        .iter()
        .map(|(_, s)| s.width + SPRITE_PAD)
        .sum::<u32>()
        + SPRITE_PAD;
    let mut rgba = vec![0u8; (atlas_w * atlas_h * 4) as usize];
    let mut slots = HashMap::new();
    let mut cx = SPRITE_PAD;
    for (kind, sprite) in entries {
        for sy in 0..sprite.height {
            for sx in 0..sprite.width {
                let src = ((sy * sprite.width + sx) * 4) as usize;
                let dx = cx + sx;
                let dy = SPRITE_PAD + sy;
                let dst = ((dy * atlas_w + dx) * 4) as usize;
                rgba[dst..dst + 4].copy_from_slice(&sprite.rgba[src..src + 4]);
            }
        }
        let u0 = cx as f32 / atlas_w as f32;
        let v0 = SPRITE_PAD as f32 / atlas_h as f32;
        let u1 = (cx + sprite.width) as f32 / atlas_w as f32;
        let v1 = (SPRITE_PAD + sprite.height) as f32 / atlas_h as f32;
        slots.insert(
            kind,
            SpriteSlot {
                u0,
                v0,
                u1,
                v1,
            },
        );
        cx += sprite.width + SPRITE_PAD;
    }
    (rgba, atlas_w, atlas_h, slots)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::EditorAssets;
    use wad::WadData;

    #[test]
    fn e1m1_flat_atlas_packs_distinct_flats() {
        let wad = WadData::new(&test_utils::doom1_wad_path());
        let map = editor_core::import_wad_map(&wad, "E1M1").expect("E1M1 imports");
        let mut assets = EditorAssets::load(&[test_utils::doom1_wad_path()], &wad, None);
        assets.set_map_wad("doom1.wad");
        let names = collect_wall_names(&assets, &map);
        assets.ensure_composed(&names, &wad);
        let sprites = ThingSpriteCache::default();
        let (data, maps) = build(&assets, &map, &sprites, 1);

        assert_eq!(data.flat_atlas_h, FLAT_SIDE);
        assert_eq!(data.flat_atlas_w % FLAT_SIDE, 0);
        let tiles = data.flat_atlas_w / FLAT_SIDE;
        assert!(tiles >= 1, "E1M1 uses at least one flat");
        assert_eq!(
            data.flat_rgba.len() as u32,
            data.flat_atlas_w * data.flat_atlas_h * 4,
            "flat atlas is RGBA (4 bytes per texel)"
        );

        assert!(!maps.sector_tile.is_empty(), "floor tiles resolved");
        let tiles: Vec<[f32; 2]> = maps
            .sector_tile
            .values()
            .chain(maps.sector_ceil_tile.values())
            .copied()
            .collect();
        assert!(!maps.sector_ceil_tile.is_empty(), "ceil tiles resolved");
        for tile in tiles {
            assert!(
                tile[0] >= 0.0 && (tile[0] as u32) < data.flat_atlas_w,
                "tile origin inside atlas"
            );
        }
    }

    #[test]
    fn e1m1_wall_atlas_packs_and_bounds() {
        let wad = WadData::new(&test_utils::doom1_wad_path());
        let map = editor_core::import_wad_map(&wad, "E1M1").expect("E1M1 imports");
        let mut assets = EditorAssets::load(&[test_utils::doom1_wad_path()], &wad, None);
        assets.set_map_wad("doom1.wad");
        let names = collect_wall_names(&assets, &map);
        assets.ensure_composed(&names, &wad);
        let sprites = ThingSpriteCache::default();
        let (data, maps) = build(&assets, &map, &sprites, 1);

        assert!(!maps.wall_rects.is_empty(), "E1M1 has wall textures");
        assert!(data.wall_atlas_w > 0 && data.wall_atlas_h > 0);
        assert_eq!(
            data.wall_rgba.len() as u32,
            data.wall_atlas_w * data.wall_atlas_h * 4
        );

        let mut rects: Vec<_> = maps.wall_rects.iter().collect();
        rects.sort_by_key(|(n, _)| n.to_wad_bytes());
        for (name, rect) in &rects {
            assert!(
                rect.w > 0 && rect.h > 0,
                "{:?} has non-zero size",
                name.as_str()
            );
            assert!(
                rect.x + rect.w <= data.wall_atlas_w,
                "{:?} right edge in atlas",
                name.as_str()
            );
            assert!(
                rect.y + rect.h <= data.wall_atlas_h,
                "{:?} bottom edge in atlas",
                name.as_str()
            );
        }

        let has_opaque = data.wall_rgba.chunks_exact(4).any(|p| p[3] == 0xff);
        assert!(has_opaque, "wall atlas has opaque texels");
    }

    /// A scoped remap of just the edited sector matches a full remap.
    #[test]
    fn scoped_remap_matches_full_remap() {
        let wad = WadData::new(&test_utils::doom1_wad_path());
        let mut map = editor_core::import_wad_map(&wad, "E1M1").expect("E1M1 imports");
        let mut assets = EditorAssets::load(&[test_utils::doom1_wad_path()], &wad, None);
        assets.set_map_wad("doom1.wad");
        let sprites = ThingSpriteCache::default();
        let (_, mut scoped) = build(&assets, &map, &sprites, 1);
        let (_, mut full) = build(&assets, &map, &sprites, 1);

        let sk = map.sectors.keys().next().expect("has sectors");
        let first_flat = map.sectors[sk].floor_flat;
        let other = map
            .sectors
            .values()
            .map(|s| s.floor_flat)
            .find(|f| *f != first_flat)
            .expect("two distinct floor flats");
        map.sectors[sk].floor_flat = other;
        map.sectors[sk].ceil_flat = Name8::new("ZZNEWFLT").expect("name");

        remap_sector_tiles(&map, &mut full);
        remap_sector_tiles_for(&map, &mut scoped, &[sk]);
        assert_eq!(scoped.sector_tile, full.sector_tile);
        assert_eq!(scoped.sector_ceil_tile, full.sector_ceil_tile);
    }

    /// Key is stable under vertex moves; changes when flat name set changes.
    #[test]
    fn content_key_tracks_used_names_not_geometry() {
        let wad = WadData::new(&test_utils::doom1_wad_path());
        let mut map = editor_core::import_wad_map(&wad, "E1M1").expect("E1M1 imports");
        let mut assets = EditorAssets::load(&[test_utils::doom1_wad_path()], &wad, None);
        assets.set_map_wad("doom1.wad");
        let sprites = ThingSpriteCache::default();

        let walls = collect_wall_names(&assets, &map);
        let key0 = content_key(&assets, &map, &sprites, &walls);
        let vk = map.vertices.keys().next().expect("has vertices");
        map.vertices[vk].x += 64.0; // a pure position move
        assert_eq!(
            content_key(&assets, &map, &sprites, &walls),
            key0,
            "moving a vertex does not change the atlas content"
        );

        let sk = map.sectors.keys().next().expect("has sectors");
        let first_flat = map.sectors[sk].floor_flat;
        let existing = map
            .sectors
            .values()
            .map(|s| s.floor_flat)
            .find(|f| *f != first_flat)
            .expect("E1M1 has two distinct floor flats");
        map.sectors[sk].floor_flat = existing;
        assert_eq!(
            content_key(&assets, &map, &sprites, &walls),
            key0,
            "reassigning to an already-packed flat does not rebuild the atlas"
        );

        map.sectors[sk].floor_flat = Name8::new("ZZNEWFLT").expect("name");
        assert_ne!(
            content_key(&assets, &map, &sprites, &walls),
            key0,
            "a new flat name changes the atlas content"
        );
    }
}
