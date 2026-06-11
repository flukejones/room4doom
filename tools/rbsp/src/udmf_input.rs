//! Bridge UDMF map data into the builder's input model. The 2D BSP is built
//! from the geometry exactly as for classic maps; slope planes ride alongside
//! and are consumed by [`crate::bsp3d::Bsp3dInput`].

use crate::BspInput;
use crate::types::{LineDefAccess, SectorAccess, SideDefAccess, SlopePlane, VertexCoords};
use wad::udmf::UdmfMap;

/// ML_TWOSIDED bit, matching the classic linedef flag the builder reads.
const TWO_SIDED_FLAG: u32 = 0x4;

/// A UDMF vertex carrying f64 coordinates.
pub struct UdmfInputVertex {
    pub x: f64,
    pub y: f64,
}

impl VertexCoords for UdmfInputVertex {
    fn x_f64(&self) -> f64 {
        self.x
    }
    fn y_f64(&self) -> f64 {
        self.y
    }
}

pub struct UdmfInputLineDef {
    pub v1: usize,
    pub v2: usize,
    pub front: Option<usize>,
    pub back: Option<usize>,
    pub flags: u32,
    pub special: u32,
    pub tag: i16,
}

impl LineDefAccess for UdmfInputLineDef {
    fn start_vertex_idx(&self) -> usize {
        self.v1
    }
    fn end_vertex_idx(&self) -> usize {
        self.v2
    }
    fn front_sidedef_idx(&self) -> Option<usize> {
        self.front
    }
    fn back_sidedef_idx(&self) -> Option<usize> {
        self.back
    }
    fn flags_u32(&self) -> u32 {
        self.flags
    }
    fn special_u32(&self) -> u32 {
        self.special
    }
    fn tag_i16(&self) -> i16 {
        self.tag
    }
}

pub struct UdmfInputSideDef {
    pub sector: usize,
    pub has_top: bool,
    pub has_bottom: bool,
    pub has_mid: bool,
}

impl SideDefAccess for UdmfInputSideDef {
    fn sector_idx(&self) -> usize {
        self.sector
    }
    fn has_top_tex(&self) -> bool {
        self.has_top
    }
    fn has_bottom_tex(&self) -> bool {
        self.has_bottom
    }
    fn has_mid_tex(&self) -> bool {
        self.has_mid
    }
}

pub struct UdmfInputSector {
    pub floor_h: f32,
    pub ceil_h: f32,
    pub tag: i16,
    pub floor_tex: String,
    pub ceil_tex: String,
    pub floor_plane: Option<SlopePlane>,
    pub ceil_plane: Option<SlopePlane>,
}

impl SectorAccess for UdmfInputSector {
    fn floor_h(&self) -> f32 {
        self.floor_h
    }
    fn ceil_h(&self) -> f32 {
        self.ceil_h
    }
    fn tag_i16(&self) -> i16 {
        self.tag
    }
    fn floor_tex_is(&self, name: &str) -> bool {
        self.floor_tex.eq_ignore_ascii_case(name)
    }
    fn ceil_tex_is(&self, name: &str) -> bool {
        self.ceil_tex.eq_ignore_ascii_case(name)
    }
    fn floor_plane(&self) -> Option<SlopePlane> {
        self.floor_plane
    }
    fn ceil_plane(&self) -> Option<SlopePlane> {
        self.ceil_plane
    }
}

/// The builder-ready records derived from a [`UdmfMap`].
pub struct UdmfInput {
    pub vertices: Vec<UdmfInputVertex>,
    pub linedefs: Vec<UdmfInputLineDef>,
    pub sidedefs: Vec<UdmfInputSideDef>,
    pub sectors: Vec<UdmfInputSector>,
}

impl UdmfInput {
    pub fn from_map(map: &UdmfMap) -> Self {
        let vertices = map
            .vertices
            .iter()
            .map(|v| UdmfInputVertex {
                x: v.x,
                y: v.y,
            })
            .collect();

        let linedefs = map
            .linedefs
            .iter()
            .map(|ld| {
                let mut flags = 0u32;
                if ld.twosided {
                    flags |= TWO_SIDED_FLAG;
                }
                UdmfInputLineDef {
                    v1: ld.v1,
                    v2: ld.v2,
                    front: Some(ld.sidefront),
                    back: ld.sideback,
                    flags,
                    special: ld.special as u32,
                    tag: i16::try_from(ld.id).unwrap_or(0),
                }
            })
            .collect();

        let sidedefs = map
            .sidedefs
            .iter()
            .map(|sd| UdmfInputSideDef {
                sector: sd.sector,
                has_top: sd.texturetop.is_some(),
                has_bottom: sd.texturebottom.is_some(),
                has_mid: sd.texturemiddle.is_some(),
            })
            .collect();

        let sectors = map
            .sectors
            .iter()
            .map(|s| UdmfInputSector {
                floor_h: s.heightfloor as f32,
                ceil_h: s.heightceiling as f32,
                tag: i16::try_from(s.id).unwrap_or(0),
                floor_tex: s.texturefloor.clone(),
                ceil_tex: s.textureceiling.clone(),
                floor_plane: s.floor_plane.and_then(plane_from_udmf),
                ceil_plane: s.ceiling_plane.and_then(plane_from_udmf),
            })
            .collect();

        Self {
            vertices,
            linedefs,
            sidedefs,
            sectors,
        }
    }

