use crate::map::{LineDef, Map, Sector, Vertex};
use std::fs::File;
use std::io::prelude::*;
use std::path::PathBuf;
use std::{fmt, str};

/// Used as an index to find a specific lump, typically combined
/// with an offset for example: find the index for lump named "E1M1"
/// in `self.wad_dirs` then combine this index with a `LumpIndex`
/// variant to get a specific lump.
enum LumpIndex {
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

/// Header which tells us the WAD type and where the data is
struct WadHeader {
    /// Will be either `IWAD` for game, or `PWAD` for patch
    wad_type: [u8; 4],
    /// The count of "lumps" of data
    dir_count: u32,
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
struct WadDirectory {
    /// The offset in bytes where the lump data starts
    lump_offset: u32,
    /// The size in bytes of the lump referenced
    lump_size: u32,
    /// Name for the lump data
    lump_name: String,
}
impl fmt::Debug for WadDirectory {
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
pub struct Wad {
    wad_file_path: PathBuf,
    /// The WAD as an array of bytes read in to memory
    wad_data: Vec<u8>,
    /// Tells us where each lump of data is
    wad_dirs: Vec<WadDirectory>,
}

impl fmt::Debug for Wad {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "\nWadLoader {{\n  wad_file_path: {:?},\n wad_dirs: {:?},\n}}",
            self.wad_file_path, self.wad_dirs
        )
    }
}

impl Wad {
    pub fn new<A>(file_path: A) -> Wad
    where
        A: Into<PathBuf>,
    {
        let mut wad = Wad {
            wad_file_path: file_path.into(),
            wad_data: Vec::new(),
            wad_dirs: Vec::new(),
        };

        let mut file = File::open(&wad.wad_file_path)
            .expect(&format!("Could not open {:?}", &wad.wad_file_path));

        let file_len = file.metadata().unwrap().len();
        wad.wad_data.reserve_exact(file_len as usize);
        let wad_len = file
            .read_to_end(&mut wad.wad_data)
            .expect(&format!("Could not read {:?}", &wad.wad_file_path));

        if wad_len != file_len as usize {
            panic!("Did not read complete WAD")
        }

        wad
    }

    fn read_2_bytes(&self, offset: usize) -> u16 {
        (self.wad_data[offset + 1] as u16) << 8 | (self.wad_data[offset] as u16)
    }

    fn read_4_bytes(&self, offset: usize) -> u32 {
        (self.wad_data[offset + 3] as u32) << 24
            | (self.wad_data[offset + 2] as u32) << 16
            | (self.wad_data[offset + 1] as u32) << 8
            | (self.wad_data[offset] as u32)
    }

    fn read_header(&self, offset: usize) -> WadHeader {
        let mut t = [0u8; 4];
        t[0] = self.wad_data[offset];
        t[1] = self.wad_data[offset + 1];
        t[2] = self.wad_data[offset + 2];
        t[3] = self.wad_data[offset + 3];

        WadHeader {
            wad_type: t,
            dir_count: self.read_4_bytes(offset + 4),
            dir_offset: self.read_4_bytes(offset + 8),
        }
    }

    fn read_dir_data(&self, offset: usize) -> WadDirectory {
        let mut n = [0u8; 8]; // length is 8 slots total
        for i in 0..8 {
            n[i] = self.wad_data[offset + 8 + i]
        }

        WadDirectory {
            lump_offset: self.read_4_bytes(offset),
            lump_size: self.read_4_bytes(offset + 4),
            lump_name: str::from_utf8(&n)
                .expect("Invalid lump name")
                .trim_end_matches("\u{0}") // better to address this early to avoid many casts later
                .to_owned(),
        }
    }

    pub fn read_directories(&mut self) {
        let header = self.read_header(0);
        self.wad_dirs.reserve_exact(header.dir_count as usize);

        for i in 0..(header.dir_count) {
            let dir = self.read_dir_data((header.dir_offset + i * 16) as usize);
            self.wad_dirs.push(dir);
        }
    }

    pub fn find_lump_index(&self, name: &str) -> usize {
        for (i, dir) in self.wad_dirs.iter().enumerate() {
            if &dir.lump_name == name {
                return i;
            }
        }
        panic!("Index not found for lump name: {}", name);
    }

    fn read_map_vertexes(&self, mut index: usize, map: &mut Map) {
        index += LumpIndex::Vertexes as usize;

        if self.wad_dirs[index].lump_name != "VERTEXES" {
            panic!(
                "Invalid vertex lump index: {}, {}",
                index, self.wad_dirs[index].lump_name
            )
        }

        // Rust sizes can differ to C/C++, we know the Vertex data is two i16 so just use this
        let vertex_count = self.wad_dirs[index].lump_size / 4; // u32 == 4 bytes

        for i in 0..vertex_count {
            let offset = (self.wad_dirs[index].lump_offset + i * 4) as usize;
            map.add_vertex(Vertex::new(
                self.read_2_bytes(offset) as i16,
                self.read_2_bytes(offset + 2) as i16,
            ));
        }
    }

