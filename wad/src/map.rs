use crate::lumps::{LineDef, Sector, Segment, SideDef, SubSector, Thing, Vertex};
use std::str;

/// The smallest vector and the largest vertex, combined make up a
/// rectangle enclosing the map area
#[derive(Debug, Default)]
pub struct MapExtents {
    pub min_vertex: Vertex,
    pub max_vertex: Vertex,
    pub automap_scale: i16,
}

/// A `Map` contains everything required for building the actual level the
/// player will see in-game, such as the data to build a map, the textures used,
/// `Things`, `Sounds` and others.
///
/// `nodes`, `subsectors`, and `segments` are what get used most to render the
/// basic map
///
/// Access to the `Vec` arrays within is limited to immutable only to
/// prevent unwanted removal of items, which *will* break references and
/// segfault
#[derive(Debug)]
pub struct Map {
    name: String,
    things: Vec<Thing>,
    vertexes: Vec<Vertex>,
    linedefs: Vec<LineDef>,
    sectors: Vec<Sector>,
    sidedefs: Vec<SideDef>,
    subsectors: Vec<SubSector>,
    segments: Vec<Segment>,
    extents: MapExtents,
}

impl Map {
    pub fn new(name: String) -> Map {
        Map {
            name,
            things: Vec::new(),
            vertexes: Vec::new(),
            linedefs: Vec::new(),
            sectors: Vec::new(),
            sidedefs: Vec::new(),
            subsectors: Vec::new(),
            segments: Vec::new(),
            extents: MapExtents::default(),
        }
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn get_things(&self) -> &[Thing] {
        &self.things
    }

    pub fn set_things(&mut self, t: Vec<Thing>) {
        self.things = t;
    }

    pub fn set_extents(&mut self) {
        for v in &self.vertexes {
            if self.extents.min_vertex.x > v.x {
                self.extents.min_vertex.x = v.x;
            } else if self.extents.max_vertex.x < v.x {
                self.extents.max_vertex.x = v.x;
            }

            if self.extents.min_vertex.y > v.y {
                self.extents.min_vertex.y = v.y;
            } else if self.extents.max_vertex.y < v.y {
                self.extents.max_vertex.y = v.y;
            }
        }
    }

    pub fn get_vertexes(&self) -> &[Vertex] {
        &self.vertexes
    }

    pub fn set_vertexes(&mut self, v: Vec<Vertex>) {
        self.vertexes = v;
        self.set_extents();
    }

    pub fn get_linedefs(&self) -> &[LineDef] {
        &self.linedefs
    }

    pub fn set_linedefs(&mut self, l: Vec<LineDef>) {
        self.linedefs = l;
    }

    pub fn get_sectors(&self) -> &[Sector] {
        &self.sectors
    }

    pub fn set_sectors(&mut self, s: Vec<Sector>) {
        self.sectors = s;
    }

    pub fn get_sidedefs(&self) -> &[SideDef] {
        &self.sidedefs
    }

    pub fn set_sidedefs(&mut self, s: Vec<SideDef>) {
        self.sidedefs = s;
    }

    pub fn get_subsectors(&self) -> &[SubSector] {
        &self.subsectors
    }

    pub fn set_subsectors(&mut self, s: Vec<SubSector>) {
        self.subsectors = s;
    }

    pub fn get_segments(&self) -> &[Segment] {
        &self.segments
    }

    pub fn set_segments(&mut self, s: Vec<Segment>) {
        self.segments = s;
    }

    pub fn get_extents(&self) -> &MapExtents {
        &self.extents
    }

    pub fn set_scale(&mut self, scale: i16) {
        self.extents.automap_scale = scale
    }
}

#[cfg(test)]
mod tests {
    use crate::lumps::*;
    use crate::map;
    use crate::wad::Wad;

    #[test]
    fn check_flags_enum() {
        let flag = 28; // upper and lower unpegged, twosided
        println!("Blocking, two-sided, unpeg top and bottom\n{:#018b}", 29);
        println!("Flag: Blocking\n{:#018b}", LineDefFlags::Blocking as u16);
        println!(
            "Flag: Block Monsters\n{:#018b}",
            LineDefFlags::BlockMonsters as u16
        );
        println!("Flag: Two-sided\n{:#018b}", LineDefFlags::TwoSided as u16);
        println!("Flag: Unpeg upper\n{:#018b}", LineDefFlags::UnpegTop as u16);
        println!(
            "Flag: Unpeg lower\n{:#018b}",
            LineDefFlags::UnpegBottom as u16
        );
        println!("Flag: Secret\n{:#018b}", LineDefFlags::Secret as u16);
        println!(
            "Flag: Block sound\n{:#018b}",
            LineDefFlags::BlockSound as u16
        );
        println!(
            "Flag: Not on AutoMap yet\n{:#018b}",
            LineDefFlags::DontDraw as u16
        );
        println!(
            "Flag: Already on AutoMap\n{:#018b}",
            LineDefFlags::Draw as u16
        );
        let compare = LineDefFlags::TwoSided as u16
            | LineDefFlags::UnpegTop as u16
            | LineDefFlags::UnpegBottom as u16;
        assert_eq!(compare, flag);
    }

