use std::ptr::NonNull;

use criterion::*;

use gamelib::d_thinker::{ActionF, TestObject, Think, ThinkerAlloc, ThinkerType};

fn push_100_000(b: &mut Bencher, _i: &u32) {
    b.iter(|| {
        let mut links = ThinkerAlloc::new(100000);

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

fn load_and_iter(b: &mut Bencher, _i: &u32) {
    let mut links = ThinkerAlloc::new(100000);

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
        let mut count = 0;
        for obj in links.iter_mut() {
            count += obj.object().bad_ref::<TestObject>().x;
        }
    });
}

fn bench(c: &mut Criterion) {
    let fun = vec![
        Fun::new("Load up linked list and iter over", load_and_iter),
        Fun::new("Push linked list 100,000", push_100_000),
    ];
    c.bench_functions("Linked lists", fun, 10);
}

criterion_group!(benches, bench,);
criterion_main!(benches);
