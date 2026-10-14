//! Animation sequence drafts and their native popup controls.

use super::DialogsView;
use crate::assets::EditorAssets;
use editor_core::{AnimDef, Name8};
use gpui::{AnyElement, Context, Div, Window, div, prelude::*, px};
use ui_common::ActiveTheme as _;
use ui_common::widgets::{Color, Label};

#[derive(Default)]
pub(super) struct AnimationsDraft {
    rows: Vec<SequenceDraft>,
    builtins: Vec<&'static wad::animations::AnimationDef>,
    new: SequenceDraft,
    next_id: usize,
    pub(super) error: &'static str,
}

struct SequenceDraft {
    id: usize,
    is_texture: bool,
    start: String,
    end: String,
    speed: String,
}

impl Default for SequenceDraft {
    fn default() -> Self {
        Self {
            id: 0,
            is_texture: true,
            start: String::new(),
            end: String::new(),
            speed: "8".into(),
        }
    }
}

impl SequenceDraft {
    fn parse(&self) -> Result<AnimDef, &'static str> {
        let start = Name8::from_dwd_field(self.start.trim()).map_err(|_| "Invalid start name")?;
        let end = Name8::from_dwd_field(self.end.trim()).map_err(|_| "Invalid end name")?;
        if start.is_empty() || end.is_empty() {
            return Err("Start and end names are required");
        }
        Ok(AnimDef {
            is_texture: self.is_texture,
            start,
            end,
            speed: self
                .speed
                .trim()
                .parse::<i32>()
                .ok()
                .filter(|&speed| speed > 0)
                .ok_or("Speed must be positive")?,
        })
    }
}

impl AnimationsDraft {
    pub(super) fn begin(&mut self, assets: &EditorAssets) {
        self.rows.clear();
        self.rows.reserve(assets.animations().len());
        self.rows.extend(
            assets
                .animations()
                .iter()
                .enumerate()
                .map(|(id, animation)| SequenceDraft {
                    id,
                    is_texture: animation.is_texture,
                    start: animation.start.as_str().into(),
                    end: animation.end.as_str().into(),
                    speed: animation.speed.to_string(),
                }),
        );
        self.builtins.clear();
        self.builtins.extend_from_slice(assets.vanilla_animations());
        self.next_id = self.rows.len() + 1;
        self.new = SequenceDraft {
            id: self.rows.len(),
            ..Default::default()
        };
        self.error = "";
    }

    pub(super) fn parse(&self) -> Result<Vec<AnimDef>, &'static str> {
        self.rows.iter().map(SequenceDraft::parse).collect()
    }

    fn row_mut(&mut self, id: usize) -> Option<&mut SequenceDraft> {
        if self.new.id == id {
            Some(&mut self.new)
        } else {
            self.rows.iter_mut().find(|row| row.id == id)
        }
    }

    fn add(&mut self) {
        if let Err(error) = self.new.parse() {
            self.error = error;
            return;
        }
        let new = SequenceDraft {
            id: self.next_id,
            ..Default::default()
        };
        self.next_id = self
            .next_id
            .checked_add(1)
            .expect("animation draft IDs exhausted");
        self.rows.push(std::mem::replace(&mut self.new, new));
        self.error = "";
    }
}

#[derive(Clone, Copy)]
enum Field {
    Start,
    End,
    Speed,
}

impl Field {
    fn label(self) -> &'static str {
        match self {
            Self::Start => "Start",
            Self::End => "End",
            Self::Speed => "Speed",
        }
    }
    fn value(self, row: &mut SequenceDraft) -> &mut String {
        match self {
            Self::Start => &mut row.start,
            Self::End => &mut row.end,
            Self::Speed => &mut row.speed,
        }
    }
}

