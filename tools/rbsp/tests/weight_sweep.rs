//! Sweep SPLIT_WEIGHT across a range and report BSP stats.

use std::path::Path;

const MAPS: &[(&str, &str)] = &[
    ("/Users/lukejones/DOOM/DOOM.WAD", "E1M1"),
    ("/Users/lukejones/DOOM/E1M8B.WAD", "E1M8"),
    ("/Users/lukejones/DOOM/sunder.wad", "MAP03"),
];

fn load_input(wad_path: &str, map_name: &str) -> rbsp::BspInput {
    use wad::wad::MapLump;
    let wad = wad::WadData::new(Path::new(wad_path));
    rbsp::BspInput {
        vertices: wad.map_iter(map_name, MapLump::Vertexes).collect(),
        linedefs: wad.map_iter(map_name, MapLump::LineDefs).collect(),
        sidedefs: wad.map_iter(map_name, MapLump::SideDefs).collect(),
        sectors: wad.map_iter(map_name, MapLump::Sectors).collect(),
    }
}

fn tree_depth(nodes: &[rbsp::Node], root: u32) -> (usize, f64) {
    let (mut count, mut sum, mut max_d) = (0usize, 0usize, 0usize);
    walk(nodes, root, 0, &mut count, &mut sum, &mut max_d);
    let avg = if count > 0 {
        sum as f64 / count as f64
    } else {
        0.0
    };
    return (max_d, avg);

    fn walk(
        nodes: &[rbsp::Node],
        child: u32,
        d: usize,
        c: &mut usize,
        s: &mut usize,
        m: &mut usize,
    ) {
        if child & rbsp::IS_SSECTOR_MASK != 0 {
            *c += 1;
            *s += d;
            *m = (*m).max(d);
            return;
        }
        let n = &nodes[child as usize];
        walk(nodes, n.child_right, d + 1, c, s, m);
        walk(nodes, n.child_left, d + 1, c, s, m);
    }
}

#[test]
fn sweep() {
    let weights: Vec<rbsp::Float> = (8..=16).map(|w| w as rbsp::Float).collect();

    for &(wad_path, map_name) in MAPS {
        if !Path::new(wad_path).exists() {
            eprintln!("Skipping: {} not found", wad_path);
            continue;
        }

        let wad_name = wad_path.rsplit('/').next().unwrap();
        eprintln!("\n=== {} {} ===", wad_name, map_name);
        eprintln!(
            "{:>6} {:>5} {:>5} {:>5} {:>5} {:>5} {:>5}",
            "weight", "ss", "segs", "nodes", "verts", "max_d", "avg_d"
        );

        for &w in &weights {
            let input = load_input(wad_path, map_name);
            let opts = rbsp::BspOptions {
                split_weight: w,
            };
            let output = rbsp::build_bsp(input, &opts);
            let (max_d, avg_d) = tree_depth(&output.nodes, output.root);

            eprintln!(
                "{:>6} {:>5} {:>5} {:>5} {:>5} {:>5} {:>5.1}",
                w as u32,
                output.subsectors.len(),
                output.segs.len(),
                output.nodes.len(),
                output.vertices.len(),
                max_d,
                avg_d,
            );
        }
    }
}
