use std::i32;

use crate::{fixed_to_float, float_to_fixed};
use lazy_static::lazy_static;

const FRACBITS: i32 = 16;
const FRACUNIT: i32 = 1 << FRACBITS;

// Size of the angle table (fineangles)
pub const FINEANGLES: usize = 8192;
pub const FINEMASK: usize = FINEANGLES - 1;

// Angle conversions
pub const ANGLE_90: i32 = FINEANGLES as i32 / 4;
pub const ANGLE_180: i32 = ANGLE_90 * 2;
pub const ANGLE_270: i32 = ANGLE_90 * 3;
pub const ANGLE_MAX: i32 = FINEANGLES as i32;

lazy_static! {
    static ref FINETANGENT_TABLE: [FixedPoint; FINEANGLES / 4 + 1] = {
        let mut table = [FixedPoint::new(0); FINEANGLES / 4 + 1];
        for i in 0..=FINEANGLES / 4 {
            // Calculate sine values from 0 to 90 degrees (0 to PI/2)
            let angle = (i as f32) * (std::f32::consts::PI / 2.0) / (FINEANGLES / 4) as f32;
            table[i] = FixedPoint::from(angle.sin());
        }
        table
    };
}

#[derive(Clone, Copy, Default)]
pub struct FixedPoint(i32);

impl FixedPoint {
    pub const fn raw(self) -> i32 {
        self.0
    }
    pub const fn from_raw(value: i32) -> Self {
        Self(value)
    }
}

impl FixedPoint {
    pub const fn new(value: i32) -> Self {
        Self(value << 16)
    }

    pub const fn unit() -> Self {
        Self(FRACUNIT)
    }

    pub const fn neg_unit() -> Self {
        Self(-FRACUNIT)
    }

    pub const fn zero() -> Self {
        Self(0)
    }

    pub const fn min_value() -> Self {
        Self(i32::MIN)
    }

    pub const fn max_value() -> Self {
        Self(i32::MAX)
    }

    pub const PI: Self = Self(205887);
    pub const TAU: Self = Self(411775);
    pub const FRAC_PI_2: Self = Self(102944);
    pub const FRAC_PI_3: Self = Self(68629);
    pub const FRAC_PI_4: Self = Self(51472);
    pub const FRAC_PI_6: Self = Self(34315);
    pub const FRAC_PI_8: Self = Self(25736);
    pub const FRAC_1_PI: Self = Self(20861);
    pub const FRAC_2_PI: Self = Self(41721);
    pub const FRAC_2_SQRT_PI: Self = Self(73588);
    pub const SQRT_2: Self = Self(92682);
    pub const FRAC_1_SQRT_2: Self = Self(46341);
    pub const E: Self = Self(178145);
    pub const LOG2_E: Self = Self(94548);
    pub const LOG10_E: Self = Self(28377);
    pub const LN_2: Self = Self(45426);
    pub const LN_10: Self = Self(150902);

    pub fn abs(self) -> Self {
        Self(self.0.abs())
    }

    pub fn min(self, other: Self) -> Self {
        if self < other { self } else { other }
    }

    pub fn max(self, other: Self) -> Self {
        if self > other { self } else { other }
    }

    pub fn sqrt(self) -> Self {
        if self.0 <= 0 {
            return Self::zero();
        }
        let float_val = fixed_to_float(self.0);
        Self(float_to_fixed(float_val.sqrt()))
    }

    pub fn floor(self) -> Self {
        Self(self.0 & !(FRACUNIT - 1))
    }

    pub fn ceil(self) -> Self {
        if self.0 & (FRACUNIT - 1) == 0 {
            self
        } else {
            Self((self.0 & !(FRACUNIT - 1)) + FRACUNIT)
        }
    }

    pub fn round(self) -> Self {
        Self((self.0 + (FRACUNIT >> 1)) & !(FRACUNIT - 1))
    }

    pub fn fract(self) -> Self {
        Self(self.0 & (FRACUNIT - 1))
    }

    pub fn powi(self, n: i32) -> Self {
        let float_val = fixed_to_float(self.0);
        Self(float_to_fixed(float_val.powi(n)))
    }

