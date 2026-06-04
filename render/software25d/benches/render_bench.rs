//! software25d render microbenchmarks.
//!
//! Mirrors the software3d bench: a camera at the player-1 start at eye height
//! renders the same frame repeatedly. Two scenes (doom1 E1M2; doom + sigil2
//! E6M6) × two resolutions (320×200, 1280×800). software25d has no voxels.

use criterion::{Criterion, criterion_group, criterion_main};
use math::{Angle, Bam, FixedT};
use pic_data::PicData;
use render_common::{BufferSize, DrawBuffer, RenderPspDef, RenderView};
use software25d::Software25D;
use std::path::Path;
use wad::WadData;

use level::LevelData;

const FOV: f32 = std::f32::consts::FRAC_PI_2;
const VIEWHEIGHT: f32 = 41.0;
/// (width, height, hi_res). hi_res LUT is required above the 200p base.
const LOW: (usize, usize, bool) = (320, 200, false);
const HI: (usize, usize, bool) = (1280, 800, true);

/// Headless framebuffer; indexes by its own width so any resolution works.
struct HeadlessBuffer {
    size: BufferSize,
    data: Vec<u32>,
    w: usize,
}

impl HeadlessBuffer {
    fn new(w: usize, h: usize) -> Self {
        Self {
            size: BufferSize::new(w, h),
            data: vec![0u32; w * h],
            w,
        }
    }
    fn any_drawn(&self) -> bool {
        self.data.iter().any(|&p| p & 0x00FF_FFFF != 0)
    }
}

impl DrawBuffer for HeadlessBuffer {
    fn size(&self) -> &BufferSize {
        &self.size
    }
    fn set_pixel(&mut self, x: usize, y: usize, colour: u32) {
        self.data[y * self.w + x] = colour;
    }
    fn read_pixel(&self, x: usize, y: usize) -> u32 {
        self.data[y * self.w + x]
    }
    fn get_buf_index(&self, x: usize, y: usize) -> usize {
        y * self.w + x
    }
    fn pitch(&self) -> usize {
        self.w
    }
    fn buf_mut(&mut self) -> &mut [u32] {
        &mut self.data
    }
    fn debug_flip_and_present(&mut self) {}
}

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
        subsector_id: 0,
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

/// Render `map` repeatedly at `(w, h, hi_res)` under the given bench name.
fn bench_scene(
    c: &mut Criterion,
    name: &str,
    level: &mut LevelData,
    pics: &mut PicData,
    (w, h, hi_res): (usize, usize, bool),
) {
    let mut renderer = Software25D::new(FOV, w as f32, h as f32, hi_res, false);
    let view = build_view(level);
    let mut buffer = HeadlessBuffer::new(w, h);

    // Sanity: a broken setup renders a blank frame.
    renderer.draw_view(&view, level, pics, &mut buffer);
    assert!(buffer.any_drawn(), "{name}: rendered a blank frame");

    c.bench_function(name, |b| {
        b.iter(|| renderer.draw_view(&view, level, pics, &mut buffer));
    });
}

fn benches(c: &mut Criterion) {
    if let Some((mut level, mut pics)) = load_iwad(&test_utils::doom1_wad_path(), "E1M2") {
        bench_scene(c, "sw25d/e1m2/320x200", &mut level, &mut pics, LOW);
        bench_scene(c, "sw25d/e1m2/1280x800", &mut level, &mut pics, HI);
    }
    if let Some((mut level, mut pics)) =
        load_pwad(&test_utils::doom_wad_path(), &test_utils::sigil2_wad_path(), "E6M6")
    {
        bench_scene(c, "sw25d/e6m6/320x200", &mut level, &mut pics, LOW);
        bench_scene(c, "sw25d/e6m6/1280x800", &mut level, &mut pics, HI);
    }
}

criterion_group!(render_benches, benches);
criterion_main!(render_benches);
