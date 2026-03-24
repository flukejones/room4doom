use std::collections::hash_map::DefaultHasher;
use std::fmt::Display;
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use std::{fmt, str};

use crate::types::WadBlockMap;

const FRACUNIT: f32 = (1 << 16) as f32;

/// Used as an index to find a specific lump, typically combined
/// with an offset for example: find the index for lump named "E1M1"
/// in `self.wad_dirs` then combine this index with a `LumpIndex`
/// variant to get a specific lump.
#[allow(dead_code)]
pub enum MapLump {
    /// Position and angle for all monster, powerup and spawn location
    Things = 1,
    /// An array of lines referencing two vertices (Two vertexes are connected
    /// by one `LineDef`). This is a direct
    /// translation of the lines used in DoomED. Also points to one or two
    /// `SideDef` depending on if this line is a wall or a portal
    LineDefs,
    /// Defines upper, lower, and middle textures. Also defines texture
    /// horizontal and vertical offsets. This is information for a `LineDef`
    SideDefs,
    /// An array of signed short X, Y pairs (`Vertex`). All coordinates in this
    /// level block are indexes into this array
    Vertexes,
    /// Portions of lines cut due to Binary Space Partitioning (see page
    /// 202 in Fabien Sanglard's Game Engine Black Book - DOOM).
    /// Each `SubSectors`'s geometry is defined by the `Segs` which it contains
    Segs,
    /// Set of segments of a `LineDef` representing a convex subspace
    SubSectors,
    /// BSP with segs, nodes and sub-sector leaves
    Nodes,
    /// Area surrounded by lines, with set ceiling and floor textures/heights
    /// with light level
    Sectors,
    /// Sector-to-sector visibility matrix to speed-up line of sight
    /// calculations
    Reject,
    /// 128x128 grid partition of the level LINEDEFS to accelerate collision
    /// detection
    Blockmap,
    Count,
}

impl Display for MapLump {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MapLump::Things => write!(f, "THINGS"),
            MapLump::LineDefs => write!(f, "LINEDEFS"),
            MapLump::SideDefs => write!(f, "SIDEDEFS"),
            MapLump::Vertexes => write!(f, "VERTEXES"),
            MapLump::Segs => write!(f, "SEGS"),
            MapLump::SubSectors => write!(f, "SSECTORS"),
            MapLump::Nodes => write!(f, "NODES"),
            MapLump::Sectors => write!(f, "SECTORS"),
            MapLump::Reject => write!(f, "REJECT"),
            MapLump::Blockmap => write!(f, "BLOCKMAP"),
            MapLump::Count => write!(f, "COUNT"),
        }
    }
}

/// Header which tells us the WAD type and where the data is
///
/// The header structure in the WAD is as follows:
///
/// | Field Size | Data Type    | Content                                              |
/// |------------|--------------|------------------------------------------------------|
/// | 0x00-0x03  | 4 ASCII char | *Must* be an ASCII string (either "IWAD" or "PWAD")  |
/// | 0x04-0x07  | unsigned int | The number entries in the directory                  |
/// | 0x08-0x0b  | unsigned int | Offset in bytes to the directory in the WAD file     |
pub struct WadHeader {
    /// Will be either `IWAD` for game-exe, or `PWAD` for patch
    wad_type: [u8; 4],
    /// The count of "lumps" of data
    dir_count: u32,
    /// Offset in bytes that the lump data starts at
    dir_offset: u32,
}

impl WadHeader {
    pub fn wad_type(&self) -> &str {
        unsafe { str::from_utf8_unchecked(&self.wad_type) }
    }
}

impl fmt::Debug for WadHeader {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "\nWadHeader {{\n  wad_type: {},\n  dir_count: {},\n  dir_offset: {},\n}}",
            str::from_utf8(&self.wad_type).unwrap(),
            self.dir_count,
            self.dir_offset
        )
    }
}

