use std::fs::File;
use std::io::prelude::*;
use std::path::PathBuf;
use std::{fmt, str};

/// Used as an index to find a specific lump, typically combined
/// with an offset for example: find the index for lump named "E1M1"
/// in `self.wad_dirs` then combine this index with a `LumpIndex`
/// variant to get a specific lump.
#[allow(dead_code)]
pub(crate) enum Lumps {
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
    /// An array of signed short X, Y pairs (`Vertex`). All coordinates in this level
    /// block are indexes into this array
    Vertexes,
    /// Portions of lines cut due to Binary Space Partitioning (see page
    /// 202 in Fabien Sanglard's Game Engine Black Book - DOOM).
    /// Each `SubSectors`'s geometry is defined by the `Segs` which it contains
    Segs,
    /// Set of segments of a `LineDef` representing a convex subspace
    SSectors,
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

impl ToString for Lumps {
    fn to_string(&self) -> String {
        match self {
            Lumps::Things => "THINGS".to_string(),
            Lumps::LineDefs => "LINEDEFS".to_string(),
            Lumps::SideDefs => "SIDEDEFS".to_string(),
            Lumps::Vertexes => "VERTEXES".to_string(),
            Lumps::Segs => "SEGS".to_string(),
            Lumps::SSectors => "SSECTORS".to_string(),
            Lumps::Nodes => "NODES".to_string(),
            Lumps::Sectors => "SECTORS".to_string(),
            Lumps::Reject => "REJECT".to_string(),
            Lumps::Blockmap => "BLOCKMAP".to_string(),
            Lumps::Count => "COUNT".to_string(),
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
///
pub struct WadHeader {
    /// Will be either `IWAD` for game, or `PWAD` for patch
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

/// Contains the details for a lump of data: where it starts, the size of it, and the name
///
/// The directory structure in the WAD is as follows:
///
/// | Field Size | Data Type    | Content                                                    |
/// |------------|--------------|------------------------------------------------------------|
/// | 0x00-0x03  | unsigned int | Offset value to the start of the lump data in the WAD file |
/// | 0x04-0x07  | unsigned int | The size of the lump in bytes                              |
/// | 0x08-0x0f  | 8 ASCII char | ASCII holding the name of the lump                         |
///
pub(crate) struct LumpInfo {
    /// The offset in bytes where the lump data starts
    pub lump_offset: usize,
    /// The size in bytes of the lump referenced
    pub lump_size: usize,
    /// Name for the lump data
    pub lump_name: String,
    /// The Index in to `WadData.file_data`
    pub file_handle: usize,
}
impl fmt::Debug for LumpInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "\nWadDirectory {{\n  lump_name: {},\n  lump_size: {},\n  lump_offset: {},\n}}",
            &self.lump_name, self.lump_size, self.lump_offset
        )
    }
}

/// "Where's All (the) Data": contains the WAD in memory, plus an array of directories
/// telling us where each data lump starts
pub struct WadData {
    /// The WAD as an array of bytes read in to memory, the index is the handle
    pub(crate) file_data: Vec<Vec<u8>>,
    /// Tells us where each lump of data is
    lump_info: Vec<LumpInfo>,
}

impl fmt::Debug for WadData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "\nWadLoader {{\n lumps: {:?},\n}}", self.lump_info)
    }
}

impl WadData {
    pub fn new(file_path: PathBuf) -> WadData {
        let mut wad = WadData {
            file_data: Vec::new(),
            lump_info: Vec::new(),
        };

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

        wad.file_data.push(file_data);
        wad.cache_lumps(0);
        wad
    }

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

