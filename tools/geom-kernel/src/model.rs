//! The editable map model: keyed arenas ([`Arena`]) with stable generational references ([`VertKey`], [`SectorKey`]) so removals never renumber survivors and a stale reference resolves to `None`; field widths are `i32` (lossless superset of every WAD `i16`/`u16`, range-checked at WAD export). Serde on [`EditorMap`] is the exact binary snapshot form (undo); the on-disk `.ron` format and all lump I/O speak [`DenseMap`] (flat Vec + u32-index) via [`EditorMap::to_dense`]/[`EditorMap::from_dense`].

use std::collections::HashMap;
use std::collections::HashSet;
use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::arena::Arena;
use crate::arena_key;
use crate::flags::{LineFlags, ThingFlags};
use crate::name8::Name8;

/// Spare capacity reserved when building dense element lists at import.
pub const GROWTH_HEADROOM: usize = 256;

arena_key!(
    /// Stable reference to a map vertex.
    VertKey
);
arena_key!(
    /// Stable reference to a linedef.
    LineKey
);
arena_key!(
    /// Stable reference to a sector.
    SectorKey
);
arena_key!(
    /// Stable reference to a thing.
    ThingKey
);

/// A map vertex in world units. DoomEd edits in floats and truncates on save.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Vertex {
    pub x: f32,
    pub y: f32,
}

/// One side of a line: texture slots, offsets, and the faced sector.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SideDef {
    pub x_offset: i32,
    pub y_offset: i32,
    pub top_tex: Name8,
    pub bottom_tex: Name8,
    pub middle_tex: Name8,
    pub sector: Option<SectorKey>,
}

/// A line between two vertices with one or two sides.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LineDef {
    pub v1: VertKey,
    pub v2: VertKey,
    pub flags: LineFlags,
    pub special: i32,
    pub tag: i32,
    pub front: SideDef,
    /// Present when the line is two-sided; kept in sync with [`LineFlags::TWO_SIDED`].
    pub back: Option<SideDef>,
}

impl LineDef {
    /// Front side, then the back side when present.
    pub fn sides(&self) -> impl Iterator<Item = &SideDef> {
        [Some(&self.front), self.back.as_ref()]
            .into_iter()
            .flatten()
    }

    /// Overwrite the editable fields from `new`, keeping this line's endpoints and syncing `back` to the two-sided flag.
    pub fn overwrite_fields(&mut self, new: Self) {
        let (v1, v2) = (self.v1, self.v2);
        *self = new;
        self.v1 = v1;
        self.v2 = v2;
        if self.flags.contains(LineFlags::TWO_SIDED) && self.back.is_none() {
            self.back = Some(self.front);
        }
        if !self.flags.contains(LineFlags::TWO_SIDED) {
            self.back = None;
        }
    }
}

/// A sector's surfaces, lighting, and behavior tags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Sector {
    pub floor_height: i32,
    pub floor_flat: Name8,
    pub ceil_height: i32,
    pub ceil_flat: Name8,
    pub light_level: i32,
    pub special: i32,
    pub tag: i32,
}

/// A placed map object (player start, monster, item, decoration); fully 3D — `z` is the world floor height the thing sits at, set on placement/import and maintained on drags and sector-floor edits.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Thing {
    pub x: i32,
    pub y: i32,
    /// World floor height (Doom units); absent in old saves → re-derived on load.
    #[serde(default)]
    pub z: i32,
    /// Facing in degrees, 0..360.
    pub angle: i32,
    /// Doom thing type number.
    pub kind: i32,
    /// Difficulty/ambush/multiplayer bits.
    pub options: ThingFlags,
}

/// A complete editable map, keyed; serde = exact snapshot (undo), disk I/O via [`DenseMap`].
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorMap {
    pub vertices: Arena<VertKey, Vertex>,
    pub lines: Arena<LineKey, LineDef>,
    pub sectors: Arena<SectorKey, Sector>,
    pub things: Arena<ThingKey, Thing>,
    /// Basenames of the WADs this map was authored against (IWAD + PWADs), captured at save so a shared map declares its resource dependencies.
    pub required_wads: Vec<String>,
}

impl EditorMap {
    /// Key of the vertex bit-equal to `p`, or a freshly inserted one; callers snap `p` first, equality is exact, not epsilon-based.
    pub fn find_or_add_vertex(&mut self, p: [f32; 2]) -> VertKey {
        for (k, v) in self.vertices.iter() {
            if v.x.to_bits() == p[0].to_bits() && v.y.to_bits() == p[1].to_bits() {
                return k;
            }
        }
        self.vertices.insert(Vertex {
            x: p[0],
            y: p[1],
        })
    }

