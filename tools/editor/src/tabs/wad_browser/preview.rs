//! Native image and vector previews. No SVG or controller representation is needed.

use crate::gfx::image::UiImage;
use std::sync::Arc;

use editor_core::{EditorMap, import_wad_map};
use gpui::{
    AnyElement, Context, ObjectFit, PathBuilder, RenderImage, Window, canvas, div, img, point,
    prelude::*, px,
};

use super::WadBrowser;

#[derive(Default)]
pub(super) struct Preview {
    pub title: String,
    pub text: String,
    pub wireframe: Option<Arc<Wireframe>>,
    image: Option<Arc<RenderImage>>,
    retired: Vec<Arc<RenderImage>>,
}

impl Preview {
    pub(super) fn clear(&mut self) {
        self.title.clear();
        self.text.clear();
        self.wireframe = None;
        if let Some(image) = self.image.take() {
            self.retired.push(image);
        }
    }

    pub(super) fn set_image(&mut self, source: &UiImage) {
        let mut pixels = source.as_bytes().to_vec();
        for pixel in pixels.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        if let Some(image) = image::RgbaImage::from_raw(source.width(), source.height(), pixels) {
            self.image = Some(Arc::new(RenderImage::new(vec![image::Frame::new(image)])));
        }
    }

    pub(super) fn retire_images(&mut self, window: &mut Window) {
        for image in self.retired.drain(..) {
            if let Err(error) = window.drop_image(image) {
                log::error!("WAD preview eviction failed: {error}");
            }
        }
    }
}

/// Independent line segments in display coordinates, grouped by canvas colour role.
pub(super) struct Wireframe {
    lines: [Vec<[f32; 4]>; 3],
    origin: [f32; 2],
    size: [f32; 2],
}

impl Wireframe {
    pub(super) fn from_map(map: &EditorMap) -> Arc<Self> {
        let mut lines = [Vec::new(), Vec::new(), Vec::new()];
        let mut min = [f32::INFINITY; 2];
        let mut max = [f32::NEG_INFINITY; 2];
        for line in map.lines.values() {
            let (Some(a), Some(b)) = (map.vertices.get(line.v1), map.vertices.get(line.v2)) else {
                continue;
            };
            let group = if line.special != 0 {
                2
            } else {
                usize::from(line.back.is_some())
            };
            lines[group].push([a.x, -a.y, b.x, -b.y]);
            for vertex in [a, b] {
                min[0] = min[0].min(vertex.x);
                min[1] = min[1].min(-vertex.y);
                max[0] = max[0].max(vertex.x);
                max[1] = max[1].max(-vertex.y);
            }
        }
        if !min[0].is_finite() {
            return Arc::new(Self {
                lines,
                origin: [0.; 2],
                size: [1.; 2],
            });
        }
        let width = (max[0] - min[0]).max(1.);
        let height = (max[1] - min[1]).max(1.);
        let pad = width.max(height) * 0.04;
        Arc::new(Self {
            lines,
            origin: [min[0] - pad, min[1] - pad],
            size: [width + 2. * pad, height + 2. * pad],
        })
    }
}

impl WadBrowser {
    pub(super) fn map_wireframe(&mut self, wad: usize, name: &str) -> Option<Arc<Wireframe>> {
        let key = (wad, name.to_owned());
        if let Some(wireframe) = self.wireframes.get(&key) {
            return Some(wireframe.clone());
        }
        let loaded = self.wads.get(wad)?;
        match import_wad_map(&loaded.wad, name) {
            Ok(map) => {
                let wireframe = Wireframe::from_map(&map);
                self.wireframes.insert(key, wireframe.clone());
                Some(wireframe)
            }
            Err(error) => {
                log::error!("Preview {name}: {error}");
                None
            }
        }
    }

    pub(super) fn render_preview(&self, _: &Context<Self>) -> AnyElement {
        if let Some(wireframe) = self.preview.wireframe.clone() {
            let style = self.shared.borrow().map_editor.style;
            let colours = [style.one_sided, style.two_sided, style.special].map(ui_common::color);
            return canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    let scale = (f32::from(bounds.size.width) / wireframe.size[0])
                        .min(f32::from(bounds.size.height) / wireframe.size[1]);
                    let origin = bounds.origin
                        + point(
                            (bounds.size.width - px(wireframe.size[0] * scale)) * 0.5,
                            (bounds.size.height - px(wireframe.size[1] * scale)) * 0.5,
                        );
                    for (segments, colour) in wireframe.lines.iter().zip(colours) {
                        let mut path = PathBuilder::stroke(px(1.));
                        for [x1, y1, x2, y2] in segments {
                            path.move_to(
                                origin
                                    + point(
                                        px((x1 - wireframe.origin[0]) * scale),
                                        px((y1 - wireframe.origin[1]) * scale),
                                    ),
                            );
                            path.line_to(
                                origin
                                    + point(
                                        px((x2 - wireframe.origin[0]) * scale),
                                        px((y2 - wireframe.origin[1]) * scale),
                                    ),
                            );
                        }
                        if let Ok(path) = path.build() {
                            window.paint_path(path, colour);
                        }
                    }
                },
            )
            .size_full()
            .into_any_element();
        }
        if let Some(image) = &self.preview.image {
            return img(image.clone())
                .size_full()
                .object_fit(ObjectFit::Contain)
                .into_any_element();
        }
        div()
            .id("wad-preview-text")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .child(self.preview.text.clone())
            .into_any_element()
    }
}