    /// The records `build_bsp` consumes (trait-typed; no WAD conversion).
    pub fn into_bsp_input(
        self,
    ) -> BspInput<UdmfInputVertex, UdmfInputLineDef, UdmfInputSideDef, UdmfInputSector> {
        BspInput {
            vertices: self.vertices,
            linedefs: self.linedefs,
            sidedefs: self.sidedefs,
            sectors: self.sectors,
        }
    }
}

fn plane_from_udmf(p: [f64; 4]) -> Option<SlopePlane> {
    SlopePlane::new(p[0] as f32, p[1] as f32, p[2] as f32, p[3] as f32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bsp3d::{Bsp3dBuilder, Bsp3dInput};
    use crate::{BspOptions, build_bsp};
    use wad::udmf::parse_textmap;

    const SLOPED_FLOOR: &str = include_str!("../../../data/test_files/udmf/sloped_floor.textmap");
    const SLOPED_CEILING: &str =
        include_str!("../../../data/test_files/udmf/sloped_ceiling.textmap");
    const FLAT_ROOM: &str = include_str!("../../../data/test_files/udmf/flat_room.textmap");

    fn build(textmap: &str) -> (Bsp3dInput, crate::bsp3d::Bsp3dLump) {
        let map = parse_textmap(textmap).expect("parse");
        let bsp_input = UdmfInput::from_map(&map).into_bsp_input();
        let bsp = build_bsp(&bsp_input, &BspOptions::default());
        let input = Bsp3dInput::new(
            &bsp_input.linedefs,
            &bsp_input.sidedefs,
            &bsp_input.sectors,
            &bsp,
            None,
            false,
        );
        let lump = Bsp3dBuilder::build(&input, &bsp.nodes);
        (input, lump)
    }

    /// Every floor-flat vertex of a sloped sector sits on its plane.
    #[test]
    fn sloped_floor_vertices_lie_on_plane() {
        let (bsp_input, lump) = build(SLOPED_FLOOR);
        let plane = bsp_input.sectors[0].floor_plane.expect("floor slope");
        assert_eq!(
            plane,
            SlopePlane {
                a: -0.25,
                b: 0.0,
                c: 1.0,
                d: 0.0
            }
        );

        // A floor-surface vertex must sit on the plane and above z=0.
        let mut saw_raised = false;
        for v in &lump.vertices {
            if (v.z - plane.z_at(v.x, v.y)).abs() < 1e-3 && v.z > 1.0 {
                saw_raised = true;
            }
        }
        assert!(saw_raised, "a sloped floor vertex was lifted above z=0");
    }

    /// Sloped ceiling vertices descend along the plane.
    #[test]
    fn sloped_ceiling_vertices_lie_on_plane() {
        let (bsp_input, lump) = build(SLOPED_CEILING);
        let plane = bsp_input.sectors[0].ceil_plane.expect("ceil slope");
        let mut saw_dropped = false;
        for v in &lump.vertices {
            if (v.z - plane.z_at(v.x, v.y)).abs() < 1e-3 && v.z < 127.0 && v.z > 1.0 {
                saw_dropped = true;
            }
        }
        assert!(saw_dropped, "a sloped ceiling vertex dropped below z=128");
    }

    /// Flat input is unaffected: floor at z=0, ceiling at z=128, nothing between.
    #[test]
    fn flat_room_has_no_slope() {
        let (bsp_input, lump) = build(FLAT_ROOM);
        assert!(bsp_input.sectors[0].floor_plane.is_none());
        assert!(bsp_input.sectors[0].ceil_plane.is_none());
        for v in &lump.vertices {
            assert!(
                v.z == 0.0 || v.z == 128.0,
                "flat room vertex at unexpected z {}",
                v.z
            );
        }
    }
}
