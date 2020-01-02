use std::str;

// TODO: Why power of two?
pub enum LineDefFlags {
    Blocking = 0,
    BlockMonsters = 1,
    TwoSided = 2,
    UnpegTop = 4,
    UnpegBottom = 8,
    Secret = 16,
    SoundBlock = 32,
    DontDraw = 64,
    Draw = 128,
}

#[derive(Debug)]
pub struct Vertex {
    x_pos: i16,
    y_pos: i16,
}

impl Vertex {
    pub fn new(x: i16, y: i16) -> Vertex {
        Vertex { x_pos: x, y_pos: y }
    }

    pub fn x(&self) -> i16 {
        self.x_pos
    }

    pub fn y(&self) -> i16 {
        self.y_pos
    }
}

#[derive(Debug)]
pub struct LineDef {
    start_vertex: i16,
    end_vertex: i16,
    flags: u16, //TODO: enum?
    line_type: u16,
    sector_tag: u16,
    front_sidedef: u16, //0xFFFF means there is no sidedef
    back_sidedef: u16,  //0xFFFF means there is no sidedef
}

impl LineDef {
    pub fn new(
        start_vertex: i16,
        end_vertex: i16,
        flags: u16,
        line_type: u16,
        sector_tag: u16,
        front_sidedef: u16,
        back_sidedef: u16,
    ) -> LineDef {
        LineDef {
            start_vertex,
            end_vertex,
            flags,
            line_type,
            sector_tag,
            front_sidedef,
            back_sidedef,
        }
    }

    pub fn start_vertex(&self) -> i16 {
        self.start_vertex
    }

    pub fn end_vertex(&self) -> i16 {
        self.end_vertex
    }

    pub fn flags(&self) -> u16 {
        self.flags
    }

    pub fn line_type(&self) -> u16 {
        self.line_type
    }

    pub fn sector_tag(&self) -> u16 {
        self.sector_tag
    }

    pub fn front_sidedef(&self) -> u16 {
        self.front_sidedef
    }

    pub fn back_sidedef(&self) -> u16 {
        self.back_sidedef
    }
}

#[derive(Debug)]
pub struct Sector {
    floor_height: i16,
    ceil_height: i16,
    floor_tex: String,
    ceil_tex: String,
    light_level: u16,
    typ: u16,
    tag: u16,
}

impl Sector {
    pub fn new(
        floor_height: i16,
        ceil_height: i16,
        floor_tex: &[u8],
        ceil_tex: &[u8],
        light_level: u16,
        typ: u16,
        tag: u16,
    ) -> Sector {
        if floor_tex.len() != 8 {
            panic!(
                "sector floor_tex name incorrect length, expected 8, got {}",
                floor_tex.len()
            )
        }
        if ceil_tex.len() != 8 {
            panic!(
                "sector ceil_tex name incorrect length, expected 8, got {}",
                ceil_tex.len()
            )
        }
        Sector {
            floor_height,
            ceil_height,
            floor_tex: str::from_utf8(floor_tex)
                .expect("Invalid floor tex name")
                .trim_end_matches("\u{0}") // better to address this early to avoid many casts later
                .to_owned(),
            ceil_tex: str::from_utf8(ceil_tex)
                .expect("Invalid ceiling tex name")
                .trim_end_matches("\u{0}") // better to address this early to avoid many casts later
                .to_owned(),
            light_level,
            typ,
            tag,
        }
    }

    pub fn floor_height(&self) -> i16 {
        self.floor_height
    }

    pub fn ceil_height(&self) -> i16 {
        self.ceil_height
    }

    pub fn floor_tex(&self) -> &str {
        &self.floor_tex
    }

    pub fn ceil_tex(&self) -> &str {
        &self.ceil_tex
    }

    pub fn light_level(&self) -> u16 {
        self.light_level
    }

    pub fn typ(&self) -> u16 {
        self.typ
    }

    pub fn tag(&self) -> u16 {
        self.tag
    }
}

#[derive(Debug)]
pub struct SideDef {
    x_offset: i16,
    y_offset: i16,
    upper_tex: String,
    lower_tex: String,
    middle_tex: String,
    sector_id: u16,
}

impl SideDef {
    pub fn new(
        x_offset: i16,
        y_offset: i16,
        upper_tex: &[u8],
        lower_tex: &[u8],
        middle_tex: &[u8],
        sector_id: u16,
    ) -> SideDef {
        if upper_tex.len() != 8 {
            panic!(
                "sidedef upper_tex name incorrect length, expected 8, got {}",
                upper_tex.len()
            )
        }
        if lower_tex.len() != 8 {
            panic!(
                "sidedef lower_tex name incorrect length, expected 8, got {}",
                lower_tex.len()
            )
        }
        if middle_tex.len() != 8 {
            panic!(
                "sidedef middle_tex name incorrect length, expected 8, got {}",
                middle_tex.len()
            )
        }
        SideDef {
            x_offset,
            y_offset,
            upper_tex: str::from_utf8(upper_tex)
                .expect("Invalid upper_tex name")
                .trim_end_matches("\u{0}") // better to address this early to avoid many casts later
                .to_owned(),
            lower_tex: str::from_utf8(lower_tex)
                .expect("Invalid lower_tex name")
                .trim_end_matches("\u{0}") // better to address this early to avoid many casts later
                .to_owned(),
            middle_tex: str::from_utf8(middle_tex)
                .expect("Invalid middle_tex name")
                .trim_end_matches("\u{0}") // better to address this early to avoid many casts later
                .to_owned(),
            sector_id,
        }
    }

