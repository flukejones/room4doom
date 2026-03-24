use std::fmt;
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

use crate::doom_trig::{fine_cos, fine_sin};

// --- Compile-time configuration ---

#[cfg(any(feature = "fixed64", feature = "fixed64hd"))]
pub type Inner = i64;
#[cfg(any(feature = "fixed64", feature = "fixed64hd"))]
pub type WideInner = i128;

#[cfg(not(any(feature = "fixed64", feature = "fixed64hd")))]
pub type Inner = i32;
#[cfg(not(any(feature = "fixed64", feature = "fixed64hd")))]
pub type WideInner = i64;

#[cfg(feature = "fixed64hd")]
pub const FRACBITS: u32 = 32;
#[cfg(not(feature = "fixed64hd"))]
pub const FRACBITS: u32 = 16;

pub const FRACUNIT: Inner = 1 << FRACBITS;

/// Shift for converting between WAD 16.16 format and internal representation.
const WAD_SHIFT: u32 = FRACBITS - 16;

/// Doom-compatible fixed-point number.
///
/// Internal format depends on compile-time feature flags:
/// - default: 16.16 (i32) — OG Doom demo compatible
/// - `fixed64`: 48.16 (i64) — large maps, same precision
/// - `fixed64hd`: 32.32 (i64) — large maps + high precision
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Default, Debug)]
pub struct FixedT(pub Inner);

impl FixedT {
    pub const ZERO: Self = Self(0);
    pub const ONE: Self = Self(FRACUNIT);
    pub const MAX: Self = Self(Inner::MAX);
    pub const MIN: Self = Self(Inner::MIN);

    /// Raw inner value (fixed-point bits).
    #[inline]
    pub const fn raw(self) -> Inner {
        self.0
    }

    /// Construct from WAD/OG 16.16 fixed-point i32.
    #[inline]
    pub const fn from_fixed(raw: i32) -> Self {
        Self((raw as Inner) << WAD_SHIFT)
    }

    /// Export as WAD-compatible 16.16 i32 (truncates in 64-bit modes).
    #[inline]
    pub const fn to_fixed_raw(self) -> i32 {
        (self.0 >> WAD_SHIFT) as i32
    }

    /// Doom `FixedMul`: `(a * b) >> FRACBITS` via wide intermediate.
    #[inline]
    pub fn fixed_mul(self, rhs: Self) -> Self {
        Self(((self.0 as WideInner * rhs.0 as WideInner) >> FRACBITS) as Inner)
    }

    /// Doom `FixedDiv` with overflow guard.
    #[inline]
    pub fn fixed_div(self, rhs: Self) -> Self {
        if rhs.0 == 0 || (self.0.unsigned_abs() >> (FRACBITS - 2)) >= rhs.0.unsigned_abs() {
            if (self.0 ^ rhs.0) < 0 {
                Self(Inner::MIN)
            } else {
                Self(Inner::MAX)
            }
        } else {
            Self((((self.0 as WideInner) << FRACBITS) / rhs.0 as WideInner) as Inner)
        }
    }

    /// Sine from BAM angle via OG Doom finesine table.
    #[inline]
    pub fn sin_bam(bam: u32) -> Self {
        fine_sin(bam)
    }

    /// Cosine from BAM angle via OG Doom finecosine table.
    #[inline]
    pub fn cos_bam(bam: u32) -> Self {
        fine_cos(bam)
    }

    /// Convert to f32.
    #[inline]
    pub fn to_f32(self) -> f32 {
        #[cfg(any(feature = "fixed64", feature = "fixed64hd"))]
        {
            (self.0 as f64 / FRACUNIT as f64) as f32
        }
        #[cfg(not(any(feature = "fixed64", feature = "fixed64hd")))]
        {
            self.0 as f32 / FRACUNIT as f32
        }
    }

    /// Convert to f64 (lossless for i32, near-lossless for i64).
    #[inline]
    pub fn to_f64(self) -> f64 {
        self.0 as f64 / FRACUNIT as f64
    }

