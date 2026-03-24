use math::bam::{ANG90, ANG180};
use math::doom_trig::{FINESINE, FINETANGENT, fine_cos, fine_sin, fine_tan};

const TOLERANCE: f64 = 0.00002;

/// Tables are always generated at 16.16 scale regardless of runtime FRACBITS.
const TABLE_FRACUNIT: f64 = 65536.0;

#[test]
fn finesine_table_length() {
    assert_eq!(FINESINE.len(), 10240);
}

#[test]
fn finetangent_table_length() {
    assert_eq!(FINETANGENT.len(), 4096);
}

#[test]
fn finesine_accuracy_all_entries() {
    use std::f64::consts::TAU;
    for i in 0..8192usize {
        let expected = ((i as f64 + 0.5) * TAU / 8192.0).sin();
        let got = FINESINE[i] as f64 / TABLE_FRACUNIT;
        let err = (got - expected).abs();
        assert!(
            err < TOLERANCE,
            "FINESINE[{i}]: expected {expected:.6} got {got:.6} err {err:.8}"
        );
    }
}

#[test]
fn cosine_overlap() {
    // entries 8192..10240 should approximately equal 0..2048.
    // OG Doom tables have 12 entries that differ by ±1 due to
    // independent rounding in the original table generator.
    for i in 0..2048 {
        let diff = (FINESINE[i + 8192] - FINESINE[i]).abs();
        assert!(
            diff <= 1,
            "cosine overlap mismatch at index {i}: diff {diff}"
        );
    }
}

#[test]
fn fine_sin_zero() {
    let s: f32 = fine_sin(0).into();
    assert!(s.abs() < 0.001, "sin(0) ~ 0, got {s}");
}

#[test]
fn fine_sin_ang90() {
    let s: f32 = fine_sin(ANG90).into();
    assert!((s - 1.0).abs() < 0.001, "sin(90°) ~ 1, got {s}");
}

#[test]
fn fine_cos_zero() {
    let c: f32 = fine_cos(0).into();
    assert!((c - 1.0).abs() < 0.001, "cos(0) ~ 1, got {c}");
}

#[test]
fn fine_cos_ang90() {
    let c: f32 = fine_cos(ANG90).into();
    assert!(c.abs() < 0.001, "cos(90°) ~ 0, got {c}");
}

#[test]
fn fine_sin_ang180() {
    let s: f32 = fine_sin(ANG180).into();
    assert!(s.abs() < 0.001, "sin(180°) ~ 0, got {s}");
}

#[test]
fn fine_cos_ang180() {
    let c: f32 = fine_cos(ANG180).into();
    assert!((c + 1.0).abs() < 0.001, "cos(180°) ~ -1, got {c}");
}

#[test]
fn fine_tan_middle() {
    let t: f32 = fine_tan(2048).into();
    assert!(t.abs() < 0.001, "tan(~0) ~ 0, got {t}");
}
