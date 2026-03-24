use std::f32::consts::TAU;
use std::fmt;
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

use glam::Vec2;

use crate::bam::{Bam, bam_to_radian, radian_to_bam};
use crate::doom_trig::{fine_cos, fine_sin};
use crate::fixed_point::FixedT;
use crate::trig::{COS_TABLE, SIN_TABLE, TAN_TABLE};

/// Angle representation trait. Implemented for `f32` (radians) and `Bam` (u32).
pub trait AngleInner: Copy + Clone + Default + fmt::Debug + PartialEq + 'static {
    fn from_radians(rad: f32) -> Self;
    fn to_radians(self) -> f32;
    fn to_bam(self) -> u32;
    fn sin_f32(self) -> f32;
    fn cos_f32(self) -> f32;
    fn tan_f32(self) -> f32;
    fn sin_cos_f32(self) -> (f32, f32);
    fn wrap_add(self, other: Self) -> Self;
    fn wrap_sub(self, other: Self) -> Self;
    fn negate(self) -> Self;
    fn scale(self, factor: f32) -> Self;
    fn inv_scale(self, factor: f32) -> Self;
    fn from_atan2(y: f32, x: f32) -> Self;
    /// Construct from BAM u32 (identity for Bam, conversion for f32).
    fn from_bam(bam: u32) -> Self;
}

// --- AngleInner for f32 (radians) ---

impl AngleInner for f32 {
    #[inline]
    fn from_radians(rad: f32) -> Self {
        let mut r = rad % TAU;
        if r < 0.0 {
            r += TAU;
        }
        r
    }

    #[inline]
    fn to_radians(self) -> f32 {
        self
    }

    #[inline]
    fn to_bam(self) -> u32 {
        radian_to_bam(self)
    }

    #[inline]
    fn sin_f32(self) -> f32 {
        if cfg!(not(feature = "trig_lut")) {
            self.sin()
        } else {
            SIN_TABLE[rad_to_table(self)]
        }
    }

    #[inline]
    fn cos_f32(self) -> f32 {
        if cfg!(not(feature = "trig_lut")) {
            self.cos()
        } else {
            COS_TABLE[rad_to_table(self)]
        }
    }

    #[inline]
    fn tan_f32(self) -> f32 {
        if cfg!(not(feature = "trig_lut")) {
            self.tan()
        } else {
            TAN_TABLE[rad_to_table(self)]
        }
    }

    #[inline]
    fn sin_cos_f32(self) -> (f32, f32) {
        if cfg!(not(feature = "trig_lut")) {
            self.sin_cos()
        } else {
            let idx = rad_to_table(self);
            (SIN_TABLE[idx], COS_TABLE[idx])
        }
    }

    #[inline]
    fn wrap_add(self, other: Self) -> Self {
        Self::from_radians(self + other)
    }

    #[inline]
    fn wrap_sub(self, other: Self) -> Self {
        Self::from_radians(self - other)
    }

    #[inline]
    fn negate(self) -> Self {
        Self::from_radians(-self)
    }

    #[inline]
    fn scale(self, factor: f32) -> Self {
        Self::from_radians(self * factor)
    }

    #[inline]
    fn inv_scale(self, factor: f32) -> Self {
        Self::from_radians(self / factor)
    }

    #[inline]
    fn from_atan2(y: f32, x: f32) -> Self {
        Self::from_radians(y.atan2(x))
    }

    #[inline]
    fn from_bam(bam: u32) -> Self {
        bam_to_radian(bam)
    }
}

// --- AngleInner for Bam ---

impl AngleInner for Bam {
    #[inline]
    fn from_radians(rad: f32) -> Self {
        Bam(radian_to_bam(rad))
    }

    #[inline]
    fn to_radians(self) -> f32 {
        bam_to_radian(self.0)
    }

    #[inline]
    fn to_bam(self) -> u32 {
        self.0
    }

    #[inline]
    fn sin_f32(self) -> f32 {
        bam_to_radian(self.0).sin()
    }

    #[inline]
    fn cos_f32(self) -> f32 {
        bam_to_radian(self.0).cos()
    }

    #[inline]
    fn tan_f32(self) -> f32 {
        bam_to_radian(self.0).tan()
    }

    #[inline]
    fn sin_cos_f32(self) -> (f32, f32) {
        let rad = bam_to_radian(self.0);
        rad.sin_cos()
    }

    #[inline]
    fn wrap_add(self, other: Self) -> Self {
        Bam(self.0.wrapping_add(other.0))
    }

    #[inline]
    fn wrap_sub(self, other: Self) -> Self {
        Bam(self.0.wrapping_sub(other.0))
    }

    #[inline]
    fn negate(self) -> Self {
        Bam(0u32.wrapping_sub(self.0))
    }

    #[inline]
    fn scale(self, factor: f32) -> Self {
        Bam((self.0 as f64 * factor as f64) as u32)
    }

    #[inline]
    fn inv_scale(self, factor: f32) -> Self {
        Bam((self.0 as f64 / factor as f64) as u32)
    }

    #[inline]
    fn from_atan2(y: f32, x: f32) -> Self {
        Bam(radian_to_bam(y.atan2(x)))
    }

    #[inline]
    fn from_bam(bam: u32) -> Self {
        Bam(bam)
    }
}

// --- Angle<A> ---

/// Angle type generic over representation. `Angle<f32>` stores radians,
/// `Angle<Bam>` stores BAM (Binary Angle Measurement, u32).
#[derive(Debug, Default, Copy, Clone, PartialEq)]
pub struct Angle<A: AngleInner = f32>(A);

impl<A: AngleInner> Angle<A> {
    #[inline]
    pub fn new(radians: f32) -> Self {
        Self(A::from_radians(radians))
    }

