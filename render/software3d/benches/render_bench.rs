use criterion::{Criterion, criterion_group, criterion_main};
use glam::Vec3;
use render_common::{BufferSize, DrawBuffer};
use software3d::{DebugDrawOptions, Software3D};
use wad::WadData;

use level::LevelData;
use pic_data::PicData;

const WIDTH: usize = 640;
const HEIGHT: usize = 480;

struct HeadlessBuffer {
    size: BufferSize,
    data: Vec<u32>,
}

impl HeadlessBuffer {
    fn new(width: usize, height: usize) -> Self {
        Self {
            size: BufferSize::new(width, height),
            data: vec![0u32; width * height],
        }
    }
}

impl DrawBuffer for HeadlessBuffer {
    fn size(&self) -> &BufferSize {
        &self.size
    }
    fn set_pixel(&mut self, x: usize, y: usize, colour: u32) {
        self.data[y * WIDTH + x] = colour;
    }
    fn read_pixel(&self, x: usize, y: usize) -> u32 {
        self.data[y * WIDTH + x]
    }
    fn get_buf_index(&self, x: usize, y: usize) -> usize {
        y * WIDTH + x
    }
    fn pitch(&self) -> usize {
        WIDTH
    }
    fn buf_mut(&mut self) -> &mut [u32] {
        &mut self.data
    }
    fn debug_flip_and_present(&mut self) {}
}

struct BenchState {
    renderer: Software3D,
    level_data: LevelData,
    pic_data: PicData,
    pos: Vec3,
    angle_rad: f32,
    buffer: HeadlessBuffer,
}

fn load_state() -> BenchState {
    let wad = WadData::new(&test_utils::doom_wad_path());
    let pic_data = PicData::init(&wad, &[]);
    let mut level_data = LevelData::default();
    level_data.load(
        "E1M6",
        |name| pic_data.flat_num_for_name(name),
        &wad,
        None,
        None,
    );

    let start = level_data
        .things()
        .iter()
        .find(|t| t.kind == 1)
        .copied()
        .unwrap_or_else(|| wad::types::WadThing::new(0, 0, 0, 1, 0));

    let pos = Vec3::new(start.x as f32, start.y as f32, 41.0);
    let angle_rad = (start.angle as f32).to_radians();

    let fov = std::f32::consts::FRAC_PI_2;
    let renderer = Software3D::new(
        WIDTH as f32,
        HEIGHT as f32,
        fov,
        DebugDrawOptions::default(),
    );
    let buffer = HeadlessBuffer::new(WIDTH, HEIGHT);

    BenchState {
        renderer,
        level_data,
        pic_data,
        pos,
        angle_rad,
        buffer,
    }
}

fn bench_render_frame(c: &mut Criterion) {
    let BenchState {
        mut renderer,
        level_data,
        mut pic_data,
        pos,
        angle_rad,
        mut buffer,
    } = load_state();

    c.bench_function("render_frame_e1m6", |b| {
        b.iter(|| {
            renderer.draw_view_bench(pos, angle_rad, 0.0, &level_data, &mut pic_data, &mut buffer);
        });
    });
}

criterion_group!(benches, bench_render_frame);
criterion_main!(benches);
