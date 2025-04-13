use crate::{fixed_to_float, float_to_fixed};

const FRACBITS: i32 = 16;
const FRACUNIT: i32 = 1 << FRACBITS;

#[derive(Debug, Clone, Copy, Default)]
pub struct FixedPoint(i32);

impl FixedPoint {
    pub const fn new(value: i32) -> Self {
        Self(value)
    }

    pub fn abs(self) -> Self {
        Self(self.0.abs())
    }

    pub fn ceil(self) -> Self {
        let floored = self.floor();
        if floored == self {
            floored
        } else {
            floored + 1
        }
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

    pub fn cos(self) -> Self {
        let angle_float: f32 = self.into();
        angle_float.cos().into()
    }

    pub const fn floor(self) -> Self {
        Self((self.0 >> FRACBITS) << FRACBITS)
    }

    pub fn max(self, other: Self) -> Self {
        if self > other { self } else { other }
    }

    pub fn min(self, other: Self) -> Self {
        if self < other { self } else { other }
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

    // pub fn round(self) -> Self {
    //     let float_val: f32 = self.into();
    //     float_val.round().into()
    // }

    pub fn sin(self) -> Self {
        let angle_float: f32 = self.into();
        angle_float.sin().into()
    }

    pub fn tan(self) -> Self {
        let angle_float: f32 = self.into();
        angle_float.tan().into()
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
    fn from(value: FixedPoint) -> Self {
        if value.0 <= 0 {
            0
        } else {
            (value.0 >> FRACBITS) as u32
        }
    }
}

impl From<FixedPoint> for usize {
    fn from(value: FixedPoint) -> Self {
        if value.0 <= 0 {
            0
        } else {
            (value.0 >> FRACBITS) as usize
        }
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
