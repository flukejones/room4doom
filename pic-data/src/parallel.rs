//! Minimal scoped-thread parallel map.
//!
//! Used for independent-per-item load work (voxel model build, GPU face
//! generation) without a thread-pool dependency; serial for tiny inputs.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

/// Run `f` over `items` across scoped threads, preserving input order.
///
/// Work-stealing by atomic cursor (each worker claims the next index), so a few
/// expensive items don't strand the other cores — critical when per-item cost
/// varies widely. Serial for `items.len() <= 1` or a single core.
pub fn parallel_map<T, R, F>(items: &[T], f: F) -> Vec<R>
where
    T: Sync,
    R: Send,
    F: Fn(&T) -> R + Sync,
{
    let n = items.len();
    let cores = thread::available_parallelism()
        .map(|c| c.get())
        .unwrap_or(1);
    if n <= 1 || cores <= 1 {
        return items.iter().map(&f).collect();
    }
    let workers = cores.min(n);
    let cursor = AtomicUsize::new(0);
    let mut out: Vec<Vec<(usize, R)>> = thread::scope(|scope| {
        let (cursor, f) = (&cursor, &f);
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(move || {
                    let mut local = Vec::new();
                    loop {
                        let i = cursor.fetch_add(1, Ordering::Relaxed);
                        if i >= n {
                            break;
                        }
                        local.push((i, f(&items[i])));
                    }
                    local
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("worker"))
            .collect()
    });

    let mut results: Vec<Option<R>> = (0..n).map(|_| None).collect();
    for chunk in out.drain(..) {
        for (i, r) in chunk {
            results[i] = Some(r);
        }
    }
    results
        .into_iter()
        .map(|o| o.expect("every index produced"))
        .collect()
}
