mod angle;
mod doom_f32;
mod fixed_point;
mod intercept;
mod macros;
pub use doom_f32::*;
mod doom_f32_test;

mod trig;
pub use fixed_point::*;

use std::f32::consts::PI;

pub use angle::*;

pub use intercept::*;

const FRACBITS: i32 = 16;
const FRACUNIT: f32 = (1 << FRACBITS) as f32;
pub const FRACUNIT_DIV4: f32 = FRACUNIT / 4.0;

/// Convert a Doom `fixed_t` fixed-point float to `DoomF32`
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
pub fn circle_seg_collide(
    c_origin_x: DoomF32,
    c_origin_y: DoomF32,
    c_radius: DoomF32,
    s_start_x: DoomF32,
    s_start_y: DoomF32,
    s_end_x: DoomF32,
    s_end_y: DoomF32,
) -> bool {
    let lc_x = c_origin_x - s_start_x;
    let lc_y = c_origin_y - s_start_y;
    let d_x = s_end_x - s_start_x;
    let d_y = s_end_y - s_start_y;
    let (p_x, p_y) = project_vec2d(lc_x, lc_y, d_x, d_y);
    let nearest_x = s_start_x + p_x;
    let nearest_y = s_start_y + p_y;

    if circle_point_intersect(c_origin_x, c_origin_y, c_radius, nearest_x, nearest_y)
        && length(p_x, p_y) < length(d_x, d_y)
        && dot(p_x, p_y, d_x, d_y) > doom_f32!(f32::EPSILON)
    {
        return true;
    }
    false
}

#[inline]
pub fn circle_line_collide(
    c_origin_x: DoomF32,
    c_origin_y: DoomF32,
    c_radius: DoomF32,
    l_start_x: DoomF32,
    l_start_y: DoomF32,
    l_end_x: DoomF32,
    l_end_y: DoomF32,
) -> bool {
    let lc_x = c_origin_x - l_start_x;
    let lc_y = c_origin_y - l_start_y;
    let (p_x, p_y) = project_vec2d(lc_x, lc_y, l_end_x - l_start_x, l_end_y - l_start_y);
    let nearest_x = l_start_x + p_x;
    let nearest_y = l_start_y + p_y;

    circle_point_intersect(c_origin_x, c_origin_y, c_radius, nearest_x, nearest_y)
}

#[inline]
pub fn circle_line_collide_xy(
    c_origin_x: DoomF32,
    c_origin_y: DoomF32,
    c_radius: DoomF32,
    l_start_x: DoomF32,
    l_start_y: DoomF32,
    l_end_x: DoomF32,
    l_end_y: DoomF32,
) -> bool {
    circle_line_collide(
        c_origin_x, c_origin_y, c_radius, l_start_x, l_start_y, l_end_x, l_end_y,
    )
}

#[inline]
pub fn circle_seg_collide_xy(
    c_origin_x: DoomF32,
    c_origin_y: DoomF32,
    c_radius: DoomF32,
    s_start_x: DoomF32,
    s_start_y: DoomF32,
    s_end_x: DoomF32,
    s_end_y: DoomF32,
) -> bool {
    circle_seg_collide(
        c_origin_x, c_origin_y, c_radius, s_start_x, s_start_y, s_end_x, s_end_y,
    )
}

/// Do a 2d XY projection.
#[inline]
fn project_vec2d(
    this_x: DoomF32,
    this_y: DoomF32,
    onto_x: DoomF32,
    onto_y: DoomF32,
) -> (DoomF32, DoomF32) {
    let d = dot(onto_x, onto_y, onto_x, onto_y);
    if d > ZERO {
        let dp = dot(this_x, this_y, onto_x, onto_y);
        return (onto_x * (dp / d), onto_y * (dp / d));
    }
    (onto_x, onto_y)
}

/// Do a 2d XY intersection.
#[inline]
pub fn circle_point_intersect(
    origin_x: DoomF32,
    origin_y: DoomF32,
    radius: DoomF32,
    point_x: DoomF32,
    point_y: DoomF32,
) -> bool {
    let dist_x = point_x - origin_x;
    let dist_y = point_y - origin_y;
    let len = length(dist_x, dist_y);
    if len < radius {
        return true;
    }
    false
}

#[inline]
pub fn circle_circle_intersect(
    origin_x: DoomF32,
    origin_y: DoomF32,
    origin_radius: DoomF32,
    point_x: DoomF32,
    point_y: DoomF32,
    point_radius: DoomF32,
) -> bool {
    let dist_x = point_x - origin_x;
    let dist_y = point_y - origin_y;
    let len = length(dist_x, dist_y);
    if len < origin_radius + point_radius {
        return true;
    }
    false
}

#[inline]
pub fn circle_circle_intersect_xy(
    origin_x: DoomF32,
    origin_y: DoomF32,
    origin_radius: DoomF32,
    point_x: DoomF32,
    point_y: DoomF32,
    point_radius: DoomF32,
) -> bool {
    let len = distance(origin_x, origin_y, point_x, point_y);
    len < origin_radius + point_radius
}

