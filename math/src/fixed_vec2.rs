use crate::{FixedPoint, fixed_to_float};
use glam::Vec2;
use std::ops::*;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct FixedVec2 {
    pub x: FixedPoint,
    pub y: FixedPoint,
}

impl FixedVec2 {
    pub const ZERO: Self = Self::new(FixedPoint::zero(), FixedPoint::zero());
    pub const ONE: Self = Self::new(FixedPoint::unit(), FixedPoint::unit());
    pub const NEG_ONE: Self = Self::new(FixedPoint::neg_unit(), FixedPoint::neg_unit());
    pub const X: Self = Self::new(FixedPoint::unit(), FixedPoint::zero());
    pub const Y: Self = Self::new(FixedPoint::zero(), FixedPoint::unit());
    pub const NEG_X: Self = Self::new(FixedPoint::neg_unit(), FixedPoint::zero());
    pub const NEG_Y: Self = Self::new(FixedPoint::zero(), FixedPoint::neg_unit());

    pub const fn new(x: FixedPoint, y: FixedPoint) -> Self {
        Self { x, y }
    }

    pub const fn splat(v: FixedPoint) -> Self {
        Self { x: v, y: v }
    }

    pub fn from_array(a: [FixedPoint; 2]) -> Self {
        Self { x: a[0], y: a[1] }
    }

    pub fn to_array(self) -> [FixedPoint; 2] {
        [self.x, self.y]
    }

    pub fn with_x(self, x: FixedPoint) -> Self {
        Self { x, y: self.y }
    }

    pub fn with_y(self, y: FixedPoint) -> Self {
        Self { x: self.x, y }
    }

    pub fn dot(self, rhs: Self) -> FixedPoint {
        self.x * rhs.x + self.y * rhs.y
    }

    pub fn min(self, rhs: Self) -> Self {
        Self {
            x: self.x.min(rhs.x),
            y: self.y.min(rhs.y),
        }
    }

    pub fn max(self, rhs: Self) -> Self {
        Self {
            x: self.x.max(rhs.x),
            y: self.y.max(rhs.y),
        }
    }

    pub fn clamp(self, min: Self, max: Self) -> Self {
        Self {
            x: self.x.clamp(min.x, max.x),
            y: self.y.clamp(min.y, max.y),
        }
    }

    pub fn min_element(self) -> FixedPoint {
        self.x.min(self.y)
    }

    pub fn max_element(self) -> FixedPoint {
        self.x.max(self.y)
    }

    pub fn element_sum(self) -> FixedPoint {
        self.x + self.y
    }

    pub fn element_product(self) -> FixedPoint {
        self.x * self.y
    }

    pub fn abs(self) -> Self {
        Self {
            x: self.x.abs(),
            y: self.y.abs(),
        }
    }

    pub fn signum(self) -> Self {
        Self {
            x: self.x.signum(),
            y: self.y.signum(),
        }
    }

    pub fn copysign(self, sign: Self) -> Self {
        Self {
            x: self.x.copysign(sign.x),
            y: self.y.copysign(sign.y),
        }
    }

    pub fn length_squared(self) -> FixedPoint {
        self.dot(self)
    }

    pub fn length(self) -> FixedPoint {
        self.length_squared().sqrt()
    }

    pub fn length_recip(self) -> FixedPoint {
        self.length().recip()
    }

    pub fn distance(self, rhs: Self) -> FixedPoint {
        (self - rhs).length()
    }

    pub fn distance_squared(self, rhs: Self) -> FixedPoint {
        (self - rhs).length_squared()
    }

    pub fn normalize(self) -> Self {
        let len = self.length();
        if len == FixedPoint::zero() {
            Self::ZERO
        } else {
            self / len
        }
    }

    pub fn try_normalize(self) -> Option<Self> {
        let len = self.length();
        if len == FixedPoint::zero() {
            None
        } else {
            Some(self / len)
        }
    }