impl DialogsView {
    pub(super) fn animations(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let mut controls = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(self.animation_fields(None, window, cx))
            .child(self.forms.button(
                "add-animation",
                "Add sequence",
                cx.listener(|this, _, _, cx| {
                    this.drafts.animations.add();
                    cx.notify();
                }),
            ))
            .child(div().border_t_1().border_color(cx.theme().colors().border));
        for index in 0..self.drafts.animations.rows.len() {
            let id = self.drafts.animations.rows[index].id;
            controls = controls.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_2()
                    .border_1()
                    .rounded_md()
                    .border_color(cx.theme().colors().border)
                    .child(self.animation_fields(Some(index), window, cx))
                    .child(self.forms.button(
                        ("delete-animation", id),
                        "Remove sequence",
                        cx.listener(move |this, _, _, cx| {
                            this.drafts.animations.rows.retain(|row| row.id != id);
                            cx.notify();
                        }),
                    )),
            );
        }
        for &animation in &self.drafts.animations.builtins {
            controls = controls.child(builtin_animation(animation, cx));
        }
        if !self.drafts.animations.error.is_empty() {
            controls = controls.child(Label::new(self.drafts.animations.error).color(Color::Error));
        }
        controls.into_any_element()
    }

    fn animation_fields(
        &mut self,
        index: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let row = index.map_or(&self.drafts.animations.new, |index| {
            &self.drafts.animations.rows[index]
        });
        let id = row.id;
        let selected = row.is_texture;
        let values = [row.start.as_str(), row.end.as_str(), row.speed.as_str()];
        let kind = self.forms.typed_choices(
            ("animation-kind", id),
            [(true, "Wall texture"), (false, "Flat")],
            Some(&selected),
            cx.listener(move |this, value: &bool, _, cx| {
                if let Some(row) = this.drafts.animations.row_mut(id) {
                    row.is_texture = *value;
                }
                cx.notify();
            }),
            window,
            cx,
        );
        let mut fields = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(readonly_field("Type", kind));
        for (field, value) in [Field::Start, Field::End, Field::Speed]
            .into_iter()
            .zip(values)
        {
            fields = fields.child(self.forms.field(
                format!("animation-{id}-{}", field.label()),
                field.label(),
                value,
                cx.listener(move |this, value: &String, _, cx| {
                    if let Some(row) = this.drafts.animations.row_mut(id) {
                        field.value(row).clone_from(value);
                    }
                    cx.notify();
                }),
                |_, _, _| {},
                window,
                cx,
            ));
        }
        fields
    }
}

fn builtin_animation(
    animation: &'static wad::animations::AnimationDef,
    cx: &Context<DialogsView>,
) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .p_2()
        .border_1()
        .rounded_md()
        .border_color(cx.theme().colors().border)
        .child(Label::new("Built-in").color(Color::Muted))
        .child(readonly_field(
            "Type",
            Label::new(if animation.is_texture {
                "Wall texture"
            } else {
                "Flat"
            }),
        ))
        .child(readonly_field("Start", Label::new(animation.start_name)))
        .child(readonly_field("End", Label::new(animation.end_name)))
        .child(readonly_field(
            "Speed",
            Label::new(animation.speed.to_string()),
        ))
}

fn readonly_field(label: &'static str, value: impl IntoElement) -> Div {
    div()
        .flex()
        .items_center()
        .gap_2()
        .child(div().w(px(140.)).flex_shrink_0().child(Label::new(label)))
        .child(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_frame_names_cannot_be_added_or_applied() {
        let mut draft = AnimationsDraft::default();
        for (start, end) in [("", "FRAME3"), ("FRAME1", " "), ("-", "FRAME3")] {
            draft.new = SequenceDraft {
                start: start.into(),
                end: end.into(),
                ..Default::default()
            };
            draft.add();
            assert!(
                draft.rows.is_empty(),
                "invalid ranges must not enter the list"
            );
            draft.rows.push(std::mem::take(&mut draft.new));
            assert!(
                draft.parse().is_err(),
                "invalid existing ranges must block Apply"
            );
            draft.rows.clear();
        }
    }
}
