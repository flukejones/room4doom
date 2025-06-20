#[cfg(test)]
mod map_data_tests {
    use crate::level::map_data::{BSPTrace, IS_SSECTOR_MASK, MapData};
    use crate::{Node, PicData};
    use glam::Vec2;
    use math::Angle;
    use std::f32::consts::{FRAC_PI_2, PI};
    use std::path::PathBuf;
    use wad::WadData;
    use wad::extended::WadExtendedMap;
    use wad::types::{WadLineDef, WadSideDef};

    #[ignore = "sunder.wad can't be included in git"]
    #[test]
    fn check_nodes_of_sunder_m3() {
        let wad = WadData::new(&PathBuf::from("/home/luke/DOOM/sunder.wad"));
        let ext = WadExtendedMap::parse(&wad, "MAP03").unwrap();
        assert_eq!(ext.num_org_vertices, 5525); // verified with crispy
        assert_eq!(ext.vertexes.len(), 996); // verified with crispy
        assert_eq!(ext.subsectors.len(), 4338);
        assert_eq!(ext.segments.len(), 14582);
        assert_eq!(ext.nodes.len(), 4337);

        let pic_data = PicData::default();
        let mut map = MapData::default();
        map.load("MAP03", &pic_data, &wad);

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
            if *seg.v1 == Vec2::new(496.0, -1072.0) && *seg.v2 == Vec2::new(496.0, -1040.0) {
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
        let wad = WadData::new(&PathBuf::from("/home/luke/DOOM/sunder.wad"));
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

        let lines: Vec<WadLineDef> = wad.linedef_iter(name).collect();
        assert_eq!(lines[1590].front_sidedef, 2924);
        assert_eq!(lines[1590].back_sidedef, Some(2925));

        let sides: Vec<WadSideDef> = wad.sidedef_iter(name).collect();
        assert_eq!(sides[2924].lower_tex, "");
        assert_eq!(sides[2924].middle_tex, "MAKWOD12");
        assert_eq!(sides[2924].upper_tex, "");
        assert_eq!(sides[2925].lower_tex, "MAKMET02");
        assert_eq!(sides[2925].middle_tex, "MAKWOD12");
        assert_eq!(sides[2925].upper_tex, "");

        let pic_data = PicData::default();
        let mut map = MapData::default();
        map.load("E1M1", &pic_data, &wad);
        // line 1590
        assert_eq!(map.linedefs[1590].v1, Vec2::new(-560.0, -3952.0));
        assert_eq!(map.linedefs[1590].v2, Vec2::new(-560.0, -3920.0));
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
        let wad = WadData::new(&PathBuf::from("../doom1.wad"));
        let mut map = MapData::default();
        map.load("E1M1", &PicData::default(), &wad);
        let origin = Vec2::new(710.0, -3400.0); // left corner from start
        let endpoint = Vec2::new(710.0, -3000.0); // 3 sectors up

        // let origin = Vec2::new(1056.0, -3616.0); // player start
        // let endpoint = Vec2::new(1088.0, -2914.0); // corpse ahead, 10?
        //let endpoint = Vec2::new(1340.0, -2884.0); // ?
        //let endpoint = Vec2::new(2912.0, -2816.0);

        let mut bsp_trace = BSPTrace::new_line(origin, endpoint, 1.0);
        // bsp_trace.trace_to_point(&map);
        // dbg!(&nodes.len());
        // dbg!(&nodes);

        let sub_sect = map.subsectors();
        // let segs = map.get_segments();
        // for x in nodes.iter() {
        //     //let x = nodes.last().unwrap();
        //     let start = sub_sect[*x as usize].start_seg as usize;
        //     let end = sub_sect[*x as usize].seg_count as usize + start;
        //     for seg in &segs[start..end] {
        //         dbg!(x);
        //         dbg!(sub_sect[*x as usize].seg_count);
        //         dbg!(&seg.v1);
        //         dbg!(&seg.v2);
        //     }
        // }

        let _endpoint = Vec2::new(710.0, -3000.0); // 3 sectors up
        let segs = map.segments();
        // wander around the coords of the subsector corner from player start
        let mut count = 0;
        for x in 705..895 {
            for y in -3551..-3361 {
                bsp_trace.origin = Vec2::new(x as f32, y as f32);
                bsp_trace.find_line_inner(map.start_node(), &map, &mut count);

                // Sector the starting vector is in. 3 segs attached
                let x = bsp_trace.intercepted_subsectors().first().unwrap();
                let start = sub_sect[*x as usize].start_seg as usize;

                // Bottom horizontal line
                assert_eq!(segs[start].v1.x, 832.0);
                assert_eq!(segs[start].v1.y, -3552.0);
                assert_eq!(segs[start].v2.x, 704.0);
                assert_eq!(segs[start].v2.y, -3552.0);
                // Left side of the pillar
                assert_eq!(segs[start + 1].v1.x, 896.0);
                assert_eq!(segs[start + 1].v1.y, -3360.0);
                assert_eq!(segs[start + 1].v2.x, 896.0);
                assert_eq!(segs[start + 1].v2.y, -3392.0);
                // Left wall
                assert_eq!(segs[start + 2].v1.x, 704.0);
                assert_eq!(segs[start + 2].v1.y, -3552.0);
                assert_eq!(segs[start + 2].v2.x, 704.0);
                assert_eq!(segs[start + 2].v2.y, -3360.0);

                // Last sector directly above starting vector
                let x = bsp_trace.intercepted_subsectors().last().unwrap();
                let start = sub_sect[*x as usize].start_seg as usize;

                assert_eq!(segs[start].v1.x, 896.0);
                assert_eq!(segs[start].v1.y, -3072.0);
                assert_eq!(segs[start].v2.x, 896.0);
                assert_eq!(segs[start].v2.y, -3104.0);
                assert_eq!(segs[start + 1].v1.x, 704.0);
                assert_eq!(segs[start + 1].v1.y, -3104.0);
                assert_eq!(segs[start + 1].v2.x, 704.0);
                assert_eq!(segs[start + 1].v2.y, -2944.0);
            }
        }
    }

    #[test]
    fn check_e1m1_things() {
        let wad = WadData::new(&PathBuf::from("../doom1.wad"));
        let mut map = MapData::default();
        map.load("E1M1", &PicData::default(), &wad);

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
        let wad = WadData::new(&PathBuf::from("../doom1.wad"));
        let mut map = MapData::default();
        map.load("E1M1", &PicData::default(), &wad);

        let linedefs = map.linedefs();

        // Check links
        // LINEDEF->VERTEX
        assert_eq!(linedefs[2].v1.x as i32, 1088);
        assert_eq!(linedefs[2].v2.x as i32, 1088);
        // // LINEDEF->SIDEDEF
        // assert_eq!(linedefs[2].front_sidedef.midtexture, "LITE3");
        // // LINEDEF->SIDEDEF->SECTOR
        // assert_eq!(linedefs[2].front_sidedef.sector.floorpic, "FLOOR4_8");
        // // LINEDEF->SIDEDEF->SECTOR
        assert_eq!(linedefs[2].front_sidedef.sector.ceilingheight, 72.0);

        let segments = map.segments();
        // SEGMENT->VERTEX
        assert_eq!(segments[0].v1.x as i32, 1552);
        assert_eq!(segments[0].v2.x as i32, 1552);
        // SEGMENT->LINEDEF->SIDEDEF->SECTOR
        // seg:0 -> line:152 -> side:209 -> sector:0 -> ceiltex:CEIL3_5
        // lightlevel:160 assert_eq!(
        //     segments[0].linedef.front_sidedef.sector.ceilingpic,
        //     "CEIL3_5"
        // );
        // // SEGMENT->LINEDEF->SIDEDEF
        // assert_eq!(segments[0].linedef.front_sidedef.toptexture, "BIGDOOR2");

        // let sides = map.get_sidedefs();
        // assert_eq!(sides[211].sector.ceilingpic, "TLITE6_4");
    }

    #[test]
    fn check_e1m1_linedefs() {
        let wad = WadData::new(&PathBuf::from("../doom1.wad"));
        let mut map = MapData::default();
        map.load("E1M1", &PicData::default(), &wad);

        let linedefs = map.linedefs();
        assert_eq!(linedefs[0].v1.x as i32, 1088);
        assert_eq!(linedefs[0].v2.x as i32, 1024);
        assert_eq!(linedefs[2].v1.x as i32, 1088);
        assert_eq!(linedefs[2].v2.x as i32, 1088);

        assert_eq!(linedefs[474].v1.x as i32, 3536);
        assert_eq!(linedefs[474].v2.x as i32, 3520);
        assert!(linedefs[2].back_sidedef.is_none());
        assert_eq!(linedefs[474].flags, 1);
        assert!(linedefs[474].back_sidedef.is_none());
        assert!(linedefs[466].back_sidedef.is_some());

        // Flag check
        assert_eq!(linedefs[26].flags, 29);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn check_e1m1_sectors() {
        let wad = WadData::new(&PathBuf::from("../doom1.wad"));
        let mut map = MapData::default();
        map.load("E1M1", &PicData::default(), &wad);

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
        let wad = WadData::new(&PathBuf::from("../doom1.wad"));
        let mut map = MapData::default();
        map.load("E1M1", &PicData::default(), &wad);

        let sidedefs = map.sidedefs();
        assert_eq!(sidedefs[0].rowoffset, 0.0);
        assert_eq!(sidedefs[0].textureoffset, 0.0);
        assert_eq!(sidedefs[9].rowoffset, 48.0);
        assert_eq!(sidedefs[9].textureoffset, 0.0);
        assert_eq!(sidedefs[647].rowoffset, 0.0);
        assert_eq!(sidedefs[647].textureoffset, 4.0);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn check_e1m1_segments() {
        let wad = WadData::new(&PathBuf::from("../doom1.wad"));
        let mut map = MapData::default();
        map.load("E1M1", &PicData::default(), &wad);

        let segments = map.segments();
        assert_eq!(segments[0].v1.x as i32, 1552);
        assert_eq!(segments[0].v2.x as i32, 1552);
        assert_eq!(segments[731].v1.x as i32, 3040);
        assert_eq!(segments[731].v2.x as i32, 2976);
        assert_eq!(segments[0].angle, Angle::new(FRAC_PI_2));

        assert_eq!(segments[731].angle, Angle::new(PI));

        let subsectors = map.subsectors();
        assert_eq!(subsectors[0].seg_count, 4);
        assert_eq!(subsectors[124].seg_count, 3);
        assert_eq!(subsectors[236].seg_count, 4);
        //assert_eq!(subsectors[0].start_seg.start_vertex.x as i32, 1552);
        //assert_eq!(subsectors[124].start_seg.start_vertex.x as i32, 472);
        //assert_eq!(subsectors[236].start_seg.start_vertex.x as i32, 3040);
    }

    #[test]
    fn find_vertex_using_bsptree() {
        let wad = WadData::new(&PathBuf::from("../doom1.wad"));
        let mut map = MapData::default();
        map.load("E1M1", &PicData::default(), &wad);

        // The actual location of THING0
        let player = Vec2::new(1056.0, -3616.0);
        let subsector = map.point_in_subsector_raw(player);
        //assert_eq!(subsector_id, Some(103));
        assert_eq!(subsector.seg_count, 5);
        assert_eq!(subsector.start_seg, 305);
    }

    #[test]
    fn check_nodes_of_e1m1() {
        let wad = WadData::new(&PathBuf::from("../doom1.wad"));
        let mut map = MapData::default();
        map.load("E1M1", &PicData::default(), &wad);

        let nodes = map.get_nodes();
        assert_eq!(nodes[0].xy.x as i32, 1552);
        assert_eq!(nodes[0].xy.y as i32, -2432);
        assert_eq!(nodes[0].delta.x as i32, 112);
        assert_eq!(nodes[0].delta.y as i32, 0);

        assert_eq!(nodes[0].bboxes[0][0].x as i32, 1552); //left
        assert_eq!(nodes[0].bboxes[0][0].y as i32, -2432); //top
        assert_eq!(nodes[0].bboxes[0][1].x as i32, 1664); //right
        assert_eq!(nodes[0].bboxes[0][1].y as i32, -2560); //bottom

        assert_eq!(nodes[0].bboxes[1][0].x as i32, 1600);
        assert_eq!(nodes[0].bboxes[1][0].y as i32, -2048);

        assert_eq!(nodes[0].children[0], 2147483648);
        assert_eq!(nodes[0].children[1], 2147483649);

        assert_eq!(nodes[235].xy.x as i32, 2176);
        assert_eq!(nodes[235].xy.y as i32, -3776);
        assert_eq!(nodes[235].delta.x as i32, 0);
        assert_eq!(nodes[235].delta.y as i32, -32);
        assert_eq!(nodes[235].children[0], 128);
        assert_eq!(nodes[235].children[1], 234);

        println!("{:#018b}", IS_SSECTOR_MASK);

        println!("00: {:#018b}", nodes[0].children[0]);
        println!("00: {:#018b}", nodes[0].children[1]);

        println!("01: {:#018b}", nodes[1].children[0]);
        println!("01: {:#018b}", nodes[1].children[1]);

        println!("02: {:#018b}", nodes[2].children[0]);
        println!("02: {:#018b}", nodes[2].children[1]);

        println!("03: {:#018b}", nodes[3].children[0]);
        println!("03: {:#018b}", nodes[3].children[1]);
    }
}
