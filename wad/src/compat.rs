use log::warn;

use crate::types::{WadNode, WadSegment, WadSubSector, WadVertex};
use crate::{Lump, MapLump, WadData};

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
            warn!("NODES is an uncompressed zdoom style");
            return if bytes[1..] == [b'N', b'O', b'D'] {
                Self::Extended(ExtendedNodeType::XNOD)
            } else if bytes[1..] == [b'G', b'L', b'N'] {
                Self::Extended(ExtendedNodeType::XGLN)
            } else if bytes[1..] == [b'G', b'L', b'2'] {
                Self::Extended(ExtendedNodeType::XGL2)
            } else {
                warn!("Unknown X node type, using default");
                Self::OGDoom
            };
        }
        Self::OGDoom
    }
}

/// The data in the WAD lump is structured as follows:
///
/// | Field Size   | Type    | Content                                                       |
/// |--------------|---------|---------------------------------------------------------------|
/// | 0x00-0x03    | str     | 4 bytes of UTF 8 making up the lump signature, such as `XNOD` |
/// | 0x04-0x07    | u32     | Number of vertices from the VERTEXES lump                     |
/// | 0x08-0x11    | u32     | The `N` additional vertices that follow from here             |
/// | 8-byte chunk | u32,u32 | fixed,fixed Vertex: 16:16 fixed-point (x,y). Repeated `N` times from above |
/// | 4-bytes      | u32     | Subsector count                                               |
/// | 4-byte chunk | u32     | Subsector N: Seg count for this subsector                     |
/// | 4-bytes      | u32     | Segs count                                                    |
/// | 11-byte chunk| Segment | Seg N: New layout: `u32`:Vertex 1, `u32`Vertex 2, `u16`:Line, `u8`:Side |
/// | 4-byte chunk | u32     | Node count                                                    |
/// | 32-byte chunk| Node    | Node N: Same as vanilla except child ref are u32              |
///
/// Note: a 16:16 fixed point number is stored in 4 bytes.
///
/// Note: the OG Doom segs and subsectors lumps are empty if an extended format
/// is used. From the OG format you will require: `WadSector`, `WadLinedef`,
/// `WadSidedef`, and `WadThing`.
#[derive(Debug, Clone)]
pub struct WadExtendedMap {
    pub node_type: ExtendedNodeType,
    pub num_org_vertices: usize,
    pub num_new_vertices: usize,
    /// These should be appended to the `VETEXES` lump
    /// "When v >= OrgVerts, v- OrgVerts is the index of a vertex stored here",
    /// so just append to OrgVerts
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
                return Some(Self::parse_uncompressed(lump, t));
            } else {
                todo!("Compress zdoom nodes not supported yet")
            }
        }
        None
    }

    fn parse_uncompressed(lump: &Lump, etype: ExtendedNodeType) -> Self {
        let mut ofs = 4;
        let num_org_vertices = lump.read_u32(ofs) as usize;
        ofs += 4;
        let num_new_vertices = lump.read_u32(ofs) as usize;
        ofs += 4;

        let mut vertexes = Vec::with_capacity(num_new_vertices);
        let end = ofs + num_new_vertices * 8;
        // The vertices are in fixed-point format and will require conversion later
        // Each vert is x,y, where x and y are 4 bytes each
        while ofs < end {
            let v1 = lump.read_u32_to_f32(ofs);
            let v2 = lump.read_u32_to_f32(ofs + 4);
            vertexes.push(WadVertex::new(v1, v2));
            ofs += 8;
        }
        debug_assert_eq!(vertexes.len(), num_new_vertices);

        let num_subs = lump.read_u32(ofs) as usize;
        ofs += 4;
        let mut subsectors = Vec::with_capacity(num_subs);
        let end = ofs + num_subs * 4;
        let mut start_seg = 0;
        // subsectors are an index
        while ofs < end {
            let seg_count = lump.read_u32(ofs);
            ofs += 4;
            subsectors.push(WadSubSector {
                seg_count,
                start_seg,
            });
            start_seg += seg_count;
        }
        debug_assert_eq!(subsectors.len(), num_subs);

        let num_segs = lump.read_u32(ofs) as usize;
        ofs += 4;
        let mut segments = Vec::with_capacity(num_segs);
        let end = ofs + num_segs * 11;
        while ofs < end {
            segments.push(WadSegment::new_z(
                lump.read_u32(ofs),
                lump.read_u32(ofs + 4),
                lump.read_u16(ofs + 8),
                u8::from_be(lump.data[ofs + 10]) as i16,
            ));
            ofs += 11;
        }
        debug_assert_eq!(segments.len(), num_segs);

        let num_nodes = lump.read_u32(ofs) as usize;
        ofs += 4;
        let mut nodes = Vec::with_capacity(num_nodes);
        let end = ofs + num_nodes * 32;
        while ofs < end {
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
                lump.read_u32(ofs + 24), // right child index
                lump.read_u32(ofs + 28), // left child index
            ));
            ofs += 32
        }
        debug_assert_eq!(nodes.len(), num_nodes);

        Self {
            node_type: etype,
            num_org_vertices,
            num_new_vertices,
            vertexes,
            subsectors,
            segments,
            nodes,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::types::{WadLineDef, WadNode, WadSector, WadSideDef, WadVertex};
    use crate::WadData;

    use super::WadExtendedMap;

    #[ignore = "sunder.wad can't be included in git"]
    #[test]
    fn extended_nodes_sunder_m3_check_vertex() {
        let wad = WadData::new("/home/luke/DOOM/sunder.wad".into());
        let map = WadExtendedMap::parse(&wad, "MAP03").unwrap();

        // All verified with crispy
        const FRACUNIT: f32 = (1 << 16) as f32;
        // newVerts: 0 : 72351744
        assert_eq!(map.vertexes[0].x, 72351744f32 / FRACUNIT);
        // newVerts: 965 : 85983232
        assert_eq!(map.vertexes[965].x, 85983232f32 / FRACUNIT);

        let vertexes: Vec<WadVertex> = wad.vertex_iter("MAP03").collect();
        // org_vertexes: 5485 : 4390912
        assert_eq!(vertexes[5485].x, 4390912f32 / FRACUNIT);
        // vertexes: 4025 : -28311552
        assert_eq!(vertexes[4025].x, -28311552f32 / FRACUNIT);
    }

    #[ignore = "sunder.wad can't be included in git"]
    #[test]
    fn extended_nodes_sunder_m3_check_subs() {
        let wad = WadData::new("/home/luke/DOOM/sunder.wad".into());
        let map = WadExtendedMap::parse(&wad, "MAP03").unwrap();
        assert_eq!(map.subsectors.len(), 4338);

        // subsectors[1130]: first: 3834, num: 4
        assert_eq!(map.subsectors[1130].start_seg, 3834);
        assert_eq!(map.subsectors[1130].seg_count, 4);
        // subsectors[2770]: first: 9445, num: 5
        assert_eq!(map.subsectors[2770].start_seg, 9445);
        assert_eq!(map.subsectors[2770].seg_count, 5);
        // subsectors[4237]: first: 14226, num: 4
        assert_eq!(map.subsectors[4237].start_seg, 14226);
        assert_eq!(map.subsectors[4237].seg_count, 4);
    }

    #[ignore = "sunder.wad can't be included in git"]
    #[test]
    fn extended_nodes_sunder_m3_check_segs() {
        let wad = WadData::new("/home/luke/DOOM/sunder.wad".into());
        let map = WadExtendedMap::parse(&wad, "MAP03").unwrap();
        // numSegs: 14582
        assert_eq!(map.segments.len(), 14582);

        // seg:7990, v1: 2932
        // seg:7990, v2: 6083
        // seg:7990, linedef: 3352
        // seg:7990, side: 0
        assert_eq!(map.segments[7990].start_vertex, 2932);
        assert_eq!(map.segments[7990].end_vertex, 6083);
        assert_eq!(map.segments[7990].linedef, 3352);
        assert_eq!(map.segments[7990].side, 0);

        // seg:14398, v1: 2080
        // seg:14398, v2: 2082
        // seg:14398, linedef: 2084
        // seg:14398, side: 0
        assert_eq!(map.segments[14398].start_vertex, 2080);
        assert_eq!(map.segments[14398].end_vertex, 2082);
        assert_eq!(map.segments[14398].linedef, 2084);
        assert_eq!(map.segments[14398].side, 0);
    }

    #[ignore = "sunder.wad can't be included in git"]
    #[test]
    fn extended_nodes_sunder_m3_check_nodes() {
        let wad = WadData::new("/home/luke/DOOM/sunder.wad".into());
        let map = WadExtendedMap::parse(&wad, "MAP03").unwrap();
        // Node: 666
        // no->x: 12, no->y: -342, no->dx: 0, no->dy: -20
        // child[0]: 665, child[1]: -2147482974
        let node = map.nodes[666].clone();
        assert_eq!(
            node,
            WadNode {
                x: 12,
                y: -342,
                dx: 0,
                dy: -20,
                bboxes: [[-342, -362, 0, 12], [-333, -371, 12, 24]],
                children: [665, -2147482974i32 as u32]
            }
        );

        assert_eq!(node.children[0], 665);
        assert_eq!(node.children[0] & 0x80000000, 0);
        assert_eq!(node.children[0] ^ 0x80000000, 0x80000000 + 665);

        assert_eq!(node.children[1], 0x80000000 + 674);
        assert_eq!(node.children[1] & 0x80000000, 0x80000000);
        assert_eq!(node.children[1] ^ 0x80000000, 674);
    }

    #[test]
    fn extended_nodes_none() {
        let wad = WadData::new("../doom1.wad".into());
        assert!(WadExtendedMap::parse(&wad, "E1M1").is_none());
    }

    #[ignore = "sunder.wad can't be included in git"]
    #[test]
    fn extended_nodes_sunder_m3() {
        let wad = WadData::new("/home/luke/DOOM/sunder.wad".into());
        let map = WadExtendedMap::parse(&wad, "MAP03").unwrap();

        assert_eq!(map.num_org_vertices, 5525); // verified with crispy
        assert_eq!(map.vertexes.len(), 996); // verified with crispy
        assert_eq!(map.subsectors.len(), 4338);
        assert_eq!(map.segments.len(), 14582);
        assert_eq!(map.nodes.len(), 11589);

        let sectors: Vec<WadSector> = wad.sector_iter("MAP03").collect();
        assert_eq!(sectors.len(), 954);

        let linedefs: Vec<WadLineDef> = wad.linedef_iter("MAP03").collect();
        assert_eq!(linedefs.len(), 7476);
        assert_eq!(linedefs[3103].front_sidedef, 5094);
        assert_eq!(linedefs[3103].back_sidedef, Some(5095));
        assert_eq!(linedefs[3103].start_vertex, 2752); // 1016, -720
        assert_eq!(linedefs[3103].end_vertex, 2753); // 984,-696

        assert_eq!(linedefs[3122].front_sidedef, 5132);
        assert_eq!(linedefs[3122].back_sidedef, Some(5133));
        assert_eq!(linedefs[3122].start_vertex, 2771); // 988,-692
        assert_eq!(linedefs[3122].end_vertex, 2772); // 1020, -716

        assert_eq!(linedefs[2670].front_sidedef, 4387);
        assert_eq!(linedefs[2670].back_sidedef, Some(4388));
        assert_eq!(linedefs[2670].start_vertex, 2499); // test this
        assert_eq!(linedefs[2670].end_vertex, 2500); //

        let sidedefs: Vec<WadSideDef> = wad.sidedef_iter("MAP03").collect();
        assert_eq!(sidedefs.len(), 12781);
        assert_eq!(sidedefs[4387].lower_tex, "");
        assert_eq!(sidedefs[4387].upper_tex, "");
        assert_eq!(sidedefs[4388].lower_tex, "");
        assert_eq!(sidedefs[4388].upper_tex, "METAL");
        assert_eq!(sidedefs[4388].sector, 0); // sector 0 why???? This breaks shit

        let vertexes: Vec<WadVertex> = wad.vertex_iter("MAP03").collect();
        assert_eq!(map.num_org_vertices, vertexes.len());
        assert_eq!(vertexes[2752].x, 1016.0);
        assert_eq!(vertexes[2752].y, -720.0);
        assert_eq!(vertexes[2772].x, 1020.0);
        assert_eq!(vertexes[2772].y, -716.0);
        assert_eq!(vertexes[2499].x, 496.0);
        assert_eq!(vertexes[2499].y, -1040.0);
        assert_eq!(vertexes[2500].x, 496.0);
        assert_eq!(vertexes[2500].y, -1072.0);

        assert_eq!(map.vertexes[666].x, 2176.0);
        assert_eq!(map.vertexes[666].y, -496.0);

        let sidedefs: Vec<WadSideDef> = wad.sidedef_iter("MAP03").collect();
        assert_eq!(sidedefs.len(), 12781);
    }
}
