#[cfg(test)]
mod pvs_tests {
    use crate::{PVS2D, PicData};
    use map_data::{MapData, PvsData, PvsView2D, RenderPvs};
    use std::path::PathBuf;

    fn doom1_wad_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("doom1.wad")
    }

    fn doom_wad_path() -> PathBuf {
        PathBuf::from("/Users/lukejones/DOOM/doom.wad")
    }

    fn sigil_wad_path() -> PathBuf {
        PathBuf::from("/Users/lukejones/DOOM/sigil.wad")
    }

    fn sigil2_wad_path() -> PathBuf {
        PathBuf::from("/Users/lukejones/DOOM/sigil2.wad")
    }

    fn doom2_wad_path() -> PathBuf {
        PathBuf::from("/Users/lukejones/DOOM/doom2.wad")
    }

    fn sunder_wad_path() -> PathBuf {
        PathBuf::from("/Users/lukejones/DOOM/sunder.wad")
    }

    /// Load a map with PWADs and build full PVS (subsector-level) from scratch.
    ///
    /// Returns `(MapData, PVS2D, RenderPvs)`. Use `PVS2D` for portal/mightsee
    /// access and `RenderPvs` for visibility queries (`is_visible`,
    /// `get_visible_subsectors`).
    fn build_full_pvs(
        iwad: &PathBuf,
        pwads: &[PathBuf],
        map_name: &str,
    ) -> (MapData, PVS2D, RenderPvs) {
        let mut wad = wad::WadData::new(iwad);
        for pwad in pwads {
            wad.add_file(pwad.clone());
        }
        let pic_data = PicData::init(&wad);
        let mut map_data = MapData::default();
        map_data.load(
            map_name,
            |name| pic_data.flat_num_for_name(name),
            &wad,
            None,
        );
        let pvs2d = PVS2D::build(
            &map_data.subsectors,
            &map_data.segments,
            &map_data.bsp_3d,
            &map_data.sectors,
            &map_data.linedefs,
            &map_data.nodes,
            map_data.start_node,
        );
        let render = pvs2d.clone_render_pvs();
        (map_data, pvs2d, render)
    }

    #[test]
    fn test_e1m2_s159_s142_subsector_visibility() {
        let (_map_data, pvs2d, pvs) = build_full_pvs(&doom1_wad_path(), &[], "E1M2");

        // ss330 is the ceiling area between ld913 (x=-1600) and ld912 (x=-1472),
        // y=1216-1408 ss380 (s159) looks north through a staircase portal chain
        // to s142

        // Diagnostic: how many subsectors can ss380 and ss330 see?
        let visible_from_380 = pvs.get_visible_subsectors(380);
        let visible_from_330 = pvs.get_visible_subsectors(330);
        eprintln!(
            "ss380 sees {} subsectors: {:?}",
            visible_from_380.len(),
            &visible_from_380[..visible_from_380.len().min(30)]
        );
        eprintln!(
            "ss330 sees {} subsectors: {:?}",
            visible_from_330.len(),
            &visible_from_330[..visible_from_330.len().min(30)]
        );
        eprintln!("ss330 visible from ss380: {}", pvs.is_visible(380, 330));
        eprintln!("ss380 visible from ss330: {}", pvs.is_visible(330, 380));

        let portals = pvs2d.portals_2d();

        // Check portals adjacent to ss330
        let portals_330: Vec<_> = portals
            .iter()
            .filter(|p| p.subsector_a == 330 || p.subsector_b == 330)
            .map(|p| (p.subsector_a, p.subsector_b))
            .collect();
        eprintln!(
            "Portals adjacent to ss330 ({} total): {:?}",
            portals_330.len(),
            portals_330
        );

        // Check portals adjacent to ss380
        let portals_380: Vec<_> = portals
            .iter()
            .filter(|p| p.subsector_a == 380 || p.subsector_b == 380)
            .map(|p| (p.subsector_a, p.subsector_b))
            .collect();
        eprintln!(
            "Portals adjacent to ss380 ({} total): {:?}",
            portals_380.len(),
            portals_380
        );

        // Total portals in map
        eprintln!("Total portals in map: {}", portals.len());

        // Trace the portal chain: find path 380 → 322 → 330 via BFS
        let n = pvs.subsector_count();
        let mut adj: Vec<Vec<(usize, usize)>> = vec![Vec::new(); n]; // ss -> [(neighbor, portal_idx)]
        for i in 0..portals.len() {
            let p = portals.get(i);
            adj[p.subsector_a].push((p.subsector_b, i));
            adj[p.subsector_b].push((p.subsector_a, i));
        }

        // BFS from 380 to 330
        let mut visited = vec![false; n];
        let mut parent = vec![(usize::MAX, usize::MAX); n]; // (parent_ss, portal_idx)
        let mut queue = std::collections::VecDeque::new();
        visited[380] = true;
        queue.push_back(380);
        while let Some(ss) = queue.pop_front() {
            if ss == 330 {
                break;
            }
            for &(nb, pi) in &adj[ss] {
                if !visited[nb] {
                    visited[nb] = true;
                    parent[nb] = (ss, pi);
                    queue.push_back(nb);
                }
            }
        }

        if visited[330] {
            let mut path = Vec::new();
            let mut cur = 330;
            while cur != 380 {
                let (p, pi) = parent[cur];
                let portal = portals.get(pi);
                path.push((cur, pi, portal.segment()));
                cur = p;
            }
            path.reverse();
            eprintln!("\nPortal chain 380 → 330 ({} hops):", path.len());
            for (i, (ss, pi, (v1, v2))) in path.iter().enumerate() {
                let sec = _map_data.subsectors[*ss].sector.num;
                eprintln!(
                    "  hop {}: → ss{} (sec {}) via portal {} seg ({:.0},{:.0})→({:.0},{:.0})",
                    i, ss, sec, pi, v1.x, v1.y, v2.x, v2.y
                );
            }
        } else {
            eprintln!("NO PATH from 380 to 330 in portal graph!");
        }

        // Print portals adjacent to ss322
        let portals_322: Vec<_> = (0..portals.len())
            .map(|i| portals.get(i))
            .enumerate()
            .filter(|(_, p)| p.subsector_a == 322 || p.subsector_b == 322)
            .map(|(i, p)| (i, p.subsector_a, p.subsector_b))
            .collect();
        eprintln!(
            "Portals adjacent to ss322 ({} total): {:?}",
            portals_322.len(),
            portals_322
        );

        // Sectors for key subsectors
        for ss in [322, 323, 324, 325, 326, 327, 328, 329, 330, 380, 381, 379] {
            if ss < _map_data.subsectors.len() {
                eprintln!(
                    "  ss{} → sector {}",
                    ss, _map_data.subsectors[ss].sector.num
                );
            }
        }

        assert!(
            pvs.is_visible(380, 330),
            "s159 ss380 should see s142 ss330 (ceiling between ld913 and ld912)"
        );
    }

    /// Verify that get_mightsee_subsectors uses the portal-normal coarse test
    /// and produces a result meaningfully smaller than the full map on E1M2.
    /// The coarse test should be a superset of PVS (no false negatives).
    #[test]
    fn test_e1m2_mightsee_coarse_normal_test() {
        let (_map_data, pvs2d, pvs) = build_full_pvs(&doom1_wad_path(), &[], "E1M2");
        let n = pvs.subsector_count();

        let mut total_ms = 0u64;
        let mut total_pv = 0u64;
        let mut false_negatives = 0usize;

        for ss in 0..n {
            let ms: std::collections::HashSet<usize> =
                pvs2d.get_mightsee_subsectors(ss).into_iter().collect();
            let pv = pvs.get_visible_subsectors(ss);
            total_ms += ms.len() as u64;
            total_pv += pv.len() as u64;
            // mightsee must be a superset of PVS
            for &vis in &pv {
                if !ms.contains(&vis) {
                    false_negatives += 1;
                    if false_negatives <= 5 {
                        eprintln!("FALSE NEGATIVE: ss{ss} sees ss{vis} in PVS but not mightsee");
                    }
                }
            }
            if ss < 5 || ss % 100 == 0 {
                eprintln!("  ss{ss}: pvs={} mightsee={}/{n}", pv.len(), ms.len());
            }
        }

        let avg_ms = total_ms / n as u64;
        let avg_pv = total_pv / n as u64;
        eprintln!(
            "E1M2: {n} subsectors, avg pvs={avg_pv}, avg mightsee={avg_ms}, false_negatives={false_negatives}"
        );

        assert_eq!(
            false_negatives, 0,
            "mightsee must be a superset of PVS (found {false_negatives} false negatives)"
        );
        assert!(
            avg_ms < n as u64 * 3 / 4,
            "avg mightsee {avg_ms}/{n} >= 75% — coarse test not filtering enough"
        );
    }

    /// E1M2 PVS invariants — hardcoded pairs derived from the diagnostic test.
    ///
    /// Can-see pairs (verified by diagnostic):
    ///   ss0 → ss140 (3 portal hops), ss0 → ss145 (3 hops), ss0 → ss151 (8 hops
    /// furthest)   ss380 → ss330 (existing corridor test)
    ///
    /// Cannot-see pairs (5+ portal hops, different room):
    ///   ss0 → ss2, ss0 → ss3, ss0 → ss4
    ///
    /// Algorithm changes must not alter these results — if this test fails,
    /// stop and report.
    #[test]
    fn test_e1m2_pvs_invariants() {
        let (_map_data, _pvs2d, pvs) = build_full_pvs(&doom1_wad_path(), &[], "E1M2");

        // Sanity: E1M2 has 448 subsectors.
        assert_eq!(pvs.subsector_count(), 448, "E1M2 subsector count changed");

        // Can-see pairs — must be true in both directions (PVS is symmetric).
        let can_see: &[(usize, usize)] = &[(0, 140), (0, 145), (0, 151), (380, 330)];
        for &(a, b) in can_see {
            assert!(pvs.is_visible(a, b), "ss{a} should see ss{b} but does not");
            assert!(
                pvs.is_visible(b, a),
                "ss{b} should see ss{a} (symmetry) but does not"
            );
        }

        // Cannot-see pairs — reachable by portals but in a different room.
        let cannot_see: &[(usize, usize)] = &[(0, 2), (0, 3), (0, 4)];
        for &(a, b) in cannot_see {
            assert!(!pvs.is_visible(a, b), "ss{a} should NOT see ss{b} but does");
        }
    }

    /// Diagnostic: build PVS for E1M2, print hop-distance pairs and can't-see
    /// candidates. Run once with `-- --nocapture` to gather data for
    /// test_e1m2_pvs_invariants.
    #[test]
    #[ignore = "diagnostic only — run manually to collect invariant data"]
    fn test_e1m2_pvs_diagnostic() {
        let (_map_data, pvs2d, pvs) = build_full_pvs(&doom1_wad_path(), &[], "E1M2");
        let n = pvs.subsector_count();

        // Build portal adjacency for BFS hop counting.
        let portals = pvs2d.portals_2d();
        let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
        for i in 0..portals.len() {
            let p = portals.get(i);
            adj[p.subsector_a].push(p.subsector_b);
            adj[p.subsector_b].push(p.subsector_a);
        }

        // BFS from ss=0 to get hop distances.
        let src = 0usize;
        let mut dist = vec![usize::MAX; n];
        dist[src] = 0;
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(src);
        while let Some(ss) = queue.pop_front() {
            for &nb in &adj[ss] {
                if dist[nb] == usize::MAX {
                    dist[nb] = dist[ss] + 1;
                    queue.push_back(nb);
                }
            }
        }

        let visible_from_0: std::collections::HashSet<usize> =
            pvs.get_visible_subsectors(src).into_iter().collect();

        // 3 hops away and visible.
        let at_3_visible: Vec<usize> = (0..n)
            .filter(|&ss| dist[ss] == 3 && visible_from_0.contains(&ss))
            .take(5)
            .collect();
        eprintln!("ss{src} → 3 hops + visible: {:?}", at_3_visible);

        // Furthest visible (max hop distance and in PVS).
        let max_hop = (0..n)
            .filter(|ss| visible_from_0.contains(ss) && dist[*ss] != usize::MAX)
            .map(|ss| dist[ss])
            .max()
            .unwrap_or(0);
        let furthest_visible: Vec<usize> = (0..n)
            .filter(|&ss| visible_from_0.contains(&ss) && dist[ss] == max_hop)
            .take(5)
            .collect();
        eprintln!(
            "ss{src} → furthest visible (hop={max_hop}): {:?}",
            furthest_visible
        );

        // Near but not visible: 1–2 hops away but NOT in PVS.
        let near_not_visible: Vec<usize> = (0..n)
            .filter(|&ss| (dist[ss] == 1 || dist[ss] == 2) && !visible_from_0.contains(&ss))
            .take(5)
            .collect();
        eprintln!(
            "ss{src} → near (1-2 hops) but NOT visible: {:?}",
            near_not_visible
        );

        // Far not visible: 5+ hops away and NOT in PVS.
        let far_not_visible: Vec<usize> = (0..n)
            .filter(|&ss| dist[ss] >= 5 && !visible_from_0.contains(&ss))
            .take(5)
            .collect();
        eprintln!("ss{src} → far (5+ hops) NOT visible: {:?}", far_not_visible);

        // Also print from ss380 (already tested sector).
        let visible_from_380: std::collections::HashSet<usize> =
            pvs.get_visible_subsectors(380).into_iter().collect();
        let near_380_not_visible: Vec<usize> = (0..n)
            .filter(|&ss| adj[380].contains(&ss) && !visible_from_380.contains(&ss))
            .take(5)
            .collect();
        eprintln!(
            "ss380 → direct neighbours NOT visible: {:?}",
            near_380_not_visible
        );
    }

    #[test]
    #[ignore = "Requires registered DOOM"]
    fn test_e5m1_subsector_visibility() {
        let sigil = sigil_wad_path();

        let (map_data, _pvs2d, pvs) = build_full_pvs(&doom_wad_path(), &[sigil], "E5M1");
        let bsp = &map_data.bsp_3d;

        // Subsector-level: s23 -> s35
        let s23_subs = &bsp.sector_subsectors[23];
        let s34_subs = &bsp.sector_subsectors[34];
        let s35_subs = &bsp.sector_subsectors[35];

        let s23_s35_visible = s23_subs
            .iter()
            .flat_map(|&src| s35_subs.iter().map(move |&tgt| (src, tgt)))
            .filter(|&(src, tgt)| pvs.is_visible(src, tgt))
            .count();
        assert!(
            s23_s35_visible > 0,
            "s23 subsectors should see at least some s35 subsectors"
        );

        let s23_s34_visible = s23_subs
            .iter()
            .flat_map(|&src| s34_subs.iter().map(move |&tgt| (src, tgt)))
            .filter(|&(src, tgt)| pvs.is_visible(src, tgt))
            .count();
        assert!(
            s23_s34_visible > 0,
            "s23 subsectors should see at least some s34 subsectors"
        );

        let s35_s34_visible = s35_subs
            .iter()
            .flat_map(|&src| s34_subs.iter().map(move |&tgt| (src, tgt)))
            .filter(|&(src, tgt)| pvs.is_visible(src, tgt))
            .count();
        assert!(
            s35_s34_visible > 0,
            "s35 subsectors should see at least some s34 subsectors"
        );
    }

    /// E6M1 PVS portal invariants — concentric ring arena pairs that must be
    /// mutually visible. The arena contains sector 28 (outer), sector 1 (inner
    /// ring), sector 3 (innermost ring), and sector 297 (starburst overlay),
    /// all open to each other with no solid walls between them.
    ///
    /// Expected to FAIL until the portal collection bug in `Portals::build` is
    /// fixed. Run with `--nocapture` to see which pairs are missing.
    #[test]
    #[ignore = "Requires registered DOOM and Sigil 2"]
    fn test_e6m1_pvs_portal_invariants() {
        let sigil2 = sigil2_wad_path();
        let (_map_data, _pvs2d, pvs) = build_full_pvs(&doom_wad_path(), &[sigil2], "E6M1");

        let n = pvs.subsector_count();
        assert!(n > 0, "E6M1 must have subsectors");
        eprintln!("E6M1: {n} subsectors");

        // Pairs derived from pvs-tool rect_select data for the ring arena.
        // All pairs are in the same open area — no solid geometry between them.
        let can_see: &[(usize, usize, &str)] = &[
            (2815, 2817, "sector=1 inner ring"),
            (2822, 2828, "sector=3 innermost ring"),
            (2821, 2827, "sector=297 starburst"),
            (2570, 2815, "sector=28 outer → sector=1 inner"),
            (2570, 2614, "sector=28 distant pair"),
            (2755, 2570, "sector=28 central pair"),
        ];

        let mut failures = 0;
        for &(a, b, label) in can_see {
            let ab = pvs.is_visible(a, b);
            let ba = pvs.is_visible(b, a);
            if !ab || !ba {
                eprintln!("FAIL [{label}] ss{a} ↔ ss{b}: a→b={ab} b→a={ba}");
                failures += 1;
            }
        }

        assert_eq!(
            failures, 0,
            "{failures} visible pair(s) missing from PVS — portal collection bug"
        );
    }

    /// MAP03 Sunder portal invariants — large open concentric-ellipse arena.
    ///
    /// `can_see_both`: pairs that PVS2D must see. `can_see_might_only`: pairs
    /// where the portal-frustum collapses geometrically (sector=429 portals are
    /// 4 units wide) — legitimately absent from PVS2D.
    #[test]
    #[ignore = "Requires registered DOOM and Sunder"]
    fn test_map03_sunder_pvs_portal_invariants() {
        let sunder = sunder_wad_path();

        // Pairs from pvs-tool rect_select over the open arena.
        // sector=0: outer open space (ceil=425)
        // sector=436: outer ellipse (floor=-8, ceil=425)
        // sector=437: inner ellipse (floor=-16, ceil=425)
        let can_see_both: &[(usize, usize, &str)] = &[
            (1188, 1219, "sector=0 distant open pair"),
            (1563, 1188, "sector=0 far corner pair"),
            (1254, 1260, "sector=437 inner ellipse"),
            (1255, 1263, "sector=436 outer ellipse"),
            (1188, 1254, "sector=0 outer → sector=437 inner"),
            (1219, 1263, "sector=0 → sector=436"),
            (1307, 1312, "sector=268 → sector=16 cross-sector"),
        ];

        let (_, _, pvs2d) = build_full_pvs(&doom2_wad_path(), &[sunder], "MAP03");
        let mut failures = 0;
        for &(a, b, label) in can_see_both {
            let ab = pvs2d.is_visible(a, b);
            let ba = pvs2d.is_visible(b, a);
            if !ab || !ba {
                eprintln!("FAIL pvs2d   [{label}] ss{a} ↔ ss{b}: a→b={ab} b→a={ba}");
                failures += 1;
            }
        }

        assert_eq!(failures, 0, "{failures} visible pair(s) missing from PVS");
    }

    /// E5M1 Sigil portal invariants — concentric ring arena (sectors 0–12).
    ///
    /// Covers sectors 0 (outermost floor), 1–4 (intermediate rings),
    /// 7–12 (innermost region), all open to each other with no solid walls.
    #[test]
    #[ignore = "Requires registered DOOM and Sigil"]
    fn test_e5m1_sigil_pvs_portal_invariants() {
        let sigil = sigil_wad_path();

        // Pairs from pvs-tool rect_select over the concentric ring arena.
        // sector=0: outer floor, sector=1: outermost ring, sector=2–4:
        // intermediate rings, sector=7–12: innermost region. All mutually
        // visible.
        let can_see: &[(usize, usize, &str)] = &[
            // Same-sector pairs — within-ring visibility
            (1108, 1181, "sector=1 ring, distant pair"),
            (1110, 1184, "sector=2 ring, distant pair"),
            (1115, 1200, "sector=3 ring, distant pair"),
            (1122, 1156, "sector=9 inner, distant pair"),
            (1239, 1346, "sector=0 ring, distant pair"),
            (1117, 1134, "sector=4 ring, distant pair"),
            (1341, 1367, "sector=7 ring, distant pair"),
            (1128, 1368, "sector=8 ring, distant pair"),
            (1135, 1210, "sector=10 ring, distant pair"),
            (1189, 1208, "sector=11 ring, distant pair"),
            // Cross-ring pairs — outer rings to inner
            (1108, 1122, "sector=1 outer → sector=9 inner"),
            (1108, 1187, "sector=1 outer → sector=12 innermost"),
            (1178, 1341, "sector=1 → sector=7 cross-ring"),
            (1239, 1117, "sector=0 → sector=4"),
            (1239, 1128, "sector=0 → sector=8"),
            (1239, 1210, "sector=0 → sector=10"),
            (1117, 1208, "sector=4 → sector=11"),
            (1341, 1135, "sector=7 → sector=10"),
            // Previously failing pairs — fixed by triangulation bug fix
            (1120, 1241, "ss1120 ↔ ss1241"),
            (1120, 1346, "ss1120 ↔ ss1346"),
            (1119, 1346, "ss1119 ↔ ss1346"),
            (1156, 1243, "ss1156 ↔ ss1243"),
            // ss1145 specific pairs
            (1145, 1197, "ss1145 ↔ ss1197"),
            (1145, 1199, "ss1145 ↔ ss1199"),
            // ss1161 against all ring subsectors
            (1161, 1108, "ss1161 ↔ ss1108"),
            (1161, 1110, "ss1161 ↔ ss1110"),
            (1161, 1115, "ss1161 ↔ ss1115"),
            (1161, 1117, "ss1161 ↔ ss1117"),
            (1161, 1119, "ss1161 ↔ ss1119"),
            (1161, 1120, "ss1161 ↔ ss1120"),
            (1161, 1122, "ss1161 ↔ ss1122"),
            (1161, 1128, "ss1161 ↔ ss1128"),
            (1161, 1134, "ss1161 ↔ ss1134"),
            (1161, 1135, "ss1161 ↔ ss1135"),
            (1161, 1156, "ss1161 ↔ ss1156"),
            (1161, 1178, "ss1161 ↔ ss1178"),
            (1161, 1181, "ss1161 ↔ ss1181"),
            (1161, 1184, "ss1161 ↔ ss1184"),
            (1161, 1187, "ss1161 ↔ ss1187"),
            (1161, 1189, "ss1161 ↔ ss1189"),
            (1161, 1197, "ss1161 ↔ ss1197"),
            (1161, 1199, "ss1161 ↔ ss1199"),
            (1161, 1200, "ss1161 ↔ ss1200"),
            (1161, 1208, "ss1161 ↔ ss1208"),
            (1161, 1210, "ss1161 ↔ ss1210"),
            (1161, 1239, "ss1161 ↔ ss1239"),
            (1161, 1241, "ss1161 ↔ ss1241"),
            (1161, 1243, "ss1161 ↔ ss1243"),
            (1161, 1341, "ss1161 ↔ ss1341"),
            (1161, 1346, "ss1161 ↔ ss1346"),
            (1161, 1367, "ss1161 ↔ ss1367"),
            (1161, 1368, "ss1161 ↔ ss1368"),
        ];

        let (_, _, pvs2d) = build_full_pvs(&doom_wad_path(), &[sigil], "E5M1");
        let mut failures = 0;
        for &(a, b, label) in can_see {
            let ab = pvs2d.is_visible(a, b);
            let ba = pvs2d.is_visible(b, a);
            if !ab || !ba {
                eprintln!("FAIL pvs2d   [{label}] ss{a} ↔ ss{b}: a→b={ab} b→a={ba}");
                failures += 1;
            }
        }

        assert_eq!(failures, 0, "{failures} visible pair(s) missing from PVS");
    }
}
