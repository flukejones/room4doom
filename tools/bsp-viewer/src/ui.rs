//! GPUI presentation; map behaviour stays in viewer.
use crate::viewer::{Control, Layer, MapViewerApp, MouseButton, Render3DMode};
use glam::Vec2;
use gpui::{
    App, ClipboardItem, Context, Entity, FocusHandle, MouseButton as GpuiButton, RenderImage,
    SharedString, StyleRefinement, WeakEntity, Window, div, img, prelude::*, px, size,
};
use std::{sync::Arc, time::Instant};
use ui_common::theme::Chrome;
use ui_common::widgets::{
    Button, ButtonStyle, Clickable, ContextMenu, DropdownMenu, Icon, IconName, Toggle, ToggleState,
    Toggleable,
};
use ui_common::{TITLEBAR_HEIGHT, color};
const SIDEBAR_WIDTH: f32 = 240.0;
const FOOTER_HEIGHT: f32 = 84.0;
const HELP_WIDTH: f32 = 320.0;
const HELP: &str = "Drag: pan/look · Scroll: zoom\nWASD: move · QE: up/down · Shift: fast\nCmd/Ctrl+drag: line probe\nCmd/Ctrl+Shift+drag: rectangle query\nCmd/Ctrl+click: sector query\nQueries print and copy to clipboard.";

struct Viewer {
    app: MapViewerApp,
    controls: Entity<Controls>,
    focus: FocusHandle,
    image: Option<Arc<RenderImage>>,
    revision: u64,
    last_tick: Instant,
    chrome: Chrome,
    help_expanded: bool,
    light_theme: String,
    dark_theme: String,
    title: SharedString,
    hover: SharedString,
    hover_key: Option<crate::viewer::HoverKey>,
}

#[derive(PartialEq)]
struct ControlState {
    view_3d: bool,
    mode: Render3DMode,
    layers: [bool; 8],
    pinned: bool,
    selection: Option<usize>,
}
impl ControlState {
    fn new(app: &MapViewerApp) -> Self {
        Self {
            view_3d: app.is_3d(),
            mode: app.render_mode(),
            layers: Layer::ALL.map(|layer| app.layer_enabled(layer)),
            pinned: app.pinned(),
            selection: app.selection_key(),
        }
    }
}

