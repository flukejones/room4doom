use std::f32::consts::PI;
use std::f64::consts::TAU;

pub const ANG45: u32 = 0x20000000;
pub const ANG90: u32 = 0x40000000;
pub const ANG180: u32 = 0x80000000;
pub const ANG270: u32 = 0xC0000000;

/// Binary Angle Measurement — wrapping u32 angle.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Bam(pub u32);

/// Converts a BAM (Binary Angle Measure) to radians.
#[inline]
#[expect(
    clippy::cast_possible_truncation,
    reason = "Doom fixed-point conversion deliberately preserves its rounding and low-bit arithmetic semantics"
)]
pub fn bam_to_radian(bam: u32) -> f32 {
    (bam as f64 / u32::MAX as f64 * TAU) as f32
}

/// Converts a look pitch to radians. Pitch is *signed* BAM, unlike the yaw
/// [`bam_to_radian`] takes, so `i32::MAX` is half a turn.
#[expect(
    clippy::cast_precision_loss,
    reason = "the divisor is a scale, and pitch is meaningful far above f32's mantissa"
)]
#[inline]
pub fn lookdir_to_radian(lookdir: f32) -> f32 {
    lookdir * PI / i32::MAX as f32
}

/// Converts radians to a BAM value. Handles negative radians via wrapping.
#[inline]
#[expect(
    clippy::cast_possible_truncation,
    reason = "Doom fixed-point conversion deliberately preserves its rounding and low-bit arithmetic semantics"
)]
pub fn radian_to_bam(rad: f32) -> u32 {
    (rad as f64 / TAU * u32::MAX as f64) as i64 as u32
}
