use std::{path::PathBuf, str::FromStr};

use criterion::*;

use gameplay::MapData;
use wad::wad::WadData;

fn bench_load_e1m1(b: &mut Bencher) {
    let wad = WadData::new(PathBuf::from_str("../doom1.wad").unwrap());
    let mut map = MapData::new("E1M1".to_owned());
    b.iter(|| {
        map.load(&wad);
    });
}

fn bench_load_e1m7(b: &mut Bencher) {
    let wad = WadData::new(PathBuf::from_str("../doom1.wad").unwrap());
    let mut map = MapData::new("E1M7".to_owned());
    b.iter(|| {
        map.load(&wad);
    });
}

fn bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("Loading E1M1");

    group.bench_function("Load e1m1 from shareware", bench_load_e1m1);
    group.bench_function("Load e1m7 from shareware", bench_load_e1m7);
}

criterion_group!(benches, bench,);
criterion_main!(benches);
