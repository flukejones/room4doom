//! Shared texture, flat and patch selection grid.

pub(crate) mod actions;

use super::*;
use crate::components::types::GfxEntry;
use std::ops::Range;

#[derive(Clone)]
pub(crate) struct AssetPickerDraft {
    pub(crate) entries: Vec<GfxEntry>,
    pub(crate) active_index: i32,
    pub(crate) title: String,
    pub(crate) filter: String,
}

impl Default for AssetPickerDraft {
    fn default() -> Self {
        Self {
            entries: Default::default(),
            active_index: -1,
            title: "Textures".into(),
            filter: String::new(),
        }
    }
}

#[derive(Default)]
pub(super) struct AssetGridState {
    scroll: UniformListScrollHandle,
    selected: Option<(i32, usize)>,
}

impl DialogsView {
    pub(super) fn asset_browser(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let filter = self.forms.field(
            "asset-filter",
            "Filter",
            &self.drafts.asset_picker.filter,
            cx.listener(|this, value: &String, _, cx| {
                this.drafts.asset_picker.filter = value.clone();
                actions::rebuild_entries(&mut this.drafts, &mut this.shared.borrow_mut());
                cx.notify();
            }),
            |_, _, _| {},
            window,
            cx,
        );
        let grid = self.asset_grid(
            "browser",
            |this| &this.drafts.asset_picker.entries,
            self.drafts.asset_picker.active_index,
            self.popup_geom(PopupId::Browser).w - 24.,
            cx,
            Self::pick_asset,
        );
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap_2()
            .child(div().flex_none().child(filter))
            .child(grid)
            .into_any_element()
    }

    fn pick_asset(&mut self, name: &str, cx: &mut Context<Self>) {
        if let Some(target) = self.drafts.browse_target.take() {
            cx.emit(DialogsEvent::AssetPicked {
                target,
                name: name.to_owned(),
            });
        }
        self.close_popup(cx);
    }

    #[expect(
        clippy::cast_possible_truncation,
        reason = "Popup width bounds the visible column count."
    )]
    pub(super) fn asset_grid(
        &mut self,
        id: &'static str,
        entries: fn(&Self) -> &[GfxEntry],
        active: i32,
        width: f32,
        cx: &Context<Self>,
        picked: fn(&mut Self, &str, &mut Context<Self>),
    ) -> AnyElement {
        // Each 128px tile has an 8px gap, except after the last column.
        let columns = ((width + 8.) / 136.).floor().max(1.) as usize;
        let state = self.asset_grids.entry(id).or_default();
        if state.selected != Some((active, columns)) {
            if let Ok(active) = usize::try_from(active) {
                state
                    .scroll
                    .scroll_to_item(active / columns, ScrollStrategy::Nearest);
            }
            state.selected = Some((active, columns));
        }
        let scroll = state.scroll.clone();
        uniform_list(
            format!("asset-grid-{id}"),
            entries(self).len().div_ceil(columns),
            cx.processor(move |this, range: Range<usize>, _, cx| {
                range
                    .map(|row| {
                        let mut line = div().flex().gap_2().h(px(176.));
                        for i in row * columns..((row + 1) * columns).min(entries(this).len()) {
                            let Some(entry) = entries(this).get(i).cloned() else {
                                continue;
                            };
                            let inputs = this.forms.inputs.clone();
                            let focus = this.forms.focus.clone();
                            let name = entry.name.clone();
                            let image = this.forms.image(format!("asset-{id}-{i}"), entry.thumb);
                            line = line.child(
                                Button::new((SharedString::from(format!("asset-{id}")), i), "")
                                    .tab_index(0_isize)
                                    .width(px(128.))
                                    .height(px(168.))
                                    .style(ButtonStyle::Outlined)
                                    .selected_style(ButtonStyle::Tinted(TintColor::Accent))
                                    .toggle_state(active == row_index(i))
                                    .aria_label(entry.name.clone())
                                    .on_click({
                                        let callback = cx.listener(move |this, _, _, cx| {
                                            picked(this, &name, cx);
                                        });
                                        move |event, window, cx| {
                                            if flush_input_registry(&inputs, window, cx) {
                                                window.focus(&focus, cx);
                                            }
                                            callback(event, window, cx);
                                        }
                                    })
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap(px(2.))
                                            .w_full()
                                            .h_full()
                                            .min_w_0()
                                            .overflow_hidden()
                                            .child(
                                                div()
                                                    .relative()
                                                    .aspect_square()
                                                    .flex_none()
                                                    .w_full()
                                                    .overflow_hidden()
                                                    .child(
                                                        div()
                                                            .absolute()
                                                            .size_full()
                                                            .children(image),
                                                    ),
                                            )
                                            .child(Label::new(entry.name).truncate())
                                            .child(
                                                Label::new(format!(
                                                    "{}×{}",
                                                    entry.tex_w, entry.tex_h
                                                ))
                                                .size(LabelSize::XSmall)
                                                .color(Color::Muted),
                                            ),
                                    ),
                            );
                        }
                        line.into_any_element()
                    })
                    .collect()
            }),
        )
        .flex_1()
        .min_h_0()
        .w_full()
        .track_scroll(&scroll)
        .into_any_element()
    }
}

#[cfg(test)]
mod tests;
