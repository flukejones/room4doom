pub const FILE_HEADER_STR: &str = r#"//! THIS FILE IS GENERATED WITH MULTIGEN
//! @generated
//! Contains all Map Object info, States and State numbers, and Sprite names/indexing.
#![allow(clippy::upper_case_acronyms, clippy::derivable_impls)]
"#;

pub const MOBJ_INFO_HEADER_STR: &str = r#"
use crate::thing::MapObjFlag;
use sound_common::SfxName;
"#;

pub const CLIPPY_ALLOW: &str = "";

pub const MOBJ_INFO_TYPE_STR: &str = r#"
#[derive(Debug, Copy, Clone)]
pub struct MapObjInfo {
    pub doomednum: i32,
    pub spawnstate: StateNum,
    pub spawnhealth: i32,
    pub seestate: StateNum,
    pub seesound: SfxName,
    pub reactiontime: i32,
    pub attacksound: SfxName,
    pub painstate: StateNum,
    pub painchance: i32,
    pub painsound: SfxName,
    pub meleestate: StateNum,
    pub missilestate: StateNum,
    pub deathstate: StateNum,
    pub xdeathstate: StateNum,
    pub deathsound: SfxName,
    /// Raw speed value matching OG Doom: plain integer for monsters,
    /// `N * FRACUNIT` (N * 65536) for projectiles.
    pub speed: i32,
    pub radius: f32,
    pub height: f32,
    pub mass: i32,
    pub damage: i32,
    pub activesound: SfxName,
    pub flags: MapObjFlag,
    pub raisestate: StateNum,
}
"#;

pub const MOBJ_INFO_ARRAY_STR: &str = r#"
const NUM_CATEGORIES: usize = MapObjKind::Count as usize;
pub const MOBJINFO: [MapObjInfo; NUM_CATEGORIES] = ["#;

pub const SPRITE_NAME_ARRAY_STR: &str = r#"
const NUM_SPRNAMES: usize = SpriteNum::Count as usize;
pub const SPRNAMES: [&str; NUM_SPRNAMES] = [
"#;

pub const ARRAY_END_STR: &str = r#"
];
"#;

pub const SPRITE_ENUM_HEADER: &str = r#"
#[repr(u16)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[allow(non_camel_case_types, dead_code)]
pub enum SpriteNum {
"#;

pub const SPRITE_ENUM_END: &str = r#"Count,
}
impl Default for SpriteNum {
    fn default() -> Self {
        SpriteNum::TROO
    }
}"#;

pub const STATE_ENUM_HEADER: &str = r#"
#[repr(u16)]
#[derive(Debug, Copy, Clone, PartialEq)]
#[allow(non_camel_case_types, dead_code)]
pub enum StateNum {
"#;

pub const STATE_ENUM_END: &str = r#"Count,
}
impl From<u16> for StateNum {
    fn from(w: u16) -> Self {
        if w >= StateNum::Count as u16 {
            panic!("{} is not a variant of StateNum", w);
        }
        unsafe { std::mem::transmute(w) }
    }
}"#;

pub const STATE_ARRAY_STR: &str = r#"
/// State data without function pointers — safe for static arrays.
#[derive(Debug)]
pub struct StateData {
    /// Sprite to use
    pub sprite: SpriteNum,
    /// The frame within this sprite to show for the state
    pub frame: u32,
    /// How many tics this state takes. On nightmare it is shifted >> 1
    pub tics: i32,
    /// Action identifier — resolved to a function pointer per numeric type
    pub action: ActionId,
    /// The state that should come after this. Can be looped.
    pub next_state: StateNum,
    pub misc1: i32,
    pub misc2: i32,
}

const NUM_STATES: usize = StateNum::Count as usize;
pub static STATES: [StateData; NUM_STATES] = [
    // StateData { sprite, frame, tics, action, next_state, misc1, misc2 }"#;

pub const MKIND_ENUM_HEADER: &str = r#"
#[repr(u16)]
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[allow(non_camel_case_types, dead_code)]
pub enum MapObjKind {
"#;

pub const MKIND_ENUM_END: &str = r#"Count,
}
impl From<u16> for MapObjKind {
    fn from(i: u16) -> Self {
        if i >= MapObjKind::Count as u16 {
            panic!("{} is not a variant of MapObjKind", i);
        }
        unsafe { std::mem::transmute(i) }
    }
}
"#;
