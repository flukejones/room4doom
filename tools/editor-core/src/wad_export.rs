//! [`EditorMap`] → vanilla WAD lumps, BSP nodes via rbsp, PWAD assembly.
//!
//! Record layouts (all little-endian):
//! - THINGS: x, y, angle, kind, options — 5 × i16, 10 bytes
//! - LINEDEFS: v1, v2, flags, special, tag, front, back — 7 × 16-bit,
//!   14 bytes; back sidedef 0xFFFF means none
//! - SIDEDEFS: x_off, y_off (i16), upper/lower/middle (8-byte names),
//!   sector (i16) — 30 bytes
//! - SECTORS: floor/ceil height (i16), floor/ceil flat (8-byte names),
//!   light, special, tag (i16) — 26 bytes
//! - Empty texture names encode as `"-"`.
//!
//! Sidedefs are embedded per line in the model; they are emitted in line
//! order, front then back, so the SIDEDEFS lump is always de-shared.
//!
//! Vertices not referenced by any line are pruned on export and LINEDEFS
//! indices renumbered to match: rbsp compacts unreferenced vertices out of
//! its VERTEXES output, so the lump and the indices must agree (doombsp
//! likewise wrote only referenced points).
//!
//! Sector splitting: the .dwd format merges byte-identical sectordefs while
//! editing, and doombsp re-split them into connected components over BSP
//! subsectors at save time (`ProcessSectors`/`RecursiveGroupSubsector`).
//! [`export_map_pwad`] reproduces that split after `build_bsp`: subsectors
//! sharing a vertex and an identical sector record group into one output
//! sector, and sidedef sector indices are rewritten from the segs that
//! reference them. Disable via
//! [`ExportOptions::split_disconnected_sectors`] for maps whose sector
//! identity is authoritative (imported from a WAD).

use std::collections::HashMap;
use std::fmt;

use rbsp::wad_io::{NodesFormat, build_node_lumps};
use rbsp::{BspInput, BspOptions, BspOutput, BuildEvent, Side, build_bsp, build_bsp_traced};
use wad::Lump;
use wad::types::{WadLineDef, WadSector, WadSideDef, WadVertex};
use wad::write::{WadWriteError, write_pwad};

use crate::model::{EditorMap, Sector, SideDef};

/// The concrete [`BspInput`] specialization the editor builds (all WAD types).
type WadBspInput = BspInput<WadVertex, WadLineDef, WadSideDef, WadSector>;

/// Default flat name marking sky surfaces in the RBSP 3D lump.
pub const DEFAULT_SKY_FLAT: &str = "F_SKY1";
/// rbsp's default BSP split cost weight.
pub const DEFAULT_SPLIT_WEIGHT: f64 = 10.0;
/// "No back sidedef" sentinel in a LINEDEFS record.
const NO_BACK_SIDEDEF: u16 = u16::MAX;
/// Cap on original (referenced) vertices before export. Below the u16 limit so
/// the BSP's appended split vertices still index as u16 downstream.
const MAX_ORIGINAL_VERTICES: usize = u16::MAX as usize - 1;

const THINGS_RECORD_LEN: usize = 10;
const LINEDEFS_RECORD_LEN: usize = 14;
const SIDEDEFS_RECORD_LEN: usize = 30;
const SECTORS_RECORD_LEN: usize = 26;

/// How a map is exported to a PWAD.
pub struct ExportOptions {
    pub nodes: NodesFormat,
    /// BSP split cost weight; [`DEFAULT_SPLIT_WEIGHT`] matches rbsp.
    pub split_weight: f64,
    /// Flat name marking sky surfaces for the RBSP 3D lump.
    pub sky_flat: String,
    /// Re-split value-merged sectors into connected components (doombsp
    /// save-time parity). Disable for WAD-imported maps to preserve their
    /// sector identity exactly.
    pub split_disconnected_sectors: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            nodes: NodesFormat::default(),
            split_weight: DEFAULT_SPLIT_WEIGHT,
            sky_flat: DEFAULT_SKY_FLAT.to_owned(),
            split_disconnected_sectors: true,
        }
    }
}

