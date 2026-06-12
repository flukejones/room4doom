//! Parse-side sector mover classification over the engine's map structures.
//!
//! The build-time mover vertex pass lives in `rbsp::bsp3d::movers`; this only
//! answers "does anything move this sector" for the AABB expansion at parse.

use crate::map_defs::{LineDef, Sector};
use rbsp::bsp3d::movers::{MoverKind, classify_special};
use std::collections::HashMap;

/// Build a tag→linedef-indices map for O(1) lookup per sector tag.
pub(crate) fn build_tag_linedef_index(linedefs: &[LineDef]) -> HashMap<i16, Vec<usize>> {
    let mut map: HashMap<i16, Vec<usize>> = HashMap::new();
    for (li, ld) in linedefs.iter().enumerate() {
        if ld.tag != 0 {
            map.entry(ld.tag).or_default().push(li);
        }
    }
    map
}

/// Classify a sector's movement type from the linedef specials targeting it.
pub(crate) fn classify_sector_mover(
    sector: &Sector,
    linedefs: &[LineDef],
    tag_linedefs: &HashMap<i16, Vec<usize>>,
) -> Option<MoverKind> {
    let mut result: Option<MoverKind> = None;

    if sector.tag != 0
        && let Some(indices) = tag_linedefs.get(&sector.tag)
    {
        for &li in indices {
            if let Some(kind) = classify_special(linedefs[li].special) {
                result = Some(match result {
                    Some(prev) => prev.combine(kind),
                    None => kind,
                });
            }
        }
    }
    for line in &sector.lines {
        if let Some(back) = &line.backsector
            && back.num == sector.num
            && let Some(kind) = classify_special(line.special)
        {
            result = Some(match result {
                Some(prev) => prev.combine(kind),
                None => kind,
            });
        }
    }
    result
}
