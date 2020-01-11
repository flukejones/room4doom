use crate::{lumps::*, Vertex};
use std::str;

/// The smallest vector and the largest vertex, combined make up a
/// rectangle enclosing the map area
#[derive(Debug, Default)]
pub struct MapExtents {
    pub min_vertex: Vertex,
    pub max_vertex: Vertex,
    pub width: f32,
    pub height: f32,
    pub automap_scale: f32,
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
    nodes: Vec<Node>,
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
            nodes: Vec::new(),
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
        // set the min/max to first vertex so we have a baseline
        // that isn't 0 causing comparison issues, eg; if it's 0,
        // then a min vertex of -3542 won't be set since it's negative
        self.extents.min_vertex.x = self.vertexes[0].x;
        self.extents.min_vertex.y = self.vertexes[0].y;
        self.extents.max_vertex.x = self.vertexes[0].x;
        self.extents.max_vertex.y = self.vertexes[0].y;
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
        self.extents.width = (self.extents.max_vertex.x - self.extents.min_vertex.x) as f32;
        self.extents.height = (self.extents.max_vertex.y - self.extents.min_vertex.y) as f32;
    }

    pub fn get_vertexes(&self) -> &[Vertex] {
        &self.vertexes
    }

    pub fn set_vertexes(&mut self, vertexes: Vec<Vertex>) {
        self.vertexes = vertexes;
        self.set_extents();
    }

    pub fn get_linedefs(&self) -> &[LineDef] {
        &self.linedefs
    }

    pub fn set_linedefs(&mut self, linedefs: Vec<LineDef>) {
        self.linedefs = linedefs;
    }

    pub fn get_sectors(&self) -> &[Sector] {
        &self.sectors
    }

    pub fn set_sectors(&mut self, sectors: Vec<Sector>) {
        self.sectors = sectors;
    }

    pub fn get_sidedefs(&self) -> &[SideDef] {
        &self.sidedefs
    }

    pub fn set_sidedefs(&mut self, sidedefs: Vec<SideDef>) {
        self.sidedefs = sidedefs;
    }

    pub fn get_subsectors(&self) -> &[SubSector] {
        &self.subsectors
    }

    pub fn set_subsectors(&mut self, subsectors: Vec<SubSector>) {
        self.subsectors = subsectors;
    }

    pub fn get_segments(&self) -> &[Segment] {
        &self.segments
    }

    pub fn set_segments(&mut self, segments: Vec<Segment>) {
        self.segments = segments;
    }

    pub fn get_extents(&self) -> &MapExtents {
        &self.extents
    }

    pub fn set_scale(&mut self, scale: f32) {
        self.extents.automap_scale = scale
    }

    pub fn get_nodes(&self) -> &[Node] {
        &self.nodes
    }

    pub fn set_nodes(&mut self, nodes: Vec<Node>) {
        self.nodes = nodes;
    }

    pub fn find_subsector(
        &self,
        point: &Vertex,
        node_id: u16,
        nodes: &[Node],
    ) -> Option<&SubSector> {
        // Test if it is a child node or a leaf node
        if node_id & IS_SSECTOR_MASK == IS_SSECTOR_MASK {
            // It's a leaf node and is the index to a subsector
            return Some(&self.get_subsectors()[(node_id ^ IS_SSECTOR_MASK) as usize]);
        }

        let node = &nodes[node_id as usize];
        let side = node.point_on_side(&point);

        if let Some(res) = self.find_subsector(&point, node.child_index[side], nodes) {
            return Some(res);
        }
        if node.point_in_bounds(&point, side ^ 1) {
            return self.find_subsector(&point, node.child_index[side ^ 1], nodes);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use crate::map;
    use crate::wad::Wad;

    #[test]
    fn check_e1m1_things() {
        let mut wad = Wad::new("../doom1.wad");
        wad.read_directories();

        let mut map = map::Map::new("E1M1".to_owned());
        wad.load_map(&mut map);

        let things = map.get_things();
        assert_eq!(things[0].pos.x, 1056.0);
        assert_eq!(things[0].pos.y, -3616.0);
        assert_eq!(things[0].angle, 90.0);
        assert_eq!(things[0].kind, 1);
        assert_eq!(things[0].flags, 7);
        assert_eq!(things[137].pos.x, 3648.0);
        assert_eq!(things[137].pos.y, -3840.0);
        assert_eq!(things[137].angle, 0.0);
        assert_eq!(things[137].kind, 2015);
        assert_eq!(things[137].flags, 7);

        assert_eq!(things[0].angle, 90.0);
        assert_eq!(things[9].angle, 135.0);
        assert_eq!(things[14].angle, 0.0);
        assert_eq!(things[16].angle, 90.0);
        assert_eq!(things[17].angle, 180.0);
        assert_eq!(things[83].angle, 270.0);
    }

    #[test]
    fn check_e1m1_vertexes() {
        let mut wad = Wad::new("../doom1.wad");
        wad.read_directories();

        let mut map = map::Map::new("E1M1".to_owned());
        wad.load_map(&mut map);

        let vertexes = map.get_vertexes();
        assert_eq!(vertexes[0].x, 1088.0);
        assert_eq!(vertexes[0].y, -3680.0);
        assert_eq!(vertexes[466].x, 2912.0);
        assert_eq!(vertexes[466].y, -4848.0);
    }

    #[test]
    fn check_e1m1_lump_pointers() {
        let mut wad = Wad::new("../doom1.wad");
        wad.read_directories();

        let mut map = map::Map::new("E1M1".to_owned());
        wad.load_map(&mut map);
        let linedefs = map.get_linedefs();

        // Check links
        // LINEDEF->VERTEX
        assert_eq!(linedefs[2].start_vertex.get().x, 1088.0);
        assert_eq!(linedefs[2].end_vertex.get().x, 1088.0);
        // LINEDEF->SIDEDEF
        assert_eq!(linedefs[2].front_sidedef.get().middle_tex, "LITE3");
        // LINEDEF->SIDEDEF->SECTOR
        assert_eq!(
            linedefs[2].front_sidedef.get().sector.get().floor_tex,
            "FLOOR4_8"
        );
        // LINEDEF->SIDEDEF->SECTOR
        assert_eq!(linedefs[2].front_sidedef.get().sector.get().ceil_height, 72);

        let segments = map.get_segments();
        // SEGMENT->VERTEX
        assert_eq!(segments[0].start_vertex.get().x, 1552.0);
        assert_eq!(segments[0].end_vertex.get().x, 1552.0);
        // SEGMENT->LINEDEF->SIDEDEF->SECTOR
        // seg:0 -> line:152 -> side:209 -> sector:0 -> ceiltex:CEIL3_5 lightlevel:160
        assert_eq!(
            segments[0]
                .linedef
                .get()
                .front_sidedef
                .get()
                .sector
                .get()
                .ceil_tex,
            "CEIL3_5"
        );
        // SEGMENT->LINEDEF->SIDEDEF
        assert_eq!(
            segments[0].linedef.get().front_sidedef.get().upper_tex,
            "BIGDOOR2"
        );

        let sides = map.get_sidedefs();
        assert_eq!(sides[211].sector.get().ceil_tex, "TLITE6_4");
    }

    #[test]
    fn check_e1m1_linedefs() {
        let mut wad = Wad::new("../doom1.wad");
        wad.read_directories();

        let mut map = map::Map::new("E1M1".to_owned());
        wad.load_map(&mut map);
        let linedefs = map.get_linedefs();
        assert_eq!(linedefs[0].start_vertex.get().x, 1088.0);
        assert_eq!(linedefs[0].end_vertex.get().x, 1024.0);
        assert_eq!(linedefs[2].start_vertex.get().x, 1088.0);
        assert_eq!(linedefs[2].end_vertex.get().x, 1088.0);

        assert_eq!(linedefs[474].start_vertex.get().x, 3536.0);
        assert_eq!(linedefs[474].end_vertex.get().x, 3520.0);
        assert!(linedefs[2].back_sidedef.is_none());
        assert_eq!(linedefs[474].flags, 1);
        assert!(linedefs[474].back_sidedef.is_none());
        assert!(linedefs[466].back_sidedef.is_some());

        // Flag check
        assert_eq!(linedefs[26].flags, 29);
    }

    #[test]
    fn check_e1m1_sectors() {
        let mut wad = Wad::new("../doom1.wad");
        wad.read_directories();

        let mut map = map::Map::new("E1M1".to_owned());
        wad.load_map(&mut map);

        let sectors = map.get_sectors();
        assert_eq!(sectors[0].floor_height, 0);
        assert_eq!(sectors[0].ceil_height, 72);
        assert_eq!(sectors[0].floor_tex, "FLOOR4_8");
        assert_eq!(sectors[0].ceil_tex, "CEIL3_5");
        assert_eq!(sectors[0].light_level, 160);
        assert_eq!(sectors[0].kind, 0);
        assert_eq!(sectors[0].tag, 0);
        assert_eq!(sectors[84].floor_height, -24);
        assert_eq!(sectors[84].ceil_height, 48);
        assert_eq!(sectors[84].floor_tex, "FLOOR5_2");
        assert_eq!(sectors[84].ceil_tex, "CEIL3_5");
        assert_eq!(sectors[84].light_level, 255);
        assert_eq!(sectors[84].kind, 0);
        assert_eq!(sectors[84].tag, 0);
    }

    #[test]
    fn check_e1m1_sidedefs() {
        let mut wad = Wad::new("../doom1.wad");
        wad.read_directories();

        let mut map = map::Map::new("E1M1".to_owned());
        wad.load_map(&mut map);

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
    }

    #[test]
    fn check_e1m1_segments() {
        let mut wad = Wad::new("../doom1.wad");
        wad.read_directories();

        let mut map = map::Map::new("E1M1".to_owned());
        wad.load_map(&mut map);

        let segments = map.get_segments();
        assert_eq!(segments[0].start_vertex.get().x, 1552.0);
        assert_eq!(segments[0].end_vertex.get().x, 1552.0);
        assert_eq!(segments[731].start_vertex.get().x, 3040.0);
        assert_eq!(segments[731].end_vertex.get().x, 2976.0);
        assert_eq!(segments[0].angle, 16384.0);
        assert_eq!(
            segments[0].linedef.get().front_sidedef.get().upper_tex,
            "BIGDOOR2"
        );
        assert_eq!(segments[0].direction, 0);
        assert_eq!(segments[0].offset, 0);

        assert_eq!(segments[731].angle, 32768.0);
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
