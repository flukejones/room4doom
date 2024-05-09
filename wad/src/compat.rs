use log::warn;

use crate::{
    types::{WadNode, WadSegment, WadSubSector, WadVertex},
    Lump, MapLump, WadData,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtendedNodeType {
    XNOD,
    XGLN,
    XGL2,
    ZNOD,
    ZGLN,
    ZGL2,
    OGDoom,
}

impl ExtendedNodeType {
    pub fn is_uncompressed(&self) -> bool {
        matches!(
            self,
            ExtendedNodeType::XGL2 | ExtendedNodeType::XGLN | ExtendedNodeType::XNOD
        )
    }

    pub fn is_gl(&self) -> bool {
        matches!(
            self,
            ExtendedNodeType::XGL2
                | ExtendedNodeType::XGLN
                | ExtendedNodeType::ZGL2
                | ExtendedNodeType::ZGLN
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeLumpType {
    /// Original Doom style NODES table, use the standard parser
    OGDoom,
    /// Extended NODES, typically this means the subsectors and segments tables
    /// are empty as all the data is contained here. You should then check if
    /// the table is compressed (with zlib) or uncompressed, and further check
    /// if GL or GL2 style
    Extended(ExtendedNodeType),
}

impl NodeLumpType {
    pub fn from_bytes(bytes: &[u8; 4]) -> Self {
        if bytes[0] == b'Z' {
            warn!("NODES is a compressed zdoom style");
            return if bytes[1..] == [b'N', b'O', b'D'] {
                Self::Extended(ExtendedNodeType::ZNOD)
            } else if bytes[1..] == [b'G', b'L', b'N'] {
                Self::Extended(ExtendedNodeType::ZGLN)
            } else if bytes[1..] == [b'G', b'L', b'2'] {
                Self::Extended(ExtendedNodeType::ZGL2)
            } else {
                panic!("Unknown Z<node> type")
            };
        } else if bytes[0] == b'X' {
            warn!("NODES is a compressed zdoom style");
            return if bytes[1..] == [b'N', b'O', b'D'] {
                Self::Extended(ExtendedNodeType::XNOD)
            } else if bytes[1..] == [b'G', b'L', b'N'] {
                Self::Extended(ExtendedNodeType::XGLN)
            } else if bytes[1..] == [b'G', b'L', b'2'] {
                Self::Extended(ExtendedNodeType::XGL2)
            } else {
                panic!("Unknown Z<node> type")
            };
        }
        Self::OGDoom
    }
}

/// The data in the WAD lump is structured as follows:
///
/// Note: a 16:16 fixed point number is stored in 4 bytes.
///
/// | Field Size   | Type    | Content                                                       |
/// |--------------|---------|---------------------------------------------------------------|
/// | 0x00-0x03    | str     | 4 bytes of UTF 8 making up the lump signature, such as `XNOD` |
/// | 0x04-0x07    | u32     | Number of vertices from the VERTEXES lump                     |
/// | 0x08-0x11    | u32     | The `N` additional vertices that follow from here             |
/// | 8-byte chunk | Vertex  | fixed,fixed Vertex: 16:16 fixed-point (x,y). Repeated `N` times from above |
/// | 4-bytes      | u32     | Subsector count                                               |
/// | 4-byte chunk | u32     | Subsector N: Seg count for this subsector                     |
/// | 4-bytes      | u32     | Segs count                                                    |
/// | 11-byte chunk| Segment | Seg N: New layout: `u32`:Vertex 1, `u32`Vertex 2, `u16`:Line, `u8`:Side |
/// | 4-byte chunk | u32     | Node count                                                    |
/// | 32-byte chunk| Node    | Node N: Same as vanilla except child ref are u32              |
#[derive(Debug, Clone)]
pub struct WadExtendedMap {
    pub node_type: ExtendedNodeType,
    pub num_org_vertices: usize,
    /// These should be appended to the `VETEXES` lump
    /// "When v >= OrgVerts, v- OrgVerts is the index of a vertex stored here", so just append to OrgVerts
    pub vertexes: Vec<WadVertex>,
    /// The start seg for a subsector inferred from being first in the count
    pub subsectors: Vec<WadSubSector>,
    /// The angle and other parts can be recalculated using the new data layout
    pub segments: Vec<WadSegment>,
    pub nodes: Vec<WadNode>,
}

impl WadExtendedMap {
    pub fn parse(wad_data: &WadData, map_name: &str) -> Option<Self> {
        let lump = wad_data.find_lump_for_map_or_panic(map_name, MapLump::Nodes);

        let bytes = [
            lump.read_i16(0) as u8,
            lump.read_i16(1) as u8,
            lump.read_i16(2) as u8,
            lump.read_i16(3) as u8,
        ];
        let node_type = NodeLumpType::from_bytes(&bytes);

        if let NodeLumpType::Extended(t) = node_type {
            if t.is_uncompressed() {
                return Some(Self::parse_uncompressed(&lump, t));
            } else {
                todo!("Compress zdoom nodes not supported yet")
            }
        }
        None
    }

    fn parse_uncompressed(lump: &Lump, etype: ExtendedNodeType) -> Self {
        let num_org_vertices = lump.read_u32(4) as usize;
        let num_new_vertices = lump.read_u32(8) as usize;

        let mut vertexes = Vec::with_capacity(num_new_vertices);
        let chunk_start = 12;
        let mut ofs = chunk_start;
        // The vertices are in fixed-point format and will require conversion later
        // Each vert is x,y, where x and y are 4 bytes each
        while ofs < chunk_start + num_new_vertices * 8 {
            vertexes.push(WadVertex::new(lump.read_i32(ofs), lump.read_i32(ofs + 4)));
            ofs += 8;
        }
        debug_assert_eq!(vertexes.len(), num_new_vertices);

        let num_subs = lump.read_u32(ofs) as usize;
        ofs += 4;
        let mut subsectors = Vec::with_capacity(num_subs);
        let chunk_start = ofs;
        let mut start_seg = 0;
        // subsectors are an index
        while ofs < chunk_start + num_subs * 4 {
            let seg_count = lump.read_u32(ofs);
            subsectors.push(WadSubSector {
                seg_count,
                start_seg,
            });
            start_seg += seg_count;
            ofs += 4;
        }
        debug_assert_eq!(subsectors.len(), num_subs);

        let num_segs = lump.read_u32(ofs) as usize;
        ofs += 4;
        let mut segments = Vec::with_capacity(num_segs);
        let chunk_start = ofs;

        while ofs < chunk_start + num_segs * 11 {
            segments.push(WadSegment::new(
                lump.read_i32(ofs),
                lump.read_i32(ofs + 4),
                i16::MAX, // angle, needs calculating when used
                lump.read_u16(ofs + 4 + 2),
                lump.data[ofs + 4 + 2 + 1] as i16,
                i16::MAX, // offset, used?
            ));
            ofs += 11;
        }
        debug_assert_eq!(segments.len(), num_segs);

        let num_nodes = lump.read_u32(ofs) as usize;
        ofs += 4;
        let mut nodes = Vec::with_capacity(num_nodes);
        let chunk_start = ofs;
        while ofs < chunk_start + num_nodes * 32 {
            nodes.push(WadNode::new(
                lump.read_i16(ofs),     // X
                lump.read_i16(ofs + 2), // Y
                lump.read_i16(ofs + 4), // DX
                lump.read_i16(ofs + 6), // DY
                [
                    [
                        lump.read_i16(ofs + 8),  // top
                        lump.read_i16(ofs + 10), // bottom
                        lump.read_i16(ofs + 12), // left
                        lump.read_i16(ofs + 14), /* right */
                    ],
                    [
                        lump.read_i16(ofs + 16), // top
                        lump.read_i16(ofs + 18), // bottom
                        lump.read_i16(ofs + 20), // left
                        lump.read_i16(ofs + 22), // right
                    ],
                ],
                lump.read_u16(ofs + 24) as u32, // right child index
                lump.read_u16(ofs + 26) as u32, // left child index
            ));
            ofs += 32
        }
        debug_assert_eq!(nodes.len(), num_nodes);

        Self {
            node_type: etype,
            num_org_vertices,
            vertexes,
            subsectors,
            segments,
            nodes,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::WadData;

    use super::WadExtendedMap;

    #[test]
    fn extended_nodes_none() {
        let wad = WadData::new("../doom1.wad".into());
        assert!(WadExtendedMap::parse(&wad, "E1M1").is_none());
    }

    #[test]
    #[ignore = "sunder.wad can't be included in git"]
    fn extended_nodes_sunder_m3() {
        let wad = WadData::new("../sunder.wad".into());
        let map = WadExtendedMap::parse(&wad, "MAP03").unwrap();

        assert_eq!(map.num_org_vertices, 5525);
        assert_eq!(map.vertexes.len(), 996);
        assert_eq!(map.subsectors.len(), 4338);
        assert_eq!(map.segments.len(), 14582);
        assert_eq!(map.nodes.len(), 4337);
    }
}
