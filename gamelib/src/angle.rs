use std::{
    f32::consts::PI,
    ops::{
        Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign,
    },
};

pub static CLASSIC_DOOM_SCREEN_XTO_VIEW: [f32; 321] = [
    45.043945, 44.824219, 44.648437, 44.472656, 44.296875, 44.121094,
    43.945312, 43.725586, 43.549805, 43.374023, 43.154297, 42.978516,
    42.802734, 42.583008, 42.407227, 42.187500, 42.011719, 41.791992,
    41.616211, 41.396484, 41.220703, 41.000977, 40.781250, 40.605469,
    40.385742, 40.166016, 39.946289, 39.770508, 39.550781, 39.331055,
    39.111328, 38.891602, 38.671875, 38.452148, 38.232422, 38.012695,
    37.792969, 37.573242, 37.353516, 37.133789, 36.870117, 36.650391,
    36.430664, 36.210937, 35.947266, 35.727539, 35.507812, 35.244141,
    35.024414, 34.760742, 34.541016, 34.277344, 34.057617, 33.793945,
    33.530273, 33.310547, 33.046875, 32.783203, 32.519531, 32.299805,
    32.036133, 31.772461, 31.508789, 31.245117, 30.981445, 30.717773,
    30.454102, 30.190430, 29.926758, 29.663086, 29.355469, 29.091797,
    28.828125, 28.564453, 28.256836, 27.993164, 27.729492, 27.421875,
    27.158203, 26.850586, 26.586914, 26.279297, 26.015625, 25.708008,
    25.444336, 25.136719, 24.829102, 24.521484, 24.257812, 23.950195,
    23.642578, 23.334961, 23.027344, 22.719727, 22.412109, 22.104492,
    21.796875, 21.489258, 21.181641, 20.874023, 20.566406, 20.258789,
    19.951172, 19.643555, 19.291992, 18.984375, 18.676758, 18.325195,
    18.017578, 17.709961, 17.358398, 17.050781, 16.699219, 16.391602,
    16.040039, 15.732422, 15.380859, 15.073242, 14.721680, 14.370117,
    14.062500, 13.710937, 13.359375, 13.051758, 12.700195, 12.348633,
    11.997070, 11.645508, 11.337891, 10.986328, 10.634766, 10.283203, 9.931641,
    9.580078, 9.228516, 8.876953, 8.525391, 8.173828, 7.822266, 7.470703,
    7.119141, 6.767578, 6.416016, 6.064453, 5.712891, 5.361328, 5.009766,
    4.658203, 4.306641, 3.955078, 3.559570, 3.208008, 2.856445, 2.504883,
    2.153320, 1.801758, 1.450195, 1.054687, 0.703125, 0.351562, 0.000000,
    359.648437, 359.296875, 358.945312, 358.549805, 358.198242, 357.846680,
    357.495117, 357.143555, 356.791992, 356.440430, 356.044922, 355.693359,
    355.341797, 354.990234, 354.638672, 354.287109, 353.935547, 353.583984,
    353.232422, 352.880859, 352.529297, 352.177734, 351.826172, 351.474609,
    351.123047, 350.771484, 350.419922, 350.068359, 349.716797, 349.365234,
    349.013672, 348.662109, 348.354492, 348.002930, 347.651367, 347.299805,
    346.948242, 346.640625, 346.289062, 345.937500, 345.629883, 345.278320,
    344.926758, 344.619141, 344.267578, 343.959961, 343.608398, 343.300781,
    342.949219, 342.641601, 342.290039, 341.982422, 341.674805, 341.323242,
    341.015625, 340.708008, 340.356445, 340.048828, 339.741211, 339.433594,
    339.125977, 338.818359, 338.510742, 338.203125, 337.895508, 337.587891,
    337.280273, 336.972656, 336.665039, 336.357422, 336.049805, 335.742187,
    335.478516, 335.170898, 334.863281, 334.555664, 334.291992, 333.984375,
    333.720703, 333.413086, 333.149414, 332.841797, 332.578125, 332.270508,
    332.006836, 331.743164, 331.435547, 331.171875, 330.908203, 330.644531,
    330.336914, 330.073242, 329.809570, 329.545898, 329.282227, 329.018555,
    328.754883, 328.491211, 328.227539, 327.963867, 327.700195, 327.480469,
    327.216797, 326.953125, 326.689453, 326.469727, 326.206055, 325.942383,
    325.722656, 325.458984, 325.239258, 324.975586, 324.755859, 324.492187,
    324.272461, 324.052734, 323.789062, 323.569336, 323.349609, 323.129883,
    322.866211, 322.646484, 322.426758, 322.207031, 321.987305, 321.767578,
    321.547852, 321.328125, 321.108398, 320.888672, 320.668945, 320.449219,
    320.229492, 320.053711, 319.833984, 319.614258, 319.394531, 319.218750,
    318.999023, 318.779297, 318.603516, 318.383789, 318.208008, 317.988281,
    317.812500, 317.592773, 317.416992, 317.197266, 317.021484, 316.845703,
    316.625977, 316.450195, 316.274414, 316.054687, 315.878906, 315.703125,
    315.527344, 315.351562, 315.175781, 314.956055,
];

