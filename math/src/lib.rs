mod angle;
mod fixed_point;
mod fixed_vec2;
mod intercept;
mod macros;

mod trig;
pub use fixed_point::*;
pub use fixed_vec2::*;

use std::f32::consts::PI;

pub use angle::*;
use glam::Vec2;
pub use intercept::*;

const FRACBITS: i32 = 16;
const FRACUNIT: f32 = (1 << FRACBITS) as f32;
pub const FRACUNIT_DIV4: f32 = FRACUNIT / 4.0;

/// Convert a Doom `fixed_t` fixed-point float to `f32`
pub const fn fixed_to_float(value: i32) -> f32 {
    value as f32 / FRACUNIT
}

pub const fn float_to_fixed(value: f32) -> i32 {
    (value * FRACUNIT) as i32
}

const DEG_TO_RAD: f32 = PI / 180.0;

/// Convert a BAM (Binary Angle Measure) to radians
#[inline]
pub const fn bam_to_radian(value: u32) -> f32 {
    (value as f32 * 8.381_903e-8) * DEG_TO_RAD
}

static mut RNDINDEX: usize = 0;
static mut PRNDINDEX: usize = 0;

pub const RNDTABLE: [i32; 256] = [
    0, 8, 109, 220, 222, 241, 149, 107, 75, 248, 254, 140, 16, 66, 74, 21, 211, 47, 80, 242, 154,
    27, 205, 128, 161, 89, 77, 36, 95, 110, 85, 48, 212, 140, 211, 249, 22, 79, 200, 50, 28, 188,
    52, 140, 202, 120, 68, 145, 62, 70, 184, 190, 91, 197, 152, 224, 149, 104, 25, 178, 252, 182,
    202, 182, 141, 197, 4, 81, 181, 242, 145, 42, 39, 227, 156, 198, 225, 193, 219, 93, 122, 175,
    249, 0, 175, 143, 70, 239, 46, 246, 163, 53, 163, 109, 168, 135, 2, 235, 25, 92, 20, 145, 138,
    77, 69, 166, 78, 176, 173, 212, 166, 113, 94, 161, 41, 50, 239, 49, 111, 164, 70, 60, 2, 37,
    171, 75, 136, 156, 11, 56, 42, 146, 138, 229, 73, 146, 77, 61, 98, 196, 135, 106, 63, 197, 195,
    86, 96, 203, 113, 101, 170, 247, 181, 113, 80, 250, 108, 7, 255, 237, 129, 226, 79, 107, 112,
    166, 103, 241, 24, 223, 239, 120, 198, 58, 60, 82, 128, 3, 184, 66, 143, 224, 145, 224, 81,
    206, 163, 45, 63, 90, 168, 114, 59, 33, 159, 95, 28, 139, 123, 98, 125, 196, 15, 70, 194, 253,
    54, 14, 109, 226, 71, 17, 161, 93, 186, 87, 244, 138, 20, 52, 123, 251, 26, 36, 17, 46, 52,
    231, 232, 76, 31, 221, 84, 37, 216, 165, 212, 106, 197, 242, 98, 43, 39, 175, 254, 145, 190,
    84, 118, 222, 187, 136, 120, 163, 236, 249,
];

#[inline]
pub const fn p_random() -> i32 {
    unsafe {
        PRNDINDEX = (PRNDINDEX + 1) & 0xFF;
        RNDTABLE[PRNDINDEX]
    }
}

#[inline]
pub const fn m_random() -> i32 {
    unsafe {
        RNDINDEX = (RNDINDEX + 1) & 0xFF;
        RNDTABLE[RNDINDEX]
    }
}

#[inline]
pub const fn m_clear_random() {
    unsafe {
        // Not clearing this random as it's used only by screen wipe so far
        RNDINDEX = 0;
        PRNDINDEX = 0;
    }
}

#[inline]
pub const fn p_subrandom() -> i32 {
    let r = p_random();
    r - p_random()
}

//
// pub fn cross(lhs: &Vec2, rhs: &Vec2) -> f32 {
//     lhs.x * rhs.y - lhs.y * rhs.x
// }

