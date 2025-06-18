#[cfg(feature = "fixed_point")]
pub use crate::FixedPoint as DoomF32;

#[cfg(not(feature = "fixed_point"))]
pub type DoomF32 = f32;

#[cfg(feature = "fixed_point")]
pub const ZERO: DoomF32 = crate::FixedPoint::zero();
#[cfg(feature = "fixed_point")]
pub const ONE: DoomF32 = crate::FixedPoint::unit();
#[cfg(feature = "fixed_point")]
pub const NEG_ONE: DoomF32 = crate::FixedPoint::neg_unit();
#[cfg(feature = "fixed_point")]
pub const MAX: DoomF32 = crate::FixedPoint::max_value();
#[cfg(feature = "fixed_point")]
pub const MIN: DoomF32 = crate::FixedPoint::min_value();

#[cfg(not(feature = "fixed_point"))]
pub const ZERO: DoomF32 = 0.0;
#[cfg(not(feature = "fixed_point"))]
pub const ONE: DoomF32 = 1.0;
#[cfg(not(feature = "fixed_point"))]
pub const NEG_ONE: DoomF32 = -1.0;
#[cfg(not(feature = "fixed_point"))]
pub const MAX: DoomF32 = f32::MAX;
#[cfg(not(feature = "fixed_point"))]
pub const MIN: DoomF32 = f32::MIN;

#[macro_export]
macro_rules! doom_f32 {
    ($val:expr) => {{
        #[cfg(feature = "fixed_point")]
        {
            $crate::FixedPoint::new($crate::float_to_fixed($val))
        }
        #[cfg(not(feature = "fixed_point"))]
        {
            $val
        }
    }};
}

#[cfg(feature = "fixed_point")]
pub fn from_f32(val: f32) -> DoomF32 {
    crate::FixedPoint::from(val)
}

#[cfg(feature = "fixed_point")]
pub const fn from_i32(val: i32) -> DoomF32 {
    crate::FixedPoint::from_raw(val)
}

#[cfg(not(feature = "fixed_point"))]
pub fn from_i32(val: i32) -> DoomF32 {
    val
}

#[cfg(not(feature = "fixed_point"))]
pub fn from_f32(val: f32) -> DoomF32 {
    val
}

#[cfg(feature = "fixed_point")]
pub fn to_f32(val: DoomF32) -> f32 {
    f32::from(val)
}

#[cfg(not(feature = "fixed_point"))]
pub fn to_f32(val: DoomF32) -> f32 {
    val
}
