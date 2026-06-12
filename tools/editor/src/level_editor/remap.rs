//! Bulk remapping of texture/flat/thing/special values across the map.

use std::collections::HashSet;

use editor_core::{EditorMap, LineDef, Name8, SpecialDef};

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
    match kind {
        RemapKind::Thing => remap_i32(map.things.values_mut().map(|t| &mut t.kind), pairs),
        RemapKind::LineSpecial => remap_i32(map.lines.values_mut().map(|l| &mut l.special), pairs),
        RemapKind::SectorSpecial => {
            remap_i32(map.sectors.values_mut().map(|s| &mut s.special), pairs)
        }
        RemapKind::Texture => remap_name8(
            map.lines.values_mut().flat_map(|line| {
                [Some(&mut line.front), line.back.as_mut()]
                    .into_iter()
                    .flatten()
                    .flat_map(|side| {
                        [
                            &mut side.top_tex,
                            &mut side.bottom_tex,
                            &mut side.middle_tex,
                        ]
                    })
            }),
            pairs,
        ),
        RemapKind::Flat => remap_name8(
            map.sectors
                .values_mut()
                .flat_map(|s| [&mut s.floor_flat, &mut s.ceil_flat]),
            pairs,
        ),
    }
}

/// One full field pass per pair, in pair order, so chained pairs (a→b then b→c) cascade a to c.
fn remap_fields<'a, T: PartialEq + Copy + 'a>(
    fields: impl Iterator<Item = &'a mut T>,
    pairs: &[RemapPair],
    parse: impl Fn(&str) -> Option<T>,
) -> usize {
    let mut fields: Vec<&mut T> = fields.collect();
    let mut changed = 0;
    for pair in pairs {
        let (Some(from), Some(to)) = (parse(&pair.from), parse(&pair.to)) else {
            continue;
        };
        for field in &mut fields {
            if **field == from {
                **field = to;
                changed += 1;
            }
        }
    }
    changed
}

fn remap_i32<'a>(fields: impl Iterator<Item = &'a mut i32>, pairs: &[RemapPair]) -> usize {
    remap_fields(fields, pairs, |s| s.parse::<i32>().ok())
}

fn remap_name8<'a>(fields: impl Iterator<Item = &'a mut Name8>, pairs: &[RemapPair]) -> usize {
    remap_fields(fields, pairs, |s| Name8::from_dwd_field(s).ok())
}

/// Whether `value` is in the project's special defs; no defs → nothing is known.
fn known_special(specials: Option<&[SpecialDef]>, value: i32) -> bool {
    specials.is_some_and(|s| s.iter().any(|d| d.value == value))
}

/// Values of `kind` in the map but unknown to the loaded assets/`specials` defs.
pub fn collect_unknown(
    map: &EditorMap,
    kind: RemapKind,
    assets: Option<&EditorAssets>,
    specials: Option<&[SpecialDef]>,
) -> Vec<String> {
    match kind {
        RemapKind::Thing => unique_sorted(
            map.things
                .values()
                .map(|t| t.kind)
                .filter(|kind| !DEFAULT_THINGS.iter().any(|t| t.kind == *kind))
                .map(|kind| kind.to_string()),
        ),
        RemapKind::LineSpecial => unique_sorted(
            map.lines
                .values()
                .map(|l| l.special)
                .filter(|&v| v != 0 && !known_special(specials, v))
                .map(|v| v.to_string()),
        ),
        RemapKind::SectorSpecial => unique_sorted(
            map.sectors
                .values()
                .map(|s| s.special)
                .filter(|&v| v != 0 && !known_special(specials, v))
                .map(|v| v.to_string()),
        ),
        RemapKind::Texture => {
            let Some(assets) = assets else {
                return Vec::new();
            };
            unique_sorted(
                map.lines
                    .values()
                    .flat_map(LineDef::sides)
                    .flat_map(|side| [side.top_tex, side.bottom_tex, side.middle_tex])
                    .filter(|slot| !slot.is_empty() && !assets.map_texture_exists(slot))
                    .map(|slot| slot.as_str().to_owned()),
            )
        }
        RemapKind::Flat => {
            let Some(assets) = assets else {
                return Vec::new();
            };
            unique_sorted(
                map.sectors
                    .values()
                    .flat_map(|s| [s.floor_flat, s.ceil_flat])
                    .filter(|slot| !slot.is_empty() && assets.iwad_flat_num(slot).is_none())
                    .map(|slot| slot.as_str().to_owned()),
            )
        }
    }
}

/// First-occurrence dedupe, then sort.
fn unique_sorted(values: impl Iterator<Item = String>) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<String> = values.filter(|v| seen.insert(v.clone())).collect();
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
            .values()
            .map(|s| s.special)
            .find(|&v| v != 0)
            .expect("fixture has a sector special");
        let before = map.sectors.values().filter(|s| s.special == from).count();
        assert!(before > 0);
        let changed = apply_remap(
            &mut map,
            RemapKind::SectorSpecial,
            &[pair(&from.to_string(), "9")],
        );
        assert_eq!(changed, before, "every matching sector special remapped");
        assert_eq!(
            map.sectors.values().filter(|s| s.special == from).count(),
            0,
            "none of the old value remains"
        );
    }

    #[test]
    fn chained_pairs_cascade_in_pair_order() {
        let mut map = e1m1();
        let n2015 = map.things.values().filter(|t| t.kind == 2015).count();
        let n2014 = map.things.values().filter(|t| t.kind == 2014).count();
        assert!(n2015 > 0 && n2014 > 0, "fixture has both thing kinds");
        let changed = apply_remap(
            &mut map,
            RemapKind::Thing,
            &[pair("2015", "2014"), pair("2014", "2013")],
        );
        assert_eq!(
            changed,
            n2015 + (n2014 + n2015),
            "second pair also remaps values produced by the first"
        );
        assert!(
            map.things
                .values()
                .all(|t| t.kind != 2015 && t.kind != 2014)
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
        let specials = collect_unknown(&map, RemapKind::LineSpecial, None, None);
        assert!(!specials.is_empty());
        let mut sorted = specials.clone();
        sorted.sort();
        assert_eq!(specials, sorted);
    }

    #[test]
    fn known_specials_are_not_reported_unknown() {
        let map = e1m1();
        let all = collect_unknown(&map, RemapKind::LineSpecial, None, None);
        let defs: Vec<SpecialDef> = all
            .iter()
            .map(|v| SpecialDef {
                value: v.parse().expect("numeric special"),
                desc: String::new(),
            })
            .collect();
        let unknown = collect_unknown(&map, RemapKind::LineSpecial, None, Some(&defs));
        assert!(unknown.is_empty(), "defs cover every special: {unknown:?}");
    }
}
