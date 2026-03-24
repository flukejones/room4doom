//! Integration test: extract E5M1 from sigil.wad, rebuild with both C and
//! Rust BSP builders, then compare all generated lumps.

mod harness;

use std::path::Path;

const SIGIL_WAD: &str = "/Users/lukejones/DOOM/sigil.wad";

#[test]
fn compare_bsp_output_sigil_e5m1() {
    if !Path::new(SIGIL_WAD).exists() {
        eprintln!("Skipping test: {} not found", SIGIL_WAD);
        return;
    }
    if !Path::new(harness::C_BSP).exists() {
        eprintln!(
            "Skipping test: C bsp binary not found at {}",
            harness::C_BSP
        );
        return;
    }

    let extracted = "/tmp/rbsp_test_sigil_e5m1_input.wad";
    harness::extract_level(SIGIL_WAD, "E5M1", extracted);
    harness::compare_bsp_output(extracted, "sigil_e5m1");
    let _ = std::fs::remove_file(extracted);
}
