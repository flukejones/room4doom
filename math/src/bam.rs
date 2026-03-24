use std::f64::consts::TAU;

use crate::fixed_point::FixedT;

pub const ANG45: u32 = 0x20000000;
pub const ANG90: u32 = 0x40000000;
pub const ANG180: u32 = 0x80000000;
pub const ANG270: u32 = 0xC0000000;

/// Binary Angle Measurement — wrapping u32 angle.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Bam(pub u32);

/// Converts a BAM (Binary Angle Measure) to radians.
#[inline]
pub fn bam_to_radian(bam: u32) -> f32 {
    (bam as f64 / u32::MAX as f64 * TAU) as f32
}

/// Converts radians to a BAM value. Handles negative radians via wrapping.
#[inline]
pub fn radian_to_bam(rad: f32) -> u32 {
    (rad as f64 / TAU * u32::MAX as f64) as i64 as u32
}

/// Doom `P_AproxDistance`: fast approximate distance without sqrt.
#[inline]
pub fn aprox_distance(dx: FixedT, dy: FixedT) -> FixedT {
    let dx = dx.doom_abs();
    let dy = dy.doom_abs();
    if dx < dy {
        dx + dy - dx.shr(1)
    } else {
        dx + dy - dy.shr(1)
    }
}
