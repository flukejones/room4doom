//! Shared application controls with local field commitment and caller-owned listeners.

use super::forms::{FormState, flush_input_registry, row_index};
use gpui::{Anchor, ClickEvent, Role};
use gpui::{
    AnyElement, App, CursorStyle, ElementId, Entity, IntoElement, SharedString, Window, div, point,
    prelude::*, px,
};
use std::{cell::RefCell, collections::HashMap, rc::Rc};
use ui_common::widgets::PopoverMenu;
use ui_common::widgets::{
    Button, ButtonStyle, Color, ContextMenu, DropdownMenu, Icon, IconName, IconPosition, IconSize,
    Label, ListItem, PopoverMenuHandle, SwitchLabelPosition, TintColor, Toggle, prelude::*,
};

type ChoiceCallback = Rc<dyn Fn(&i32, &mut Window, &mut App)>;

#[derive(Default)]
pub(crate) struct ChoiceMenus(RefCell<HashMap<ElementId, ChoiceMenu>>);

struct ChoiceMenu {
    labels: Vec<SharedString>,
    selected: Option<usize>,
    menu: Entity<ContextMenu>,
    handle: PopoverMenuHandle,
    callback: Rc<RefCell<ChoiceCallback>>,
}

// Measure both halves as the popover trigger while only the arrow opens it.
#[derive(IntoElement)]
struct SplitMenuTrigger {
    primary: Button,
    arrow: Button,
}

impl Clickable for SplitMenuTrigger {
    fn on_click(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.arrow = self.arrow.on_click(handler);
        self
    }

    fn cursor_style(mut self, style: CursorStyle) -> Self {
        self.arrow = self.arrow.cursor_style(style);
        self
    }
}

impl Toggleable for SplitMenuTrigger {
    fn toggle_state(mut self, selected: bool) -> Self {
        self.arrow = self.arrow.toggle_state(selected).aria_expanded(selected);
        self
    }
}

impl RenderOnce for SplitMenuTrigger {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .child(self.primary)
            .child(self.arrow)
    }
}

impl FormState {
    pub(crate) fn button_foreground(&self, selected: bool) -> Color {
        if selected {
            self.selected_foreground
                .map_or(Color::Default, Color::Custom)
        } else {
            Color::Default
        }
    }

