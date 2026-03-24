use log::debug;

use crate::extended::NodeLumpType;
use crate::types::*;
use crate::{Lump, MapLump, WadData};
use std::marker::PhantomData;

/// Iterator over fixed-size records in a byte slice, transforming each via `T:
/// From<&[u8]>`.
pub struct RecordIter<'a, T: WadRecord> {
    data: &'a [u8],
    pos: usize,
    _t: PhantomData<T>,
}

impl<'a, T: WadRecord> RecordIter<'a, T> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            _t: PhantomData,
        }
    }
}

impl<T: WadRecord> Iterator for RecordIter<'_, T> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        if self.pos + T::SIZE > self.data.len() {
            return None;
        }
        let item = T::from(&self.data[self.pos..self.pos + T::SIZE]);
        self.pos += T::SIZE;
        Some(item)
    }
}

/// An iterator to iter over all items between start and end (exclusive),
/// skipping zero-sized lumps. This is good for iterating over flats for
/// example, as each `LumpInfo` also contains the name of the flat and is in
/// order.
///
/// When used via the iterator methods such as for flats, with pwads added, then
/// the iteration returns each flat in each *chunk* if there are multiple, in
/// reverse order.
///
/// Wad loading order iwad->pwad1->pwad2 results in:
/// - iter over flats in pwad2
/// - iter over flats in pwad1
/// - iter over flats in iwad
///
/// It is the responsibility of the user to dedup the iteration results for
/// Doom.
pub struct LumpIter<'a, T, F: Fn(&Lump) -> T> {
    /// Index to all the starting points. The first is the index to the
    /// starting point in the last pwad, then the next wad etc.
    start_lumps: Vec<usize>,
    /// Index to all the end points, paired with `start_lumps`. The first is the
    /// index to the starting point in the last pwad, then the next wad etc.
    end_lumps: Vec<usize>,
    lumps: &'a [Lump],
    /// Index to the current start+end
    current_start: usize,
    transformer: F,
}

impl<'a, T, F> Iterator for LumpIter<'a, T, F>
where
    F: Fn(&Lump) -> T,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.current_start >= self.start_lumps.len() {
                return None;
            }

            let start = &mut self.start_lumps[self.current_start];
            let end = self.end_lumps[self.current_start];

            // Exhausted current chunk — advance to next
            if *start >= end {
                self.current_start += 1;
                continue;
            }

            if *start >= self.lumps.len() {
                return None;
            }

            // Skip empty lumps (markers between groups)
            if self.lumps[*start].data.is_empty() {
                *start += 1;
                continue;
            }

            let item = (self.transformer)(&self.lumps[*start]);
            *start += 1;
            return Some(item);
        }
    }
}

/// The `OffsetIter` is used for lumps that contain groups of data, such as the
/// the texture definitions, or LineDefs.
pub struct OffsetIter<T, F: Fn(usize) -> T> {
    item_size: usize,
    item_count: usize,
    lump_offset: usize,
    current: usize,
    transformer: F,
    _phantom: PhantomData<T>,
}

impl<T, F> Iterator for OffsetIter<T, F>
where
    F: Fn(usize) -> T,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current < self.item_count {
            let offset = self.lump_offset + self.current * self.item_size;
            let item = (self.transformer)(offset);
            self.current += 1;
            return Some(item);
        }
        None
    }
}