/// Failure while exporting a map to WAD lumps.
#[derive(Debug)]
pub enum ExportError {
    /// A vertex coordinate cannot be stored as a WAD i16.
    VertexOutOfRange {
        index: usize,
        x: f32,
        y: f32,
    },
    /// An integer field exceeds its WAD lump width.
    FieldOutOfRange {
        what: &'static str,
        index: usize,
        value: i32,
    },
    /// More records than a 16-bit lump index can address.
    TooManyItems {
        what: &'static str,
        count: usize,
    },
    /// A linedef side references a sector index past the sector list.
    InvalidSectorRef {
        line: usize,
        sector: u32,
        sector_count: usize,
    },
    Wad(WadWriteError),
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::VertexOutOfRange {
                index,
                x,
                y,
            } => {
                write!(f, "vertex {index} ({x},{y}) outside the WAD i16 range")
            }
            Self::FieldOutOfRange {
                what,
                index,
                value,
            } => {
                write!(
                    f,
                    "{what} {index}: value {value} outside its WAD field range"
                )
            }
            Self::TooManyItems {
                what,
                count,
            } => {
                write!(f, "{count} {what} exceed the 16-bit WAD index range")
            }
            Self::InvalidSectorRef {
                line,
                sector,
                sector_count,
            } => {
                write!(
                    f,
                    "line {line} side references sector {sector} but only {sector_count} exist"
                )
            }
            Self::Wad(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ExportError {}

impl From<WadWriteError> for ExportError {
    fn from(e: WadWriteError) -> Self {
        Self::Wad(e)
    }
}

/// Per-line sidedef numbering: sides emitted in line order, front then back.
struct FlatSides<'a> {
    sides: Vec<&'a SideDef>,
    front: Vec<u16>,
    back: Vec<u16>,
}

/// Line-referenced vertices in original order with old→new index mapping.
struct PrunedVertices {
    wad_vertices: Vec<WadVertex>,
    /// Indexed by model vertex id; `u32::MAX` for pruned entries.
    remap: Vec<u32>,
}

fn prune_vertices(map: &EditorMap) -> Result<PrunedVertices, ExportError> {
    let mut used = vec![false; map.vertices.len()];
    for line in &map.lines {
        used[line.v1 as usize] = true;
        used[line.v2 as usize] = true;
    }

    let mut wad_vertices = Vec::with_capacity(map.vertices.len());
    let mut remap = vec![u32::MAX; map.vertices.len()];
    let range = i64::from(i16::MIN)..=i64::from(i16::MAX);
    for (i, (v, &is_used)) in map.vertices.iter().zip(&used).enumerate() {
        if !is_used {
            continue;
        }
        if !range.contains(&(v.x as i64)) || !range.contains(&(v.y as i64)) {
            return Err(ExportError::VertexOutOfRange {
                index: i,
                x: v.x,
                y: v.y,
            });
        }
        remap[i] = wad_vertices.len() as u32;
        wad_vertices.push(WadVertex {
            x: v.x,
            y: v.y,
        });
    }
    // Original referenced vertices must index as u16 (0..=65535). The BSP then
    // appends split vertices that also need u16 indices in SEGS/SSECTORS, so
    // we cap originals below the limit to leave headroom rather than at the
    // exact maximum.
    if wad_vertices.len() > MAX_ORIGINAL_VERTICES {
        return Err(ExportError::TooManyItems {
            what: "vertices",
            count: wad_vertices.len(),
        });
    }
    Ok(PrunedVertices {
        wad_vertices,
        remap,
    })
}

fn flatten_sides(map: &EditorMap) -> Result<FlatSides<'_>, ExportError> {
    let mut sides = Vec::with_capacity(map.lines.len() * 2);
    let mut front = Vec::with_capacity(map.lines.len());
    let mut back = Vec::with_capacity(map.lines.len());
    for line in &map.lines {
        front.push(sides.len() as u16);
        sides.push(&line.front);
        match &line.back {
            Some(side) => {
                back.push(sides.len() as u16);
                sides.push(side);
            }
            None => back.push(NO_BACK_SIDEDEF),
        }
        if sides.len() >= NO_BACK_SIDEDEF as usize {
            return Err(ExportError::TooManyItems {
                what: "sidedefs",
                count: sides.len(),
            });
        }
    }
    Ok(FlatSides {
        sides,
        front,
        back,
    })
}

fn to_i16(what: &'static str, index: usize, value: i32) -> Result<i16, ExportError> {
    i16::try_from(value).map_err(|_| ExportError::FieldOutOfRange {
        what,
        index,
        value,
    })
}