#[inline]
pub fn distance(x1: DoomF32, y1: DoomF32, x2: DoomF32, y2: DoomF32) -> DoomF32 {
    let dx = x2 - x1;
    let dy = y2 - y1;
    (dx * dx + dy * dy).sqrt()
}

#[inline]
pub fn distance_squared(x1: DoomF32, y1: DoomF32, x2: DoomF32, y2: DoomF32) -> DoomF32 {
    let dx = x2 - x1;
    let dy = y2 - y1;
    dx * dx + dy * dy
}

#[inline]
pub fn length(x: DoomF32, y: DoomF32) -> DoomF32 {
    (x * x + y * y).sqrt()
}

#[inline]
pub fn length_squared(x: DoomF32, y: DoomF32) -> DoomF32 {
    x * x + y * y
}

#[inline]
pub fn dot(x1: DoomF32, y1: DoomF32, x2: DoomF32, y2: DoomF32) -> DoomF32 {
    x1 * x2 + y1 * y2
}

#[inline]
pub fn normalize(x: DoomF32, y: DoomF32) -> (DoomF32, DoomF32) {
    let len = length(x, y);
    if len > ZERO {
        (x / len, y / len)
    } else {
        (ZERO, ZERO)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    fn assert_doom_f32_eq(doom_val: DoomF32, f: f32) {
        let diff = (to_f32(doom_val) - f).abs();
        assert!(
            diff < EPSILON,
            "DoomF32 {} != f32 {}, diff: {}",
            to_f32(doom_val),
            f,
            diff
        );
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
    fn test_doom_f32_fixed_point_on() {
        #[cfg(feature = "fixed_point")]
        {
            let a = doom_f32!(3.5);
            let b = doom_f32!(2.0);
            assert_doom_f32_eq(a + b, 5.5);
            assert_doom_f32_eq(a - b, 1.5);
            assert_doom_f32_eq(a * b, 7.0);
            assert_doom_f32_eq(a / b, 1.75);
        }
    }

    #[test]
    fn test_doom_f32_fixed_point_off() {
        #[cfg(not(feature = "fixed_point"))]
        {
            let a = doom_f32!(3.5);
            let b = doom_f32!(2.0);
            assert_doom_f32_eq(a + b, 5.5);
            assert_doom_f32_eq(a - b, 1.5);
            assert_doom_f32_eq(a * b, 7.0);
            assert_doom_f32_eq(a / b, 1.75);
        }
    }

    #[test]
    fn test_circle_point_intersect() {
        let radius = doom_f32!(1.0);
        let origin_x = doom_f32!(3.0);
        let origin_y = doom_f32!(5.0);

        let point_x = doom_f32!(2.5);
        let point_y = doom_f32!(4.5);
        assert!(circle_point_intersect(
            origin_x, origin_y, radius, point_x, point_y
        ));

        let point_x = doom_f32!(3.5);
        let point_y = doom_f32!(5.5);
        assert!(circle_point_intersect(
            origin_x, origin_y, radius, point_x, point_y
        ));

        let point_x = doom_f32!(2.0);
        let point_y = doom_f32!(4.0);
        assert!(!circle_point_intersect(
            origin_x, origin_y, radius, point_x, point_y
        ));
    }

    #[test]
    fn test_distance_functions() {
        let x1 = doom_f32!(1.0);
        let y1 = doom_f32!(2.0);
        let x2 = doom_f32!(4.0);
        let y2 = doom_f32!(6.0);

        let dist = distance(x1, y1, x2, y2);
        assert_doom_f32_eq(dist, 5.0);

        let dist_sq = distance_squared(x1, y1, x2, y2);
        assert_doom_f32_eq(dist_sq, 25.0);
    }

    #[test]
    fn test_vector_operations() {
        let x1 = doom_f32!(3.0);
        let y1 = doom_f32!(4.0);
        let x2 = doom_f32!(1.0);
        let y2 = doom_f32!(2.0);

        let dot_result = dot(x1, y1, x2, y2);
        assert_doom_f32_eq(dot_result, 11.0);

        let len = length(x1, y1);
        assert_doom_f32_eq(len, 5.0);

        let len_sq = length_squared(x1, y1);
        assert_doom_f32_eq(len_sq, 25.0);

        let (norm_x, norm_y) = normalize(x1, y1);
        assert_doom_f32_eq(norm_x, 0.6);
        assert_doom_f32_eq(norm_y, 0.8);

        let (zero_x, zero_y) = normalize(ZERO, ZERO);
        assert_doom_f32_eq(zero_x, 0.0);
        assert_doom_f32_eq(zero_y, 0.0);
    }

    #[test]
    fn test_circle_circle_intersect() {
        let origin1_x = doom_f32!(0.0);
        let origin1_y = doom_f32!(0.0);
        let radius1 = doom_f32!(2.0);

        let origin2_x = doom_f32!(1.0);
        let origin2_y = doom_f32!(1.0);
        let radius2 = doom_f32!(2.0);

        assert!(circle_circle_intersect(
            origin1_x, origin1_y, radius1, origin2_x, origin2_y, radius2
        ));

        let origin2_x = doom_f32!(5.0);
        let origin2_y = doom_f32!(5.0);
        assert!(!circle_circle_intersect(
            origin1_x, origin1_y, radius1, origin2_x, origin2_y, radius2
        ));
    }

    #[test]
    fn test_circle_line_collisions() {
        let c_x = doom_f32!(5.0);
        let c_y = doom_f32!(5.0);
        let c_radius = doom_f32!(2.0);

        let l_start_x = doom_f32!(0.0);
        let l_start_y = doom_f32!(5.0);
        let l_end_x = doom_f32!(10.0);
        let l_end_y = doom_f32!(5.0);

        assert!(circle_line_collide(
            c_x, c_y, c_radius, l_start_x, l_start_y, l_end_x, l_end_y
        ));

        let l_start_y = doom_f32!(0.0);
        let l_end_y = doom_f32!(0.0);
        assert!(!circle_line_collide(
            c_x, c_y, c_radius, l_start_x, l_start_y, l_end_x, l_end_y
        ));
    }

    #[test]
    fn test_circle_segment_collisions() {
        let c_x = doom_f32!(5.0);
        let c_y = doom_f32!(5.0);
        let c_radius = doom_f32!(1.5);

        let s_start_x = doom_f32!(4.0);
        let s_start_y = doom_f32!(5.0);
        let s_end_x = doom_f32!(6.0);
        let s_end_y = doom_f32!(5.0);

        assert!(circle_seg_collide(
            c_x, c_y, c_radius, s_start_x, s_start_y, s_end_x, s_end_y
        ));

        let s_start_x = doom_f32!(0.0);
        let s_start_y = doom_f32!(0.0);
        let s_end_x = doom_f32!(1.0);
        let s_end_y = doom_f32!(1.0);
        assert!(!circle_seg_collide(
            c_x, c_y, c_radius, s_start_x, s_start_y, s_end_x, s_end_y
        ));
    }

    #[test]
    fn test_project_vec2d() {
        let this_x = doom_f32!(3.0);
        let this_y = doom_f32!(4.0);
        let onto_x = doom_f32!(1.0);
        let onto_y = doom_f32!(0.0);

        let (proj_x, proj_y) = project_vec2d(this_x, this_y, onto_x, onto_y);
        assert_doom_f32_eq(proj_x, 3.0);
        assert_doom_f32_eq(proj_y, 0.0);

        let onto_x = doom_f32!(0.0);
        let onto_y = doom_f32!(0.0);
        let (proj_x, proj_y) = project_vec2d(this_x, this_y, onto_x, onto_y);
        assert_doom_f32_eq(proj_x, 0.0);
        assert_doom_f32_eq(proj_y, 0.0);
    }

    #[test]
    fn test_fixed_point_conversion() {
        let f_val = 3.14159;
        let fixed = float_to_fixed(f_val);
        let back_to_float = fixed_to_float(fixed);
        assert!((back_to_float - f_val).abs() < 0.001);

        let negative_val = -2.5;
        let fixed_neg = float_to_fixed(negative_val);
        let back_neg = fixed_to_float(fixed_neg);
        assert!((back_neg - negative_val).abs() < 0.001);
    }

    #[test]
    fn test_random_functions() {
        let r1 = m_random();
        let r2 = m_random();
        assert!(r1 >= 0 && r1 < 256);
        assert!(r2 >= 0 && r2 < 256);

        let p1 = p_random();
        let p2 = p_random();
        assert!(p1 >= 0 && p1 < 256);
        assert!(p2 >= 0 && p2 < 256);

        let sub_r = p_subrandom();
        assert!(sub_r >= -255 && sub_r <= 255);

        m_clear_random();
    }

    #[test]
    fn test_doom_f32_arithmetic_precision() {
        let a = doom_f32!(0.1);
        let b = doom_f32!(0.2);
        let c = a + b;

        #[cfg(not(feature = "fixed_point"))]
        {
            assert_doom_f32_eq(c, 0.3);
        }

        #[cfg(feature = "fixed_point")]
        {
            let result = to_f32(c);
            assert!((result - 0.3).abs() < 0.01, "Expected ~0.3, got {}", result);
        }
    }

    #[test]
    fn test_doom_f32_constants() {
        assert_doom_f32_eq(ZERO, 0.0);
        assert_doom_f32_eq(ONE, 1.0);
        assert_doom_f32_eq(NEG_ONE, -1.0);

        assert!(to_f32(MAX) > 1000.0);
        assert!(to_f32(MIN) < -1000.0);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn convert_bam_to_rad() {
        let ang45: u32 = 0x20000000;
        let ang90: u32 = 0x40000000;
        let ang180: u32 = 0x80000000;
        let one: u32 = 1 << 26;

        assert_eq!(bam_to_radian(ang45), FRAC_PI_4);
        assert_eq!(bam_to_radian(ang90), FRAC_PI_2);
        assert_eq!(bam_to_radian(ang180), PI);
        let result = to_f32(doom_f32!(bam_to_radian(one))).to_degrees();
        assert!(
            (result - 5.625).abs() < 0.01,
            "Expected ~5.625, got {}",
            result
        );
    }
}