    /// Remove vertices no line references. Returns the number removed.
    pub fn prune_orphan_vertices(&mut self) -> usize {
        let used: HashSet<VertKey> = self.lines.values().flat_map(|l| [l.v1, l.v2]).collect();
        self.vertices.retain(|k, _| used.contains(&k))
    }

    /// Remove sectors no line side references. Returns the number removed.
    pub fn prune_unused_sectors(&mut self) -> usize {
        let used: HashSet<SectorKey> = self
            .lines
            .values()
            .flat_map(|l| [l.front.sector, l.back.and_then(|b| b.sector)])
            .flatten()
            .collect();
        self.sectors.retain(|k, _| used.contains(&k))
    }

    /// Remove lines by key (stale keys ignored), then prune orphaned vertices.
    pub fn remove_lines(&mut self, keys: &[LineKey]) {
        for &k in keys {
            self.lines.remove(k);
        }
        self.prune_orphan_vertices();
    }

    /// Remove things by key; stale keys are ignored.
    pub fn remove_things(&mut self, keys: &[ThingKey]) {
        for &k in keys {
            self.things.remove(k);
        }
    }

    /// Flatten to the on-disk shape: elements in slot order, references as dense list positions. Panics on a dangling reference (kernel invariant).
    pub fn to_dense(&self) -> DenseMap {
        let vert_pos: HashMap<VertKey, u32> = self
            .vertices
            .keys()
            .enumerate()
            .map(|(i, k)| (k, i as u32))
            .collect();
        let sector_pos: HashMap<SectorKey, u32> = self
            .sectors
            .keys()
            .enumerate()
            .map(|(i, k)| (k, i as u32))
            .collect();
        let side = |s: &SideDef| DenseSideDef {
            x_offset: s.x_offset,
            y_offset: s.y_offset,
            top_tex: s.top_tex,
            bottom_tex: s.bottom_tex,
            middle_tex: s.middle_tex,
            sector: s
                .sector
                .map(|k| *sector_pos.get(&k).expect("side references a live sector")),
        };
        DenseMap {
            vertices: self.vertices.values().copied().collect(),
            lines: self
                .lines
                .values()
                .map(|l| DenseLineDef {
                    v1: *vert_pos.get(&l.v1).expect("line references a live vertex"),
                    v2: *vert_pos.get(&l.v2).expect("line references a live vertex"),
                    flags: l.flags,
                    special: l.special,
                    tag: l.tag,
                    front: side(&l.front),
                    back: l.back.as_ref().map(&side),
                })
                .collect(),
            sectors: self.sectors.values().copied().collect(),
            things: self.things.values().copied().collect(),
            required_wads: self.required_wads.clone(),
        }
    }

    /// Build a keyed map from the on-disk shape; errors on out-of-range references.
    pub fn from_dense(dense: DenseMap) -> Result<Self, DenseError> {
        let mut map = Self {
            required_wads: dense.required_wads,
            ..Self::default()
        };
        let vert_keys: Vec<VertKey> = dense
            .vertices
            .into_iter()
            .map(|v| map.vertices.insert(v))
            .collect();
        let sector_keys: Vec<SectorKey> = dense
            .sectors
            .into_iter()
            .map(|s| map.sectors.insert(s))
            .collect();
        for t in dense.things {
            map.things.insert(t);
        }
        for (i, l) in dense.lines.into_iter().enumerate() {
            let vert = |v: u32| {
                vert_keys
                    .get(v as usize)
                    .copied()
                    .ok_or(DenseError::BadVertexRef {
                        line: i,
                        index: v,
                    })
            };
            let side = |s: DenseSideDef| -> Result<SideDef, DenseError> {
                let sector = match s.sector {
                    Some(v) => Some(sector_keys.get(v as usize).copied().ok_or(
                        DenseError::BadSectorRef {
                            line: i,
                            index: v,
                        },
                    )?),
                    None => None,
                };
                Ok(SideDef {
                    x_offset: s.x_offset,
                    y_offset: s.y_offset,
                    top_tex: s.top_tex,
                    bottom_tex: s.bottom_tex,
                    middle_tex: s.middle_tex,
                    sector,
                })
            };
            map.lines.insert(LineDef {
                v1: vert(l.v1)?,
                v2: vert(l.v2)?,
                flags: l.flags,
                special: l.special,
                tag: l.tag,
                front: side(l.front)?,
                back: l.back.map(side).transpose()?,
            });
        }
        Ok(map)
    }
}

/// On-disk side shape: `sector` is a dense list position.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DenseSideDef {
    pub x_offset: i32,
    pub y_offset: i32,
    pub top_tex: Name8,
    pub bottom_tex: Name8,
    pub middle_tex: Name8,
    pub sector: Option<u32>,
}

