use std::fs::File;
use std::io::prelude::*;
use std::path::PathBuf;
use std::{fmt, str};

pub struct WadHeader {
    wad_type: [u8; 4],
    dir_count: u32,
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

pub struct WadDirectory {
    lump_offset: u32,
    lump_size: u32,
    lump_name: [u8; 8],
}
impl fmt::Debug for WadDirectory {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "\nWadDirectory {{\n  lump_name: {},\n  lump_size: {},\n  lump_offset: {},\n}}",
            str::from_utf8(&self.lump_name).unwrap(),
            self.lump_size,
            self.lump_offset
        )
    }
}

pub struct WadFile {
    wad_file_path: PathBuf,
    wad_data: Vec<u8>,
    wad_dirs: Vec<WadDirectory>,
    wad_len: usize,
}

impl fmt::Debug for WadFile {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "\nWadLoader {{\n  wad_file_path: {:?},\n  wad_data: [..],\n  wad_dirs: {:?},\n  wad_len: {},\n}}",
            self.wad_file_path, self.wad_dirs, self.wad_len
        )
    }
}

impl WadFile {
    pub fn new<A>(file_path: A) -> WadFile
    where
        A: Into<PathBuf>,
    {
        WadFile {
            wad_file_path: file_path.into(),
            wad_data: Vec::new(),
            wad_dirs: Vec::new(),
            wad_len: 0,
        }
    }

    pub fn load(&mut self) {
        let mut file = File::open(&self.wad_file_path)
            .expect(&format!("Could not open {:?}", &self.wad_file_path));
        let file_len = file.metadata().unwrap().len();
        self.wad_data.reserve_exact(file_len as usize);
        self.wad_len = file
            .read_to_end(&mut self.wad_data)
            .expect(&format!("Could not read {:?}", &self.wad_file_path));
        if self.wad_len != file_len as usize {
            panic!("Did not read complete WAD")
        }
    }

    pub fn read_2_bytes(&self, offset: usize) -> u16 {
        (self.wad_data[offset + 1] as u16) << 8 | (self.wad_data[offset] as u16)
    }

    pub fn read_4_bytes(&self, offset: usize) -> u32 {
        (self.wad_data[offset + 3] as u32) << 24
            | (self.wad_data[offset + 2] as u32) << 16
            | (self.wad_data[offset + 1] as u32) << 8
            | (self.wad_data[offset] as u32)
    }

    pub fn read_header(&self, offset: usize) -> WadHeader {
        let mut t = [0; 4];
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

    pub fn read_dir_data(&self, offset: usize) -> WadDirectory {
        let mut n = [0; 8]; // length is 8 slots total

        // exclusive of 8
        for i in 0..8 {
            n[i] = self.wad_data[offset + 8 + i]
        }

        WadDirectory {
            lump_offset: self.read_4_bytes(offset),
            lump_size: self.read_4_bytes(offset + 4),
            lump_name: n,
        }
    }

    pub fn read_directories(&mut self) {
        let header = self.read_header(0);
        dbg!("{}", &header);
        self.wad_dirs.reserve_exact(header.dir_count as usize);

        for i in 0..(header.dir_count) {
            let dir = self.read_dir_data((header.dir_offset + i * 16) as usize);
            self.wad_dirs.push(dir);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::WadFile;
    #[test]
    fn load_wad() {
        let mut wad = WadFile::new("../doom.wad");
        wad.load();
        dbg!(&wad);
        assert!(wad.wad_len > 0);
    }

    #[test]
    fn read_two_bytes() {
        let mut wad = WadFile::new("../doom.wad");
        wad.load();
        let x1 = wad.read_2_bytes(0);
        dbg!(&x1);
        let x2 = wad.read_2_bytes(2);
        dbg!(&x2);
    }

    #[test]
    fn read_four_bytes() {
        let mut wad = WadFile::new("../doom.wad");
        wad.load();
        let x = wad.read_4_bytes(0);
        dbg!(&x);

        let y = (wad.read_2_bytes(2) as u32) << 16 | (wad.read_2_bytes(0) as u32);
        dbg!(&y);

        assert_eq!(x, y);
    }

    #[test]
    fn read_header() {
        let mut wad = WadFile::new("../doom.wad");
        wad.load();

        let header = wad.read_header(0);
        dbg!(&header);
    }

    #[test]
    fn read_single_dir() {
        let mut wad = WadFile::new("../doom.wad");
        wad.load();

        let header = wad.read_header(0);
        let dir = wad.read_dir_data((header.dir_offset) as usize);
        dbg!(&dir);
    }

    #[test]
    fn read_all_dirs() {
        let mut wad = WadFile::new("../doom.wad");
        wad.load();
        wad.read_directories();

        for i in 7..18 {
            dbg!(&wad.wad_dirs[i]);
        }

        let header = wad.read_header(0);
        assert_eq!(wad.wad_dirs.len(), header.dir_count as usize);
    }
}
