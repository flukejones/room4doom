//! Import a vanilla map (THINGS/LINEDEFS/SIDEDEFS/VERTEXES/SECTORS) from a WAD into the editor model. WAD sectors are already first-class records and convert 1:1 with no merging (the merge only exists for .dwd ingestion). All VERTEXES entries are kept, including node-builder seg vertices, so linedef indices are preserved; unreferenced vertices are harmless. WAD sidedefs may be shared between linedefs (rare, illegal in vanilla); sides are embedded per line here, so shared sidedefs are de-shared by value construction. Thing coordinates are NOT grid-snapped on WAD import (snapping is a .dwd-load behavior only); WAD data is authoritative as-is. The reverse direction — `EditorMap` into `rbsp::BspInput` and WAD lumps — is the export path in `wad_export`.

use std::error::Error;
use std::fmt;

use wad::types::{WadLineDef, WadRecord, WadSector, WadSideDef, WadThing, WadVertex};
use wad::{MapLump, WadData};

use crate::model::{
    DenseLineDef, DenseMap, DenseSideDef, EditorMap, GROWTH_HEADROOM, Sector, Thing, Vertex,
};
use crate::name8::Name8;
use crate::{LineFlags, ThingFlags, geom};

/// Sentinel for "no front sidedef" in a WAD linedef; always invalid.
const NO_SIDEDEF: u16 = u16::MAX;

/// Failure while converting WAD lumps into an [`EditorMap`].
#[derive(Debug)]
pub enum WadImportError {
    /// No lump with the map marker name exists in the WAD.
    MapNotFound { name: String },
    /// A linedef has no front sidedef (sentinel 0xFFFF).
    MissingFrontSide { linedef: usize },
    /// A linedef references a sidedef index outside the SIDEDEFS lump.
    BadSideIndex { linedef: usize, index: u16 },
    /// A sidedef references a sector index outside the SECTORS lump.
    BadSectorIndex { sidedef: usize, index: i16 },
    /// A linedef references a vertex index outside the VERTEXES lump.
    BadVertexIndex { linedef: usize, index: u16 },
    /// A texture or flat name in the WAD is not a valid 8-byte name.
    BadName { context: &'static str, name: String },
}

impl fmt::Display for WadImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MapNotFound {
                name,
            } => write!(f, "map {name} not found in wad"),
            Self::MissingFrontSide {
                linedef,
            } => {
                write!(f, "linedef {linedef} has no front sidedef")
            }
            Self::BadSideIndex {
                linedef,
                index,
            } => {
                write!(f, "linedef {linedef}: sidedef index {index} out of range")
            }
            Self::BadSectorIndex {
                sidedef,
                index,
            } => {
                write!(f, "sidedef {sidedef}: sector index {index} out of range")
            }
            Self::BadVertexIndex {
                linedef,
                index,
            } => {
                write!(f, "linedef {linedef}: vertex index {index} out of range")
            }
            Self::BadName {
                context,
                name,
            } => write!(f, "{context}: invalid name {name:?}"),
        }
    }
}

impl Error for WadImportError {}

