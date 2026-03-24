#[cfg(test)]
mod map_data_tests {
    use std::f32::consts::PI;

    use crate::bsp_trace::BSPTrace;
    use glam::Vec2;
    use level::level_data::LevelData;
    use level::{IS_SSECTOR_MASK, LineDefFlags, Node};
    use math::{Angle, FixedT};
    use test_utils::{doom1_wad_path, sunder_wad_path};
    use wad::extended::WadExtendedMap;
    use wad::types::{WadLineDef, WadSideDef};
    use wad::{MapLump, WadData};

    #[ignore = "sunder.wad can't be included in git"]
    #[test]
    fn check_nodes_of_sunder_m3() {
        let wad = WadData::new(&sunder_wad_path());
        let ext = WadExtendedMap::parse(&wad, "MAP03").unwrap();
        assert_eq!(ext.num_org_vertices, 5525); // verified with crispy
        assert_eq!(ext.vertexes.len(), 996); // verified with crispy
        assert_eq!(ext.subsectors.len(), 4338);
        assert_eq!(ext.segments.len(), 14582);
        assert_eq!(ext.nodes.len(), 4337);

        let mut map = LevelData::default();
        map.load("MAP03", |_| None, &wad, None, None);

        // 666: no->x: 12.000000, no->y: -342.000000, no->dx: 0.000000, no->dy:
        // -20.000000 666: child[0]: 665, child[1]: -2147482974
        assert_eq!(
            map.get_nodes()[666],
            Node {
                xy: Vec2::new(12.0, -342.0),
                delta: Vec2::new(0.0, -20.0),
                bboxes: [
                    [Vec2::new(0.0, -342.0), Vec2::new(12.0, -362.0)],
                    [Vec2::new(12.0, -333.0), Vec2::new(24.0, -371.0)]
                ],
                children: [665, 2147484322],
            }
        );

        // seg v1:, x:496.000000, y:-1072.000000
        // seg v2:, x:496.000000, y:-1040.000000
        // sidedef->toptexture: 151
        // linedef: 2670
        // side: 1
        // sidenum: 4387
        let mut success = false;
        for (i, seg) in map.segments().iter().enumerate() {
            if seg.v1.pos == Vec2::new(496.0, -1072.0) && seg.v2.pos == Vec2::new(496.0, -1040.0) {
                assert_eq!(ext.segments[i].linedef, 2670);
                dbg!(i, &ext.segments[i]);
                assert_eq!(ext.segments[i].linedef, 2670);
                assert_eq!(ext.segments[i].side, 1);
                // dbg!(&seg.sidedef);
                assert_eq!(seg.sidedef.toptexture, Some(151));
                success = true;
            }
        }
        assert!(success);
    }

    #[ignore = "sunder.wad can't be included in git"]
    #[test]
    fn check_nodes_of_sunder_m20() {
        let name = "MAP20";
        let wad = WadData::new(&sunder_wad_path());
        let ext = WadExtendedMap::parse(&wad, name).unwrap();
        // orgVerts: 54347
        // newVerts: 25125
        // numSubs: 48504
        // numSegs: 161892
        // numNodes: 48503

        assert_eq!(ext.num_org_vertices, 54347); // verified with slade
        assert_eq!(ext.num_new_vertices, 25125); // with crispy
        assert_eq!(ext.vertexes.len(), 25125);
        assert_eq!(ext.subsectors.len(), 48504);
        assert_eq!(ext.segments.len(), 161892);
        assert_eq!(ext.nodes.len(), 48503);

        // seg:, x:-560.000000, y:-3952.000000
        // seg:, x:-560.000000, y:-3920.000000
        // sidedef->midtexture: 1657
        // linedef: 1590
        // side: 0
        // and other side:
        // sidedef->bottomtexture: 1628
        // sidedef->midtexture: 1657
        // linedef: 1590
        // side: 1
        for seg in ext.segments.iter() {
            if seg.linedef == 1590 {
                dbg!(seg); // two segs, one each side for this seg
            }
        }

        let lines: Vec<WadLineDef> = wad
            .map_iter::<WadLineDef>(name, MapLump::LineDefs)
            .collect();
        assert_eq!(lines[1590].front_sidedef, 2924);
        assert_eq!(lines[1590].back_sidedef, Some(2925));

        let sides: Vec<WadSideDef> = wad
            .map_iter::<WadSideDef>(name, MapLump::SideDefs)
            .collect();
        assert_eq!(sides[2924].lower_tex, "");
        assert_eq!(sides[2924].middle_tex, "MAKWOD12");
        assert_eq!(sides[2924].upper_tex, "");
        assert_eq!(sides[2925].lower_tex, "MAKMET02");
        assert_eq!(sides[2925].middle_tex, "MAKWOD12");
        assert_eq!(sides[2925].upper_tex, "");

        let mut map = LevelData::default();
        map.load(name, |_| None, &wad, None, None);
        // line 1590
        assert_eq!(map.linedefs[1590].v1.pos, Vec2::new(-560.0, -3952.0));
        assert_eq!(map.linedefs[1590].v2.pos, Vec2::new(-560.0, -3920.0));
        assert_eq!(map.linedefs[1590].front_sidedef.midtexture, Some(1657));
        assert_eq!(
            map.linedefs[1590].back_sidedef.as_ref().unwrap().midtexture,
            Some(1657)
        );
        assert_eq!(
            map.linedefs[1590]
                .back_sidedef
                .as_ref()
                .unwrap()
                .bottomtexture,
            Some(1628)
        );
        assert_eq!(
            map.linedefs[1590].back_sidedef.as_ref().unwrap().toptexture,
            None
        );
    }