/// Contains the details for a lump of data: where it starts, the size of it,
/// and the name
///
/// The directory structure in the WAD is as follows:
///
/// | Field Size | Data Type    | Content                                                    |
/// |------------|--------------|------------------------------------------------------------|
/// | 0x00-0x03  | unsigned int | Offset value to the start of the lump data in the WAD file |
/// | 0x04-0x07  | unsigned int | The size of the lump in bytes                              |
/// | 0x08-0x0f  | 8 ASCII char | ASCII holding the name of the lump                         |
pub struct Lump {
    /// Name for the lump data
    pub name: String,
    /// The Index in to `WadData.file_data`
    pub data: Vec<u8>,
}

impl Lump {
    pub fn read_i16(&self, offset: usize) -> i16 {
        i16::from_le_bytes([self.data[offset], self.data[offset + 1]])
    }

    pub fn read_u16(&self, offset: usize) -> u16 {
        u16::from_le_bytes([self.data[offset], self.data[offset + 1]])
    }

    pub fn read_i32(&self, offset: usize) -> i32 {
        i32::from_le_bytes([
            self.data[offset],
            self.data[offset + 1],
            self.data[offset + 2],
            self.data[offset + 3],
        ])
    }

    pub fn read_u32(&self, offset: usize) -> u32 {
        u32::from_le_bytes([
            self.data[offset],
            self.data[offset + 1],
            self.data[offset + 2],
            self.data[offset + 3],
        ])
    }

    pub fn read_u32_to_f32(&self, offset: usize) -> f32 {
        i32::from_le_bytes([
            self.data[offset],
            self.data[offset + 1],
            self.data[offset + 2],
            self.data[offset + 3],
        ]) as f32
            / FRACUNIT
    }
}

impl fmt::Debug for Lump {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "\nWadDirectory {{\n  lump_name: {},\n  lump_size: {},\n}}",
            &self.name,
            self.data.len()
        )
    }
}

/// "Where's All (the) Data": contains the WAD in memory, plus an array of
/// directories telling us where each data lump starts
pub struct WadData {
    pub(super) lumps: Vec<Lump>,
    file_path: PathBuf,
}

impl fmt::Debug for WadData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "\nWadLoader {{\n lumps: {:?},\n}}", self.lumps)
    }
}

impl WadData {
    /// Load and cache all lumps from a WAD file at `file_path`.
    pub fn new(file_path: &Path) -> WadData {
        let mut wad = WadData {
            lumps: Vec::new(),
            file_path: file_path.into(),
        };

        let mut file = File::open(file_path)
            .unwrap_or_else(|_| panic!("Could not open wad file: {:?}", &file_path));

        let file_len = file.metadata().unwrap().len();
        let mut file_data = Vec::with_capacity(file_len as usize);

        let wad_len = file
            .read_to_end(&mut file_data)
            .unwrap_or_else(|_| panic!("Could not read {:?}", &file_path));

        if wad_len != file_len as usize {
            panic!("Did not read complete WAD")
        }

        wad.cache_lumps(&file_data);
        wad
    }

    /// Append lumps from an additional WAD file (PWAD). Later lumps override
    /// earlier ones with the same name.
    pub fn add_file(&mut self, file_path: PathBuf) {
        let mut file =
            File::open(&file_path).unwrap_or_else(|_| panic!("Could not open {:?}", &file_path));

        let file_len = file.metadata().unwrap().len();
        let mut file_data = Vec::with_capacity(file_len as usize);

        let wad_len = file
            .read_to_end(&mut file_data)
            .unwrap_or_else(|_| panic!("Could not read {:?}", &file_path));

        if wad_len != file_len as usize {
            panic!("Did not read complete WAD")
        }

        self.cache_lumps(&file_data);
    }

    /// Parse the 12-byte WAD header (type + directory count + offset).
    fn read_header(file: &[u8]) -> WadHeader {
        let mut t = [0u8; 4];
        t[0] = file[0];
        t[1] = file[1];
        t[2] = file[2];
        t[3] = file[3];

        WadHeader {
            wad_type: t,
            dir_count: u32::from_le_bytes([file[4], file[5], file[6], file[7]]),
            dir_offset: u32::from_le_bytes([file[8], file[9], file[10], file[11]]),
        }
    }

