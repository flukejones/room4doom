//! Integration test: rebuild e1m8b.wad with both C and Rust BSP builders,
//! then compare all generated lumps.

mod harness;

use std::path::Path;

const INPUT_WAD: &str = "/Users/lukejones/DOOM/e1m8b.wad";

#[test]
fn compare_bsp_output_e1m8b() {
    if !Path::new(INPUT_WAD).exists() {
        eprintln!("Skipping test: {} not found", INPUT_WAD);
        return;
    }
    if !Path::new(harness::C_BSP).exists() {
        eprintln!(
            "Skipping test: C bsp binary not found at {}",
            harness::C_BSP
        );
        return;
    }

    harness::compare_bsp_output(INPUT_WAD, "e1m8b");
}
