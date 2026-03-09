use std::path::PathBuf;

use criterion::{Criterion, criterion_group, criterion_main};
use glam::Vec3;
use render_trait::{BufferSize, DrawBuffer, SOFT_PIXEL_CHANNELS};
use software3d::{DebugDrawOptions, Software3D};
use wad::WadData;

use gameplay::{MapData, PicData};

// ─── Headless framebuffer ────────────────────────────────────────────────────

const WIDTH: usize = 640;
const HEIGHT: usize = 480;

struct HeadlessBuffer {
    size: BufferSize,
    data: Vec<u8>,
}

impl HeadlessBuffer {
    fn new(width: usize, height: usize) -> Self {
        Self {
            size: BufferSize::new(width, height),
            data: vec![0u8; width * height * SOFT_PIXEL_CHANNELS],
        }
    }
}

impl DrawBuffer for HeadlessBuffer {
    fn size(&self) -> &BufferSize {
        &self.size
    }
    fn set_pixel(&mut self, x: usize, y: usize, colour: &[u8; 4]) {
        let i = (y * WIDTH + x) * SOFT_PIXEL_CHANNELS;
        self.data[i..i + 4].copy_from_slice(colour);
    }
    fn read_pixel(&self, x: usize, y: usize) -> [u8; SOFT_PIXEL_CHANNELS] {
        let i = (y * WIDTH + x) * SOFT_PIXEL_CHANNELS;
        self.data[i..i + 4].try_into().unwrap()
    }
    fn get_buf_index(&self, x: usize, y: usize) -> usize {
        (y * WIDTH + x) * SOFT_PIXEL_CHANNELS
    }
    fn pitch(&self) -> usize {
        WIDTH * SOFT_PIXEL_CHANNELS
    }
    fn buf_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }
    fn debug_flip_and_present(&mut self) {}
}

// ─── Benchmark setup ─────────────────────────────────────────────────────────

struct BenchState {
    renderer: Software3D,
    map_data: MapData,
    pic_data: PicData,
    pos: Vec3,
    angle_rad: f32,
    subsector_id: usize,
    buffer: HeadlessBuffer,
}

fn load_state() -> BenchState {
    let wad = WadData::new(&PathBuf::from("/Users/lukejones/DOOM/doom.wad"));
    let pic_data = PicData::init(&wad);
    let mut map_data = MapData::default();
    map_data.load("E1M6", |name| pic_data.flat_num_for_name(name), &wad);

    // Find the player 1 start thing (type 1) for a realistic camera position.
    let start = map_data
        .things()
        .iter()
        .find(|t| t.kind == 1)
        .copied()
        .unwrap_or_else(|| wad::types::WadThing::new(0, 0, 0, 1, 0));

    let pos = Vec3::new(start.x as f32, start.y as f32, 41.0); // 41 = VIEWHEIGHT
    let angle_rad = (start.angle as f32).to_radians();

    // Locate the subsector containing the player start. Use the raw-pointer
    // variant so the mutable borrow ends before we scan the slice.
    let player_sub_ptr = map_data
        .point_in_subsector_raw(glam::Vec2::new(pos.x, pos.y))
        .as_ptr();
    let subsector_id = map_data
        .subsectors
        .iter()
        .position(|s| std::ptr::eq(s as *const _, player_sub_ptr))
        .unwrap_or(0);

    let fov = std::f32::consts::FRAC_PI_2; // 90°
    let renderer = Software3D::new(
        WIDTH as f32,
        HEIGHT as f32,
        fov,
        DebugDrawOptions::default(),
    );
    let buffer = HeadlessBuffer::new(WIDTH, HEIGHT);

    BenchState {
        renderer,
        map_data,
        pic_data,
        pos,
        angle_rad,
        subsector_id,
        buffer,
    }
}

// ─── Benchmarks ──────────────────────────────────────────────────────────────

fn bench_render_frame(c: &mut Criterion) {
    let BenchState {
        mut renderer,
        map_data,
        mut pic_data,
        pos,
        angle_rad,
        subsector_id,
        mut buffer,
    } = load_state();

    c.bench_function("render_frame_e1m6", |b| {
        b.iter(|| {
            renderer.draw_view_bench(
                pos,
                angle_rad,
                0.0,
                subsector_id,
                &map_data,
                &mut pic_data,
                &mut buffer,
            );
        });
    });
}

criterion_group!(benches, bench_render_frame);
criterion_main!(benches);
