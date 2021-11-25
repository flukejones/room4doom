// #![feature(const_fn_floating_point_arithmetic)]

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};

use angle::Angle;
use glam::Vec2;
use std::fmt;
use std::ops::{Deref, DerefMut};
use std::ptr::NonNull;

pub(crate) mod angle;
pub mod d_main;
pub(crate) mod d_thinker;
pub(crate) mod doom_def;
pub(crate) mod errors;
pub(crate) mod flags;
pub mod game;
pub(crate) mod info;
pub mod input;
pub(crate) mod level_data;
pub(crate) mod p_enemy;
pub(crate) mod p_lights;
pub(crate) mod p_local;
pub(crate) mod p_map;
pub(crate) mod p_map_object;
pub(crate) mod p_map_util;
pub(crate) mod p_player_sprite;
pub(crate) mod p_spec;
pub(crate) mod player;
pub(crate) mod renderer;
pub(crate) mod shaders;
pub(crate) mod sounds;
pub(crate) mod tic_cmd;
pub(crate) mod timestep;

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
struct DPtr<T> {
    p: NonNull<T>,
}

impl<T> DPtr<T> {
    fn new(t: &T) -> DPtr<T> {
        DPtr {
            p: NonNull::from(t),
        }
    }
}

impl<T> DPtr<T> {
    fn as_ptr(&self) -> *mut T {
        self.p.as_ptr()
    }
}

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
