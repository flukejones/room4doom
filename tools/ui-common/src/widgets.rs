//! Compact application controls built directly on Apache-licensed GPUI primitives.

use crate::ActiveTheme as _;
pub use crate::icons::{Icon, IconName, IconSize};
pub use crate::layout::{Modal, ModalFooter, Section, Tab, TabBar, TabPosition};
pub use crate::menus::{
    ContextMenu, DropdownMenu, IconPosition, PopoverMenu, PopoverMenuHandle, right_click_menu,
};
use gpui::{
    App, ClickEvent, CursorStyle, Div, ElementId, Hsla, IntoElement, Pixels, RenderOnce, Role,
    SharedString, Stateful, StyleRefinement, Window, div, prelude::*, px,
};
use std::rc::Rc;

pub mod prelude {
    pub use super::{Clickable, Toggleable};
}

#[derive(Clone, Copy, Default)]
pub enum Color {
    #[default]
    Default,
    Muted,
    Error,
    Custom(Hsla),
}
impl Color {
    pub fn resolve(self, cx: &App) -> Hsla {
        let c = cx.theme().colors();
        match self {
            Self::Default => c.text,
            Self::Muted => c.text_muted,
            Self::Error => c.error,
            Self::Custom(c) => c,
        }
    }
}

#[derive(Clone, Copy)]
pub enum TintColor {
    Accent,
}
#[derive(Clone, Copy, Default)]
pub enum ButtonStyle {
    #[default]
    Subtle,
    Transparent,
    Filled,
    Outlined,
    Tinted(TintColor),
}
#[derive(Clone, Copy)]
pub enum LabelSize {
    XSmall,
    Small,
}

#[derive(IntoElement)]
pub struct Label {
    root: Div,
    text: SharedString,
    color: Color,
}
impl Label {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            root: div().text_size(px(12.)),
            text: text.into(),
            color: Color::Default,
        }
    }
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
    pub fn size(mut self, size: LabelSize) -> Self {
        self.root = self.root.text_size(px(match size {
            LabelSize::XSmall => 10.,
            LabelSize::Small => 12.,
        }));
        self
    }
    pub fn truncate(mut self) -> Self {
        self.root = self.root.truncate();
        self
    }
    pub fn single_line(mut self) -> Self {
        self.root = self.root.whitespace_nowrap();
        self
    }
}
impl Styled for Label {
    fn style(&mut self) -> &mut StyleRefinement {
        self.root.style()
    }
}
impl RenderOnce for Label {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        self.root
            .text_color(self.color.resolve(cx))
            .child(self.text)
    }
}