    #[test]
    fn load_e1m1() {
        let mut wad = Wad::new("../doom1.wad");
        wad.read_directories();

        let mut map = map::Map::new("E1M1".to_owned());
        wad.load_map(&mut map);

        let things = map.get_things();
        assert_eq!(things[0].pos_x, 1056);
        assert_eq!(things[0].pos_y, -3616);
        assert_eq!(things[0].angle, 90);
        assert_eq!(things[0].typ, 1);
        assert_eq!(things[0].flags, 7);
        assert_eq!(things[137].pos_x, 3648);
        assert_eq!(things[137].pos_y, -3840);
        assert_eq!(things[137].angle, 0);
        assert_eq!(things[137].typ, 2015);
        assert_eq!(things[137].flags, 7);

        let vertexes = map.get_vertexes();
        assert_eq!(vertexes[0].x, 1088);
        assert_eq!(vertexes[0].y, -3680);
        assert_eq!(vertexes[466].x, 2912);
        assert_eq!(vertexes[466].y, -4848);

        let linedefs = map.get_linedefs();
        assert_eq!(linedefs[0].start_vertex.get().x, 1088);
        assert_eq!(linedefs[0].end_vertex.get().x, 1024);
        assert_eq!(linedefs[2].start_vertex.get().x, 1088);
        assert_eq!(linedefs[2].end_vertex.get().x, 1088);
        assert_eq!(
            linedefs[2].front_sidedef.get().sector.get().floor_tex,
            "FLOOR4_8"
        );
        assert_eq!(linedefs[474].start_vertex.get().x, 3536);
        assert_eq!(linedefs[474].end_vertex.get().x, 3520);
        assert_eq!(
            linedefs[474].front_sidedef.get().sector.get().floor_tex,
            "FLOOR4_8"
        );
        assert!(linedefs[2].back_sidedef.is_none());
        assert_eq!(linedefs[474].flags, 1);
        assert!(linedefs[474].back_sidedef.is_none());
        assert!(linedefs[466].back_sidedef.is_some());

        // Flag check
        assert_eq!(linedefs[26].flags, 29);
        let compare = LineDefFlags::Blocking as u16
            | LineDefFlags::TwoSided as u16
            | LineDefFlags::UnpegTop as u16
            | LineDefFlags::UnpegBottom as u16;
        assert_eq!(linedefs[26].flags, compare);

        let sectors = map.get_sectors();
        assert_eq!(sectors[0].floor_height, 0);
        assert_eq!(sectors[0].ceil_height, 72);
        assert_eq!(sectors[0].floor_tex, "FLOOR4_8");
        assert_eq!(sectors[0].ceil_tex, "CEIL3_5");
        assert_eq!(sectors[0].light_level, 160);
        assert_eq!(sectors[0].typ, 0);
        assert_eq!(sectors[0].tag, 0);
        assert_eq!(sectors[84].floor_height, -24);
        assert_eq!(sectors[84].ceil_height, 48);
        assert_eq!(sectors[84].floor_tex, "FLOOR5_2");
        assert_eq!(sectors[84].ceil_tex, "CEIL3_5");
        assert_eq!(sectors[84].light_level, 255);
        assert_eq!(sectors[84].typ, 0);
        assert_eq!(sectors[84].tag, 0);

        let sidedefs = map.get_sidedefs();
        assert_eq!(sidedefs[0].x_offset, 0);
        assert_eq!(sidedefs[0].y_offset, 0);
        assert_eq!(sidedefs[0].middle_tex, "DOOR3");
        assert_eq!(sidedefs[0].sector.get().floor_tex, "FLOOR4_8");
        assert_eq!(sidedefs[9].x_offset, 0);
        assert_eq!(sidedefs[9].y_offset, 48);
        assert_eq!(sidedefs[9].middle_tex, "BROWN1");
        assert_eq!(sidedefs[9].sector.get().floor_tex, "FLOOR4_8");
        assert_eq!(sidedefs[647].x_offset, 4);
        assert_eq!(sidedefs[647].y_offset, 0);
        assert_eq!(sidedefs[647].middle_tex, "SUPPORT2");
        assert_eq!(sidedefs[647].sector.get().floor_tex, "FLOOR4_8");

        let segments = map.get_segments();
        assert_eq!(segments[0].start_vertex.get().x, 1552);
        assert_eq!(segments[0].end_vertex.get().x, 1552);
        assert_eq!(segments[731].start_vertex.get().x, 3040);
        assert_eq!(segments[731].end_vertex.get().x, 2976);
        assert_eq!(segments[0].angle, 16384);
        assert_eq!(
            segments[0].linedef.get().front_sidedef.get().upper_tex,
            "BIGDOOR2"
        );
        assert_eq!(segments[0].direction, 0);
        assert_eq!(segments[0].offset, 0);

        assert_eq!(segments[731].angle, 32768);
        assert_eq!(
            segments[731].linedef.get().front_sidedef.get().upper_tex,
            "STARTAN1"
        );
        assert_eq!(segments[731].direction, 1);
        assert_eq!(segments[731].offset, 0);

        let subsectors = map.get_subsectors();
        assert_eq!(subsectors[0].seg_count, 4);
        assert_eq!(subsectors[124].seg_count, 3);
        assert_eq!(subsectors[236].seg_count, 4);
        //assert_eq!(subsectors[0].start_seg.get().start_vertex.get().x, 1552);
        //assert_eq!(subsectors[124].start_seg.get().start_vertex.get().x, 472);
        //assert_eq!(subsectors[236].start_seg.get().start_vertex.get().x, 3040);
    }
}