    #[test]
    fn test_tracing_bsp() {
        let wad = WadData::new(&doom1_wad_path());
        let mut map = LevelData::default();
        map.load("E1M1", |_| None, &wad, None, None);
        let ox = FixedT::from_f32(710.0);
        let oy = FixedT::from_f32(-3400.0);
        let ex = FixedT::from_f32(710.0);
        let ey = FixedT::from_f32(-3000.0);

        let mut bsp_trace = BSPTrace::new_line(ox, oy, ex, ey, FixedT::from_f32(1.0));

        let sub_sect = map.subsectors();
        let segs = map.segments();

        // BSP trace should find valid subsectors along a vertical line
        let mut count = 0;
        bsp_trace.origin_x = FixedT::from_f32(710.0);
        bsp_trace.origin_y = FixedT::from_f32(-3400.0);
        bsp_trace.nodes.clear();
        bsp_trace.find_intercepts(map.start_node(), &map, &mut count);

        // Should find at least one subsector
        assert!(
            !bsp_trace.nodes.is_empty(),
            "BSP trace should find subsectors"
        );

        // First subsector should have valid segs
        let first_ss = bsp_trace.nodes.as_slice().first().unwrap();
        let start = sub_sect[*first_ss as usize].start_seg as usize;
        let seg_count = sub_sect[*first_ss as usize].seg_count as usize;
        assert!(seg_count > 0, "Subsector should have segs");
        assert!(start + seg_count <= segs.len(), "Seg range should be valid");
    }