pub trait Clickable: Sized {
    fn on_click(self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self;
    fn cursor_style(self, style: CursorStyle) -> Self;
}
pub trait Toggleable: Sized {
    fn toggle_state(self, selected: bool) -> Self;
}

type Click = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct Button {
    root: Stateful<Div>,
    label: Option<SharedString>,
    start: Option<Icon>,
    end: Option<Icon>,
    style: ButtonStyle,
    selected_style: Option<ButtonStyle>,
    selected: bool,
    disabled: bool,
    color: Color,
    selected_color: Option<Color>,
    click: Option<Click>,
}
impl Button {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        let label = label.into();
        Self {
            root: div()
                .text_size(px(12.))
                .id(id)
                .role(Role::Button)
                .tab_index(0)
                .flex()
                .items_center()
                .justify_start()
                .gap_1()
                .px_1()
                .h(px(24.))
                .flex_shrink_0()
                .rounded_sm(),
            label: (!label.is_empty()).then_some(label),
            start: None,
            end: None,
            style: ButtonStyle::Subtle,
            selected_style: None,
            selected: false,
            disabled: false,
            color: Color::Default,
            selected_color: None,
            click: None,
        }
    }
    pub fn style(mut self, style: ButtonStyle) -> Self {
        self.style = style;
        self
    }
    pub fn selected_style(mut self, style: ButtonStyle) -> Self {
        self.selected_style = Some(style);
        self
    }
    pub fn selected_label_color(mut self, color: Color) -> Self {
        self.selected_color = Some(color);
        self
    }
    pub fn label_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
    pub fn start_icon(mut self, icon: Icon) -> Self {
        self.start = Some(icon);
        self
    }
    pub fn end_icon(mut self, icon: Icon) -> Self {
        self.end = Some(icon);
        self
    }
    pub fn width(mut self, width: Pixels) -> Self {
        self.root = self.root.w(width);
        self
    }
    pub fn height(mut self, height: Pixels) -> Self {
        self.root = self.root.h(height);
        self
    }
    pub fn full_width(mut self) -> Self {
        self.root = self.root.w_full();
        self
    }
    pub fn truncate(mut self, truncate: bool) -> Self {
        if truncate {
            self.root = self.root.overflow_hidden();
        }
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn tab_index(mut self, index: isize) -> Self {
        self.root = self.root.tab_index(index);
        self
    }
    pub fn debug_selector(mut self, selector: impl FnOnce() -> String) -> Self {
        self.root = self.root.debug_selector(selector);
        self
    }
    pub fn aria_label(mut self, label: impl Into<SharedString>) -> Self {
        self.root = self.root.aria_label(label);
        self
    }
    pub fn aria_expanded(mut self, expanded: bool) -> Self {
        self.root = self.root.aria_expanded(expanded);
        self
    }
    pub fn aria_value(mut self, value: impl Into<SharedString>) -> Self {
        self.root = self.root.aria_value(value);
        self
    }
    pub fn aria_role(mut self, role: Role) -> Self {
        self.root = self.root.role(role);
        self
    }
    pub fn on_a11y_action(
        mut self,
        action: gpui::accesskit::Action,
        callback: impl FnMut(Option<&gpui::accesskit::ActionData>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.root = self.root.on_a11y_action(action, callback);
        self
    }
    pub fn tooltip(
        mut self,
        tooltip: impl Fn(&mut Window, &mut App) -> gpui::AnyView + 'static,
    ) -> Self {
        self.root = self.root.tooltip(tooltip);
        self
    }
}
impl Styled for Button {
    fn style(&mut self) -> &mut StyleRefinement {
        self.root.style()
    }
}
impl ParentElement for Button {
    fn extend(&mut self, elements: impl IntoIterator<Item = gpui::AnyElement>) {
        self.root.extend(elements);
    }
}
impl Clickable for Button {
    fn on_click(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.click = Some(Rc::new(handler));
        self
    }
    fn cursor_style(mut self, style: CursorStyle) -> Self {
        self.root = self.root.cursor(style);
        self
    }
}
impl Toggleable for Button {
    fn toggle_state(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}
impl RenderOnce for Button {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let c = cx.theme().colors();
        let style = if self.selected {
            self.selected_style
                .unwrap_or(ButtonStyle::Tinted(TintColor::Accent))
        } else {
            self.style
        };
        let accent = matches!(style, ButtonStyle::Tinted(_));
        let foreground = if self.disabled {
            c.text_disabled
        } else if self.selected {
            self.selected_color.map_or_else(
                || {
                    if accent {
                        crate::accent_foreground(cx)
                    } else {
                        self.color.resolve(cx)
                    }
                },
                |color| color.resolve(cx),
            )
        } else {
            self.color.resolve(cx)
        };
        let mut root = self
            .root
            .text_color(foreground)
            .when(self.disabled, |root| root.tab_stop(false));
        root = match style {
            ButtonStyle::Subtle | ButtonStyle::Transparent => root,
            ButtonStyle::Filled => root.bg(c.element_background),
            ButtonStyle::Outlined => root.border_1().border_color(c.border),
            ButtonStyle::Tinted(_) => root.bg(c.text_accent),
        };
        if self.disabled {
            root = root.bg(c.element_disabled);
        } else {
            let hover = if accent {
                c.text_accent.opacity(0.85)
            } else {
                c.element_hover
            };
            root = root
                .cursor_pointer()
                .when(!matches!(style, ButtonStyle::Transparent), |root| {
                    root.hover(move |style| style.bg(hover))
                });
            if let Some(click) = self.click {
                root = root.on_click(move |event, window, cx| click(event, window, cx));
            }
        }
        if let Some(icon) = self.start {
            root = root.child(icon.color(Color::Custom(foreground)));
        }
        if let Some(label) = self.label {
            root = root.child(div().min_w_0().truncate().child(label));
        }
        if let Some(icon) = self.end {
            root = root.child(icon.color(Color::Custom(foreground)));
        }
        root
    }
}

#[derive(Clone, Copy)]
pub enum ToggleState {
    Selected,
    Unselected,
}
impl ToggleState {
    pub fn selected(self) -> bool {
        matches!(self, Self::Selected)
    }
}
impl From<bool> for ToggleState {
    fn from(value: bool) -> Self {
        if value {
            Self::Selected
        } else {
            Self::Unselected
        }
    }
}
#[derive(Clone, Copy)]
pub enum SwitchLabelPosition {
    Start,
    End,
}

#[derive(IntoElement)]
pub struct Toggle {
    button: Button,
    checked: bool,
    label: Option<SharedString>,
    switch: bool,
    label_position: Option<SwitchLabelPosition>,
}
impl Toggle {
    pub fn checkbox(id: impl Into<ElementId>, state: ToggleState) -> Self {
        Self {
            button: Button::new(id, ""),
            checked: state.selected(),
            label: None,
            switch: false,
            label_position: Some(SwitchLabelPosition::End),
        }
    }
    pub fn switch(id: impl Into<ElementId>, state: ToggleState) -> Self {
        Self {
            switch: true,
            ..Self::checkbox(id, state)
        }
    }
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
    pub fn label_color(mut self, color: Color) -> Self {
        self.button.color = color;
        self
    }
    pub fn label_position(mut self, position: impl Into<Option<SwitchLabelPosition>>) -> Self {
        self.label_position = position.into();
        self
    }
    pub fn tab_index(mut self, index: isize) -> Self {
        self.button = self.button.tab_index(index);
        self
    }
    pub fn fill(mut self) -> Self {
        self.button = self.button.full_width();
        self
    }
    pub fn full_width(mut self, full: bool) -> Self {
        if full {
            self.button = self.button.full_width();
        }
        self
    }
    pub fn on_click(
        mut self,
        callback: impl Fn(&ToggleState, &mut Window, &mut App) + 'static,
    ) -> Self {
        let next = ToggleState::from(!self.checked);
        self.button = self
            .button
            .on_click(move |_, window, cx| callback(&next, window, cx));
        self
    }
}
impl RenderOnce for Toggle {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let c = cx.theme().colors();
        let mut glyph = div()
            .relative()
            .flex_shrink_0()
            .w(px(if self.switch { 28. } else { 16. }))
            .h(px(16.))
            .border_1()
            .border_color(c.border)
            .bg(if self.checked {
                c.text_accent
            } else {
                c.element_background
            });
        if self.switch {
            glyph = glyph.rounded_full().child(
                div()
                    .absolute()
                    .top(px(2.))
                    .left(px(if self.checked { 15. } else { 2. }))
                    .size(px(10.))
                    .rounded_full()
                    .bg(c.text),
            );
        } else {
            glyph = glyph.rounded_sm().when(self.checked, |g| {
                g.child(
                    Icon::new(IconName::Check).color(Color::Custom(crate::accent_foreground(cx))),
                )
            });
        }
        let mut button = self.button.aria_role(if self.switch {
            Role::Switch
        } else {
            Role::CheckBox
        });
        button.root = button.root.aria_toggled(if self.checked {
            gpui::accesskit::Toggled::True
        } else {
            gpui::accesskit::Toggled::False
        });
        if matches!(self.label_position, Some(SwitchLabelPosition::Start)) {
            if let Some(label) = self.label {
                button = button.child(Label::new(label));
            }
            button.child(glyph)
        } else {
            button = button.child(glyph);
            if self.label_position.is_some()
                && let Some(label) = self.label
            {
                button = button.child(Label::new(label));
            }
            button
        }
    }
}

#[derive(IntoElement)]
pub struct ListItem {
    button: Button,
    indent: usize,
    step: Pixels,
    expanded: Option<bool>,
    toggle: Option<Click>,
    end: Option<gpui::AnyElement>,
}
impl ListItem {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            button: Button::new(id, "").full_width().height(px(32.)),
            indent: 0,
            step: px(14.),
            expanded: None,
            toggle: None,
            end: None,
        }
    }
    pub fn height(mut self, height: Pixels) -> Self {
        self.button = self.button.height(height);
        self
    }
    pub fn indent_level(mut self, indent: usize) -> Self {
        self.indent = indent;
        self
    }
    pub fn indent_step_size(mut self, step: Pixels) -> Self {
        self.step = step;
        self
    }
    pub fn aria_label(mut self, label: impl Into<SharedString>) -> Self {
        self.button = self.button.aria_label(label);
        self
    }
    pub fn start_slot(mut self, slot: impl IntoElement) -> Self {
        self.button = self.button.child(slot);
        self
    }
    pub fn end_slot(mut self, slot: impl IntoElement) -> Self {
        self.end = Some(slot.into_any_element());
        self
    }
    pub fn toggle(mut self, expanded: bool) -> Self {
        self.expanded = Some(expanded);
        self
    }
    pub fn on_toggle(
        mut self,
        callback: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.toggle = Some(Rc::new(callback));
        self
    }
}
impl Styled for ListItem {
    fn style(&mut self) -> &mut StyleRefinement {
        self.button.root.style()
    }
}
impl ParentElement for ListItem {
    fn extend(&mut self, elements: impl IntoIterator<Item = gpui::AnyElement>) {
        self.button.extend(elements);
    }
}
impl Clickable for ListItem {
    fn on_click(mut self, callback: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.button = self.button.on_click(callback);
        self
    }
    fn cursor_style(mut self, style: CursorStyle) -> Self {
        self.button = self.button.cursor_style(style);
        self
    }
}
impl Toggleable for ListItem {
    fn toggle_state(mut self, selected: bool) -> Self {
        self.button = self.button.toggle_state(selected);
        self
    }
}
impl RenderOnce for ListItem {
    fn render(mut self, _: &mut Window, _: &mut App) -> impl IntoElement {
        #[expect(
            clippy::cast_precision_loss,
            reason = "tree indentation is bounded by visible directory depth"
        )]
        let indent = self.indent as f32;
        self.button.root = self.button.root.pl(self.step * indent + px(4.));
        if let Some(expanded) = self.expanded {
            let mut arrow = Button::new("expand", "").start_icon(Icon::new(if expanded {
                IconName::ChevronDown
            } else {
                IconName::ChevronRight
            }));
            if let Some(toggle) = self.toggle {
                arrow = arrow.on_click(move |event, window, cx| {
                    cx.stop_propagation();
                    toggle(event, window, cx);
                });
            }
            self.button = self.button.child(arrow);
        }
        if let Some(end) = self.end {
            self.button = self.button.child(div().flex_1()).child(end);
        }
        self.button
    }
}

pub struct Tooltip;
struct TooltipView(SharedString);
impl Tooltip {
    pub fn text(
        text: impl Into<SharedString>,
    ) -> impl Fn(&mut Window, &mut App) -> gpui::AnyView + 'static {
        let text = text.into();
        move |_, cx| cx.new(|_| TooltipView(text.clone())).into()
    }
}
impl Render for TooltipView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .p_1()
            .rounded_sm()
            .bg(cx.theme().colors().panel_background)
            .child(Label::new(self.0.clone()))
    }
}
