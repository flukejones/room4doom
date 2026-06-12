//! Structural integrity checks over an [`EditorMap`].
//!
//! Returns every problem found rather than failing fast; never panics. Name
//! length violations cannot occur here — [`crate::name8::Name8`] makes them
//! unrepresentable, so they surface as parse/import errors instead.

use std::fmt;

use crate::LineFlags;
use crate::model::EditorMap;

/// Which endpoint of a line an issue refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhichVertex {
    V1,
    V2,
}

/// Which side of a line an issue refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhichSide {
    Front,
    Back,
}

/// A structural problem in a map. `line` is the index into
/// [`EditorMap::lines`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Issue {
    /// A line endpoint index is outside the vertex array.
    DanglingVertex {
        line: usize,
        which: WhichVertex,
        index: u32,
    },
    /// A side's sector index is outside the sector array.
    DanglingSector {
        line: usize,
        side: WhichSide,
        index: u32,
    },
    /// A side faces the void (no sector) — the line is not part of an enclosed
    /// sector, which a Doom map cannot export.
    UnenclosedSide { line: usize, side: WhichSide },
    /// Both endpoints are the same vertex, or distinct vertices with
    /// bit-identical coordinates.
    DegenerateLine { line: usize },
    /// `TWO_SIDED` is set but the line has no back side; the .dwd writer
    /// rejects such a line.
    TwoSidedWithoutBack { line: usize },
    /// A back side exists but `TWO_SIDED` is clear; the .dwd writer
    /// silently drops the back side (DoomEd behavior).
    BackWithoutTwoSidedFlag { line: usize },
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DanglingVertex {
                line,
                which,
                index,
            } => {
                write!(
                    f,
                    "line {line}: {which:?} vertex index {index} out of range"
                )
            }
            Self::DanglingSector {
                line,
                side,
                index,
            } => {
                write!(f, "line {line}: {side:?} sector index {index} out of range")
            }
            Self::UnenclosedSide {
                line,
                side,
            } => {
                write!(f, "line {line}: {side:?} side faces the void (no sector)")
            }
            Self::DegenerateLine {
                line,
            } => write!(f, "line {line}: zero-length line"),
            Self::TwoSidedWithoutBack {
                line,
            } => {
                write!(f, "line {line}: TWO_SIDED set but no back side")
            }
            Self::BackWithoutTwoSidedFlag {
                line,
            } => {
                write!(f, "line {line}: back side present but TWO_SIDED clear")
            }
        }
    }
}

/// Flag a side's sector reference: out-of-range index, or a void (`None`) side.
fn check_side_sector(
    sector: Option<u32>,
    line: usize,
    side: WhichSide,
    map: &EditorMap,
    issues: &mut Vec<Issue>,
) {
    match sector {
        Some(s) if (s as usize) >= map.sectors.len() => issues.push(Issue::DanglingSector {
            line,
            side,
            index: s,
        }),
        Some(_) => {}
        None => issues.push(Issue::UnenclosedSide {
            line,
            side,
        }),
    }
}

