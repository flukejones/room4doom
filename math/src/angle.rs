use std::f32::consts::TAU;
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

use glam::Vec2;

use crate::doom_f32::DoomF32;
use crate::trig::{COS_TABLE, SIN_TABLE, TAN_TABLE};

#[derive(Debug, Default, Copy, Clone, PartialEq)]
pub struct Angle(f32);

impl Angle {
    /// Will always wrap < 0 to > PI
    #[inline]
    pub fn new(radians: f32) -> Self {
        let mut rad = radians;
        rad %= TAU;
        if rad < 0.0 {
            rad += TAU;
        }
        Angle(rad)
    }

    #[inline]
    fn inner_wrap(&mut self) {
        let mut rad = self.0;
        rad %= TAU;
        if rad < 0.0 {
            rad += TAU;
        }
        self.0 = rad;
    }

    #[inline]
    pub fn rad(&self) -> f32 {
        self.0
    }

    #[inline]
    fn table(&self) -> usize {
        let mut idx = (self.0.to_degrees() * 22.755_556) as i32;
        idx &= 8191;
        if idx < 0 {
            idx += 8192;
        }
        idx as usize
    }

    #[inline]
    pub fn sin(&self) -> f32 {
        if cfg!(not(feature = "trig_lut")) {
            (self.0).sin()
        } else {
            SIN_TABLE[self.table()]
        }
    }

    #[inline]
    pub fn cos(&self) -> f32 {
        if cfg!(not(feature = "trig_lut")) {
            (self.0).cos()
        } else {
            COS_TABLE[self.table()]
        }
    }

    #[inline]
    pub fn tan(&self) -> f32 {
        if cfg!(not(feature = "trig_lut")) {
            (self.0).tan()
        } else {
            TAN_TABLE[self.table()]
        }
    }

    #[inline]
    pub fn sin_cos(&self) -> (f32, f32) {
        if cfg!(not(feature = "trig_lut")) {
            let (s, c) = (self.0).sin_cos();
            ((s), (c))
        } else {
            let idx = self.table();
            ((SIN_TABLE[idx]), (COS_TABLE[idx]))
        }
    }

    #[inline(always)]
    pub fn unit_vec2(&self) -> Vec2 {
        let (y, x) = self.sin_cos();
        Vec2::new(x, y)
    }

    #[inline(always)]
    pub fn unit_xy(&self) -> (f32, f32) {
        let (y, x) = self.sin_cos();
        (x, y)
    }

    pub fn from_vector_xy(x: f32, y: f32) -> Self {
        Angle::new((y).atan2(x))
    }

    #[inline]
    pub fn sub_other(self, other: Angle) -> Angle {
        Angle::new(self.0 - other.0)
    }
}

impl Add for Angle {
    type Output = Angle;
    #[inline]
    fn add(self, other: Angle) -> Angle {
        Angle::new(self.0 + other.0)
    }
}

impl Add<f32> for Angle {
    type Output = Angle;
    #[inline]
    fn add(self, other: f32) -> Angle {
        Angle::new(self.0 + other)
    }
}

// impl Add<i32> for Angle {
//     type Output = Angle;

//
//     fn add(self, other: i32) -> Angle {
//         Angle::new(self.0 + (other as f32).to_radians())
//     }
// }

impl AddAssign for Angle {
    #[inline]
    fn add_assign(&mut self, other: Angle) {
        self.0 += other.0;
        self.inner_wrap();
    }
}

impl AddAssign<f32> for Angle {
    #[inline]
    fn add_assign(&mut self, other: f32) {
        self.0 += other;
        self.inner_wrap();
    }
}

// impl AddAssign<i32> for Angle{
//
//     fn add_assign(&mut self, other: i32) {
//         self.0 += (other as f32).to_radians();
//         self.inner_wrap();
//     }
// }

//

impl Mul for Angle {
    type Output = Angle;
    #[inline]
    fn mul(self, other: Angle) -> Angle {
        Angle::new(self.0 * other.0)
    }
}

impl Mul<f32> for Angle {
    type Output = Angle;
    #[inline]
    fn mul(self, other: f32) -> Angle {
        Angle::new(self.0 * other)
    }
}

// impl Mul<i32> for Angle {
//     type Output = Angle;

//
//     fn mul(self, other: i32) -> Angle {
//         Angle::new(self.0 * (other as f32).to_radians())
//     }
// }

impl MulAssign for Angle {
    #[inline]
    fn mul_assign(&mut self, other: Angle) {
        self.0 *= other.0;
        self.inner_wrap();
    }
}

impl MulAssign<f32> for Angle {
    #[inline]
    fn mul_assign(&mut self, other: f32) {
        self.0 *= other;
        self.inner_wrap();
    }
}

// impl MulAssign<i32> for Angle{
//
//     fn mul_assign(&mut self, other: i32) {
//         self.0 *= (other as f32).to_radians();
//         self.inner_wrap();
//     }
// }

// negatives

impl Sub for Angle {
    type Output = Angle;
    #[inline]
    fn sub(self, other: Angle) -> Angle {
        Angle::new(self.0 - other.0)
    }
}

impl Sub<f32> for Angle {
    type Output = Angle;
    #[inline]
    fn sub(self, other: f32) -> Angle {
        Angle::new(self.0 - other)
    }
}

// impl Sub<i32> for Angle {
//     type Output = Angle;

//
//     fn sub(self, other: i32) -> Angle {
//         Angle::new(self.0 - (other as f32).to_radians())
//     }
// }

impl SubAssign for Angle {
    #[inline]
    fn sub_assign(&mut self, other: Angle) {
        self.0 -= other.0;
        self.inner_wrap();
    }
}

impl SubAssign<f32> for Angle {
    #[inline]
    fn sub_assign(&mut self, other: f32) {
        self.0 -= other;
        self.inner_wrap();
    }
}

// impl SubAssign<i32> for Angle{
//
//     fn sub_assign(&mut self, other: i32) {
//         self.0 -= (other as f32).to_radians();
//         self.inner_wrap();
//     }
// }

//

impl Div for Angle {
    type Output = Angle;
    #[inline]
    fn div(self, other: Angle) -> Angle {
        Angle::new(self.0 / other.0)
    }
}

impl Div<f32> for Angle {
    type Output = Angle;
    #[inline]
    fn div(self, other: f32) -> Angle {
        Angle::new(self.0 / other)
    }
}

// impl Div<i32> for Angle {
//     type Output = Angle;

//
//     fn div(self, other: i32) -> Angle {
//         Angle::new(self.0 / (other as f32).to_radians()
//     }
// }

impl DivAssign for Angle {
    #[inline]
    fn div_assign(&mut self, other: Angle) {
        self.0 /= other.0;
        self.inner_wrap();
    }
}

impl DivAssign<f32> for Angle {
    #[inline]
    fn div_assign(&mut self, other: f32) {
        self.0 /= other;
        self.inner_wrap();
    }
}

// impl DivAssign<i32> for Angle{
//
//     fn div_assign(&mut self, other: i32) {
//         self.0 /= (other as f32).to_radians();
//         self.inner_wrap();
//     }
// }

//

impl Neg for Angle {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self::Output {
        Angle(-self.0)
    }
}

pub fn point_to_angle_2_xy(x1: DoomF32, y1: DoomF32, x2: DoomF32, y2: DoomF32) -> Angle {
    let x = x1 - x2;
    let y = y1 - y2;
    Angle::new((y).atan2(x).into())
}
