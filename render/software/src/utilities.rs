use std::f32::consts::FRAC_PI_2;

use gameplay::{Angle, MapObject};
use glam::Vec2;

const ZERO_POINT_THREE: f32 = 0.0052359877;
const OG_RATIO: f32 = 320. / 200.;

/// Find a new fov for the width of buffer proportional to the OG Doom height
pub fn corrected_fov_for_height(fov: f32, width: f32, height: f32) -> f32 {
    let v_dist = height / 2.0 / (fov * 0.82 / 2.0).tan();
    2.0 * (width / 2.0 / v_dist).atan() - ZERO_POINT_THREE
}

/// A scaling factor generally applied to the sprite rendering to get the
/// height proportions right
pub fn y_scale(fov: f32, buf_width: f32, buf_height: f32) -> f32 {
    // Find the canonical FOV of OG Doom.
    // TODO: This needs to be inversely proportional to the actual FOV so
    // that custom fov can be used
    // let v_dist = 200.0 / (fov * 0.82 / 2.0).tan();
    // let og_fov = 2.0 * (320.0 / v_dist).atan() - 0.3f32.to_radians();// ==
    // 100degrees
    let og_fov = 100f32.to_radians();
    let fov_ratio = og_fov / fov;
    let wide_ratio = buf_height / buf_width * OG_RATIO;
    (fov / 2.0 * wide_ratio / fov_ratio).tan()
}

pub fn projection(fov: f32, screen_width_half: f32) -> f32 {
    screen_width_half / Angle::new(fov / 2.0).tan()
}

/// Used to build a table for drawing process. The table cuts out a huge amount
/// of math
pub fn screen_to_angle(fov: f32, x: f32, screen_width_half: f32) -> f32 {
    ((screen_width_half - x) / projection(fov, screen_width_half)).atan()
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
// The out value if floored and clamped to the screen width min/max.
pub fn angle_to_screen(focal: f32, half_screen_width: f32, screen_width: f32, angle: Angle) -> f32 {
    let t = angle.tan() * focal;
    // The root cause of missing columns is this. It must be tipped a little so that
    // two values straddling a line may go one way or the other
    let t = half_screen_width - t + 0.99998474;
    t.floor().clamp(-1.0, screen_width + 1.0)
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
    screen_width_half: f32,
) -> f32 {
    let anglea: Angle = Angle::new(FRAC_PI_2 + (visangle.sub_other(view_angle)).rad());
    let angleb: Angle = Angle::new(FRAC_PI_2 + (visangle.sub_other(rw_normalangle)).rad());
    let projection: f32 = screen_width_half;
    let num: f32 = projection * angleb.sin();
    let den: f32 = rw_distance * anglea.sin();

    // return num / den;

    const MIN_DEN: f32 = 0.0001;
    if den.abs() < MIN_DEN {
        if num > 0.0 { 64.0 } else { -64.0 }
    } else {
        (num / den).clamp(-180.0, 180.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gameplay::Angle;

    #[test]
    fn test_perpendicular_segment_edge_cases() {
        let screen_width_half = 160.0;
        let view_angle = Angle::new(0.0);
        let rw_normalangle = Angle::new(FRAC_PI_2);
        let visangle = Angle::new(0.0);
        let rw_distance = 1.0;

        let scale = scale_from_view_angle(
            visangle,
            rw_normalangle,
            rw_distance,
            view_angle,
            screen_width_half,
        );
        assert!(scale.is_finite());
        assert!(scale.abs() <= 64.0);
    }

    #[test]
    fn test_zero_distance() {
        let screen_width_half = 160.0;
        let view_angle = Angle::new(0.0);
        let rw_normalangle = Angle::new(0.0);
        let visangle = Angle::new(0.0);
        let rw_distance = 0.0;

        let scale = scale_from_view_angle(
            visangle,
            rw_normalangle,
            rw_distance,
            view_angle,
            screen_width_half,
        );
        assert!(scale.is_finite());
        assert!(scale.abs() <= 64.0);
    }

    #[test]
    fn test_angle_bounds() {
        let screen_width_half = 160.0;
        let view_angle = Angle::new(0.0);
        let rw_normalangle = Angle::new(FRAC_PI_2);
        let visangle = Angle::new(FRAC_PI_2);
        let rw_distance = 0.00001;

        let scale = scale_from_view_angle(
            visangle,
            rw_normalangle,
            rw_distance,
            view_angle,
            screen_width_half,
        );
        assert!(scale.is_finite());
        assert!(scale.abs() <= 64.0);
    }
}
