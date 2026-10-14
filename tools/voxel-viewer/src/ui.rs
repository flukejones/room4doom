//! GPUI window, controls and bounded frame presentation.

use crate::renderer::{H, Scene, W};
use gpui::{
    App, Context, Entity, FocusHandle, MouseButton, RenderImage, SharedString, StyleRefinement,
    WeakEntity, Window, div, img, prelude::*, px, size,
};
use std::sync::{Arc, mpsc};
use ui_common::widgets::{Button, ButtonStyle, Clickable as _, Icon, IconName};
use ui_common::{TITLEBAR_HEIGHT, color};

enum Input {
    Key(String),
    Drag(f32, f32),
    Scroll(f32),
    Background([u8; 4]),
    Present,
    Quit,
}

impl Input {
    fn apply(self, scene: &mut Scene) {
        match self {
            Self::Key(key) => scene.key(&key),
            Self::Drag(dx, dy) => scene.drag(dx, dy),
            Self::Scroll(delta) => scene.scroll(delta),
            Self::Background(colour) => scene.background(colour),
            Self::Present | Self::Quit => {}
        }
    }
}

struct Frame {
    pixels: Vec<u8>,
    title: SharedString,
}

enum RenderSource {
    Inline(Box<Scene>),
    Benchmark {
        inputs: mpsc::Sender<Input>,
        frames: mpsc::Receiver<Frame>,
        pending: bool,
    },
}

impl RenderSource {
    fn new(mut scene: Scene, benchmark: bool) -> Self {
        if !benchmark {
            return Self::Inline(Box::new(scene));
        }
        let (inputs, receive) = mpsc::channel();
        let (output, frames) = mpsc::sync_channel(1);
        // Render freely for benchmarking; prepare only frames requested by the UI.
        // A single outstanding request bounds both queued pixels and conversion work.
        std::thread::spawn(move || {
            let mut title = SharedString::default();
            loop {
                let mut present = false;
                for input in receive.try_iter() {
                    match input {
                        Input::Quit => return,
                        Input::Present => present = true,
                        input => input.apply(&mut scene),
                    }
                }
                scene.render();
                if present {
                    if title.as_ref() != scene.title() {
                        title = scene.title().to_owned().into();
                    }
                    if output
                        .send(Frame {
                            pixels: scene.frame(),
                            title: title.clone(),
                        })
                        .is_err()
                    {
                        return;
                    }
                }
            }
        });
        Self::Benchmark {
            inputs,
            frames,
            pending: false,
        }
    }

    fn animating(&self) -> bool {
        match self {
            Self::Inline(scene) => scene.animating(),
            Self::Benchmark {
                ..
            } => true,
        }
    }

    fn input(&mut self, input: Input) {
        match self {
            Self::Inline(scene) => input.apply(scene),
            Self::Benchmark {
                inputs,
                ..
            } => {
                let _ = inputs.send(input);
            }
        }
    }

    fn frame(&mut self, title: &SharedString) -> Option<Frame> {
        match self {
            Self::Inline(scene) => {
                if !scene.needs_frame() {
                    return None;
                }
                scene.render();
                Some(Frame {
                    pixels: scene.frame(),
                    title: if title.as_ref() == scene.title() {
                        title.clone()
                    } else {
                        scene.title().to_owned().into()
                    },
                })
            }
            Self::Benchmark {
                inputs,
                frames,
                pending,
            } => {
                let frame = frames.try_recv().ok();
                if frame.is_some() {
                    *pending = false;
                }
                if !*pending {
                    *pending = inputs.send(Input::Present).is_ok();
                }
                frame
            }
        }
    }
}

impl Drop for RenderSource {
    fn drop(&mut self) {
        if let Self::Benchmark {
            inputs,
            ..
        } = self
        {
            let _ = inputs.send(Input::Quit);
        }
    }
}

struct Viewer {
    source: RenderSource,
    controls: Entity<Controls>,
    focus: FocusHandle,
    image: Option<Arc<RenderImage>>,
    chrome: ui_common::theme::Chrome,
    title: SharedString,
    dragging: bool,
    last_mouse: gpui::Point<gpui::Pixels>,
}

struct Controls {
    viewer: WeakEntity<Viewer>,
    background: [u8; 4],
    #[cfg(test)]
    renders: usize,
}
impl Controls {
    fn button(
        &self,
        id: &'static str,
        label: &'static str,
        key: &'static str,
        icon: IconName,
    ) -> Button {
        Button::new(id, label)
            .style(ButtonStyle::Filled)
            .start_icon(Icon::new(icon))
            .on_click({
                let viewer = self.viewer.clone();
                move |_, _, cx| {
                    let _ = viewer.update(cx, |this, cx| {
                        this.source.input(Input::Key(key.into()));
                        cx.notify();
                    });
                }
            })
    }
}
impl Render for Controls {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        #[cfg(test)]
        {
            self.renders += 1;
        }
        div()
            .flex()
            .gap_2()
            .p_2()
            .bg(color(self.background))
            .child(self.button("mode", "Slices / Direct", "s", IconName::Box))
            .child(self.button("wireframe", "Wireframe", "w", IconName::BoxOpen))
            .child(self.button("slice", "Single slice", "d", IconName::Image))
            .child(self.button("rotate", "Auto rotate", "r", IconName::RotateCw))
    }
}

