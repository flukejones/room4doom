mod viewer;

use argh::FromArgs;
use level::LevelData;
use log::info;
use serde::Serialize;
use std::path::PathBuf;
use wad::WadData;

/// BSP geometry inspection tool
#[derive(FromArgs)]
struct Args {
    /// path to IWAD file
    #[argh(option, short = 'i')]
    iwad: String,
    /// path to PWAD file (repeatable)
    #[argh(option, short = 'p')]
    pwad: Vec<String>,
    /// map lump name (e.g. E1M1, MAP01)
    #[argh(option, short = 'm')]
    map: String,
    #[argh(subcommand)]
    command: Option<Command>,
}

#[derive(FromArgs)]
#[argh(subcommand)]
enum Command {
    Stats(StatsCmd),
    Sectors(SectorsCmd),
    Diag(DiagCmd),
    Dump(DumpCmd),
    View(ViewCmd),
}

/// show summary statistics
#[derive(FromArgs)]
#[argh(subcommand, name = "stats")]
struct StatsCmd {}

/// dump sector info as JSON
#[derive(FromArgs)]
#[argh(subcommand, name = "sectors")]
struct SectorsCmd {}

/// diagnose subsector polygon issues
#[derive(FromArgs)]
#[argh(subcommand, name = "diag")]
struct DiagCmd {}

/// dump map geometry and BSP data to JSON file
#[derive(FromArgs)]
#[argh(subcommand, name = "dump")]
struct DumpCmd {
    /// output file path (default: <map>.json)
    #[argh(option, short = 'o')]
    output: Option<String>,
}

/// launch interactive BSP geometry viewer
#[derive(FromArgs)]
#[argh(subcommand, name = "view")]
struct ViewCmd {}

fn load_map(args: &Args, wad: &WadData) -> std::pin::Pin<Box<LevelData>> {
    let flat_lookup: std::collections::HashMap<String, usize> = wad
        .flats_iter()
        .enumerate()
        .map(|(i, f)| (f.name.clone(), i))
        .collect();

    let mut level_data = Box::pin(LevelData::default());
    unsafe { level_data.as_mut().get_unchecked_mut() }.load(
        &args.map,
        |name| flat_lookup.get(name).copied(),
        wad,
        None,
        None,
    );
    level_data
}

fn load_wad(args: &Args) -> WadData {
    let wad_path: PathBuf = args.iwad.clone().into();
    let mut wad = WadData::new(&wad_path);
    for pwad in &args.pwad {
        wad.add_file(pwad.into());
    }
    wad
}

#[derive(Serialize)]
struct StatsOutput {
    map: String,
    sectors: usize,
    subsectors: usize,
    segments: usize,
    linedefs: usize,
    vertices: usize,
    bsp3d_vertices: usize,
    bsp3d_polygons: usize,
    bsp3d_nodes: usize,
}

#[derive(Serialize)]
struct SectorOutput {
    id: usize,
    floor_height: f32,
    ceiling_height: f32,
    light_level: usize,
    tag: i16,
    kind: i16,
}

fn cmd_stats(args: &Args) {
    let wad = load_wad(args);
    let level_data = load_map(args, &wad);

    let poly_count: usize = level_data
        .bsp_3d
        .subsector_leaves
        .iter()
        .map(|l| l.polygons.len())
        .sum();

    let output = StatsOutput {
        map: args.map.clone(),
        sectors: level_data.sectors.len(),
        subsectors: level_data.subsectors.len(),
        segments: level_data.segments.len(),
        linedefs: level_data.linedefs.len(),
        vertices: level_data.vertexes.len(),
        bsp3d_vertices: level_data.bsp_3d.vertices.len(),
        bsp3d_polygons: poly_count,
        bsp3d_nodes: level_data.bsp_3d.nodes().len(),
    };
    println!("{}", serde_json::to_string_pretty(&output).unwrap());
}

fn cmd_sectors(args: &Args) {
    let wad = load_wad(args);
    let level_data = load_map(args, &wad);
    let sectors: Vec<SectorOutput> = level_data
        .sectors
        .iter()
        .enumerate()
        .map(|(i, s)| SectorOutput {
            id: i,
            floor_height: s.floorheight.to_f32(),
            ceiling_height: s.ceilingheight.to_f32(),
            light_level: s.lightlevel,
            tag: s.tag,
            kind: s.special,
        })
        .collect();
    println!("{}", serde_json::to_string_pretty(&sectors).unwrap());
}