    pub fn powf(self, n: Self) -> Self {
        let base = fixed_to_float(self.0);
        let exp = fixed_to_float(n.0);
        Self(float_to_fixed(base.powf(exp)))
    }

    pub fn exp(self) -> Self {
        let float_val = fixed_to_float(self.0);
        Self(float_to_fixed(float_val.exp()))
    }

    pub fn ln(self) -> Self {
        if self.0 <= 0 {
            return Self(i32::MIN);
        }
        let float_val = fixed_to_float(self.0);
        Self(float_to_fixed(float_val.ln()))
    }

    pub fn log(self, base: Self) -> Self {
        if self.0 <= 0 || base.0 <= 0 {
            return Self(i32::MIN);
        }
        let val = fixed_to_float(self.0);
        let base_val = fixed_to_float(base.0);
        Self(float_to_fixed(val.log(base_val)))
    }

    pub fn log2(self) -> Self {
        if self.0 <= 0 {
            return Self(i32::MIN);
        }
        let float_val = fixed_to_float(self.0);
        Self(float_to_fixed(float_val.log2()))
    }

    pub fn log10(self) -> Self {
        if self.0 <= 0 {
            return Self(i32::MIN);
        }
        let float_val = fixed_to_float(self.0);
        Self(float_to_fixed(float_val.log10()))
    }

    pub fn recip(self) -> Self {
        Self::unit() / self
    }

    pub fn to_degrees(self) -> Self {
        self * Self::from(180.0 / std::f32::consts::PI)
    }

    pub fn to_radians(self) -> Self {
        self * Self::from(std::f32::consts::PI / 180.0)
    }

    pub fn signum(self) -> Self {
        if self.0 > 0 {
            Self::unit()
        } else if self.0 < 0 {
            -Self::unit()
        } else {
            Self::zero()
        }
    }

    pub fn copysign(self, sign: Self) -> Self {
        if sign.0 >= 0 { self.abs() } else { -self.abs() }
    }

    pub fn is_sign_positive(self) -> bool {
        self.0 >= 0
    }

    pub fn is_sign_negative(self) -> bool {
        self.0 < 0
    }

    pub fn is_finite(self) -> bool {
        true
    }

    pub fn is_infinite(self) -> bool {
        false
    }

    pub fn is_nan(self) -> bool {
        false
    }

    pub fn is_normal(self) -> bool {
        self.0 != 0
    }

    pub fn mul_add(self, a: Self, b: Self) -> Self {
        self * a + b
    }

    pub fn atan(self) -> Self {
        let float_val = fixed_to_float(self.0);
        Self(float_to_fixed(float_val.atan()))
    }

    pub fn atan2(self, other: Self) -> Self {
        let y = fixed_to_float(self.0);
        let x = fixed_to_float(other.0);
        Self(float_to_fixed(y.atan2(x)))
    }

    pub fn asin(self) -> Self {
        let float_val = fixed_to_float(self.0);
        Self(float_to_fixed(float_val.asin()))
    }

    pub fn acos(self) -> Self {
        let float_val = fixed_to_float(self.0);
        Self(float_to_fixed(float_val.acos()))
    }

    pub fn sinh(self) -> Self {
        let float_val = fixed_to_float(self.0);
        Self(float_to_fixed(float_val.sinh()))
    }

    pub fn cosh(self) -> Self {
        let float_val = fixed_to_float(self.0);
        Self(float_to_fixed(float_val.cosh()))
    }

    pub fn tanh(self) -> Self {
        let float_val = fixed_to_float(self.0);
        Self(float_to_fixed(float_val.tanh()))
    }

    pub fn asinh(self) -> Self {
        let float_val = fixed_to_float(self.0);
        Self(float_to_fixed(float_val.asinh()))
    }

    pub fn acosh(self) -> Self {
        let float_val = fixed_to_float(self.0);
        Self(float_to_fixed(float_val.acosh()))
    }

    pub fn atanh(self) -> Self {
        let float_val = fixed_to_float(self.0);
        Self(float_to_fixed(float_val.atanh()))
    }

    pub fn exp2(self) -> Self {
        let float_val = fixed_to_float(self.0);
        Self(float_to_fixed(float_val.exp2()))
    }

