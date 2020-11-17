use wad::{lumps::SubSector, DPtr, Vertex};

use crate::angle::Angle;

#[derive(Debug)]
pub struct Player {
    pub xy:         Vertex,
    pub z:          f32,
    pub rotation:   Angle,
    pub sub_sector: DPtr<SubSector>,
}

impl Player {
    pub fn new(
        xy: Vertex,
        z: f32,
        rotation: Angle,
        sub_sector: DPtr<SubSector>,
    ) -> Player {
        Player {
            xy,
            z,
            rotation,
            sub_sector,
        }
    }
}
