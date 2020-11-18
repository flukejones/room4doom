use wad::Vertex;

pub struct Stuff {
    pos: Vertex,
    rot: f32,
}

impl Stuff {
    pub fn new(pos: Vertex, rot: f32) -> Self {
        Self { pos, rot }
    }

    pub fn pos(&self) -> &Vertex {
        &self.pos
    }

    pub fn rot(&self) -> f32 {
        self.rot
    }

    pub fn set_r(&mut self, r: f32) {
        self.rot = r
    }
}