    pub fn exp_m1(self) -> Self {
        let float_val = fixed_to_float(self.0);
        Self(float_to_fixed(float_val.exp_m1()))
    }

    pub fn ln_1p(self) -> Self {
        let float_val = fixed_to_float(self.0);
        Self(float_to_fixed(float_val.ln_1p()))
    }

    pub fn cbrt(self) -> Self {
        let float_val = fixed_to_float(self.0);
        Self(float_to_fixed(float_val.cbrt()))
    }

    pub fn hypot(self, other: Self) -> Self {
        let x = fixed_to_float(self.0);
        let y = fixed_to_float(other.0);
        Self(float_to_fixed(x.hypot(y)))
    }

    pub fn clamp(self, min: Self, max: Self) -> Self {
        if self < min {
            min
        } else if self > max {
            max
        } else {
            self
        }
    }

    pub const fn truncate(self) -> Self {
        Self(self.0 & !(FRACUNIT - 1))
    }

    /// Returns the minimum of self and the given i32 value
    pub fn min_with_i32(self, other: i32) -> Self {
        // Properly convert i32 to fixed point by shifting
        let other_fixed = Self(other << FRACBITS);
        if self < other_fixed {
            self
        } else {
            other_fixed
        }
    }

    pub fn from_radian(rad: f32) -> Self {
        // 2π radians = FINEANGLES units
        let doom_angle = (rad * FINEANGLES as f32 / (2.0 * std::f32::consts::PI)) as i32;
        Self(doom_angle & FINEMASK as i32)
    }

    // Get sine from table using Doom-style angle (0-8191)
    pub fn finesine(angle: i32) -> Self {
        let angle = angle & FINEMASK as i32;
        // unsafety gives us 0.44ms gain
        unsafe {
            match angle / ANGLE_90 {
                0 => *FINETANGENT_TABLE.get_unchecked(angle as usize),
                1 => *FINETANGENT_TABLE.get_unchecked((ANGLE_180 - angle) as usize),
                2 => -*FINETANGENT_TABLE.get_unchecked((angle - ANGLE_180) as usize),
                _ => -*FINETANGENT_TABLE.get_unchecked((ANGLE_MAX - angle) as usize),
            }
        }
    }

    pub fn finecosine(angle: i32) -> Self {
        Self::finesine((angle + ANGLE_90) & FINEMASK as i32)
    }

    pub fn sin(self) -> Self {
        let float_val = fixed_to_float(self.0);
        Self(float_to_fixed(float_val.sin()))
    }

    pub fn cos(self) -> Self {
        let float_val = fixed_to_float(self.0);
        Self(float_to_fixed(float_val.cos()))
    }

    pub fn tan(self) -> Self {
        let float_val = fixed_to_float(self.0);
        Self(float_to_fixed(float_val.tan()))
    }
}

impl From<usize> for FixedPoint {
    fn from(value: usize) -> Self {
        Self((value as i32) << FRACBITS)
    }
}

impl std::ops::Mul<FixedPoint> for f32 {
    type Output = FixedPoint;

    fn mul(self, rhs: FixedPoint) -> FixedPoint {
        FixedPoint(float_to_fixed(self * fixed_to_float(rhs.0)))
    }
}

// Safe addition implementation using Rust's built-in checked methods
impl std::ops::Add for FixedPoint {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self(self.0.saturating_add(rhs.0))
    }
}

impl PartialOrd<u32> for FixedPoint {
    fn partial_cmp(&self, other: &u32) -> Option<std::cmp::Ordering> {
        // Convert u32 to fixed point format for comparison
        let other_fixed = (*other as i32) << FRACBITS;
        self.0.partial_cmp(&other_fixed)
    }
}

impl PartialEq<u32> for FixedPoint {
    fn eq(&self, other: &u32) -> bool {
        let other_fixed = (*other as i32) << FRACBITS;
        self.0 == other_fixed
    }
}

impl std::ops::Mul<i32> for FixedPoint {
    type Output = Self;

    fn mul(self, rhs: i32) -> Self::Output {
        Self(((self.0 as i64) * (rhs as i64) >> FRACBITS) as i32)
    }
}