    pub fn normalize_or(self, fallback: Self) -> Self {
        self.try_normalize().unwrap_or(fallback)
    }

    pub fn normalize_or_zero(self) -> Self {
        self.normalize_or(Self::ZERO)
    }

    pub fn is_normalized(self) -> bool {
        let len_sq = self.length_squared();
        (len_sq - FixedPoint::unit()).abs() < FixedPoint::from(0.001)
    }

    pub fn project_onto(self, rhs: Self) -> Self {
        let scalar = self.dot(rhs) / rhs.dot(rhs);
        rhs * scalar
    }

    pub fn reject_from(self, rhs: Self) -> Self {
        self - self.project_onto(rhs)
    }

    pub fn project_onto_normalized(self, rhs: Self) -> Self {
        rhs * self.dot(rhs)
    }

    pub fn reject_from_normalized(self, rhs: Self) -> Self {
        self - self.project_onto_normalized(rhs)
    }

    pub fn round(self) -> Self {
        Self {
            x: self.x.round(),
            y: self.y.round(),
        }
    }

    pub fn floor(self) -> Self {
        Self {
            x: self.x.floor(),
            y: self.y.floor(),
        }
    }

    pub fn ceil(self) -> Self {
        Self {
            x: self.x.ceil(),
            y: self.y.ceil(),
        }
    }

    pub fn fract(self) -> Self {
        Self {
            x: self.x.fract(),
            y: self.y.fract(),
        }
    }

    pub fn lerp(self, rhs: Self, s: FixedPoint) -> Self {
        self + ((rhs - self) * s)
    }

    pub fn move_towards(self, target: Self, max_distance: FixedPoint) -> Self {
        let diff = target - self;
        let distance = diff.length();
        if distance <= max_distance || distance == FixedPoint::zero() {
            target
        } else {
            self + diff * (max_distance / distance)
        }
    }

    pub fn midpoint(self, rhs: Self) -> Self {
        (self + rhs) * FixedPoint::from(0.5)
    }

    pub fn clamp_length(self, min_length: FixedPoint, max_length: FixedPoint) -> Self {
        let len_sq = self.length_squared();
        let len = len_sq.sqrt();
        if len < min_length {
            self * (min_length / len)
        } else if len > max_length {
            self * (max_length / len)
        } else {
            self
        }
    }

    pub fn clamp_length_max(self, max_length: FixedPoint) -> Self {
        let len_sq = self.length_squared();
        if len_sq > max_length * max_length {
            self * (max_length / len_sq.sqrt())
        } else {
            self
        }
    }

    pub fn clamp_length_min(self, min_length: FixedPoint) -> Self {
        let len_sq = self.length_squared();
        if len_sq < min_length * min_length {
            self * (min_length / len_sq.sqrt())
        } else {
            self
        }
    }

    pub fn mul_add(self, a: Self, b: Self) -> Self {
        Self {
            x: self.x.mul_add(a.x, b.x),
            y: self.y.mul_add(a.y, b.y),
        }
    }

    pub fn reflect(self, n: Self) -> Self {
        self - n * (FixedPoint::from(2.0) * self.dot(n))
    }

    pub fn angle_between(self, rhs: Self) -> FixedPoint {
        let cos_theta = self.dot(rhs) / (self.length() * rhs.length());
        FixedPoint::from(fixed_to_float(cos_theta.raw()).acos())
    }

    pub fn perp(self) -> Self {
        Self {
            x: -self.y,
            y: self.x,
        }
    }

    pub fn perp_dot(self, rhs: Self) -> FixedPoint {
        self.x * rhs.y - self.y * rhs.x
    }

    pub fn rotate(self, angle: FixedPoint) -> Self {
        let cos_a = angle.cos();
        let sin_a = angle.sin();
        Self {
            x: self.x * cos_a - self.y * sin_a,
            y: self.x * sin_a + self.y * cos_a,
        }
    }

    pub fn from_angle(angle: FixedPoint) -> Self {
        Self {
            x: angle.cos(),
            y: angle.sin(),
        }
    }

