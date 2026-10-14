//! Geometry audit results and navigation.

pub(crate) mod actions;

use super::DialogsView;
use gpui::{AnyElement, Context, div, prelude::*};

#[derive(Clone, Default)]
pub(crate) struct AuditDraft {
    pub(crate) issues: Vec<String>,
}

impl DialogsView {
    pub(super) fn audit(&self, cx: &Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(self.forms.button(
                "audit-rescan",
                "Rescan",
                cx.listener(|this, _, _, cx| {
                    actions::refresh(&mut this.drafts, &this.shared.borrow());
                    cx.notify();
                }),
            ))
            .children(
                self.drafts
                    .audit
                    .issues
                    .iter()
                    .enumerate()
                    .map(|(i, issue)| {
                        self.forms.list_item(
                            ("audit-issue", i),
                            issue.clone(),
                            false,
                            cx.listener(move |this, _, _, cx| {
                                let damage = actions::jump_to(
                                    &this.drafts,
                                    &mut this.shared.borrow_mut(),
                                    i,
                                );
                                Self::emit_damage(damage, cx);
                            }),
                        )
                    }),
            )
            .into_any_element()
    }
}