    /// Construct from an f32 value.
    #[inline]
    pub fn from_f32(v: f32) -> Self {
        #[cfg(any(feature = "fixed64", feature = "fixed64hd"))]
        {
            Self((v as f64 * FRACUNIT as f64) as Inner)
        }
        #[cfg(not(any(feature = "fixed64", feature = "fixed64hd")))]
        {
            Self((v * FRACUNIT as f32) as Inner)
        }
    }

    /// Absolute value (wrapping, matching OG Doom).
    #[inline]
    pub fn doom_abs(self) -> Self {
        if self.0 < 0 {
            Self(self.0.wrapping_neg())
        } else {
            self
        }
    }

    /// Clamp between min and max.
    #[inline]
    pub fn clamp(self, min: Self, max: Self) -> Self {
        if self.0 < min.0 {
            min
        } else if self.0 > max.0 {
            max
        } else {
            self
        }
    }

    /// Arithmetic right shift by `bits`.
    #[inline]
    pub fn shr(self, bits: u32) -> Self {
        Self(self.0 >> bits)
    }

    /// Raw integer `/2` (toward zero), matching C division.
    #[inline]
    pub fn half_toward_zero(self) -> Self {
        Self(self.0 / 2)
    }

    /// Convert to i32 (truncating fractional part).
    #[inline]
    pub fn to_i32(self) -> i32 {
        (self.0 >> FRACBITS) as i32
    }

    /// Check if negative.
    #[inline]
    pub fn is_negative(self) -> bool {
        self.0 < 0
    }

    /// Check if zero.
    #[inline]
    pub fn is_zero(self) -> bool {
        self.0 == 0
    }
}

// --- Arithmetic ops (wrapping) ---

impl Add for FixedT {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self(self.0.wrapping_add(rhs.0))
    }
}

impl Sub for FixedT {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self(self.0.wrapping_sub(rhs.0))
    }
}

impl Neg for FixedT {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        Self(self.0.wrapping_neg())
    }
}

impl Mul for FixedT {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: Self) -> Self {
        self.fixed_mul(rhs)
    }
}

impl Div for FixedT {
    type Output = Self;
    #[inline]
    fn div(self, rhs: Self) -> Self {
        self.fixed_div(rhs)
    }
}

impl AddAssign for FixedT {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        self.0 = self.0.wrapping_add(rhs.0);
    }
}

impl AddAssign<i32> for FixedT {
    #[inline]
    fn add_assign(&mut self, rhs: i32) {
        self.0 = self.0.wrapping_add((rhs as Inner) << FRACBITS);
    }
}

impl SubAssign for FixedT {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        self.0 = self.0.wrapping_sub(rhs.0);
    }
}

impl SubAssign<i32> for FixedT {
    #[inline]
    fn sub_assign(&mut self, rhs: i32) {
        self.0 = self.0.wrapping_sub((rhs as Inner) << FRACBITS);
    }
}

impl MulAssign for FixedT {
    #[inline]
    fn mul_assign(&mut self, rhs: Self) {
        *self = self.fixed_mul(rhs);
    }
}

impl DivAssign for FixedT {
    #[inline]
    fn div_assign(&mut self, rhs: Self) {
        *self = self.fixed_div(rhs);
    }
}

// --- Mixed ops with f32 ---

impl Add<f32> for FixedT {
    type Output = Self;
    #[inline]
    fn add(self, rhs: f32) -> Self {
        Self(self.0.wrapping_add(Self::from_f32(rhs).0))
    }
}

impl Sub<f32> for FixedT {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: f32) -> Self {
        Self(self.0.wrapping_sub(Self::from_f32(rhs).0))
    }
}

impl Mul<f32> for FixedT {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: f32) -> Self {
        self.fixed_mul(Self::from_f32(rhs))
    }
}

