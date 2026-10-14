//! Map selection from loaded resources.

use super::{DialogsEvent, DialogsView};
use gpui::{AnyElement, Context, div, prelude::*};

#[derive(Clone, Default)]
pub(crate) struct MapsDraft {
    pub(crate) maps: Vec<String>,
}

impl DialogsView {
    pub(super) fn map_list(&self, cx: &Context<Self>) -> AnyElement {
        div()
            .debug_selector(|| "map-choices".into())
            .flex()
            .flex_col()
            .flex_none()
            .w_full()
            .gap_2()
            .children(self.drafts.maps.maps.iter().enumerate().map(|(i, name)| {
                let name = name.clone();
                self.forms.list_item(
                    ("map-choice", i),
                    name.clone(),
                    false,
                    cx.listener(move |this, _, _, cx| {
                        cx.emit(DialogsEvent::ChooseMap(name.clone()));
                        this.close_popup(cx);
                    }),
                )
            }))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests;
