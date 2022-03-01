use crate::{lumps::*, MapLump, WadData};
use std::{marker::PhantomData, thread::current};

pub struct LumpIter<T, F: Fn(usize) -> T> {
    item_size: usize,
    item_count: usize,
    lump_offset: usize,
    current: usize,
    transformer: F,
    _phantom: PhantomData<T>,
}

impl<T, F> Iterator for LumpIter<T, F>
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

pub struct PatchIter<'a> {
    names: Vec<String>,
    current: usize,
    wad: &'a WadData,
    _phantom: PhantomData<WadPatch>,
}

impl<'a> Iterator for PatchIter<'a> {
    type Item = WadPatch;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current < self.names.len() {
            let info = self.wad.find_lump_or_panic(&self.names[self.current]);
            let patch = WadPatch::from_lump(info, self.wad);

            // cycle through and check until we find one
            self.current += 1;
            for n in self.current..self.names.len() {
                if !self.wad.lump_exists(&self.names[n]) {
                    self.current += 1;
                } else {
                    break;
                }
            }

            return Some(patch);
        }
        None
    }
}

impl WadData {
    pub fn playpal_iter(&self) -> LumpIter<WadPalette, impl Fn(usize) -> WadPalette + '_> {
        let info = self.find_lump_or_panic("PLAYPAL");
        let item_size = 3 * 256;
        let file = &self.file_data[info.handle];