fn to_u16(what: &'static str, index: usize, value: i32) -> Result<u16, ExportError> {
    u16::try_from(value).map_err(|_| ExportError::FieldOutOfRange {
        what,
        index,
        value,
    })
}

/// Encode the THINGS lump.
pub fn encode_things(map: &EditorMap) -> Result<Vec<u8>, ExportError> {
    let mut buf = Vec::with_capacity(map.things.len() * THINGS_RECORD_LEN);
    for (i, t) in map.things.iter().enumerate() {
        for (what, value) in [
            ("thing x", t.x),
            ("thing y", t.y),
            ("thing angle", t.angle),
            ("thing kind", t.kind),
            ("thing options", t.options.bits()),
        ] {
            buf.extend_from_slice(&to_i16(what, i, value)?.to_le_bytes());
        }
    }
    Ok(buf)
}

fn encode_linedefs(
    map: &EditorMap,
    flat: &FlatSides<'_>,
    remap: &[u32],
) -> Result<Vec<u8>, ExportError> {
    let mut buf = Vec::with_capacity(map.lines.len() * LINEDEFS_RECORD_LEN);
    for (i, line) in map.lines.iter().enumerate() {
        buf.extend_from_slice(&(remap[line.v1 as usize] as u16).to_le_bytes());
        buf.extend_from_slice(&(remap[line.v2 as usize] as u16).to_le_bytes());
        buf.extend_from_slice(&to_u16("line flags", i, line.flags.bits())?.to_le_bytes());
        buf.extend_from_slice(&to_i16("line special", i, line.special)?.to_le_bytes());
        buf.extend_from_slice(&to_i16("line tag", i, line.tag)?.to_le_bytes());
        buf.extend_from_slice(&flat.front[i].to_le_bytes());
        buf.extend_from_slice(&flat.back[i].to_le_bytes());
    }
    Ok(buf)
}

fn encode_sidedefs(flat: &FlatSides<'_>, side_sector: &[u32]) -> Result<Vec<u8>, ExportError> {
    let mut buf = Vec::with_capacity(flat.sides.len() * SIDEDEFS_RECORD_LEN);
    for (i, (side, &sector)) in flat.sides.iter().zip(side_sector).enumerate() {
        buf.extend_from_slice(&to_i16("sidedef x offset", i, side.x_offset)?.to_le_bytes());
        buf.extend_from_slice(&to_i16("sidedef y offset", i, side.y_offset)?.to_le_bytes());
        buf.extend_from_slice(&side.top_tex.to_wad_bytes());
        buf.extend_from_slice(&side.bottom_tex.to_wad_bytes());
        buf.extend_from_slice(&side.middle_tex.to_wad_bytes());
        buf.extend_from_slice(&to_i16("sidedef sector", i, sector as i32)?.to_le_bytes());
    }
    Ok(buf)
}

fn encode_sectors(sectors: &[Sector]) -> Result<Vec<u8>, ExportError> {
    let mut buf = Vec::with_capacity(sectors.len() * SECTORS_RECORD_LEN);
    for (i, s) in sectors.iter().enumerate() {
        buf.extend_from_slice(&to_i16("sector floor height", i, s.floor_height)?.to_le_bytes());
        buf.extend_from_slice(&to_i16("sector ceiling height", i, s.ceil_height)?.to_le_bytes());
        buf.extend_from_slice(&s.floor_flat.to_wad_bytes());
        buf.extend_from_slice(&s.ceil_flat.to_wad_bytes());
        buf.extend_from_slice(&to_i16("sector light", i, s.light_level)?.to_le_bytes());
        buf.extend_from_slice(&to_i16("sector special", i, s.special)?.to_le_bytes());
        buf.extend_from_slice(&to_i16("sector tag", i, s.tag)?.to_le_bytes());
    }
    Ok(buf)
}

/// Convert the editor model into rbsp's input. Sidedef sector indices are
/// the model's (pre-split) indices; rbsp only reads sector records for 3D
/// geometry heights, which the split never changes.
pub fn to_bsp_input(map: &EditorMap) -> Result<WadBspInput, ExportError> {
    let flat = flatten_sides(map)?;
    let PrunedVertices {
        wad_vertices,
        remap,
    } = prune_vertices(map)?;
    to_bsp_input_flat(map, &flat, wad_vertices, &remap)
}