fn cmd_diag(args: &Args) {
    use glam::Vec2;

    let wad = load_wad(args);
    let level_data = load_map(args, &wad);
    let carved = &level_data.bsp_3d.carved_polygons;

    let extents = level_data.get_map_extents();
    let map_w = extents.max_vertex.x - extents.min_vertex.x;
    let map_h = extents.max_vertex.y - extents.min_vertex.y;
    let map_area = map_w * map_h;

    println!(
        "Map: {} ({} subsectors, extents {:.0}x{:.0}, area {:.0})",
        args.map,
        carved.len(),
        map_w,
        map_h,
        map_area
    );

    let mut empty = 0;
    let mut degenerate = 0;
    let mut non_convex = 0;
    let mut oversized = 0;
    let mut world_bound = 0;

    for (i, poly) in carved.iter().enumerate() {
        let sector_id = level_data.subsectors[i].sector.num as usize;

        if poly.len() < 3 {
            if poly.is_empty() {
                empty += 1;
            } else {
                degenerate += 1;
                println!(
                    "  ss{i} (sector {sector_id}): DEGENERATE ({} verts)",
                    poly.len()
                );
            }
            continue;
        }

        let has_world_bound = poly
            .iter()
            .any(|v| v.x.abs() > 30000.0 || v.y.abs() > 30000.0);
        if has_world_bound {
            world_bound += 1;
            let min = poly
                .iter()
                .fold(Vec2::splat(f32::MAX), |acc, v| acc.min(*v));
            let max = poly
                .iter()
                .fold(Vec2::splat(f32::MIN), |acc, v| acc.max(*v));
            println!(
                "  ss{i} (sector {sector_id}): WORLD_BOUND {} verts, bounds ({:.0},{:.0})-({:.0},{:.0})",
                poly.len(),
                min.x,
                min.y,
                max.x,
                max.y
            );
        }

        let area = polygon_area(poly).abs();
        if area > map_area * 0.1 {
            oversized += 1;
            println!(
                "  ss{i} (sector {sector_id}): OVERSIZED {} verts, area {:.0} ({:.1}% of map)",
                poly.len(),
                area,
                area / map_area * 100.0
            );
        }

        let n = poly.len();
        let mut pos = 0;
        let mut neg = 0;
        let mut min_cross = f32::MAX;
        let mut max_cross = f32::MIN;
        for j in 0..n {
            let a = poly[j];
            let b = poly[(j + 1) % n];
            let c = poly[(j + 2) % n];
            let cross = (b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x);
            if cross > 1e-4 {
                pos += 1;
                min_cross = min_cross.min(cross);
                max_cross = max_cross.max(cross);
            } else if cross < -1e-4 {
                neg += 1;
                min_cross = min_cross.min(cross);
                max_cross = max_cross.max(cross);
            }
        }
        if pos > 0 && neg > 0 {
            non_convex += 1;
            let outlier = if pos < neg { max_cross } else { min_cross };
            println!(
                "  ss{i} (sector {sector_id}): NON_CONVEX {} verts, +{pos}/-{neg} cross products (outlier: {outlier:.6})",
                poly.len()
            );
        }
    }

    println!("\nSummary:");
    println!("  Total subsectors: {}", carved.len());
    println!("  Empty (0 verts):  {empty}");
    println!("  Degenerate (<3):  {degenerate}");
    println!("  Non-convex:       {non_convex}");
    println!("  Oversized:        {oversized}");
    println!("  World-bound:      {world_bound}");
    println!(
        "  OK:               {}",
        carved.len() - empty - degenerate - non_convex - oversized - world_bound
    );
}

fn polygon_area(verts: &[glam::Vec2]) -> f32 {
    let n = verts.len();
    let mut area = 0.0;
    for i in 0..n {
        let j = (i + 1) % n;
        area += verts[i].x * verts[j].y;
        area -= verts[j].x * verts[i].y;
    }
    area * 0.5
}

#[derive(Serialize)]
struct DumpOutput {
    map: String,
    vertices: Vec<[f32; 2]>,
    sectors: Vec<DumpSector>,
    subsectors: Vec<DumpSubsector>,
    linedefs: Vec<DumpLinedef>,
    segments: Vec<DumpSegment>,
    nodes: Vec<DumpNode>,
    sector_subsectors: Vec<Vec<usize>>,
    carved_polygons: Vec<Vec<[f32; 2]>>,
}

#[derive(Serialize)]
struct DumpSector {
    id: usize,
    floor_height: f32,
    ceiling_height: f32,
    light_level: usize,
    tag: i16,
    special: i16,
}

#[derive(Serialize)]
struct DumpSubsector {
    id: usize,
    sector_id: usize,
    seg_count: u32,
    start_seg: u32,
}

#[derive(Serialize)]
struct DumpLinedef {
    id: usize,
    v1: usize,
    v2: usize,
    flags: u32,
    special: i16,
    tag: i16,
    front_sector: usize,
    back_sector: Option<usize>,
    two_sided: bool,
}

