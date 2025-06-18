#[cfg(test)]
mod tests {
    use crate::{DoomF32, MAX, MIN, NEG_ONE, ONE, ZERO, doom_f32, from_f32, to_f32};

    #[test]
    fn test_numeric_basic_operations() {
        let a = doom_f32!(5.0);
        let b = doom_f32!(3.0);

        let sum = a + b;
        let diff = a - b;
        let product = a * b;
        let quotient = a / b;

        // Basic sanity checks
        assert!(sum > a);
        assert!(diff > doom_f32!(0.0));
        assert!(product > a);
        assert!(quotient > doom_f32!(1.0));
    }

    #[test]
    fn test_numeric_conversions() {
        let val = 42.5f32;
        let numeric_val = from_f32(val);
        let back_to_f32 = to_f32(numeric_val);

        // Should be close to original value
        let diff = (val - back_to_f32).abs();
        assert!(diff < 0.01, "Conversion error too large: {}", diff);
    }

    #[test]
    fn test_numeric_constants() {
        assert!(ONE > ZERO);
        assert!(ZERO > NEG_ONE);
        assert!(MAX > ONE);
        assert!(MIN < NEG_ONE);
    }

    #[cfg(feature = "fixed_point")]
    #[test]
    fn test_fixed_point_feature() {
        let val = doom_f32!(1.0);
        // When fixed_point feature is enabled, DoomF32 should be FixedPoint
        assert_eq!(
            std::any::type_name::<DoomF32>(),
            "math::fixed_point::FixedPoint"
        );
    }

    #[cfg(not(feature = "fixed_point"))]
    #[test]
    fn test_f32_feature() {
        // let val = doom_f32!(1.0);
        // When fixed_point feature is disabled, DoomF32 should be f32
        assert_eq!(std::any::type_name::<DoomF32>(), "f32");
    }
}