impl Viewer {
    fn new(
        source: RenderSource,
        focus: FocusHandle,
        chrome: ui_common::theme::Chrome,
        cx: &mut Context<Self>,
    ) -> Self {
        let viewer = cx.weak_entity();
        let controls = cx.new(|_| Controls {
            viewer,
            background: chrome.panel_background,
            #[cfg(test)]
            renders: 0,
        });
        Self {
            source,
            controls,
            focus,
            image: None,
            chrome,
            title: "Voxel Viewer".into(),
            dragging: false,
            last_mouse: gpui::point(px(0.0), px(0.0)),
        }
    }
}

impl Render for Viewer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !window.is_window_active() {
            self.dragging = false;
        }
        if let Some(frame) = self.source.frame(&self.title) {
            self.title = frame.title;
            let pixels = image::RgbaImage::from_raw(W, H, frame.pixels).expect("voxel frame size");
            let next = Arc::new(RenderImage::new([image::Frame::new(pixels)]));
            if let Some(previous) = self.image.replace(next) {
                window.drop_image(previous).expect("evict voxel frame");
            }
        }
        if self.source.animating() && window.is_window_active() {
            window.request_animation_frame();
        }
        let chrome = &self.chrome;
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(color(chrome.window_background))
            .text_color(color(chrome.text))
            .text_size(px(12.0))
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    cx.quit();
                } else {
                    this.source.input(Input::Key(event.keystroke.key.clone()));
                    cx.notify();
                }
            }))
            .when(TITLEBAR_HEIGHT > 0.0, |root| {
                root.child(ui_common::titlebar(self.title.clone(), cx))
            })
            .child(
                self.controls.clone().cached(
                    StyleRefinement::default()
                        .w_full()
                        .h(gpui::rems(38.0 / 16.0))
                        .flex_shrink_0(),
                ),
            )
            .child(
                div()
                    .id("voxel-canvas")
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .when_some(self.image.clone(), |canvas, image| {
                        canvas.child(img(image).size_full().object_fit(gpui::ObjectFit::Contain))
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                            window.focus(&this.focus, cx);
                            this.dragging = true;
                            this.last_mouse = event.position;
                            this.source.input(Input::Key("pause-rotate".into()));
                            cx.notify();
                        }),
                    )
                    .on_scroll_wheel(cx.listener(|this, event: &gpui::ScrollWheelEvent, _, cx| {
                        let delta = match event.delta {
                            gpui::ScrollDelta::Lines(delta) => delta.y,
                            gpui::ScrollDelta::Pixels(delta) => f32::from(delta.y) * 0.01,
                        };
                        this.source.input(Input::Scroll(delta));
                        cx.notify();
                    })),
            )
            .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                if this.dragging {
                    this.source.input(Input::Drag(
                        f32::from(event.position.x - this.last_mouse.x),
                        f32::from(event.position.y - this.last_mouse.y),
                    ));
                    cx.notify();
                }
                this.last_mouse = event.position;
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.dragging = false),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.dragging = false),
            )
            .child(
                div()
                    .p_2()
                    .bg(color(chrome.panel_background))
                    .child("Drag: rotate · Scroll: zoom / slice · Arrows: pan · S/W/D/R: modes"),
            )
    }
}

