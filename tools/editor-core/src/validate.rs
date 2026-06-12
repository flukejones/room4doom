//! Structural integrity checks over an [`EditorMap`]: returns every problem found rather than failing fast, never panics. Dangling index references cannot occur — keys are generation-checked, so a broken reference surfaces as [`Issue::StaleRef`] (a kernel bug, not map data).

use std::fmt;

use crate::LineFlags;
use crate::model::{EditorMap, LineKey};

/// Which side of a line an issue refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhichSide {
    Front,
    Back,
}

/// A structural problem in a map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Issue {
    /// A side faces the void (no sector) — the line is not part of an enclosed sector, which a Doom map cannot export.
    UnenclosedSide { line: LineKey, side: WhichSide },
    /// Both endpoints are the same vertex, or distinct vertices with bit-identical coordinates.
    DegenerateLine { line: LineKey },
    /// `TWO_SIDED` is set but the line has no back side; the .dwd writer rejects such a line.
    TwoSidedWithoutBack { line: LineKey },
    /// A back side exists but `TWO_SIDED` is clear; the .dwd writer silently drops the back side (DoomEd behavior).
    BackWithoutTwoSidedFlag { line: LineKey },
    /// A vertex or sector reference resolves to nothing (stale key).
    StaleRef { line: LineKey },
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnenclosedSide {
                line,
                side,
            } => {
                write!(f, "line {line:?}: {side:?} side faces the void (no sector)")
            }
            Self::DegenerateLine {
                line,
            } => write!(f, "line {line:?}: zero-length line"),
            Self::TwoSidedWithoutBack {
                line,
            } => {
                write!(f, "line {line:?}: TWO_SIDED set but no back side")
            }
            Self::BackWithoutTwoSidedFlag {
                line,
            } => {
                write!(f, "line {line:?}: back side present but TWO_SIDED clear")
            }
            Self::StaleRef {
                line,
            } => write!(f, "line {line:?}: stale vertex/sector reference"),
        }
    }
}

pub fn validate(map: &EditorMap) -> Vec<Issue> {
    let mut issues = Vec::new();
    for (k, line) in map.lines.iter() {
        let (p1, p2) = (map.vertices.get(line.v1), map.vertices.get(line.v2));
        let sectors_ok = line
            .sides()
            .all(|s| s.sector.is_none_or(|key| map.sectors.contains(key)));
        let (Some(p1), Some(p2)) = (p1, p2) else {
            issues.push(Issue::StaleRef {
                line: k,
            });
            continue;
        };
        if !sectors_ok {
            issues.push(Issue::StaleRef {
                line: k,
            });
            continue;
        }

        let same_key = line.v1 == line.v2;
        let same_coords = p1.x.to_bits() == p2.x.to_bits() && p1.y.to_bits() == p2.y.to_bits();
        if same_key || same_coords {
            issues.push(Issue::DegenerateLine {
                line: k,
            });
        }

        if line.front.sector.is_none() {
            issues.push(Issue::UnenclosedSide {
                line: k,
                side: WhichSide::Front,
            });
        }
        if let Some(back) = &line.back
            && back.sector.is_none()
        {
            issues.push(Issue::UnenclosedSide {
                line: k,
                side: WhichSide::Back,
            });
        }

        let two_sided = line.flags.contains(LineFlags::TWO_SIDED);
        if two_sided && line.back.is_none() {
            issues.push(Issue::TwoSidedWithoutBack {
                line: k,
            });
        }
        if !two_sided && line.back.is_some() {
            issues.push(Issue::BackWithoutTwoSidedFlag {
                line: k,
            });
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DenseLineDef, DenseMap, DenseSideDef, Sector, Vertex};
    use crate::name8::Name8;

    fn dside(sector: Option<u32>) -> DenseSideDef {
        DenseSideDef {
            x_offset: 0,
            y_offset: 0,
            top_tex: Name8::EMPTY,
            bottom_tex: Name8::EMPTY,
            middle_tex: Name8::EMPTY,
            sector,
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

    fn dline(v1: u32, v2: u32, flags: LineFlags, back: Option<DenseSideDef>) -> DenseLineDef {
        DenseLineDef {
            v1,
            v2,
            flags,
            special: 0,
            tag: 0,
            front: dside(Some(0)),
            back,
        }
    }

    fn triangle() -> EditorMap {
        EditorMap::from_dense(DenseMap {
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
                dline(0, 1, LineFlags::BLOCKING, None),
                dline(1, 2, LineFlags::BLOCKING, None),
                dline(2, 0, LineFlags::BLOCKING, None),
            ],
            sectors: vec![sector()],
            things: Vec::new(),
            required_wads: Vec::new(),
        })
        .expect("triangle refs valid")
    }

    fn line_keys(map: &EditorMap) -> Vec<LineKey> {
        map.lines.keys().collect()
    }

    #[test]
    fn clean_triangle_has_no_issues() {
        assert_eq!(validate(&triangle()), Vec::new());
    }

    #[test]
    fn degenerate_line_by_key_and_by_coords() {
        let mut map = triangle();
        let k = line_keys(&map);
        let v1 = map.lines[k[0]].v1;
        map.lines[k[0]].v2 = v1;
        assert_eq!(
            validate(&map),
            vec![Issue::DegenerateLine {
                line: k[0]
            }]
        );

        let mut map = triangle();
        let k = line_keys(&map);
        let dup = map.vertices.insert(Vertex {
            x: 0.0,
            y: 0.0,
        });
        let first_v1 = map.lines[k[0]].v1;
        let p = map.vertices[first_v1];
        map.vertices[dup] = p;
        map.lines[k[0]].v2 = dup;
        assert_eq!(
            validate(&map),
            vec![Issue::DegenerateLine {
                line: k[0]
            }]
        );
    }

    #[test]
    fn unenclosed_side_reported() {
        let mut map = triangle();
        let k = line_keys(&map);
        map.lines[k[1]].front.sector = None;
        assert_eq!(
            validate(&map),
            vec![Issue::UnenclosedSide {
                line: k[1],
                side: WhichSide::Front
            }]
        );
    }

    #[test]
    fn stale_sector_ref_reported() {
        let mut map = triangle();
        let k = line_keys(&map);
        let s = map.sectors.keys().next().expect("one sector");
        map.sectors.remove(s);
        // Every line's front now references a removed sector.
        assert_eq!(
            validate(&map),
            k.iter()
                .map(|&line| Issue::StaleRef {
                    line
                })
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn two_sided_flag_mismatches_reported() {
        let mut map = triangle();
        let k = line_keys(&map);
        map.lines[k[0]].flags = LineFlags::TWO_SIDED;
        assert_eq!(
            validate(&map),
            vec![Issue::TwoSidedWithoutBack {
                line: k[0]
            }]
        );

        let mut map = triangle();
        let k = line_keys(&map);
        let front = map.lines[k[2]].front;
        map.lines[k[2]].back = Some(front);
        assert_eq!(
            validate(&map),
            vec![Issue::BackWithoutTwoSidedFlag {
                line: k[2]
            }]
        );
    }
}