impl std::ops::Mul<u32> for FixedPoint {
    type Output = Self;

    fn mul(self, rhs: u32) -> Self::Output {
        Self(((self.0 as i64) * (rhs as i64) >> FRACBITS) as i32)
    }
}

impl std::ops::Mul<FixedPoint> for i32 {
    type Output = FixedPoint;

    fn mul(self, rhs: FixedPoint) -> FixedPoint {
        rhs * self
    }
}

impl std::ops::Sub<FixedPoint> for i32 {
    type Output = FixedPoint;

    fn sub(self, rhs: FixedPoint) -> FixedPoint {
        // Convert i32 to fixed point format, then perform subtraction
        let self_fixed = self << FRACBITS;
        FixedPoint(self_fixed.saturating_sub(rhs.0))
    }
}

impl std::ops::Sub<i32> for FixedPoint {
    type Output = FixedPoint;

    fn sub(self, rhs: i32) -> FixedPoint {
        // Convert i32 to fixed point format, then perform subtraction
        let rhs_fixed = rhs << FRACBITS;
        FixedPoint(self.0.saturating_sub(rhs_fixed))
    }
}
// Safe addition with f32
impl std::ops::Add<f32> for FixedPoint {
    type Output = Self;

    fn add(self, rhs: f32) -> Self::Output {
        let rhs_fixed = float_to_fixed(rhs);
        Self(self.0.saturating_add(rhs_fixed))
    }
}

// Safe subtraction
impl std::ops::Sub for FixedPoint {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0.saturating_sub(rhs.0))
    }
}

// Safe subtraction with f32
impl std::ops::Sub<f32> for FixedPoint {
    type Output = Self;

    fn sub(self, rhs: f32) -> Self::Output {
        let rhs_fixed = float_to_fixed(rhs);
        Self(self.0.saturating_sub(rhs_fixed))
    }
}

// Simplified multiplication for FixedPoint
impl std::ops::Mul<FixedPoint> for FixedPoint {
    type Output = Self;

    fn mul(self, rhs: FixedPoint) -> Self::Output {
        // Use i64 to avoid overflow during multiplication
        let a = self.0 as i64;
        let b = rhs.0 as i64;
        let result = (a * b) >> FRACBITS;

        // Convert back to i32 with saturation
        if result > i32::MAX as i64 {
            Self(i32::MAX)
        } else if result < i32::MIN as i64 {
            Self(i32::MIN)
        } else {
            Self(result as i32)
        }
    }
}

impl std::ops::Mul<f32> for FixedPoint {
    type Output = Self;

    fn mul(self, rhs: f32) -> Self::Output {
        // Convert to float, multiply, and convert back with safety checks
        let result = fixed_to_float(self.0) * rhs;
        if result.is_infinite()
            || result.is_nan()
            || result > fixed_to_float(i32::MAX)
            || result < fixed_to_float(i32::MIN)
        {
            if result > 0.0 {
                Self(i32::MAX)
            } else {
                Self(i32::MIN)
            }
        } else {
            Self(float_to_fixed(result))
        }
    }
}

impl std::ops::Shr<i32> for FixedPoint {
    type Output = Self;

    fn shr(self, rhs: i32) -> Self::Output {
        // For fixed point, we need to shift by FRACBITS + rhs to maintain the fixed point format
        let shift_amount = FRACBITS + rhs;

        // Clamp shift amount to prevent undefined behavior
        if shift_amount >= 32 {
            // If shifting by 32 or more bits, result is 0 (or -1 for negative numbers)
            Self(if self.0 < 0 { -1 } else { 0 })
        } else if shift_amount < 0 {
            // If shift amount is negative, we're actually left shifting
            Self(self.0 << (-shift_amount))
        } else {
            Self(self.0 >> shift_amount)
        }
    }
}

impl std::ops::Div<i32> for FixedPoint {
    type Output = Self;

