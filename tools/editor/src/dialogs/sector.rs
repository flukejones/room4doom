//! Floor and ceiling previews, flat selection and heights.

pub(crate) mod actions;

use super::*;
use crate::components::types::{FlatSlot, GfxEntry};
use crate::gfx::image::UiImage;

#[derive(Clone)]
pub(crate) struct SectorDraft {
    pub(crate) sector_index: i32,
    pub(crate) ceil_img: UiImage,
    pub(crate) floor_img: UiImage,
    pub(crate) selected_slot: FlatSlot,
    pub(crate) entries: Vec<GfxEntry>,
    pub(crate) active_index: i32,
    pub(crate) floor_h: i32,
    pub(crate) ceil_h: i32,
}

impl Default for SectorDraft {
    fn default() -> Self {
        Self {
            sector_index: -1,
            ceil_img: Default::default(),
            floor_img: Default::default(),
            selected_slot: Default::default(),
            entries: Default::default(),
            active_index: -1,
            floor_h: Default::default(),
            ceil_h: Default::default(),
        }
    }
}

impl DialogsView {
    pub(super) fn sector_editor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let previews = self.sector_previews(window, cx);
        let grid = self.asset_grid(
            "sector",
            |this| &this.drafts.sector.entries,
            self.drafts.sector.active_index,
            self.popup_geom(PopupId::SectorEdit).w - 256.,
            cx,
            Self::pick_sector_flat,
        );
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap_2()
            .child(self.forms.segmented(
                "sector-slot",
                ["Floor", "Ceiling"],
                usize::from(self.drafts.sector.selected_slot == FlatSlot::Ceil),
                cx.listener(|this, index: &usize, _, cx| {
                    actions::select_flat(
                        &mut this.drafts,
                        if *index == 0 {
                            FlatSlot::Floor
                        } else {
                            FlatSlot::Ceil
                        },
                    );
                    cx.notify();
                }),
            ))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .gap_3()
                    .child(previews)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .child(grid),
                    ),
            )
            .into_any_element()
    }

    fn sector_previews(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let draft = &self.drafts.sector;
        let mut previews = div()
            .id("sector-previews")
            .flex()
            .flex_col()
            .gap_2()
            .w(px(220.))
            .flex_shrink_0()
            .h_full()
            .overflow_y_scroll();
        for (label, slot, image, height) in [
            (
                "Ceiling",
                FlatSlot::Ceil,
                draft.ceil_img.clone(),
                draft.ceil_h,
            ),
            (
                "Floor",
                FlatSlot::Floor,
                draft.floor_img.clone(),
                draft.floor_h,
            ),
        ] {
            let image = self.forms.image(format!("sector-preview-{slot:?}"), image);
            let inputs = self.forms.inputs.clone();
            let focus = self.forms.focus.clone();
            let callback = cx.listener(move |this, _, _, cx| {
                actions::select_flat(&mut this.drafts, slot);
                cx.notify();
            });
            previews = previews
                .child(
                    Button::new(SharedString::from(format!("sector-slot-{slot:?}")), "")
                        .tab_index(0_isize)
                        .full_width()
                        .height(px(156.))
                        .style(ButtonStyle::Outlined)
                        .selected_style(ButtonStyle::Tinted(TintColor::Accent))
                        .toggle_state(draft.selected_slot == slot)
                        .aria_label(label)
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .w_full()
                                .gap_1()
                                .child(div().h(px(128.)).children(image))
                                .child(Label::new(label)),
                        )
                        .on_click(move |event, window, cx| {
                            if flush_input_registry(&inputs, window, cx) {
                                window.focus(&focus, cx);
                            }
                            callback(event, window, cx);
                        }),
                )
                .child(self.forms.field(
                    format!("sector-height-{slot:?}"),
                    "Height",
                    height.to_string(),
                    cx.listener(move |this, value: &String, _, cx| {
                        if let Ok(n) = value.parse() {
                            if slot == FlatSlot::Floor {
                                this.drafts.sector.floor_h = n;
                            } else {
                                this.drafts.sector.ceil_h = n;
                            }
                            let damage = actions::height_changed(
                                &mut this.drafts,
                                &mut this.shared.borrow_mut(),
                            );
                            Self::emit_damage(damage, cx);
                        }
                    }),
                    |_, _, _| {},
                    window,
                    cx,
                ));
        }
        previews.into_any_element()
    }
    fn pick_sector_flat(&mut self, name: &str, cx: &mut Context<Self>) {
        let damage = actions::pick_flat(&mut self.drafts, &mut self.shared.borrow_mut(), name);
        Self::emit_damage(damage, cx);
    }
}
