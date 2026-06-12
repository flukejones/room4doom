//! Reader (import only) for the DoomEd .dwd "WorldServer version 4" text format.
//! The editor writes its own native RON map format (see [`crate::map_ron`]).
//!
//! ```text
//! WorldServer version 4
//!
//! lines:{N}
//! (x1,y1) to (x2,y2) : flags : special : tag
//!     y_off (x_off : top / bottom / middle )
//!     floor_h : floor_flat ceil_h : ceil_flat light special tag
//!     [second side + sectordef, only when flags & ML_TWOSIDED]
//!
//! things:{M}
//! (x,y, angle) :kind, options
//! ```
//!
//! Format facts the records do not show:
//! - The first side field is the texture **y offset** (DoomEd's reader calls
//!   it `flags`, but doombsp reads the same slot as `firstrow` and stores it
//!   as the WAD `rowoffset`). The parenthesized field is the x offset.
//! - Empty texture/flat names are written as `-`.
//! - Each side carries a full sectordef. Byte-identical sectordefs (all seven
//!   fields) merge into one [`Sector`] in first-encounter order, exactly like
//!   doombsp's `UniqueSector`. Two distinct map areas with identical
//!   sectordefs therefore share one sector while editing — that is the
//!   format's semantics, not a defect. doombsp re-splits such sectors into
//!   connected components at save time (`RecursiveGroupSubsector`, which
//!   needs BSP subsectors); the WAD export pipeline owns that step.
//! - Thing coordinates snap to the 16-unit grid on load (`x & -16`).
//! - DoomEd reads thing coordinates with `%i` (which would accept octal/hex);
//!   this parser accepts decimal only, since DoomEd never writes anything
//!   else.
//! - Whitespace between tokens is flexible exactly where `fscanf` would
//!   accept it; structure and field order are strict.

use std::collections::HashMap;
use std::fmt;

use geom_kernel::{
    DenseLineDef, DenseMap, DenseSideDef, EditorMap, GROWTH_HEADROOM, LineFlags, Sector, Thing,
    ThingFlags, Vertex,
};

use crate::cursor::{Cursor, CursorError};

/// First line of every .dwd file.
pub const DWD_HEADER: &str = "WorldServer version 4";
/// The only supported format version.
pub const DWD_VERSION: i32 = 4;
/// Thing coordinates are AND-ed with this on load (16-unit grid snap).
pub const THING_SNAP_MASK: i32 = -16;

/// Exact minimum bytes a line record can occupy, used to cap pre-allocation
/// against a header count larger than the input could possibly contain.
/// Header `(%f,%f) to (%f,%f) : %d : %d : %d` = 11 literal bytes (`( , ) to
/// ( , ) : : :`) + 7 one-char tokens = 18, plus one mandatory side (below).
const MIN_LINE_RECORD_BYTES: usize = 18 + MIN_SIDE_RECORD_BYTES;
/// Exact minimum bytes one sidedef record can occupy:
/// `%d (%d : %s / %s / %s ) %d : %s %d : %s %d %d %d` = 7 literal bytes
/// (`( : / / ) : :`) + 12 one-char tokens.
const MIN_SIDE_RECORD_BYTES: usize = 7 + 12;
/// Exact minimum bytes one thing record can occupy: `(%i,%i, %i) :%d, %d` =
/// 6 literal bytes (`( , , ) : ,`) + 5 one-char tokens.
const MIN_THING_RECORD_BYTES: usize = 6 + 5;

/// Capacity to reserve for `declared` records given `remaining` input bytes:
/// the declared count, but never more than the input could hold, so a hostile
/// header cannot trigger a huge allocation. `extra` is added headroom.
fn capped_capacity(
    declared: usize,
    remaining: usize,
    min_record_bytes: usize,
    extra: usize,
) -> usize {
    declared.min(remaining / min_record_bytes) + extra
}

/// Failure while parsing .dwd text. `line` fields are 1-based text line numbers.
#[derive(Debug)]
pub enum DwdError {
    BadHeader {
        line: usize,
        found: String,
    },
    UnsupportedVersion {
        version: i32,
    },
    BadRecord {
        line: usize,
        expected: &'static str,
        found: String,
    },
    BadName {
        line: usize,
        name: String,
    },
    UnexpectedEof {
        line: usize,
    },
    TrailingData {
        line: usize,
    },
}