    /// Construct from BAM u32. Exact for Bam backend, converted for f32.
    #[inline]
    pub fn from_bam(bam: u32) -> Self {
        Self(A::from_bam(bam))
    }

    /// Construct from a raw inner value (no conversion).
    #[inline]
    pub fn from_inner(inner: A) -> Self {
        Self(inner)
    }

    #[inline]
    pub fn inner(self) -> A {
        self.0
    }

    #[inline]
    pub fn rad(&self) -> f32 {
        self.0.to_radians()
    }

    #[inline]
    pub fn to_radians(&self) -> f32 {
        self.0.to_radians()
    }

    #[inline]
    pub fn to_bam(&self) -> u32 {
        self.0.to_bam()
    }

    #[inline]
    pub fn sin(&self) -> f32 {
        self.0.sin_f32()
    }

    #[inline]
    pub fn cos(&self) -> f32 {
        self.0.cos_f32()
    }

    #[inline]
    pub fn tan(&self) -> f32 {
        self.0.tan_f32()
    }

    #[inline]
    pub fn sin_cos(&self) -> (f32, f32) {
        self.0.sin_cos_f32()
    }

    #[inline(always)]
    pub fn unit(&self) -> Vec2 {
        let (y, x) = self.sin_cos();
        Vec2::new(x, y)
    }

    #[inline]
    pub fn from_vector(input: Vec2) -> Self {
        Self(A::from_atan2(input.y, input.x))
    }

    #[inline]
    pub fn sub_other(self, other: Angle<A>) -> Angle<A> {
        Angle(self.0.wrap_sub(other.0))
    }

    #[inline]
    pub fn convert<B: AngleInner>(&self) -> Angle<B> {
        Angle(B::from_radians(self.0.to_radians()))
    }

    /// Sine as `FixedT` via OG Doom finesine table lookup.
    #[inline]
    pub fn sin_fixedt(&self) -> FixedT {
        fine_sin(self.0.to_bam())
    }

    /// Cosine as `FixedT` via OG Doom finecosine table lookup.
    #[inline]
    pub fn cos_fixedt(&self) -> FixedT {
        fine_cos(self.0.to_bam())
    }
}

// --- Arithmetic ---

impl<A: AngleInner> Add for Angle<A> {
    type Output = Angle<A>;
    #[inline]
    fn add(self, other: Angle<A>) -> Angle<A> {
        Angle(self.0.wrap_add(other.0))
    }
}

impl<A: AngleInner> Add<f32> for Angle<A> {
    type Output = Angle<A>;
    #[inline]
    fn add(self, other: f32) -> Angle<A> {
        Angle(self.0.wrap_add(A::from_radians(other)))
    }
}

impl<A: AngleInner> AddAssign for Angle<A> {
    #[inline]
    fn add_assign(&mut self, other: Angle<A>) {
        self.0 = self.0.wrap_add(other.0);
    }
}

impl<A: AngleInner> AddAssign<f32> for Angle<A> {
    #[inline]
    fn add_assign(&mut self, other: f32) {
        self.0 = self.0.wrap_add(A::from_radians(other));
    }
}

impl<A: AngleInner> Sub for Angle<A> {
    type Output = Angle<A>;
    #[inline]
    fn sub(self, other: Angle<A>) -> Angle<A> {
        Angle(self.0.wrap_sub(other.0))
    }
}

impl<A: AngleInner> Sub<f32> for Angle<A> {
    type Output = Angle<A>;
    #[inline]
    fn sub(self, other: f32) -> Angle<A> {
        Angle(self.0.wrap_sub(A::from_radians(other)))
    }
}

impl<A: AngleInner> SubAssign for Angle<A> {
    #[inline]
    fn sub_assign(&mut self, other: Angle<A>) {
        self.0 = self.0.wrap_sub(other.0);
    }
}

impl<A: AngleInner> SubAssign<f32> for Angle<A> {
    #[inline]
    fn sub_assign(&mut self, other: f32) {
        self.0 = self.0.wrap_sub(A::from_radians(other));
    }
}

impl<A: AngleInner> Mul<f32> for Angle<A> {
    type Output = Angle<A>;
    #[inline]
    fn mul(self, other: f32) -> Angle<A> {
        Angle(self.0.scale(other))
    }
}

impl<A: AngleInner> MulAssign<f32> for Angle<A> {
    #[inline]
    fn mul_assign(&mut self, other: f32) {
        self.0 = self.0.scale(other);
    }
}

impl<A: AngleInner> Div<f32> for Angle<A> {
    type Output = Angle<A>;
    #[inline]
    fn div(self, other: f32) -> Angle<A> {
        Angle(self.0.inv_scale(other))
    }
}

impl<A: AngleInner> DivAssign<f32> for Angle<A> {
    #[inline]
    fn div_assign(&mut self, other: f32) {
        self.0 = self.0.inv_scale(other);
    }
}

impl<A: AngleInner> Neg for Angle<A> {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self::Output {
        Angle(self.0.negate())
    }
}

// --- Free functions ---

/// Compute angle from point2 to point1.
#[inline]
pub fn point_to_angle_2<A: AngleInner>(point1: (f32, f32), point2: (f32, f32)) -> Angle<A> {
    let x = point1.0 - point2.0;
    let y = point1.1 - point2.1;
    Angle(A::from_atan2(y, x))
}

// --- Helpers ---

#[inline]
fn rad_to_table(rad: f32) -> usize {
    let mut idx = (rad.to_degrees() * 22.755_556) as i32;
    idx &= 8191;
    if idx < 0 {
        idx += 8192;
    }
    idx as usize
}
