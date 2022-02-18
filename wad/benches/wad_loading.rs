use std::{path::PathBuf, str::FromStr};

use criterion::*;
use wad::wad::WadData;

fn bench_doom1(b: &mut Bencher) {
    b.iter(|| {
        let mut _wad = WadData::new(PathBuf::from_str("../doom1.wad").unwrap());
    });
}

fn bench_doom(b: &mut Bencher) {
    b.iter(|| {
        let mut _wad = WadData::new(PathBuf::from_str("../doom.wad").unwrap());
    });
}

fn bench_doom2(b: &mut Bencher) {
    b.iter(|| {
        let mut _wad = WadData::new(PathBuf::from_str("../doom2.wad").unwrap());
    });
}

fn bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("WAD Loading");

    group.bench_function("Load and read shareware wad", bench_doom1);
    group.bench_function("Load and read ultimate wad", bench_doom);
    group.bench_function("Load and read Doom II wad", bench_doom2);
}

criterion_group!(benches, bench,);
criterion_main!(benches);