// Own the displayed state: reading the viewer during render would make its
// per-frame notifications invalidate this cached subtree.
struct Controls {
    viewer: WeakEntity<Viewer>,
    state: ControlState,
    background: [u8; 4],
    map_name: SharedString,
    summary: SharedString,
    selection: SharedString,
    theme_menu: Option<Entity<ContextMenu>>,
    theme_label: SharedString,
    #[cfg(test)]
    renders: usize,
}
impl Controls {
    fn theme_label(name: &str, variant: ui_common::theme::ThemeVariant) -> SharedString {
        ui_common::theme::themes_for(variant)
            .into_iter()
            .find(|(theme, _)| theme == name)
            .map_or_else(|| name.into(), |(_, label)| label.into())
    }
    fn theme_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) -> DropdownMenu {
        if self.theme_menu.is_none() {
            let dark = ui_common::theme::is_dark(window.appearance());
            let variant = if dark {
                ui_common::theme::ThemeVariant::Dark
            } else {
                ui_common::theme::ThemeVariant::Light
            };
            let viewer = self.viewer.clone();
            self.theme_menu = Some(ContextMenu::build(window, cx, |mut menu, _, _| {
                for (name, label) in ui_common::theme::themes_for(variant) {
                    let viewer = viewer.clone();
                    let label: SharedString = label.into();
                    menu = menu.entry(label, None, move |window, cx| {
                        let _ = viewer.update(cx, |this, cx| {
                            if dark {
                                this.dark_theme.clone_from(&name);
                            } else {
                                this.light_theme.clone_from(&name);
                            }
                            this.apply_theme(window, cx);
                        });
                    });
                }
                menu
            }));
        }
        DropdownMenu::new(
            "theme",
            self.theme_label.clone(),
            self.theme_menu.clone().expect("theme menu"),
        )
        .full_width(true)
    }
    fn button(
        &self,
        id: usize,
        label: &'static str,
        selected: bool,
        control: Control,
    ) -> gpui::AnyElement {
        if matches!(control, Control::Layer(_)) {
            div()
                .w_full()
                .py_1()
                .child(
                    Toggle::checkbox(
                        ("checkbox", id),
                        if selected {
                            ToggleState::Selected
                        } else {
                            ToggleState::Unselected
                        },
                    )
                    .label(label)
                    .label_color(ui_common::widgets::Color::Default)
                    .fill()
                    .on_click({
                        let viewer = self.viewer.clone();
                        move |_, _, cx| {
                            let _ = viewer.update(cx, |this, cx| {
                                if !this.app.animating() {
                                    this.last_tick = Instant::now();
                                }
                                this.app.control(control);
                                cx.notify();
                            });
                        }
                    }),
                )
                .into_any_element()
        } else {
            Button::new(("control", id), label)
                .full_width()
                .style(ButtonStyle::Filled)
                .toggle_state(selected)
                .on_click({
                    let viewer = self.viewer.clone();
                    move |_, _, cx| {
                        let _ = viewer.update(cx, |this, cx| {
                            this.app.control(control);
                            cx.notify();
                        });
                    }
                })
                .into_any_element()
        }
    }
}
impl Render for Controls {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(test)]
        {
            self.renders += 1;
        }
        let theme_picker = self.theme_picker(window, cx);
        let mut controls = div()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .child(div().text_lg().child(self.map_name.clone()))
            .child(self.summary.clone())
            .child(self.button(0, "3D view", self.state.view_3d, Control::View3D));
        if self.state.view_3d {
            for (index, mode) in Render3DMode::ALL.into_iter().enumerate() {
                controls = controls.child(self.button(
                    10 + index,
                    mode.label(),
                    self.state.mode == mode,
                    Control::RenderMode(mode),
                ));
            }
        }
        controls = controls.child(div().mt_2().child("Layers"));
        for (index, layer) in Layer::ALL.into_iter().enumerate() {
            let selected = self.state.layers[index];
            controls = controls.child(self.button(
                20 + index,
                layer.label(),
                selected,
                Control::Layer(layer),
            ));
        }
        controls = controls
            .child(self.button(30, "Fit map", false, Control::Fit))
            .child(div().mt_2().child("Theme (follows system)"))
            .child(theme_picker)
            .child(div().mt_2().child(if self.state.pinned {
                "Selected (pinned)"
            } else {
                "Selected"
            }));
        if self.state.pinned {
            controls = controls.child(self.button(31, "Unpin selection", false, Control::Unpin));
        }
        controls = controls.child(self.selection.clone());

        div()
            .id("sidebar")
            .size_full()
            .overflow_y_scroll()
            .bg(color(self.background))
            .child(controls)
    }
}

impl Viewer {
    fn default_theme_label(appearance: gpui::WindowAppearance) -> SharedString {
        if ui_common::theme::is_dark(appearance) {
            Controls::theme_label(
                ui_common::theme::DEFAULT_DARK_THEME,
                ui_common::theme::ThemeVariant::Dark,
            )
        } else {
            Controls::theme_label(
                ui_common::theme::DEFAULT_LIGHT_THEME,
                ui_common::theme::ThemeVariant::Light,
            )
        }
    }
    fn new(
        app: MapViewerApp,
        focus: FocusHandle,
        appearance: gpui::WindowAppearance,
        cx: &mut Context<Self>,
    ) -> Self {
        let chrome = ui_common::theme::system(appearance).0;
        let viewer = cx.weak_entity();
        let controls = cx.new(|_| Controls {
            viewer,
            state: ControlState::new(&app),
            background: chrome.panel_background,
            map_name: app.data.map_name.as_str().to_owned().into(),
            summary: app.summary().into(),
            selection: app.selection_info().into(),
            theme_menu: None,
            theme_label: Self::default_theme_label(appearance),
            #[cfg(test)]
            renders: 0,
        });
        Self {
            title: format!("bsp-viewer · {}", app.data.map_name).into(),
            controls,
            hover: SharedString::default(),
            hover_key: None,
            app,
            focus,
            image: None,
            revision: 0,
            last_tick: Instant::now(),
            chrome,
            help_expanded: true,
            light_theme: ui_common::theme::DEFAULT_LIGHT_THEME.into(),
            dark_theme: ui_common::theme::DEFAULT_DARK_THEME.into(),
        }
    }
    fn apply_theme(&mut self, window: &Window, cx: &mut Context<Self>) {
        let dark = ui_common::theme::is_dark(window.appearance());
        let (name, variant) = if dark {
            (&self.dark_theme, ui_common::theme::ThemeVariant::Dark)
        } else {
            (&self.light_theme, ui_common::theme::ThemeVariant::Light)
        };
        let (chrome, canvas) = ui_common::theme::resolve(name, variant);
        self.app.set_style(canvas);
        ui_common::apply_chrome(&chrome, dark, cx);
        self.controls.update(cx, |controls, cx| {
            controls.background = chrome.panel_background;
            controls.theme_menu = None;
            controls.theme_label = Controls::theme_label(name, variant);
            cx.notify();
        });
        self.chrome = chrome;
        cx.notify();
    }