        self.file_data.push(file_data);
        self.cache_lumps(self.file_data.len() - 1);
    }

    pub(crate) fn read_2_bytes(&self, offset: usize, file: &[u8]) -> i16 {
        (file[offset + 1] as i16) << 8 | (file[offset] as i16)
    }

    pub(crate) fn read_4_bytes(&self, offset: usize, file: &[u8]) -> i32 {
        (file[offset + 3] as i32) << 24
            | (file[offset + 2] as i32) << 16
            | (file[offset + 1] as i32) << 8
            | (file[offset] as i32)
    }

    fn read_header(&self, file: &[u8]) -> WadHeader {
        let mut t = [0u8; 4];
        t[0] = file[0];
        t[1] = file[1];
        t[2] = file[2];
        t[3] = file[3];

        WadHeader {
            wad_type: t,
            dir_count: self.read_4_bytes(4, file) as u32,
            dir_offset: self.read_4_bytes(8, file) as u32,
        }
    }

    pub fn get_headers(&self) -> Vec<WadHeader> {
        self.file_data
            .iter()
            .map(|file| {
                let mut t = [0u8; 4];
                t[0] = file[0];
                t[1] = file[1];
                t[2] = file[2];
                t[3] = file[3];

                WadHeader {
                    wad_type: t,
                    dir_count: self.read_4_bytes(4, file) as u32,
                    dir_offset: self.read_4_bytes(8, file) as u32,
                }
            })
            .collect()
    }

    fn read_dir_data(&self, offset: usize, file_idx: usize) -> LumpInfo {
        let mut n = [0u8; 8]; // length is 8 slots total
        for (i, slot) in n.iter_mut().enumerate() {
            *slot = self.file_data[file_idx][offset + 8 + i]
        }
        let file = &self.file_data[file_idx];

        LumpInfo {
            file_handle: file_idx,
            lump_offset: self.read_4_bytes(offset, file) as usize,
            lump_size: self.read_4_bytes(offset + 4, file) as usize,
            lump_name: str::from_utf8(&n)
                .expect("Invalid lump name")
                .trim_end_matches('\u{0}') // better to address this early to avoid many casts later
                .to_owned(),
        }
    }

    fn cache_lumps(&mut self, file_idx: usize) {
        let file = &self.file_data[file_idx];
        let header = self.read_header(file);
        self.lump_info.reserve_exact(header.dir_count as usize);

        for i in 0..(header.dir_count) {
            let dir = self.read_dir_data((header.dir_offset + i * 16) as usize, file_idx);
            self.lump_info.push(dir);
        }
    }

    pub(crate) fn find_lump_for_map_or_panic(&self, map_name: &str, lump: Lumps) -> &LumpInfo {
        for (idx, info) in self.lump_info.iter().enumerate() {
            if info.lump_name == map_name {
                return &self.lump_info[idx + lump as usize];
            }
        }
        panic!("Could not find {}", map_name);
    }

    pub fn lump_exists(&self, lump_name: &str) -> bool {
        for lump in self.lump_info.iter().rev() {
            if lump.lump_name == lump_name {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use crate::wad::WadData;
    use crate::Lumps;

    #[test]
    fn load_wad() {
        let wad = WadData::new("../doom1.wad".into());
        assert_eq!(wad.file_data[0].len(), 4225460);
    }

    #[test]
    fn read_two_bytes() {
        let wad = WadData::new("../doom1.wad".into());
        let x1 = wad.read_2_bytes(0, &wad.file_data[0]);
        dbg!(&x1);
        let x2 = wad.read_2_bytes(2, &wad.file_data[0]);
        dbg!(&x2);
    }

    #[test]
    fn read_four_bytes() {
        let wad = WadData::new("../doom1.wad".into());
        let x = wad.read_4_bytes(0, &wad.file_data[0]);
        dbg!(&x);

        let y = (wad.read_2_bytes(2, &wad.file_data[0]) as i32) << 16
            | (wad.read_2_bytes(0, &wad.file_data[0]) as i32);
        dbg!(&y);

        assert_eq!(x, y);
    }

    #[test]
    fn read_header() {
        let wad = WadData::new("../doom1.wad".into());

        let header = wad.read_header(&wad.file_data[0]);
        dbg!(&header);

        let headers = wad.get_headers();
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].wad_type(), "IWAD");
    }

    #[test]
    fn read_single_dir() {
        let wad = WadData::new("../doom1.wad".into());

        let header = wad.read_header(&wad.file_data[0]);
        let dir = wad.read_dir_data((header.dir_offset) as usize, 0);
        dbg!(&dir);
    }

    #[test]
    fn read_all_dirs() {
        let wad = WadData::new("../doom1.wad".into());

        for i in 0..18 {
            dbg!("{:?}", &wad.lump_info[i]);
        }

        let header = wad.read_header(&wad.file_data[0]);
        assert_eq!(wad.lump_info.len(), header.dir_count as usize);
    }

    #[test]
    fn find_e1m1_things() {
        let wad = WadData::new("../doom1.wad".into());
        let things_lump = wad.find_lump_for_map_or_panic("E1M1", Lumps::Things);
        assert_eq!(things_lump.lump_name, "THINGS");
    }

    #[test]
    fn find_e1m1_blockmap() {
        let wad = WadData::new("../doom1.wad".into());
        let things_lump = wad.find_lump_for_map_or_panic("E1M1", Lumps::Blockmap);
        assert_eq!(things_lump.lump_name, "BLOCKMAP");
    }

    #[test]
    fn find_e1m2_vertexes() {
        let wad = WadData::new("../doom1.wad".into());
        let things_lump = wad.find_lump_for_map_or_panic("E1M2", Lumps::Vertexes);
        assert_eq!(things_lump.lump_name, Lumps::Vertexes.to_string());
    }

    #[test]
    #[ignore]
    fn load_sigil() {
        let mut wad = WadData::new("../doom.wad".into());
        assert_eq!(wad.lump_info.len(), 2306);
        wad.add_file("../sigil.wad".into());
        assert_eq!(wad.lump_info.len(), 2452);

        let headers = wad.get_headers();
        assert_eq!(headers.len(), 2);
        assert_eq!(headers[0].wad_type(), "IWAD");
        assert_eq!(headers[1].wad_type(), "PWAD");

        let things_lump = wad.find_lump_for_map_or_panic("E3M2", Lumps::Vertexes);
        assert_eq!(things_lump.lump_name, Lumps::Vertexes.to_string());

        let things_lump = wad.find_lump_for_map_or_panic("E5M1", Lumps::Vertexes);
        assert_eq!(things_lump.lump_name, Lumps::Vertexes.to_string());

        let mut iter = wad.thing_iter("E5M1");
        // All verified with SLADE

        let next = iter.next().unwrap();
        assert_eq!(next.x, -208);
        assert_eq!(next.y, 72);
        assert_eq!(next.angle, 270);
        assert_eq!(next.kind, 2001);
        assert_eq!(next.flags, 7);
    }
}