    #[test]
    fn check_e1m1_things() {
        let wad = WadData::new(&doom1_wad_path());
        let mut map = LevelData::default();
        map.load("E1M1", |_| None, &wad, None, None);

        let things = &map.things();
        assert_eq!(things[0].x as i32, 1056);
        assert_eq!(things[0].y as i32, -3616);
        assert_eq!(things[0].angle, 90);
        assert_eq!(things[0].kind, 1);
        assert_eq!(things[0].flags, 7);
        assert_eq!(things[137].x as i32, 3648);
        assert_eq!(things[137].y as i32, -3840);
        assert_eq!(things[137].angle, 0);
        assert_eq!(things[137].kind, 2015);
        assert_eq!(things[137].flags, 7);

        assert_eq!(things[0].angle, 90);
        assert_eq!(things[9].angle, 135);
        assert_eq!(things[14].angle, 0);
        assert_eq!(things[16].angle, 90);
        assert_eq!(things[17].angle, 180);
        assert_eq!(things[83].angle, 270);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn check_e1m1_lump_pointers() {
        let wad = WadData::new(&doom1_wad_path());
        let mut map = LevelData::default();
        map.load("E1M1", |_| None, &wad, None, None);

        let linedefs = map.linedefs();

        // Builder may remove zero-length linedefs, but the first few should survive
        // Check LINEDEF->VERTEX chain is intact
        assert!(linedefs.len() > 400, "Should have many linedefs");
        // Linedef 2 is a non-degenerate line, should survive cleanup
        assert_eq!(linedefs[2].v1.x as i32, 1088);
        assert_eq!(linedefs[2].v2.x as i32, 1088);
        // LINEDEF->SIDEDEF->SECTOR chain
        assert_eq!(linedefs[2].front_sidedef.sector.ceilingheight, 72.0);

        let segments = map.segments();
        // Segments should exist and have valid vertex pointers
        assert!(segments.len() > 500, "Should have many segments");
        // Every segment should have valid linedef reference
        for seg in segments.iter() {
            assert!(seg.linedef.num < linedefs.len());
        }
    }

    #[test]
    fn check_e1m1_linedefs() {
        let wad = WadData::new(&doom1_wad_path());
        let mut map = LevelData::default();
        map.load("E1M1", |_| None, &wad, None, None);

        let linedefs = map.linedefs();
        assert_eq!(linedefs[0].v1.x as i32, 1088);
        assert_eq!(linedefs[0].v2.x as i32, 1024);
        assert_eq!(linedefs[2].v1.x as i32, 1088);
        assert_eq!(linedefs[2].v2.x as i32, 1088);

        assert_eq!(linedefs[474].v1.x as i32, 3536);
        assert_eq!(linedefs[474].v2.x as i32, 3520);
        assert!(linedefs[2].back_sidedef.is_none());
        assert_eq!(linedefs[474].flags, LineDefFlags::Blocking);
        assert!(linedefs[474].back_sidedef.is_none());
        assert!(linedefs[466].back_sidedef.is_some());

        // Flag check
        assert_eq!(
            linedefs[26].flags,
            LineDefFlags::Blocking
                | LineDefFlags::TwoSided
                | LineDefFlags::UnpegTop
                | LineDefFlags::UnpegBottom
        );
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn check_e1m1_sectors() {
        let wad = WadData::new(&doom1_wad_path());
        let mut map = LevelData::default();
        map.load("E1M1", |_| None, &wad, None, None);

        let sectors = map.sectors();
        assert_eq!(sectors[0].floorheight, 0.0);
        assert_eq!(sectors[0].ceilingheight, 72.0);
        assert_eq!(sectors[0].lightlevel, 160);
        assert_eq!(sectors[0].tag, 0);
        assert_eq!(sectors[84].floorheight, -24.0);
        assert_eq!(sectors[84].ceilingheight, 48.0);
        assert_eq!(sectors[84].lightlevel, 255);
        assert_eq!(sectors[84].special, 0);
        assert_eq!(sectors[84].tag, 0);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn check_e1m1_sidedefs() {
        let wad = WadData::new(&doom1_wad_path());
        let mut map = LevelData::default();
        map.load("E1M1", |_| None, &wad, None, None);

        let sidedefs = map.sidedefs();
        assert_eq!(sidedefs[0].rowoffset, 0i32);
        assert_eq!(sidedefs[0].textureoffset, 0i32);
        assert_eq!(sidedefs[9].rowoffset, 48i32);
        assert_eq!(sidedefs[9].textureoffset, 0i32);
        assert_eq!(sidedefs[647].rowoffset, 0i32);
        assert_eq!(sidedefs[647].textureoffset, 4i32);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn check_e1m1_segments() {
        let wad = WadData::new(&doom1_wad_path());
        let mut map = LevelData::default();
        map.load("E1M1", |_| None, &wad, None, None);

        let segments = map.segments();
        assert!(segments.len() > 500, "Should have many segments");

        // Every segment should have a valid angle computed from its vertices
        for (i, seg) in segments.iter().enumerate() {
            let dx = seg.v2.x - seg.v1.x;
            let dy = seg.v2.y - seg.v1.y;
            let len = (dx * dx + dy * dy).sqrt();
            if len < 1e-6 {
                continue; // skip degenerate zero-length segs
            }
            let expected: Angle = Angle::new(dy.atan2(dx));
            // Compare using unit vectors to handle wrapping
            let diff = (seg.angle.rad() - expected.rad()).abs();
            let wrapped_diff = diff.min((2.0 * PI - diff).abs());
            assert!(
                wrapped_diff < 1e-3,
                "Segment {} angle mismatch: got {} expected {}",
                i,
                seg.angle.rad(),
                expected.rad()
            );
        }

        let subsectors = map.subsectors();
        assert!(subsectors.len() > 100, "Should have many subsectors");
        // Every subsector should have at least 1 seg
        for ss in subsectors.iter() {
            assert!(ss.seg_count >= 1, "Subsector should have segs");
            let end = ss.start_seg as usize + ss.seg_count as usize;
            assert!(end <= segments.len(), "Seg range should be valid");
        }
    }

    #[test]
    fn find_vertex_using_bsptree() {
        let wad = WadData::new(&doom1_wad_path());
        let mut map = LevelData::default();
        map.load("E1M1", |_| None, &wad, None, None);

        // The actual location of THING0 (map units << 16 for fixed-point)
        let subsector = map.point_in_subsector(FixedT::from(1056i32), FixedT::from(-3616i32));
        // Should find a valid subsector with segs
        assert!(subsector.seg_count >= 1, "Should have segs");
        let end = subsector.start_seg as usize + subsector.seg_count as usize;
        assert!(end <= map.segments().len(), "Seg range should be valid");
    }

    #[test]
    fn check_nodes_of_e1m1() {
        let wad = WadData::new(&doom1_wad_path());
        let mut map = LevelData::default();
        map.load("E1M1", |_| None, &wad, None, None);

        let nodes = map.get_nodes();
        assert!(nodes.len() > 100, "Should have many BSP nodes");

        // Every node child should be either a valid node index or a valid subsector ref
        let num_subsectors = map.subsectors().len();
        for node in nodes.iter() {
            for &child in &node.children {
                if child & IS_SSECTOR_MASK != 0 {
                    let ss_idx = (child & !IS_SSECTOR_MASK) as usize;
                    assert!(ss_idx < num_subsectors, "Subsector index out of range");
                } else {
                    assert!((child as usize) < nodes.len(), "Node index out of range");
                }
            }
        }

        // Root node should exist and have non-zero delta
        let root = &nodes[nodes.len() - 1];
        let len = (root.delta.x * root.delta.x + root.delta.y * root.delta.y).sqrt();
        assert!(len > 0.0, "Root node should have non-zero partition line");
    }
}