impl Div<f32> for FixedT {
    type Output = Self;
    #[inline]
    fn div(self, rhs: f32) -> Self {
        self.fixed_div(Self::from_f32(rhs))
    }
}

impl PartialEq<f32> for FixedT {
    #[inline]
    fn eq(&self, other: &f32) -> bool {
        self.0 == Self::from_f32(*other).0
    }
}

impl PartialOrd<f32> for FixedT {
    #[inline]
    fn partial_cmp(&self, other: &f32) -> Option<std::cmp::Ordering> {
        Some(self.0.cmp(&Self::from_f32(*other).0))
    }
}

// --- Mixed ops with i32 ---

impl Add<i32> for FixedT {
    type Output = Self;
    #[inline]
    fn add(self, rhs: i32) -> Self {
        Self(self.0.wrapping_add((rhs as Inner) << FRACBITS))
    }
}

impl Sub<i32> for FixedT {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: i32) -> Self {
        Self(self.0.wrapping_sub((rhs as Inner) << FRACBITS))
    }
}

impl Mul<i32> for FixedT {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: i32) -> Self {
        self.fixed_mul(Self((rhs as Inner) << FRACBITS))
    }
}

impl Div<i32> for FixedT {
    type Output = Self;
    #[inline]
    fn div(self, rhs: i32) -> Self {
        self.fixed_div(Self((rhs as Inner) << FRACBITS))
    }
}

// --- Reverse ops (i32/f32 on left) ---

impl Div<FixedT> for f32 {
    type Output = FixedT;
    #[inline]
    fn div(self, rhs: FixedT) -> FixedT {
        FixedT::from_f32(self).fixed_div(rhs)
    }
}

impl Sub<FixedT> for i32 {
    type Output = FixedT;
    #[inline]
    fn sub(self, rhs: FixedT) -> FixedT {
        FixedT(((self as Inner) << FRACBITS).wrapping_sub(rhs.0))
    }
}

impl Div<FixedT> for i32 {
    type Output = FixedT;
    #[inline]
    fn div(self, rhs: FixedT) -> FixedT {
        FixedT((self as Inner) << FRACBITS).fixed_div(rhs)
    }
}

impl PartialEq<i32> for FixedT {
    #[inline]
    fn eq(&self, other: &i32) -> bool {
        self.0 == ((*other as Inner) << FRACBITS)
    }
}

impl PartialOrd<i32> for FixedT {
    #[inline]
    fn partial_cmp(&self, other: &i32) -> Option<std::cmp::Ordering> {
        Some(self.0.cmp(&((*other as Inner) << FRACBITS)))
    }
}

// --- Conversions ---

impl From<i32> for FixedT {
    #[inline]
    fn from(v: i32) -> Self {
        Self((v as Inner) << FRACBITS)
    }
}

impl From<f32> for FixedT {
    #[inline]
    fn from(v: f32) -> Self {
        Self::from_f32(v)
    }
}

impl From<FixedT> for f32 {
    #[inline]
    fn from(v: FixedT) -> f32 {
        v.to_f32()
    }
}

impl From<FixedT> for i32 {
    #[inline]
    fn from(v: FixedT) -> i32 {
        v.to_i32()
    }
}

impl From<FixedT> for f64 {
    #[inline]
    fn from(v: FixedT) -> f64 {
        v.to_f64()
    }
}

impl From<f64> for FixedT {
    #[inline]
    fn from(v: f64) -> Self {
        Self((v * FRACUNIT as f64) as Inner)
    }
}

impl fmt::Display for FixedT {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0 as f64 / FRACUNIT as f64)
    }
}

// --- Free functions ---

/// OG Doom `P_AproxDistance`.
#[inline]
pub fn p_aprox_distance(dx: FixedT, dy: FixedT) -> FixedT {
    let dx = dx.doom_abs();
    let dy = dy.doom_abs();
    if dx < dy {
        dx.shr(1) + dy
    } else {
        dx + dy.shr(1)
    }
}
