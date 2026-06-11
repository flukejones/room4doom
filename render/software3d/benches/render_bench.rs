//! software3d render microbenchmarks.
//!
//! Spawns a camera at the player-1 start at eye height and renders the same
//! frame repeatedly — isolating the rasterizer. Two scenes (doom1 E1M2;
//! doom + sigil2 E6M6) × two resolutions (320×200, 1280×800), no voxels. The
//! scene writes final pixels straight into the `PixelTarget` surface (no index
//! plane, no resolve).

use std::path::Path;

use criterion::{Criterion, criterion_group, criterion_main};
use level::LevelData;
use math::{Angle, Bam, FixedT};
use pic_data::{ByteOrder, PalLit, PicData};
use render_common::{BufferSize, PixelTarget, RenderPspDef, RenderView};
use software3d::Software3D;
use wad::WadData;

const FOV: f32 = std::f32::consts::FRAC_PI_2;
const VIEWHEIGHT: f32 = 41.0;
const LOW: (usize, usize) = (320, 200);
const HI: (usize, usize) = (1280, 800);

/// Build a fixed-pose RenderView at the player-1 start, eye height above floor.
fn build_view(level: &mut LevelData) -> RenderView {
    let start = level.things().iter().find(|t| t.kind == 1).copied();
    let (x, y, angle) = match start {
        Some(t) => (t.x as f32, t.y as f32, t.angle as f32),
        None => (0.0, 0.0, 0.0),
    };
    let floor = level
        .point_in_subsector(FixedT::from_f32(x), FixedT::from_f32(y))
        .sector
        .floorheight
        .to_f32();
    let eye = floor + VIEWHEIGHT;
    let fp = FixedT::from_f32;
    RenderView {
        x: fp(x),
        y: fp(y),
        z: fp(eye),
        viewz: fp(eye),
        viewheight: fp(0.0),
        angle: Angle::<Bam>::new(angle.to_radians()),
        lookdir: 0.0,
        fixedcolormap: 0,
        extralight: 0,
        is_shadow: false,
        psprites: [RenderPspDef::default(); 2],
        sector_lightlevel: 0,
        player_mobj_id: 0,
        frac: 1.0,
        frac_fp: fp(1.0),
        game_tic: 0,
    }
}

/// Load a level + PicData from a single IWAD. `None` (with a skip message) if
/// the WAD is absent — benches cannot skip cleanly otherwise.
fn load_iwad(wad_path: &Path, map: &str) -> Option<(LevelData, PicData)> {
    if !wad_path.exists() {
        eprintln!("skip: {} not found", wad_path.display());
        return None;
    }
    Some(load_from(WadData::new(wad_path), map))
}

/// Load a level + PicData from an IWAD patched with a PWAD.
fn load_pwad(iwad: &Path, pwad: &Path, map: &str) -> Option<(LevelData, PicData)> {
    if !iwad.exists() || !pwad.exists() {
        eprintln!("skip: {} or {} not found", iwad.display(), pwad.display());
        return None;
    }
    let mut wad = WadData::new(iwad);
    wad.add_file(pwad.into());
    Some(load_from(wad, map))
}

fn load_from(wad: WadData, map: &str) -> (LevelData, PicData) {
    let pics = PicData::init(&wad, &["TROO"]);
    let mut level = LevelData::default();
    level.load(map, |n| pics.flat_num_for_name(n), &wad, None, None);
    (level, pics)
}

/// True if the rendered surface drew anything (RGB non-zero somewhere).
fn any_drawn(surface: &[u32]) -> bool {
    surface.iter().any(|&p| p & 0x00FF_FFFF != 0)
}

/// Render `map` repeatedly at `(w, h)` into a u32 `PixelTarget` under `name`.
fn bench_scene(
    c: &mut Criterion,
    name: &str,
    level: &mut LevelData,
    pics: &mut PicData,
    (w, h): (usize, usize),
) {
    let mut renderer = Software3D::new(w as f32, h as f32, FOV);
    let view = build_view(level);
    let tint = pics.use_palette();
    let pal: PalLit<u32> = pics.build_pal_lit(ByteOrder::Argb);
    let size = BufferSize::new(w, h);
    let mut surface = vec![0u32; w * h];

    // Sanity: a broken setup renders a blank frame.
    {
        let mut t = PixelTarget::new(&mut surface, size, w, &pal, tint);
        renderer.draw_view(&view, level, pics, &mut t);
    }
    assert!(any_drawn(&surface), "{name}: rendered a blank frame");

    c.bench_function(name, |b| {
        b.iter(|| {
            let mut t = PixelTarget::new(&mut surface, size, w, &pal, tint);
            renderer.draw_view(&view, level, pics, &mut t);
        });
    });
}

/// Compare the full scene-draw cost of the two surface formats at one size: the
/// scene writes final pixels straight into the output surface (no resolve).
fn bench_pixel_modes(
    c: &mut Criterion,
    prefix: &str,
    level: &mut LevelData,
    pics: &mut PicData,
    (w, h): (usize, usize),
) {
    let mut renderer = Software3D::new(w as f32, h as f32, FOV);
    let view = build_view(level);
    let tint = pics.use_palette();
    let pal32: PalLit<u32> = pics.build_pal_lit(ByteOrder::Argb);
    let pal16: PalLit<u16> = pics.build_pal_lit(ByteOrder::Argb);
    let size = BufferSize::new(w, h);

    let mut out32 = vec![0u32; w * h];
    c.bench_function(&format!("{prefix}/rgb888"), |b| {
        b.iter(|| {
            let mut t = PixelTarget::new(&mut out32, size, w, &pal32, tint);
            renderer.draw_view(&view, level, pics, &mut t);
        });
    });

    let mut out16 = vec![0u16; w * h];
    c.bench_function(&format!("{prefix}/rgb565"), |b| {
        b.iter(|| {
            let mut t = PixelTarget::new(&mut out16, size, w, &pal16, tint);
            renderer.draw_view(&view, level, pics, &mut t);
        });
    });
}

fn benches(c: &mut Criterion) {
    if let Some((mut level, mut pics)) = load_iwad(&test_utils::doom1_wad_path(), "E1M2") {
        bench_scene(c, "sw3d/e1m2/320x200", &mut level, &mut pics, LOW);
        bench_scene(c, "sw3d/e1m2/1280x800", &mut level, &mut pics, HI);
        bench_pixel_modes(c, "sw3d/pixmode/e1m2/320x200", &mut level, &mut pics, LOW);
        bench_pixel_modes(c, "sw3d/pixmode/e1m2/1280x800", &mut level, &mut pics, HI);
    }
    if let Some((mut level, mut pics)) = load_pwad(
        &test_utils::doom_wad_path(),
        &test_utils::sigil2_wad_path(),
        "E6M6",
    ) {
        bench_scene(c, "sw3d/e6m6/320x200", &mut level, &mut pics, LOW);
        bench_scene(c, "sw3d/e6m6/1280x800", &mut level, &mut pics, HI);
    }
}

criterion_group!(render_benches, benches);
criterion_main!(render_benches);