    fn div(self, rhs: i32) -> Self::Output {
        if rhs == 0 {
            return Self(if self.0 < 0 { i32::MIN } else { i32::MAX });
        }

        // Use i64 to avoid overflow
        let a = self.0 as i64;
        let b = rhs as i64;
        let result = a / b;

        // Saturating conversion to i32
        if result > i32::MAX as i64 {
            Self(i32::MAX)
        } else if result < i32::MIN as i64 {
            Self(i32::MIN)
        } else {
            Self(result as i32)
        }
    }
}

// Safe division for FixedPoint
impl std::ops::Div<FixedPoint> for FixedPoint {
    type Output = Self;

    fn div(self, rhs: FixedPoint) -> Self::Output {
        if rhs.0 == 0 {
            return Self(if self.0 < 0 { i32::MIN } else { i32::MAX });
        }

        // Use i64 to avoid overflow
        let a = (self.0 as i64) << FRACBITS;
        let b = rhs.0 as i64;
        let result = a / b;

        // Saturating conversion to i32
        if result > i32::MAX as i64 {
            Self(i32::MAX)
        } else if result < i32::MIN as i64 {
            Self(i32::MIN)
        } else {
            Self(result as i32)
        }
    }
}

impl std::ops::Div<f32> for FixedPoint {
    type Output = Self;

    fn div(self, rhs: f32) -> Self::Output {
        if rhs == 0.0 {
            return Self(if self.0 < 0 { i32::MIN } else { i32::MAX });
        }
        let result = fixed_to_float(self.0) / rhs;
        if result.is_infinite()
            || result.is_nan()
            || result > fixed_to_float(i32::MAX)
            || result < fixed_to_float(i32::MIN)
        {
            if result > 0.0 {
                Self(i32::MAX)
            } else {
                Self(i32::MIN)
            }
        } else {
            Self(float_to_fixed(result))
        }
    }
}

// Add assign implementation with saturation
impl std::ops::AddAssign for FixedPoint {
    fn add_assign(&mut self, rhs: Self) {
        self.0 = self.0.saturating_add(rhs.0);
    }
}

impl std::ops::AddAssign<f32> for FixedPoint {
    fn add_assign(&mut self, rhs: f32) {
        self.0 = self.0.saturating_add(float_to_fixed(rhs));
    }
}

// Sub assign implementation with saturation
impl std::ops::SubAssign for FixedPoint {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 = self.0.saturating_sub(rhs.0);
    }
}

impl std::ops::SubAssign<f32> for FixedPoint {
    fn sub_assign(&mut self, rhs: f32) {
        self.0 = self.0.saturating_sub(float_to_fixed(rhs));
    }
}

// MulAssign implementation with saturation
impl std::ops::MulAssign for FixedPoint {
    fn mul_assign(&mut self, rhs: Self) {
        // Use the multiplication implementation with saturation
        *self = *self * rhs;
    }
}

impl std::ops::MulAssign<f32> for FixedPoint {
    fn mul_assign(&mut self, rhs: f32) {
        *self = *self * rhs;
    }
}

// Division assign with saturation
impl std::ops::DivAssign for FixedPoint {
    fn div_assign(&mut self, rhs: Self) {
        // Use the division implementation with saturation
        *self = *self / rhs;
    }
}

impl std::ops::DivAssign<f32> for FixedPoint {
    fn div_assign(&mut self, rhs: f32) {
        *self = *self / rhs;
    }
}

impl std::ops::Add<i32> for FixedPoint {
    type Output = Self;

    fn add(self, rhs: i32) -> Self::Output {
        Self(self.0.saturating_add(rhs << FRACBITS))
    }
}

impl std::ops::Neg for FixedPoint {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self(self.0.wrapping_neg())
    }
}

impl From<f32> for FixedPoint {
    fn from(value: f32) -> Self {
        Self(float_to_fixed(value))
    }
}

impl From<i16> for FixedPoint {
    fn from(value: i16) -> Self {
        Self((value as i32) << FRACBITS)
    }
}

impl From<i32> for FixedPoint {
    fn from(value: i32) -> Self {
        Self(value << FRACBITS)
    }
}

impl From<u32> for FixedPoint {
    fn from(value: u32) -> Self {
        Self((value as i32) << FRACBITS)
    }
}

impl From<FixedPoint> for f32 {
    fn from(value: FixedPoint) -> Self {
        fixed_to_float(value.0)
    }
}

