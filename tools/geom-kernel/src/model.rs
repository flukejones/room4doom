//! The editable map model: first-class sectors referenced by index.
//!
//! Unlike DoomEd's in-memory format (a full sectordef copied onto every line
//! side), sectors here are unique records and sides hold indices into
//! [`EditorMap::sectors`]. The .dwd reader merges identical per-side sectordefs
//! into this form; the writer expands them back. Field widths are `i32`, a
//! lossless superset of every WAD `i16`/`u16` field — range checks happen at
//! WAD export, not here.
//!
//! Serde derives back exact binary snapshots (undo) and the native RON map
//! format (see [`crate::map_ron`]); the `.dwd` import format has its own parser.

use serde::{Deserialize, Serialize};

use crate::flags::{LineFlags, ThingFlags};
use crate::name8::Name8;

/// Spare capacity reserved on load so interactive edits do not reallocate.
pub const GROWTH_HEADROOM: usize = 256;

/// A map vertex in world units. DoomEd edits in floats and truncates to
/// integers on save.
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
    pub sector: Option<u32>,
}

/// A line between two vertices with one or two sides.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct LineDef {
    /// Index into [`EditorMap::vertices`].
    pub v1: u32,
    /// Index into [`EditorMap::vertices`].
    pub v2: u32,
    pub flags: LineFlags,
    pub special: i32,
    pub tag: i32,
    pub front: SideDef,
    /// Present when the line is two-sided; serialized only while
    /// [`LineFlags::TWO_SIDED`] is set in `flags`.
    pub back: Option<SideDef>,
}

impl LineDef {
    /// Overwrite the editable fields from `new`, keeping this line's endpoints,
    /// then sync `back` to the two-sided flag (a newly two-sided line mirrors
    /// its front; clearing the flag drops the back).
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

/// A placed map object (player start, monster, item, decoration).
///
/// Fully 3D: `z` is the world floor height the thing sits at, set when the thing
/// is placed/imported and maintained on drag and sector-floor edits. The WAD
/// format has no thing Z; it is derived from the containing sector on import.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Thing {
    pub x: i32,
    pub y: i32,
    /// World floor height the thing sits at (Doom units). Absent in old saves and
    /// the WAD format → defaults to 0, then re-derived from the sector on load.
    #[serde(default)]
    pub z: i32,
    /// Facing in degrees, 0..360.
    pub angle: i32,
    /// Doom thing type number.
    pub kind: i32,
    /// Difficulty/ambush/multiplayer bits.
    pub options: ThingFlags,
}

/// A complete editable map.
#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EditorMap {
    pub vertices: Vec<Vertex>,
    pub lines: Vec<LineDef>,
    pub sectors: Vec<Sector>,
    pub things: Vec<Thing>,
    /// Basenames of the WADs this map was authored against (IWAD + PWADs),
    /// captured at save. Lets a shared map file declare its texture/flat/patch
    /// dependencies without the project. `#[serde(default)]` so older maps load.
    #[serde(default)]
    pub required_wads: Vec<String>,
}

impl EditorMap {
    /// Index of the vertex bit-equal to `p`, or a freshly pushed one.
    /// Callers snap `p` first; equality is exact, not epsilon-based.
    pub fn find_or_add_vertex(&mut self, p: [f32; 2]) -> u32 {
        for (i, v) in self.vertices.iter().enumerate() {
            if v.x.to_bits() == p[0].to_bits() && v.y.to_bits() == p[1].to_bits() {
                return i as u32;
            }
        }
        self.vertices.push(Vertex {
            x: p[0],
            y: p[1],
        });
        (self.vertices.len() - 1) as u32
    }

    /// Remove vertices no line references; line indices are remapped.
    /// Returns the number removed.
    pub fn prune_orphan_vertices(&mut self) -> usize {
        let mut used = vec![false; self.vertices.len()];
        for line in &self.lines {
            used[line.v1 as usize] = true;
            used[line.v2 as usize] = true;
        }

        let mut remap = vec![u32::MAX; self.vertices.len()];
        let mut kept = 0u32;
        for (i, &is_used) in used.iter().enumerate() {
            if is_used {
                remap[i] = kept;
                kept += 1;
            }
        }
        let removed = self.vertices.len() - kept as usize;
        if removed == 0 {
            return 0;
        }

        let mut keep = used.iter();
        self.vertices
            .retain(|_| *keep.next().expect("used parallels vertices"));

        for line in &mut self.lines {
            line.v1 = remap[line.v1 as usize];
            line.v2 = remap[line.v2 as usize];
        }
        removed
    }

