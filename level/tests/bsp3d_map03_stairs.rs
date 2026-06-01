use glam::Vec2;
use level::SurfaceKind;
use test_utils::{doom2_wad_path, load_map};

/// 2D point-in-polygon over a subsector's carved floor polygon (XY only).
fn point_in_floor(verts: &[Vec2], p: Vec2) -> bool {
    let mut inside = false;
    let n = verts.len();
    let mut j = n - 1;
    for i in 0..n {
        let vi = verts[i];
        let vj = verts[j];
        if (vi.y > p.y) != (vj.y > p.y) {
            let t = (p.y - vi.y) / (vj.y - vi.y);
            if p.x < vi.x + t * (vj.x - vi.x) {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

/// Find the subsector whose floor polygon contains `p`, return (ss, sector_id,
/// floor_z). Panics if none/multiple cover it.
fn floor_at(map: &level::LevelData, p: Vec2) -> (usize, usize, f32) {
    let bsp3d = &map.bsp_3d;
    let mut hit = None;
    for ss in 0..bsp3d.subsector_leaves.len() {
        let leaf = bsp3d.get_subsector_leaf(ss).unwrap();
        for &fi in &leaf.floor_polygons {
            let poly = &leaf.polygons[fi];
            let verts: Vec<Vec2> = poly
                .vertices
                .iter()
                .map(|&v| {
                    let p3 = bsp3d.vertices[v];
                    Vec2::new(p3.x, p3.y)
                })
                .collect();
            if verts.len() >= 3 && point_in_floor(&verts, p) {
                let z = bsp3d.vertices[poly.vertices[0]].z;
                assert!(
                    hit.is_none(),
                    "point {p:?} covered by multiple floor polys: {hit:?} and ss {ss}"
                );
                hit = Some((ss, leaf.sector_id, z));
            }
        }
    }
    hit.unwrap_or_else(|| panic!("no floor polygon covers point {p:?}"))
}

/// MAP03 stairs (sectors 93..96) sit south of y=3328, below sector 90's floor
/// (112). Linedefs 129..132 (two-sided, on y=3328) separate each step from
/// sector 90. Before the box-margin fix in rbsp `box_on_line_side`, these
/// collinear two-sided lines failed to split the BSP leaf, so step subsectors
/// overran north into sector 90 and painted its floor at the step heights.
///
/// Sample points just north of each step (sector 90 territory) and on each
/// step itself; assert each renders at the correct floor height.
#[test]
fn map03_stairs_do_not_overrun_sector_90() {
    let map = load_map(&doom2_wad_path(), "MAP03");

    // Step floors: (x_centre, step_floor_height). Steps span y in [3328,3392]
    // (north of the linedef) per the WAD; sample y=3360 (mid-step).
    let steps = [
        (4020.0, 80.0),  // sector 93 (ld129)
        (3996.0, 88.0),  // sector 94 (ld130)
        (3972.0, 96.0),  // sector 95 (ld131)
        (3948.0, 104.0), // sector 96 (ld132)
    ];
    for (x, expect_floor) in steps {
        let (ss, sid, z) = floor_at(&map, Vec2::new(x, 3360.0));
        assert!(
            (z - expect_floor).abs() < 0.5,
            "step at x={x}: ss {ss} sector {sid} floor {z}, expected {expect_floor}"
        );
    }

    // Sector 90 territory south of the steps (y < 3328): must be floor 112,
    // never a step height. Sample the same x columns at y=3312.
    for (x, _) in steps {
        let (ss, sid, z) = floor_at(&map, Vec2::new(x, 3312.0));
        assert!(
            (z - 112.0).abs() < 0.5,
            "sector-90 area at x={x},y=3312: ss {ss} sector {sid} floor {z}, expected 112 \
             (step overran into sector 90)"
        );
    }

    // No subsector may merge two of these step sectors: every leaf is single
    // floor height across its floor polygon.
    let bsp3d = &map.bsp_3d;
    for ss in 0..bsp3d.subsector_leaves.len() {
        let leaf = bsp3d.get_subsector_leaf(ss).unwrap();
        for &fi in &leaf.floor_polygons {
            let poly = &leaf.polygons[fi];
            let z0 = bsp3d.vertices[poly.vertices[0]].z;
            for &v in &poly.vertices {
                assert!(
                    (bsp3d.vertices[v].z - z0).abs() < 0.5,
                    "ss {ss} floor poly spans heights {z0}..{} (non-flat leaf)",
                    bsp3d.vertices[v].z
                );
            }
        }
    }

    let _ = SurfaceKind::Horizontal {
        texture: 0,
        tex_cos: 0.0,
        tex_sin: 0.0,
    };
}
