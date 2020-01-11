use wad::Vertex;

pub struct Player {
    pos: Vertex,
    rot: f32,
}

impl Player {
    pub fn new(pos: Vertex, rot: f32) -> Player {
        Player { pos, rot }
    }

    pub fn pos(&self) -> &Vertex {
        &self.pos
    }

    pub fn rot(&self) -> f32 {
        self.rot
    }

    pub fn set_x(&mut self, x: f32) {
        self.pos.x = x
    }

    pub fn set_y(&mut self, y: f32) {
        self.pos.y = y
    }

    pub fn set_r(&mut self, r: f32) {
        self.rot = r
    }
}
