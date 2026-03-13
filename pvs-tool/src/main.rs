mod viewer;

use argh::FromArgs;
use log::info;
use map_data::{MapData, PVS2D, PvsCluster, PvsData, PvsFile, RenderPvs};
use serde::Serialize;
use std::path::PathBuf;
use viewer::PvsInput;
use wad::WadData;

/// PVS inspection and debugging tool
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
    command: Command,
}

#[derive(FromArgs)]
#[argh(subcommand)]
enum Command {
    Stats(StatsCmd),
    Portals(PortalsCmd),
    Sectors(SectorsCmd),
    Build(BuildCmd),
    View(ViewCmd),
    Diag(DiagCmd),
    Trace(TraceCmd),
    Dump(DumpCmd),
}

/// show summary statistics
#[derive(FromArgs)]
#[argh(subcommand, name = "stats")]
struct StatsCmd {}

/// dump portal graph as JSON
#[derive(FromArgs)]
#[argh(subcommand, name = "portals")]
struct PortalsCmd {}

/// dump sector info as JSON
#[derive(FromArgs)]
#[argh(subcommand, name = "sectors")]
struct SectorsCmd {}

/// build PVS and save to cache
#[derive(FromArgs)]
#[argh(subcommand, name = "build")]
struct BuildCmd {}

/// diagnose subsector polygon issues
#[derive(FromArgs)]
#[argh(subcommand, name = "diag")]
struct DiagCmd {}

/// trace visibility between two sectors
#[derive(FromArgs)]
#[argh(subcommand, name = "trace")]
struct TraceCmd {
    /// first sector id
    #[argh(positional)]
    sector_a: usize,
    /// second sector id
    #[argh(positional)]
    sector_b: usize,
}

/// dump map geometry and BSP data to JSON file
#[derive(FromArgs)]
#[argh(subcommand, name = "dump")]
struct DumpCmd {
    /// output file path (default: <map>.json)
    #[argh(option, short = 'o')]
    output: Option<String>,
}

/// open interactive 2D map viewer
#[derive(FromArgs)]
#[argh(subcommand, name = "view")]
struct ViewCmd {
    /// skip PVS computation (faster startup, no PVS/portal layers)
    #[argh(switch)]
    no_pvs: bool,
    /// use cluster-based PVS instead of PVS2D (skeleton: all-visible)
    #[argh(switch)]
    cluster: bool,
}

fn load_map(args: &Args) -> std::pin::Pin<Box<MapData>> {
    let wad_path: PathBuf = args.iwad.clone().into();
    let mut wad = WadData::new(&wad_path);
    for pwad in &args.pwad {
        wad.add_file(pwad.into());
    }

    let mut map_data = Box::pin(MapData::default());
    // Safety: load() populates internal MapPtr raw pointers that reference
    // MapData's own vecs. Pin ensures the heap allocation won't move.
    unsafe { map_data.as_mut().get_unchecked_mut() }.load(&args.map, |_| Some(0), &wad, None);
    map_data
}

fn load_wad(args: &Args) -> WadData {
    let wad_path: PathBuf = args.iwad.clone().into();
    let mut wad = WadData::new(&wad_path);
    for pwad in &args.pwad {
        wad.add_file(pwad.into());
    }
    wad
}

// ── JSON output types ──

#[derive(Serialize)]
struct StatsOutput {
    map: String,
    sectors: usize,
    subsectors: usize,
    segments: usize,
    linedefs: usize,
    vertices: usize,
    subsector_vis_pairs: u64,
    subsector_total_pairs: u64,
    subsector_cull_pct: f64,
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
    let mut map_data = MapData::default();
    map_data.load(&args.map, |_| Some(0), &wad, None);

    let vertex_count = map_data.vertexes.len();

    let MapData {
        subsectors,
        segments,
        bsp_3d,
        sectors,
        linedefs,
        nodes,
        start_node,
        ..
    } = &mut map_data;

    let pvs2d = PVS2D::build(
        subsectors,
        segments,
        bsp_3d,
        sectors,
        linedefs,
        nodes,
        *start_node,
    );
    let pvs = pvs2d.clone_render_pvs();

    let ss_count = subsectors.len();
    let ss_vis_pairs = pvs.count_visible_pairs();
    let ss_total = ss_count as u64 * ss_count as u64;

