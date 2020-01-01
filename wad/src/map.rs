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
    pub x_pos: i16,
    pub y_pos: i16,
}
#[derive(Debug)]
pub struct LineDef {
    pub start_vertex: i16,
    pub end_vertex: i16,
    pub flags: u16, //TODO: enum?
    pub line_type: u16,
    pub sector_tag: u16,
    pub front_sidedef: u16, //0xFFFF means there is no sidedef
    pub back_sidedef: u16,  //0xFFFF means there is no sidedef
}

pub struct Map {
    pub name: String,
    pub vertexes: Vec<Vertex>,
    pub linedefs: Vec<LineDef>,
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
}