    pub(crate) fn button(
        &self,
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        callback: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Button {
        let inputs = self.inputs.clone();
        let focus = self.focus.clone();
        Button::new(id, label)
            .style(ButtonStyle::Filled)
            .selected_style(ButtonStyle::Tinted(TintColor::Accent))
            .selected_label_color(self.button_foreground(true))
            .tab_index(0_isize)
            .on_click(move |event, window, cx| {
                if flush_input_registry(&inputs, window, cx) {
                    window.focus(&focus, cx);
                }
                callback(event, window, cx);
            })
    }

    pub(crate) fn list_item(
        &self,
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        selected: bool,
        callback: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> ListItem {
        let inputs = self.inputs.clone();
        let focus = self.focus.clone();
        ListItem::new(id)
            .toggle_state(selected)
            .child(Label::new(label))
            .on_click(move |event, window, cx| {
                cx.stop_propagation();
                flush_input_registry(&inputs, window, cx);
                window.focus(&focus, cx);
                callback(event, window, cx);
            })
    }

    pub(crate) fn check(
        &self,
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        checked: bool,
        callback: impl Fn(&bool, &mut Window, &mut App) + 'static,
    ) -> Toggle {
        let inputs = self.inputs.clone();
        let focus = self.focus.clone();
        Toggle::checkbox(id, checked.into())
            .label(label)
            .label_color(Color::Default)
            .fill()
            .on_click(move |state, window, cx| {
                cx.stop_propagation();
                flush_input_registry(&inputs, window, cx);
                window.focus(&focus, cx);
                let value = state.selected();
                callback(&value, window, cx);
            })
    }

    pub(crate) fn switch(
        &self,
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        checked: bool,
        callback: impl Fn(&bool, &mut Window, &mut App) + 'static,
    ) -> Toggle {
        let inputs = self.inputs.clone();
        let focus = self.focus.clone();
        Toggle::switch(id, checked.into())
            .label(label)
            .label_position(SwitchLabelPosition::End)
            .tab_index(0_isize)
            .on_click(move |state, window, cx| {
                cx.stop_propagation();
                if flush_input_registry(&inputs, window, cx) {
                    window.focus(&focus, cx);
                }
                let value = state.selected();
                callback(&value, window, cx);
            })
    }

    pub(crate) fn segmented<const N: usize>(
        &self,
        id: impl Into<SharedString>,
        labels: [&'static str; N],
        selected: usize,
        callback: impl Fn(&usize, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let id = id.into();
        let callback = Rc::new(callback);
        let mut group = div().id(id.clone()).flex().items_center().gap_1();
        for (index, label) in labels.into_iter().enumerate() {
            let callback = callback.clone();
            group = group.child(
                self.button((id.clone(), index), label, move |_, window, cx| {
                    callback(&index, window, cx);
                })
                .toggle_state(index == selected),
            );
        }
        group.into_any_element()
    }

    /// Use a small array or a shared immutable option table: the click listener
    /// retains its own handle, while labels are collected only on menu changes.
    pub(crate) fn typed_choices<T: PartialEq + 'static, S: Clone + Into<SharedString> + 'static>(
        &self,
        id: impl Into<ElementId>,
        options: impl AsRef<[(T, S)]> + Clone + 'static,
        selected: Option<&T>,
        callback: impl Fn(&T, &mut Window, &mut App) + 'static,
        window: &mut Window,
        cx: &mut App,
    ) -> DropdownMenu {
        let selected =
            selected.and_then(|value| options.as_ref().iter().position(|(item, _)| item == value));
        let label = selected
            .map(|index| options.as_ref()[index].1.clone().into())
            .unwrap_or_else(|| "Choose…".into());
        let values = options.clone();
        let id = id.into();
        let (menu, handle) = self.cached_menu(
            id.clone(),
            options
                .as_ref()
                .iter()
                .map(|(_, label)| label.clone().into()),
            selected,
            move |index, window, cx| {
                if let Some((value, _)) = usize::try_from(*index)
                    .ok()
                    .and_then(|i| values.as_ref().get(i))
                {
                    callback(value, window, cx);
                }
            },
            window,
            cx,
        );
        DropdownMenu::new(id, label, menu)
            .handle(handle)
            .attach(Anchor::BottomLeft)
            .offset(point(px(0.), px(0.)))
            .tab_index(0)
    }

    pub(crate) fn typed_select_field<
        T: PartialEq + 'static,
        S: Clone + Into<SharedString> + 'static,
    >(
        &self,
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        options: impl AsRef<[(T, S)]> + Clone + 'static,
        selected: Option<&T>,
        callback: impl Fn(&T, &mut Window, &mut App) + 'static,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(Label::new(label))
            .child(
                self.typed_choices(id, options, selected, callback, window, cx)
                    .full_width(true),
            )
            .into_any_element()
    }

    pub(crate) fn choices(
        &self,
        id: impl Into<ElementId>,
        labels: impl AsRef<[SharedString]>,
        selected: i32,
        callback: impl Fn(&i32, &mut Window, &mut App) + 'static,
        window: &mut Window,
        cx: &mut App,
    ) -> DropdownMenu {
        let labels = labels.as_ref();
        let selected = usize::try_from(selected).ok().filter(|&i| i < labels.len());
        let label = selected
            .map(|i| labels[i].clone())
            .unwrap_or_else(|| "Choose…".into());
        self.cached_choice(id.into(), label, labels, selected, callback, window, cx)
    }

    pub(crate) fn choice_menu(
        &self,
        id: impl Into<ElementId>,
        label: String,
        labels: impl AsRef<[SharedString]>,
        callback: impl Fn(&i32, &mut Window, &mut App) + 'static,
        window: &mut Window,
        cx: &mut App,
    ) -> DropdownMenu {
        let labels = labels.as_ref();
        let selected = matching_choice(&label, labels);
        self.cached_choice(
            id.into(),
            label.into(),
            labels,
            selected,
            callback,
            window,
            cx,
        )
    }

    pub(crate) fn split_choices(
        &self,
        id: impl Into<ElementId>,
        primary: Button,
        labels: impl AsRef<[SharedString]>,
        selected: i32,
        callback: impl Fn(&i32, &mut Window, &mut App) + 'static,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let id = id.into();
        let labels = labels.as_ref();
        let selected = usize::try_from(selected).ok().filter(|&i| i < labels.len());
        let value = selected.map(|i| labels[i].clone()).unwrap_or_default();
        let (menu, handle) = self.cached_menu(
            id.clone(),
            labels.iter().cloned(),
            selected,
            callback,
            window,
            cx,
        );
        let show_handle = handle.clone();
        let hide_handle = handle.clone();
        let arrow = Button::new((id.clone(), "arrow"), "")
            .end_icon(
                Icon::new(IconName::ChevronDown)
                    .size(IconSize::XSmall)
                    .color(Color::Muted),
            )
            .style(ButtonStyle::Filled)
            .tab_index(0_isize)
            .aria_role(Role::ComboBox)
            .aria_label("Choose tool")
            .aria_value(value)
            .on_a11y_action(gpui::accesskit::Action::Expand, move |_, window, cx| {
                show_handle.show(window, cx);
            })
            .on_a11y_action(gpui::accesskit::Action::Collapse, move |_, _, cx| {
                hide_handle.hide(cx);
            });
        let menu_for_open = menu.clone();
        PopoverMenu::new((id, "popover"))
            .with_handle(handle)
            .anchor(Anchor::TopLeft)
            .attach(Anchor::BottomLeft)
            .offset(point(px(0.), px(0.)))
            .on_open(Rc::new(move |window, cx| {
                menu_for_open.update(cx, |menu, cx| menu.select_toggled_or_first(window, cx));
            }))
            .menu(move |_, _| Some(menu.clone()))
            .trigger(SplitMenuTrigger {
                primary,
                arrow,
            })
            .into_any_element()
    }

    pub(crate) fn select_field(
        &self,
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        labels: impl AsRef<[SharedString]>,
        selected: i32,
        callback: impl Fn(&i32, &mut Window, &mut App) + 'static,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(Label::new(label))
            .child(
                self.choices(id, labels, selected, callback, window, cx)
                    .full_width(true),
            )
            .into_any_element()
    }

    fn cached_choice(
        &self,
        id: ElementId,
        label: SharedString,
        labels: &[SharedString],
        selected: Option<usize>,
        callback: impl Fn(&i32, &mut Window, &mut App) + 'static,
        window: &mut Window,
        cx: &mut App,
    ) -> DropdownMenu {
        let (menu, handle) = self.cached_menu(
            id.clone(),
            labels.iter().cloned(),
            selected,
            callback,
            window,
            cx,
        );
        DropdownMenu::new(id, label, menu)
            .handle(handle)
            .attach(Anchor::BottomLeft)
            .offset(point(px(0.), px(0.)))
            .tab_index(0)
    }

    fn cached_menu(
        &self,
        id: ElementId,
        labels: impl Iterator<Item = SharedString> + Clone,
        selected: Option<usize>,
        callback: impl Fn(&i32, &mut Window, &mut App) + 'static,
        window: &mut Window,
        cx: &mut App,
    ) -> (Entity<ContextMenu>, PopoverMenuHandle) {
        // Drop the cache borrow before creating entities or refreshing a popup.
        let previous = self.choice_menus.0.borrow_mut().remove(&id);
        let callback: ChoiceCallback = Rc::new(callback);
        let cached = if let Some(mut cached) = previous {
            cached.callback.replace(callback);
            if !cached.labels.iter().cloned().eq(labels.clone()) || cached.selected != selected {
                cached.labels.clear();
                cached.labels.extend(labels);
                cached.menu = self.build_choice(
                    &cached.labels,
                    selected,
                    cached.callback.clone(),
                    window,
                    cx,
                );
                cached.selected = selected;
                if cached.handle.is_deployed() {
                    let menu = cached.menu.clone();
                    cached
                        .handle
                        .refresh_menu(window, cx, move |_, _| Some(menu));
                    cached
                        .menu
                        .update(cx, |menu, cx| menu.select_toggled_or_first(window, cx));
                }
            }
            cached
        } else {
            let callback = Rc::new(RefCell::new(callback));
            let labels: Vec<_> = labels.collect();
            ChoiceMenu {
                menu: self.build_choice(&labels, selected, callback.clone(), window, cx),
                labels,
                selected,
                handle: PopoverMenuHandle::default(),
                callback,
            }
        };
        let menu = cached.menu.clone();
        let handle = cached.handle.clone();
        self.choice_menus.0.borrow_mut().insert(id, cached);
        (menu, handle)
    }

    fn build_choice(
        &self,
        labels: &[SharedString],
        selected: Option<usize>,
        callback: Rc<RefCell<ChoiceCallback>>,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<ContextMenu> {
        let labels = labels.to_vec();
        let inputs = self.inputs.clone();
        let focus = self.focus.clone();
        ContextMenu::build(window, cx, move |mut menu, _, _| {
            for (index, label) in labels.into_iter().enumerate() {
                let callback = callback.clone();
                let inputs = inputs.clone();
                let focus = focus.clone();
                menu = menu.toggleable_entry(
                    label,
                    selected == Some(index),
                    IconPosition::Start,
                    None,
                    move |window, cx| {
                        flush_input_registry(&inputs, window, cx);
                        // A changed enum may replace its prior text field, so
                        // dismiss the menu into the editor instead of that field.
                        window.focus(&focus, cx);
                        let callback = callback.borrow().clone();
                        callback(&row_index(index), window, cx);
                    },
                );
            }
            menu
        })
    }
}

fn matching_choice(label: &str, labels: &[SharedString]) -> Option<usize> {
    labels.iter().position(|option| {
        option.trim().eq_ignore_ascii_case(label.trim())
            || choice_number(option)
                .zip(choice_number(label))
                .is_some_and(|(left, right)| (left - right).abs() < 0.0001)
    })
}

fn choice_number(label: &str) -> Option<f32> {
    let value = label.split_ascii_whitespace().last()?;
    if let Some(percent) = value.strip_suffix('%') {
        Some(percent.parse::<f32>().ok()? / 100.)
    } else {
        value.trim_end_matches('x').parse().ok()
    }
}