    let output = StatsOutput {
        map: args.map.clone(),
        sectors: sectors.len(),
        subsectors: ss_count,
        segments: segments.len(),
        linedefs: linedefs.len(),
        vertices: vertex_count,
        subsector_vis_pairs: ss_vis_pairs,
        subsector_total_pairs: ss_total,
        subsector_cull_pct: if ss_total > 0 {
            100.0 * (1.0 - ss_vis_pairs as f64 / ss_total as f64)
        } else {
            0.0
        },
    };
    println!("{}", serde_json::to_string_pretty(&output).unwrap());
}

fn cmd_portals(args: &Args) {
    use map_data::PvsView2D;

    let wad = load_wad(args);
    let mut map_data = MapData::default();
    map_data.load(&args.map, |_| Some(0), &wad, None);

    let MapData {
        subsectors,
        segments,
        bsp_3d,
        sectors,
        linedefs,
        nodes,
        start_node,
        ..
    } = &mut map_data;

    let pvs2d = PVS2D::build(
        subsectors,
        segments,
        bsp_3d,
        sectors,
        linedefs,
        nodes,
        *start_node,
    );
    let portals = pvs2d.portals_2d();

    #[derive(Serialize)]
    struct PortalInfo {
        ss_a: usize,
        ss_b: usize,
        v1: [f32; 2],
        v2: [f32; 2],
    }
    #[derive(Serialize)]
    struct PortalsOutput {
        subsectors: usize,
        portals: Vec<PortalInfo>,
    }
    let output = PortalsOutput {
        subsectors: portals.subsector_count(),
        portals: portals
            .iter()
            .map(|p| PortalInfo {
                ss_a: p.subsector_a,
                ss_b: p.subsector_b,
                v1: [p.v1.x, p.v1.y],
                v2: [p.v2.x, p.v2.y],
            })
            .collect(),
    };
    println!("{}", serde_json::to_string_pretty(&output).unwrap());
}

fn cmd_sectors(args: &Args) {
    let map_data = load_map(args);
    let sectors: Vec<SectorOutput> = map_data
        .sectors
        .iter()
        .enumerate()
        .map(|(i, s)| SectorOutput {
            id: i,
            floor_height: s.floorheight,
            ceiling_height: s.ceilingheight,
            light_level: s.lightlevel,
            tag: s.tag,
            kind: s.special,
        })
        .collect();
    println!("{}", serde_json::to_string_pretty(&sectors).unwrap());
}

fn cmd_build(args: &Args) {
    let wad = load_wad(args);
    let hash = wad.map_bsp_hash(&args.map).unwrap_or_default();

    let mut map_data = MapData::default();
    map_data.load(&args.map, |_| Some(0), &wad, None);

    let MapData {
        subsectors,
        segments,
        bsp_3d,
        sectors,
        linedefs,
        nodes,
        start_node,
        ..
    } = &mut map_data;

    let pvs2d = PVS2D::build(
        subsectors,
        segments,
        bsp_3d,
        sectors,
        linedefs,
        nodes,
        *start_node,
    );

    match pvs2d.save_to_cache(&args.map, hash) {
        Ok(()) => match RenderPvs::cache_path(&args.map, hash) {
            Ok(cache_path) => println!("PVS saved to {}", cache_path.display()),
            Err(_) => println!("PVS saved."),
        },
        Err(e) => {
            eprintln!("Failed to save PVS: {}", e);
            std::process::exit(1);
        }
    }
}

fn main() {
    let args: Args = argh::from_env();

    simplelog::TermLogger::init(
        log::LevelFilter::Info,
        simplelog::ConfigBuilder::default()
            .set_time_level(log::LevelFilter::Trace)
            .build(),
        simplelog::TerminalMode::Stderr,
        simplelog::ColorChoice::Auto,
    )
    .ok();

    match &args.command {
        Command::Stats(_) => cmd_stats(&args),
        Command::Portals(_) => cmd_portals(&args),
        Command::Sectors(_) => cmd_sectors(&args),
        Command::Build(_) => cmd_build(&args),
        Command::View(cmd) => cmd_view(&args, cmd),
        Command::Diag(_) => cmd_diag(&args),
        Command::Trace(cmd) => cmd_trace(&args, cmd),
        Command::Dump(cmd) => cmd_dump(&args, cmd),
    }
}