pub(crate) fn run(mut scene: Scene, benchmark: bool) {
    ui_common::application().run(move |cx: &mut App| {
        ui_common::init("voxel-viewer", cx);
        scene.background(
            ui_common::theme::system(cx.window_appearance())
                .0
                .window_background,
        );
        let source = RenderSource::new(scene, benchmark);
        cx.open_window(
            ui_common::window_options("Voxel Viewer".into(), size(px(720.0), px(600.0)), cx),
            move |window, cx| {
                cx.new(|cx: &mut Context<Viewer>| {
                    cx.observe_window_activation(window, |_, _, cx| cx.notify())
                        .detach();
                    cx.observe_window_appearance(window, |this, window, cx| {
                        ui_common::apply_system_theme(window.appearance(), cx);
                        this.chrome = ui_common::theme::system(window.appearance()).0;
                        this.controls.update(cx, |controls, cx| {
                            controls.background = this.chrome.panel_background;
                            cx.notify();
                        });
                        this.source
                            .input(Input::Background(this.chrome.window_background));
                        cx.notify();
                    })
                    .detach();
                    let focus = cx.focus_handle();
                    window.focus(&focus, cx);
                    Viewer::new(
                        source,
                        focus,
                        ui_common::theme::system(window.appearance()).0,
                        cx,
                    )
                })
            },
        )
        .expect("voxel viewer window");
        cx.activate(true);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{TestAppContext, VisualTestContext};
    use pic_data::{ByteOrder, PALETTE_LEN, PalLit, WadPalette};
    use std::{
        hash::{Hash as _, Hasher as _},
        time::Instant,
    };

    fn profile_scene() -> Scene {
        let path = std::env::var_os("VOXEL_PROFILE_MODEL")
            .map_or_else(|| test_utils::kvx_path("trooa.kvx"), Into::into);
        let model = pic_data::voxel::kvx::VoxelModel::load(&std::fs::read(path).unwrap()).unwrap();
        let slices = pic_data::voxel::slices::generate(&model);
        let palette = [(128, 128, 128); 256];
        let pal = PalLit::new(
            &[WadPalette([0xFF808080; 256]); PALETTE_LEN],
            ByteOrder::Argb,
        );
        Scene::new(model, slices, &palette, pal, true)
    }

    #[test]
    #[ignore = "manual unlimited renderer profiling; requires trooa.kvx"]
    fn profile_voxel_worker() {
        let mut source = RenderSource::new(profile_scene(), true);
        let mut title = SharedString::default();
        let mut presented = 0;
        for _ in 0..200 {
            if let Some(frame) = source.frame(&title) {
                title = frame.title;
                assert_eq!(frame.pixels.len(), (W * H * 4) as usize);
                presented += 1;
            }
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
        println!("VOXEL_WORKER: {presented} presented frames");
    }

    #[gpui::test]
    #[ignore = "manual optimized viewer profiling workload; requires trooa.kvx"]
    fn profile_voxel(cx: &mut TestAppContext) {
        cx.update(|cx| ui_common::init("Voxel profile", cx));
        let scene = profile_scene();
        let root = cx.open_window(size(px(720.), px(600.)), |_, cx| {
            Viewer::new(
                RenderSource::new(scene, false),
                cx.focus_handle(),
                ui_common::theme::system(cx.window_appearance()).0,
                cx,
            )
        });
        let mut visual = VisualTestContext::from_window(root.into(), cx);
        visual.run_until_parked();
        root.update(&mut visual, |viewer, _, _| {
            viewer.source.input(Input::Key("pause-rotate".into()));
        })
        .unwrap();
        visual.update(|window, _| window.activate_window());
        visual.run_until_parked();
        visual.update(|window, _| assert!(window.is_window_active()));
        for (stage, key) in [
            ("paused", "pause-rotate"),
            ("rotating", "r"),
            ("wireframe", "w"),
            ("single-slice", "d"),
            ("direct", "s"),
        ] {
            root.update(&mut visual, |viewer, _, _| {
                viewer.source.input(Input::Key(key.into()));
            })
            .unwrap();
            let control_renders = root
                .update(&mut visual, |viewer, _, cx| {
                    viewer.controls.read(cx).renders
                })
                .unwrap();
            let started = Instant::now();
            for _ in 0..200 {
                // Test mode draws notified windows while flushing effects.
                root.update(&mut visual, |_, _, cx| cx.notify()).unwrap();
            }
            println!("VOXEL_PROFILE: {stage}: {:?}", started.elapsed());
            root.update(&mut visual, |viewer, _, cx| {
                assert!(
                    viewer.controls.read(cx).renders <= control_renders + 1,
                    "voxel redraws must reuse the controls"
                );
                let mut hash = std::collections::hash_map::DefaultHasher::new();
                viewer
                    .image
                    .as_ref()
                    .unwrap()
                    .as_bytes(0)
                    .unwrap()
                    .hash(&mut hash);
                println!("VOXEL_IMAGE: {stage}: {:x}", hash.finish());
            })
            .unwrap();
        }
        let renders = root
            .update(&mut visual, |viewer, _, cx| {
                viewer.controls.read(cx).renders
            })
            .unwrap();
        visual.simulate_resize(size(px(900.), px(700.)));
        visual.run_until_parked();
        root.update(&mut visual, |viewer, _, cx| {
            assert!(
                viewer.controls.read(cx).renders > renders,
                "resize must relayout controls"
            );
            viewer.controls.update(cx, |controls, cx| {
                controls.background = [200, 200, 200, 255];
                cx.notify();
            });
        })
        .unwrap();
        root.update(&mut visual, |viewer, _, cx| {
            assert!(
                viewer.controls.read(cx).renders > renders + 1,
                "theme must invalidate controls"
            );
        })
        .unwrap();
    }
}
