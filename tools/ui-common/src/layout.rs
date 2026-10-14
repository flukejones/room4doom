//! Tab strips and dialog content layout. Native windows own resizing and titlebars.

use crate::{
    ActiveTheme as _,
    widgets::{Button, ButtonStyle, Clickable, Toggleable},
};
use gpui::{
    AnyElement, App, ClickEvent, Div, ElementId, IntoElement, RenderOnce, ScrollHandle, Stateful,
    Window, div, prelude::*, px,
};

#[derive(Clone, Copy)]
pub enum TabPosition {
    First,
    Middle,
    Last,
}
#[derive(IntoElement)]
pub struct Tab {
    button: Button,
    selected: bool,
    position: TabPosition,
}
impl Tab {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            button: Button::new(id, "")
                .px_3()
                .height(px(28.))
                .selected_style(ButtonStyle::Filled)
                .aria_role(gpui::Role::Tab),
            selected: false,
            position: TabPosition::Middle,
        }
    }
    pub fn position(mut self, position: TabPosition) -> Self {
        self.position = position;
        self
    }
    pub fn debug_selector(mut self, selector: impl FnOnce() -> String) -> Self {
        self.button = self.button.debug_selector(selector);
        self
    }
}
impl ParentElement for Tab {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.button.extend(elements);
    }
}
impl Toggleable for Tab {
    fn toggle_state(mut self, selected: bool) -> Self {
        self.selected = selected;
        self.button = self.button.toggle_state(selected);
        self
    }
}
impl Clickable for Tab {
    fn on_click(mut self, callback: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.button = self.button.on_click(callback);
        self
    }
    fn cursor_style(mut self, style: gpui::CursorStyle) -> Self {
        self.button = self.button.cursor_style(style);
        self
    }
}
impl RenderOnce for Tab {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        self.button
            .border_b_2()
            .border_color(if self.selected {
                cx.theme().colors().text_accent
            } else {
                cx.theme().colors().border
            })
            .when(matches!(self.position, TabPosition::First), |button| {
                button.rounded_tl_sm()
            })
            .when(matches!(self.position, TabPosition::Last), |button| {
                button.rounded_tr_sm()
            })
    }
}
#[derive(IntoElement)]
pub struct TabBar(Stateful<Div>);
impl TabBar {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self(
            div()
                .id(id)
                .role(gpui::Role::TabList)
                .flex()
                .flex_shrink_0()
                .h(px(28.))
                .w_full(),
        )
    }
}
impl ParentElement for TabBar {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.0.extend(elements);
    }
}
impl RenderOnce for TabBar {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        self.0
    }
}

#[derive(IntoElement)]
pub struct Section(Div);
impl Default for Section {
    fn default() -> Self {
        Self::new()
    }
}
impl Section {
    pub fn new() -> Self {
        Self(div().flex().flex_col().gap_2().p_3())
    }
}
impl ParentElement for Section {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.0.extend(elements);
    }
}
impl RenderOnce for Section {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        self.0
    }
}

#[derive(IntoElement)]
pub struct ModalFooter {
    end: Option<AnyElement>,
}
impl Default for ModalFooter {
    fn default() -> Self {
        Self::new()
    }
}
impl ModalFooter {
    pub fn new() -> Self {
        Self {
            end: None,
        }
    }
    pub fn end_slot(mut self, end: impl IntoElement) -> Self {
        self.end = Some(end.into_any_element());
        self
    }
}
impl RenderOnce for ModalFooter {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .flex()
            .flex_shrink_0()
            .justify_end()
            .p_2()
            .border_t_1()
            .border_color(cx.theme().colors().border)
            .children(self.end)
    }
}

#[derive(IntoElement)]
pub struct Modal {
    root: Stateful<Div>,
    body: Stateful<Div>,
    scroll: Option<ScrollHandle>,
    footer: Option<ModalFooter>,
}
impl Modal {
    pub fn new(id: impl Into<ElementId>, scroll: Option<ScrollHandle>) -> Self {
        Self {
            root: div()
                .id(id)
                .size_full()
                .flex()
                .flex_col()
                .min_h_0()
                .overflow_hidden(),
            body: div()
                .id("dialog-content")
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .w_full(),
            scroll,
            footer: None,
        }
    }
    pub fn footer(mut self, footer: ModalFooter) -> Self {
        self.footer = Some(footer);
        self
    }
}
impl ParentElement for Modal {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.body.extend(elements);
    }
}
impl RenderOnce for Modal {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let mut body = self.body;
        if let Some(scroll) = self.scroll {
            body = body.overflow_y_scroll().track_scroll(&scroll);
        }
        self.root.child(body).children(self.footer)
    }
}