impl fmt::Display for DwdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadHeader {
                line,
                found,
            } => {
                write!(f, "line {line}: expected `{DWD_HEADER}`, found {found:?}")
            }
            Self::UnsupportedVersion {
                version,
            } => {
                write!(
                    f,
                    "unsupported WorldServer version {version}, expected {DWD_VERSION}"
                )
            }
            Self::BadRecord {
                line,
                expected,
                found,
            } => {
                write!(f, "line {line}: expected {expected}, found {found:?}")
            }
            Self::BadName {
                line,
                name,
            } => write!(f, "line {line}: invalid name {name:?}"),
            Self::UnexpectedEof {
                line,
            } => write!(f, "line {line}: unexpected end of file"),
            Self::TrailingData {
                line,
            } => write!(f, "line {line}: trailing data after records"),
        }
    }
}

impl std::error::Error for DwdError {}

impl CursorError for DwdError {
    fn unexpected_eof(line: usize) -> Self {
        Self::UnexpectedEof {
            line,
        }
    }

    fn bad_token(line: usize, expected: &'static str, found: String) -> Self {
        Self::BadRecord {
            line,
            expected,
            found,
        }
    }

    fn bad_name(line: usize, name: String) -> Self {
        Self::BadName {
            line,
            name,
        }
    }
}

/// Deduplicating builders: vertices keyed by coordinate bits, sectors by full
/// record. Both push in first-encounter order, which fixes the output index
/// numbering (lookup-only maps; iteration order never matters).
struct Interner {
    vertices: Vec<Vertex>,
    vertex_ids: HashMap<(u32, u32), u32>,
    sectors: Vec<Sector>,
    sector_ids: HashMap<Sector, u32>,
}

impl Interner {
    /// `cap` is the already-capped reserve (see [`capped_capacity`]); each
    /// line contributes up to two vertices/sectors, so the caller doubles it.
    fn with_capacity(cap: usize) -> Self {
        Self {
            vertices: Vec::with_capacity(cap),
            vertex_ids: HashMap::with_capacity(cap),
            sectors: Vec::with_capacity(cap),
            sector_ids: HashMap::with_capacity(cap),
        }
    }

    fn vertex(&mut self, x: f32, y: f32) -> u32 {
        let key = (x.to_bits(), y.to_bits());
        if let Some(&id) = self.vertex_ids.get(&key) {
            return id;
        }
        let id = self.vertices.len() as u32;
        self.vertices.push(Vertex {
            x,
            y,
        });
        self.vertex_ids.insert(key, id);
        id
    }

    fn sector(&mut self, sector: Sector) -> u32 {
        if let Some(&id) = self.sector_ids.get(&sector) {
            return id;
        }
        let id = self.sectors.len() as u32;
        self.sectors.push(sector);
        self.sector_ids.insert(sector, id);
        id
    }
}

/// Parse .dwd text into an [`EditorMap`].
pub fn parse_dwd(text: &str) -> Result<EditorMap, DwdError> {
    let mut s = Cursor::<DwdError>::new(text);

    let header_line = {
        s.skip_ws();
        s.line
    };
    s.lit("WorldServer").map_err(|_| DwdError::BadHeader {
        line: header_line,
        found: s.found(),
    })?;
    s.lit("version").map_err(|_| DwdError::BadHeader {
        line: header_line,
        found: s.found(),
    })?;
    let version = s.int()?;
    if version != DWD_VERSION {
        return Err(DwdError::UnsupportedVersion {
            version,
        });
    }

    s.lit("lines:")?;
    let line_count = s.int()?.max(0) as usize;
    // Cap reserves against the remaining input so a hostile header count
    // cannot trigger a huge allocation; the loop still reads the true count
    // and errors with UnexpectedEof if records run out.
    let line_cap = capped_capacity(
        line_count,
        s.remaining(),
        MIN_LINE_RECORD_BYTES,
        GROWTH_HEADROOM,
    );
    let mut intern = Interner::with_capacity(2 * line_cap);
    let mut lines = Vec::with_capacity(line_cap);
    for _ in 0..line_count {
        lines.push(parse_line_record(&mut s, &mut intern)?);
    }

    s.lit("things:")?;
    let thing_count = s.int()?.max(0) as usize;
    let thing_cap = capped_capacity(
        thing_count,
        s.remaining(),
        MIN_THING_RECORD_BYTES,
        GROWTH_HEADROOM,
    );
    let mut things = Vec::with_capacity(thing_cap);
    for _ in 0..thing_count {
        things.push(parse_thing_record(&mut s)?);
    }

    if !s.at_eof() {
        return Err(DwdError::TrailingData {
            line: s.line,
        });
    }

    let dense = DenseMap {
        vertices: intern.vertices,
        lines,
        sectors: intern.sectors,
        things,
        required_wads: Vec::new(),
    };
    Ok(EditorMap::from_dense(dense).expect("interned refs are dense-valid"))
}

