//! Runtime gameplay state for switch buttons. Asset data (textures, sprites,
//! palettes, colourmaps) has moved to the `pic-data` crate.

use level::MapPtr;
use level::map_defs::LineDef;

#[derive(Debug)]
pub enum ButtonWhere {
    Top,
    Middle,
    Bottom,
}

#[derive(Debug)]
pub struct Button {
    pub line: MapPtr<LineDef>,
    pub bwhere: ButtonWhere,
    pub texture: usize,
    pub timer: u32,
}
