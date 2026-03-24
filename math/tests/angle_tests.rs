use math::angle::{Angle, point_to_angle_2};
use math::bam::{ANG45, ANG90, ANG180, Bam};
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};

const TOL: f32 = 0.01;

// --- Angle<f32> ---

#[test]
fn f32_new_wraps_negative() {
    let a = Angle::<f32>::new(-PI);
    assert!((a.rad() - PI).abs() < TOL);
}

#[test]
fn f32_sin_cos_90() {
    let a = Angle::<f32>::new(FRAC_PI_2);
    assert!((a.sin() - 1.0).abs() < TOL);
    assert!(a.cos().abs() < TOL);
}

#[test]
fn f32_add_sub() {
    let a = Angle::<f32>::new(FRAC_PI_4);
    let b = Angle::<f32>::new(FRAC_PI_4);
    let sum = a + b;
    assert!((sum.rad() - FRAC_PI_2).abs() < TOL);

    let diff = sum - a;
    assert!((diff.rad() - FRAC_PI_4).abs() < TOL);
}

#[test]
fn f32_mul_div() {
    let a = Angle::<f32>::new(FRAC_PI_4);
    let doubled = a * 2.0;
    assert!((doubled.rad() - FRAC_PI_2).abs() < TOL);

    let halved = doubled / 2.0;
    assert!((halved.rad() - FRAC_PI_4).abs() < TOL);
}

#[test]
fn f32_neg() {
    let a = Angle::<f32>::new(FRAC_PI_4);
    let neg = -a;
    // Negation wraps: -PI/4 + TAU
    let expected = std::f32::consts::TAU - FRAC_PI_4;
    assert!((neg.rad() - expected).abs() < TOL);
}

#[test]
fn f32_to_bam() {
    let a = Angle::<f32>::new(FRAC_PI_2);
    let bam = a.to_bam();
    let diff = (bam as i64 - ANG90 as i64).unsigned_abs();
    assert!(
        diff < 256,
        "90° BAM: got {bam}, expected {ANG90}, diff {diff}"
    );
}

#[test]
fn f32_from_bam() {
    let a = Angle::<f32>::from_bam(ANG90);
    assert!((a.rad() - FRAC_PI_2).abs() < TOL);
}

// --- Angle<Bam> ---

#[test]
fn bam_from_bam_identity() {
    let a = Angle::<Bam>::from_bam(ANG45);
    assert_eq!(a.to_bam(), ANG45);
}

#[test]
fn bam_add_wraps() {
    let a = Angle::<Bam>::from_bam(ANG180);
    let b = Angle::<Bam>::from_bam(ANG180);
    let sum = a + b;
    assert_eq!(sum.to_bam(), 0); // wraps to 0
}

#[test]
fn bam_sub() {
    let a = Angle::<Bam>::from_bam(ANG90);
    let b = Angle::<Bam>::from_bam(ANG45);
    let diff = a - b;
    assert_eq!(diff.to_bam(), ANG45);
}

#[test]
fn bam_neg() {
    let a = Angle::<Bam>::from_bam(ANG90);
    let neg = -a;
    assert_eq!(neg.to_bam(), 0u32.wrapping_sub(ANG90));
}

#[test]
fn bam_to_radians() {
    let a = Angle::<Bam>::from_bam(ANG90);
    assert!((a.to_radians() - FRAC_PI_2).abs() < TOL);
}

#[test]
fn bam_sin_cos() {
    let a = Angle::<Bam>::from_bam(ANG90);
    assert!((a.sin() - 1.0).abs() < TOL);
    assert!(a.cos().abs() < TOL);
}

// --- sin_fixedt / cos_fixedt ---

#[test]
fn sin_fixedt_90() {
    let a = Angle::<Bam>::from_bam(ANG90);
    let s = a.sin_fixedt();
    assert!((s.to_f32() - 1.0).abs() < 0.001);
}

#[test]
fn cos_fixedt_0() {
    let a = Angle::<Bam>::from_bam(0);
    let c = a.cos_fixedt();
    assert!((c.to_f32() - 1.0).abs() < 0.001);
}

#[test]
fn sin_fixedt_f32_angle() {
    let a = Angle::<f32>::new(FRAC_PI_2);
    let s = a.sin_fixedt();
    assert!((s.to_f32() - 1.0).abs() < 0.001);
}

// --- convert ---

#[test]
fn convert_f32_to_bam() {
    let f = Angle::<f32>::new(FRAC_PI_2);
    let b: Angle<Bam> = f.convert();
    let diff = (b.to_bam() as i64 - ANG90 as i64).unsigned_abs();
    assert!(diff < 256);
}

#[test]
fn convert_bam_to_f32() {
    let b = Angle::<Bam>::from_bam(ANG45);
    let f: Angle<f32> = b.convert();
    assert!((f.rad() - FRAC_PI_4).abs() < TOL);
}

// --- point_to_angle_2 ---

#[test]
fn point_to_angle_2_east() {
    let a: Angle<f32> = point_to_angle_2((10.0, 0.0), (0.0, 0.0));
    assert!(a.rad().abs() < TOL); // 0 radians = east
}

#[test]
fn point_to_angle_2_north() {
    let a: Angle<f32> = point_to_angle_2((0.0, 10.0), (0.0, 0.0));
    assert!((a.rad() - FRAC_PI_2).abs() < TOL);
}

// --- unit vector ---

#[test]
fn unit_vector_east() {
    let a = Angle::<f32>::new(0.0);
    let u = a.unit();
    assert!((u.x - 1.0).abs() < TOL);
    assert!(u.y.abs() < TOL);
}
