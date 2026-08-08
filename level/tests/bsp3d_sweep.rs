//! Swept-sphere queries against real E1M1 geometry.

use glam::Vec3;
use test_utils::{doom1_wad_path, load_map};

/// Nothing may be pruned when the query box covers the whole map, or a sweep
/// would silently pass through the geometry the descent skipped.
#[test]
fn a_box_over_the_whole_map_gathers_every_leaf() {
    let map = load_map(&doom1_wad_path(), "E1M1");
    let bsp = map.bsp_3d();

    let (mut min, mut max) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for leaf in &bsp.leaves {
        min = min.min(leaf.aabb.min);
        max = max.max(leaf.aabb.max);
    }

    let mut found = Vec::new();
    bsp.leaves_overlapping(min, max, &mut found);
    assert_eq!(
        found.len(),
        bsp.leaves.len(),
        "the descent pruned leaves that overlap the query box"
    );
}

/// Every vertex of a floor polygon lies on a solid surface, so a sphere centred
/// on one must report that surface however small the radius.
#[test]
fn a_sphere_on_a_floor_is_blocked_by_it() {
    let map = load_map(&doom1_wad_path(), "E1M1");
    let bsp = map.bsp_3d();
    let mut leaves = Vec::new();
    let mut checked = 0usize;

    for leaf_id in 0..bsp.leaves.len() {
        for gi in bsp.leaf_floor_polys(leaf_id) {
            for &v in bsp.poly_vert_indices(gi) {
                let on_floor = bsp.vertex_get(v);
                leaves.clear();
                let pad = Vec3::splat(1.0);
                bsp.leaves_overlapping(on_floor - pad, on_floor + pad, &mut leaves);
                assert!(
                    bsp.blocking_poly_near(on_floor, 1.0, &leaves).is_some(),
                    "leaf {leaf_id} poly {gi}: a sphere on the floor at {on_floor} found nothing"
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 0, "E1M1 has floor polygons to check");
}