fn to_bsp_input_flat(
    map: &EditorMap,
    flat: &FlatSides<'_>,
    wad_vertices: Vec<WadVertex>,
    remap: &[u32],
) -> Result<WadBspInput, ExportError> {
    let mut sidedefs = Vec::with_capacity(flat.sides.len());
    for (i, side) in flat.sides.iter().enumerate() {
        sidedefs.push(WadSideDef {
            x_offset: to_i16("sidedef x offset", i, side.x_offset)?,
            y_offset: to_i16("sidedef y offset", i, side.y_offset)?,
            upper_tex: side.top_tex.as_str().to_owned(),
            lower_tex: side.bottom_tex.as_str().to_owned(),
            middle_tex: side.middle_tex.as_str().to_owned(),
            sector: to_i16("sidedef sector", i, side.sector.unwrap_or(0) as i32)?,
        });
    }

    let mut linedefs = Vec::with_capacity(map.lines.len());
    for (i, line) in map.lines.iter().enumerate() {
        let front = flat.front[i];
        let back = flat.back[i];
        linedefs.push(WadLineDef::new(
            remap[line.v1 as usize] as u16,
            remap[line.v2 as usize] as u16,
            to_u16("line flags", i, line.flags.bits())?,
            to_i16("line special", i, line.special)?,
            to_i16("line tag", i, line.tag)?,
            front,
            (back != NO_BACK_SIDEDEF).then_some(back),
            [front, back],
        ));
    }

    let mut sectors = Vec::with_capacity(map.sectors.len());
    for (i, s) in map.sectors.iter().enumerate() {
        sectors.push(WadSector {
            floor_height: to_i16("sector floor height", i, s.floor_height)?,
            ceil_height: to_i16("sector ceiling height", i, s.ceil_height)?,
            floor_tex: s.floor_flat.as_str().to_owned(),
            ceil_tex: s.ceil_flat.as_str().to_owned(),
            light_level: to_i16("sector light", i, s.light_level)?,
            kind: to_i16("sector special", i, s.special)?,
            tag: to_i16("sector tag", i, s.tag)?,
        });
    }

    Ok(BspInput {
        vertices: wad_vertices,
        linedefs,
        sidedefs,
        sectors,
    })
}

/// Output sector list and the final sector index for every flattened side.
struct SectorAssignment {
    sectors: Vec<Sector>,
    side_sector: Vec<u32>,
}

fn identity_assignment(map: &EditorMap, flat: &FlatSides<'_>) -> SectorAssignment {
    let mut sectors = Vec::with_capacity(map.sectors.len());
    sectors.extend_from_slice(&map.sectors);
    let side_sector = flat.sides.iter().map(|s| s.sector.unwrap_or(0)).collect();
    SectorAssignment {
        sectors,
        side_sector,
    }
}

/// doombsp `ProcessSectors` parity: group subsectors into connected
/// components (shared vertex + identical sector record), one output sector
/// per component, sidedef sectors reassigned from the segs touching them.
/// Union-find root with path compression.
fn uf_find(parent: &mut [u32], mut i: u32) -> u32 {
    while parent[i as usize] != i {
        parent[i as usize] = parent[parent[i as usize] as usize];
        i = parent[i as usize];
    }
    i
}

