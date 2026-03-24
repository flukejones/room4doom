use std::fs;
use std::path::Path;

use argh::FromArgs;
use serde_json::{Value, json};
use wad::types::{
    WadLineDef, WadNode, WadSector, WadSegment, WadSideDef, WadSubSector, WadThing, WadVertex
};
use wad::{MapLump, WadData};

/// WAD file inspection and extraction tool
#[derive(FromArgs)]
struct Args {
    /// path to WAD file
    #[argh(option, short = 'w')]
    wad: String,
    #[argh(subcommand)]
    command: Command,
}

#[derive(FromArgs)]
#[argh(subcommand)]
enum Command {
    List(ListCmd),
    Show(ShowCmd),
    Extract(ExtractCmd),
    Info(InfoCmd),
    Dump(DumpCmd),
}

/// list all lumps in the WAD
#[derive(FromArgs)]
#[argh(subcommand, name = "list")]
struct ListCmd {
    /// filter lumps by name (case-insensitive substring match)
    #[argh(option, short = 'f')]
    filter: Option<String>,
}

/// show lump contents (parsed text for known types, hex dump for binary)
#[derive(FromArgs)]
#[argh(subcommand, name = "show")]
struct ShowCmd {
    /// lump name to display
    #[argh(positional)]
    name: String,
    /// output as hex dump instead of parsed text
    #[argh(switch)]
    hex: bool,
}

/// extract a lump to a file
#[derive(FromArgs)]
#[argh(subcommand, name = "extract")]
struct ExtractCmd {
    /// lump name to extract
    #[argh(positional)]
    name: String,
    /// output file path (defaults to <name>.lmp)
    #[argh(option, short = 'o')]
    output: Option<String>,
}

/// show WAD metadata and detected formats
#[derive(FromArgs)]
#[argh(subcommand, name = "info")]
struct InfoCmd {}

/// dump all map data as JSON (vertexes, linedefs, sidedefs, sectors, things,
/// segs, subsectors, nodes)
#[derive(FromArgs)]
#[argh(subcommand, name = "dump")]
struct DumpCmd {
    /// map name (e.g. E1M1, MAP01)
    #[argh(positional)]
    map: String,
    /// output file (defaults to stdout)
    #[argh(option, short = 'o')]
    output: Option<String>,
}

fn main() {
    let args: Args = argh::from_env();
    let wad = WadData::new(Path::new(&args.wad));

    match args.command {
        Command::List(cmd) => cmd_list(&wad, &cmd),
        Command::Show(cmd) => cmd_show(&wad, &cmd),
        Command::Extract(cmd) => cmd_extract(&wad, &cmd),
        Command::Info(_) => cmd_info(&wad, &args.wad),
        Command::Dump(cmd) => cmd_dump(&wad, &cmd),
    }
}

fn cmd_list(wad: &WadData, cmd: &ListCmd) {
    let filter = cmd.filter.as_deref().unwrap_or("").to_ascii_uppercase();
    println!("{:<4}  {:<8}  {:>8}  {}", "#", "NAME", "SIZE", "TYPE");
    println!("{}", "-".repeat(40));
    for (i, lump) in wad.lumps().iter().enumerate() {
        if !filter.is_empty() && !lump.name.contains(&filter) {
            continue;
        }
        let kind = classify_lump(&lump.name, &lump.data);
        println!(
            "{:<4}  {:<8}  {:>8}  {}",
            i,
            lump.name,
            lump.data.len(),
            kind
        );
    }
}

fn cmd_show(wad: &WadData, cmd: &ShowCmd) {
    let name = cmd.name.to_ascii_uppercase();
    let Some(lump) = wad.get_lump(&name) else {
        eprintln!("Lump '{}' not found", name);
        std::process::exit(1);
    };

    if cmd.hex || !try_show_parsed(&name, &lump.data) {
        hex_dump(&lump.data);
    }
}

fn cmd_extract(wad: &WadData, cmd: &ExtractCmd) {
    let name = cmd.name.to_ascii_uppercase();
    let Some(lump) = wad.get_lump(&name) else {
        eprintln!("Lump '{}' not found", name);
        std::process::exit(1);
    };

    let output = cmd
        .output
        .clone()
        .unwrap_or_else(|| format!("{}.lmp", name));
    fs::write(&output, &lump.data).expect("Failed to write file");
    println!(
        "Extracted '{}' ({} bytes) -> {}",
        name,
        lump.data.len(),
        output
    );
}

