use test_utils::{doom1_wad_path, sunder_wad_path};
use wad::extended::{NodeLumpType, WadExtendedMap};
use wad::types::*;
use wad::{MapLump, WadData};

#[ignore = "sunder.wad can't be included in git"]
#[test]
fn extended_nodes_sunder_m3_check_vertex() {
    let name = "MAP03";
    let wad = WadData::new(&sunder_wad_path());
    let map = WadExtendedMap::parse(&wad, name).unwrap();

    assert_eq!(map.num_org_vertices, 5525);
    assert_eq!(map.num_new_vertices, 996);
    assert_eq!(map.vertexes.len(), 996);

    assert_eq!(map.vertexes[0].x, 1072.0);
    assert_eq!(map.vertexes[0].y, -256.0);
    assert_eq!(map.vertexes[995].x, -2832.0);
    assert_eq!(map.vertexes[995].y, -1872.0);
}

#[ignore = "sunder.wad can't be included in git"]
#[test]
fn extended_nodes_sunder_m3_check_subs() {
    let name = "MAP03";
    let wad = WadData::new(&sunder_wad_path());
    let map = WadExtendedMap::parse(&wad, name).unwrap();
    assert_eq!(map.subsectors.len(), 4338);

    assert_eq!(map.subsectors[1130].start_seg, 3834);
    assert_eq!(map.subsectors[1130].seg_count, 4);
    assert_eq!(map.subsectors[2770].start_seg, 9445);
    assert_eq!(map.subsectors[2770].seg_count, 5);
    assert_eq!(map.subsectors[4237].start_seg, 14226);
    assert_eq!(map.subsectors[4237].seg_count, 4);
}

#[ignore = "sunder.wad can't be included in git"]
#[test]
fn extended_nodes_sunder_m3_check_segs() {
    let name = "MAP03";
    let wad = WadData::new(&sunder_wad_path());
    let map = WadExtendedMap::parse(&wad, name).unwrap();
    assert_eq!(map.segments.len(), 14582);

    assert_eq!(map.segments[7990].start_vertex, 2932);
    assert_eq!(map.segments[7990].end_vertex, 6083);
    assert_eq!(map.segments[7990].linedef, 3352);
    assert_eq!(map.segments[7990].side, 0);
}

#[ignore = "sunder.wad can't be included in git"]
#[test]
fn extended_nodes_sunder_m3_check_nodes() {
    let name = "MAP03";
    let wad = WadData::new(&sunder_wad_path());
    let map = WadExtendedMap::parse(&wad, name).unwrap();
    let node = map.nodes[666].clone();
    assert_eq!(
        node,
        WadNode {
            x: 12,
            y: -342,
            dx: 0,
            dy: -20,
            bboxes: [
                [
                    node.bboxes[0][0],
                    node.bboxes[0][1],
                    node.bboxes[0][2],
                    node.bboxes[0][3]
                ],
                [
                    node.bboxes[1][0],
                    node.bboxes[1][1],
                    node.bboxes[1][2],
                    node.bboxes[1][3]
                ]
            ],
            children: [node.children[0], node.children[1]],
        }
    );
    assert_eq!(node.x, 12);
    assert_eq!(node.y, -342);
    assert_eq!(node.dx, 0);
    assert_eq!(node.dy, -20);
}

#[ignore = "sunder.wad can't be included in git"]
#[test]
fn extended_nodes_sunder_m3() {
    let name = "MAP03";
    let wad = WadData::new(&sunder_wad_path());
    let map = WadExtendedMap::parse(&wad, name).unwrap();

    assert_eq!(map.num_org_vertices, 5525);
    assert_eq!(map.vertexes.len(), 996);
    assert_eq!(map.subsectors.len(), 4338);
    assert_eq!(map.segments.len(), 14582);
    assert_eq!(map.nodes.len(), 4337);

    let sectors: Vec<WadSector> = wad.map_iter::<WadSector>(name, MapLump::Sectors).collect();
    assert_eq!(sectors.len(), 954);

    let linedefs: Vec<WadLineDef> = wad
        .map_iter::<WadLineDef>(name, MapLump::LineDefs)
        .collect();
    assert_eq!(linedefs.len(), 7476);
}

#[test]
fn doom1_node_lump_type() {
    let wad = WadData::new(&doom1_wad_path());
    assert!(matches!(wad.node_lump_type("E1M1"), NodeLumpType::OGDoom));
}

#[ignore = "sunder.wad can't be included in git"]
#[test]
fn sunder_node_lump_type() {
    let wad = WadData::new(&sunder_wad_path());
    assert!(!matches!(wad.node_lump_type("MAP03"), NodeLumpType::OGDoom));
}

#[ignore = "sunder.wad can't be included in git"]
#[test]
fn extended_nodes_sunder_m19() {
    let name = "MAP19";
    let wad = WadData::new(&sunder_wad_path());
    let map = WadExtendedMap::parse(&wad, name).unwrap();

    assert_eq!(map.num_org_vertices, 55802);
    assert_eq!(map.vertexes.len(), 21241);
    assert_eq!(map.subsectors.len(), 51692);
    assert_eq!(map.segments.len(), 158867);
    assert_eq!(map.nodes.len(), 51691);

    let linedefs: Vec<WadLineDef> = wad
        .map_iter::<WadLineDef>(name, MapLump::LineDefs)
        .collect();
    assert_eq!(linedefs.len(), 65524);
}
