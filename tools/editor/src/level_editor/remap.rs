//! Bulk remapping of texture/flat/thing/special values across the map.

use editor_core::{EditorMap, Name8};

use crate::assets::EditorAssets;
use crate::defaults::DEFAULT_THINGS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemapKind {
    Thing,
    Texture,
    Flat,
    LineSpecial,
    SectorSpecial,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RemapPair {
    pub from: String,
    pub to: String,
}

/// Apply all pairs of `kind`; returns the count of changed fields.
pub fn apply_remap(map: &mut EditorMap, kind: RemapKind, pairs: &[RemapPair]) -> usize {
    let mut changed = 0;
    match kind {
        RemapKind::Thing => {
            for pair in pairs {
                let (Ok(from), Ok(to)) = (pair.from.parse::<i32>(), pair.to.parse::<i32>()) else {
                    continue;
                };
                for thing in &mut map.things {
                    if thing.kind == from {
                        thing.kind = to;
                        changed += 1;
                    }
                }
            }
        }
        RemapKind::LineSpecial => {
            for pair in pairs {
                let (Ok(from), Ok(to)) = (pair.from.parse::<i32>(), pair.to.parse::<i32>()) else {
                    continue;
                };
                for line in &mut map.lines {
                    if line.special == from {
                        line.special = to;
                        changed += 1;
                    }
                }
            }
        }
        RemapKind::SectorSpecial => {
            for pair in pairs {
                let (Ok(from), Ok(to)) = (pair.from.parse::<i32>(), pair.to.parse::<i32>()) else {
                    continue;
                };
                for sector in &mut map.sectors {
                    if sector.special == from {
                        sector.special = to;
                        changed += 1;
                    }
                }
            }
        }
        RemapKind::Texture => {
            for pair in pairs {
                let (Ok(from), Ok(to)) = (
                    Name8::from_dwd_field(&pair.from),
                    Name8::from_dwd_field(&pair.to),
                ) else {
                    continue;
                };
                for line in &mut map.lines {
                    let mut sides = [Some(&mut line.front), line.back.as_mut()];
                    for side in sides.iter_mut().flatten() {
                        for slot in [
                            &mut side.top_tex,
                            &mut side.bottom_tex,
                            &mut side.middle_tex,
                        ] {
                            if *slot == from {
                                *slot = to;
                                changed += 1;
                            }
                        }
                    }
                }
            }
        }
        RemapKind::Flat => {
            for pair in pairs {
                let (Ok(from), Ok(to)) = (
                    Name8::from_dwd_field(&pair.from),
                    Name8::from_dwd_field(&pair.to),
                ) else {
                    continue;
                };
                for sector in &mut map.sectors {
                    for slot in [&mut sector.floor_flat, &mut sector.ceil_flat] {
                        if *slot == from {
                            *slot = to;
                            changed += 1;
                        }
                    }
                }
            }
        }
    }
    changed
}

/// Values of `kind` present in the map but unknown to the loaded assets.
pub fn collect_unknown(
    map: &EditorMap,
    kind: RemapKind,
    assets: Option<&EditorAssets>,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push_unique = |value: String| {
        if !out.contains(&value) {
            out.push(value);
        }
    };
    match kind {
        RemapKind::Thing => {
            for thing in &map.things {
                if !DEFAULT_THINGS.iter().any(|t| t.kind == thing.kind) {
                    push_unique(thing.kind.to_string());
                }
            }
        }
        RemapKind::LineSpecial => {
            for line in &map.lines {
                if line.special != 0 {
                    push_unique(line.special.to_string());
                }
            }
        }
        RemapKind::SectorSpecial => {
            for sector in &map.sectors {
                if sector.special != 0 {
                    push_unique(sector.special.to_string());
                }
            }
        }
        RemapKind::Texture => {
            let Some(assets) = assets else { return out };
            for line in &map.lines {
                for side in [Some(&line.front), line.back.as_ref()]
                    .into_iter()
                    .flatten()
                {
                    for slot in [side.top_tex, side.bottom_tex, side.middle_tex] {
                        if !slot.is_empty() && !assets.map_texture_exists(&slot) {
                            push_unique(slot.as_str().to_owned());
                        }
                    }
                }
            }
        }
        RemapKind::Flat => {
            let Some(assets) = assets else { return out };
            for sector in &map.sectors {
                for slot in [sector.floor_flat, sector.ceil_flat] {
                    if !slot.is_empty() && assets.iwad_flat_num(&slot).is_none() {
                        push_unique(slot.as_str().to_owned());
                    }
                }
            }
        }
    }
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use editor_core::import_wad_map;

    fn e1m1() -> EditorMap {
        let wad = wad::WadData::new(&test_utils::doom1_wad_path());
        import_wad_map(&wad, "E1M1").expect("E1M1 imports")
    }

    fn pair(from: &str, to: &str) -> RemapPair {
        RemapPair {
            from: from.to_owned(),
            to: to.to_owned(),
        }
    }

    #[test]
    fn texture_remap_changes_all_slots_and_is_idempotent() {
        let mut map = e1m1();
        let changed = apply_remap(&mut map, RemapKind::Texture, &[pair("STARTAN3", "STARG3")]);
        assert!(changed > 0);
        let again = apply_remap(&mut map, RemapKind::Texture, &[pair("STARTAN3", "STARG3")]);
        assert_eq!(again, 0, "second apply finds nothing left");
    }

    #[test]
    fn flat_thing_and_special_remaps_count() {
        let mut map = e1m1();
        assert!(apply_remap(&mut map, RemapKind::Flat, &[pair("FLOOR4_8", "FLOOR4_5")]) > 0);
        assert!(apply_remap(&mut map, RemapKind::Thing, &[pair("2015", "2014")]) > 0);
        assert!(apply_remap(&mut map, RemapKind::LineSpecial, &[pair("1", "31")]) > 0);
    }

    #[test]
    fn sector_special_remap_matches_by_value() {
        let mut map = e1m1();
        let from = map
            .sectors
            .iter()
            .map(|s| s.special)
            .find(|&v| v != 0)
            .expect("fixture has a sector special");
        let before = map.sectors.iter().filter(|s| s.special == from).count();
        assert!(before > 0);
        let changed = apply_remap(
            &mut map,
            RemapKind::SectorSpecial,
            &[pair(&from.to_string(), "9")],
        );
        assert_eq!(changed, before, "every matching sector special remapped");
        assert_eq!(
            map.sectors.iter().filter(|s| s.special == from).count(),
            0,
            "none of the old value remains"
        );
    }

    #[test]
    fn unparseable_pairs_are_skipped() {
        let mut map = e1m1();
        assert_eq!(
            apply_remap(&mut map, RemapKind::Thing, &[pair("notanum", "1")]),
            0
        );
        assert_eq!(
            apply_remap(&mut map, RemapKind::Texture, &[pair("WAYTOOLONG", "X")]),
            0
        );
    }

    #[test]
    fn unknown_things_and_specials_collect_unique_sorted() {
        let map = e1m1();
        let specials = collect_unknown(&map, RemapKind::LineSpecial, None);
        assert!(!specials.is_empty());
        let mut sorted = specials.clone();
        sorted.sort();
        assert_eq!(specials, sorted);
    }
}
