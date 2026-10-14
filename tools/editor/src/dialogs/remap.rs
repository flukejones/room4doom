//! Batch remapping of things, textures, flats and specials.

pub(crate) mod actions;

use super::{DialogsView, labels, row_index};
use crate::components::types::{RemapKind, RemapRow};
use gpui::{AnyElement, Context, Window, div, prelude::*};

#[derive(Clone)]
pub(crate) struct RemapDraft {
    pub(crate) kind: RemapKind,
    pub(crate) rows: Vec<RemapRow>,
    pub(crate) status: String,
}

impl Default for RemapDraft {
    fn default() -> Self {
        Self {
            kind: RemapKind::Thing,
            rows: Default::default(),
            status: String::new(),
        }
    }
}

impl DialogsView {
    pub(super) fn remap(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let draft = &self.drafts.remap;
        let kinds = [
            RemapKind::Thing,
            RemapKind::Texture,
            RemapKind::Flat,
            RemapKind::LineSpecial,
            RemapKind::SectorSpecial,
        ];
        let mut panel = div().flex().flex_col().gap_2().child(
            self.forms.select_field(
                "remap-kind",
                "Kind",
                labels(&[
                    "Things",
                    "Textures",
                    "Flats",
                    "Line specials",
                    "Sector specials",
                ]),
                row_index(
                    kinds
                        .iter()
                        .position(|kind| *kind == draft.kind)
                        .unwrap_or(0),
                ),
                cx.listener(move |this, index: &i32, _, cx| {
                    actions::kind_changed(
                        &mut this.drafts,
                        &mut this.shared.borrow_mut(),
                        kinds[*index as usize],
                    );
                    cx.notify();
                }),
                window,
                cx,
            ),
        );
        for (i, row) in draft.rows.iter().enumerate() {
            panel = panel.child(self.forms.field(
                format!("remap-{i}"),
                row.from.clone(),
                &row.to,
                cx.listener(move |this, value: &String, _, cx| {
                    actions::to_edited(
                        &mut this.drafts,
                        &this.shared.borrow(),
                        row_index(i),
                        value.clone(),
                    );
                    cx.notify();
                }),
                |_, _, _| {},
                window,
                cx,
            ));
        }
        panel
            .child(draft.status.clone())
            .child(self.forms.button(
                "remap-apply",
                "Apply",
                cx.listener(|this, _, _, cx| {
                    let damage = actions::apply(&mut this.drafts, &mut this.shared.borrow_mut());
                    Self::emit_damage(damage, cx);
                }),
            ))
            .into_any_element()
    }
}
