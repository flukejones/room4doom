//! Everything related to gameplay lives here. This is stuff like:
//! - world movers
//! - monster data and actions
//! - shooty stuff and damage
//! - stuff like that...

pub mod d_thinker; // required by level data
pub mod enemy; // required by states
pub mod map_object; // info, level data, game, bsp
pub mod player;
pub mod player_sprite; // info/states
pub mod specials; // game
pub mod utilities; // level data node // many places

mod ceiling;
mod doors;
mod floor;
mod interaction;
mod lights;
mod movement;
mod platforms;
mod switch;