impl WadData {
    /// Iterate fixed-size records of type `T` from a map's lump.
    pub fn map_iter<T: WadRecord>(&self, map_name: &str, lump: MapLump) -> RecordIter<'_, T> {
        let info = self.find_lump_for_map_or_panic(map_name, lump);
        RecordIter::new(&info.data)
    }

    /// Iterate fixed-size records of type `T` from a named lump.
    pub fn lump_iter<T: WadRecord>(&self, name: &str) -> RecordIter<'_, T> {
        let info = self.find_lump_or_panic(name);
        RecordIter::new(&info.data)
    }

    pub fn patches_iter(&'_ self) -> LumpIter<'_, WadPatch, impl Fn(&Lump) -> WadPatch + '_> {
        let mut starts = Vec::new();
        let mut ends = Vec::new();
        for (i, info) in self.lumps.iter().enumerate() {
            if info.name == "P1_START"
                || info.name.contains("P2_START")
                || info.name.contains("P3_START")
                || info.name.contains("PP_START")
            {
                starts.push(i);
            }

            if info.name == "P1_END"
                || info.name.contains("P2_END")
                || info.name.contains("P3_END")
                || info.name.contains("PP_END")
            {
                ends.push(i);
            }
        }
        if starts.is_empty() {
            panic!("Could not find patches");
        }

        LumpIter {
            end_lumps: ends,
            lumps: &self.lumps,
            start_lumps: starts,
            current_start: 0,
            transformer: WadPatch::from_lump,
        }
    }

    pub fn flats_iter(&'_ self) -> LumpIter<'_, WadFlat, impl Fn(&Lump) -> WadFlat> {
        let mut starts = Vec::new();
        let mut ends = Vec::new();
        for (i, info) in self.lumps.iter().enumerate().rev() {
            if info.name == "F_START" {
                starts.push(i);
            } else if info.name.contains("F_START") {
                starts.push(i);
                debug!("Did not find F_START but found {}", info.name);
            }

            if info.name == "F_END" {
                ends.push(i);
            } else if info.name.contains("F_END") {
                ends.push(i);
                debug!("Did not find F_END but found {}", info.name);
            }
        }
        if starts.is_empty() {
            panic!("Could not find flats");
        }

        LumpIter {
            end_lumps: ends,
            lumps: &self.lumps,
            start_lumps: starts,
            current_start: 0,
            transformer: move |lump| {
                let name = lump.name.clone();
                let mut data = vec![0; lump.data.len()];
                data.copy_from_slice(&lump.data);
                WadFlat {
                    name,
                    data,
                }
            },
        }
    }

    pub fn sprites_iter(&'_ self) -> LumpIter<'_, WadPatch, impl Fn(&Lump) -> WadPatch> {
        let mut starts = Vec::new();
        for (i, info) in self.lumps.iter().enumerate().rev() {
            if info.name == "S_START" {
                starts.push(i);
            } else if info.name.contains("S_START") {
                starts.push(i);
                debug!("Did not find S_START but found {}", info.name);
            }
        }
        let mut ends = Vec::new();
        for (i, info) in self.lumps.iter().enumerate().rev() {
            if info.name == "S_END" {
                ends.push(i);
            } else if info.name.contains("S_END") {
                ends.push(i);
                debug!("Did not find S_END but found {}", info.name);
            }
        }
        if starts.is_empty() {
            panic!("Could not find flats");
        }

        LumpIter {
            start_lumps: starts,
            end_lumps: ends,
            lumps: &self.lumps,
            current_start: 0,
            transformer: WadPatch::from_lump,
        }
    }

    pub fn colourmap_iter(&self) -> OffsetIter<u8, impl Fn(usize) -> u8 + '_> {
        let info = self.find_lump_or_panic("COLORMAP");
        let item_size = 1;

        OffsetIter {
            item_size,
            item_count: info.data.len(),
            lump_offset: 0,
            current: 0,
            transformer: move |offset| info.data[offset],
            _phantom: Default::default(),
        }
    }

    pub fn pnames_iter(&self) -> OffsetIter<String, impl Fn(usize) -> String + '_> {
        let info = self.find_lump_or_panic("PNAMES");
        let item_size = 8;

        OffsetIter {
            item_size,
            item_count: info.read_i32(0) as usize,
            lump_offset: 4,
            current: 0,
            transformer: move |offset| {
                let mut n = [0u8; 8]; // length is 8 slots total
                for (i, slot) in n.iter_mut().enumerate() {
                    *slot = info.data[offset + i];
                }
                std::str::from_utf8(&n)
                    .expect("Invalid lump name")
                    .trim_end_matches('\u{0}')
                    .trim_end()
                    .to_ascii_uppercase()
            },
            _phantom: Default::default(),
        }
    }

    /// Producer for the base texture data. This returns `WadTexture` which
    /// includes data on how the patches are put together to form a texture.
    pub fn texture_iter(
        &self,
        name: &str,
    ) -> OffsetIter<WadTexture, impl Fn(usize) -> WadTexture + '_> {
        let info = self.find_lump_or_panic(name);
        let item_size = 4;

        OffsetIter {
            item_size,
            // texture count
            item_count: info.read_i32(0) as usize,
            lump_offset: 4,
            current: 0,
            transformer: move |ofs| {
                let mut ofs = i32::from_le_bytes([
                    info.data[ofs],
                    info.data[ofs + 1],
                    info.data[ofs + 2],
                    info.data[ofs + 3],
                ]) as usize;

                let mut n = [0u8; 8]; // length is 8 slots total
                for (i, slot) in n.iter_mut().enumerate() {
                    *slot = info.data[ofs + i];
                }
                let name = std::str::from_utf8(&n)
                    .expect("Invalid lump name")
                    .to_ascii_uppercase()
                    .trim_end_matches('\u{0}')
                    .to_owned();

                let width = info.read_u16(ofs + 12) as u32;
                let height = info.read_u16(ofs + 14) as u32;
                let patch_count = info.read_u16(ofs + 20) as u32;

                let mut patches = Vec::new();
                for _ in 0..patch_count {
                    patches.push(WadTexPatch {
                        origin_x: info.read_i16(ofs + 22) as i32,
                        origin_y: info.read_i16(ofs + 24) as i32,
                        patch_index: info.read_i16(ofs + 26) as usize,
                    });
                    ofs += 10;
                }

                WadTexture {
                    name,
                    width,
                    height,
                    patches,
                }
            },
            _phantom: Default::default(),
        }
    }

    pub fn node_lump_type(&self, map_name: &str) -> NodeLumpType {
        let info = self.find_lump_for_map_or_panic(map_name, MapLump::Nodes);
        let bytes = [
            info.read_i16(0) as u8,
            info.read_i16(1) as u8,
            info.read_i16(2) as u8,
            info.read_i16(3) as u8,
        ];
        NodeLumpType::from_bytes(&bytes)
    }
}