        LumpIter {
            item_size,
            item_count: info.size / item_size,
            lump_offset: info.offset,
            current: 0,
            transformer: move |offset| {
                let mut palette = WadPalette::new();
                for i in 0..256 {
                    palette.0[i] = WadColour::new(
                        self.read_byte(offset + i * 3, file),
                        self.read_byte(offset + i * 3 + 1, file),
                        self.read_byte(offset + i * 3 + 2, file),
                    );
                }
                palette
            },
            _phantom: Default::default(),
        }
    }

    pub fn colourmap_iter(&self) -> LumpIter<u8, impl Fn(usize) -> u8 + '_> {
        let info = self.find_lump_or_panic("COLORMAP");
        let item_size = 1;
        let file = &self.file_data[info.handle];

        LumpIter {
            item_size,
            item_count: info.size,
            lump_offset: info.offset,
            current: 0,
            transformer: move |offset| self.read_byte(offset, file),
            _phantom: Default::default(),
        }
    }

    pub fn pnames_iter(&self) -> LumpIter<String, impl Fn(usize) -> String + '_> {
        let info = self.find_lump_or_panic("PNAMES");
        let item_size = 8;
        let file = &self.file_data[info.handle];

        LumpIter {
            item_size,
            item_count: self.read_4_bytes(info.offset, file) as usize,
            lump_offset: info.offset + 4,
            current: 0,
            transformer: move |offset| {
                let mut n = [0u8; 8]; // length is 8 slots total
                for (i, slot) in n.iter_mut().enumerate() {
                    *slot = file[offset + i];
                }
                std::str::from_utf8(&n)
                    .expect("Invalid lump name")
                    .trim_end_matches('\u{0}')
                    .to_owned()
            },
            _phantom: Default::default(),
        }
    }

    /// Iterate over patches in order determined by PNAME lump
    pub fn patches_iter(&self) -> PatchIter {
        PatchIter {
            names: self.pnames_iter().collect(),
            current: 0,
            wad: self,
            _phantom: Default::default(),
        }
    }

    /// Producer for the base texture data. This returns `WadTexture` which includes data
    /// on how the patches are put together to form a texture.
    pub fn texture_iter(
        &self,
        name: &str,
    ) -> LumpIter<WadTexture, impl Fn(usize) -> WadTexture + '_> {
        let info = self.find_lump_or_panic(name);
        let item_size = 4;
        let file = &self.file_data[info.handle];

        LumpIter {
            item_size,
            // texture count
            item_count: self.read_4_bytes(info.offset, file) as usize,
            lump_offset: info.offset + 4,
            current: 0,
            transformer: move |offset| {
                let mut offset = info.offset + self.read_4_bytes(offset, file) as usize;

                let mut n = [0u8; 8]; // length is 8 slots total
                for (i, slot) in n.iter_mut().enumerate() {
                    *slot = file[offset + i];
                }
                let name = std::str::from_utf8(&n)
                    .expect("Invalid lump name")
                    .to_ascii_uppercase()
                    .trim_end_matches('\u{0}')
                    .to_owned();

                let width = self.read_2_bytes(offset + 12, file) as u32;
                let height = self.read_2_bytes(offset + 14, file) as u32;
                let patch_count = self.read_2_bytes(offset + 20, file) as u32;

                let mut patches = Vec::new();
                for _ in 0..patch_count {
                    patches.push(WadTexPatch {
                        origin_x: self.read_2_bytes(offset + 22, file) as i32,
                        origin_y: self.read_2_bytes(offset + 24, file) as i32,
                        patch_index: self.read_2_bytes(offset + 26, file) as usize,
                    });
                    offset += 10;
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
    ) -> LumpIter<WadThing, impl Fn(usize) -> WadThing + '_> {
        let info = self.find_lump_for_map_or_panic(map_name, MapLump::Things);
        let item_size = 10;
        let file = &self.file_data[info.handle];

        LumpIter {
            item_size,
            item_count: info.size / item_size,
            lump_offset: info.offset,
            current: 0,
            transformer: move |offset| {
                WadThing::new(
                    self.read_2_bytes(offset, file),
                    self.read_2_bytes(offset + 2, file),
                    self.read_2_bytes(offset + 4, file),
                    self.read_2_bytes(offset + 6, file),
                    self.read_2_bytes(offset + 8, file),
                )
            },
            _phantom: Default::default(),
        }
    }

    pub fn vertex_iter(
        &self,
        map_name: &str,
    ) -> LumpIter<WadVertex, impl Fn(usize) -> WadVertex + '_> {
        let info = self.find_lump_for_map_or_panic(map_name, MapLump::Vertexes);
        let item_size = 4;
        let file = &self.file_data[info.handle];

        LumpIter {
            item_size,
            item_count: info.size / item_size,
            lump_offset: info.offset,
            current: 0,
            transformer: move |offset| {
                WadVertex::new(
                    self.read_2_bytes(offset, file),
                    self.read_2_bytes(offset + 2, file),
                )
            },
            _phantom: Default::default(),
        }
    }

    pub fn sector_iter(
        &self,
        map_name: &str,
    ) -> LumpIter<WadSector, impl Fn(usize) -> WadSector + '_> {
        let info = self.find_lump_for_map_or_panic(map_name, MapLump::Sectors);
        let item_size = 26;
        let file = &self.file_data[info.handle];

        LumpIter {
            item_size,
            item_count: info.size / item_size,
            lump_offset: info.offset,
            current: 0,
            transformer: move |offset| {
                WadSector::new(
                    self.read_2_bytes(offset, file) as i16,
                    self.read_2_bytes(offset + 2, file) as i16,
                    &self.file_data[info.handle][offset + 4..offset + 12],
                    &self.file_data[info.handle][offset + 12..offset + 20],
                    self.read_2_bytes(offset + 20, file),
                    self.read_2_bytes(offset + 22, file),
                    self.read_2_bytes(offset + 24, file),
                )
            },
            _phantom: Default::default(),
        }
    }

    pub fn sidedef_iter(
        &self,
        map_name: &str,
    ) -> LumpIter<WadSideDef, impl Fn(usize) -> WadSideDef + '_> {
        let info = self.find_lump_for_map_or_panic(map_name, MapLump::SideDefs);
        let item_size = 30;
        let file = &self.file_data[info.handle];

        LumpIter {
            item_size,
            item_count: info.size / item_size,
            lump_offset: info.offset,
            current: 0,
            transformer: move |offset| {
                WadSideDef::new(
                    self.read_2_bytes(offset, file) as i16,
                    self.read_2_bytes(offset + 2, file) as i16,
                    &self.file_data[info.handle][offset + 4..offset + 12],
                    &self.file_data[info.handle][offset + 12..offset + 20],
                    &self.file_data[info.handle][offset + 20..offset + 28],
                    self.read_2_bytes(offset + 28, file),
                )
            },
            _phantom: Default::default(),
        }
    }

    pub fn linedef_iter(
        &self,
        map_name: &str,
    ) -> LumpIter<WadLineDef, impl Fn(usize) -> WadLineDef + '_> {
        let info = self.find_lump_for_map_or_panic(map_name, MapLump::LineDefs);
        let item_size = 14;
        let file = &self.file_data[info.handle];

        LumpIter {
            item_size,
            item_count: info.size / item_size,
            lump_offset: info.offset,
            current: 0,
            transformer: move |offset| {
                let back_sidedef = {
                    let index = self.read_2_bytes(offset + 12, file);
                    if (index as u16) < u16::MAX {
                        Some(index)
                    } else {
                        None
                    }
                };

                WadLineDef::new(
                    self.read_2_bytes(offset, file),
                    self.read_2_bytes(offset + 2, file),
                    self.read_2_bytes(offset + 4, file),
                    self.read_2_bytes(offset + 6, file),
                    self.read_2_bytes(offset + 8, file),
                    self.read_2_bytes(offset + 10, file),
                    back_sidedef,
                )
            },
            _phantom: Default::default(),
        }
    }

    pub fn segment_iter(
        &self,
        map_name: &str,
    ) -> LumpIter<WadSegment, impl Fn(usize) -> WadSegment + '_> {
        let info = self.find_lump_for_map_or_panic(map_name, MapLump::Segs);
        let item_size = 12;
        let file = &self.file_data[info.handle];

        LumpIter {
            item_size,
            item_count: info.size / item_size,
            lump_offset: info.offset,
            current: 0,
            transformer: move |offset| {
                WadSegment::new(
                    self.read_2_bytes(offset, file),
                    self.read_2_bytes(offset + 2, file),
                    self.read_2_bytes(offset + 4, file),
                    self.read_2_bytes(offset + 6, file),
                    self.read_2_bytes(offset + 8, file), // 0 front or 1 back
                    self.read_2_bytes(offset + 10, file),
                )
            },
            _phantom: Default::default(),
        }
    }

    pub fn subsector_iter(
        &self,
        map_name: &str,
    ) -> LumpIter<WadSubSector, impl Fn(usize) -> WadSubSector + '_> {
        let info = self.find_lump_for_map_or_panic(map_name, MapLump::SSectors);
        let item_size = 4;
        let file = &self.file_data[info.handle];

        LumpIter {
            item_size,
            item_count: info.size / item_size,
            lump_offset: info.offset,
            current: 0,
            transformer: move |offset| {
                WadSubSector::new(
                    self.read_2_bytes(offset, file),
                    self.read_2_bytes(offset + 2, file),
                )
            },
            _phantom: Default::default(),
        }
    }

    pub fn node_iter(&self, map_name: &str) -> LumpIter<WadNode, impl Fn(usize) -> WadNode + '_> {
        let info = self.find_lump_for_map_or_panic(map_name, MapLump::Nodes);
        let item_size = 28;
        let file = &self.file_data[info.handle];

        LumpIter {
            item_size,
            item_count: info.size / item_size,
            lump_offset: info.offset,
            current: 0,
            transformer: move |offset| {
                WadNode::new(
                    self.read_2_bytes(offset, file),
                    self.read_2_bytes(offset + 2, file),
                    self.read_2_bytes(offset + 4, file),
                    self.read_2_bytes(offset + 6, file),
                    [
                        [
                            self.read_2_bytes(offset + 8, file),  // top
                            self.read_2_bytes(offset + 10, file), // bottom
                            self.read_2_bytes(offset + 12, file), // left
                            self.read_2_bytes(offset + 14, file), // right
                        ],
                        [
                            self.read_2_bytes(offset + 16, file),
                            self.read_2_bytes(offset + 18, file),
                            self.read_2_bytes(offset + 20, file),
                            self.read_2_bytes(offset + 22, file),
                        ],
                    ],
                    self.read_2_bytes(offset + 24, file) as u16,
                    self.read_2_bytes(offset + 26, file) as u16,
                )
            },
            _phantom: Default::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{lumps::*, wad::WadData};

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
    fn palette_iter() {
        let wad = WadData::new("../doom1.wad".into());
        let count = wad.playpal_iter().count();
        assert_eq!(count, 14);

        let palettes: Vec<WadPalette> = wad.playpal_iter().collect();

        assert_eq!(palettes[0].0[0].r, 0);
        assert_eq!(palettes[0].0[0].g, 0);
        assert_eq!(palettes[0].0[0].b, 0);

        assert_eq!(palettes[0].0[1].r, 31);
        assert_eq!(palettes[0].0[1].g, 23);
        assert_eq!(palettes[0].0[1].b, 11);

        assert_eq!(palettes[0].0[119].r, 67);
        assert_eq!(palettes[0].0[119].g, 147);
        assert_eq!(palettes[0].0[119].b, 55);

        assert_eq!(palettes[4].0[119].r, 150);
        assert_eq!(palettes[4].0[119].g, 82);
        assert_eq!(palettes[4].0[119].b, 31);
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
        assert_eq!(wad.patches_iter().count(), 163);
    }

    #[test]
    fn patches_doom_iter() {
        let wad = WadData::new("../doom.wad".into());
        assert_eq!(wad.patches_iter().count(), 351);
    }

    #[test]
    fn patches_doom2_iter() {
        // W94_1 is missing in DOOM2?
        let wad = WadData::new("../doom2.wad".into());
        assert_eq!(wad.patches_iter().count(), 469);
    }

    #[test]
    fn W94_1() {
        // W94_1 has incorrect capitalisation as "w94_1"
        let wad = WadData::new("../doom2.wad".into());
        let lump = wad.find_lump_or_panic("W94_1");
        assert_eq!(lump.name, "W94_1");

        let lump = wad.find_lump_or_panic("w94_1");
        assert_eq!(lump.name, "W94_1");
    }

    #[test]
    fn pnames_doom2_iter() {
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
        let patches: Vec<WadPatch> = wad.patches_iter().collect();

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
}