fn cmd_trace(args: &Args, cmd: &TraceCmd) {
    eprintln!(
        "trace: sector-level trace not implemented (sectors {} → {})",
        cmd.sector_a, cmd.sector_b
    );
    eprintln!("Use 'view' and the line-probe tool to inspect visibility interactively.");
    let _ = args;
}

fn cmd_diag(args: &Args) {
    use glam::Vec2;

    let map_data = load_map(args);
    let carved = &map_data.bsp_3d.carved_polygons;

    let extents = map_data.get_map_extents();
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
        let sector_id = map_data.subsectors[i].sector.num as usize;

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

        // Check for vertices at world bounds (clipping didn't close properly)
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

        // Check polygon area (shoelace formula) vs expected
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

        // Check convexity: all cross products should have same sign
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
                if cross < min_cross {
                    min_cross = cross;
                }
                if cross > max_cross {
                    max_cross = cross;
                }
            } else if cross < -1e-4 {
                neg += 1;
                if cross < min_cross {
                    min_cross = cross;
                }
                if cross > max_cross {
                    max_cross = cross;
                }
            }
        }
        if pos > 0 && neg > 0 {
            non_convex += 1;
            // Show the minority sign's value
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
    portals: Vec<DumpPortal>,
}

#[derive(Serialize)]
struct DumpPortal {
    ss_a: usize,
    ss_b: usize,
    v1: [f32; 2],
    v2: [f32; 2],
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
    let map_data = load_map(args);
    let vert_base = map_data.vertexes.as_ptr();

    let portals: Vec<DumpPortal> = {
        use map_data::Portals;
        Portals::build(
            map_data.start_node,
            &map_data.nodes,
            &map_data.subsectors,
            &map_data.segments,
            &map_data.bsp_3d,
        )
        .iter()
        .map(|p| DumpPortal {
            ss_a: p.subsector_a,
            ss_b: p.subsector_b,
            v1: [p.v1.x, p.v1.y],
            v2: [p.v2.x, p.v2.y],
        })
        .collect()
    };

    let carved = &map_data.bsp_3d.carved_polygons;

    let output = DumpOutput {
        map: args.map.clone(),
        vertices: map_data.vertexes.iter().map(|v| [v.x, v.y]).collect(),
        sectors: map_data
            .sectors
            .iter()
            .enumerate()
            .map(|(i, s)| DumpSector {
                id: i,
                floor_height: s.floorheight,
                ceiling_height: s.ceilingheight,
                light_level: s.lightlevel,
                tag: s.tag,
                special: s.special,
            })
            .collect(),
        subsectors: map_data
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
        linedefs: map_data
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
        segments: map_data
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
        nodes: map_data
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
        sector_subsectors: map_data.bsp_3d.sector_subsectors.clone(),
        carved_polygons: carved
            .iter()
            .map(|poly| poly.iter().map(|v| [v.x, v.y]).collect())
            .collect(),
        portals,
    };

    let json = serde_json::to_string_pretty(&output).unwrap();
    let output_path = cmd
        .output
        .clone()
        .unwrap_or_else(|| format!("{}.json", args.map));
    std::fs::write(&output_path, &json).expect("Failed to write output file");
    info!("Dumped {} to {}", args.map, output_path);
}

fn cmd_view(args: &Args, view_cmd: &ViewCmd) {
    let wad = load_wad(args);
    let map_hash = wad.map_bsp_hash(&args.map).unwrap_or_default();
    let map_data = load_map(args);

    let pvs: Option<PvsInput> = if view_cmd.no_pvs {
        None
    } else if view_cmd.cluster {
        info!("Building PvsCluster...");
        Some(PvsInput::Cluster(PvsCluster::build(
            &map_data.subsectors,
            &map_data.segments,
            &map_data.bsp_3d,
            &map_data.sectors,
            &map_data.linedefs,
            &map_data.nodes,
            map_data.start_node,
        )))
    } else {
        info!("Building PVS2D...");
        Some(PvsInput::Pvs2D(PVS2D::build(
            &map_data.subsectors,
            &map_data.segments,
            &map_data.bsp_3d,
            &map_data.sectors,
            &map_data.linedefs,
            &map_data.nodes,
            map_data.start_node,
        )))
    };

    let viewer_data = viewer::extract_viewer_data(
        &args.map,
        &map_data,
        pvs,
        map_hash,
        args.iwad.clone(),
        args.pwad.clone(),
    );
    drop(map_data);
    viewer::run(viewer_data);
}