    pub fn x_offset(&self) -> i16 {
        self.x_offset
    }

    pub fn y_offset(&self) -> i16 {
        self.y_offset
    }

    pub fn upper_tex(&self) -> &str {
        &self.upper_tex
    }

    pub fn lower_tex(&self) -> &str {
        &self.lower_tex
    }

    pub fn middle_tex(&self) -> &str {
        &self.middle_tex
    }

    pub fn sector_id(&self) -> u16 {
        self.sector_id
    }
}

#[derive(Debug)]
pub struct Map {
    name: String,
    vertexes: Vec<Vertex>,
    linedefs: Vec<LineDef>,
    sectors: Vec<Sector>,
    sidedefs: Vec<SideDef>,
}

impl Map {
    pub fn new(name: String) -> Map {
        Map {
            name,
            vertexes: Vec::new(),
            linedefs: Vec::new(),
            sectors: Vec::new(),
            sidedefs: Vec::new(),
        }
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn add_vertex(&mut self, v: Vertex) {
        self.vertexes.push(v);
    }

    pub fn get_vertexes(&self) -> &[Vertex] {
        &self.vertexes
    }

    pub fn add_linedef(&mut self, l: LineDef) {
        self.linedefs.push(l);
    }

    pub fn get_linedefs(&self) -> &[LineDef] {
        &self.linedefs
    }

    pub fn add_sector(&mut self, s: Sector) {
        self.sectors.push(s);
    }

    pub fn get_sectors(&self) -> &[Sector] {
        &self.sectors
    }

    pub fn add_sidedef(&mut self, s: SideDef) {
        self.sidedefs.push(s);
    }

    pub fn get_sidedefs(&self) -> &[SideDef] {
        &self.sidedefs
    }
}

#[cfg(test)]
mod tests {
    use crate::map;
    use crate::wad::Wad;

    #[test]
    fn load_e1m1() {
        let mut wad = Wad::new("../doom1.wad");
        wad.read_directories();

        let mut map = map::Map::new("E1M1".to_owned());
        wad.load_map(&mut map);

        let vertexes = map.get_vertexes();
        assert_eq!(vertexes[0].x(), 1088);
        assert_eq!(vertexes[0].y(), -3680);
        assert_eq!(vertexes[466].x(), 2912);
        assert_eq!(vertexes[466].y(), -4848);

        let linedefs = map.get_linedefs();
        assert_eq!(linedefs[0].start_vertex(), 0);
        assert_eq!(linedefs[0].end_vertex(), 1);
        assert_eq!(linedefs[2].start_vertex(), 3);
        assert_eq!(linedefs[2].end_vertex(), 0);
        assert_eq!(linedefs[2].front_sidedef(), 2);
        assert_eq!(linedefs[2].back_sidedef(), 65535);
        assert_eq!(linedefs[474].start_vertex(), 384);
        assert_eq!(linedefs[474].end_vertex(), 348);
        assert_eq!(linedefs[474].flags(), 1);
        assert_eq!(linedefs[474].front_sidedef(), 647);
        assert_eq!(linedefs[474].back_sidedef(), 65535);

        let sectors = map.get_sectors();
        assert_eq!(sectors[0].floor_height(), 0);
        assert_eq!(sectors[0].ceil_height(), 72);
        assert_eq!(sectors[0].floor_tex(), "FLOOR4_8");
        assert_eq!(sectors[0].ceil_tex(), "CEIL3_5");
        assert_eq!(sectors[0].light_level(), 160);
        assert_eq!(sectors[0].typ(), 0);
        assert_eq!(sectors[0].tag(), 0);
        assert_eq!(sectors[84].floor_height(), -24);
        assert_eq!(sectors[84].ceil_height(), 48);
        assert_eq!(sectors[84].floor_tex(), "FLOOR5_2");
        assert_eq!(sectors[84].ceil_tex(), "CEIL3_5");
        assert_eq!(sectors[84].light_level(), 255);
        assert_eq!(sectors[84].typ(), 0);
        assert_eq!(sectors[84].tag(), 0);

        let sidedefs = map.get_sidedefs();
        assert_eq!(sidedefs[0].x_offset(), 0);
        assert_eq!(sidedefs[0].y_offset(), 0);
        assert_eq!(sidedefs[0].middle_tex(), "DOOR3");
        assert_eq!(sidedefs[0].sector_id(), 40);
        assert_eq!(sidedefs[9].x_offset(), 0);
        assert_eq!(sidedefs[9].y_offset(), 48);
        assert_eq!(sidedefs[9].middle_tex(), "BROWN1");
        assert_eq!(sidedefs[9].sector_id(), 38);
        assert_eq!(sidedefs[647].x_offset(), 4);
        assert_eq!(sidedefs[647].y_offset(), 0);
        assert_eq!(sidedefs[647].middle_tex(), "SUPPORT2");
        assert_eq!(sidedefs[647].sector_id(), 70);
    }
}
