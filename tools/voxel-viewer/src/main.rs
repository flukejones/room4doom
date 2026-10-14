//! Voxel inspection and profiling using the engine's software renderer.
mod renderer;
mod ui;

use pic_data::voxel::kvx::VoxelModel;
use pic_data::voxel::slices::{self, VoxelColumn};
use pic_data::{ByteOrder, PALETTE_LEN, PalLit, WadPalette};

#[expect(
    clippy::cast_possible_truncation,
    reason = "map and viewport coordinates deliberately use float precision; GPU and image records retain their packed widths"
)]
fn main() {
    simplelog::TermLogger::init(
        simplelog::LevelFilter::Info,
        simplelog::Config::default(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    )
    .expect("logger initializes once");

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        log::error!("Usage: voxel-viewer <kvx-file> [--slices] [--no-vsync] [palette.lmp]");
        std::process::exit(1);
    }

    let use_slices = args.iter().any(|a| a == "--slices");
    let no_vsync = args.iter().any(|a| a == "--no-vsync");
    let kvx_data = std::fs::read(&args[1]).unwrap_or_else(|e| {
        log::error!("Failed to read {}: {}", args[1], e);
        std::process::exit(1);
    });
    let mut model = VoxelModel::load(&kvx_data).unwrap_or_else(|e| {
        log::error!("Failed to parse KVX: {e}");
        std::process::exit(1);
    });

    // Resolve palette: external Doom palette remaps grid indices,
    // KVX embedded palette scaled 6-bit→8-bit, else procedural fallback.
    let ext_pal = args
        .iter()
        .find(|a| a.ends_with(".lmp") || a.ends_with(".pal"))
        .and_then(|p| std::fs::read(p).ok());

    let mut palette = [(128u8, 128u8, 128u8); 256];
    if let Some(data) = &ext_pal {
        if data.len() >= 768 {
            model.remap_to_doom_palette(data);
            for i in 0..256 {
                palette[i] = (data[i * 3], data[i * 3 + 1], data[i * 3 + 2]);
            }
            log::info!("Using external Doom palette (remapped KVX indices)");
        }
    } else if let Some(kvx_pal) = &model.palette {
        for i in 0..256.min(kvx_pal.len() / 3) {
            let (r, g, b) = (kvx_pal[i * 3], kvx_pal[i * 3 + 1], kvx_pal[i * 3 + 2]);
            palette[i] = (
                (r << 2) | (r >> 4),
                (g << 2) | (g >> 4),
                (b << 2) | (b >> 4),
            );
        }
        log::info!("Using KVX embedded palette (6-bit scaled to 8-bit)");
    } else {
        for (i, palette) in palette.iter_mut().enumerate() {
            *palette = (
                ((i * 37 + 7) % 200 + 55) as u8,
                ((i * 53 + 13) % 200 + 55) as u8,
                ((i * 97 + 29) % 200 + 55) as u8,
            );
        }
        palette[0] = (0, 0, 0);
        log::warn!("No palette found, using procedural fallback");
    }

    let occupied = model.grid.iter().filter(|&&v| v != 255).count();
    log::info!(
        "Loaded {}x{}x{}, {} occupied voxels, mode={}",
        model.xsiz,
        model.ysiz,
        model.zsiz,
        occupied,
        if use_slices { "slices" } else { "direct" }
    );

    let slices = slices::generate(&model);
    for (i, axis) in slices.slices.iter().enumerate() {
        let count = |cols: &[VoxelColumn]| -> usize {
            cols.iter()
                .flat_map(|c| c.spans.iter())
                .map(|s| s.pixels.len())
                .sum()
        };
        let neg: usize = axis.iter().map(|q| count(&q.neg_columns)).sum();
        let pos: usize = axis.iter().map(|q| count(&q.pos_columns)).sum();
        log::info!(
            "  {}: {} quads, neg={} px, pos={} px",
            ["X", "Y", "Z"][i],
            axis.len(),
            neg,
            pos
        );
    }

    // Build u32 palette (0xAARRGGBB) and fold it into a single-tint PalLit for
    // the PixelTarget store path.
    let mut palette_u32 = [0u32; 256];
    for i in 0..256 {
        let (r, g, b) = palette[i];
        palette_u32[i] = 0xFF000000 | ((r as u32) << 16) | ((g as u32) << 8) | b as u32;
    }
    let pal_lit: PalLit<u32> =
        PalLit::new(&[WadPalette(palette_u32); PALETTE_LEN], ByteOrder::Argb);

    ui::run(
        renderer::Scene::new(model, slices, &palette, pal_lit, use_slices),
        no_vsync,
    );
}
