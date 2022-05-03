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
}"#;

pub const MOBJ_INFO_ARRAY_STR: &str = r#"
const NUM_CATEGORIES: usize = MapObjKind::NUMMOBJTYPES as usize;
pub const MOBJINFO: [MapObjInfo; NUM_CATEGORIES] = ["#;

pub const MOBJ_INFO_ARRAY_END_STR: &str = r#"
];"#;