fn split_assignment(map: &EditorMap, flat: &FlatSides<'_>, output: &BspOutput) -> SectorAssignment {
    let num_ss = output.subsectors.len();

    // Value-identity of each input sector (the merge doombsp's UniqueSector
    // performed; already true for dwd-imported maps, harmless otherwise).
    let mut def_ids: HashMap<Sector, u32> = HashMap::with_capacity(map.sectors.len());
    let mut def_of_input = Vec::with_capacity(map.sectors.len());
    for s in &map.sectors {
        let next = def_ids.len() as u32;
        def_of_input.push(*def_ids.entry(*s).or_insert(next));
    }
    let ss_def: Vec<u32> = output
        .subsectors
        .iter()
        .map(|ss| def_of_input[ss.sector as usize])
        .collect();

    // Subsectors touching each seg-endpoint vertex.
    let mut vertex_ss: HashMap<usize, Vec<u32>> = HashMap::with_capacity(output.vertices.len());
    for (i, ss) in output.subsectors.iter().enumerate() {
        for &seg_idx in &ss.seg_indices {
            let seg = &output.segs[seg_idx as usize];
            for v in [seg.start, seg.end] {
                vertex_ss.entry(v).or_default().push(i as u32);
            }
        }
    }

    // Union-find over subsectors: same vertex + same def --> same sector.
    // Vertex buckets iterate in sorted order so the partition (and thus the
    // exported byte image) is deterministic across runs.
    let mut parent: Vec<u32> = (0..num_ss as u32).collect();
    let mut vertex_keys: Vec<usize> = vertex_ss.keys().copied().collect();
    vertex_keys.sort_unstable();
    for key in vertex_keys {
        let group = &vertex_ss[&key];
        for (pos, &a) in group.iter().enumerate() {
            for &b in &group[pos + 1..] {
                if ss_def[a as usize] == ss_def[b as usize] {
                    let ra = uf_find(&mut parent, a);
                    let rb = uf_find(&mut parent, b);
                    if ra != rb {
                        parent[ra as usize] = rb;
                    }
                }
            }
        }
    }

    // Number components in first-encounter order; record per-def fallback.
    let mut group_of_root: HashMap<u32, u32> = HashMap::with_capacity(num_ss);
    let mut sectors = Vec::with_capacity(map.sectors.len());
    let mut first_group_of_def: HashMap<u32, u32> = HashMap::with_capacity(map.sectors.len());
    let mut ss_group = Vec::with_capacity(num_ss);
    for (i, ss) in output.subsectors.iter().enumerate() {
        let root = uf_find(&mut parent, i as u32);
        let group = *group_of_root.entry(root).or_insert_with(|| {
            let id = sectors.len() as u32;
            sectors.push(map.sectors[ss.sector as usize]);
            first_group_of_def.entry(ss_def[i]).or_insert(id);
            id
        });
        ss_group.push(group);
    }

    // Sidedef sector assignment from the segs referencing each side.
    let mut side_sector = vec![u32::MAX; flat.sides.len()];
    for (i, ss) in output.subsectors.iter().enumerate() {
        for &seg_idx in &ss.seg_indices {
            let seg = &output.segs[seg_idx as usize];
            let flat_idx = match seg.side {
                Side::Front => flat.front[seg.linedef],
                Side::Back => flat.back[seg.linedef],
            };
            if flat_idx != NO_BACK_SIDEDEF {
                side_sector[flat_idx as usize] = ss_group[i];
            }
        }
    }

    // Sides no seg touched (degenerate geometry): first component of their
    // def, or a fresh sector if the def produced no subsectors at all.
    for (i, side) in flat.sides.iter().enumerate() {
        if side_sector[i] == u32::MAX {
            let s = side.sector.unwrap_or(0) as usize;
            let def = def_of_input[s];
            side_sector[i] = *first_group_of_def.entry(def).or_insert_with(|| {
                let id = sectors.len() as u32;
                sectors.push(map.sectors[s]);
                id
            });
        }
    }

    SectorAssignment {
        sectors,
        side_sector,
    }
}

/// Full export pipeline: range checks, BSP build, sector split, lump
/// encoding, PWAD assembly. Returns the complete PWAD byte image.
pub fn export_map_pwad(
    map: &EditorMap,
    map_name: &str,
    opts: &ExportOptions,
) -> Result<Vec<u8>, ExportError> {
    export_map_pwad_with_lumps(map, map_name, opts, Vec::new())
}

/// [`export_map_pwad`] plus extra lumps appended after the map (project
/// TEXTURE1/2 + PNAMES when custom textures exist).
pub fn export_map_pwad_with_lumps(
    map: &EditorMap,
    map_name: &str,
    opts: &ExportOptions,
    extra_lumps: Vec<Lump>,
) -> Result<Vec<u8>, ExportError> {
    let (bytes, _events) = export_inner(map, map_name, opts, extra_lumps, false)?;
    Ok(bytes)
}

/// [`export_map_pwad_with_lumps`] that also returns the BSP build trace (in
/// build order) for the editor's animation overlay. The PWAD bytes are
/// identical to the untraced call.
pub fn export_map_pwad_with_lumps_traced(
    map: &EditorMap,
    map_name: &str,
    opts: &ExportOptions,
    extra_lumps: Vec<Lump>,
) -> Result<(Vec<u8>, Vec<BuildEvent>), ExportError> {
    export_inner(map, map_name, opts, extra_lumps, true)
}