/// Convert one map's vanilla lumps into an [`EditorMap`].
pub fn import_wad_map(wad: &WadData, map_name: &str) -> Result<EditorMap, WadImportError> {
    if !wad.lump_exists(map_name) {
        return Err(WadImportError::MapNotFound {
            name: map_name.to_owned(),
        });
    }

    let vertices = collect_presized(wad, map_name, MapLump::Vertexes, |v: WadVertex| Vertex {
        x: v.x,
        y: v.y,
    });

    let wad_sectors: Vec<WadSector> =
        collect_presized(wad, map_name, MapLump::Sectors, |s: WadSector| s);
    let mut sectors = Vec::with_capacity(wad_sectors.len() + GROWTH_HEADROOM);
    for s in &wad_sectors {
        sectors.push(Sector {
            floor_height: i32::from(s.floor_height),
            floor_flat: name_from_wad("sector floor flat", &s.floor_tex)?,
            ceil_height: i32::from(s.ceil_height),
            ceil_flat: name_from_wad("sector ceiling flat", &s.ceil_tex)?,
            light_level: i32::from(s.light_level),
            special: i32::from(s.kind),
            tag: i32::from(s.tag),
        });
    }

    let sidedefs: Vec<WadSideDef> =
        collect_presized(wad, map_name, MapLump::SideDefs, |s: WadSideDef| s);

    let wad_lines: Vec<WadLineDef> =
        collect_presized(wad, map_name, MapLump::LineDefs, |l: WadLineDef| l);
    let mut lines = Vec::with_capacity(wad_lines.len() + GROWTH_HEADROOM);
    for (i, l) in wad_lines.iter().enumerate() {
        check_vertex(i, l.start_vertex, vertices.len())?;
        check_vertex(i, l.end_vertex, vertices.len())?;

        if l.front_sidedef == NO_SIDEDEF {
            return Err(WadImportError::MissingFrontSide {
                linedef: i,
            });
        }
        let front = convert_side(i, l.front_sidedef, &sidedefs, sectors.len())?;
        let back = match l.back_sidedef {
            Some(index) => Some(convert_side(i, index, &sidedefs, sectors.len())?),
            None => None,
        };

        lines.push(DenseLineDef {
            v1: u32::from(l.start_vertex),
            v2: u32::from(l.end_vertex),
            flags: LineFlags::from_bits_retain(i32::from(l.flags)),
            special: i32::from(l.special),
            tag: i32::from(l.sector_tag),
            front,
            back,
        });
    }

    let things = collect_presized(wad, map_name, MapLump::Things, |t: WadThing| Thing {
        x: i32::from(t.x),
        y: i32::from(t.y),
        // WAD things have no Z; derived from the sector after assembly.
        z: 0,
        angle: i32::from(t.angle),
        kind: i32::from(t.kind),
        options: ThingFlags::from_bits_retain(i32::from(t.flags)),
    });

    // Vanilla VERTEXES holds the linedef-endpoint vertices followed by the BSP build's seg-split vertices; the editor only needs the former.
    let dense = DenseMap {
        vertices,
        lines,
        sectors,
        things,
        required_wads: Vec::new(),
    };
    let mut map = EditorMap::from_dense(dense).expect("indices range-checked above");
    map.prune_orphan_vertices();
    geom::derive_thing_heights(&mut map);
    Ok(map)
}

/// Collect a map lump's records with exact pre-sizing. `RecordIter` has no size hint, so the record count comes from a first counting pass (iteration is allocation-free).
fn collect_presized<R: WadRecord, T>(
    wad: &WadData,
    map_name: &str,
    lump: MapLump,
    convert: impl Fn(R) -> T,
) -> Vec<T> {
    let count = wad.map_iter::<R>(map_name, lump).count();
    let mut out = Vec::with_capacity(count + GROWTH_HEADROOM);
    out.extend(wad.map_iter::<R>(map_name, lump).map(convert));
    out
}

fn name_from_wad(context: &'static str, name: &str) -> Result<Name8, WadImportError> {
    Name8::from_wad(name).map_err(|_| WadImportError::BadName {
        context,
        name: name.to_owned(),
    })
}

fn check_vertex(linedef: usize, index: u16, vertex_count: usize) -> Result<(), WadImportError> {
    if (index as usize) < vertex_count {
        Ok(())
    } else {
        Err(WadImportError::BadVertexIndex {
            linedef,
            index,
        })
    }
}