fn parse_line_record(
    s: &mut Cursor<'_, DwdError>,
    intern: &mut Interner,
) -> Result<DenseLineDef, DwdError> {
    s.lit("(")?;
    let x1 = s.float()?;
    s.lit(",")?;
    let y1 = s.float()?;
    s.lit(")")?;
    s.lit("to")?;
    s.lit("(")?;
    let x2 = s.float()?;
    s.lit(",")?;
    let y2 = s.float()?;
    s.lit(")")?;
    s.lit(":")?;
    let flags = s.int()?;
    s.lit(":")?;
    let special = s.int()?;
    s.lit(":")?;
    let tag = s.int()?;

    let flags = LineFlags::from_bits_retain(flags);
    let front = parse_side(s, intern)?;
    let back = if flags.contains(LineFlags::TWO_SIDED) {
        Some(parse_side(s, intern)?)
    } else {
        None
    };

    Ok(DenseLineDef {
        v1: intern.vertex(x1, y1),
        v2: intern.vertex(x2, y2),
        flags,
        special,
        tag,
        front,
        back,
    })
}

fn parse_side(
    s: &mut Cursor<'_, DwdError>,
    intern: &mut Interner,
) -> Result<DenseSideDef, DwdError> {
    let y_offset = s.int()?;
    s.lit("(")?;
    let x_offset = s.int()?;
    s.lit(":")?;
    let top_tex = s.name8()?;
    s.lit("/")?;
    let bottom_tex = s.name8()?;
    s.lit("/")?;
    let middle_tex = s.name8()?;
    s.lit(")")?;

    let floor_height = s.int()?;
    s.lit(":")?;
    let floor_flat = s.name8()?;
    let ceil_height = s.int()?;
    s.lit(":")?;
    let ceil_flat = s.name8()?;
    let light_level = s.int()?;
    let special = s.int()?;
    let tag = s.int()?;

    let sector = intern.sector(Sector {
        floor_height,
        floor_flat,
        ceil_height,
        ceil_flat,
        light_level,
        special,
        tag,
    });

    Ok(DenseSideDef {
        x_offset,
        y_offset,
        top_tex,
        bottom_tex,
        middle_tex,
        sector: Some(sector),
    })
}

