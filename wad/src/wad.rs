use std::fmt::Display;
use std::fs::File;
use std::io::prelude::*;
use std::path::PathBuf;
use std::{fmt, str};

use crate::types::WadBlockMap;

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

impl Display for MapLump {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MapLump::Things => write!(f, "THINGS"),
            MapLump::LineDefs => write!(f, "LINEDEFS"),
            MapLump::SideDefs => write!(f, "SIDEDEFS"),
            MapLump::Vertexes => write!(f, "VERTEXES"),
            MapLump::Segs => write!(f, "SEGS"),
            MapLump::SSectors => write!(f, "SSECTORS"),
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
        const FRACUNIT: f32 = (1 << 16) as f32;

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
}

impl fmt::Debug for WadData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "\nWadLoader {{\n lumps: {:?},\n}}", self.lumps)
    }
}

impl WadData {
    pub fn new(file_path: PathBuf) -> WadData {
        let mut wad = WadData { lumps: Vec::new() };

        let mut file = File::open(&file_path)
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

    /// Find a general lump by name
    pub(super) fn find_lump_or_panic(&self, name: &str) -> &Lump {
        for info in self.lumps.iter().rev() {
            if info.name == name.to_ascii_uppercase() {
                return info;
            }
        }
        panic!("Could not find lump {}", name);
    }

    /// Find the map name and adds the desired lump offset
    pub(super) fn find_lump_for_map_or_panic(&self, map_name: &str, lump: MapLump) -> &Lump {
        for (idx, info) in self.lumps.iter().enumerate().rev() {
            if info.name == map_name.to_ascii_uppercase() {
                return &self.lumps[idx + lump as usize];
            }
        }
        panic!("Could not find lump {}", map_name);
    }

    pub fn lump_exists(&self, lump_name: &str) -> bool {
        for lump in self.lumps.iter().rev() {
            if lump.name == lump_name.to_ascii_uppercase() {
                return true;
            }
        }
        false
    }

    pub fn read_blockmap(&self, map_name: &str) -> WadBlockMap {
        let info = self.find_lump_for_map_or_panic(map_name, MapLump::Blockmap);
        // let file = &self.file_data[info.handle];

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

        WadBlockMap::new(
            info.read_i16(0),
            info.read_i16(2),
            info.read_i16(4),
            info.read_i16(6),
            line_groups,
        )
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Read;
    use std::path::PathBuf;

    use crate::types::WadPatch;
    use crate::wad::WadData;
    use crate::MapLump;

    fn read_file(file_path: PathBuf) -> Vec<u8> {
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
        file_data
    }

    #[test]
    fn load_wad() {
        let wad = WadData::new("../doom1.wad".into());
        assert_eq!(wad.lumps.len(), 1243);
    }

    #[test]
    fn read_header() {
        let wad = read_file("../doom1.wad".into());
        let header = WadData::read_header(&wad);
        assert_eq!(header.wad_type(), "IWAD");
    }

    #[test]
    #[ignore = "sunder.wad can't be included in git"]
    fn read_header_sunder() {
        let wad = read_file("../sunder.wad".into());
        let header = WadData::read_header(&wad);
        assert_eq!(header.wad_type(), "PWAD");
    }

    #[test]
    fn read_single_dir() {
        let wad = read_file("../doom1.wad".into());
        let header = WadData::read_header(&wad);
        let dir = WadData::read_dir_data((header.dir_offset) as usize, &wad);
        dbg!(&dir);
    }

    #[test]
    fn read_all_dirs() {
        let wad = WadData::new("../doom1.wad".into());

        for i in 0..18 {
            dbg!("{:?}", &wad.lumps[i]);
        }

        let file = read_file("../doom1.wad".into());
        let header = WadData::read_header(&file);

        assert_eq!(wad.lumps.len(), header.dir_count as usize);
    }

    #[test]
    fn find_e1m1_things() {
        let wad = WadData::new("../doom1.wad".into());
        let things_lump = wad.find_lump_for_map_or_panic("E1M1", MapLump::Things);
        assert_eq!(things_lump.name, "THINGS");
    }

    #[test]
    fn find_e1m2_vertexes() {
        let wad = WadData::new("../doom1.wad".into());
        let things_lump = wad.find_lump_for_map_or_panic("E1M2", MapLump::Vertexes);
        assert_eq!(things_lump.name, MapLump::Vertexes.to_string());
    }

    #[test]
    fn find_texture_lump() {
        let wad = WadData::new("../doom1.wad".into());
        let _tex = wad.find_lump_or_panic("TEXTURE1");
        assert_eq!(_tex.name, "TEXTURE1");
        assert_eq!(_tex.data.len(), 9234);
    }

    #[test]
    fn find_playpal_lump() {
        let wad = WadData::new("../doom1.wad".into());
        let pal_lump = wad.find_lump_or_panic("PLAYPAL");
        assert_eq!(pal_lump.name, "PLAYPAL");
        assert_eq!(pal_lump.data.len(), 10752);
    }

    #[test]
    fn check_image_patch() {
        let wad = WadData::new("../doom1.wad".into());
        let lump = wad.find_lump_or_panic("WALL01_7");
        assert_eq!(lump.name, "WALL01_7");
        assert_eq!(lump.data.len(), 1304);

        let patch = WadPatch::from_lump(lump);

        assert_eq!(patch.columns[0].y_offset, 0);
        assert_eq!(patch.columns[15].y_offset, 255);
        assert_eq!(patch.columns[15].pixels.len(), 0);
        //let end = wad.read_byte(col_start + len as usize + 1, file);
        //assert_eq!(end, 255);
    }

    #[ignore]
    #[test]
    fn load_sigil() {
        let file = read_file("/home/luke/DOOM/sigil.wad".into());
        let header = WadData::read_header(&file);
        assert_eq!(header.wad_type(), "PWAD");
        assert_eq!(header.wad_type(), "PWAD");

        let mut wad = WadData::new("/home/luke/DOOM/doom.wad".into());
        assert_eq!(wad.lumps.len(), 2306);
        wad.add_file("/home/luke/DOOM/sigil.wad".into());
        assert_eq!(wad.lumps.len(), 2452);

        let things_lump = wad.find_lump_for_map_or_panic("E3M2", MapLump::Vertexes);
        assert_eq!(things_lump.name, MapLump::Vertexes.to_string());

        let things_lump = wad.find_lump_for_map_or_panic("E5M1", MapLump::Vertexes);
        assert_eq!(things_lump.name, MapLump::Vertexes.to_string());

        let pnames = wad.find_lump_or_panic("PNAMES");
        assert_eq!(pnames.name, "PNAMES");
        let pnames_collect: Vec<String> = wad.pnames_iter().collect();
        // This is a flat
        assert!(pnames_collect.contains(&String::from("SKY5")));

        let mut iter = wad.thing_iter("E5M1");
        // All verified with SLADE

        let next = iter.next().unwrap();
        assert_eq!(next.x, -208);
        assert_eq!(next.y, 72);
        assert_eq!(next.angle, 270);
        assert_eq!(next.kind, 2001);
        assert_eq!(next.flags, 7);
    }

    #[test]
    #[ignore = "sunder.wad can't be included in git"]
    fn load_sunder() {
        let file = read_file("../sunder.wad".into());
        let header = WadData::read_header(&file);
        assert_eq!(header.wad_type(), "PWAD");
        assert_eq!(header.wad_type(), "PWAD");

        let wad = WadData::new("../sunder.wad".into());
        assert_eq!(wad.lumps.len(), 2530);

        let things_lump = wad.find_lump_for_map_or_panic("MAP10", MapLump::Vertexes);
        assert_eq!(things_lump.name, MapLump::Vertexes.to_string());

        let things_lump = wad.find_lump_for_map_or_panic("MAP13", MapLump::Vertexes);
        assert_eq!(things_lump.name, MapLump::Vertexes.to_string());

        let pnames = wad.find_lump_or_panic("PNAMES");
        assert_eq!(pnames.name, "PNAMES");
        let pnames_collect: Vec<String> = wad.pnames_iter().collect();
        // This is a flat
        assert!(pnames_collect.contains(&String::from("BODIES")));

        let _: Vec<WadPatch> = wad.patches_iter().collect();
        // let mut iter = wad.thing_iter("MAP10");
        // All verified with SLADE

        // let next = iter.next().unwrap();
        // assert_eq!(next.x, -208);
        // assert_eq!(next.y, 72);
        // assert_eq!(next.angle, 270);
        // assert_eq!(next.kind, 2001);
        // assert_eq!(next.flags, 7);
    }

    #[test]
    fn find_e1m1_blockmap() {
        let wad = WadData::new("../doom1.wad".into());
        let things_lump = wad.find_lump_for_map_or_panic("E1M1", MapLump::Blockmap);
        assert_eq!(things_lump.name, "BLOCKMAP");

        let blockmap = wad.read_blockmap("E1M1");
        assert_eq!(blockmap.x_origin, -768 + -8); // -776 confirmed, needs
                                                  // conversion to float
        assert_eq!(blockmap.y_origin, -4864 + -8); // -4872 confirmed, needs conversion to float
        assert_eq!(blockmap.columns, 36); // confirmed
        assert_eq!(blockmap.rows, 23); // confirmed

        // DOOM1.wad, E1M1
        let blocks = [
            0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0,
            -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1,
            0, -1, 0, -1, 0, -1, 0, 329, 330, 332, -1, 0, 332, 347, 348, 349, 350, -1, 0, 328, 332,
            -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1,
            0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0,
            -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, 327, 330, 331, -1, 0,
            318, 319, 320, 321, 322, 323, 324, 325, 326, 327, 333, -1, 0, 326, 328, -1, 0, -1, 0,
            -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1,
            0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0,
            -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, 305, 311, -1, 0, 305, -1, 0, 305, 307, 315,
            316, 317, 318, 319, 342, 343, 344, 345, 346, -1, 0, 304, 306, 315, 316, -1, 0, 304, -1,
            0, 304, 312, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1,
            0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0,
            -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, 311, -1, 0, 287, 288, 289, -1,
            0, -1, 0, 284, 285, 286, -1, 0, -1, 0, 312, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0,
            -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1,
            0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0,
            311, -1, 0, 287, 289, 296, 297, 298, 299, 309, 313, -1, 0, 309, -1, 0, 284, 286, 292,
            293, 301, 303, 309, 314, -1, 0, -1, 0, 312, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0,
            -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1,
            0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0,
            279, 311, -1, 0, 279, 280, 281, 290, 291, 299, 300, 308, 313, 336, -1, 0, 308, -1, 0,
            282, 283, 294, 295, 302, 303, 308, 314, 339, -1, 0, 278, 283, -1, 0, 278, 312, -1, 0,
            -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1,
            0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0,
            215, 261, -1, 0, 215, 222, 441, 442, -1, 0, 223, 256, 440, 441, -1, 0, 256, -1, 0, 256,
            -1, 0, -1, 0, 336, -1, 0, 249, 310, 334, 336, 337, 340, 341, -1, 0, 190, 310, 335, 338,
            339, 340, 341, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0,
            -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1,
            0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, 211, 212, 220, 221, 261, 262, -1, 0, 213, 214,
            220, 221, 222, 259, 260, 443, 444, -1, 0, 223, 257, 444, 445, -1, 0, 257, -1, 0, 230,
            255, 256, -1, 0, 227, 228, 229, 230, 232, 240, 241, 242, 255, -1, 0, 224, 225, 226,
            238, 239, 240, 244, 245, 248, -1, 0, 245, 247, 249, -1, 0, 190, -1, 0, 429, 436, 437,
            -1, 0, 428, 431, 436, -1, 0, 428, 465, 473, 474, -1, 0, 451, 452, 466, 473, -1, 0, 452,
            -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1,
            0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, 48, -1, 0, 48, 161, -1, 0, 161, -1, 0,
            161, 205, 253, -1, 0, 209, 210, 211, 217, 218, 219, 448, -1, 0, 160, 203, 207, 208,
            213, 217, 218, 219, 254, 447, -1, 0, 160, -1, 0, 159, 160, 257, -1, 0, 159, 230, 231,
            257, 258, -1, 0, 227, 228, 229, 231, 233, 234, 235, -1, 0, 224, 225, 226, 235, 236,
            237, 243, 246, 248, -1, 0, 191, 246, 247, -1, 0, 190, -1, 0, 429, 438, 439, -1, 0, 430,
            431, 439, -1, 0, 450, 465, 471, 472, -1, 0, 453, 454, 466, 472, -1, 0, 452, 457, -1, 0,
            126, -1, 0, 125, 126, -1, 0, 125, -1, 0, 125, -1, 0, 125, 130, -1, 0, 125, 130, -1, 0,
            124, 125, 130, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, 6, -1, 0, 4, 6, -1, 0, 0, 1,
            2, 3, 4, 49, -1, 0, 3, 5, -1, 0, 37, 47, 252, -1, 0, 47, 252, -1, 0, 47, 48, 252, -1,
            0, 252, -1, 0, 252, -1, 0, 205, 206, 251, 252, 253, -1, 0, 206, 216, 251, 448, 449, -1,
            0, 203, 204, 216, 250, 251, 254, 446, 447, -1, 0, 250, -1, 0, 159, 250, -1, 0, 159,
            250, -1, 0, 159, 179, 189, 250, -1, 0, 189, -1, 0, 189, 191, 194, -1, 0, 188, 190, 194,
            -1, 0, 188, 198, 199, -1, 0, 430, 434, -1, 0, 450, 454, -1, 0, 454, 457, -1, 0, 457,
            -1, 0, 126, 127, -1, 0, 130, 131, -1, 0, 130, -1, 0, 130, -1, 0, 130, -1, 0, -1, 0,
            113, 124, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, 8, 55, -1, 0, 6, 8, -1, 0, 6, -1, 0, -1,
            0, 5, -1, 0, 5, 7, 25, 37, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1,
            0, -1, 0, -1, 0, 178, 179, 189, 196, -1, 0, 193, 196, -1, 0, 183, 193, 194, 196, -1, 0,
            197, 198, -1, 0, 198, 199, 201, 202, -1, 0, 199, 201, 202, 270, 434, 435, -1, 0, 269,
            270, 432, 433, 450, 454, 455, -1, 0, 457, 458, -1, 0, -1, 0, 127, -1, 0, 131, -1, 0,
            -1, 0, 110, 111, -1, 0, 104, 111, 118, -1, 0, 97, 105, 112, 113, 118, -1, 0, 94, 95,
            96, 97, 113, -1, 0, 69, 94, -1, 0, 69, 141, 142, 144, -1, 0, 68, 69, -1, 0, 63, -1, 0,
            55, 57, 62, 63, -1, 0, -1, 0, 9, 10, 11, 12, 39, 51, 52, 53, -1, 0, 39, 51, -1, 0, 13,
            14, 15, 16, 39, 40, 51, -1, 0, 25, 26, 31, 33, 37, 40, -1, 0, -1, 0, 170, 171, -1, 0,
            170, -1, 0, 169, 170, -1, 0, 169, -1, 0, 168, 169, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0,
            178, 179, 180, 193, 275, 276, -1, 0, 193, -1, 0, 183, 184, -1, 0, 183, 184, 197, -1, 0,
            197, -1, 0, 200, 201, 268, -1, 0, 200, 268, 269, 455, 456, -1, 0, 456, 458, 459, -1, 0,
            459, -1, 0, 127, -1, 0, 131, -1, 0, -1, 0, 100, 106, 110, 121, 122, -1, 0, 100, 104,
            117, 352, 353, 354, 355, 357, 358, 359, -1, 0, 97, 105, 116, 117, 135, 136, -1, 0, 95,
            -1, 0, 73, 74, 75, 76, 86, 87, 88, 91, 95, -1, 0, 76, 77, 78, 79, 89, 90, 142, 143,
            144, -1, 0, 64, 65, 67, 68, -1, 0, 58, 63, 65, -1, 0, 57, -1, 0, -1, 0, 52, 53, -1, 0,
            -1, 0, -1, 0, 26, 27, 31, 32, 36, -1, 0, 171, -1, 0, 171, -1, 0, -1, 0, -1, 0, -1, 0,
            167, 168, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, 275, 276, -1, 0, 184, 186, -1, 0, 184,
            185, -1, 0, 185, -1, 0, 185, 197, -1, 0, 268, -1, 0, 268, -1, 0, 456, 460, -1, 0, 459,
            461, -1, 0, 127, -1, 0, 131, -1, 0, -1, 0, 101, 107, 108, 109, 121, 122, -1, 0, 101,
            103, 108, 119, 120, 355, 356, 357, -1, 0, 98, 102, 114, 115, 116, 119, 120, 133, 134,
            -1, 0, 92, 98, -1, 0, 82, 83, 84, 85, 86, 87, 88, 91, 92, -1, 0, 79, 80, 81, 82, 89,
            90, 137, 138, 140, -1, 0, 64, 66, 71, 72, -1, 0, 59, 60, 66, -1, 0, 56, 57, 60, 61, -1,
            0, -1, 0, 17, 18, 19, 50, 52, 53, -1, 0, 50, -1, 0, 22, 23, 24, 41, 50, -1, 0, 27, 28,
            29, 30, 34, 35, 36, 38, 41, -1, 0, 164, 171, -1, 0, 164, -1, 0, 164, 165, -1, 0, 165,
            166, -1, 0, 166, -1, 0, 166, 167, -1, 0, -1, 0, 272, -1, 0, 272, 273, -1, 0, 273, -1,
            0, 273, 274, 275, 276, 277, -1, 0, 186, -1, 0, 185, 186, 187, -1, 0, 187, -1, 0, -1, 0,
            268, 464, 469, 470, -1, 0, 460, -1, 0, 460, 461, -1, 0, 461, -1, 0, 127, -1, 0, 131,
            -1, 0, -1, 0, -1, 0, -1, 0, 115, -1, 0, 92, 93, 98, 99, 115, -1, 0, 70, 93, -1, 0, 70,
            138, 139, 140, -1, 0, 70, 72, -1, 0, -1, 0, 56, -1, 0, -1, 0, 17, 19, 20, 54, -1, 0,
            54, -1, 0, 21, 22, 24, 41, 54, -1, 0, 28, 38, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0,
            -1, 0, 271, 272, -1, 0, 272, -1, 0, -1, 0, -1, 0, 177, 195, 263, 277, -1, 0, 195, 263,
            -1, 0, 181, 186, 263, -1, 0, 181, 182, 187, -1, 0, 181, 182, 464, 467, -1, 0, 464, 467,
            468, 469, -1, 0, 460, 462, 469, -1, 0, 461, -1, 0, -1, 0, 127, 128, -1, 0, 128, 131,
            132, -1, 0, 132, -1, 0, 132, -1, 0, 132, -1, 0, 132, -1, 0, 115, 123, 132, -1, 0, -1,
            0, -1, 0, -1, 0, -1, 0, 42, 56, -1, 0, 42, 43, -1, 0, 43, 44, -1, 0, 44, -1, 0, 44,
            145, 149, -1, 0, 28, 38, 45, 146, 149, -1, 0, 45, 46, -1, 0, 46, -1, 0, 46, -1, 0, 46,
            162, -1, 0, 162, 163, -1, 0, 163, -1, 0, 163, 271, -1, 0, -1, 0, -1, 0, -1, 0, 177,
            264, 351, -1, 0, 175, 195, 264, 351, -1, 0, 176, 181, 195, 267, -1, 0, 181, -1, 0, -1,
            0, 463, 468, -1, 0, 463, -1, 0, 461, 463, -1, 0, -1, 0, 128, -1, 0, 128, 129, -1, 0,
            129, -1, 0, 129, -1, 0, 129, -1, 0, 129, -1, 0, 123, 129, 132, -1, 0, -1, 0, -1, 0, -1,
            0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, 145, -1, 0, 146, -1, 0, -1, 0, 375, 376, 377, -1,
            0, 374, 375, 378, 379, -1, 0, 374, 378, -1, 0, 374, 378, -1, 0, 374, 378, -1, 0, 365,
            374, 378, 396, 397, -1, 0, 396, -1, 0, 380, 381, 396, -1, 0, 172, 381, 427, -1, 0, 172,
            192, 264, 265, -1, 0, 192, -1, 0, 267, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0,
            -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1,
            0, -1, 0, -1, 0, 145, -1, 0, 146, 147, -1, 0, 147, -1, 0, 362, 377, -1, 0, 362, 379,
            390, 391, -1, 0, 417, 418, 420, 421, -1, 0, 415, 418, 421, 422, -1, 0, 398, 411, -1, 0,
            398, -1, 0, 398, 399, -1, 0, 380, 381, 383, 385, -1, 0, 427, -1, 0, 174, 427, -1, 0,
            174, 192, -1, 0, 174, 192, 266, 267, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0,
            -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1,
            0, -1, 0, -1, 0, 145, 148, -1, 0, 148, -1, 0, 147, 148, 150, 153, -1, 0, 151, 152, 153,
            156, 157, -1, 0, 157, 384, 391, -1, 0, 416, 417, 419, 420, -1, 0, 415, 416, 419, 422,
            -1, 0, 409, 410, 411, 412, -1, 0, 407, 408, 409, 413, 414, -1, 0, 399, 406, 407, -1, 0,
            173, 364, 373, 382, 385, 394, 395, -1, 0, 173, 363, 364, -1, 0, 173, 174, 427, -1, 0,
            -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1,
            0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0,
            148, 150, 154, -1, 0, 151, 152, 154, 155, 158, 360, 361, -1, 0, 158, 360, 371, 384,
            392, 393, -1, 0, -1, 0, -1, 0, 412, -1, 0, 413, 414, -1, 0, 399, 406, -1, 0, 373, -1,
            0, 363, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0,
            -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1,
            0, -1, 0, -1, 0, -1, 0, -1, 0, 361, -1, 0, 371, -1, 0, 387, 388, 389, 423, 425, 426,
            -1, 0, 386, 387, 389, 423, 424, 425, -1, 0, 400, 401, 402, 403, 412, -1, 0, 400, 403,
            404, 405, 413, 414, -1, 0, 399, 400, 405, 406, -1, 0, 373, -1, 0, 363, -1, 0, -1, 0,
            -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1,
            0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0,
            -1, 0, 361, 370, -1, 0, 371, 372, -1, 0, 372, -1, 0, 372, -1, 0, 372, -1, 0, 372, -1,
            0, 372, -1, 0, 372, 373, -1, 0, 363, 369, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1,
            0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0,
            -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, 366, 370, -1, 0, 366,
            367, -1, 0, 367, -1, 0, 367, -1, 0, 367, -1, 0, 367, -1, 0, 367, -1, 0, 367, 368, -1,
            0, 368, 369, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0, -1,
        ];
        assert_eq!(blockmap.line_indexes, blocks);
        assert_eq!(blockmap.line_indexes.len(), blocks.len());
    }
}