    pub fn to_angle(self) -> FixedPoint {
        FixedPoint::from(fixed_to_float(self.y.raw()).atan2(fixed_to_float(self.x.raw())))
    }
}

impl Add for FixedVec2 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}

impl AddAssign for FixedVec2 {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl Sub for FixedVec2 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

impl SubAssign for FixedVec2 {
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

impl Mul for FixedVec2 {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        Self {
            x: self.x * rhs.x,
            y: self.y * rhs.y,
        }
    }
}

impl MulAssign for FixedVec2 {
    fn mul_assign(&mut self, rhs: Self) {
        *self = *self * rhs;
    }
}

impl Mul<FixedPoint> for FixedVec2 {
    type Output = Self;
    fn mul(self, rhs: FixedPoint) -> Self {
        Self {
            x: self.x * rhs,
            y: self.y * rhs,
        }
    }
}

impl MulAssign<FixedPoint> for FixedVec2 {
    fn mul_assign(&mut self, rhs: FixedPoint) {
        *self = *self * rhs;
    }
}

impl Mul<FixedVec2> for FixedPoint {
    type Output = FixedVec2;
    fn mul(self, rhs: FixedVec2) -> FixedVec2 {
        rhs * self
    }
}

impl Div for FixedVec2 {
    type Output = Self;
    fn div(self, rhs: Self) -> Self {
        Self {
            x: self.x / rhs.x,
            y: self.y / rhs.y,
        }
    }
}

impl DivAssign for FixedVec2 {
    fn div_assign(&mut self, rhs: Self) {
        *self = *self / rhs;
    }
}

impl Div<FixedPoint> for FixedVec2 {
    type Output = Self;
    fn div(self, rhs: FixedPoint) -> Self {
        Self {
            x: self.x / rhs,
            y: self.y / rhs,
        }
    }
}

impl DivAssign<FixedPoint> for FixedVec2 {
    fn div_assign(&mut self, rhs: FixedPoint) {
        *self = *self / rhs;
    }
}

impl Neg for FixedVec2 {
    type Output = Self;
    fn neg(self) -> Self {
        Self {
            x: -self.x,
            y: -self.y,
        }
    }
}

impl Index<usize> for FixedVec2 {
    type Output = FixedPoint;
    fn index(&self, index: usize) -> &Self::Output {
        match index {
            0 => &self.x,
            1 => &self.y,
            _ => panic!("index out of bounds"),
        }
    }
}

impl IndexMut<usize> for FixedVec2 {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        match index {
            0 => &mut self.x,
            1 => &mut self.y,
            _ => panic!("index out of bounds"),
        }
    }
}

impl From<[FixedPoint; 2]> for FixedVec2 {
    fn from(v: [FixedPoint; 2]) -> Self {
        Self::from_array(v)
    }
}

impl From<FixedVec2> for [FixedPoint; 2] {
    fn from(v: FixedVec2) -> Self {
        v.to_array()
    }
}

impl From<(FixedPoint, FixedPoint)> for FixedVec2 {
    fn from((x, y): (FixedPoint, FixedPoint)) -> Self {
        Self::new(x, y)
    }
}

impl From<FixedVec2> for (FixedPoint, FixedPoint) {
    fn from(v: FixedVec2) -> Self {
        (v.x, v.y)
    }
}

impl From<Vec2> for FixedVec2 {
    fn from(v: Vec2) -> Self {
        Self {
            x: FixedPoint::from(v.x),
            y: FixedPoint::from(v.y),
        }
    }
}

impl From<FixedVec2> for Vec2 {
    fn from(v: FixedVec2) -> Self {
        Self {
            x: v.x.into(),
            y: v.y.into(),
        }
    }
}

impl std::fmt::Display for FixedVec2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}, {})", self.x, self.y)
    }
}

impl std::fmt::Debug for FixedVec2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "FixedVec2({}, {})", self.x, self.y)
    }
}