    /// Read a single 16-byte directory entry at `ofs` and extract the lump
    /// data it points to.
    fn read_dir_data(ofs: usize, file: &[u8]) -> Lump {
        let mut n = [b'\n'; 8]; // length is 8 slots total
        for (i, slot) in n.iter_mut().enumerate() {
            *slot = file[ofs + 8 + i]
        }

        let size = i32::from_le_bytes([file[ofs + 4], file[ofs + 5], file[ofs + 6], file[ofs + 7]])
            as usize;
        let offset =
            i32::from_le_bytes([file[ofs], file[ofs + 1], file[ofs + 2], file[ofs + 3]]) as usize;

        Lump {
            data: file[offset..offset + size].to_owned(),
            name: str::from_utf8(&n)
                .expect("Invalid lump name")
                .trim_end_matches('\u{0}')
                .trim_end()
                .to_ascii_uppercase(), /* better to address this early to
                                        * avoid many casts later */
        }
    }

    /// Read the directory and cache every lump's data into `self.lumps`.
    fn cache_lumps(&mut self, file: &[u8]) {
        let header = Self::read_header(file);
        self.lumps.reserve_exact(header.dir_count as usize);

        for i in 0..(header.dir_count) {
            let dir = Self::read_dir_data((header.dir_offset + i * 16) as usize, file);
            self.lumps.push(dir);
        }
    }

    /// Find a general lump by name
    pub fn get_lump(&self, name: &str) -> Option<&Lump> {
        self.lumps
            .iter()
            .rev()
            .find(|lump| lump.name == name.to_ascii_uppercase())
    }

    /// Find a lump by name, panicking if absent.
    pub(super) fn find_lump_or_panic(&self, name: &str) -> &Lump {
        for info in self.lumps.iter().rev() {
            if info.name == name.to_ascii_uppercase() {
                return info;
            }
        }
        panic!("Could not find lump {}", name);
    }

    /// Find a map marker by name and return the lump at the given offset.
    /// Searches in reverse so the last loaded WAD wins.
    pub(super) fn find_lump_for_map_or_panic(&self, map_name: &str, lump: MapLump) -> &Lump {
        for (idx, info) in self.lumps.iter().enumerate().rev() {
            if info.name == map_name.to_ascii_uppercase() {
                return &self.lumps[idx + lump as usize];
            }
        }
        panic!("Could not find lump {}", map_name);
    }

    pub(super) fn find_lump_for_map(&self, map_name: &str, lump: MapLump) -> Option<&Lump> {
        for (idx, info) in self.lumps.iter().enumerate().rev() {
            if info.name == map_name.to_ascii_uppercase() {
                return Some(&self.lumps[idx + lump as usize]);
            }
        }
        None
    }

    pub fn lump_exists(&self, lump_name: &str) -> bool {
        for lump in self.lumps.iter().rev() {
            if lump.name == lump_name.to_ascii_uppercase() {
                return true;
            }
        }
        false
    }

    /// Parse the UMAPINFO lump if present.
    pub fn umapinfo(&self) -> Option<crate::umapinfo::UMapInfo> {
        let lump = self.get_lump("UMAPINFO")?;
        let text = std::str::from_utf8(&lump.data).ok()?;
        match crate::umapinfo::parse(text) {
            Ok(info) => Some(info),
            Err(e) => {
                log::warn!("Failed to parse UMAPINFO: {}", e);
                None
            }
        }
    }

    /// Parse the MAPINFO lump if present.
    pub fn mapinfo(&self) -> Option<crate::umapinfo::UMapInfo> {
        let lump = self.get_lump("MAPINFO")?;
        let text = std::str::from_utf8(&lump.data).ok()?;
        match crate::umapinfo::parse_mapinfo(text) {
            Ok(info) => Some(info),
            Err(e) => {
                log::warn!("Failed to parse MAPINFO: {}", e);
                None
            }
        }
    }

    /// Parse the ZMAPINFO lump if present.
    pub fn zmapinfo(&self) -> Option<crate::umapinfo::UMapInfo> {
        let lump = self.get_lump("ZMAPINFO")?;
        let text = std::str::from_utf8(&lump.data).ok()?;
        match crate::umapinfo::parse(text) {
            Ok(info) => Some(info),
            Err(e) => {
                log::warn!("Failed to parse ZMAPINFO: {}", e);
                None
            }
        }
    }

