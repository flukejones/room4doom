use crate::lumps::{
    WadLineDef, WadNode, WadSector, WadSegment, WadSideDef, WadSubSector,
    WadThing, WadVertex,
};
use std::fs::File;
use std::io::prelude::*;
use std::mem::size_of;
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::ptr::NonNull;
use std::{fmt, str};

/// Functions purely as a safe fn wrapper around a `NonNull` because we know that
/// the Map structure is not going to change under us
pub struct WadPtr<T> {
    p: NonNull<T>,
}

impl<T> WadPtr<T> {
    pub fn new(t: &T) -> WadPtr<T> {
        WadPtr {
            p: NonNull::from(t),
        }
    }
}

impl<T> Clone for WadPtr<T> {
    fn clone(&self) -> WadPtr<T> { WadPtr { p: self.p } }
}

impl<T> Deref for WadPtr<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target { unsafe { self.p.as_ref() } }
}

impl<T> DerefMut for WadPtr<T> {
    fn deref_mut(&mut self) -> &mut Self::Target { unsafe { self.p.as_mut() } }
}

impl<T: fmt::Debug> fmt::Debug for WadPtr<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ptr->{:?}->{:#?}", self.p, unsafe { self.p.as_ref() })
    }
}

/// Used as an index to find a specific lump, typically combined
/// with an offset for example: find the index for lump named "E1M1"
/// in `self.wad_dirs` then combine this index with a `LumpIndex`
/// variant to get a specific lump.
#[allow(dead_code)]
pub enum LumpIndex {
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
    /// An array of signed short X, Y pairs (`Vertex`). All coordinates in this map
    /// block are indexes into this array
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
    /// 128x128 grid partition of the map LINEDEFS to accelerate collision
    /// detection
    Blockmap,
    Count,
}

