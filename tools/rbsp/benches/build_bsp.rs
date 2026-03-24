use std::path::Path;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use wad::WadData;
use wad::wad::MapLump;

fn load_input(wad_path: &str, map_name: &str) -> rbsp::BspInput {
    let wad = WadData::new(Path::new(wad_path));
    rbsp::BspInput {
        vertices: wad.map_iter(map_name, MapLump::Vertexes).collect(),
        linedefs: wad.map_iter(map_name, MapLump::LineDefs).collect(),
        sidedefs: wad.map_iter(map_name, MapLump::SideDefs).collect(),
        sectors: wad.map_iter(map_name, MapLump::Sectors).collect(),
    }
}

struct MapCase {
    name: &'static str,
    wad: &'static str,
    map: &'static str,
}

const CASES: &[MapCase] = &[
    MapCase {
        name: "E1M8B",
        wad: "/Users/lukejones/DOOM/E1M8B.WAD",
        map: "E1M8",
    },
    MapCase {
        name: "Sunder-MAP03",
        wad: "/Users/lukejones/DOOM/sunder.wad",
        map: "MAP03",
    },
];

fn bench_build_bsp(c: &mut Criterion) {
    let mut group = c.benchmark_group("build_bsp");

    for case in CASES {
        if !Path::new(case.wad).exists() {
            eprintln!("Skipping {}: {} not found", case.name, case.wad);
            continue;
        }

        for w in [8, 11, 14] {
            let opts = rbsp::BspOptions {
                split_weight: w as f64,
            };
            group.bench_with_input(
                BenchmarkId::new(format!("{}/w{}", case.name, w), case.map),
                &(),
                |b, _| {
                    b.iter_with_setup(
                        || load_input(case.wad, case.map),
                        |input| rbsp::build_bsp(input, &opts),
                    );
                },
            );
        }
    }

    group.finish();
}

criterion_group!(benches, bench_build_bsp);
criterion_main!(benches);
