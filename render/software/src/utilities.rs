use std::f32::consts::FRAC_PI_2;

use gameplay::{Angle, MapObject};
use glam::Vec2;

fn fov_delta(fov: f32, screen_width: f32, screen_height: f32) -> f32 {
    (screen_width / (screen_height / (fov * 0.82).tan())).atan() - fov
}

pub fn fov_adjusted(fov: f32, screen_width: f32, screen_height: f32) -> f32 {
    fov - fov_delta(fov, screen_width, screen_height)
}

pub fn player_dist_to_screen(fov: f32, screen_width_half: f32) -> f32 {
    screen_width_half / (fov / 2.0).tan()
}

pub fn screen_to_x_view(fov: f32, x: f32, screen_width_half: f32) -> f32 {
    ((screen_width_half - x) / player_dist_to_screen(fov, screen_width_half)).atan()
}

/// R_PointToDist
pub fn point_to_dist(x: f32, y: f32, to: Vec2) -> f32 {
    let mut dx = (x - to.x).abs();
    let mut dy = (y - to.y).abs();

    if dy > dx {
        std::mem::swap(&mut dx, &mut dy);
    }
    (dx.powi(2) + dy.powi(2)).sqrt()
}

// The viewangletox LUT as a funtion. Should maybe turn this in back in to a LUT
pub fn angle_to_screen(fov: f32, half_screen_width: f32, screen_width: f32, angle: Angle) -> f32 {
    let focal = player_dist_to_screen(fov, half_screen_width);
    let t = angle.tan() * focal;
    // The root cause of missing columns is this. It must be tipped a little so that two
    // values straddling a line may go one way or the other
    let t = (half_screen_width - t + 0.5).round();
    t.clamp(0.0, screen_width)
}

/// R_PointToAngle
pub fn vertex_angle_to_object(vertex: &Vec2, mobj: &MapObject) -> Angle {
    let x = vertex.x - mobj.xy.x;
    let y = vertex.y - mobj.xy.y;
    Angle::new(y.atan2(x))
}

/// R_ScaleFromGlobalAngle
// All should be in rads
pub fn scale_from_view_angle(
    visangle: Angle,
    rw_normalangle: Angle,
    rw_distance: f32,
    view_angle: Angle,
    screen_width: f32,
) -> f32 {
    let anglea = Angle::new(FRAC_PI_2 + (visangle - view_angle).rad()); // CORRECT
    let angleb = Angle::new(FRAC_PI_2 + (visangle - rw_normalangle).rad()); // CORRECT

    let sinea = anglea.sin(); // not correct?
    let sineb = angleb.sin();

    let projection = screen_width / 2.0; // / (FRAC_PI_4).tan();
    let num = projection * sineb;
    let den = rw_distance * sinea;

    num / den
}
