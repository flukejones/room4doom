use log::debug;

use crate::compat::NodeLumpType;
use crate::types::*;
use crate::{Lump, MapLump, WadData};
use std::marker::PhantomData;

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
        let mut start = &mut self.start_lumps[self.current_start];
        let mut end = self.end_lumps[self.current_start];

        // Skip empty. Good for iterating over two groups of patches with markers
        // between
        loop {
            if *start < self.lumps.len() - 1 && self.lumps[*start].data.is_empty() {
                *start += 1;
                if *start >= end && self.current_start < self.end_lumps.len() - 1 {
                    self.current_start += 1;
                    start = &mut self.start_lumps[self.current_start];
                    end = self.end_lumps[self.current_start];
                    // *start += 1; // skip the next byte as it will be a marker
                }
            } else {
                break;
            }
        }
        if self.current_start >= self.end_lumps.len() - 1 && *start >= end {
            return None;
        }

        if *start == self.lumps.len() {
            return None;
        }

        let item = (self.transformer)(&self.lumps[*start]);
        *start += 1;
        Some(item)
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
    pub fn patches_iter(&self) -> LumpIter<WadPatch, impl Fn(&Lump) -> WadPatch + '_> {
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

    pub fn flats_iter(&self) -> LumpIter<WadFlat, impl Fn(&Lump) -> WadFlat + '_> {
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
                WadFlat { name, data }
            },
        }
    }

    pub fn sprites_iter(&self) -> LumpIter<WadPatch, impl Fn(&Lump) -> WadPatch + '_> {
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

    pub fn playpal_iter(&self) -> OffsetIter<WadPalette, impl Fn(usize) -> WadPalette + '_> {
        let info = self.find_lump_or_panic("PLAYPAL");
        let item_size = 3 * 256;

        OffsetIter {
            item_size,
            item_count: info.data.len() / item_size,
            lump_offset: 0,
            current: 0,
            transformer: move |offset| {
                let mut palette = WadPalette::new();
                for i in 0..256 {
                    palette.0[i] = WadColour::new(
                        info.data[offset + i * 3],
                        info.data[offset + i * 3 + 1],
                        info.data[offset + i * 3 + 2],
                    );
                }
                palette
            },
            _phantom: Default::default(),
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

    pub fn thing_iter(
        &self,
        map_name: &str,
    ) -> OffsetIter<WadThing, impl Fn(usize) -> WadThing + '_> {
        let info = self.find_lump_for_map_or_panic(map_name, MapLump::Things);
        let item_size = 10;

        OffsetIter {
            item_size,
            item_count: info.data.len() / item_size,
            lump_offset: 0,
            current: 0,
            transformer: move |ofs| {
                WadThing::new(
                    info.read_i16(ofs),
                    info.read_i16(ofs + 2),
                    info.read_i16(ofs + 4),
                    info.read_i16(ofs + 6),
                    info.read_i16(ofs + 8),
                )
            },
            _phantom: Default::default(),
        }
    }

    pub fn vertex_iter(
        &self,
        map_name: &str,
    ) -> OffsetIter<WadVertex, impl Fn(usize) -> WadVertex + '_> {
        let info = self.find_lump_for_map_or_panic(map_name, MapLump::Vertexes);
        let item_size = 4;

        OffsetIter {
            item_size,
            item_count: info.data.len() / item_size,
            lump_offset: 0,
            current: 0,
            transformer: move |ofs| {
                WadVertex::new(info.read_i16(ofs) as f32, info.read_i16(ofs + 2) as f32)
            },
            _phantom: Default::default(),
        }
    }

    pub fn sector_iter(
        &self,
        map_name: &str,
    ) -> OffsetIter<WadSector, impl Fn(usize) -> WadSector + '_> {
        let info = self.find_lump_for_map_or_panic(map_name, MapLump::Sectors);
        let item_size = 26;

        OffsetIter {
            item_size,
            item_count: info.data.len() / item_size,
            lump_offset: 0,
            current: 0,
            transformer: move |ofs| {
                WadSector::new(
                    info.read_i16(ofs),
                    info.read_i16(ofs + 2),
                    &info.data[ofs + 4..12 + ofs],
                    &info.data[ofs + 12..20 + ofs],
                    info.read_i16(ofs + 20),
                    info.read_i16(ofs + 22),
                    info.read_i16(ofs + 24),
                )
            },
            _phantom: Default::default(),
        }
    }

    pub fn sidedef_iter(
        &self,
        map_name: &str,
    ) -> OffsetIter<WadSideDef, impl Fn(usize) -> WadSideDef + '_> {
        let info = self.find_lump_for_map_or_panic(map_name, MapLump::SideDefs);
        let item_size = 30;

        OffsetIter {
            item_size,
            item_count: info.data.len() / item_size,
            lump_offset: 0,
            current: 0,
            transformer: move |ofs| {
                WadSideDef::new(
                    info.read_i16(ofs),
                    info.read_i16(ofs + 2),
                    &info.data[ofs + 4..12 + ofs],
                    &info.data[ofs + 12..20 + ofs],
                    &info.data[ofs + 20..28 + ofs],
                    info.read_i16(ofs + 28),
                )
            },
            _phantom: Default::default(),
        }
    }

    pub fn linedef_iter(
        &self,
        map_name: &str,
    ) -> OffsetIter<WadLineDef, impl Fn(usize) -> WadLineDef + '_> {
        let info = self.find_lump_for_map_or_panic(map_name, MapLump::LineDefs);
        let item_size = 14;

        OffsetIter {
            item_size,
            item_count: info.data.len() / item_size,
            lump_offset: 0,
            current: 0,
            transformer: move |ofs| {
                let back_sidedef = {
                    let index = info.read_u16(ofs + 12);
                    if index < u16::MAX {
                        Some(index)
                    } else {
                        None
                    }
                };

                WadLineDef::new(
                    info.read_u16(ofs),
                    info.read_u16(ofs + 2),
                    info.read_i16(ofs + 4),
                    info.read_i16(ofs + 6),
                    info.read_i16(ofs + 8),
                    info.read_u16(ofs + 10),
                    back_sidedef,
                    [info.read_u16(ofs + 10), info.read_u16(ofs + 12)],
                )
            },
            _phantom: Default::default(),
        }
    }

    pub fn segment_iter(
        &self,
        map_name: &str,
    ) -> OffsetIter<WadSegment, impl Fn(usize) -> WadSegment + '_> {
        let info = self.find_lump_for_map_or_panic(map_name, MapLump::Segs);
        let item_size = 12;

        OffsetIter {
            item_size,
            item_count: info.data.len() / item_size,
            lump_offset: 0,
            current: 0,
            transformer: move |ofs| {
                WadSegment::new(
                    info.read_i16(ofs) as u32,
                    info.read_i16(ofs + 2) as u32,
                    info.read_i16(ofs + 4),
                    info.read_i16(ofs + 6) as u16,
                    info.read_i16(ofs + 8),
                    info.read_i16(ofs + 10),
                )
            },
            _phantom: Default::default(),
        }
    }

    pub fn subsector_iter(
        &self,
        map_name: &str,
    ) -> OffsetIter<WadSubSector, impl Fn(usize) -> WadSubSector + '_> {
        let info = self.find_lump_for_map_or_panic(map_name, MapLump::SSectors);
        let item_size = 4;

        OffsetIter {
            item_size,
            item_count: info.data.len() / item_size,
            lump_offset: 0,
            current: 0,
            transformer: move |ofs| {
                WadSubSector::new(info.read_i16(ofs) as u32, info.read_i16(ofs + 2) as u32)
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

    pub fn node_iter(&self, map_name: &str) -> OffsetIter<WadNode, impl Fn(usize) -> WadNode + '_> {
        let info = self.find_lump_for_map_or_panic(map_name, MapLump::Nodes);
        let item_size = 28;

        let bytes = [
            info.read_i16(0) as u8,
            info.read_i16(1) as u8,
            info.read_i16(2) as u8,
            info.read_i16(3) as u8,
        ];
        let node_type = NodeLumpType::from_bytes(&bytes);
        if !matches!(node_type, NodeLumpType::OGDoom) {
            panic!(
                "Currently can't parse {:?} as WadNode, check with node_lump_type() and use compat",
                node_type
            );
        }

        OffsetIter {
            item_size,
            item_count: info.data.len() / item_size,
            lump_offset: 0,
            current: 0,
            transformer: move |ofs| {
                let mut right = info.read_u16(ofs + 24) as u32;
                if right == u16::MAX as u32 {
                    right = u32::MAX;
                }
                let mut left = info.read_u16(ofs + 26) as u32;
                if left == u16::MAX as u32 {
                    left = u32::MAX;
                }
                WadNode::new(
                    info.read_i16(ofs),     // X
                    info.read_i16(ofs + 2), // Y
                    info.read_i16(ofs + 4), // DX
                    info.read_i16(ofs + 6), // DY
                    [
                        [
                            info.read_i16(ofs + 8),  // top
                            info.read_i16(ofs + 10), // bottom
                            info.read_i16(ofs + 12), // left
                            info.read_i16(ofs + 14), /* right */
                        ],
                        [
                            info.read_i16(ofs + 16), // top
                            info.read_i16(ofs + 18), // bottom
                            info.read_i16(ofs + 20), // left
                            info.read_i16(ofs + 22), // right
                        ],
                    ],
                    right, // right child index
                    left,  // left child index
                )
            },
            _phantom: Default::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::types::*;
    use crate::wad::WadData;

    #[test]
    fn things_iter() {
        let wad = WadData::new("../doom1.wad".into());
        let mut iter = wad.thing_iter("E1M1");
        // All verified with SLADE

        let next = iter.next().unwrap();
        assert_eq!(next.x, 1056);
        assert_eq!(next.y, -3616);
        assert_eq!(next.angle, 90);
        assert_eq!(next.kind, 1);
        assert_eq!(next.flags, 7);

        let next = iter.next().unwrap();
        assert_eq!(next.x, 1008);
        assert_eq!(next.y, -3600);
        assert_eq!(next.angle, 90);
        assert_eq!(next.kind, 2);
        assert_eq!(next.flags, 7);

        assert_eq!(wad.thing_iter("E1M1").count(), 138);
    }

    #[test]
    fn node_iter() {
        let wad = WadData::new("../doom1.wad".into());
        let mut iter = wad.node_iter("E1M1");
        // All verified with SLADE

        let next = iter.next().unwrap();
        assert_eq!(next.x, 1552);
        assert_eq!(next.y, -2432);
        assert_eq!(next.dx, 112);
        assert_eq!(next.dy, 0);

        assert_eq!(wad.node_iter("E1M1").count(), 236);
    }

    #[test]
    fn palette_iter() {
        let wad = WadData::new("../doom1.wad".into());
        let count = wad.playpal_iter().count();
        assert_eq!(count, 14);

        let palettes: Vec<WadPalette> = wad.playpal_iter().collect();

        assert_eq!(palettes[0].0[0].0[0], 0);
        assert_eq!(palettes[0].0[0].0[1], 0);
        assert_eq!(palettes[0].0[0].0[2], 0);

        assert_eq!(palettes[0].0[1].0[0], 31);
        assert_eq!(palettes[0].0[1].0[1], 23);
        assert_eq!(palettes[0].0[1].0[2], 11);

        assert_eq!(palettes[0].0[119].0[0], 67);
        assert_eq!(palettes[0].0[119].0[1], 147);
        assert_eq!(palettes[0].0[119].0[2], 55);

        assert_eq!(palettes[4].0[119].0[0], 150);
        assert_eq!(palettes[4].0[119].0[1], 82);
        assert_eq!(palettes[4].0[119].0[2], 31);
    }

    #[test]
    fn pnames_iter() {
        let wad = WadData::new("../doom1.wad".into());
        let mut iter = wad.pnames_iter();
        // All verified with SLADE

        let next = iter.next().unwrap();
        assert_eq!(next, "WALL00_3");

        let next = iter.next().unwrap();
        assert_eq!(next, "W13_1");

        let next = iter.next().unwrap();
        assert_eq!(next, "DOOR2_1");

        assert_eq!(wad.pnames_iter().count(), 350);
    }

    #[test]
    fn texture_iter() {
        let wad = WadData::new("../doom1.wad".into());
        let mut iter = wad.texture_iter("TEXTURE1");
        // All verified with SLADE

        let next = iter.next().unwrap();
        assert_eq!(next.name, "AASTINKY");
        assert_eq!(next.width, 24);
        assert_eq!(next.height, 72);
        assert_eq!(next.patches.len(), 2);
        assert_eq!(next.patches[0].origin_x, 0);
        assert_eq!(next.patches[0].origin_y, 0);
        assert_eq!(next.patches[0].patch_index, 0);

        let next = iter.next().unwrap();
        assert_eq!(next.name, "BIGDOOR1");

        let next = iter.next().unwrap();
        assert_eq!(next.name, "BIGDOOR2");

        assert_eq!(wad.texture_iter("TEXTURE1").count(), 125);
    }

    #[test]
    fn patches_doom1_iter() {
        let wad = WadData::new("../doom1.wad".into());
        assert_eq!(wad.patches_iter().count(), 165);
    }

    #[test]
    #[ignore = "doom.wad is commercial"]
    fn patches_doom_iter_commercial() {
        let wad = WadData::new("../../doom.wad".into());
        assert_eq!(wad.patches_iter().count(), 351);
    }

    #[test]
    #[ignore = "doom2.wad is commercial"]
    fn patches_doom2_iter() {
        // W94_1 is missing in DOOM2?
        let wad = WadData::new("../doom2.wad".into());
        assert_eq!(wad.patches_iter().count(), 469);
    }

    #[test]
    #[ignore = "doom2.wad is commercial"]
    fn w94_1_commercial() {
        // W94_1 has incorrect capitalisation as "w94_1"
        let wad = WadData::new("../doom2.wad".into());
        let lump = wad.find_lump_or_panic("W94_1");
        assert_eq!(lump.name, "W94_1");

        let lump = wad.find_lump_or_panic("w94_1");
        assert_eq!(lump.name, "W94_1");
    }

    #[test]
    #[ignore = "doom2.wad is commercial"]
    fn pnames_doom2_iter_commercial() {
        let wad = WadData::new("../doom2.wad".into());
        let mut iter = wad.pnames_iter();
        // All verified with SLADE

        let next = iter.next().unwrap();
        assert_eq!(next, "BODIES");

        let next = iter.next().unwrap();
        assert_eq!(next, "RW22_1");

        let next = iter.next().unwrap();
        assert_eq!(next, "RW22_2");

        assert_eq!(wad.pnames_iter().count(), 469);
    }

    #[test]
    fn patches_doom1_tex19() {
        let wad = WadData::new("../doom1.wad".into());
        let iter: Vec<WadTexture> = wad.texture_iter("TEXTURE1").collect();
        let patch = &iter[19];

        assert_eq!(patch.width, 128);
        assert_eq!(patch.height, 128);

        assert_eq!(patch.patches[0].patch_index, 24);
        //assert_eq!(patches[patch.patches[0].patch_index].)

        assert_eq!(patch.patches[0].origin_x, 0);
        assert_eq!(patch.patches[0].origin_y, 0);

        assert_eq!(patch.patches[1].patch_index, 25);
    }

    #[test]
    fn colormap_iter() {
        let wad = WadData::new("../doom1.wad".into());
        let mut iter = wad.colourmap_iter();
        // All verified with SLADE

        let next = iter.next().unwrap();
        assert_eq!(next, 0);

        let next = iter.next().unwrap();
        assert_eq!(next, 1);

        let next = iter.next().unwrap();
        assert_eq!(next, 2);

        assert_eq!(wad.colourmap_iter().count(), 8704);
        assert_eq!(wad.colourmap_iter().count() / 256, 34);

        let colourmap: Vec<u8> = wad.colourmap_iter().collect();

        assert_eq!(colourmap[256], 0);
        assert_eq!(colourmap[8 * 256], 0);
        assert_eq!(colourmap[16 * 256], 0);

        assert_eq!(colourmap[256 + 32], 33);
        assert_eq!(colourmap[8 * 256 + 32], 36);
        assert_eq!(colourmap[16 * 256 + 32], 15);

        assert_eq!(colourmap[256 + 48], 49);
        assert_eq!(colourmap[8 * 256 + 48], 89);
        assert_eq!(colourmap[16 * 256 + 48], 98);

        assert_eq!(colourmap[256 + 64], 64);
        assert_eq!(colourmap[8 * 256 + 64], 69);
        assert_eq!(colourmap[16 * 256 + 64], 74);
    }

    #[test]
    fn flats_doom1() {
        let wad = WadData::new("../doom1.wad".into());
        let lump = wad.find_lump_or_panic("NUKAGE3");
        assert_eq!(lump.name, "NUKAGE3");
        assert_eq!(wad.flats_iter().count(), 54);
    }

    #[ignore = "doom.wad is commercial"]
    #[test]
    fn flats_doom_commercial() {
        let wad = WadData::new("../../doom.wad".into());
        let lump = wad.find_lump_or_panic("NUKAGE3");
        assert_eq!(lump.name, "NUKAGE3");
        assert_eq!(wad.flats_iter().count(), 107);
    }

    #[ignore = "doom2.wad is commercial"]
    #[test]
    fn flats_doom2_commercial() {
        let wad = WadData::new("../doom2.wad".into());
        let lump = wad.find_lump_or_panic("NUKAGE3");
        assert_eq!(lump.name, "NUKAGE3");
        assert_eq!(wad.flats_iter().count(), 147);
    }
}
