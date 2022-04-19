use glam::Vec2;
use std::{
    f32::consts::PI,
    ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign},
};

#[derive(Debug, Default, Copy, Clone, PartialEq)]
pub struct Angle(f32);

impl Angle {
    /// Will always wrap < 0 to > PI
    pub fn new(angle: f32) -> Self {
        let mut a = Angle(angle);
        a.inner_wrap();
        a
    }

    fn inner_wrap(&mut self) {
        if self.0 < 0.0 {
            self.0 += 2.0 * PI;
        } else if self.0 >= 2.0 * PI {
            self.0 -= 2.0 * PI;
        }
    }

    //pub fn as_degrees(&self) -> i32 { (self.0 * 180.0 / PI) as i32 }

    pub fn rad(&self) -> f32 {
        self.0
    }

    pub fn sin(&self) -> f32 {
        self.0.sin()
    }

    pub fn cos(&self) -> f32 {
        self.0.cos()
    }

    pub fn tan(&self) -> f32 {
        self.0.tan()
    }

    pub fn unit(&self) -> Vec2 {
        let (y, x) = self.0.sin_cos();
        Vec2::new(x, y)
    }

    //pub fn tan(&self) -> f32 { self.0.tan() }

    pub fn from_vector(input: Vec2) -> Self {
        Angle::new(input.y.atan2(input.x))
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

//     #[inline]
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
//     #[inline]
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

//     #[inline]
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
//     #[inline]
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

//     #[inline]
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
//     #[inline]
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

//     #[inline]
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
//     #[inline]
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