#[derive(Serialize)]
struct DumpSegment {
    id: usize,
    v1: [f32; 2],
    v2: [f32; 2],
    linedef: usize,
    front_sector: usize,
    back_sector: Option<usize>,
}

#[derive(Serialize)]
struct DumpNode {
    id: usize,
    xy: [f32; 2],
    delta: [f32; 2],
    children: [u32; 2],
    bboxes: [[[f32; 2]; 2]; 2],
}

fn cmd_dump(args: &Args, cmd: &DumpCmd) {
    let wad = load_wad(args);
    let level_data = load_map(args, &wad);
    let vert_base = level_data.vertexes.as_ptr();

    let carved = &level_data.bsp_3d.carved_polygons;

    let output = DumpOutput {
        map: args.map.clone(),
        vertices: level_data.vertexes.iter().map(|v| [v.x, v.y]).collect(),
        sectors: level_data
            .sectors
            .iter()
            .enumerate()
            .map(|(i, s)| DumpSector {
                id: i,
                floor_height: s.floorheight.to_f32(),
                ceiling_height: s.ceilingheight.to_f32(),
                light_level: s.lightlevel,
                tag: s.tag,
                special: s.special,
            })
            .collect(),
        subsectors: level_data
            .subsectors
            .iter()
            .enumerate()
            .map(|(i, ss)| DumpSubsector {
                id: i,
                sector_id: ss.sector.num as usize,
                seg_count: ss.seg_count,
                start_seg: ss.start_seg,
            })
            .collect(),
        linedefs: level_data
            .linedefs
            .iter()
            .enumerate()
            .map(|(i, ld)| {
                let v1_idx = unsafe { ld.v1.as_ptr().offset_from(vert_base as *mut _) as usize };
                let v2_idx = unsafe { ld.v2.as_ptr().offset_from(vert_base as *mut _) as usize };
                DumpLinedef {
                    id: i,
                    v1: v1_idx,
                    v2: v2_idx,
                    flags: ld.flags.bits(),
                    special: ld.special,
                    tag: ld.tag,
                    front_sector: ld.frontsector.num as usize,
                    back_sector: ld.backsector.as_ref().map(|s| s.num as usize),
                    two_sided: ld.back_sidedef.is_some(),
                }
            })
            .collect(),
        segments: level_data
            .segments
            .iter()
            .enumerate()
            .map(|(i, seg)| DumpSegment {
                id: i,
                v1: [seg.v1.x, seg.v1.y],
                v2: [seg.v2.x, seg.v2.y],
                linedef: seg.linedef.num,
                front_sector: seg.frontsector.num as usize,
                back_sector: seg.backsector.as_ref().map(|s| s.num as usize),
            })
            .collect(),
        nodes: level_data
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| DumpNode {
                id: i,
                xy: [n.xy.x, n.xy.y],
                delta: [n.delta.x, n.delta.y],
                children: n.children,
                bboxes: [
                    [
                        [n.bboxes[0][0].x, n.bboxes[0][0].y],
                        [n.bboxes[0][1].x, n.bboxes[0][1].y],
                    ],
                    [
                        [n.bboxes[1][0].x, n.bboxes[1][0].y],
                        [n.bboxes[1][1].x, n.bboxes[1][1].y],
                    ],
                ],
            })
            .collect(),
        sector_subsectors: level_data.bsp_3d.sector_subsectors.clone(),
        carved_polygons: carved
            .iter()
            .map(|poly| poly.iter().map(|v| [v.x, v.y]).collect())
            .collect(),
    };

    let json = serde_json::to_string_pretty(&output).unwrap();
    let output_path = cmd
        .output
        .clone()
        .unwrap_or_else(|| format!("{}.json", args.map));
    std::fs::write(&output_path, &json).expect("Failed to write output file");
    info!("Dumped {} to {}", args.map, output_path);
}

fn cmd_view(args: &Args) {
    let wad = load_wad(args);
    let level_data = load_map(args, &wad);
    let data = viewer::extract_viewer_data(&args.map, &level_data, &wad);
    viewer::run(data);
}

fn main() {
    let args: Args = argh::from_env();

    simplelog::TermLogger::init(
        log::LevelFilter::Info,
        simplelog::ConfigBuilder::default()
            .set_time_level(log::LevelFilter::Debug)
            .build(),
        simplelog::TerminalMode::Stderr,
        simplelog::ColorChoice::Auto,
    )
    .ok();

    match &args.command {
        Some(Command::Stats(_)) => cmd_stats(&args),
        Some(Command::Sectors(_)) => cmd_sectors(&args),
        Some(Command::Diag(_)) => cmd_diag(&args),
        Some(Command::Dump(cmd)) => cmd_dump(&args, cmd),
        Some(Command::View(_)) | None => cmd_view(&args),
    }
}