impl ToString for LumpIndex {
    fn to_string(&self) -> String {
        match self {
            LumpIndex::Things => "THINGS".to_string(),
            LumpIndex::LineDefs => "LINEDEFS".to_string(),
            LumpIndex::SideDefs => "SIDEDEFS".to_string(),
            LumpIndex::Vertexes => "VERTEXES".to_string(),
            LumpIndex::Segs => "SEGS".to_string(),
            LumpIndex::SubSectors => "SSECTORS".to_string(),
            LumpIndex::Nodes => "NODES".to_string(),
            LumpIndex::Sectors => "SECTORS".to_string(),
            LumpIndex::Reject => "REJECT".to_string(),
            LumpIndex::Blockmap => "BLOCKMAP".to_string(),
            LumpIndex::Count => "COUNT".to_string(),
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
    wad_type:   [u8; 4],
    /// The count of "lumps" of data
    dir_count:  u32,
    /// Offset in bytes that the lump data starts at
    dir_offset: u32,
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
pub struct LumpInfo {
    /// The offset in bytes where the lump data starts
    lump_offset: usize,
    /// The size in bytes of the lump referenced
    lump_size:   usize,
    /// Name for the lump data
    lump_name:   String,
    /// The Index in to `WadData.file_data`
    file_handle: usize,
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
    file_data: Vec<Vec<u8>>,
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

        let mut file = File::open(&file_path)
            .unwrap_or_else(|_| panic!("Could not open {:?}", &file_path));

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
        let mut file = File::open(&file_path)
            .unwrap_or_else(|_| panic!("Could not open {:?}", &file_path));

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

    fn read_2_bytes(&self, offset: usize, file_idx: usize) -> i16 {
        (self.file_data[file_idx][offset + 1] as i16) << 8
            | (self.file_data[file_idx][offset] as i16)
    }

    fn read_4_bytes(&self, offset: usize, file_idx: usize) -> i32 {
        (self.file_data[file_idx][offset + 3] as i32) << 24
            | (self.file_data[file_idx][offset + 2] as i32) << 16
            | (self.file_data[file_idx][offset + 1] as i32) << 8
            | (self.file_data[file_idx][offset] as i32)
    }

    fn read_header(&self, file_idx: usize) -> WadHeader {
        let mut t = [0u8; 4];
        t[0] = self.file_data[file_idx][0];
        t[1] = self.file_data[file_idx][1];
        t[2] = self.file_data[file_idx][2];
        t[3] = self.file_data[file_idx][3];

        WadHeader {
            wad_type:   t,
            dir_count:  self.read_4_bytes(4, file_idx) as u32,
            dir_offset: self.read_4_bytes(8, file_idx) as u32,
        }
    }

    fn read_dir_data(&self, offset: usize, file_idx: usize) -> LumpInfo {
        let mut n = [0u8; 8]; // length is 8 slots total
        for i in 0..8 {
            n[i] = self.file_data[file_idx][offset + 8 + i]
        }

        LumpInfo {
            file_handle: file_idx,
            lump_offset: self.read_4_bytes(offset, file_idx) as usize,
            lump_size:   self.read_4_bytes(offset + 4, file_idx) as usize,
            lump_name:   str::from_utf8(&n)
                .expect("Invalid lump name")
                .trim_end_matches('\u{0}') // better to address this early to avoid many casts later
                .to_owned(),
        }
    }

    fn cache_lumps(&mut self, file_idx: usize) {
        let header = self.read_header(file_idx);
        self.lump_info.reserve_exact(header.dir_count as usize);

        for i in 0..(header.dir_count) {
            let dir = self
                .read_dir_data((header.dir_offset + i * 16) as usize, file_idx);
            self.lump_info.push(dir);
        }
    }

    fn find_lump_info(&self, name: &str) -> Option<&LumpInfo> {
        for info in self.lump_info.iter() {
            if info.lump_name == name {
                return Some(info);
            }
        }
        println!("Index not found for lump name: {}", name);
        None
    }

    fn read_lump_to_vec<F, T>(&self, lump_info: &LumpInfo, func: F) -> Vec<T>
    where
        F: Fn(usize) -> T, {
        let item_size = size_of::<T>();
        let item_count = lump_info.lump_size / item_size;

        let mut v: Vec<T> = Vec::with_capacity(item_count);
        for i in 0..item_count {
            let offset = (lump_info.lump_offset + i * item_size) as usize;
            v.push(func(offset));
        }
        v
    }

    fn find_lump_for_map_or_panic(
        &self,
        map_name: &str,
        lump: LumpIndex,
    ) -> &LumpInfo {
        for (idx, info) in self.lump_info.iter().enumerate() {
            if info.lump_name == map_name {
                return &self.lump_info[idx + lump as usize];
            }
        }
        panic!("Could not find {}", map_name);
    }

    /// A map index must be provided
    pub fn read_things(&self, map_name: &str) -> Vec<WadThing> {
        let info = self.find_lump_for_map_or_panic(map_name, LumpIndex::Things);

        self.read_lump_to_vec(info, |offset| {
            WadThing::new(
                self.read_2_bytes(offset, info.file_handle),
                self.read_2_bytes(offset + 2, info.file_handle),
                self.read_2_bytes(offset + 4, info.file_handle),
                self.read_2_bytes(offset + 6, info.file_handle),
                self.read_2_bytes(offset + 8, info.file_handle),
            )
        })
    }

    pub fn read_vertexes(&self, map_name: &str) -> Vec<WadVertex> {
        let info =
            self.find_lump_for_map_or_panic(map_name, LumpIndex::Vertexes);

        self.read_lump_to_vec(info, |offset| {
            WadVertex::new(
                self.read_2_bytes(offset, info.file_handle),
                self.read_2_bytes(offset + 2, info.file_handle),
            )
        })
    }

    pub fn read_sectors(&self, map_name: &str) -> Vec<WadSector> {
        let info =
            self.find_lump_for_map_or_panic(map_name, LumpIndex::Sectors);

        self.read_lump_to_vec(info, |offset| {
            WadSector::new(
                self.read_2_bytes(offset, info.file_handle) as i16,
                self.read_2_bytes(offset + 2, info.file_handle) as i16,
                &self.file_data[info.file_handle][offset + 4..offset + 12],
                &self.file_data[info.file_handle][offset + 12..offset + 20],
                self.read_2_bytes(offset + 20, info.file_handle),
                self.read_2_bytes(offset + 22, info.file_handle),
                self.read_2_bytes(offset + 24, info.file_handle),
            )
        })
    }

    pub fn read_sidedefs(&self, map_name: &str) -> Vec<WadSideDef> {
        let info =
            self.find_lump_for_map_or_panic(map_name, LumpIndex::SideDefs);

        self.read_lump_to_vec(info, |offset| {
            WadSideDef::new(
                self.read_2_bytes(offset, info.file_handle) as i16,
                self.read_2_bytes(offset + 2, info.file_handle) as i16,
                &self.file_data[info.file_handle][offset + 4..offset + 12],
                &self.file_data[info.file_handle][offset + 12..offset + 20],
                &self.file_data[info.file_handle][offset + 20..offset + 28],
                self.read_2_bytes(offset + 28, info.file_handle),
            )
        })
    }

    pub fn read_linedefs(&self, map_name: &str) -> Vec<WadLineDef> {
        let info =
            self.find_lump_for_map_or_panic(map_name, LumpIndex::LineDefs);

        self.read_lump_to_vec(info, |offset| {
            let back_sidedef = {
                let index = self.read_2_bytes(offset + 12, info.file_handle);
                if index < i16::MAX {
                    Some(index)
                } else {
                    None
                }
            };

            WadLineDef::new(
                self.read_2_bytes(offset, info.file_handle),
                self.read_2_bytes(offset + 2, info.file_handle),
                self.read_2_bytes(offset + 4, info.file_handle),
                self.read_2_bytes(offset + 6, info.file_handle),
                self.read_2_bytes(offset + 8, info.file_handle),
                self.read_2_bytes(offset + 10, info.file_handle),
                back_sidedef,
            )
        })
    }

    pub fn read_segments(&self, map_name: &str) -> Vec<WadSegment> {
        let info = self.find_lump_for_map_or_panic(map_name, LumpIndex::Segs);

        self.read_lump_to_vec(info, |offset| {
            WadSegment::new(
                self.read_2_bytes(offset, info.file_handle),
                self.read_2_bytes(offset + 2, info.file_handle),
                self.read_2_bytes(offset + 4, info.file_handle),
                self.read_2_bytes(offset + 6, info.file_handle),
                self.read_2_bytes(offset + 8, info.file_handle), // 0 front or 1 back
                self.read_2_bytes(offset + 10, info.file_handle),
            )
        })
    }

    pub fn read_subsectors(&self, map_name: &str) -> Vec<WadSubSector> {
        let info =
            self.find_lump_for_map_or_panic(map_name, LumpIndex::SubSectors);

        self.read_lump_to_vec(info, |offset| {
            WadSubSector::new(
                self.read_2_bytes(offset, info.file_handle),
                self.read_2_bytes(offset + 2, info.file_handle),
            )
        })
    }

    pub fn read_nodes(&self, map_name: &str) -> Vec<WadNode> {
        let info = self.find_lump_for_map_or_panic(map_name, LumpIndex::Nodes);

        self.read_lump_to_vec(info, |offset| {
            WadNode::new(
                self.read_2_bytes(offset, info.file_handle),
                self.read_2_bytes(offset + 2, info.file_handle),
                self.read_2_bytes(offset + 4, info.file_handle),
                self.read_2_bytes(offset + 6, info.file_handle),
                [
                    [
                        self.read_2_bytes(offset + 12, info.file_handle), // top
                        self.read_2_bytes(offset + 8, info.file_handle), // left
                        self.read_2_bytes(offset + 14, info.file_handle), // bottom
                        self.read_2_bytes(offset + 10, info.file_handle), // right
                    ],
                    [
                        self.read_2_bytes(offset + 20, info.file_handle),
                        self.read_2_bytes(offset + 16, info.file_handle),
                        self.read_2_bytes(offset + 22, info.file_handle),
                        self.read_2_bytes(offset + 18, info.file_handle),
                    ],
                ],
                self.read_2_bytes(offset + 24, info.file_handle) as u16,
                self.read_2_bytes(offset + 26, info.file_handle) as u16,
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::wad::WadData;
    use crate::LumpIndex;

    #[test]
    fn load_wad() {
        let wad = WadData::new("../doom1.wad".into());
        assert_eq!(wad.file_data[0].len(), 4225460);
    }

    #[test]
    fn read_two_bytes() {
        let wad = WadData::new("../doom1.wad".into());
        let x1 = wad.read_2_bytes(0, 0);
        dbg!(&x1);
        let x2 = wad.read_2_bytes(2, 0);
        dbg!(&x2);
    }

    #[test]
    fn read_four_bytes() {
        let wad = WadData::new("../doom1.wad".into());
        let x = wad.read_4_bytes(0, 0);
        dbg!(&x);

        let y = (wad.read_2_bytes(2, 0) as i32) << 16
            | (wad.read_2_bytes(0, 0) as i32);
        dbg!(&y);

        assert_eq!(x, y);
    }

    #[test]
    fn read_header() {
        let wad = WadData::new("../doom1.wad".into());

        let header = wad.read_header(0);
        dbg!(&header);
    }

    #[test]
    fn read_single_dir() {
        let wad = WadData::new("../doom1.wad".into());

        let header = wad.read_header(0);
        let dir = wad.read_dir_data((header.dir_offset) as usize, 0);
        dbg!(&dir);
    }

    #[test]
    fn read_all_dirs() {
        let wad = WadData::new("../doom1.wad".into());

        for i in 0..18 {
            dbg!("{:?}", &wad.lump_info[i]);
        }

        let header = wad.read_header(0);
        assert_eq!(wad.lump_info.len(), header.dir_count as usize);
    }

    #[test]
    fn find_e1m1() {
        let wad = WadData::new("../doom1.wad".into());

        assert_eq!(wad.lump_info[6].lump_name, "E1M1");

        let info = wad.find_lump_info("E1M1").unwrap();
        dbg!(&info.lump_name);
        dbg!(&info.file_handle);
        dbg!(&info.lump_offset);
        dbg!(&info.lump_size);
        assert_eq!(info.lump_name, "E1M1");
    }

    #[test]
    fn find_e1m1_things() {
        let wad = WadData::new("../doom1.wad".into());
        let things_lump =
            wad.find_lump_for_map_or_panic("E1M1", LumpIndex::Things);
        assert_eq!(things_lump.lump_name, "THINGS");
    }

    #[test]
    fn find_e1m2_vertexes() {
        let wad = WadData::new("../doom1.wad".into());
        let things_lump =
            wad.find_lump_for_map_or_panic("E1M2", LumpIndex::Vertexes);
        assert_eq!(things_lump.lump_name, LumpIndex::Vertexes.to_string());
    }

    #[test]
    #[ignore]
    fn load_sigil() {
        let mut wad = WadData::new("../doom.wad".into());
        assert_eq!(wad.lump_info.len(), 2306);
        wad.add_file("../sigil.wad".into());
        assert_eq!(wad.lump_info.len(), 2452);
        let things_lump =
            wad.find_lump_for_map_or_panic("E3M2", LumpIndex::Vertexes);
        assert_eq!(things_lump.lump_name, LumpIndex::Vertexes.to_string());
    }
}
