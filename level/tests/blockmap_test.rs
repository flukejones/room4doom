use test_utils::{doom1_wad_path, load_map, sunder_wad_path};

/// E1M1 has a WAD blockmap. Build one from scratch and verify it covers the
/// same grid dimensions and that every linedef appears in at least one cell.
#[test]
fn test_e1m1_generated_blockmap_coverage() {
    let mut map = load_map(&doom1_wad_path(), "E1M1");
    let wad_bm = map.blockmap();
    let wad_cols = wad_bm.columns;
    let wad_rows = wad_bm.rows;
    let _wad_line_refs = wad_bm.block_lines.len();

    assert!(
        wad_cols > 0 && wad_rows > 0,
        "WAD blockmap should exist for E1M1"
    );

    // Rebuild from linedefs
    map.build_blockmap("E1M1");
    let gen_bm = map.blockmap();

    assert_eq!(
        gen_bm.columns, wad_cols,
        "Generated columns should match WAD"
    );
    assert_eq!(gen_bm.rows, wad_rows, "Generated rows should match WAD");
    assert!(
        gen_bm.block_lines.len() > 0,
        "Generated blockmap should have line refs"
    );

    // Every linedef should appear in at least one cell
    let num_lines = map.linedefs.len();
    let mut line_found = vec![false; num_lines];
    for i in 0..gen_bm.block_offsets.len() - 1 {
        let start = gen_bm.block_offsets[i];
        let end = gen_bm.block_offsets[i + 1];
        for j in start..end {
            let ld_num = gen_bm.block_lines[j].num;
            if ld_num < num_lines {
                line_found[ld_num] = true;
            }
        }
    }
    let missing: Vec<usize> = line_found
        .iter()
        .enumerate()
        .filter(|(_, found)| !**found)
        .map(|(i, _)| i)
        .collect();
    assert!(
        missing.is_empty(),
        "All linedefs should appear in at least one blockmap cell. Missing: {:?}",
        &missing[..missing.len().min(10)]
    );
}

/// Sunder MAP20 has no WAD blockmap. Verify that build_blockmap produces one.
#[test]
#[ignore = "sunder.wad can't be included in git"]
fn test_sunder_map20_generated_blockmap() {
    let map = load_map(&sunder_wad_path(), "MAP20");
    let bm = map.blockmap();

    assert!(
        bm.columns > 0 && bm.rows > 0,
        "Generated blockmap should have valid dimensions for MAP20"
    );
    assert!(
        bm.block_lines.len() > 0,
        "Generated blockmap should have line refs"
    );

    // Sunder MAP20 is huge — should have more than 256 cells per axis
    let total = bm.columns * bm.rows;
    assert!(
        total > 1000,
        "MAP20 blockmap should be large, got {}x{} = {}",
        bm.columns,
        bm.rows,
        total
    );
}
