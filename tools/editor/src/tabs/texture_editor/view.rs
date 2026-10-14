//! Native controls and layout for the texture editor tab.

use editor_core::PatchPlacement;
use gpui::{
    Anchor, AnyElement, AppContext as _, Context, Div, ElementId, FontWeight, ListSizingBehavior,
    Pixels, Render, SharedString, Window, div, point, prelude::*, px, uniform_list,
};
use ui_common::ActiveTheme as _;
use ui_common::widgets::{
    Button, ButtonStyle, ContextMenu, DropdownMenu, IconName, IconPosition, Label, ListItem,
    Tooltip, prelude::*,
};

use super::{EditAction, Input, TextureEditor, TextureEditorEvent};

const PATCH_TILE_HEIGHT: f32 = 112.0;
const SETTINGS_COLUMN_WIDTH: f32 = 240.0;

impl TextureEditor {
    pub(super) fn button(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        cx: &Context<Self>,
        action: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> Button {
        Button::new(id, label)
            .style(ButtonStyle::Filled)
            .tab_index(0_isize)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.flush_inputs(window, cx);
                this.focus(window, cx);
                action(this, window, cx);
            }))
    }

    pub(super) fn field(
        &mut self,
        key: Input,
        label: &'static str,
        value: impl AsRef<str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.field_with_label_width(key, label, value, px(80.), window, cx)
    }

    fn tile_field(
        &mut self,
        key: Input,
        label: &'static str,
        value: impl AsRef<str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.field_with_label_width(key, label, value, px(36.), window, cx)
    }

    fn field_with_label_width(
        &mut self,
        key: Input,
        label: &'static str,
        value: impl AsRef<str>,
        label_width: Pixels,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let weak = cx.weak_entity();
        let input = self
            .inputs
            .entry(key)
            .or_insert_with(|| {
                cx.new(|cx| {
                    let changes = weak.clone();
                    ui_common::TextInput::new(value.as_ref().to_owned(), window, cx)
                        .on_change(move |value, _, cx| {
                            changes
                                .update(cx, |this, _| {
                                    this.drafts.insert(key, value.to_owned());
                                })
                                .ok();
                        })
                        .on_commit(move |reason, window, cx| {
                            let weak = weak.clone();
                            window.defer(cx, move |window, cx| {
                                weak.update(cx, |this, cx| {
                                    this.flush_inputs(window, cx);
                                    if matches!(reason, ui_common::InputCommit::Submit) {
                                        if key == Input::NewPatch {
                                            this.add_patch(cx);
                                        }
                                        this.focus(window, cx);
                                    }
                                })
                                .ok();
                            });
                        })
                })
            })
            .clone();
        input.update(cx, |input, cx| input.set_text(value.as_ref(), window, cx));
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap_2()
            .child(
                div()
                    .w(label_width)
                    .flex_shrink_0()
                    .whitespace_nowrap()
                    .child(Label::new(label)),
            )
            .child(div().flex_1().min_w_0().child(input))
            .into_any_element()
    }

    fn colormap_menu(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let selected = self.colormap;
        let weak = cx.weak_entity();
        let menu = ContextMenu::build(window, cx, move |mut menu, _, _| {
            for level in std::iter::once(None).chain((0..32).map(Some)) {
                let label =
                    level.map_or_else(|| "Full bright".into(), |n| format!("Light level {n}"));
                let weak = weak.clone();
                menu = menu.toggleable_entry(
                    label,
                    level == selected,
                    IconPosition::Start,
                    None,
                    move |window, cx| {
                        weak.update(cx, |this, cx| {
                            this.flush_inputs(window, cx);
                            this.colormap = level;
                            this.focus(window, cx);
                            this.refresh_preview(cx);
                        })
                        .ok();
                    },
                );
            }
            menu
        });
        let label = selected.map_or_else(|| "Full bright".into(), |n| format!("Light level {n}"));
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(div().w(px(80.0)).child(Label::new("Colormap")))
            .child(
                DropdownMenu::new("texture-colormap", label, menu)
                    .attach(Anchor::BottomLeft)
                    .offset(point(px(0.0), px(0.0)))
                    .tab_index(0),
            )
            .into_any_element()
    }

    fn texture_list(&self, cx: &Context<Self>) -> AnyElement {
        let (title, count) = {
            let state = self.shared.borrow();
            state.assets.as_ref().map_or_else(
                || ("No WAD loaded".into(), 0),
                |assets| {
                    (
                        assets
                            .group_label(assets.active_group())
                            .unwrap_or_default(),
                        assets.textures().len(),
                    )
                },
            )
        };
        div()
            .id("texture-list-scroll")
            .flex()
            .flex_col()
            .flex_shrink_0()
            .h_full()
            .min_h_0()
            .gap_2()
            .p_3()
            .border_1()
            .rounded_md()
            .border_color(cx.theme().colors().border)
            .child(heading(title))
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .gap_1()
                    .child(Self::button("new-texture", "New", cx, |this, _, cx| {
                        this.new_texture(false, cx);
                    }))
                    .child(Self::button(
                        "duplicate-texture",
                        "Duplicate",
                        cx,
                        |this, _, cx| {
                            this.new_texture(true, cx);
                        },
                    ))
                    .child(Self::button(
                        "delete-texture",
                        "Delete",
                        cx,
                        |this, _, cx| {
                            this.delete_texture(cx);
                        },
                    )),
            )
            .child(Self::button(
                "open-animations",
                "Animations…",
                cx,
                |_, _, cx| {
                    cx.emit(TextureEditorEvent::Animations);
                },
            ))
            .child(
                uniform_list(
                    "texture-list",
                    count,
                    cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                        range
                            .filter_map(|index| this.texture_row(index, cx))
                            .collect()
                    }),
                )
                .with_sizing_behavior(ListSizingBehavior::Infer)
                .flex_1()
                .min_h_0()
                .track_scroll(&self.texture_scroll),
            )
            .into_any_element()
    }

    fn texture_row(&self, index: usize, cx: &Context<Self>) -> Option<AnyElement> {
        let state = self.shared.borrow();
        let def = state.assets.as_ref()?.textures().get(index)?;
        Some(
            ListItem::new(("texture", index))
                .height(px(32.))
                .toggle_state(self.current == Some(index))
                .child(Label::new(def.name.as_str().to_owned()).truncate())
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.flush_inputs(window, cx);
                    this.focus(window, cx);
                    this.select(Some(index), cx);
                }))
                .into_any_element(),
        )
    }

    fn patch_controls(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let shared = std::rc::Rc::clone(&self.shared);
        let state = shared.borrow();
        let patches = self
            .current
            .and_then(|index| state.assets.as_ref()?.textures().get(index))
            .map_or(&[][..], |def| def.patches.as_slice());
        let mut grid = div().flex().flex_wrap().flex_none().w_full().gap_2();
        for (index, patch) in patches.iter().copied().enumerate() {
            grid = grid.child(self.patch_tile(index, patch, window, cx));
        }
        let patch_name = self.field(Input::NewPatch, "Patch", self.new_patch.clone(), window, cx);
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(patch_name)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(Self::button("add-patch", "Add patch", cx, |this, _, cx| {
                        this.add_patch(cx);
                    }))
                    .child(Self::button(
                        "browse-patches",
                        "Browse…",
                        cx,
                        |_, _, cx| {
                            cx.emit(TextureEditorEvent::BrowsePatches);
                        },
                    ))
                    .child(Self::button(
                        "import-patch",
                        "Import PNG…",
                        cx,
                        |this, _, cx| {
                            this.import_patch(cx);
                        },
                    )),
            )
            .child(div().border_t_1().border_color(cx.theme().colors().border))
            .child(grid)
            .into_any_element()
    }

    fn patch_tile(
        &mut self,
        index: usize,
        patch: PatchPlacement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let x = self.tile_field(
            Input::PatchX(index),
            "X",
            patch.origin_x.to_string(),
            window,
            cx,
        );
        let y = self.tile_field(
            Input::PatchY(index),
            "Y",
            patch.origin_y.to_string(),
            window,
            cx,
        );
        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .w(px(PATCH_TILE_HEIGHT * 2.))
            .h(px(PATCH_TILE_HEIGHT))
            .gap_1()
            .border_1()
            .rounded_md()
            .p_2()
            .border_color(if self.selected_patch == Some(index) {
                cx.theme().colors().border_focused
            } else {
                cx.theme().colors().border
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .justify_between()
                    .child(
                        div().flex_1().min_w_0().child(
                            Button::new(("select-patch", index), patch.patch.as_str().to_owned())
                                .full_width()
                                .truncate(true)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.flush_inputs(window, cx);
                                    this.focus(window, cx);
                                    this.selected_patch = Some(index);
                                    this.refresh_preview(cx);
                                })),
                        ),
                    )
                    .child(
                        Button::new(("delete-patch", index), "")
                            .start_icon(ui_common::Icon::new(IconName::Trash))
                            .aria_label("Remove patch")
                            .tooltip(Tooltip::text("Remove patch"))
                            .tab_index(0_isize)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.flush_inputs(window, cx);
                                this.focus(window, cx);
                                this.delete_patch(index, cx);
                            })),
                    ),
            )
            .child(x)
            .child(y)
            .into_any_element()
    }

    fn editor_columns(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let [texture_width, texture_height] = {
            let state = self.shared.borrow();
            self.current
                .and_then(|index| state.assets.as_ref()?.textures().get(index))
                .map(|def| [def.width.to_string(), def.height.to_string()])
                .unwrap_or_default()
        };
        let width = self.field(Input::Width, "Width", texture_width, window, cx);
        let height = self.field(Input::Height, "Height", texture_height, window, cx);
        let colormap = self.colormap_menu(window, cx);
        let patches = self.patch_controls(window, cx);
        div()
            .id("texture-columns-scroll")
            .flex()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .gap_3()
            .overflow_x_scroll()
            .child(
                column(
                    "texture-properties-scroll",
                    "Texture properties",
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(colormap)
                        .child(width)
                        .child(height)
                        .child(Label::new(self.status.clone())),
                    cx,
                )
                .w(px(SETTINGS_COLUMN_WIDTH))
                .flex_shrink_0(),
            )
            .child(
                column("texture-patches-scroll", "Patches", patches, cx)
                    .flex_1()
                    .min_w(px(256.)),
            )
            .into_any_element()
    }
}

