use criterion::*;
use wad::WadFile;

fn bench_iter_safe(b: &mut Bencher, _i: &u32) {
    b.iter(|| {
        let mut wad = WadFile::new("../doom1.wad");
        wad.load();
        wad.read_directories();
    });
}

fn bench(c: &mut Criterion) {
    let load_and_read = Fun::new("Load and read shareware wad", bench_iter_safe);
    let fun = vec![load_and_read];
    c.bench_functions("Iteration and modify", fun, 10000);
}

criterion_group!(benches, bench,);
criterion_main!(benches);
