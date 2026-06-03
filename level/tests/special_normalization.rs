//! End-to-end check that vanilla mover specials are normalised to generalized
//! form at level load, while `default_special` preserves the original number
//! and the decoder reproduces the right mover identity.

use level::special_encode::{Category, decode, encode_vanilla, is_generalized};
use test_utils::{doom1_wad_path, load_map};

#[test]
fn e1m5_specials_normalized_at_load() {
    let map = load_map(&doom1_wad_path(), "E1M5");

    let mut saw_mover = false;
    for line in map.linedefs.iter() {
        let original = line.default_special;
        if original <= 0 {
            // No special: stays 0.
            assert_eq!(line.special, original as u32);
            continue;
        }
        match encode_vanilla(original as u32) {
            Some(generalized) => {
                // A mover special: rewritten in place, original preserved.
                saw_mover = true;
                assert_eq!(
                    line.special, generalized,
                    "linedef special {original} not normalized (got {:#x})",
                    line.special
                );
                assert!(is_generalized(line.special));
                // The generalized form must decode back to a mover identity.
                assert!(
                    decode(line.special).is_some(),
                    "normalized special {:#x} (from {original}) did not decode",
                    line.special
                );
            }
            None => {
                // Non-mover special: left as the original vanilla number.
                assert_eq!(line.special, original as u32);
            }
        }
    }
    assert!(saw_mover, "E1M5 should contain at least one mover special");
}

#[test]
fn manual_door_decodes_manual() {
    // E1M5 has manual doors (special 1/31/...). Find one and confirm it
    // normalizes to a manual-flagged door whose original number survives.
    let map = load_map(&doom1_wad_path(), "E1M5");
    let manual_specials = [1i16, 26, 27, 28, 31, 32, 33, 34, 117, 118];
    let mut found = false;
    for line in map.linedefs.iter() {
        if manual_specials.contains(&line.default_special) {
            found = true;
            let spec = decode(line.special).expect("manual door decodes");
            assert_eq!(spec.category, Category::Door);
            assert!(
                spec.manual,
                "door {} not flagged manual",
                line.default_special
            );
        }
    }
    assert!(found, "E1M5 should contain at least one manual door");
}
