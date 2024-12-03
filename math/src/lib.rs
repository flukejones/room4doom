mod angle;
mod intercept;
mod trig;

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

#[cfg(test)]
mod tests {
    use super::bam_to_radian;
    use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};

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
