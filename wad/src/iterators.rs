use crate::lumps::*;
use crate::{Lumps, WadData};
use std::marker::PhantomData;
use std::mem::size_of;

pub struct LumpIter<T, F: Fn(usize) -> T> {
    item_size:   usize,
    item_count:  usize,
    lump_offset: usize,
    current:     usize,
    transformer: F,
    _phantom:    PhantomData<T>,
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

impl WadData {
    pub fn thing_iter(
        &self,
        map_name: &str,
    ) -> LumpIter<WadThing, impl Fn(usize) -> WadThing + '_> {
        let info = self.find_lump_for_map_or_panic(map_name, Lumps::Things);
        let item_size = size_of::<WadThing>();
        let file = &self.file_data[info.file_handle];

        LumpIter {
            item_size,
            item_count: info.lump_size / item_size,
            lump_offset: info.lump_offset,
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
        let info = self.find_lump_for_map_or_panic(map_name, Lumps::Vertexes);
        let item_size = size_of::<WadVertex>();
        let file = &self.file_data[info.file_handle];

        LumpIter {
            item_size,
            item_count: info.lump_size / item_size,
            lump_offset: info.lump_offset,
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
        let info = self.find_lump_for_map_or_panic(map_name, Lumps::Sectors);
        let item_size = size_of::<WadSector>();
        let file = &self.file_data[info.file_handle];

        LumpIter {
            item_size,
            item_count: info.lump_size / item_size,
            lump_offset: info.lump_offset,
            current: 0,
            transformer: move |offset| {
                WadSector::new(
                    self.read_2_bytes(offset, file) as i16,
                    self.read_2_bytes(offset + 2, file) as i16,
                    &self.file_data[info.file_handle][offset + 4..offset + 12],
                    &self.file_data[info.file_handle][offset + 12..offset + 20],
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
        let info = self.find_lump_for_map_or_panic(map_name, Lumps::SideDefs);
        let item_size = size_of::<WadSideDef>();
        let file = &self.file_data[info.file_handle];

        LumpIter {
            item_size,
            item_count: info.lump_size / item_size,
            lump_offset: info.lump_offset,
            current: 0,
            transformer: move |offset| {
                WadSideDef::new(
                    self.read_2_bytes(offset, file) as i16,
                    self.read_2_bytes(offset + 2, file) as i16,
                    &self.file_data[info.file_handle][offset + 4..offset + 12],
                    &self.file_data[info.file_handle][offset + 12..offset + 20],
                    &self.file_data[info.file_handle][offset + 20..offset + 28],
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
        let info = self.find_lump_for_map_or_panic(map_name, Lumps::LineDefs);
        let item_size = size_of::<WadLineDef>();
        let file = &self.file_data[info.file_handle];

        LumpIter {
            item_size,
            item_count: info.lump_size / item_size,
            lump_offset: info.lump_offset,
            current: 0,
            transformer: move |offset| {
                let back_sidedef = {
                    let index = self.read_2_bytes(offset + 12, file);
                    if index < i16::MAX {
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
        let info = self.find_lump_for_map_or_panic(map_name, Lumps::Segs);
        let item_size = size_of::<WadSegment>();
        let file = &self.file_data[info.file_handle];

        LumpIter {
            item_size,
            item_count: info.lump_size / item_size,
            lump_offset: info.lump_offset,
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
        let info = self.find_lump_for_map_or_panic(map_name, Lumps::SSectors);
        let item_size = size_of::<WadSubSector>();
        let file = &self.file_data[info.file_handle];

        LumpIter {
            item_size,
            item_count: info.lump_size / item_size,
            lump_offset: info.lump_offset,
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

    pub fn node_iter(
        &self,
        map_name: &str,
    ) -> LumpIter<WadNode, impl Fn(usize) -> WadNode + '_> {
        let info = self.find_lump_for_map_or_panic(map_name, Lumps::Nodes);
        let item_size = size_of::<WadNode>();
        let file = &self.file_data[info.file_handle];

        LumpIter {
            item_size,
            item_count: info.lump_size / item_size,
            lump_offset: info.lump_offset,
            current: 0,
            transformer: move |offset| {
                WadNode::new(
                    self.read_2_bytes(offset, file),
                    self.read_2_bytes(offset + 2, file),
                    self.read_2_bytes(offset + 4, file),
                    self.read_2_bytes(offset + 6, file),
                    [
                        [
                            self.read_2_bytes(offset + 12, file), // top
                            self.read_2_bytes(offset + 8, file),  // left
                            self.read_2_bytes(offset + 14, file), // bottom
                            self.read_2_bytes(offset + 10, file), // right
                        ],
                        [
                            self.read_2_bytes(offset + 20, file),
                            self.read_2_bytes(offset + 16, file),
                            self.read_2_bytes(offset + 22, file),
                            self.read_2_bytes(offset + 18, file),
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
    use crate::lumps::WadThing;
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

        let collection: Vec<WadThing> = wad.thing_iter("E1M1").collect();
        assert_eq!(collection.len(), 138);
    }
}