pub fn validate(map: &EditorMap) -> Vec<Issue> {
    let mut issues = Vec::new();
    for (i, line) in map.lines.iter().enumerate() {
        let v1_ok = (line.v1 as usize) < map.vertices.len();
        let v2_ok = (line.v2 as usize) < map.vertices.len();
        if !v1_ok {
            issues.push(Issue::DanglingVertex {
                line: i,
                which: WhichVertex::V1,
                index: line.v1,
            });
        }
        if !v2_ok {
            issues.push(Issue::DanglingVertex {
                line: i,
                which: WhichVertex::V2,
                index: line.v2,
            });
        }
        if v1_ok && v2_ok {
            let p1 = map.vertices[line.v1 as usize];
            let p2 = map.vertices[line.v2 as usize];
            let same_index = line.v1 == line.v2;
            let same_coords = p1.x.to_bits() == p2.x.to_bits() && p1.y.to_bits() == p2.y.to_bits();
            if same_index || same_coords {
                issues.push(Issue::DegenerateLine {
                    line: i,
                });
            }
        }

        check_side_sector(line.front.sector, i, WhichSide::Front, map, &mut issues);
        if let Some(back) = &line.back {
            check_side_sector(back.sector, i, WhichSide::Back, map, &mut issues);
        }

        let two_sided = line.flags.contains(LineFlags::TWO_SIDED);
        if two_sided && line.back.is_none() {
            issues.push(Issue::TwoSidedWithoutBack {
                line: i,
            });
        }
        if !two_sided && line.back.is_some() {
            issues.push(Issue::BackWithoutTwoSidedFlag {
                line: i,
            });
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{LineDef, Sector, SideDef, Vertex};
    use crate::name8::Name8;

    fn side(sector: u32) -> SideDef {
        SideDef {
            x_offset: 0,
            y_offset: 0,
            top_tex: Name8::EMPTY,
            bottom_tex: Name8::EMPTY,
            middle_tex: Name8::EMPTY,
            sector: Some(sector),
        }
    }

    fn sector() -> Sector {
        Sector {
            floor_height: 0,
            floor_flat: Name8::EMPTY,
            ceil_height: 128,
            ceil_flat: Name8::EMPTY,
            light_level: 255,
            special: 0,
            tag: 0,
        }
    }

    fn line(v1: u32, v2: u32, flags: LineFlags, back: Option<SideDef>) -> LineDef {
        LineDef {
            v1,
            v2,
            flags,
            special: 0,
            tag: 0,
            front: side(0),
            back,
        }
    }

    fn triangle() -> EditorMap {
        EditorMap {
            vertices: vec![
                Vertex {
                    x: 0.0,
                    y: 0.0,
                },
                Vertex {
                    x: 64.0,
                    y: 0.0,
                },
                Vertex {
                    x: 0.0,
                    y: 64.0,
                },
            ],
            lines: vec![
                line(0, 1, LineFlags::BLOCKING, None),
                line(1, 2, LineFlags::BLOCKING, None),
                line(2, 0, LineFlags::BLOCKING, None),
            ],
            sectors: vec![sector()],
            things: Vec::new(),
            required_wads: Vec::new(),
        }
    }

    #[test]
    fn clean_triangle_has_no_issues() {
        assert_eq!(validate(&triangle()), Vec::new());
    }

    #[test]
    fn dangling_vertex_reported() {
        let mut map = triangle();
        map.lines[0].v2 = 99;
        assert_eq!(
            validate(&map),
            vec![Issue::DanglingVertex {
                line: 0,
                which: WhichVertex::V2,
                index: 99
            }]
        );
    }

    #[test]
    fn dangling_sector_on_back_side_reported() {
        let mut map = triangle();
        map.lines[1].flags.insert(LineFlags::TWO_SIDED);
        map.lines[1].back = Some(side(7));
        assert_eq!(
            validate(&map),
            vec![Issue::DanglingSector {
                line: 1,
                side: WhichSide::Back,
                index: 7
            }]
        );
    }

    #[test]
    fn degenerate_line_by_index_and_by_coords() {
        let mut map = triangle();
        map.lines[0].v2 = 0;
        assert_eq!(
            validate(&map),
            vec![Issue::DegenerateLine {
                line: 0
            }]
        );

        let mut map = triangle();
        map.vertices.push(Vertex {
            x: 0.0,
            y: 0.0,
        });
        map.lines[0].v2 = 3;
        assert_eq!(
            validate(&map),
            vec![Issue::DegenerateLine {
                line: 0
            }]
        );
    }

    #[test]
    fn two_sided_flag_mismatches_reported() {
        let mut map = triangle();
        map.lines[0].flags = LineFlags::TWO_SIDED;
        assert_eq!(
            validate(&map),
            vec![Issue::TwoSidedWithoutBack {
                line: 0
            }]
        );

        let mut map = triangle();
        map.lines[2].back = Some(side(0));
        assert_eq!(
            validate(&map),
            vec![Issue::BackWithoutTwoSidedFlag {
                line: 2
            }]
        );
    }
}
