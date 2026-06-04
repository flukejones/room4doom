//! Headless direct-path golden for software25d (mirrors software3d's).
//! Renders a fixed E1M2 pose through the direct `PixelTarget` path (final pixels,
//! no index/resolve) and asserts a deterministic, varied frame, plus the u16
//! (565) path equals the u32 (ARGB) path 565-quantized per pixel.
//! Uses the bundled `data/doom1.wad`; skips if absent.

use std::collections::HashSet;

use level::LevelData;
use math::{Angle, Bam, FixedT};
use pic_data::{ByteOrder, PalLit, PicData, PixelFmt};
use render_common::{BufferSize, PixelTarget, RenderPspDef, RenderView};
use software25d::Software25D;
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
        subsector_id: 0,
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
        eprintln!("skip sw25d golden: {} not found", path.display());
        return None;
    }
    let wad = WadData::new(&path);
    let pics = PicData::init(&wad, &["TROO"]);
    let mut level = LevelData::default();
    level.load(map, |n| pics.flat_num_for_name(n), &wad, None, None);
    Some((level, pics))
}

/// Render the fixed pose via the direct `PixelTarget` path (final `T` pixels).
fn render_direct<T: PixelFmt + Copy>(w: usize, h: usize) -> Option<Vec<T>> {
    let (mut level, mut pics) = load("E1M2")?;
    let mut r = Software25D::new(FOV, w as f32, h as f32, false);
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

#[test]
fn direct_u32_deterministic_and_varied() {
    let (w, h) = (320, 200);
    let Some(a) = render_direct::<u32>(w, h) else {
        return;
    };
    let distinct = a.iter().collect::<HashSet<_>>().len();
    assert!(distinct > 50, "expected varied frame, got {distinct}");
    let b = render_direct::<u32>(w, h).expect("second render");
    assert_eq!(a, b, "direct path must be deterministic");
}

#[test]
fn direct_u16_equals_u32_quantized() {
    let (w, h) = (320, 200);
    let Some(u32_frame) = render_direct::<u32>(w, h) else {
        return;
    };
    let u16_frame = render_direct::<u16>(w, h).expect("direct u16 render");
    let mismatches = u32_frame
        .iter()
        .zip(u16_frame.iter())
        .filter(|(argb, got)| u16::from_argb(**argb, ByteOrder::Argb) != **got)
        .count();
    assert_eq!(
        mismatches, 0,
        "direct u16 must equal 565-quantized u32 ({mismatches} differ)"
    );
}
