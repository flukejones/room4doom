use glam::Vec2;
use std::f32::consts::{PI, TAU};
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

use crate::trig::{COS_TABLE, SIN_TABLE, TAN_TABLE};

#[derive(Debug, Default, Copy, Clone, PartialEq)]
pub struct Angle(f32);

impl Angle {
    /// Will always wrap < 0 to > PI
    #[inline(always)]
    pub const fn new(mut radians: f32) -> Self {
        radians = radians % TAU;
        if radians < 0.0 {
            radians += TAU;
        }
        Angle(radians)
    }

    #[inline(always)]
    const fn inner_wrap(&mut self) {
        self.0 = self.0 % TAU;
        if self.0 < 0.0 {
            self.0 += TAU;
        }
    }

    #[inline(always)]
    pub const fn rad(&self) -> f32 {
        self.0
    }

    #[inline(always)]
    const fn to_table(&self) -> usize {
        let mut idx = (self.0.to_degrees() * 22.7555556) as i32;
        idx &= 8191;
        if idx < 0 {
            idx += 8192;
        }
        idx as usize
    }

    #[inline(always)]
    pub const fn sin(&self) -> f32 {
        // self.0.sin()
        SIN_TABLE[self.to_table()]
    }

    #[inline(always)]
    pub const fn cos(&self) -> f32 {
        // self.0.cos()
        COS_TABLE[self.to_table()]
    }

    #[inline(always)]
    pub const fn tan(&self) -> f32 {
        // self.0.tan()
        TAN_TABLE[self.to_table()]
    }

    #[inline(always)]
    pub const fn sin_cos(&self) -> (f32, f32) {
        let idx = self.to_table();
        (SIN_TABLE[idx], COS_TABLE[idx])
    }

    #[inline(always)]
    pub const fn unit(&self) -> Vec2 {
        let (y, x) = self.sin_cos();
        Vec2::new(x, y)
    }

    pub fn from_vector(input: Vec2) -> Self {
        Angle::new(input.y.atan2(input.x))
    }

    #[inline(always)]
    pub const fn sub_other(self, other: Angle) -> Angle {
        Angle::new(self.0 - other.0)
    }
}

impl Add for Angle {
    type Output = Angle;

    fn add(self, other: Angle) -> Angle {
        Angle::new(self.0 + other.0)
    }
}

impl Add<f32> for Angle {
    type Output = Angle;

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
    fn add_assign(&mut self, other: Angle) {
        self.0 += other.0;
        self.inner_wrap();
    }
}

impl AddAssign<f32> for Angle {
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

    fn mul(self, other: Angle) -> Angle {
        Angle::new(self.0 * other.0)
    }
}

impl Mul<f32> for Angle {
    type Output = Angle;

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
    fn mul_assign(&mut self, other: Angle) {
        self.0 *= other.0;
        self.inner_wrap();
    }
}

impl MulAssign<f32> for Angle {
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

    fn sub(self, other: Angle) -> Angle {
        Angle::new(self.0 - other.0)
    }
}

impl Sub<f32> for Angle {
    type Output = Angle;

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
    fn sub_assign(&mut self, other: Angle) {
        self.0 -= other.0;
        self.inner_wrap();
    }
}

impl SubAssign<f32> for Angle {
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

    fn div(self, other: Angle) -> Angle {
        Angle::new(self.0 / other.0)
    }
}

impl Div<f32> for Angle {
    type Output = Angle;

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
    fn div_assign(&mut self, other: Angle) {
        self.0 /= other.0;
        self.inner_wrap();
    }
}

impl DivAssign<f32> for Angle {
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

    fn neg(self) -> Self::Output {
        Angle(-self.0)
    }
}
