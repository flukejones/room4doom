use math::fixed_point::{FRACBITS, FRACUNIT, FixedT, Inner, p_aprox_distance};

#[test]
fn one_times_one() {
    assert_eq!(FixedT::ONE.fixed_mul(FixedT::ONE), FixedT::ONE);
}

#[test]
fn fixed_mul_half() {
    let half = FixedT(FRACUNIT / 2);
    assert_eq!(half.fixed_mul(half), FixedT(FRACUNIT / 4));
}

#[test]
fn fixed_div_identity() {
    let two = FixedT::from(2i32);
    let result = two.fixed_div(FixedT::ONE);
    assert_eq!(result, two);
}

#[test]
fn fixed_div_by_zero_returns_max() {
    let pos = FixedT(FRACUNIT);
    assert_eq!(pos.fixed_div(FixedT::ZERO), FixedT::MAX);
}

#[test]
fn fixed_div_overflow_returns_max() {
    let big = FixedT(Inner::MAX);
    let small = FixedT(1);
    assert_eq!(big.fixed_div(small), FixedT::MAX);
}

#[test]
fn fixed_div_opposite_signs_returns_min() {
    let pos = FixedT(Inner::MAX);
    let neg = FixedT(-1);
    assert_eq!(pos.fixed_div(neg), FixedT::MIN);
}

#[test]
fn add_wraps() {
    let a = FixedT(Inner::MAX);
    let b = FixedT(1);
    assert_eq!(a + b, FixedT(Inner::MIN));
}

#[test]
fn sub_wraps() {
    let a = FixedT(Inner::MIN);
    let b = FixedT(1);
    assert_eq!(a - b, FixedT(Inner::MAX));
}

#[test]
fn neg_wraps_min() {
    assert_eq!(-FixedT(Inner::MIN), FixedT(Inner::MIN));
}

#[test]
fn from_i32_shifts_left() {
    let f = FixedT::from(1i32);
    assert_eq!(f.0, FRACUNIT);
}

#[test]
fn to_i32_shifts_right() {
    assert_eq!(FixedT::ONE.to_i32(), 1);
}

#[test]
fn from_f32_roundtrip() {
    let original = 3.5f32;
    let f = FixedT::from_f32(original);
    let back = f.to_f32();
    assert!((back - original).abs() < 1e-4, "got {back}");
}

#[test]
fn shr() {
    let v = FixedT::from(8i32);
    assert_eq!(v.shr(1), FixedT::from(4i32));
}

#[test]
fn half_toward_zero_positive() {
    let v = FixedT(7);
    assert_eq!(v.half_toward_zero(), FixedT(3));
}

#[test]
fn half_toward_zero_negative() {
    let v = FixedT(-7);
    assert_eq!(v.half_toward_zero(), FixedT(-3));
}

#[test]
fn clamp() {
    let lo = FixedT::from(1i32);
    let hi = FixedT::from(10i32);
    assert_eq!(FixedT::from(5i32).clamp(lo, hi), FixedT::from(5i32));
    assert_eq!(FixedT::from(0i32).clamp(lo, hi), lo);
    assert_eq!(FixedT::from(20i32).clamp(lo, hi), hi);
}

#[test]
fn doom_abs() {
    let neg = FixedT::from_fixed(-5 << 16);
    let pos = FixedT::from_fixed(5 << 16);
    assert_eq!(neg.doom_abs(), pos);
    assert_eq!(pos.doom_abs(), pos);
}

#[test]
fn from_fixed_roundtrip() {
    let raw: i32 = 0x00030000; // 3.0 in 16.16
    let f = FixedT::from_fixed(raw);
    assert_eq!(f.to_fixed_raw(), raw);
}

#[test]
fn p_aprox_distance_basic() {
    let dx = FixedT::from(3i32);
    let dy = FixedT::from(4i32);
    let dist = p_aprox_distance(dx, dy);
    // OG formula: max + min/2 = 4 + 1.5 = 5.5
    assert!((dist.to_f32() - 5.5).abs() < 0.01);
}

#[test]
fn mixed_add_i32() {
    let a = FixedT::from_fixed(1 << 16);
    let result = a + 2_i32;
    assert!((result.to_f32() - 3.0).abs() < 0.001);
}

#[test]
fn mixed_sub_i32() {
    let a = FixedT::from_fixed(5 << 16);
    let result = a - 2_i32;
    assert!((result.to_f32() - 3.0).abs() < 0.001);
}

#[test]
fn mixed_add_f32() {
    let a = FixedT::from_fixed(1 << 16);
    let result = a + 2.0_f32;
    assert!((result.to_f32() - 3.0).abs() < 0.001);
}

#[test]
fn mixed_mul_f32() {
    let a = FixedT::from_fixed(3 << 16);
    let result = a * 2.0_f32;
    assert!((result.to_f32() - 6.0).abs() < 0.01);
}

#[test]
fn is_zero() {
    assert!(FixedT::ZERO.is_zero());
    assert!(!FixedT::ONE.is_zero());
}

#[test]
fn is_negative() {
    assert!(FixedT(-1).is_negative());
    assert!(!FixedT(0).is_negative());
    assert!(!FixedT(1).is_negative());
}

#[test]
fn fracbits_consistent() {
    assert_eq!(FRACUNIT, 1 << FRACBITS);
}

#[test]
fn partial_eq_i32() {
    assert!(FixedT::from(5i32) == 5_i32);
    assert!(FixedT::from(5i32) != 6_i32);
}

#[test]
fn partial_ord_i32() {
    assert!(FixedT::from(5i32) > 4_i32);
    assert!(FixedT::from(5i32) < 6_i32);
}

#[test]
fn partial_eq_f32() {
    let a = FixedT::from_f32(5.0);
    assert!(a == 5.0_f32);
}

#[test]
fn partial_ord_f32() {
    let a = FixedT::from_f32(5.0);
    assert!(a > 4.0_f32);
    assert!(a < 6.0_f32);
}

#[test]
fn reverse_sub_i32() {
    let a = FixedT::from(3i32);
    let result = 10_i32 - a;
    assert!((result.to_f32() - 7.0).abs() < 0.001);
}

#[test]
fn reverse_div_i32() {
    let a = FixedT::from(4i32);
    let result = 20_i32 / a;
    assert!((result.to_f32() - 5.0).abs() < 0.001);
}

#[test]
fn reverse_div_f32() {
    let a = FixedT::from_f32(4.0);
    let result = 20.0_f32 / a;
    assert!((result.to_f32() - 5.0).abs() < 0.01);
}
