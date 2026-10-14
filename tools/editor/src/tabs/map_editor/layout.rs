//! Map canvas, toolbar and inspector composition.

use super::MapEditor;
use crate::app::preferences::ToolbarPositionPref;
use gpui::{
    AnyElement, Context, CursorStyle, MouseButton, MouseDownEvent, Pixels, Point, StyleRefinement,
    Window, div, prelude::*, px,
};
use ui_common::ActiveTheme as _;

pub(super) struct InspectorDrag {
    start: f32,
    width: f32,
}

impl MapEditor {
    pub(super) fn map_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        // These controls wrap, so their height must be measured from the contents.
        let toolbar = self.toolbar.clone();
        let inspector = self.inspector.clone().cached(
            StyleRefinement::default()
                .w(px(self.inspector_width))
                .h_full()
                .flex_shrink_0(),
        );
        let canvas = self.map_canvas(window, cx);
        let content = div()
            .flex()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .border_t_1()
            .border_color(cx.theme().colors().border)
            .child(div().flex_1().min_w_0().relative().child(canvas))
            .child(self.splitter(cx))
            .child(inspector);
        if self.shared.borrow().prefs.toolbar_position == ToolbarPositionPref::Top {
            div()
                .flex()
                .flex_col()
                .size_full()
                .child(toolbar)
                .child(content)
                .into_any_element()
        } else {
            div()
                .flex()
                .size_full()
                .child(toolbar)
                .child(content)
                .into_any_element()
        }
    }
    pub(super) fn splitter(&self, cx: &Context<Self>) -> AnyElement {
        let width = self.inspector_width;
        div()
            .id("inspector-splitter")
            .w(px(5.))
            .h_full()
            .flex_shrink_0()
            .cursor(CursorStyle::ResizeLeftRight)
            .bg(cx.theme().colors().border)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    this.chrome_drag = Some(InspectorDrag {
                        start: f32::from(event.position.x),
                        width,
                    });
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    pub(crate) fn update_chrome_drag(&mut self, position: Point<Pixels>) -> bool {
        let Some(drag) = &self.chrome_drag else {
            return false;
        };
        let delta = f32::from(position.x) - drag.start;
        self.inspector_width = (drag.width - delta).clamp(240., 640.);
        true
    }

    pub(crate) fn finish_chrome_drag(&mut self) {
        self.chrome_drag = None;
    }
}
