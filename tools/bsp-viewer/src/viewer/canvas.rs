//! Shared opaque BGRA canvas. Toolkits only upload and display its pixels.
use glam::Vec2;
use tiny_skia::{Paint, PathBuilder, PixmapMut, Transform};

#[derive(Clone, Copy)]
pub struct Color(pub [u8; 4]);

impl Color {
    pub fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self([r, g, b, 255])
    }
    pub fn from_rgba_unmultiplied(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self([r, g, b, a])
    }
    pub fn r(self) -> u8 {
        self.0[0]
    }
    pub fn g(self) -> u8 {
        self.0[1]
    }
    pub fn b(self) -> u8 {
        self.0[2]
    }
}

#[derive(Clone, Copy)]
pub struct Stroke {
    width: f32,
    color: Color,
}
impl Stroke {
    pub fn new(width: f32, color: Color) -> Self {
        Self {
            width,
            color,
        }
    }
}

pub struct Canvas<'a> {
    pixels: PixmapMut<'a>,
    builder: &'a mut PathBuilder,
}
impl<'a> Canvas<'a> {
    pub fn new(
        width: u32,
        height: u32,
        pixels: &'a mut [u8],
        builder: &'a mut PathBuilder,
        background: Color,
    ) -> Self {
        let mut pixels = PixmapMut::from_bytes(pixels, width, height).expect("canvas dimensions");
        let [r, g, b, a] = background.0;
        pixels.fill(tiny_skia::Color::from_rgba8(b, g, r, a));
        Self {
            pixels,
            builder,
        }
    }
    pub fn polygon(&mut self, points: impl IntoIterator<Item = Vec2>, color: Color) {
        let mut points = points.into_iter();
        let Some(first) = points.next() else { return };
        let path = &mut self.builder;
        path.move_to(first.x, first.y);
        for point in points {
            path.line_to(point.x, point.y);
        }
        path.close();
        if let Some(path) = std::mem::take(self.builder).finish() {
            self.pixels.fill_path(
                &path,
                &paint(color),
                tiny_skia::FillRule::Winding,
                Transform::identity(),
                None,
            );
            *self.builder = path.clear();
        }
    }
    pub fn line_segment(&mut self, points: [Vec2; 2], stroke: Stroke) {
        let path = &mut self.builder;
        path.move_to(points[0].x, points[0].y);
        path.line_to(points[1].x, points[1].y);
        if let Some(path) = std::mem::take(self.builder).finish() {
            self.pixels.stroke_path(
                &path,
                &paint(stroke.color),
                &tiny_skia::Stroke {
                    width: stroke.width,
                    ..Default::default()
                },
                Transform::identity(),
                None,
            );
            *self.builder = path.clear();
        }
    }
    pub fn circle_filled(&mut self, center: Vec2, radius: f32, color: Color) {
        self.builder.push_circle(center.x, center.y, radius);
        if let Some(path) = std::mem::take(self.builder).finish() {
            self.pixels.fill_path(
                &path,
                &paint(color),
                tiny_skia::FillRule::Winding,
                Transform::identity(),
                None,
            );
            *self.builder = path.clear();
        }
    }
    pub fn circle_stroke(&mut self, center: Vec2, radius: f32, stroke: Stroke) {
        self.builder.push_circle(center.x, center.y, radius);
        if let Some(path) = std::mem::take(self.builder).finish() {
            self.pixels.stroke_path(
                &path,
                &paint(stroke.color),
                &tiny_skia::Stroke {
                    width: stroke.width,
                    ..Default::default()
                },
                Transform::identity(),
                None,
            );
            *self.builder = path.clear();
        }
    }
}
fn paint(color: Color) -> Paint<'static> {
    let mut paint = Paint::default();
    let [r, g, b, a] = color.0;
    // GPUI consumes BGRA. Swap paint channels once, rather than the full image.
    paint.set_color_rgba8(b, g, r, a);
    paint
}

#[derive(Default)]
pub struct Frame {
    pub size: [u32; 2],
    /// BGRA pixels. Presentation may take ownership; unchanged revisions need no upload.
    pub pixels: Vec<u8>,
    pub revision: u64,
}
