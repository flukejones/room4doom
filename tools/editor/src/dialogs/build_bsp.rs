//! BSP build settings and launch action.

use super::{DialogsEvent, DialogsView};
use gpui::{AnyElement, Context, Window, div, prelude::*};

#[derive(Clone)]
pub(crate) struct BuildBspDraft {
    pub(crate) interval_ms: i32,
    pub(crate) keep_all: bool,
}

impl Default for BuildBspDraft {
    fn default() -> Self {
        Self {
            interval_ms: 250,
            keep_all: true,
        }
    }
}

impl DialogsView {
    pub(super) fn build_bsp(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let interval = self.forms.field(
            "bsp-interval",
            "Interval (ms)",
            self.drafts.build_bsp.interval_ms.to_string(),
            cx.listener(|this, value: &String, _, cx| {
                if let Ok(ms) = value.parse() {
                    this.drafts.build_bsp.interval_ms = ms;
                    cx.notify();
                }
            }),
            |_, _, _| {},
            window,
            cx,
        );
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(interval)
            .child(self.forms.switch(
                "bsp-keep-all",
                "Keep all split planes",
                self.drafts.build_bsp.keep_all,
                cx.listener(|this, value: &bool, _, cx| {
                    this.drafts.build_bsp.keep_all = *value;
                    cx.notify();
                }),
            ))
            .child(self.forms.button(
                "bsp-run",
                "Build",
                cx.listener(|this, _, _, cx| {
                    cx.emit(DialogsEvent::BuildBsp {
                        interval_ms: this.drafts.build_bsp.interval_ms,
                        keep_all: this.drafts.build_bsp.keep_all,
                    });
                    this.close_popup(cx);
                }),
            ))
            .into_any_element()
    }
}
