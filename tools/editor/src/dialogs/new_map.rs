//! New map naming and creation.

use super::DialogsView;
use gpui::{AnyElement, Context, Window, div, prelude::*};

#[derive(Clone)]
pub(crate) struct NewMapDraft {
    pub(crate) name: String,
}

impl Default for NewMapDraft {
    fn default() -> Self {
        Self {
            name: "MAP01".into(),
        }
    }
}

impl DialogsView {
    pub(super) fn new_map(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let input = self.forms.action_field(
            "new-map-name",
            "Name",
            &self.drafts.new_map.name,
            cx.listener(|this, value: &String, _, cx| {
                this.drafts.new_map.name = value.clone();
                cx.notify();
            }),
            cx.listener(|this, _, _, cx| this.create_map(cx)),
            window,
            cx,
        );
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(input)
            .child(self.forms.button(
                "create-map",
                "Create map",
                cx.listener(|this, _, _, cx| this.create_map(cx)),
            ))
            .into_any_element()
    }
}
