pub mod angle;
pub mod bam;
pub mod doom_trig;
pub mod fixed_point;
mod intercept;
mod trig;

pub use angle::*;
pub use bam::{ANG45, ANG90, ANG180, ANG270, Bam, aprox_distance, bam_to_radian, radian_to_bam};
pub use doom_trig::{
    ANGLETOFINESHIFT, fine_cos, fine_sin, fine_tan, finecosine, finesine, r_point_to_angle, r_point_to_dist
};
pub use fixed_point::{FRACBITS, FRACUNIT, FixedT, Inner, p_aprox_distance};
pub use intercept::*;

const FRACBITS_F: i32 = 16;
const FRACUNIT_F: f32 = (1 << FRACBITS_F) as f32;

/// Convert a Doom `fixed_t` fixed-point value to `f32`
pub const fn fixed_to_float(value: i32) -> f32 {
    value as f32 / FRACUNIT_F
}

pub const fn float_to_fixed(value: f32) -> i32 {
    (value * FRACUNIT_F) as i32
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
pub fn p_random() -> i32 {
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
pub fn m_clear_random() {
    unsafe {
        RNDINDEX = 0;
        PRNDINDEX = 0;
    }
}

#[inline]
pub fn get_prndindex() -> usize {
    unsafe { PRNDINDEX }
}

#[inline]
pub fn set_prndindex(i: usize) {
    unsafe { PRNDINDEX = i & 0xFF }
}

#[inline]
pub fn get_rndindex() -> usize {
    unsafe { RNDINDEX }
}

#[inline]
pub fn set_rndindex(i: usize) {
    unsafe { RNDINDEX = i & 0xFF }
}

#[inline]
pub fn p_subrandom() -> i32 {
    let r = p_random();
    r - p_random()
}

#[cfg(test)]
mod tests {
    use super::bam_to_radian;
    use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};

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
