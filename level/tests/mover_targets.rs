//! `mover_targets_for_sector` computes plausible destinations on a real map.

use level::env_target::mover_targets_for_sector;
use test_utils::{doom1_wad_path, load_map};

#[test]
fn e1m5_movers_have_targets() {
    let mut map = load_map(&doom1_wad_path(), "E1M5");

    // E1M5 sector 48 is a tagged floor mover (drops into a pit); sector 50 is
    // the door behind ld808. At least one of the map's sectors must yield a
    // mover target.
    let count = map.sectors.len();
    let mut any = false;
    for sid in 0..count {
        let targets = mover_targets_for_sector(sid, &mut map, &|_| 0);
        if !targets.is_empty() {
            any = true;
            for t in &targets {
                assert!(t.height.is_finite(), "sector {sid} target not finite");
            }
        }
    }
    assert!(any, "E1M5 should yield at least one mover target");
}
