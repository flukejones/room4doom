use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};

use angle::Angle;
use player::Player;

pub mod angle;
pub mod bsp;
pub mod doom_def;
pub mod entities;
pub mod flags;
pub mod info;
pub mod local;
pub mod map_object;
pub mod player;
pub mod segs;
pub mod sounds;
pub mod thinker;

/// R_PointToDist
fn point_to_dist(x: f32, y: f32, object: &Player) -> f32 {
    let mut dx = (x - object.xy.x()).abs();
    let mut dy = (y - object.xy.y()).abs();

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