    fn local(point: gpui::Point<gpui::Pixels>) -> Vec2 {
        Vec2::new(
            f32::from(point.x) - SIDEBAR_WIDTH,
            f32::from(point.y) - TITLEBAR_HEIGHT,
        )
    }
    fn help(&self, width: f32, height: f32, cx: &Context<Self>) -> impl IntoElement {
        let colours = &self.chrome;
        div()
            .id("help")
            .absolute()
            .top_3()
            .right_3()
            .w(px(if self.help_expanded { HELP_WIDTH } else { 84.0 }))
            .max_w(px((width - 24.0).max(1.0)))
            .max_h(px((height - 24.0).max(28.0)))
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .rounded_md()
            .shadow_md()
            .bg(color(colours.panel_background))
            .occlude()
            .child(
                Button::new("toggle-help", "Help")
                    .start_icon(Icon::new(if self.help_expanded {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronRight
                    }))
                    .style(ButtonStyle::Filled)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.help_expanded = !this.help_expanded;
                        cx.notify();
                    })),
            )
            .when(self.help_expanded, |help| {
                help.child(
                    div()
                        .id("help-text")
                        .min_h_0()
                        .overflow_y_scroll()
                        .child(HELP),
                )
            })
            .on_mouse_down(GpuiButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(GpuiButton::Middle, |_, _, cx| cx.stop_propagation())
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                let changed = this
                    .app
                    .pointer_move(this.app.interacting().then(|| Self::local(event.position)));
                cx.stop_propagation();
                if changed {
                    cx.notify();
                }
            }))
    }
    fn release(&mut self, event: &gpui::MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        let button = match event.button {
            GpuiButton::Left => MouseButton::Primary,
            GpuiButton::Middle => MouseButton::Middle,
            _ => return,
        };
        if !self.app.animating() {
            self.last_tick = Instant::now();
        }
        if let Some(text) = self.app.pointer_up(button, Self::local(event.position)) {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
        cx.notify();
    }
}
impl Render for Viewer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colours = &self.chrome;
        let now = Instant::now();
        if !window.is_window_active() {
            self.app.release_input();
        }
        self.app
            .advance(now.duration_since(self.last_tick).as_secs_f32());
        self.last_tick = now;
        let viewport = window.viewport_size();
        let width = (f32::from(viewport.width) - SIDEBAR_WIDTH).max(1.0);
        let height = (f32::from(viewport.height) - FOOTER_HEIGHT - TITLEBAR_HEIGHT).max(1.0);
        let frame = self.app.render_frame(width as u32, height as u32);
        if self.image.is_none() || self.revision != frame.revision {
            let pixels = std::mem::take(&mut frame.pixels);
            let rgba = image::RgbaImage::from_raw(frame.size[0], frame.size[1], pixels)
                .expect("canvas byte count");
            let next = Arc::new(RenderImage::new([image::Frame::new(rgba)]));
            if let Some(previous) = self.image.replace(next) {
                // Dropping the Arc alone leaves the image's GPU atlas entry cached.
                window
                    .drop_image(previous)
                    .unwrap_or_else(|error| log::error!("GPUI canvas eviction failed: {error}"));
            }
            self.revision = frame.revision;
        }
        if self.app.animating() && window.is_window_active() {
            window.request_animation_frame();
        }
        let state = ControlState::new(&self.app);
        self.controls.update(cx, |controls, cx| {
            if controls.state != state {
                if controls.state.selection != state.selection {
                    controls.selection = self.app.selection_info().into();
                }
                controls.state = state;
                cx.notify();
            }
        });
        let hover_key = self.app.hover_key();
        if self.hover_key != Some(hover_key) {
            self.hover_key = Some(hover_key);
            self.hover = self.app.hover_info().into();
        }
        let canvas = div()
            .id("canvas")
            .relative()
            .w_full()
            .h(px(height))
            .overflow_hidden()
            .child(
                img(self.image.as_ref().expect("canvas uploaded").clone())
                    .w_full()
                    .h_full(),
            )
            .child(self.help(width, height, cx))
            .on_mouse_down(
                GpuiButton::Left,
                cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                    window.focus(&this.focus, cx);
                    this.app.pointer_down(
                        MouseButton::Primary,
                        Self::local(event.position),
                        event.modifiers.platform || event.modifiers.control,
                        event.modifiers.shift,
                    );
                    cx.notify();
                }),
            )
            .on_mouse_down(
                GpuiButton::Middle,
                cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                    this.app.pointer_down(
                        MouseButton::Middle,
                        Self::local(event.position),
                        false,
                        event.modifiers.shift,
                    );
                    cx.notify();
                }),
            );
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(color(colours.window_background))
            .text_color(color(colours.text))
            .text_size(px(12.0))
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                if !this.app.animating() {
                    this.last_tick = Instant::now();
                }
                this.app.key(&event.keystroke.key, true);
                this.app.key("shift", event.keystroke.modifiers.shift);
                cx.notify();
            }))
            .on_key_up(cx.listener(|this, event: &gpui::KeyUpEvent, _, cx| {
                this.app.key(&event.keystroke.key, false);
                this.app.key("shift", event.keystroke.modifiers.shift);
                cx.notify();
            }))
            .on_modifiers_changed(cx.listener(
                |this, event: &gpui::ModifiersChangedEvent, _, cx| {
                    this.app.key("shift", event.modifiers.shift);
                    cx.notify();
                },
            ))
            .on_mouse_move(
                cx.listener(move |this, event: &gpui::MouseMoveEvent, _, cx| {
                    let pos = Self::local(event.position);
                    let changed = this.app.pointer_move(
                        (pos.x >= 0.0 && pos.x < width && pos.y >= 0.0 && pos.y < height
                            || this.app.interacting())
                        .then_some(pos),
                    );
                    if changed {
                        cx.notify();
                    }
                }),
            )
            .on_mouse_up(GpuiButton::Left, cx.listener(Self::release))
            .on_mouse_up(GpuiButton::Middle, cx.listener(Self::release))
            .on_mouse_up_out(GpuiButton::Left, cx.listener(Self::release))
            .on_mouse_up_out(GpuiButton::Middle, cx.listener(Self::release))
            .on_scroll_wheel(
                cx.listener(move |this, event: &gpui::ScrollWheelEvent, _, cx| {
                    let pos = Self::local(event.position);
                    if pos.x >= 0.0 && pos.x < width && pos.y >= 0.0 && pos.y < height {
                        let delta = match event.delta {
                            gpui::ScrollDelta::Pixels(delta) => f32::from(delta.y),
                            gpui::ScrollDelta::Lines(delta) => delta.y * 40.0,
                        };
                        this.app.scroll(pos, delta);
                        cx.notify();
                    }
                }),
            )
            .when(TITLEBAR_HEIGHT > 0.0, |root| {
                root.child(ui_common::titlebar(self.title.clone(), cx))
            })
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(
                        self.controls.clone().cached(
                            StyleRefinement::default()
                                .w(px(SIDEBAR_WIDTH))
                                .h_full()
                                .flex_shrink_0(),
                        ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .child(canvas)
                            .child(
                                div()
                                    .h(px(FOOTER_HEIGHT))
                                    .w_full()
                                    .overflow_hidden()
                                    .p_2()
                                    .child(self.hover.clone()),
                            ),
                    ),
            )
    }
}
pub fn run(mut app: MapViewerApp) {
    let title = format!("bsp-viewer · {}", app.data.map_name);
    ui_common::application().run(move |cx: &mut App| {
        ui_common::init("bsp-viewer", cx);
        app.set_style(ui_common::theme::system(cx.window_appearance()).1);
        cx.open_window(
            ui_common::window_options(title.into(), size(px(1280.0), px(800.0)), cx),
            move |window, cx| {
                cx.new(|cx: &mut Context<Viewer>| {
                    cx.observe_window_activation(window, |this, _, cx| {
                        this.last_tick = Instant::now();
                        cx.notify();
                    })
                    .detach();
                    cx.observe_window_appearance(window, |this, window, cx| {
                        this.apply_theme(window, cx);
                    })
                    .detach();
                    let focus = cx.focus_handle();
                    window.focus(&focus, cx);
                    Viewer::new(app, focus, window.appearance(), cx)
                })
            },
        )
        .expect("GPUI window");
        cx.activate(true);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{TestAppContext, VisualTestContext};
    use std::{
        hash::{Hash as _, Hasher as _},
        time::Duration,
    };

    #[gpui::test]
    #[ignore = "manual optimized viewer profiling workload"]
    fn profile_bsp(cx: &mut TestAppContext) {
        cx.update(|cx| ui_common::init("BSP profile", cx));
        let args = crate::Args {
            iwad: test_utils::doom1_wad_path().display().to_string(),
            pwad: Vec::new(),
            map: "E1M1".into(),
            command: None,
        };
        let wad = wad::WadData::new(&test_utils::doom1_wad_path());
        let mut level = crate::load_map(&args, &wad);
        let camera = crate::player_spawn_camera(&mut level);
        let data = crate::viewer::extract_viewer_data("E1M1", &level, &wad);
        let renderer = crate::viewer::Renderer3D::new(&wad, Render3DMode::Textured, 640, 480);
        let mut app = MapViewerApp::new(data, Some(level), Some(renderer), camera);
        if std::env::var_os("BSP_PROFILE_NO_LAYERS").is_some() {
            for layer in Layer::ALL {
                if app.layer_enabled(layer) {
                    app.control(Control::Layer(layer));
                }
            }
        }
        let root = cx.open_window(
            size(
                px(640. + SIDEBAR_WIDTH),
                px(480. + FOOTER_HEIGHT + TITLEBAR_HEIGHT),
            ),
            |_, cx| {
                let mut viewer = Viewer::new(app, cx.focus_handle(), cx.window_appearance(), cx);
                viewer.help_expanded = false;
                viewer
            },
        );
        let mut visual = VisualTestContext::from_window(root.into(), cx);
        visual.run_until_parked();
        visual.update(|window, _| window.activate_window());
        visual.run_until_parked();
        visual.update(|window, _| assert!(window.is_window_active()));
        for stage in ["hover", "zoom", "3d-idle", "3d-moving", "pick"] {
            root.update(&mut visual, |viewer, _, _| {
                if stage == "3d-idle" {
                    viewer.app.control(Control::View3D);
                }
                viewer.app.key("w", stage == "3d-moving");
            })
            .unwrap();
            let control_renders = root
                .update(&mut visual, |viewer, _, cx| {
                    viewer.controls.read(cx).renders
                })
                .unwrap();
            let started = Instant::now();
            for frame in 0_u16..200 {
                root.update(&mut visual, |viewer, _, cx| {
                    match stage {
                        "hover" => {
                            viewer
                                .app
                                .pointer_move(Some(Vec2::new(200. + f32::from(frame % 20), 180.)));
                        }
                        "zoom" => viewer.app.scroll(
                            Vec2::new(320., 240.),
                            if frame % 2 == 0 { 10. } else { -10. },
                        ),
                        "pick" => {
                            let pos = Vec2::new(320., 240.);
                            viewer
                                .app
                                .pointer_down(MouseButton::Primary, pos, false, false);
                            let _ = viewer.app.pointer_up(MouseButton::Primary, pos);
                        }
                        _ => {}
                    }
                    viewer.last_tick = Instant::now() - Duration::from_millis(16);
                    // Test mode draws notified windows while flushing effects.
                    cx.notify();
                })
                .unwrap();
            }
            println!("BSP_PROFILE: {stage}: {:?}", started.elapsed());
            root.update(&mut visual, |viewer, _, cx| {
                if stage == "3d-idle" || stage == "3d-moving" {
                    assert!(
                        viewer.controls.read(cx).renders <= control_renders + 1,
                        "camera redraws must reuse the controls"
                    );
                }
                let mut hash = std::collections::hash_map::DefaultHasher::new();
                viewer
                    .image
                    .as_ref()
                    .unwrap()
                    .as_bytes(0)
                    .unwrap()
                    .hash(&mut hash);
                println!("BSP_IMAGE: {stage}: {:x}", hash.finish());
            })
            .unwrap();
        }
        let renders = root
            .update(&mut visual, |viewer, _, cx| {
                viewer.controls.read(cx).renders
            })
            .unwrap();
        visual.simulate_resize(size(px(1000.), px(800.)));
        visual.run_until_parked();
        root.update(&mut visual, |viewer, window, cx| {
            assert!(
                viewer.controls.read(cx).renders > renders,
                "resize must relayout controls"
            );
            viewer.dark_theme = "gruvbox-dark".into();
            viewer.light_theme = "gruvbox-light".into();
            viewer.apply_theme(window, cx);
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