fn cmd_info(wad: &WadData, path: &str) {
    println!("WAD: {}", path);
    let wad_type = if wad.lumps().first().map_or(false, |l| {
        l.name.starts_with("MAP") || l.name.starts_with("E")
    }) {
        "PWAD"
    } else {
        "IWAD"
    };
    println!("Type: {}", wad_type);
    println!("Lumps: {}", wad.lumps().len());

    // Detect map entries
    let mut maps = Vec::new();
    for lump in wad.lumps() {
        if is_map_marker(&lump.name) {
            maps.push(lump.name.clone());
        }
    }
    if !maps.is_empty() {
        println!("Maps: {}", maps.join(", "));
    }

    // Detect node format
    let node_types: Vec<&str> = wad
        .lumps()
        .iter()
        .filter(|l| l.name == "NODES" && l.data.len() >= 4)
        .map(|l| match &l.data[..4] {
            b"XNOD" => "XNOD",
            b"ZNOD" => "ZNOD",
            b"XGLN" => "XGLN",
            b"XGL2" => "XGL2",
            b"ZGLN" => "ZGLN",
            b"ZGL2" => "ZGL2",
            _ => "vanilla",
        })
        .collect();
    if !node_types.is_empty() {
        let unique: Vec<&str> = {
            let mut v = node_types;
            v.dedup();
            v
        };
        println!("Node format: {}", unique.join(", "));
    }

    // Detect special lumps
    let specials = [
        "UMAPINFO", "MAPINFO", "SWITCHES", "ANIMATED", "TRANMAP", "DEHACKED", "TEXTMAP", "ENDMAP",
    ];
    let found: Vec<&str> = specials
        .iter()
        .filter(|&&s| wad.lump_exists(s))
        .copied()
        .collect();
    if !found.is_empty() {
        println!("Special lumps: {}", found.join(", "));
    }
}

fn classify_lump(name: &str, data: &[u8]) -> &'static str {
    match name {
        "THINGS" | "LINEDEFS" | "SIDEDEFS" | "VERTEXES" | "SEGS" | "SSECTORS" | "NODES"
        | "SECTORS" | "REJECT" | "BLOCKMAP" => "map data",
        "PLAYPAL" => "palette",
        "COLORMAP" => "colourmap",
        "ENDOOM" => "text screen",
        "TEXTURE1" | "TEXTURE2" | "PNAMES" => "texture def",
        "SWITCHES" => "switch list",
        "ANIMATED" => "animation list",
        "TRANMAP" => "translucency map",
        "UMAPINFO" => "umapinfo",
        "MAPINFO" => "mapinfo",
        "DEHACKED" => "dehacked",
        "TEXTMAP" => "udmf map",
        "ENDMAP" => "udmf end",
        "GENMIDI" | "DMXGUS" | "DMXGUSC" => "sound config",
        _ if data.is_empty() => "marker",
        _ if name.starts_with("D_") || name.starts_with("DS") || name.starts_with("DP") => {
            "sound/music"
        }
        _ if name.starts_with("DEMO") => "demo",
        _ if is_map_marker(name) => "map marker",
        _ => "data",
    }
}

fn is_map_marker(name: &str) -> bool {
    if name.starts_with("MAP") && name.len() >= 5 {
        return name[3..].chars().all(|c| c.is_ascii_digit());
    }
    if name.starts_with('E') && name.len() >= 4 {
        let chars: Vec<char> = name.chars().collect();
        return chars[1].is_ascii_digit()
            && chars[2] == 'M'
            && chars[3..].iter().all(|c| c.is_ascii_digit());
    }
    false
}

fn try_show_parsed(name: &str, data: &[u8]) -> bool {
    match name {
        "UMAPINFO" | "MAPINFO" => {
            if let Ok(text) = std::str::from_utf8(data) {
                println!("{}", text);
                return true;
            }
        }
        "SWITCHES" => {
            let entries = wad::boom::parse_switches(data);
            for (i, e) in entries.iter().enumerate() {
                println!(
                    "{:3}: {} <-> {} (episode {})",
                    i, e.name1, e.name2, e.episode
                );
            }
            println!("({} entries)", entries.len());
            return true;
        }
        "ANIMATED" => {
            let entries = wad::boom::parse_animated(data);
            for (i, e) in entries.iter().enumerate() {
                let kind = if e.is_texture { "tex" } else { "flat" };
                println!(
                    "{:3}: {} -> {} ({}, speed {})",
                    i, e.start_name, e.end_name, kind, e.speed
                );
            }
            println!("({} entries)", entries.len());
            return true;
        }
        "TRANMAP" => {
            println!("Translucency map: {} bytes (expected 65536)", data.len());
            if data.len() == 65536 {
                println!(
                    "Identity check: src[0][0]={}, src[255][255]={}",
                    data[0],
                    data[255 * 256 + 255]
                );
            }
            return true;
        }
        "ENDOOM" => {
            println!("Text screen: {} bytes", data.len());
            return true;
        }
        _ => {}
    }
    false
}

