#![feature(const_fn_floating_point_arithmetic)]

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};

use angle::Angle;
use glam::Vec2;
pub(crate) mod angle;
pub mod d_main;
pub(crate) mod d_thinker;
pub(crate) mod doom_def;
pub(crate) mod entities;
pub(crate) mod errors;
pub(crate) mod flags;
pub mod game;
pub(crate) mod info;
pub mod input;
pub(crate) mod level;
pub mod map_data;
pub(crate) mod p_enemy;
pub(crate) mod p_lights;
pub(crate) mod p_local;
pub(crate) mod p_map;
pub(crate) mod p_map_object;
pub(crate) mod p_player_sprite;
pub(crate) mod p_spec;
pub(crate) mod player;
pub(crate) mod r_bsp;
pub(crate) mod r_segs;
pub(crate) mod renderer;
pub(crate) mod sounds;
pub(crate) mod tic_cmd;
pub(crate) mod timestep;

/// R_PointToDist
fn point_to_dist(x: f32, y: f32, to: Vec2) -> f32 {
    let mut dx = (x - to.x()).abs();
    let mut dy = (y - to.y()).abs();

    if dy > dx {
        let temp = dx;
        dx = dy;
        dy = temp;
    }

    let dist = (dx.powi(2) + dy.powi(2)).sqrt();
    dist
}

/// R_ScaleFromGlobalAngle
// All should be in rads
fn scale(
    visangle: Angle,
    rw_normalangle: Angle,
    rw_distance: f32,
    view_angle: Angle,
) -> f32 {
    static MAX_SCALEFACTOR: f32 = 64.0;
    static MIN_SCALEFACTOR: f32 = 0.00390625;

    let anglea = Angle::new(FRAC_PI_2 + visangle.rad() - view_angle.rad()); // CORRECT
    let angleb = Angle::new(FRAC_PI_2 + visangle.rad() - rw_normalangle.rad()); // CORRECT

    let sinea = anglea.sin(); // not correct?
    let sineb = angleb.sin();

    //            projection
    //m_iDistancePlayerToScreen = m_HalfScreenWidth / HalfFOV.GetTanValue();
    let p = 160.0 / (FRAC_PI_4).tan();
    let num = p * sineb; // oof a bit
    let den = rw_distance * sinea;

    let mut scale = num / den;

    if scale > MAX_SCALEFACTOR {
        scale = MAX_SCALEFACTOR;
    } else if MIN_SCALEFACTOR > scale {
        scale = MIN_SCALEFACTOR;
    }
    scale
}
