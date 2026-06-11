//! Golden direct-path harness.
//!
//! Renders a fixed E1M2 pose through the direct `PixelTarget` path (final pixels,
//! no index/resolve) and asserts the frame is deterministic, varied, and that the
//! u16 (RGB565) path equals the u32 (ARGB) path 565-quantized per pixel. Plus the
//! data-layer invariant that `PalLit` reproduces the active palette.
//!
//! Uses the bundled `data/doom1.wad`; skips cleanly if absent.

use std::collections::HashSet;

use level::LevelData;
use math::{Angle, Bam, FixedT};
use pic_data::{ByteOrder, PalLit, PicData, PixelFmt};
use render_common::{BufferSize, PixelTarget, RenderPspDef, RenderView};
use software3d::Software3D;
use wad::WadData;

const VIEWHEIGHT: f32 = 41.0;
const FOV: f32 = std::f32::consts::FRAC_PI_2;

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

fn load(map: &str) -> Option<(LevelData, PicData)> {
    let path = test_utils::doom1_wad_path();
    if !path.exists() {
        eprintln!("skip golden_cross_path: {} not found", path.display());
        return None;
    }
    let wad = WadData::new(&path);
    let pics = PicData::init(&wad, &["TROO"]);
    let mut level = LevelData::default();
    level.load(map, |n| pics.flat_num_for_name(n), &wad, None, None);
    Some((level, pics))
}

/// Render the fixed pose via the direct `PixelTarget` path (final `T` pixels, no
/// resolve). Tight pitch, so the returned buffer is directly comparable.
fn render_direct<T: PixelFmt + Copy>(w: usize, h: usize) -> Option<Vec<T>> {
    let (mut level, mut pics) = load("E1M2")?;
    let mut r = Software3D::new(w as f32, h as f32, FOV);
    let view = build_view(&mut level);
    let pal_lit: PalLit<T> = pics.build_pal_lit(ByteOrder::Argb);
    let void = pal_lit.block(pics.use_palette())[0];
    let mut surface = vec![void; w * h];
    {
        let mut target = PixelTarget::new(
            &mut surface,
            BufferSize::new(w, h),
            w,
            &pal_lit,
            pics.use_palette(),
        );
        r.draw_view(&view, &level, &mut pics, &mut target);
    }
    Some(surface)
}

/// The direct u32 path produces a varied, deterministic frame across renders.
#[test]
fn direct_u32_deterministic_and_varied() {
    let (w, h) = (320, 200);
    let Some(a) = render_direct::<u32>(w, h) else {
        return;
    };
    let distinct = a.iter().collect::<HashSet<_>>().len();
    assert!(
        distinct > 50,
        "expected a varied frame, got {distinct} colours"
    );
    let b = render_direct::<u32>(w, h).expect("second render");
    assert_eq!(a, b, "direct path must be deterministic across renders");
}

/// The direct u16 (RGB565) path equals the direct u32 (ARGB) path 565-quantized
/// per pixel — same E1M2 pose, same palette, just a narrower surface format.
#[test]
fn direct_u16_equals_u32_quantized() {
    let (w, h) = (320, 200);
    let Some(u32_frame) = render_direct::<u32>(w, h) else {
        return;
    };
    let u16_frame = render_direct::<u16>(w, h).expect("direct u16 render");
    assert_eq!(u32_frame.len(), u16_frame.len());
    let mismatches = u32_frame
        .iter()
        .zip(u16_frame.iter())
        .filter(|(argb, got)| u16::from_argb(**argb, ByteOrder::Argb) != **got)
        .count();
    assert_eq!(
        mismatches,
        0,
        "direct u16 must equal 565-quantized u32 ({mismatches}/{} differ)",
        u32_frame.len()
    );
}

/// Data-layer invariant: `PalLit<u32>` (ARGB) applied to a lit index reproduces
/// the active palette exactly. This is what the direct store relies on per pixel.
#[test]
fn pal_lit_u32_matches_palette() {
    let Some((_, pics)) = load("E1M2") else {
        return;
    };
    let pal_lit: PalLit<u32> = pics.build_pal_lit(ByteOrder::Argb);
    let palette = pics.palette();
    let block = pal_lit.block(pics.use_palette());
    for i in 0..256 {
        assert_eq!(
            block[i], palette[i],
            "pal_lit[{i}] must equal palette[{i}] for tint 0"
        );
    }
}
