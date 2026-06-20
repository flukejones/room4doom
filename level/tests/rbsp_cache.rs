//! Lump-path equivalence: the cache and a tool-emitted WAD `RBSP`
//! lump must materialize runtime data identical to an in-process build.

use level::LevelData;
use test_utils::{doom1_wad_path, load_map, load_map_with_pwad};

fn assert_same_runtime(a: &LevelData, b: &LevelData) {
    // 2D structures.
    assert_eq!(a.vertexes.len(), b.vertexes.len(), "a.vertexes.len()");
    assert_eq!(a.segments.len(), b.segments.len(), "a.segments.len()");
    assert_eq!(a.subsectors.len(), b.subsectors.len(), "a.subsectors.len()");
    let (an, bn) = (a.bsp_3d.nodes(), b.bsp_3d.nodes());
    assert_eq!(an.len(), bn.len(), "node count");
    for (na, nb) in an.iter().zip(bn.iter()) {
        assert_eq!(na.normal, nb.normal, "na.normal");
        assert_eq!(na.d, nb.d, "na.d");
        assert_eq!(na.xy_fp, nb.xy_fp, "na.xy_fp");
        assert_eq!(na.delta_fp, nb.delta_fp, "na.delta_fp");
        assert_eq!(na.children, nb.children, "na.children");
    }
    assert_eq!(
        a.bsp_3d.node_bboxes(),
        b.bsp_3d.node_bboxes(),
        "node bboxes must survive the lump"
    );
    assert_eq!(a.bsp_3d.leaves.len(), b.bsp_3d.leaves.len(), "leaf count");
    for (la, lb) in a.bsp_3d.leaves.iter().zip(b.bsp_3d.leaves.iter()) {
        assert_eq!(
            la.subsector, lb.subsector,
            "leaf subsector must survive the lump"
        );
    }

    // 3D runtime.
    let (ba, bb) = (&a.bsp_3d, &b.bsp_3d);
    assert_eq!(ba.vertices, bb.vertices, "ba.vertices");
    assert_eq!(ba.poly_verts, bb.poly_verts, "ba.poly_verts");
    assert_eq!(
        ba.poly_vertex_range, bb.poly_vertex_range,
        "ba.poly_vertex_range"
    );
    assert_eq!(ba.poly_vertex_uv, bb.poly_vertex_uv, "ba.poly_vertex_uv");
    assert_eq!(ba.triangles, bb.triangles, "ba.triangles");
    assert_eq!(ba.poly_tex, bb.poly_tex, "ba.poly_tex");
    assert_eq!(ba.poly_back_tex, bb.poly_back_tex, "ba.poly_back_tex");
    assert_eq!(ba.poly_flags, bb.poly_flags, "ba.poly_flags");
    assert_eq!(ba.shared_walls, bb.shared_walls, "ba.shared_walls");
    assert_eq!(
        ba.sector_floor_polys, bb.sector_floor_polys,
        "ba.sector_floor_polys"
    );
    assert_eq!(
        ba.sector_ceiling_polys, bb.sector_ceiling_polys,
        "ba.sector_ceiling_polys"
    );
    assert_eq!(
        ba.sector_wall_polys, bb.sector_wall_polys,
        "ba.sector_wall_polys"
    );
    assert_eq!(
        ba.linedef_wall_polys, bb.linedef_wall_polys,
        "ba.linedef_wall_polys"
    );
    for (pa, pb) in ba.polygons.iter().zip(bb.polygons.iter()) {
        assert_eq!(pa.normal, pb.normal, "pa.normal");
        assert_eq!(pa.seg_offset, pb.seg_offset, "pa.seg_offset");
        assert_eq!(pa.sector.num, pb.sector.num, "pa.sector.num");
        assert_eq!(
            pa.linedef.as_ref().map(|l| l.num),
            pb.linedef.as_ref().map(|l| l.num),
            "polygon linedef",
        );
    }
}

#[test]
fn wad_lump_matches_built() {
    let out = std::env::temp_dir().join(format!("r4d-rbsp-tool-{}.wad", std::process::id()));
    // Same sky condition as `load_map` (none) so both paths build identically.
    rbsp::wad_io::process_wad(&doom1_wad_path(), &out, &rbsp::BspOptions::default(), None)
        .expect("rbsp tool run");

    let built = load_map(&doom1_wad_path(), "E1M3");
    let from_lump = load_map_with_pwad(&doom1_wad_path(), &out, "E1M3");
    assert_same_runtime(&built, &from_lump);
    std::fs::remove_file(&out).ok();
}

#[test]
fn cache_hit_produces_identical_runtime() {
    let a = load_map(&doom1_wad_path(), "E1M2");

    // test-utils points ROOM4DOOM_CACHE_DIR at a per-process temp dir; the
    // load above must have written the entry there.
    let cache_dir = std::path::PathBuf::from(
        std::env::var_os("ROOM4DOOM_CACHE_DIR").expect("test cache dir set"),
    )
    .join("rbsp");
    let cached: Vec<_> = std::fs::read_dir(&cache_dir)
        .expect("cache dir created")
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with("E1M2-"))
        .collect();
    assert_eq!(cached.len(), 1, "one cache entry for E1M2");

    let b = load_map(&doom1_wad_path(), "E1M2");
    assert_same_runtime(&a, &b);
}
