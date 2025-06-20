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

/// Converts a view-relative angle to screen X coordinate
///
/// This implements the original Doom viewangletox LUT functionality.
/// The algorithm matches the C code:
/// ```c
/// focallength = FixedDiv(centerxfrac, finetangent[FINEANGLES / 4 + FieldOfView / 2]);
/// for (i = 0; i < FINEANGLES / 2; i++) {
///     int t;
///     int limit = finetangent[FINEANGLES / 4 + FieldOfView / 2];
///     if (finetangent[i] > limit)
///         t = -1;
///     else if (finetangent[i] < -limit)
///         t = viewwidth + 1;
///     else {
///         t = FixedMul(finetangent[i], focallength);
///         t = (centerxfrac - t + FRACUNIT - 1) >> FRACBITS;
///         if (t < -1) t = -1;
///         else if (t > viewwidth + 1) t = viewwidth + 1;
///     }
///     viewangletox[i] = t;
/// }
/// ```
///
/// # Arguments
/// * `focal_len` - Focal length (projection distance)
/// * `half_fov` - Half field of view in radians
/// * `half_screen_width` - Half screen width in pixels
/// * `screen_width` - Full screen width in pixels
/// * `angle` - View-relative angle (0 = straight ahead, + = left, - = right)
///
/// # Returns
/// Screen X coordinate (-1 to screen_width+1)
pub fn angle_to_screen(
    focal_len: f32,
    half_fov: f32,
    half_screen_width: f32,
    screen_width: f32,
    angle: Angle,
) -> f32 {
    let limit_angle = Angle::new(half_fov);
    let limit = limit_angle.tan();
    let tan_angle = angle.tan();

    if tan_angle > limit {
        -1.0
    } else if tan_angle < -limit {
        screen_width + 1.0
    } else {
        let t = tan_angle * focal_len;
        let t = half_screen_width - t + 0.99998474;
        t.floor().clamp(-1.0, screen_width + 1.0)
    }
}

/// R_PointToAngle
pub fn vertex_angle_to_object(vertex: Vec2, mobj: &MapObject) -> Angle {
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

    #[test]
    fn test_angle_to_screen_3440x1440() {
        let screen_width = 3440.0;
        let half_screen_width = screen_width / 2.0;
        let fov = 90.0_f32.to_radians();
        let half_fov = fov / 2.0;
        let focal_len = projection(fov, half_screen_width);

        let center_angle = Angle::new(0.0);
        let center_x = angle_to_screen(
            focal_len,
            half_fov,
            half_screen_width,
            screen_width,
            center_angle,
        );

        // Due to f32 precision limits with large numbers, center may be off by 1 pixel
        assert!((center_x - half_screen_width.floor()).abs() <= 1.0);

        const FINEANGLES: usize = 8192;

        for i in 0..FINEANGLES / 2 {
            let view_angle_rad = (i as f32) * std::f32::consts::PI / (FINEANGLES / 2) as f32
                - std::f32::consts::FRAC_PI_2;
            let angle = Angle::new(view_angle_rad);
            let screen_x =
                angle_to_screen(focal_len, half_fov, half_screen_width, screen_width, angle);

            assert!(screen_x >= -1.0 && screen_x <= screen_width + 1.0);
        }

        let left_edge = Angle::new(half_fov);
        let left_x = angle_to_screen(
            focal_len,
            half_fov,
            half_screen_width,
            screen_width,
            left_edge,
        );
        assert!(left_x <= 1.0);

        let right_edge = Angle::new(-half_fov);
        let right_x = angle_to_screen(
            focal_len,
            half_fov,
            half_screen_width,
            screen_width,
            right_edge,
        );
        assert!(right_x >= screen_width - 1.0);
    }
}