    /// Returns UMAPINFO if present, then ZMAPINFO, then MAPINFO.
    pub fn map_info(&self) -> Option<crate::umapinfo::UMapInfo> {
        self.umapinfo()
            .or_else(|| self.zmapinfo())
            .or_else(|| self.mapinfo())
    }

    pub fn lumps(&self) -> &[Lump] {
        &self.lumps
    }

    pub fn wad_name(&self) -> &str {
        self.file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
    }

    /// Parse the BLOCKMAP lump for `map_name` into a grid of linedef groups.
    pub fn read_blockmap(&self, map_name: &str) -> Option<WadBlockMap> {
        if let Some(info) = self.find_lump_for_map(map_name, MapLump::Blockmap) {
            if info.data.len() == 0 {
                return None;
            }

            let w = info.read_i16(4) as usize;
            let h = info.read_i16(6) as usize;
            let word_len = 2;
            let ofs = 8; //info.offset;
            let len = ofs + w * h * word_len;
            let mut line_groups = Vec::with_capacity(info.data.len() / word_len);
            for i in (ofs..len).step_by(2) {
                let mut start =
                    i16::from_le_bytes([info.data[i], info.data[i + 1]]) as usize * word_len;
                while start < info.data.len() {
                    let line = i16::from_le_bytes([info.data[start], info.data[start + 1]]);
                    line_groups.push(line);
                    if line == -1 {
                        break;
                    }
                    start += word_len;
                }
            }

            return Some(WadBlockMap::new(
                info.read_i16(0),
                info.read_i16(2),
                info.read_i16(4),
                info.read_i16(6),
                line_groups,
            ));
        }
        None
    }

    /// Return a copy of the REJECT lump for `map_name`, if present and
    /// non-empty.
    pub fn read_rejects(&self, map_name: &str) -> Option<Vec<u8>> {
        if let Some(info) = self.find_lump_for_map(map_name, MapLump::Reject) {
            if info.data.len() == 0 {
                return None;
            }
            return Some(info.data.clone());
        }
        None
    }

    /// Compute a hash of the BSP geometry lumps (NODES + SEGS + SSECTORS)
    /// for cache invalidation.
    pub fn map_bsp_hash(&self, map_name: &str) -> Option<u64> {
        let nodes = self.find_lump_for_map(map_name, MapLump::Nodes)?;
        let segs = self.find_lump_for_map(map_name, MapLump::Segs)?;
        let subs = self.find_lump_for_map(map_name, MapLump::SubSectors)?;

        let mut hasher = DefaultHasher::new();
        nodes.data.hash(&mut hasher);
        segs.data.hash(&mut hasher);
        subs.data.hash(&mut hasher);

        Some(hasher.finish())
    }
}

#[cfg(test)]
mod tests {
    use crate::wad::WadData;
    use std::fs::File;
    use std::io::Read;
    use std::path::PathBuf;
    use test_utils::doom1_wad_path;

    fn read_file(file_path: PathBuf) -> Vec<u8> {
        let mut file = File::open(&file_path).unwrap();
        let mut data = Vec::new();
        file.read_to_end(&mut data).unwrap();
        data
    }

    #[test]
    fn read_header() {
        let wad = read_file(doom1_wad_path());
        let header = WadData::read_header(&wad);
        assert_eq!(header.wad_type(), "IWAD");
    }

    #[test]
    fn read_single_dir() {
        let wad = read_file(doom1_wad_path());
        let header = WadData::read_header(&wad);
        let dir = WadData::read_dir_data(header.dir_offset as usize, &wad);
        assert!(!dir.name.is_empty());
    }

    #[test]
    fn read_all_dirs() {
        let wad = WadData::new(&doom1_wad_path());
        let file = read_file(doom1_wad_path());
        let header = WadData::read_header(&file);
        assert_eq!(wad.lumps.len(), header.dir_count as usize);
    }
}
