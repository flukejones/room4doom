use level::{LineDefFlags, SurfaceKind};
use test_utils::{doom1_wad_path, load_map};

/// E1M5 linedef 808 borders two movers with different travel:
/// - sector 48 (front, tag=10): floor drops into a pit
/// - sector 50 (back): a door, zero-height until opened (ceiling rises)
///
/// ld808's **lower** wall lives in sector 50's subsector but its top edge is
/// shared with sector 48's floor, so it stretches *downward* as s48 drops. The
/// leaf AABB used for frustum culling must cover that full downward travel,
/// otherwise the lower wall is culled when the player looks into the pit and
/// only reappears when the view tilts up.
///
/// Regression guard for the cross-sector mover AABB expansion in
/// `expand_node_aabbs_for_movers`.
#[test]
fn e1m5_ld808_lower_wall_aabb_covers_pit() {
    let map = load_map(&doom1_wad_path(), "E1M5");
    let bsp3d = &map.bsp_3d;
    let sectors = &map.sectors;

    // Lowest floor reachable by sector 48 across its two-sided neighbours —
    // the depth the down-mover's floor (and the shared lower wall) reaches.
    let s48 = &sectors[48];
    let mut min_floor = s48.floorheight.to_f32();
    for line in &s48.lines {
        if !line.flags.contains(LineDefFlags::TwoSided) {
            continue;
        }
        let neighbour = if line.frontsector.num == s48.num {
            line.backsector.as_ref()
        } else {
            Some(&line.frontsector)
        };
        if let Some(other) = neighbour {
            min_floor = min_floor.min(other.floorheight.to_f32());
        }
    }
    assert!(
        min_floor < -100.0,
        "expected sector 48 to drop into a deep pit, got min_floor={min_floor}"
    );

    // Every subsector leaf that hosts an ld808 wall must have its AABB cover
    // the full vertical travel, down to the pit floor.
    let mut checked_lower = false;
    for sid in [48usize, 50usize] {
        for &ss in &bsp3d.sector_subsectors[sid] {
            let leaf = bsp3d.get_subsector_leaf(ss).expect("leaf exists");
            for poly in bsp3d.leaf_polygons(ss) {
                if let SurfaceKind::Vertical {
                    linedef_id,
                    wall_type,
                    ..
                } = poly.surface_kind
                    && linedef_id == 808
                {
                    assert!(
                        leaf.aabb.min.z <= min_floor + 0.5,
                        "leaf {ss} (sector {sid}) hosts ld808 {wall_type:?} wall but \
                         aabb.min.z={} does not cover pit floor {min_floor}",
                        leaf.aabb.min.z
                    );
                    if wall_type == level::WallType::Lower {
                        checked_lower = true;
                    }
                }
            }
        }
    }
    assert!(
        checked_lower,
        "did not find ld808 lower wall — test geometry assumptions changed"
    );
}