fn cmd_dump(wad: &WadData, cmd: &DumpCmd) {
    let map = cmd.map.to_ascii_uppercase();

    let vertexes: Vec<Value> = wad
        .map_iter::<WadVertex>(&map, MapLump::Vertexes)
        .map(|v| json!({"x": v.x, "y": v.y}))
        .collect();

    let linedefs: Vec<Value> = wad
        .map_iter::<WadLineDef>(&map, MapLump::LineDefs)
        .map(|l| {
            json!({
                "start_vertex": l.start_vertex,
                "end_vertex": l.end_vertex,
                "flags": l.flags,
                "special": l.special,
                "sector_tag": l.sector_tag,
                "front_sidedef": l.front_sidedef,
                "back_sidedef": l.back_sidedef,
            })
        })
        .collect();

    let sidedefs: Vec<Value> = wad
        .map_iter::<WadSideDef>(&map, MapLump::SideDefs)
        .map(|s| {
            json!({
                "x_offset": s.x_offset,
                "y_offset": s.y_offset,
                "upper_tex": s.upper_tex,
                "lower_tex": s.lower_tex,
                "middle_tex": s.middle_tex,
                "sector": s.sector,
            })
        })
        .collect();

    let sectors: Vec<Value> = wad
        .map_iter::<WadSector>(&map, MapLump::Sectors)
        .map(|s| {
            json!({
                "floor_height": s.floor_height,
                "ceil_height": s.ceil_height,
                "floor_tex": s.floor_tex,
                "ceil_tex": s.ceil_tex,
                "light_level": s.light_level,
                "type": s.kind,
                "tag": s.tag,
            })
        })
        .collect();

    let things: Vec<Value> = wad
        .map_iter::<WadThing>(&map, MapLump::Things)
        .map(|t| {
            json!({
                "x": t.x,
                "y": t.y,
                "angle": t.angle,
                "type": t.kind,
                "flags": t.flags,
            })
        })
        .collect();

    let segments: Vec<Value> = wad
        .map_iter::<WadSegment>(&map, MapLump::Segs)
        .map(|s| {
            json!({
                "start_vertex": s.start_vertex,
                "end_vertex": s.end_vertex,
                "angle": s.angle,
                "linedef": s.linedef,
                "side": s.side,
                "offset": s.offset,
            })
        })
        .collect();

    let subsectors: Vec<Value> = wad
        .map_iter::<WadSubSector>(&map, MapLump::SubSectors)
        .map(|s| {
            json!({
                "seg_count": s.seg_count,
                "start_seg": s.start_seg,
            })
        })
        .collect();

    let nodes: Vec<Value> = wad
        .map_iter::<WadNode>(&map, MapLump::Nodes)
        .map(|n| {
            json!({
                "x": n.x, "y": n.y, "dx": n.dx, "dy": n.dy,
                "bbox_right": n.bboxes[0],
                "bbox_left": n.bboxes[1],
                "child_right": n.children[0],
                "child_left": n.children[1],
            })
        })
        .collect();

    let result = json!({
        "map": map,
        "vertexes": vertexes,
        "linedefs": linedefs,
        "sidedefs": sidedefs,
        "sectors": sectors,
        "things": things,
        "segments": segments,
        "subsectors": subsectors,
        "nodes": nodes,
    });

    let output = serde_json::to_string_pretty(&result).expect("JSON serialization failed");

    if let Some(path) = &cmd.output {
        fs::write(path, &output).expect("Failed to write file");
        eprintln!(
            "Dumped {} ({} verts, {} lines, {} sides, {} sectors, {} things, {} segs, {} ssectors, {} nodes)",
            map,
            vertexes.len(),
            linedefs.len(),
            sidedefs.len(),
            sectors.len(),
            things.len(),
            segments.len(),
            subsectors.len(),
            nodes.len()
        );
    } else {
        println!("{}", output);
    }
}

fn hex_dump(data: &[u8]) {
    for (i, chunk) in data.chunks(16).enumerate() {
        print!("{:08x}  ", i * 16);
        for (j, byte) in chunk.iter().enumerate() {
            print!("{:02x} ", byte);
            if j == 7 {
                print!(" ");
            }
        }
        for _ in chunk.len()..16 {
            print!("   ");
            if chunk.len() <= 8 {
                print!(" ");
            }
        }
        print!(" |");
        for byte in chunk {
            let c = if byte.is_ascii_graphic() || *byte == b' ' {
                *byte as char
            } else {
                '.'
            };
            print!("{}", c);
        }
        println!("|");
    }
}
