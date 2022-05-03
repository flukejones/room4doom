pub const MOBJ_INFO_HEADER_STR: &str = r#"use crate::{
    info::{MapObjKind, StateNum},
    obj::MapObjFlag,
};

use super::SfxEnum;
"#;

pub const MOBJ_INFO_TYPE_STR: &str = r#"
#[derive(Debug, Copy, Clone)]
pub struct MapObjInfo {
    pub doomednum: i32,
    pub spawnstate: StateNum,
    pub spawnhealth: i32,
    pub seestate: StateNum,
    pub seesound: SfxEnum,
    pub reactiontime: i32,
    pub attacksound: SfxEnum,
    pub painstate: StateNum,
    pub painchance: i32,
    pub painsound: SfxEnum,
    pub meleestate: StateNum,
    pub missilestate: StateNum,
    pub deathstate: StateNum,
    pub xdeathstate: StateNum,
    pub deathsound: SfxEnum,
    pub speed: f32,
    pub radius: f32,
    pub height: f32,
    pub mass: i32,
    pub damage: i32,
    pub activesound: SfxEnum,
    pub flags: u32,
    pub raisestate: StateNum,
}
"#;

pub const MOBJ_INFO_ARRAY_STR: &str = r#"
const NUM_CATEGORIES: usize = MapObjKind::NUMMOBJTYPES as usize;
pub const MOBJINFO: [MapObjInfo; NUM_CATEGORIES] = ["#;

pub const SPRITE_NAME_ARRAY_STR: &str = r#"
const NUMSPRITES: usize = SpriteNum::NUMSPRITES as usize;
pub const SPRNAMES: [&str; NUMSPRITES] = ["#;

pub const ARRAY_END_STR: &str = r#"
];"#;

pub const SPRITE_ENUM_HEADER: &str = r#"
#[repr(u16)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[allow(non_camel_case_types, dead_code)]
pub enum SpriteNum {"#;

pub const SPRITE_ENUM_END: &str = r#"
    NUMSPRITES,
}
impl Default for SpriteNum {
    fn default() -> Self {
        SpriteNum::SPR_TROO
    }
}"#;

pub const STATE_ENUM_HEADER: &str = r#"
#[repr(u16)]
#[derive(Debug, Copy, Clone, PartialEq)]
#[allow(non_camel_case_types, dead_code)]
pub enum StateNum {"#;

pub const STATE_ENUM_END: &str = r#"
    NUMSTATES,
}
impl From<u16> for StateNum {
    fn from(w: u16) -> Self {
        if w >= StateNum::NUMSTATES as u16 {
            panic!("{} is not a variant of StateNum", w);
        }
        unsafe { std::mem::transmute(w) }
    }
}"#;