/// Reject sidedefs that reference a non-existent sector before the BSP build,
/// so the sector-assignment step (which indexes `map.sectors` by subsector
/// sector) cannot panic on a malformed map.
fn check_sector_refs(map: &EditorMap) -> Result<(), ExportError> {
    let n = map.sectors.len();
    for (line, l) in map.lines.iter().enumerate() {
        for side in [Some(&l.front), l.back.as_ref()].into_iter().flatten() {
            if let Some(s) = side.sector
                && s as usize >= n
            {
                return Err(ExportError::InvalidSectorRef {
                    line,
                    sector: s,
                    sector_count: n,
                });
            }
        }
    }
    Ok(())
}

fn export_inner(
    map: &EditorMap,
    map_name: &str,
    opts: &ExportOptions,
    extra_lumps: Vec<Lump>,
    traced: bool,
) -> Result<(Vec<u8>, Vec<BuildEvent>), ExportError> {
    check_sector_refs(map)?;
    let flat = flatten_sides(map)?;
    let PrunedVertices {
        wad_vertices,
        remap,
    } = prune_vertices(map)?;
    let input = to_bsp_input_flat(map, &flat, wad_vertices, &remap)?;
    let bsp_options = BspOptions {
        split_weight: opts.split_weight,
        ..BspOptions::default()
    };
    let (output, events) = if traced {
        build_bsp_traced(&input, &bsp_options)
    } else {
        (build_bsp(&input, &bsp_options), Vec::new())
    };

    let assignment = if opts.split_disconnected_sectors {
        split_assignment(map, &flat, &output)
    } else {
        identity_assignment(map, &flat)
    };

    let node_lumps = build_node_lumps(
        &input,
        &output,
        opts.nodes,
        Some(&opts.sky_flat),
        assignment.sectors.len(),
    );

    let lump = |name: &str, data: Vec<u8>| Lump {
        name: name.to_owned(),
        data,
    };
    let mut lumps = Vec::with_capacity(12);
    lumps.push(lump(&map_name.to_ascii_uppercase(), Vec::new()));
    lumps.push(lump("THINGS", encode_things(map)?));
    lumps.push(lump("LINEDEFS", encode_linedefs(map, &flat, &remap)?));
    lumps.push(lump(
        "SIDEDEFS",
        encode_sidedefs(&flat, &assignment.side_sector)?,
    ));
    lumps.push(lump("VERTEXES", node_lumps.vertexes));
    lumps.push(lump("SEGS", node_lumps.segs));
    lumps.push(lump("SSECTORS", node_lumps.ssectors));
    lumps.push(lump("NODES", node_lumps.nodes));
    lumps.push(lump("SECTORS", encode_sectors(&assignment.sectors)?));
    lumps.push(lump("REJECT", node_lumps.reject));
    lumps.push(lump("BLOCKMAP", node_lumps.blockmap));
    if let Some(rbsp_lump) = node_lumps.rbsp {
        lumps.push(lump("RBSP", rbsp_lump));
    }
    lumps.extend(extra_lumps);

    Ok((write_pwad(&lumps)?, events))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{LineDef, Thing, Vertex};
    use crate::name8::Name8;
    use crate::wad_import::import_wad_map;
    use crate::{LineFlags, ThingFlags};
    use wad::WadData;

    fn name(s: &str) -> Name8 {
        Name8::new(s).expect("test name valid")
    }

    fn side(sector: u32, middle: &str) -> SideDef {
        SideDef {
            x_offset: 0,
            y_offset: 0,
            top_tex: Name8::EMPTY,
            bottom_tex: Name8::EMPTY,
            middle_tex: name(middle),
            sector: Some(sector),
        }
    }

    fn sector(floor: i32, light: i32) -> Sector {
        Sector {
            floor_height: floor,
            floor_flat: name("FLOOR4_8"),
            ceil_height: floor + 128,
            ceil_flat: name("CEIL3_5"),
            light_level: light,
            special: 0,
            tag: 0,
        }
    }

    /// Clockwise square: the front (right) side of every line faces the
    /// interior, the Doom convention for a closed room.
    fn square_at(x0: f32, y0: f32) -> (Vec<Vertex>, Vec<(usize, usize)>) {
        let verts = vec![
            Vertex {
                x: x0,
                y: y0,
            },
            Vertex {
                x: x0 + 128.0,
                y: y0,
            },
            Vertex {
                x: x0 + 128.0,
                y: y0 + 128.0,
            },
            Vertex {
                x: x0,
                y: y0 + 128.0,
            },
        ];
        let edges = vec![(1, 0), (0, 3), (3, 2), (2, 1)];
        (verts, edges)
    }

    fn one_square_map() -> EditorMap {
        let (verts, edges) = square_at(0.0, 0.0);
        let lines = edges
            .iter()
            .map(|&(a, b)| LineDef {
                v1: a as u32,
                v2: b as u32,
                flags: LineFlags::BLOCKING,
                special: 0,
                tag: 0,
                front: side(0, "STARTAN3"),
                back: None,
            })
            .collect();
        EditorMap {
            vertices: verts,
            lines,
            sectors: vec![sector(0, 255)],
            things: vec![Thing {
                x: 64,
                y: 64,
                z: 0,
                angle: 90,
                kind: 1,
                options: ThingFlags::from_bits_retain(7),
            }],
            required_wads: Vec::new(),
        }
    }

    /// Two disconnected squares whose sides reference ONE merged sector
    /// record — the dwd-import shape that doombsp re-splits at save time.
    fn two_disconnected_squares_map() -> EditorMap {
        let (mut verts, edges) = square_at(0.0, 0.0);
        let (verts2, _) = square_at(512.0, 0.0);
        let base2 = verts.len() as u32;
        verts.extend(verts2);

        let mut lines: Vec<LineDef> = edges
            .iter()
            .map(|&(a, b)| LineDef {
                v1: a as u32,
                v2: b as u32,
                flags: LineFlags::BLOCKING,
                special: 0,
                tag: 0,
                front: side(0, "STARTAN3"),
                back: None,
            })
            .collect();
        lines.extend(edges.iter().map(|&(a, b)| LineDef {
            v1: base2 + a as u32,
            v2: base2 + b as u32,
            flags: LineFlags::BLOCKING,
            special: 0,
            tag: 0,
            front: side(0, "STARTAN3"),
            back: None,
        }));

        EditorMap {
            vertices: verts,
            lines,
            sectors: vec![sector(0, 255)],
            things: Vec::new(),
            required_wads: Vec::new(),
        }
    }

    #[test]
    fn things_lump_golden_bytes() {
        let map = one_square_map();
        let buf = encode_things(&map).expect("in-range fields encode");
        assert_eq!(buf.len(), 10);
        let rd = |i: usize| i16::from_le_bytes([buf[i * 2], buf[i * 2 + 1]]);
        assert_eq!([rd(0), rd(1), rd(2), rd(3), rd(4)], [64, 64, 90, 1, 7]);
    }

    #[test]
    fn linedef_and_sidedef_lumps_reference_in_emission_order() {
        let map = one_square_map();
        let flat = flatten_sides(&map).expect("4 sides flatten");
        let verts = prune_vertices(&map).expect("in range");
        let lines = encode_linedefs(&map, &flat, &verts.remap).expect("encodes");
        assert_eq!(lines.len(), 4 * 14);
        // Line 0: v1=1 v2=0 flags=1 special=0 tag=0 front=0 back=0xFFFF.
        let rd = |i: usize| u16::from_le_bytes([lines[i * 2], lines[i * 2 + 1]]);
        assert_eq!(
            [rd(0), rd(1), rd(2), rd(3), rd(4), rd(5), rd(6)],
            [1, 0, 1, 0, 0, 0, u16::MAX]
        );

        let sides = encode_sidedefs(&flat, &[0, 0, 0, 0]).expect("encodes");
        assert_eq!(sides.len(), 4 * 30);
        assert_eq!(&sides[4..12], b"-\0\0\0\0\0\0\0");
        assert_eq!(&sides[20..28], b"STARTAN3");
        assert_eq!(i16::from_le_bytes([sides[28], sides[29]]), 0);
    }

    #[test]
    fn sectors_lump_golden_bytes() {
        let buf = encode_sectors(&[sector(-8, 128)]).expect("encodes");
        assert_eq!(buf.len(), 26);
        assert_eq!(i16::from_le_bytes([buf[0], buf[1]]), -8);
        assert_eq!(i16::from_le_bytes([buf[2], buf[3]]), 120);
        assert_eq!(&buf[4..12], b"FLOOR4_8");
        assert_eq!(&buf[12..20], b"CEIL3_5\0");
        assert_eq!(i16::from_le_bytes([buf[20], buf[21]]), 128);
    }

    #[test]
    fn out_of_range_vertex_rejected() {
        let mut map = one_square_map();
        map.vertices[0].x = 40000.0;
        let err = export_map_pwad(&map, "E1M1", &ExportOptions::default())
            .expect_err("coordinate exceeds i16");
        assert!(
            matches!(
                err,
                ExportError::VertexOutOfRange {
                    index: 0,
                    ..
                }
            ),
            "{err}"
        );
    }

    #[test]
    fn out_of_range_field_rejected() {
        let mut map = one_square_map();
        map.lines[0].tag = 70000;
        let err =
            export_map_pwad(&map, "E1M1", &ExportOptions::default()).expect_err("tag exceeds i16");
        assert!(
            matches!(
                err,
                ExportError::FieldOutOfRange {
                    what: "line tag",
                    ..
                }
            ),
            "{err}"
        );
    }

    #[test]
    fn traced_export_matches_untraced_and_records_events() {
        let map = one_square_map();
        let opts = ExportOptions::default();
        let plain = export_map_pwad_with_lumps(&map, "E1M1", &opts, Vec::new()).expect("export ok");
        let (traced, events) =
            export_map_pwad_with_lumps_traced(&map, "E1M1", &opts, Vec::new()).expect("export ok");
        assert_eq!(plain, traced, "trace flag must not change PWAD bytes");
        assert!(!events.is_empty(), "build emits at least one subsector");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, BuildEvent::SubsectorDone { .. })),
            "a leaf subsector is recorded"
        );
    }

    #[test]
    fn disconnected_merged_sectors_split_on_export() {
        let map = two_disconnected_squares_map();
        let flat = flatten_sides(&map).expect("8 sides flatten");
        let PrunedVertices {
            wad_vertices,
            remap,
        } = prune_vertices(&map).expect("in range");
        let input = to_bsp_input_flat(&map, &flat, wad_vertices, &remap).expect("in range");
        let output = build_bsp(
            &input,
            &BspOptions {
                split_weight: DEFAULT_SPLIT_WEIGHT,
                ..BspOptions::default()
            },
        );

        let split = split_assignment(&map, &flat, &output);
        assert_eq!(split.sectors.len(), 2, "two components from one record");
        assert_eq!(split.sectors[0], split.sectors[1], "identical records");
        let first = split.side_sector[0];
        let second = split.side_sector[4];
        assert_ne!(first, second);
        assert!(split.side_sector[..4].iter().all(|&s| s == first));
        assert!(split.side_sector[4..].iter().all(|&s| s == second));

        let identity = identity_assignment(&map, &flat);
        assert_eq!(identity.sectors.len(), 1);
        assert!(identity.side_sector.iter().all(|&s| s == 0));
    }

    /// Round-trip through a real engine-shaped map: shareware E1M1 in, PWAD
    /// out, imported again. Split disabled — the WAD's sector identity is
    /// authoritative.
    #[test]
    fn e1m1_export_reimports_identically() {
        let wad = WadData::new(&test_utils::doom1_wad_path());
        let map = import_wad_map(&wad, "E1M1").expect("E1M1 imports");

        let opts = ExportOptions {
            split_disconnected_sectors: false,
            ..ExportOptions::default()
        };
        let bytes = export_map_pwad(&map, "E1M1", &opts).expect("E1M1 exports");

        let path = std::env::temp_dir().join("editor_core_e1m1_roundtrip.wad");
        std::fs::write(&path, &bytes).expect("temp pwad writes");
        let rewad = WadData::new(&path);
        let remap = import_wad_map(&rewad, "E1M1").expect("exported pwad imports");
        std::fs::remove_file(&path).ok();

        assert_eq!(remap.things, map.things);
        assert_eq!(remap.sectors, map.sectors);
        assert_eq!(remap.lines.len(), map.lines.len());
        // Vertex indices renumber (unreferenced originals are pruned on
        // export), so endpoints compare by coordinate.
        for (i, (a, b)) in map.lines.iter().zip(&remap.lines).enumerate() {
            let ap = (map.vertices[a.v1 as usize], map.vertices[a.v2 as usize]);
            let bp = (remap.vertices[b.v1 as usize], remap.vertices[b.v2 as usize]);
            assert_eq!(ap, bp, "line {i} endpoint coordinates");
            assert_eq!(
                (a.flags, a.special, a.tag),
                (b.flags, b.special, b.tag),
                "line {i} header"
            );
            assert_eq!(a.front, b.front, "line {i} front side");
            assert_eq!(a.back, b.back, "line {i} back side");
        }
    }
}