impl Render for TextureEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.retire_previews(window);
        let columns = self.editor_columns(window, cx);
        div()
            .id("texture-editor")
            .key_context("TextureEditor")
            .track_focus(&self.focus)
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .text_color(cx.theme().colors().text)
            .on_action(cx.listener(Self::nudge))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .items_center()
                    .p_2()
                    .child(heading("Texture editor"))
                    .child(Self::button(
                        "undo-texture-edit",
                        "Undo",
                        cx,
                        |this, window, cx| {
                            this.edit_action(EditAction::Undo, window, cx);
                        },
                    ))
                    .child(Self::button(
                        "redo-texture-edit",
                        "Redo",
                        cx,
                        |this, window, cx| {
                            this.edit_action(EditAction::Redo, window, cx);
                        },
                    ))
                    .child(Self::button(
                        "save-texture-project",
                        "Save project",
                        cx,
                        |_, _, cx| {
                            cx.emit(TextureEditorEvent::SaveProject);
                        },
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .gap_3()
                    .p_3()
                    .child(self.texture_list(cx))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .gap_3()
                            .child(
                                section("Texture preview", cx)
                                    .flex_none()
                                    .w_full()
                                    .child(self.preview_view(cx)),
                            )
                            .child(columns),
                    ),
            )
    }
}

fn section(title: &'static str, cx: &Context<TextureEditor>) -> Div {
    div()
        .flex()
        .flex_col()
        .min_h_0()
        .gap_2()
        .p_3()
        .border_1()
        .rounded_md()
        .border_color(cx.theme().colors().border)
        .child(
            div()
                .flex_none()
                .font_weight(FontWeight::SEMIBOLD)
                .child(Label::new(title)),
        )
}

fn column(
    id: &'static str,
    title: &'static str,
    content: impl IntoElement,
    cx: &Context<TextureEditor>,
) -> Div {
    section(title, cx).h_full().child(
        div()
            .id(id)
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .child(content),
    )
}

pub(super) fn heading(label: impl Into<SharedString>) -> Div {
    div()
        .font_weight(FontWeight::SEMIBOLD)
        .mt_2()
        .child(label.into())
}
