pub const MOBJ_INFO_HEADER_STR: &str = r#"
use crate::{
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
pub const SPRNAMES: [&str; NUMSPRITES] = [
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

pub const SPRITE_ENUM_END: &str = r#"NUMSPRITES,
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
pub enum StateNum {
"#;

pub const STATE_ENUM_END: &str = r#"NUMSTATES,
}
impl From<u16> for StateNum {
    fn from(w: u16) -> Self {
        if w >= StateNum::NUMSTATES as u16 {
            panic!("{} is not a variant of StateNum", w);
        }
        unsafe { std::mem::transmute(w) }
    }
}"#;

pub const STATE_ARRAY_STR: &str = r#"
use std::fmt;
use crate::{obj::enemy::*, player_sprite::*};
use super::{ActionF};

pub struct State {
    /// Sprite to use
    pub sprite: SpriteNum,
    /// The frame within this sprite to show for the state
    pub frame: u32,
    /// How many tics this state takes. On nightmare it is shifted >> 1
    pub tics: i32,
    // void (*action) (): i32,
    /// An action callback to run on this state
    pub action: ActionF,
    /// The state that should come after this. Can be looped.
    pub next_state: StateNum,
    /// Don't know, Doom seems to set all to zero
    pub misc1: i32,
    /// Don't know, Doom seems to set all to zero
    pub misc2: i32,
}

impl State {
    pub const fn new(
        sprite: SpriteNum,
        frame: u32,
        tics: i32,
        action: ActionF,
        next_state: StateNum,
        misc1: i32,
        misc2: i32,
    ) -> Self {
        Self {
            sprite,
            frame,
            tics,
            action,
            next_state,
            misc1,
            misc2,
        }
    }
}

impl Clone for State {
    fn clone(&self) -> Self {
        State::new(
            self.sprite,
            self.frame,
            self.tics,
            self.action.clone(),
            self.next_state,
            self.misc1,
            self.misc2,
        )
    }
}

impl fmt::Debug for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("State")
            .field("sprite", &self.sprite)
            .finish()
    }
}

const NUMSTATES: usize = StateNum::NUMSTATES as usize;
pub const STATES: [State; NUMSTATES] = ["#;

pub const MKIND_ENUM_HEADER: &str = r#"
#[repr(u16)]
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[allow(non_camel_case_types, dead_code)]
pub enum MapObjKind {
"#;

pub const MKIND_ENUM_END: &str = r#"NUMMOBJTYPES,
}

impl From<u16> for MapObjKind {
    fn from(i: u16) -> Self {
        if i >= MapObjKind::NUMMOBJTYPES as u16 {
            panic!("{} is not a variant of SfxEnum", i);
        }
        unsafe { std::mem::transmute(i) }
    }
}
"#;
