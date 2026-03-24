use math::bam::{ANG45, ANG90, ANG180, bam_to_radian, radian_to_bam};
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};

const TOL: f32 = 0.001;

#[test]
fn bam_to_rad_45() {
    let r = bam_to_radian(ANG45);
    assert!((r - FRAC_PI_4).abs() < TOL);
}

#[test]
fn bam_to_rad_90() {
    let r = bam_to_radian(ANG90);
    assert!((r - FRAC_PI_2).abs() < TOL);
}

#[test]
fn bam_to_rad_180() {
    let r = bam_to_radian(ANG180);
    assert!((r - PI).abs() < TOL);
}

#[test]
fn rad_to_bam_90() {
    let bam = radian_to_bam(FRAC_PI_2);
    let diff = (bam as i64 - ANG90 as i64).unsigned_abs();
    assert!(
        diff < 256,
        "90° BAM: got {bam}, expected {ANG90}, diff {diff}"
    );
}

#[test]
fn rad_to_bam_180() {
    let bam = radian_to_bam(PI);
    let diff = (bam as i64 - ANG180 as i64).unsigned_abs();
    assert!(
        diff < 256,
        "180° BAM: got {bam}, expected {ANG180}, diff {diff}"
    );
}

#[test]
fn roundtrip_bam_rad_bam() {
    let original = ANG45;
    let rad = bam_to_radian(original);
    let back = radian_to_bam(rad);
    let diff = (back as i64 - original as i64).unsigned_abs();
    assert!(
        diff < 256,
        "roundtrip: {original} -> {rad} -> {back}, diff {diff}"
    );
}

#[test]
fn bam_5_625_degrees() {
    let one: u32 = 1 << 26;
    let r = bam_to_radian(one);
    assert!((r.to_degrees() - 5.625).abs() < 0.01);
}