fn convert_side(
    linedef: usize,
    index: u16,
    sidedefs: &[WadSideDef],
    sector_count: usize,
) -> Result<DenseSideDef, WadImportError> {
    let side = sidedefs
        .get(index as usize)
        .ok_or(WadImportError::BadSideIndex {
            linedef,
            index,
        })?;
    if side.sector < 0 || side.sector as usize >= sector_count {
        return Err(WadImportError::BadSectorIndex {
            sidedef: index as usize,
            index: side.sector,
        });
    }
    Ok(DenseSideDef {
        x_offset: i32::from(side.x_offset),
        y_offset: i32::from(side.y_offset),
        top_tex: name_from_wad("sidedef upper texture", &side.upper_tex)?,
        bottom_tex: name_from_wad("sidedef lower texture", &side.lower_tex)?,
        middle_tex: name_from_wad("sidedef middle texture", &side.middle_tex)?,
        sector: Some(side.sector as u32),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dwd::{THING_SNAP_MASK, parse_dwd};
    use crate::validate::validate;

    const FIXTURE: &str = include_str!("../../doomed-parser/tests/fixtures/E1M1.dwd");

    fn e1m1() -> EditorMap {
        let wad = WadData::new(&test_utils::doom1_wad_path());
        import_wad_map(&wad, "E1M1").expect("shareware E1M1 imports")
    }

    #[test]
    fn e1m1_imports_with_expected_counts_and_clean_validation() {
        let map = e1m1();
        assert_eq!(map.lines.len(), 475);
        assert_eq!(map.things.len(), 138);
        assert_eq!(map.sectors.len(), 85);
        assert_eq!(
            map.vertices.len(),
            385,
            "only linedef-referenced vertices; the 82 BSP seg vertices are pruned"
        );
        assert_eq!(validate(&map), Vec::new());
    }

    #[test]
    fn e1m1_first_thing_and_linedef_fields() {
        let map = e1m1();
        let t = *map.things.values().next().expect("has things");
        assert_eq!(
            (t.x, t.y, t.angle, t.kind, t.options),
            (1056, -3616, 90, 1, ThingFlags::from_bits_retain(7))
        );
        let l = map.lines.values().next().expect("has lines");
        let p1 = map.vertices[l.v1];
        let p2 = map.vertices[l.v2];
        assert_eq!((p1.x, p1.y), (1088.0, -3680.0));
        assert_eq!((p2.x, p2.y), (1024.0, -3680.0));
        assert_eq!(l.front.middle_tex.as_str(), "DOOR3");
    }

    #[test]
    fn missing_map_reports_not_found() {
        let wad = WadData::new(&test_utils::doom1_wad_path());
        let err = import_wad_map(&wad, "E9M9").expect_err("no such map");
        assert!(matches!(err, WadImportError::MapNotFound { .. }), "{err}");
    }

    /// The .dwd fixture is John Romero's released DoomEd source for E1M1; doombsp compiled it into the shipped WAD. Comparing resolved data (not indices — the dwd dedups endpoint vertices and merges sectors by value, while doombsp re-split merged sectors into connected components at save time) proves both importers reconstruct the same map. The WAD's 85 sectors deduplicate by value to exactly the dwd merge count.
    #[test]
    fn wad_import_matches_dwd_fixture_record_for_record() {
        let from_wad = e1m1().to_dense();
        let from_dwd = parse_dwd(FIXTURE).expect("fixture parses").to_dense();

        let mut distinct_wad_sectors: Vec<Sector> = Vec::with_capacity(from_wad.sectors.len());
        for s in &from_wad.sectors {
            if !distinct_wad_sectors.contains(s) {
                distinct_wad_sectors.push(*s);
            }
        }
        assert_eq!(from_dwd.sectors.len(), distinct_wad_sectors.len());
        assert_eq!(from_dwd.lines.len(), from_wad.lines.len());
        assert_eq!(from_dwd.things.len(), from_wad.things.len());

        for i in 0..from_dwd.lines.len() {
            let dl = &from_dwd.lines[i];
            let wl = &from_wad.lines[i];
            let dp = (
                from_dwd.vertices[dl.v1 as usize],
                from_dwd.vertices[dl.v2 as usize],
            );
            let wp = (
                from_wad.vertices[wl.v1 as usize],
                from_wad.vertices[wl.v2 as usize],
            );
            assert_eq!(
                (dp.0.x, dp.0.y, dp.1.x, dp.1.y),
                (wp.0.x, wp.0.y, wp.1.x, wp.1.y),
                "line {i}: endpoint coordinates differ"
            );
            assert_eq!(
                (dl.flags, dl.special, dl.tag),
                (wl.flags, wl.special, wl.tag),
                "line {i}: header fields differ"
            );
            assert_side_eq(i, "front", &from_dwd, &dl.front, &from_wad, &wl.front);
            assert_eq!(
                dl.back.is_some(),
                wl.back.is_some(),
                "line {i}: sidedness differs"
            );
            if let (Some(db), Some(wb)) = (&dl.back, &wl.back) {
                assert_side_eq(i, "back", &from_dwd, db, &from_wad, wb);
            }
        }

        for (i, (dt, wt)) in from_dwd.things.iter().zip(&from_wad.things).enumerate() {
            assert_eq!(
                (dt.x, dt.y, dt.angle, dt.kind, dt.options),
                (
                    wt.x & THING_SNAP_MASK,
                    wt.y & THING_SNAP_MASK,
                    wt.angle,
                    wt.kind,
                    wt.options
                ),
                "thing {i} differs"
            );
        }
    }

    fn assert_side_eq(
        line: usize,
        which: &str,
        dwd_map: &DenseMap,
        dwd_side: &DenseSideDef,
        wad_map: &DenseMap,
        wad_side: &DenseSideDef,
    ) {
        assert_eq!(
            (dwd_side.x_offset, dwd_side.y_offset),
            (wad_side.x_offset, wad_side.y_offset),
            "line {line} {which}: offsets differ"
        );
        assert_eq!(
            (dwd_side.top_tex, dwd_side.bottom_tex, dwd_side.middle_tex),
            (wad_side.top_tex, wad_side.bottom_tex, wad_side.middle_tex),
            "line {line} {which}: textures differ"
        );
        let dwd_s = dwd_side.sector.expect("dwd side has a sector") as usize;
        let wad_s = wad_side.sector.expect("wad side has a sector") as usize;
        assert_eq!(
            dwd_map.sectors[dwd_s], wad_map.sectors[wad_s],
            "line {line} {which}: resolved sector differs"
        );
    }
}