#[derive(Debug, Default, Copy, Clone)]
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

    pub fn as_degrees(&self) -> i32 { (self.0 * 180.0 / PI) as i32 }

    pub fn rad(&self) -> f32 { self.0 }

    pub fn sin(&self) -> f32 { self.0.sin() }

    pub fn cos(&self) -> f32 { self.0.cos() }

    pub fn sin_cos(&self) -> (f32, f32) { self.0.sin_cos() }

    pub fn tan(&self) -> f32 { self.0.tan() }
}

impl Add for Angle {
    type Output = Angle;

    #[inline]
    fn add(self, other: Angle) -> Angle { Angle::new(self.0 + other.0) }
}

impl Add<f32> for Angle {
    type Output = Angle;

    #[inline]
    fn add(self, other: f32) -> Angle { Angle::new(self.0 + other) }
}

// impl Add<i32> for Angle {
//     type Output = Angle;

//     #[inline]
//     fn add(self, other: i32) -> Angle {
//         Angle::new(self.0 + other as f32 * PI / 180.0)
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
//         self.0 += other as f32 * PI / 180.0;
//         self.inner_wrap();
//     }
// }

//

impl Mul for Angle {
    type Output = Angle;

    #[inline]
    fn mul(self, other: Angle) -> Angle { Angle::new(self.0 * other.0) }
}

impl Mul<f32> for Angle {
    type Output = Angle;

    #[inline]
    fn mul(self, other: f32) -> Angle { Angle::new(self.0 * other) }
}

// impl Mul<i32> for Angle {
//     type Output = Angle;

//     #[inline]
//     fn mul(self, other: i32) -> Angle {
//         Angle::new(self.0 * (other as f32 * PI / 180.0))
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
//         self.0 *= other as f32 * PI / 180.0;
//         self.inner_wrap();
//     }
// }

// negatives

impl Sub for Angle {
    type Output = Angle;

    #[inline]
    fn sub(self, other: Angle) -> Angle { Angle::new(self.0 - other.0) }
}

impl Sub<f32> for Angle {
    type Output = Angle;

    #[inline]
    fn sub(self, other: f32) -> Angle { Angle::new(self.0 - other) }
}

// impl Sub<i32> for Angle {
//     type Output = Angle;

//     #[inline]
//     fn sub(self, other: i32) -> Angle {
//         Angle::new(self.0 - other as f32 * PI / 180.0)
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
//         self.0 -= other as f32 * PI / 180.0;
//         self.inner_wrap();
//     }
// }

//

impl Div for Angle {
    type Output = Angle;

    #[inline]
    fn div(self, other: Angle) -> Angle { Angle::new(self.0 / other.0) }
}

impl Div<f32> for Angle {
    type Output = Angle;

    #[inline]
    fn div(self, other: f32) -> Angle { Angle::new(self.0 / other) }
}

// impl Div<i32> for Angle {
//     type Output = Angle;

//     #[inline]
//     fn div(self, other: i32) -> Angle {
//         Angle::new(self.0 / (other as f32 * PI / 180.0))
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
//         self.0 /= other as f32 * PI / 180.0;
//         self.inner_wrap();
//     }
// }

//

impl Neg for Angle {
    type Output = Self;

    fn neg(self) -> Self::Output { Angle(-self.0) }
}