/// On-disk line shape: `v1`/`v2` are dense list positions.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DenseLineDef {
    pub v1: u32,
    pub v2: u32,
    pub flags: LineFlags,
    pub special: i32,
    pub tag: i32,
    pub front: DenseSideDef,
    pub back: Option<DenseSideDef>,
}

/// The flat Vec + index map shape: the `.ron` disk format (unchanged from the pre-arena model, so existing maps load) and the lump-I/O interchange form.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct DenseMap {
    pub vertices: Vec<Vertex>,
    pub lines: Vec<DenseLineDef>,
    pub sectors: Vec<Sector>,
    pub things: Vec<Thing>,
    #[serde(default)]
    pub required_wads: Vec<String>,
}

/// A dense reference pointing outside its list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenseError {
    BadVertexRef { line: usize, index: u32 },
    BadSectorRef { line: usize, index: u32 },
}

impl fmt::Display for DenseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadVertexRef {
                line,
                index,
            } => write!(f, "line {line}: vertex index {index} out of range"),
            Self::BadSectorRef {
                line,
                index,
            } => write!(f, "line {line}: sector index {index} out of range"),
        }
    }
}

impl Error for DenseError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn side(sector: Option<SectorKey>) -> SideDef {
        SideDef {
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
            light_level: 160,
            special: 0,
            tag: 0,
        }
    }

    fn map_with_line() -> (EditorMap, LineKey) {
        let mut map = EditorMap::default();
        let s = map.sectors.insert(sector());
        let v1 = map.vertices.insert(Vertex {
            x: 0.0,
            y: 0.0,
        });
        let v2 = map.vertices.insert(Vertex {
            x: 64.0,
            y: 0.0,
        });
        let line = map.lines.insert(LineDef {
            v1,
            v2,
            flags: LineFlags::BLOCKING,
            special: 0,
            tag: 0,
            front: side(Some(s)),
            back: None,
        });
        (map, line)
    }

    #[test]
    fn find_or_add_vertex_reuses_exact_match() {
        let mut map = EditorMap::default();
        let a = map.find_or_add_vertex([16.0, -32.0]);
        let b = map.find_or_add_vertex([16.0, -32.0]);
        assert_eq!(a, b);
        assert_eq!(map.vertices.len(), 1);
        let c = map.find_or_add_vertex([16.0, -33.0]);
        assert_ne!(a, c);
    }

    #[test]
    fn prune_orphans_keeps_referenced_keys_valid() {
        let (mut map, line) = map_with_line();
        let orphan = map.vertices.insert(Vertex {
            x: 99.0,
            y: 99.0,
        });
        assert_eq!(map.prune_orphan_vertices(), 1);
        assert!(!map.vertices.contains(orphan));
        let l = map.lines[line];
        assert!(map.vertices.contains(l.v1) && map.vertices.contains(l.v2));
    }

    #[test]
    fn remove_lines_prunes_and_ignores_stale() {
        let (mut map, line) = map_with_line();
        map.remove_lines(&[line, line]);
        assert!(map.lines.is_empty());
        assert!(map.vertices.is_empty(), "endpoints pruned");
    }

    #[test]
    fn dense_round_trip_preserves_structure() {
        let (map, _) = map_with_line();
        let dense = map.to_dense();
        assert_eq!(dense.lines[0].v1, 0);
        assert_eq!(dense.lines[0].v2, 1);
        assert_eq!(dense.lines[0].front.sector, Some(0));
        let back = EditorMap::from_dense(dense.clone()).expect("valid refs");
        assert_eq!(back.to_dense(), dense);
    }

    #[test]
    fn from_dense_rejects_bad_refs() {
        let dense = DenseMap {
            vertices: vec![Vertex {
                x: 0.0,
                y: 0.0,
            }],
            lines: vec![DenseLineDef {
                v1: 0,
                v2: 7,
                flags: LineFlags::empty(),
                special: 0,
                tag: 0,
                front: DenseSideDef {
                    x_offset: 0,
                    y_offset: 0,
                    top_tex: Name8::EMPTY,
                    bottom_tex: Name8::EMPTY,
                    middle_tex: Name8::EMPTY,
                    sector: None,
                },
                back: None,
            }],
            ..Default::default()
        };
        assert!(matches!(
            EditorMap::from_dense(dense),
            Err(DenseError::BadVertexRef {
                line: 0,
                index: 7
            })
        ));
    }

    #[test]
    fn dense_ron_matches_pre_arena_field_names() {
        let (map, _) = map_with_line();
        let text = ron::to_string(&map.to_dense()).expect("serializes");
        for field in [
            "vertices", "lines", "sectors", "things", "v1", "front", "sector",
        ] {
            assert!(text.contains(field), "missing field {field}: {text}");
        }
    }
}