/// True if the line segment from point1 to point2 penetrates the circle
#[inline]
pub fn circle_seg_collide(c_origin: Vec2, c_radius: f32, s_start: Vec2, s_end: Vec2) -> bool {
    let lc = c_origin - s_start;
    let d = s_end - s_start;
    let p = project_vec2d(lc, d);
    let nearest = s_start + p;

    if circle_point_intersect(c_origin, c_radius, nearest)
        && p.length() < d.length()
        && p.dot(d) > f32::EPSILON
    {
        // return Some((nearest - c_origin).normalize() * dist);
        return true;
    }
    false
}

#[inline]
pub fn circle_line_collide(c_origin: Vec2, c_radius: f32, l_start: Vec2, l_end: Vec2) -> bool {
    let lc = c_origin - l_start;
    let p = project_vec2d(lc, l_end - l_start);
    let nearest = l_start + p;

    circle_point_intersect(c_origin, c_radius, nearest)
}

#[inline]
pub fn circle_line_collide_xy(
    c_origin_x: f32,
    c_origin_y: f32,
    c_radius: f32,
    l_start_x: f32,
    l_start_y: f32,
    l_end_x: f32,
    l_end_y: f32,
) -> bool {
    let c_origin = Vec2::new(c_origin_x, c_origin_y);
    let l_start = Vec2::new(l_start_x, l_start_y);
    let l_end = Vec2::new(l_end_x, l_end_y);
    circle_line_collide(c_origin, c_radius, l_start, l_end)
}

#[inline]
pub fn circle_seg_collide_xy(
    c_origin_x: f32,
    c_origin_y: f32,
    c_radius: f32,
    s_start_x: f32,
    s_start_y: f32,
    s_end_x: f32,
    s_end_y: f32,
) -> bool {
    let c_origin = Vec2::new(c_origin_x, c_origin_y);
    let s_start = Vec2::new(s_start_x, s_start_y);
    let s_end = Vec2::new(s_end_x, s_end_y);
    circle_seg_collide(c_origin, c_radius, s_start, s_end)
}

/// Do a 2d XY projection. Zeroes out the Z component in the `Vec2` copy
/// internally.
#[inline]
fn project_vec2d(this: Vec2, onto: Vec2) -> Vec2 {
    let d = onto.dot(onto);
    if d > 0.0 {
        let dp = this.dot(onto);
        return onto * (dp / d);
    }
    onto
}

/// Do a 2d XY intersection. Zeroes out the Z component in the `Vec2` copy
/// internally.
#[inline]
pub fn circle_point_intersect(origin: Vec2, radius: f32, point: Vec2) -> bool {
    let dist = point - origin;
    let len = dist.length();
    if len < radius {
        return true; // Some(len - radius);
    }
    false
}

#[inline]
pub fn circle_circle_intersect(
    origin: Vec2,
    origin_radius: f32,
    point: Vec2,
    point_radius: f32,
) -> bool {
    let dist = point - origin;
    let len = dist.length();
    if len < origin_radius + point_radius {
        return true; // Some(len - radius);
    }
    false
}

#[inline]
pub fn circle_circle_intersect_xy(
    origin_x: f32,
    origin_y: f32,
    origin_radius: f32,
    point_x: f32,
    point_y: f32,
    point_radius: f32,
) -> bool {
    let len = distance(origin_x, origin_y, point_x, point_y);
    len < origin_radius + point_radius
}

#[inline]
pub fn distance(x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let dx = x2 - x1;
    let dy = y2 - y1;
    (dx * dx + dy * dy).sqrt()
}

#[inline]
pub fn distance_squared(x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let dx = x2 - x1;
    let dy = y2 - y1;
    dx * dx + dy * dy
}

#[inline]
pub fn length(x: f32, y: f32) -> f32 {
    (x * x + y * y).sqrt()
}

#[inline]
pub fn length_squared(x: f32, y: f32) -> f32 {
    x * x + y * y
}

#[inline]
pub fn dot(x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    x1 * x2 + y1 * y2
}