    fn read_map_linedefs(&self, mut index: usize, map: &mut Map) {
        index += LumpIndex::LineDefs as usize;

        if self.wad_dirs[index].lump_name != "LINEDEFS" {
            panic!(
                "Invalid vertex lump index: {}, {}",
                index, self.wad_dirs[index].lump_name
            )
        }

        let linedef_byte_size = std::mem::size_of::<LineDef>() as u32;
        let linedef_count = self.wad_dirs[index].lump_size / linedef_byte_size;

        for i in 0..linedef_count {
            let offset = (self.wad_dirs[index].lump_offset + i * linedef_byte_size) as usize;
            map.add_linedef(LineDef::new(
                self.read_2_bytes(offset) as i16,
                self.read_2_bytes(offset + 2) as i16,
                self.read_2_bytes(offset + 4),
                self.read_2_bytes(offset + 6),
                self.read_2_bytes(offset + 8),
                self.read_2_bytes(offset + 10),
                self.read_2_bytes(offset + 12),
            ));
        }
    }

    fn read_map_sectors(&self, mut index: usize, map: &mut Map) {
        index += LumpIndex::Sectors as usize;

        if self.wad_dirs[index].lump_name != "SECTORS" {
            panic!(
                "Invalid sector lump index: {}, {}",
                index, self.wad_dirs[index].lump_name
            )
        }

        let sector_byte_size = 26; // this will never change size
        let sector_count = self.wad_dirs[index].lump_size / sector_byte_size;

        for i in 0..sector_count {
            let offset = (self.wad_dirs[index].lump_offset + i * sector_byte_size) as usize;

            let mut floor_tex = [0u8; 8];
            for i in 0..8 {
                floor_tex[i] = self.wad_data[offset + i + 4]
            }

            let mut ceil_tex = [0u8; 8];
            for i in 0..8 {
                ceil_tex[i] = self.wad_data[offset + i + 12]
            }

            map.add_sector(Sector::new(
                self.read_2_bytes(offset) as i16,
                self.read_2_bytes(offset + 2) as i16,
                str::from_utf8(&floor_tex)
                    .expect("Invalid floor tex name")
                    .trim_end_matches("\u{0}") // better to address this early to avoid many casts later
                    .to_owned(),
                str::from_utf8(&ceil_tex)
                    .expect("Invalid ceiling tex name")
                    .trim_end_matches("\u{0}") // better to address this early to avoid many casts later
                    .to_owned(),
                self.read_2_bytes(offset + 20),
                self.read_2_bytes(offset + 22),
                self.read_2_bytes(offset + 24),
            ))
        }
    }

    pub fn load_map(&self, mut map: &mut Map) {
        let i = self.find_lump_index("E1M1");
        self.read_map_vertexes(i, &mut map);
        self.read_map_linedefs(i, &mut map);
        self.read_map_sectors(i, &mut map);
        // Sector, Sidedef, Linedef, Seg all need to be preprocessed before
        // storing in map struct
    }
}

#[cfg(test)]
mod tests {
    use crate::map;
    use crate::wad::Wad;

    #[test]
    fn load_wad() {
        let mut wad = Wad::new("../doom1.wad");
        assert_eq!(wad.wad_data.len(), 4225460);
    }

    #[test]
    fn read_two_bytes() {
        let wad = Wad::new("../doom1.wad");
        let x1 = wad.read_2_bytes(0);
        dbg!(&x1);
        let x2 = wad.read_2_bytes(2);
        dbg!(&x2);
    }

    #[test]
    fn read_four_bytes() {
        let wad = Wad::new("../doom1.wad");
        let x = wad.read_4_bytes(0);
        dbg!(&x);

        let y = (wad.read_2_bytes(2) as u32) << 16 | (wad.read_2_bytes(0) as u32);
        dbg!(&y);

        assert_eq!(x, y);
    }

    #[test]
    fn read_header() {
        let wad = Wad::new("../doom1.wad");

        let header = wad.read_header(0);
        dbg!(&header);
    }

    #[test]
    fn read_single_dir() {
        let wad = Wad::new("../doom1.wad");

        let header = wad.read_header(0);
        let dir = wad.read_dir_data((header.dir_offset) as usize);
        dbg!(&dir);
    }

    #[test]
    fn read_all_dirs() {
        let mut wad = Wad::new("../doom1.wad");
        wad.read_directories();

        for i in 6..18 {
            dbg!(&wad.wad_dirs[i]);
        }

        let header = wad.read_header(0);
        assert_eq!(wad.wad_dirs.len(), header.dir_count as usize);
    }

    #[test]
    fn find_e1m1() {
        let mut wad = Wad::new("../doom1.wad");
        wad.read_directories();

        assert_eq!(wad.wad_dirs[6].lump_name, "E1M1");

        let i = wad.find_lump_index("E1M1");
        assert_eq!(wad.wad_dirs[i].lump_name, "E1M1");
    }
}
