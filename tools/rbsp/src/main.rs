use argh::FromArgs;
use simplelog::{ColorChoice, ConfigBuilder, LevelFilter, TermLogger, TerminalMode};
use std::path::Path;
use std::process;

#[derive(FromArgs)]
/// rbsp — BSP node builder for Doom maps
struct Args {
    /// input WAD file
    #[argh(positional)]
    input: String,

    /// output WAD path (default: <input>.rbsp.wad)
    #[argh(option, short = 'o')]
    output: Option<String>,

    /// split cost weight (default 11)
    #[argh(option, default = "11.0")]
    split_weight: f32,
}

fn main() {
    let args: Args = argh::from_env();

    let config = ConfigBuilder::new()
        .set_time_level(LevelFilter::Off)
        .set_target_level(LevelFilter::Off)
        .build();
    TermLogger::init(
        LevelFilter::Info,
        config,
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )
    .ok();

    let output_path = args
        .output
        .unwrap_or_else(|| format!("{}.rbsp.wad", args.input.trim_end_matches(".wad")));

    let options = rbsp::BspOptions {
        split_weight: args.split_weight as rbsp::Float,
    };

    if let Err(e) =
        rbsp::wad_io::process_wad(Path::new(&args.input), Path::new(&output_path), &options)
    {
        eprintln!("Error: {}", e);
        process::exit(1);
    }

    eprintln!("Wrote {}", output_path);
}