#[inline]
pub fn normalize(x: f32, y: f32) -> (f32, f32) {
    let len = length(x, y);
    if len > 0.0 {
        (x / len, y / len)
    } else {
        (0.0, 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec2;
    use std::f32::consts::{E, FRAC_PI_2, FRAC_PI_4, PI};

    const EPSILON: f32 = 0.01;

    fn assert_fixed_f32_eq(fixed: FixedPoint, f: f32) {
        let diff = (f32::from(fixed) - f).abs();
        assert!(
            diff < EPSILON,
            "Fixed {} != f32 {}, diff: {}",
            fixed,
            f,
            diff
        );
    }

    fn assert_fixed_vec2_eq(fixed: FixedVec2, f: Vec2) {
        assert_fixed_f32_eq(fixed.x, f.x);
        assert_fixed_f32_eq(fixed.y, f.y);
    }

    #[test]
    fn test_basic_arithmetic() {
        let fa = FixedPoint::from(3.5);
        let fb = FixedPoint::from(2.0);
        let a = 3.5f32;
        let b = 2.0f32;

        assert_fixed_f32_eq(fa + fb, a + b);
        assert_fixed_f32_eq(fa - fb, a - b);
        assert_fixed_f32_eq(fa * fb, a * b);
        assert_fixed_f32_eq(fa / fb, a / b);
    }

    #[test]
    fn test_vec2_basic_ops() {
        let fv1 = fvec2!(3.0, 4.0);
        let fv2 = fvec2!(1.0, 2.0);
        let v1 = Vec2::new(3.0, 4.0);
        let v2 = Vec2::new(1.0, 2.0);

        assert_fixed_vec2_eq(fv1 + fv2, v1 + v2);
        assert_fixed_vec2_eq(fv1 - fv2, v1 - v2);
        assert_fixed_vec2_eq(fv1 * fv2, v1 * v2);
        assert_fixed_vec2_eq(fv1 / fv2, v1 / v2);
    }

    #[test]
    fn test_scalar_ops() {
        let fv = fvec2!(3.0, 4.0);
        let fs = fixed!(2.0);
        let v = Vec2::new(3.0, 4.0);
        let s = 2.0f32;

        assert_fixed_vec2_eq(fv * fs, v * s);
        assert_fixed_vec2_eq(fv / fs, v / s);
        assert_fixed_vec2_eq(fs * fv, s * v);
    }

    #[test]
    fn test_dot_product() {
        let fv1 = fvec2!(3.0, 4.0);
        let fv2 = fvec2!(2.0, 1.0);
        let v1 = Vec2::new(3.0, 4.0);
        let v2 = Vec2::new(2.0, 1.0);

        assert_fixed_f32_eq(fv1.dot(fv2), v1.dot(v2));
    }

    #[test]
    fn test_length() {
        let fv = fvec2!(3.0, 4.0);
        let v = Vec2::new(3.0, 4.0);

        assert_fixed_f32_eq(fv.length(), v.length());
        assert_fixed_f32_eq(fv.length_squared(), v.length_squared());
    }

    #[test]
    fn test_normalize() {
        let fv = fvec2!(3.0, 4.0);
        let v = Vec2::new(3.0, 4.0);

        let fn_norm = fv.normalize();
        let v_norm = v.normalize();

        assert_fixed_vec2_eq(fn_norm, v_norm);
        assert!(fv.is_normalized() == false);
        assert!(fn_norm.is_normalized());
    }

    #[test]
    fn test_trig_functions() {
        let angles = [0.0, PI / 6.0, PI / 4.0, PI / 3.0, PI];

        for angle in angles.iter() {
            let f_angle = fixed!(*angle);

            assert_fixed_f32_eq(f_angle.sin(), angle.sin());
            assert_fixed_f32_eq(f_angle.cos(), angle.cos());
            assert_fixed_f32_eq(f_angle.tan(), angle.tan());
        }

        let pi_2 = fixed!(PI / 2.0);
        assert_fixed_f32_eq(pi_2.sin(), (PI / 2.0).sin());
        assert_fixed_f32_eq(pi_2.cos(), (PI / 2.0).cos());
    }

    #[test]
    fn test_distance() {
        let fv1 = fvec2!(1.0, 2.0);
        let fv2 = fvec2!(4.0, 6.0);
        let v1 = Vec2::new(1.0, 2.0);
        let v2 = Vec2::new(4.0, 6.0);

        assert_fixed_f32_eq(fv1.distance(fv2), v1.distance(v2));
        assert_fixed_f32_eq(fv1.distance_squared(fv2), v1.distance_squared(v2));
    }

    #[test]
    fn test_min_max_vec2() {
        let fv1 = fvec2!(1.0, 4.0);
        let fv2 = fvec2!(3.0, 2.0);
        let v1 = Vec2::new(1.0, 4.0);
        let v2 = Vec2::new(3.0, 2.0);

        assert_fixed_vec2_eq(fv1.min(fv2), v1.min(v2));
        assert_fixed_vec2_eq(fv1.max(fv2), v1.max(v2));
    }

    #[test]
    fn test_clamp_vec2() {
        let fv = fvec2!(5.0, -2.0);
        let fmin = fvec2!(0.0, 0.0);
        let fmax = fvec2!(3.0, 3.0);
        let v = Vec2::new(5.0, -2.0);
        let vmin = Vec2::new(0.0, 0.0);
        let vmax = Vec2::new(3.0, 3.0);

        assert_fixed_vec2_eq(fv.clamp(fmin, fmax), v.clamp(vmin, vmax));
    }

    #[test]
    fn test_abs_vec2() {
        let fv = fvec2!(-3.0, 4.0);
        let v = Vec2::new(-3.0, 4.0);

        assert_fixed_vec2_eq(fv.abs(), v.abs());
    }

    #[test]
    fn test_signum_vec2() {
        let fv = fvec2!(-3.0, 4.0);
        let v = Vec2::new(-3.0, 4.0);

        assert_fixed_vec2_eq(fv.signum(), v.signum());
    }

    #[test]
    fn test_floor_ceil_round_vec2() {
        let fv = fvec2!(3.7, -2.3);
        let v = Vec2::new(3.7, -2.3);

        assert_fixed_vec2_eq(fv.floor(), v.floor());
        assert_fixed_vec2_eq(fv.ceil(), v.ceil());
        assert_fixed_vec2_eq(fv.round(), v.round());
    }

    #[test]
    fn test_fract_vec2() {
        let fv = fvec2!(3.7, 2.3);
        let v = Vec2::new(3.7, 2.3);

        assert_fixed_vec2_eq(fv.fract(), v.fract());

        // Test positive fractional parts only since fract behavior differs for negatives
        let fv2 = fvec2!(0.25, 0.75);
        let v2 = Vec2::new(0.25, 0.75);
        assert_fixed_vec2_eq(fv2.fract(), v2.fract());
    }

    #[test]
    fn test_lerp_vec2() {
        let fv1 = fvec2!(0.0, 0.0);
        let fv2 = fvec2!(10.0, 20.0);
        let fs = fixed!(0.5);
        let v1 = Vec2::new(0.0, 0.0);
        let v2 = Vec2::new(10.0, 20.0);
        let s = 0.5f32;

        assert_fixed_vec2_eq(fv1.lerp(fv2, fs), v1.lerp(v2, s));
    }

    #[test]
    fn test_project_onto_vec2() {
        let fv1 = fvec2!(4.0, 2.0);
        let fv2 = fvec2!(3.0, 0.0);
        let v1 = Vec2::new(4.0, 2.0);
        let v2 = Vec2::new(3.0, 0.0);

        assert_fixed_vec2_eq(fv1.project_onto(fv2), v1.project_onto(v2));
    }

    #[test]
    fn test_reject_from_vec2() {
        let fv1 = fvec2!(4.0, 2.0);
        let fv2 = fvec2!(3.0, 0.0);
        let v1 = Vec2::new(4.0, 2.0);
        let v2 = Vec2::new(3.0, 0.0);

        assert_fixed_vec2_eq(fv1.reject_from(fv2), v1.reject_from(v2));
    }

    #[test]
    fn test_perp_vec2() {
        let fv = fvec2!(3.0, 4.0);
        let v = Vec2::new(3.0, 4.0);

        assert_fixed_vec2_eq(fv.perp(), v.perp());
    }

    #[test]
    fn test_perp_dot_vec2() {
        let fv1 = fvec2!(3.0, 4.0);
        let fv2 = fvec2!(2.0, 1.0);
        let v1 = Vec2::new(3.0, 4.0);
        let v2 = Vec2::new(2.0, 1.0);

        assert_fixed_f32_eq(fv1.perp_dot(fv2), v1.perp_dot(v2));
    }

    #[test]
    fn test_angle_between_vec2() {
        let fv1 = fvec2!(1.0, 0.0);
        let fv2 = fvec2!(0.0, 1.0);
        let v1 = Vec2::new(1.0, 0.0);
        let v2 = Vec2::new(0.0, 1.0);

        assert_fixed_f32_eq(fv1.angle_between(fv2), v1.angle_to(v2));
    }

    #[test]
    fn test_rotate_vec2() {
        let fv = fvec2!(1.0, 0.0);
        let rotated_f = fv.rotate(fconst!(FRAC_PI_2));

        // After rotating (1,0) by 90 degrees, we should get approximately (0,1)
        assert_fixed_f32_eq(rotated_f.x, 0.0);
        assert_fixed_f32_eq(rotated_f.y, 1.0);
    }

    #[test]
    fn test_from_angle_vec2() {
        let angle_f = fconst!(FRAC_PI_4);
        let angle_v = FRAC_PI_4;

        let fv = FixedVec2::from_angle(angle_f);
        let v = Vec2::from_angle(angle_v);

        assert_fixed_vec2_eq(fv, v);
    }

    #[test]
    fn test_to_angle_vec2() {
        let fv = fvec2!(1.0, 1.0);
        let v = Vec2::new(1.0, 1.0);

        assert_fixed_f32_eq(fv.to_angle(), v.to_angle());
    }

    #[test]
    fn test_reflect_vec2() {
        let fv = fvec2!(1.0, -1.0);
        let fn_normal = fvec2!(0.0, 1.0);
        let v = Vec2::new(1.0, -1.0);
        let v_normal = Vec2::new(0.0, 1.0);

        assert_fixed_vec2_eq(fv.reflect(fn_normal), v.reflect(v_normal));
    }

    #[test]
    fn test_midpoint_vec2() {
        let fv1 = fvec2!(2.0, 4.0);
        let fv2 = fvec2!(6.0, 8.0);
        let v1 = Vec2::new(2.0, 4.0);
        let v2 = Vec2::new(6.0, 8.0);

        assert_fixed_vec2_eq(fv1.midpoint(fv2), v1.midpoint(v2));
    }

    #[test]
    fn test_move_towards_vec2() {
        let fv1 = fvec2!(0.0, 0.0);
        let fv2 = fvec2!(10.0, 0.0);
        let fmax_dist = fixed!(5.0);
        let v1 = Vec2::new(0.0, 0.0);
        let v2 = Vec2::new(10.0, 0.0);
        let max_dist = 5.0f32;

        assert_fixed_vec2_eq(
            fv1.move_towards(fv2, fmax_dist),
            v1.move_towards(v2, max_dist),
        );
    }

    #[test]
    fn test_clamp_length_vec2() {
        let fv = fvec2!(10.0, 0.0);
        let fmin = fixed!(2.0);
        let fmax = fixed!(5.0);
        let v = Vec2::new(10.0, 0.0);
        let min = 2.0f32;
        let max = 5.0f32;

        assert_fixed_vec2_eq(fv.clamp_length(fmin, fmax), v.clamp_length(min, max));
    }

    #[test]
    fn test_element_operations_vec2() {
        let fv = fvec2!(3.0, 4.0);
        let v = Vec2::new(3.0, 4.0);

        assert_fixed_f32_eq(fv.min_element(), v.min_element());
        assert_fixed_f32_eq(fv.max_element(), v.max_element());
        assert_fixed_f32_eq(fv.element_sum(), v.element_sum());
        assert_fixed_f32_eq(fv.element_product(), v.element_product());
    }

    #[test]
    fn test_assignment_ops_vec2() {
        let mut fv = fvec2!(1.0, 2.0);
        let mut v = Vec2::new(1.0, 2.0);

        fv += fvec2!(2.0, 3.0);
        v += Vec2::new(2.0, 3.0);
        assert_fixed_vec2_eq(fv, v);

        fv -= fvec2!(1.0, 1.0);
        v -= Vec2::new(1.0, 1.0);
        assert_fixed_vec2_eq(fv, v);

        fv *= fvec2!(2.0, 0.5);
        v *= Vec2::new(2.0, 0.5);
        assert_fixed_vec2_eq(fv, v);

        fv /= fvec2!(2.0, 2.0);
        v /= Vec2::new(2.0, 2.0);
        assert_fixed_vec2_eq(fv, v);
    }

    #[test]
    fn test_scalar_assignment_ops_vec2() {
        let mut fv = fvec2!(2.0, 4.0);
        let mut v = Vec2::new(2.0, 4.0);

        fv *= fixed!(2.0);
        v *= 2.0;
        assert_fixed_vec2_eq(fv, v);

        fv /= fixed!(4.0);
        v /= 4.0;
        assert_fixed_vec2_eq(fv, v);
    }

    #[test]
    fn test_negation_vec2() {
        let fv = fvec2!(3.0, -4.0);
        let v = Vec2::new(3.0, -4.0);

        assert_fixed_vec2_eq(-fv, -v);
    }

    #[test]
    fn test_try_normalize_vec2() {
        let fv_zero = FixedVec2::ZERO;
        let v_zero = Vec2::ZERO;

        assert!(fv_zero.try_normalize().is_none());
        assert!(v_zero.try_normalize().is_none());

        let fv_normal = fvec2!(3.0, 4.0);
        let v_normal = Vec2::new(3.0, 4.0);

        let fn_opt = fv_normal.try_normalize().unwrap();
        let v_opt = v_normal.try_normalize().unwrap();

        assert_fixed_vec2_eq(fn_opt, v_opt);
    }

    #[test]
    fn test_conversions_vec2() {
        let glam_vec = Vec2::new(3.5, -2.7);
        let fixed_vec = FixedVec2::from(glam_vec);
        let back_to_glam = Vec2::from(fixed_vec);

        assert!((glam_vec.x - back_to_glam.x).abs() < EPSILON);
        assert!((glam_vec.y - back_to_glam.y).abs() < EPSILON);
    }

    #[test]
    fn test_indexing_vec2() {
        let fv = fvec2!(3.0, 4.0);
        let v = Vec2::new(3.0, 4.0);

        assert_fixed_f32_eq(fv[0], v[0]);
        assert_fixed_f32_eq(fv[1], v[1]);
    }

    #[test]
    fn test_math_functions() {
        let values = [0.5, 1.0, 2.0, E, PI];

        for val in values.iter() {
            let f_val = fixed!(*val);

            assert_fixed_f32_eq(f_val.sqrt(), val.sqrt());
            assert_fixed_f32_eq(f_val.abs(), val.abs());
            assert_fixed_f32_eq(f_val.floor(), val.floor());
            assert_fixed_f32_eq(f_val.ceil(), val.ceil());
            assert_fixed_f32_eq(f_val.round(), val.round());
            assert_fixed_f32_eq(f_val.fract(), val.fract());
            assert_fixed_f32_eq(f_val.signum(), val.signum());

            if *val > 0.0 {
                assert_fixed_f32_eq(f_val.ln(), val.ln());
                assert_fixed_f32_eq(f_val.log2(), val.log2());
                assert_fixed_f32_eq(f_val.log10(), val.log10());
            }

            assert_fixed_f32_eq(f_val.exp(), val.exp());
        }
    }

    #[test]
    fn test_constants() {
        assert_fixed_f32_eq(FixedPoint::PI, PI);
        assert_fixed_f32_eq(FixedPoint::E, E);
        assert_fixed_f32_eq(FixedPoint::FRAC_PI_2, std::f32::consts::FRAC_PI_2);
        assert_fixed_f32_eq(FixedPoint::FRAC_PI_4, std::f32::consts::FRAC_PI_4);
        assert_fixed_f32_eq(FixedPoint::SQRT_2, std::f32::consts::SQRT_2);
    }

    #[test]
    fn test_macros() {
        let f1 = fixed!(3.14);
        let f2 = FixedPoint::from(3.14);
        assert_eq!(f1, f2);

        let fv1 = fvec2!(2.0, 3.0);
        let fv2 = FixedVec2::new(fixed!(2.0), fixed!(3.0));
        assert_eq!(fv1, fv2);

        let pi = fconst!(PI);
        assert_eq!(pi, FixedPoint::PI);
    }

    // use crate::play::utilities::{circle_point_intersect,
    // circle_to_line_intercept_basic};

    // #[test]
    // fn circle_vec2_intersect() {
    //     let r = 1.0;
    //     let origin = Vec2::new(3.0, 5.0);
    //     let point = Vec2::new(2.5, 4.5);
    //     assert!(circle_point_intersect(origin, r, point).is_some());

    //     let point = Vec2::new(3.5, 5.5);
    //     assert!(circle_point_intersect(origin, r, point).is_some());

    //     let point = Vec2::new(2.0, 4.0);
    //     assert!(circle_point_intersect(origin, r, point).is_none());

    //     let point = Vec2::new(4.0, 7.0);
    //     let r = 2.5;
    //     assert!(circle_point_intersect(origin, r, point).is_some());
    // }

    // #[test]
    // fn test_circle_to_line_intercept_basic() {
    //     let r = 5.0;
    //     let origin = Vec2::new(5.0, 7.0);
    //     let point1 = Vec2::new(1.0, 3.0);
    //     let point2 = Vec2::new(7.0, 20.0);
    //     assert!(circle_to_line_intercept_basic(origin, r, point1,
    // point2).is_some());

    //     let r = 2.0;
    //     assert!(circle_to_line_intercept_basic(origin, r, point1,
    // point2).is_none()); }

    // #[test]
    // fn test_line_line_intersection() {
    //     let origin1 = Vec2::new(5.0, 1.0);
    //     let origin2 = Vec2::new(5.0, 10.0);
    //     let point1 = Vec2::new(1.0, 5.0);
    //     let point2 = Vec2::new(10.0, 5.0);
    //     assert!(line_line_intersection(origin1, origin2, point1, point2));

    //     let point1 = Vec2::new(5.0, 1.0);
    //     let point2 = Vec2::new(5.0, 10.0);
    //     assert!(line_line_intersection(origin1, origin2, point1, point2));

    //     let point1 = Vec2::new(4.0, 1.0);
    //     let point2 = Vec2::new(4.0, 10.0);
    //     assert!(!line_line_intersection(origin1, origin2, point1, point2));

    //     let origin1 = Vec2::new(1.0, 1.0);
    //     let origin2 = Vec2::new(10.0, 10.0);
    //     let point1 = Vec2::new(10.0, 1.0);
    //     let point2 = Vec2::new(1.0, 10.0);
    //     assert!(line_line_intersection(origin1, origin2, point1, point2));
    // }

    #[test]
    #[allow(clippy::float_cmp)]
    fn convert_bam_to_rad() {
        // DOOM constants
        let ang45: u32 = 0x20000000;
        let ang90: u32 = 0x40000000;
        let ang180: u32 = 0x80000000;

        let one: u32 = 1 << 26;

        assert_eq!(bam_to_radian(ang45), FRAC_PI_4);
        assert_eq!(bam_to_radian(ang90), FRAC_PI_2);
        assert_eq!(bam_to_radian(ang180), PI);
        assert_eq!(bam_to_radian(one).to_degrees(), 5.625);
    }
}
