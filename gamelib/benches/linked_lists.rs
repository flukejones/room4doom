use std::ptr::NonNull;

use criterion::*;

use gamelib::d_thinker::{ActionF, TestObject, Think, ThinkerAlloc, ThinkerType};

fn push_100_000(b: &mut Bencher) {
    let mut links = unsafe { ThinkerAlloc::new(100000) };
    b.iter(|| {
        for i in 0..100000 {
            links.push::<TestObject>(TestObject::create_thinker(
                ThinkerType::Test(TestObject {
                    x: i,
                    thinker: NonNull::dangling(),
                }),
                ActionF::None,
            ));
        }
    });
}

fn load_and_iter(b: &mut Bencher) {
    let mut links = unsafe { ThinkerAlloc::new(100000) };

    for i in 0..100000 {
        links.push::<TestObject>(TestObject::create_thinker(
            ThinkerType::Test(TestObject {
                x: i,
                thinker: NonNull::dangling(),
            }),
            ActionF::None,
        ));
    }

    b.iter(|| {
        let mut _count = 0;
        for obj in links.iter_mut() {
            _count += obj.obj_mut().bad_ref::<TestObject>().x;
        }
    });
}

fn bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("ThinkerAlloc stressing");

    group.bench_function("Push 100,000", push_100_000);
    group.bench_function("Iterate over 100,000", load_and_iter);
}

criterion_group!(benches, bench,);
criterion_main!(benches);
