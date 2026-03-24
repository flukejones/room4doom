//! The gameplay crate is purely gameplay. It loads a level from the wad, all
//! definitions, and level state.
//!
//! The `Gameplay` is very self contained, such that it really only expects
//! input, the player thinkers to be run, and the MapObject thinkers to be run.
//! The owner of the `Gameplay` is then expected to get what is required to
//! display the results from the exposed public API.

// #![feature(const_fn_floating_point_arithmetic)]
#![allow(clippy::new_without_default)]

use std::f32::consts::TAU;

pub mod dirs;
mod doom_def;
pub(crate) mod env;
#[rustfmt::skip]
mod info;
pub(crate) mod bsp_trace;
mod lang;
mod level;
mod pic;
mod player;
mod player_sprite;
pub mod save;
pub(crate) mod sector_ext;
mod thing;
mod thinker;

pub use doom_def::{
    AmmoType, Card, DOOM_VERSION, GameAction, MAXPLAYERS, PowerType, TICRATE, WEAPON_INFO
};
pub use env::specials::{respawn_specials, spawn_specials, update_specials};
pub use env::teleport::teleport_move;
pub use info::{MapObjKind, SPRNAMES, STATES, StateNum};
pub use lang::english;
pub use level::LevelState;
pub use pic::{Button, ButtonWhere};
pub use player::{Player, PlayerCheat, PlayerState, PlayerStatus, WorldEndPlayerInfo};
pub use player_sprite::PspDef;
pub use sector_ext::SectorExt;
pub use thing::{MapObjFlag, MapObject};

pub fn radian_range(rad: f32) -> f32 {
    if rad < 0.0 {
        return rad + TAU;
    } else if rad >= TAU {
        return rad - TAU;
    }
    rad
}
