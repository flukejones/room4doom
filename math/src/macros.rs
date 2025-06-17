#[macro_export]
macro_rules! fixed {
    ($val:expr) => {
        $crate::FixedPoint::from($val)
    };
}

#[macro_export]
macro_rules! fconst {
    (PI) => {
        $crate::FixedPoint::PI
    };
    (TAU) => {
        $crate::FixedPoint::TAU
    };
    (E) => {
        $crate::FixedPoint::E
    };
    (FRAC_PI_2) => {
        $crate::FixedPoint::FRAC_PI_2
    };
    (FRAC_PI_3) => {
        $crate::FixedPoint::FRAC_PI_3
    };
    (FRAC_PI_4) => {
        $crate::FixedPoint::FRAC_PI_4
    };
    (FRAC_PI_6) => {
        $crate::FixedPoint::FRAC_PI_6
    };
    (FRAC_PI_8) => {
        $crate::FixedPoint::FRAC_PI_8
    };
    (FRAC_1_PI) => {
        $crate::FixedPoint::FRAC_1_PI
    };
    (FRAC_2_PI) => {
        $crate::FixedPoint::FRAC_2_PI
    };
    (FRAC_2_SQRT_PI) => {
        $crate::FixedPoint::FRAC_2_SQRT_PI
    };
    (SQRT_2) => {
        $crate::FixedPoint::SQRT_2
    };
    (FRAC_1_SQRT_2) => {
        $crate::FixedPoint::FRAC_1_SQRT_2
    };
    (LOG2_E) => {
        $crate::FixedPoint::LOG2_E
    };
    (LOG10_E) => {
        $crate::FixedPoint::LOG10_E
    };
    (LN_2) => {
        $crate::FixedPoint::LN_2
    };
    (LN_10) => {
        $crate::FixedPoint::LN_10
    };
}

#[macro_export]
macro_rules! v2f {
    ($fvec:expr) => {
        glam::Vec2::from($fvec)
    };
}
