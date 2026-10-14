//! Native dialog window creation, focus and platform events.

use super::{DialogsEvent, DialogsView, PopupId, flush_input_registry};
use gpui::{
    AnyWindowHandle, Bounds, Context, Window, WindowBackgroundAppearance, WindowBounds, WindowKind,
    px, size,
};

impl DialogsView {
    pub(crate) fn native_window(&self) -> Option<AnyWindowHandle> {
        self.native_window.map(Into::into)
    }

    pub(super) fn open_native(&mut self, id: PopupId, parent: &Window, cx: &mut Context<Self>) {
        let defaults = Self::default_size(id);
        let minimum = match id {
            PopupId::WallEdit => size(px(640.), px(360.)),
            PopupId::SectorEdit => size(px(560.), px(320.)),
            _ => size(px(320.), px(180.)),
        };
        let geom = self.popup_geom(id);
        let dimension = |saved: f32, default| {
            if saved.is_finite() && saved > 0. {
                px(saved)
            } else {
                default
            }
        };
        let mut requested = size(
            dimension(geom.w, defaults.width),
            dimension(geom.h, defaults.height),
        );
        let display = parent.display(cx);
        if let Some(display) = &display {
            let available = display.bounds().size;
            requested.width = requested.width.min(available.width - px(48.));
            requested.height = requested.height.min(available.height - px(48.));
        }
        requested.width = requested.width.max(minimum.width);
        requested.height = requested.height.max(minimum.height);
        self.native_title = self.window_title(id);
        let mut options = ui_common::window_options(self.native_title.clone(), requested, cx);
        options.kind = WindowKind::Dialog;
        options.is_resizable = true;
        options.is_minimizable = false;
        options.window_min_size = Some(minimum);
        options.window_background = WindowBackgroundAppearance::Opaque;
        options.display_id = display.as_ref().map(|display| display.id());
        options.window_bounds = Some(WindowBounds::Windowed(Bounds::centered(
            options.display_id,
            requested,
            cx,
        )));
        let view = cx.entity();
        let serial = self.serial;
        // Window creation draws immediately; leave the dialog entity's update before opening it.
        cx.defer(move |cx| {
            if view.read(cx).serial != serial || view.read(cx).active != Some(id) {
                return;
            }
            match cx.open_window(options, |window, cx| {
                view.update(cx, |this, cx| this.initialize_native(window, serial, cx));
                view.clone()
            }) {
                Ok(handle) => view.update(cx, |this, _| this.native_window = Some(handle)),
                Err(error) => {
                    log::error!("opening dialog: {error}");
                    view.update(cx, |this, cx| this.close_popup(cx));
                }
            }
        });
    }

    fn initialize_native(&mut self, window: &mut Window, serial: u64, cx: &mut Context<Self>) {
        window.focus(&self.popup_focus, cx);
        self.update_native_size(window);
        self.native_subscriptions.push(cx.observe_window_bounds(
            window,
            move |this, window, cx| {
                if this.serial == serial {
                    this.update_native_size(window);
                    cx.notify();
                }
            },
        ));
        self.native_subscriptions.push(cx.observe_window_appearance(
            window,
            move |this, window, cx| {
                if this.serial == serial {
                    cx.emit(DialogsEvent::Appearance(window.appearance()));
                    cx.notify();
                }
            },
        ));
        self.native_subscriptions
            .push(cx.observe_global::<ui_common::GlobalTheme>(|_, cx| cx.notify()));
        let weak = cx.weak_entity();
        let inputs = self.input_registry();
        window.on_window_should_close(cx, move |window, cx| {
            if !weak
                .update(cx, |this, _| this.serial == serial)
                .unwrap_or(false)
            {
                return true;
            }
            flush_input_registry(&inputs, window, cx);
            weak.update(cx, |this, cx| {
                if this.serial == serial {
                    this.update_native_size(window);
                    // The platform is closing this window; do not remove it a second time.
                    this.native_window = None;
                    this.close_popup(cx);
                }
            })
            .ok();
            true
        });
        cx.emit(DialogsEvent::Appearance(window.appearance()));
    }

    pub(super) fn close_native(&mut self, cx: &mut Context<Self>) {
        self.native_subscriptions.clear();
        if let Some(handle) = self.native_window.take() {
            let handle: AnyWindowHandle = handle.into();
            cx.defer(move |cx| {
                handle
                    .update(cx, |_, window, _| window.remove_window())
                    .ok();
            });
        }
    }

    pub(super) fn update_native_size(&mut self, window: &Window) {
        if let Some(id) = self.active {
            let mut geom = self.popup_geom(id);
            geom.w = window.viewport_size().width.into();
            geom.h = window.viewport_size().height.into();
            self.set_popup_geom(id, geom);
        }
    }
}
