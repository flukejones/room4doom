//! Profiling harness for `sector_loops` (the editor's sector-outline tracer), the
//! hot path of a large-map load. Loads a map and re-traces every sector N times so
//! a sampler (Instruments / samply) has steady work to attribute.
//!
//! Usage: `cargo run --release -p editor-core --example profile_sector_loops -- \
//!   <iwad> <map> [pwad] [iters]`
//! e.g.   `... -- ~/DOOM/doom2.wad MAP19 ~/DOOM/sunder.wad 200`

use std::hint::black_box;
use std::path::PathBuf;
use std::time::Instant;

use editor_core::{import_wad_map, sector_loops, sector_loops_all};
use wad::WadData;

fn main() {
    let mut args = std::env::args().skip(1);
    let iwad = PathBuf::from(args.next().expect("usage: <iwad> <map> [pwad] [iters]"));
    let map_name = args.next().expect("map name");
    // Third arg is the pwad path when it exists on disk, else it is the iter count.
    let third = args.next();
    let pwad = third
        .as_deref()
        .map(PathBuf::from)
        .filter(|p| p.exists());
    let iters: usize = if pwad.is_some() { args.next() } else { third }
        .and_then(|s| s.parse().ok())
        .unwrap_or(200);

    let mut wad = WadData::new(&iwad);
    if let Some(pwad) = &pwad {
        wad.add_file(pwad.clone());
    }
    let map = import_wad_map(&wad, &map_name).expect("map imports");
    let sectors = map.sectors.len() as u32;

    let t = Instant::now();
    let mut per = 0usize;
    for _ in 0..iters {
        for s in 0..sectors {
            per += black_box(sector_loops(&map, s)).len();
        }
    }
    let per_ms = t.elapsed().as_secs_f64() * 1000.0 / iters as f64;

    let t = Instant::now();
    let mut all = 0usize;
    for _ in 0..iters {
        all += black_box(sector_loops_all(&map))
            .iter()
            .map(Vec::len)
            .sum::<usize>();
    }
    let all_ms = t.elapsed().as_secs_f64() * 1000.0 / iters as f64;

    println!("{map_name}: {sectors} sectors, {iters} iters");
    println!("  per-sector sector_loops: {per_ms:.2}ms/map ({per} loops)");
    println!("  batch sector_loops_all:  {all_ms:.2}ms/map ({all} loops)");
    assert_eq!(per / iters, all / iters, "batch and per-sector loop counts differ");
}