    /// Remove sectors no line side references; side indices are remapped.
    /// Returns the number removed.
    pub fn prune_unused_sectors(&mut self) -> usize {
        let mut used = vec![false; self.sectors.len()];
        for line in &self.lines {
            if let Some(s) = line.front.sector {
                used[s as usize] = true;
            }
            if let Some(s) = line.back.and_then(|b| b.sector) {
                used[s as usize] = true;
            }
        }

        let mut remap = vec![u32::MAX; self.sectors.len()];
        let mut kept = 0u32;
        for (i, &is_used) in used.iter().enumerate() {
            if is_used {
                remap[i] = kept;
                kept += 1;
            }
        }
        let removed = self.sectors.len() - kept as usize;
        if removed == 0 {
            return 0;
        }

        let mut keep = used.iter();
        self.sectors
            .retain(|_| *keep.next().expect("used parallels sectors"));

        for line in &mut self.lines {
            if let Some(s) = line.front.sector {
                line.front.sector = Some(remap[s as usize]);
            }
            if let Some(back) = &mut line.back
                && let Some(s) = back.sector
            {
                back.sector = Some(remap[s as usize]);
            }
        }
        removed
    }

    /// Remove lines by index, then prune orphaned vertices. Indices may be
    /// in any order; duplicates are ignored.
    pub fn remove_lines(&mut self, indices: &[u32]) {
        let mut sorted: Vec<u32> = indices.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        for &i in sorted.iter().rev() {
            if (i as usize) < self.lines.len() {
                self.lines.remove(i as usize);
            }
        }
        self.prune_orphan_vertices();
    }

    /// Remove things by index. Indices may be in any order; duplicates are
    /// ignored.
    pub fn remove_things(&mut self, indices: &[u32]) {
        let mut sorted: Vec<u32> = indices.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        for &i in sorted.iter().rev() {
            if (i as usize) < self.things.len() {
                self.things.remove(i as usize);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn side() -> SideDef {
        SideDef {
            x_offset: 0,
            y_offset: 0,
            top_tex: Name8::EMPTY,
            bottom_tex: Name8::EMPTY,
            middle_tex: Name8::EMPTY,
            sector: Some(0),
        }
    }

    fn line(v1: u32, v2: u32) -> LineDef {
        LineDef {
            v1,
            v2,
            flags: LineFlags::BLOCKING,
            special: 0,
            tag: 0,
            front: side(),
            back: None,
        }
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
    fn prune_remaps_line_indices() {
        let mut map = EditorMap {
            vertices: vec![
                Vertex {
                    x: 0.0,
                    y: 0.0,
                }, // orphan
                Vertex {
                    x: 10.0,
                    y: 0.0,
                }, // used
                Vertex {
                    x: 20.0,
                    y: 0.0,
                }, // orphan
                Vertex {
                    x: 30.0,
                    y: 0.0,
                }, // used
            ],
            lines: vec![line(1, 3)],
            ..Default::default()
        };
        assert_eq!(map.prune_orphan_vertices(), 2);
        assert_eq!(map.vertices.len(), 2);
        assert_eq!((map.lines[0].v1, map.lines[0].v2), (0, 1));
        assert_eq!(map.vertices[0].x, 10.0);
        assert_eq!(map.vertices[1].x, 30.0);
    }

    #[test]
    fn remove_lines_prunes_and_handles_unsorted_duplicates() {
        let mut map = EditorMap {
            vertices: vec![
                Vertex {
                    x: 0.0,
                    y: 0.0,
                },
                Vertex {
                    x: 10.0,
                    y: 0.0,
                },
                Vertex {
                    x: 20.0,
                    y: 0.0,
                },
            ],
            lines: vec![line(0, 1), line(1, 2), line(2, 0)],
            ..Default::default()
        };
        map.remove_lines(&[2, 0, 2]);
        assert_eq!(map.lines.len(), 1);
        assert_eq!(map.vertices.len(), 2);
        let l = &map.lines[0];
        assert_eq!(map.vertices[l.v1 as usize].x, 10.0);
        assert_eq!(map.vertices[l.v2 as usize].x, 20.0);
    }

    #[test]
    fn remove_things_in_any_order() {
        let mut map = EditorMap::default();
        map.things = (0..4)
            .map(|i| Thing {
                x: i,
                y: 0,
                z: 0,
                angle: 0,
                kind: 1,
                options: ThingFlags::EASY | ThingFlags::NORMAL | ThingFlags::HARD,
            })
            .collect();
        map.remove_things(&[3, 1]);
        let xs: Vec<i32> = map.things.iter().map(|t| t.x).collect();
        assert_eq!(xs, vec![0, 2]);
    }
}