impl From<FixedPoint> for i32 {
    fn from(value: FixedPoint) -> Self {
        value.0 >> FRACBITS
    }
}

impl From<FixedPoint> for u32 {
    /// Will use `abs()` on internal value
    fn from(value: FixedPoint) -> Self {
        (value.0 >> FRACBITS) as u32
    }
}

impl From<FixedPoint> for usize {
    /// Will use `abs()` on internal value
    fn from(value: FixedPoint) -> Self {
        (value.0 >> FRACBITS) as u32 as usize
    }
}

impl From<FixedPoint> for i16 {
    fn from(value: FixedPoint) -> Self {
        let shifted = value.0 >> FRACBITS;

        if shifted > i16::MAX as i32 {
            i16::MAX
        } else if shifted < i16::MIN as i32 {
            i16::MIN
        } else {
            shifted as i16
        }
    }
}

impl From<FixedPoint> for u16 {
    fn from(value: FixedPoint) -> Self {
        if value.0 <= 0 {
            0
        } else {
            let shifted = value.0 >> FRACBITS;
            if shifted > u16::MAX as i32 {
                u16::MAX
            } else {
                shifted as u16
            }
        }
    }
}

impl PartialEq for FixedPoint {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl std::ops::Div<FixedPoint> for f32 {
    type Output = FixedPoint;

    fn div(self, rhs: FixedPoint) -> FixedPoint {
        if rhs.0 == 0 {
            return FixedPoint(if self < 0.0 { i32::MIN } else { i32::MAX });
        }

        let result = self / fixed_to_float(rhs.0);

        if result.is_infinite()
            || result.is_nan()
            || result > fixed_to_float(i32::MAX)
            || result < fixed_to_float(i32::MIN)
        {
            if result > 0.0 {
                FixedPoint(i32::MAX)
            } else {
                FixedPoint(i32::MIN)
            }
        } else {
            FixedPoint(float_to_fixed(result))
        }
    }
}
impl Eq for FixedPoint {}

impl PartialOrd for FixedPoint {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

impl Ord for FixedPoint {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

impl PartialEq<f32> for FixedPoint {
    fn eq(&self, other: &f32) -> bool {
        fixed_to_float(self.0) == *other
    }
}

impl PartialOrd<f32> for FixedPoint {
    fn partial_cmp(&self, other: &f32) -> Option<std::cmp::Ordering> {
        fixed_to_float(self.0).partial_cmp(other)
    }
}

impl PartialEq<i32> for FixedPoint {
    fn eq(&self, other: &i32) -> bool {
        let other_fixed = (*other as i32) << FRACBITS;
        self.0 == other_fixed
    }
}

impl PartialOrd<i32> for FixedPoint {
    fn partial_cmp(&self, other: &i32) -> Option<std::cmp::Ordering> {
        let other_fixed = (*other as i32) << FRACBITS;
        self.0.partial_cmp(&other_fixed)
    }
}

impl PartialEq<i16> for FixedPoint {
    fn eq(&self, other: &i16) -> bool {
        let other_fixed = (*other as i32) << FRACBITS;
        self.0 == other_fixed
    }
}

impl PartialOrd<i16> for FixedPoint {
    fn partial_cmp(&self, other: &i16) -> Option<std::cmp::Ordering> {
        let other_fixed = (*other as i32) << FRACBITS;
        self.0.partial_cmp(&other_fixed)
    }
}

impl PartialEq<u16> for FixedPoint {
    fn eq(&self, other: &u16) -> bool {
        let other_fixed = (*other as i32) << FRACBITS;
        self.0 == other_fixed
    }
}

impl PartialOrd<u16> for FixedPoint {
    fn partial_cmp(&self, other: &u16) -> Option<std::cmp::Ordering> {
        let other_fixed = (*other as i32) << FRACBITS;
        self.0.partial_cmp(&other_fixed)
    }
}

impl std::fmt::Display for FixedPoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", fixed_to_float(self.0))
    }
}

impl std::fmt::Debug for FixedPoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "FixedPoint({})", fixed_to_float(self.0))
    }
}
