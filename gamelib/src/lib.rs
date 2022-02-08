// #![feature(const_fn_floating_point_arithmetic)]

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};

use angle::Angle;
use glam::Vec2;
use std::fmt;
use std::ops::{Deref, DerefMut};
use std::ptr::NonNull;

pub mod angle;
pub mod d_main;
pub mod d_thinker;
pub mod doom_def;
pub mod errors;
pub mod flags;
pub mod game;
pub mod info;
pub mod input;
pub mod level_data;
pub mod p_doors;
pub mod p_enemy;
pub mod p_lights;
pub mod p_local;
pub mod p_map;
pub mod p_map_object;
pub mod p_map_util;
pub mod p_player_sprite;
pub mod p_spec;
pub mod p_switch;
pub mod player;
pub mod renderer;
pub mod shaders;
pub mod sounds;
pub mod tic_cmd;
pub mod timestep;

/// R_PointToDist
fn point_to_dist(x: f32, y: f32, to: Vec2) -> f32 {
    let mut dx = (x - to.x()).abs();
    let mut dy = (y - to.y()).abs();

    if dy > dx {
        std::mem::swap(&mut dx, &mut dy);
    }

    let dist = (dx.powi(2) + dy.powi(2)).sqrt();
    dist
}

/// R_ScaleFromGlobalAngle
// All should be in rads
fn scale_from_view_angle(
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

/// Functions purely as a safe fn wrapper around a `NonNull` because we know that
/// the Map structure is not going to change under us
pub struct DPtr<T> {
    p: NonNull<T>,
}

impl<T> DPtr<T> {
    fn new(t: &T) -> DPtr<T> {
        DPtr {
            p: NonNull::from(t),
        }
    }
}

// impl<T> DPtr<T> {
//     fn as_ptr(&self) -> *mut T {
//         self.p.as_ptr()
//     }
// }

impl<T> Clone for DPtr<T> {
    fn clone(&self) -> DPtr<T> {
        DPtr { p: self.p }
    }
}

impl<T> Deref for DPtr<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { self.p.as_ref() }
    }
}

impl<T> DerefMut for DPtr<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.p.as_mut() }
    }
}

impl<T: fmt::Debug> fmt::Debug for DPtr<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ptr->{:?}->{:#?}", self.p, unsafe { self.p.as_ref() })
    }
}

pub fn radian_range(rad: f32) -> f32 {
    if rad < 0.0 {
        return rad + 2.0 * PI;
    } else if rad >= 2.0 * PI {
        return rad - 2.0 * PI;
    }
    rad
}
