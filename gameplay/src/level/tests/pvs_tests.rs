#[cfg(test)]
mod pvs_tests {
    use crate::{PVS, PicData};
    use map_data::MapData;
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

    /// Load a map with PWADs and build full PVS (subsector-level) from scratch.
    fn build_full_pvs(iwad: &PathBuf, pwads: &[PathBuf], map_name: &str) -> (MapData, PVS) {
        let mut wad = wad::WadData::new(iwad);
        for pwad in pwads {
            wad.add_file(pwad.clone());
        }
        let pic_data = PicData::init(&wad);
        let mut map_data = MapData::default();
        map_data.load(map_name, |name| pic_data.flat_num_for_name(name), &wad);
        let mut pvs = PVS::new(map_data.subsectors.len());
        pvs.build(
            &map_data.subsectors,
            &map_data.segments,
            &map_data.bsp_3d,
            &map_data.sectors,
            &map_data.linedefs,
            &map_data.nodes,
            map_data.start_node,
        );
        (map_data, pvs)
    }

    #[test]
    fn test_e1m2_s159_s142_subsector_visibility() {
        let (_map_data, pvs) = build_full_pvs(&doom1_wad_path(), &[], "E1M2");

        // ss330 is the ceiling area between ld913 (x=-1600) and ld912 (x=-1472),
        // y=1216-1408 ss380 (s159) looks north through a staircase portal chain
        // to s142
        assert!(
            pvs.is_visible(380, 330),
            "s159 ss380 should see s142 ss330 (ceiling between ld913 and ld912)"
        );
    }

    #[test]
    #[ignore = "Requires registered DOOM"]
    fn test_e5m1_subsector_visibility() {
        let sigil = sigil_wad_path();

        let (map_data, pvs) = build_full_pvs(&doom_wad_path(), &[sigil], "E5M1");
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
}
