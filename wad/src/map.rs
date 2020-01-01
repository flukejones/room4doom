// TODO: Why power of two?
pub enum LineDefFlags {
    Blocking = 0,
    BlockMonsters = 1,
    TwoSided = 2,
    DontPegTop = 4,
    DontPegBottom = 8,
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

pub struct Map {
    name: String,
    vertexes: Vec<Vertex>,
    linedefs: Vec<LineDef>,
}

impl Map {
    pub fn new(name: String) -> Map {
        Map {
            name,
            vertexes: Vec::new(),
            linedefs: Vec::new(),
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
}

#[cfg(test)]
mod tests {
    use crate::map;
    use crate::wad::Wad;

    #[test]
    fn load_e1m1_vertexes() {
        let mut wad = Wad::new("../doom1.wad");
        wad.read_directories();

        let mut map = map::Map::new("E1M1".to_owned());
        let index = wad.find_lump_index(map.get_name());
        wad.read_map_vertexes(index, &mut map);

        assert_eq!(map.get_vertexes()[0].x(), 1088);
        assert_eq!(map.get_vertexes()[0].y(), -3680);
    }

    #[test]
    fn load_e1m1_linedefs() {
        let mut wad = Wad::new("../doom1.wad");
        wad.read_directories();

        let mut map = map::Map::new("E1M1".to_owned());
        let index = wad.find_lump_index(map.get_name());
        wad.read_map_linedefs(index, &mut map);

        let linedefs = map.get_linedefs();
        assert_eq!(linedefs[0].start_vertex(), 0);
        assert_eq!(linedefs[0].end_vertex(), 1);
        assert_eq!(linedefs[2].start_vertex(), 3);
        assert_eq!(linedefs[2].end_vertex(), 0);
        assert_eq!(linedefs[2].front_sidedef(), 2);
        assert_eq!(linedefs[2].back_sidedef(), 65535);
    }
}