fn parse_thing_record(s: &mut Cursor<'_, DwdError>) -> Result<Thing, DwdError> {
    s.lit("(")?;
    let x = s.int()?;
    s.lit(",")?;
    let y = s.int()?;
    s.lit(",")?;
    let angle = s.int()?;
    s.lit(")")?;
    s.lit(":")?;
    let kind = s.int()?;
    s.lit(",")?;
    let options = s.int()?;
    Ok(Thing {
        x: x & THING_SNAP_MASK,
        y: y & THING_SNAP_MASK,
        z: 0,
        angle,
        kind,
        options: ThingFlags::from_bits_retain(options),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../tests/fixtures/E1M1.dwd");

    /// Sector count produced by the byte-identical sectordef merge over the
    /// fixture. The shipped WAD has 85 SECTORS because doombsp re-splits
    /// merged sectordefs into connected components at save time; the merge
    /// stage itself yields 71 distinct records (cross-checked against the
    /// WAD's deduplicated sector values in wad_import).
    const FIXTURE_SECTOR_COUNT: usize = 71;

    fn fixture_map() -> EditorMap {
        parse_dwd(FIXTURE).expect("fixture must parse")
    }

    #[test]
    fn fixture_counts() {
        let map = fixture_map();
        assert_eq!(map.lines.len(), 475);
        assert_eq!(map.things.len(), 138);
        assert_eq!(map.lines.values().filter(|l| l.back.is_some()).count(), 173);
        assert_eq!(map.sectors.len(), FIXTURE_SECTOR_COUNT);
    }

    #[test]
    fn fixture_first_line_record_fields() {
        let map = fixture_map();
        let line = map.lines.values().next().expect("fixture has lines");
        let p1 = map.vertices[line.v1];
        let p2 = map.vertices[line.v2];
        assert_eq!((p1.x, p1.y), (1088.0, -3680.0));
        assert_eq!((p2.x, p2.y), (1024.0, -3680.0));
        assert_eq!(
            (line.flags, line.special, line.tag),
            (LineFlags::BLOCKING, 0, 0)
        );
        assert!(line.back.is_none());
        assert_eq!(line.front.middle_tex.as_str(), "DOOR3");
        assert!(line.front.top_tex.is_empty());
        let sec = map.sectors[line.front.sector.expect("dwd side has a sector")];
        assert_eq!(sec.floor_height, 0);
        assert_eq!(sec.floor_flat.as_str(), "FLOOR4_8");
        assert_eq!(sec.ceil_height, 72);
        assert_eq!(sec.ceil_flat.as_str(), "CEIL3_5");
        assert_eq!((sec.light_level, sec.special, sec.tag), (255, 1, 0));
    }

    #[test]
    fn fixture_two_sided_record_fields() {
        let map = fixture_map();
        let line = map
            .lines
            .values()
            .find(|l| {
                let p1 = map.vertices[l.v1];
                let p2 = map.vertices[l.v2];
                (p1.x, p1.y) == (496.0, -3160.0) && (p2.x, p2.y) == (496.0, -3304.0)
            })
            .expect("two-sided spot-check line exists in fixture");
        assert_eq!(line.flags, LineFlags::TWO_SIDED);
        assert_eq!(line.front.y_offset, 104);
        assert_eq!(line.front.x_offset, 0);
        assert_eq!(line.front.top_tex.as_str(), "STARG3");
        let back = line.back.as_ref().expect("flags say two-sided");
        assert!(
            back.top_tex.is_empty() && back.bottom_tex.is_empty() && back.middle_tex.is_empty()
        );
        let front_sec = map.sectors[line.front.sector.expect("front sector")];
        let back_sec = map.sectors[back.sector.expect("back sector")];
        assert_eq!(
            (front_sec.ceil_height, front_sec.ceil_flat.as_str()),
            (224, "FLOOR7_2")
        );
        assert_eq!(
            (back_sec.ceil_height, back_sec.ceil_flat.as_str()),
            (120, "CEIL3_5")
        );
        assert_ne!(line.front.sector, back.sector);
    }

    #[test]
    fn fixture_first_and_last_things() {
        let map = fixture_map();
        let first = *map.things.values().next().expect("fixture has things");
        let opts = ThingFlags::from_bits_retain(7);
        assert_eq!(
            (first.x, first.y, first.angle, first.kind, first.options),
            (1056, -3616, 90, 1, opts)
        );
        let last = *map.things.values().last().expect("fixture has things");
        assert_eq!(
            (last.x, last.y, last.angle, last.kind, last.options),
            (3648, -3840, 0, 2015, opts)
        );
    }

    /// Byte-identity holds for this fixture because: the writer templates
    /// match DoomEd's fprintf byte-for-byte; the sector merge/expand is
    /// lossless; every coordinate in the fixture is integral (f32→i32
    /// truncation is identity); and every thing coordinate is already
    /// 16-aligned (the load snap is identity). Inputs violating any of those
    /// round-trip semantically but not byte-identically.
    #[test]
    fn fixture_parses_to_expected_model() {
        // The hand-authored model matches what parsing the fixture yields.
        assert_eq!(parse_dwd(FIXTURE).expect("fixture parses"), fixture_map());
    }

    #[test]
    fn thing_snap_applies_on_load() {
        let text = "WorldServer version 4\n\nlines:0\n\nthings:1\n(1001,-7, 90) :1, 7\n";
        let map = parse_dwd(text).expect("snap fixture parses");
        let t = map.things.values().next().expect("one thing");
        assert_eq!((t.x, t.y), (992, -16));
    }

    #[test]
    fn bad_header_rejected() {
        let err = parse_dwd("MapServer version 4\n").expect_err("wrong header word");
        assert!(
            matches!(
                err,
                DwdError::BadHeader {
                    line: 1,
                    ..
                }
            ),
            "{err}"
        );
    }

    #[test]
    fn unsupported_version_rejected() {
        let err = parse_dwd("WorldServer version 3\n\nlines:0\n\nthings:0\n")
            .expect_err("version 3 unsupported");
        assert!(
            matches!(
                err,
                DwdError::UnsupportedVersion {
                    version: 3
                }
            ),
            "{err}"
        );
    }

    #[test]
    fn count_larger_than_records_is_eof() {
        let text = "WorldServer version 4\n\nlines:1\n";
        let err = parse_dwd(text).expect_err("no line records follow");
        assert!(
            matches!(
                err,
                DwdError::UnexpectedEof {
                    line: 4
                }
            ),
            "{err}"
        );
    }

    #[test]
    fn huge_header_count_does_not_overallocate() {
        // A hostile count far exceeding what the input could hold must error
        // (no records follow) without reserving billions of entries — the
        // pre-allocation is capped at remaining_bytes / MIN_RECORD_BYTES.
        let lines = "WorldServer version 4\n\nlines:2000000000\n";
        assert!(matches!(
            parse_dwd(lines).expect_err("no line records"),
            DwdError::UnexpectedEof { .. }
        ));
        let things = "WorldServer version 4\n\nlines:0\n\nthings:2000000000\n";
        assert!(matches!(
            parse_dwd(things).expect_err("no thing records"),
            DwdError::UnexpectedEof { .. }
        ));
        // The cap itself: a 2-billion declared count over 40 input bytes
        // reserves only ~2 records plus headroom.
        assert_eq!(
            capped_capacity(2_000_000_000, 40, MIN_LINE_RECORD_BYTES, GROWTH_HEADROOM),
            40 / MIN_LINE_RECORD_BYTES + GROWTH_HEADROOM
        );
    }

    #[test]
    fn truncated_side_record_is_eof() {
        let text = "WorldServer version 4\n\nlines:1\n(0,0) to (64,0) : 1 : 0 : 0\n    0 (0 : - / - / DOOR3 )\n";
        let err = parse_dwd(text).expect_err("sectordef line missing");
        assert!(
            matches!(
                err,
                DwdError::UnexpectedEof {
                    line: 6
                }
            ),
            "{err}"
        );
    }

    #[test]
    fn nine_char_name_rejected_with_line() {
        let text = "WorldServer version 4\n\nlines:1\n(0,0) to (64,0) : 1 : 0 : 0\n    0 (0 : - / - / WAYTOLONG )\n    0 : FLOOR4_8 72 : CEIL3_5 255 0 0\n\nthings:0\n";
        let err = parse_dwd(text).expect_err("9-char texture name");
        match err {
            DwdError::BadName {
                line,
                name,
            } => {
                assert_eq!(line, 5);
                assert_eq!(name, "WAYTOLONG");
            }
            other => panic!("expected BadName, got {other}"),
        }
    }

    #[test]
    fn non_numeric_field_rejected() {
        let text = "WorldServer version 4\n\nlines:1\n(a,0) to (64,0) : 1 : 0 : 0\n";
        let err = parse_dwd(text).expect_err("letter where number expected");
        assert!(
            matches!(
                err,
                DwdError::BadRecord {
                    line: 4,
                    expected: "number",
                    ..
                }
            ),
            "{err}"
        );
    }

    #[test]
    fn trailing_junk_rejected() {
        let text = "WorldServer version 4\n\nlines:0\n\nthings:0\nleftover\n";
        let err = parse_dwd(text).expect_err("junk after records");
        assert!(
            matches!(
                err,
                DwdError::TrailingData {
                    line: 6
                }
            ),
            "{err}"
        );
    }

    #[test]
    fn count_smaller_than_records_rejected() {
        let text = "WorldServer version 4\n\nlines:0\n(0,0) to (64,0) : 1 : 0 : 0\n";
        let err = parse_dwd(text).expect_err("record where things: expected");
        assert!(
            matches!(
                err,
                DwdError::BadRecord {
                    line: 4,
                    expected: "things:",
                    ..
                }
            ),
            "{err}"
        );
    }
}
