//! Linedef drafts and batch editing through the owning map entity.

use super::{InspectorField, MapInspector, SideField};
use crate::boundary::TexSlot;
use crate::components::forms::heading;
use crate::components::types::{LineFlags, LineSide, LineTouched};
use crate::dialogs::asset_picker::actions::TexBrowseTarget;
use crate::tabs::map_editor::MapCommand;
use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*, px};
use std::rc::Rc;
use ui_common::ActiveTheme as _;
use ui_common::widgets::{Label, Toggleable as _};

#[derive(Clone)]
pub(crate) struct LineInspectorData {
    pub(crate) active: bool,
    pub(crate) line_index: i32,
    pub(crate) flags: LineFlags,
    pub(crate) special: String,
    pub(crate) specials: Rc<[(i32, SharedString)]>,
    pub(crate) tag: String,
    pub(crate) front: LineSide,
    pub(crate) has_back: bool,
    pub(crate) back: LineSide,
    pub(crate) touched: LineTouched,
}

impl Default for LineInspectorData {
    fn default() -> Self {
        Self {
            active: false,
            line_index: -1,
            flags: Default::default(),
            special: Default::default(),
            specials: Default::default(),
            tag: Default::default(),
            front: Default::default(),
            has_back: false,
            back: Default::default(),
            touched: Default::default(),
        }
    }
}

const LABEL_WIDTH: f32 = 60.;

impl MapInspector {
    #[expect(
        clippy::too_many_lines,
        reason = "flags and both sidedef drafts share a batch edit transaction"
    )]
    pub(super) fn line_inspector(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = &self.data.line;
        let mut panel = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(heading(format!("Line {}", c.line_index)));
        let mut flags = div().flex().flex_wrap().gap_1();
        macro_rules! flag {
            ($name:ident,$label:literal) => {
                flags = flags.child(
                    self.forms
                        .button(
                            concat!("line-", stringify!($name)),
                            $label,
                            cx.listener(|this, _, _, cx| {
                                let flags = &mut this.data.line.flags;
                                flags.$name = !flags.$name;
                                this.data.line.touched.$name = true;
                                Self::command(MapCommand::LineApply, cx);
                            }),
                        )
                        .toggle_state(c.flags.$name),
                );
            };
        }
        flag!(blocks, "Impassable");
        flag!(block_monsters, "Block monsters");
        flag!(two_sided, "Two sided");
        flag!(upper_unpeg, "Upper unpegged");
        flag!(lower_unpeg, "Lower unpegged");
        flag!(secret, "Secret");
        flag!(block_sound, "Block sound");
        flag!(hidden, "Hidden");
        panel = panel.child(flags);
        let mut specials = div()
            .flex()
            .flex_col()
            .gap_2()
            .p_2()
            .border_1()
            .border_color(cx.theme().colors().border)
            .rounded_sm();
        macro_rules! text {
            ($field:ident,$key:ident,$label:literal) => {
                let field =
                    self.fields
                        .field(InspectorField::$key, "", &c.$field, &self.forms, window, cx);
                specials = specials.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .w(px(LABEL_WIDTH))
                                .flex_shrink_0()
                                .child(Label::new($label)),
                        )
                        .child(div().flex_1().min_w_0().child(field)),
                );
            };
        }
        text!(special, LineSpecial, "Special");
        specials = specials.child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .w(px(LABEL_WIDTH))
                        .flex_shrink_0()
                        .child(Label::new("Preset:")),
                )
                .child(
                    div().flex_1().min_w_0().child(
                        self.forms
                            .typed_choices(
                                "line-special-choice",
                                c.specials.clone(),
                                c.special.trim().parse::<i32>().ok().as_ref(),
                                cx.listener(|this, value: &i32, _, cx| {
                                    this.data.line.special = value.to_string();
                                    this.data.line.touched.special = true;
                                    Self::command(MapCommand::LineApply, cx);
                                }),
                                window,
                                cx,
                            )
                            .full_width(true),
                    ),
                ),
        );
        text!(tag, LineTag, "Tag");
        panel = panel.child(specials);
        for back in [false, true] {
            if back && !c.has_back {
                continue;
            }
            panel = panel.child(heading(if back { "Back" } else { "Front" }));
            let side = if back { &c.back } else { &c.front };
            let mut offsets = div().flex().items_center().gap_2().flex_1().min_w_0();
            for (index, (label, value, slot)) in [
                (
                    "Upper",
                    &side.top,
                    if back {
                        TexSlot::BackTop
                    } else {
                        TexSlot::FrontTop
                    },
                ),
                (
                    "Middle",
                    &side.mid,
                    if back {
                        TexSlot::BackMid
                    } else {
                        TexSlot::FrontMid
                    },
                ),
                (
                    "Lower",
                    &side.bottom,
                    if back {
                        TexSlot::BackBottom
                    } else {
                        TexSlot::FrontBottom
                    },
                ),
                (
                    "X offset",
                    &side.xoff,
                    if back {
                        TexSlot::BackMid
                    } else {
                        TexSlot::FrontMid
                    },
                ),
                (
                    "Y offset",
                    &side.yoff,
                    if back {
                        TexSlot::BackMid
                    } else {
                        TexSlot::FrontMid
                    },
                ),
            ]
            .into_iter()
            .enumerate()
            {
                let part = [
                    SideField::Top,
                    SideField::Middle,
                    SideField::Bottom,
                    SideField::X,
                    SideField::Y,
                ][index];
                let field = self.fields.field(
                    InspectorField::LineSide {
                        back,
                        part,
                    },
                    "",
                    value,
                    &self.forms,
                    window,
                    cx,
                );
                if index < 3 {
                    panel = panel.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(
                                div()
                                    .w(px(LABEL_WIDTH))
                                    .flex_shrink_0()
                                    .child(Label::new(label)),
                            )
                            .child(div().flex_1().min_w_0().child(field))
                            .child(self.forms.button(
                                (if back { "browse-back" } else { "browse-front" }, index),
                                "…",
                                cx.listener(move |_, _, _, cx| {
                                    Self::browse(TexBrowseTarget::LineSlot(slot), cx);
                                }),
                            )),
                    );
                } else {
                    offsets = offsets
                        .child(Label::new(if index == 3 { "X" } else { "Y" }))
                        .child(div().flex_1().min_w_0().child(field));
                }
            }
            panel = panel.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .w(px(LABEL_WIDTH))
                            .flex_shrink_0()
                            .child(Label::new("Offsets")),
                    )
                    .child(offsets),
            );
        }
        panel
            .child(self.forms.button(
                "flip-line",
                "Flip line",
                cx.listener(|_, _, _, cx| Self::command(MapCommand::LineFlip, cx)),
            ))
            .into_any_element()
    }
}
