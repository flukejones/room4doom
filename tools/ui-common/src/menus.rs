//! Keyboard-accessible, anchored menus with persistent popup state.

use crate::{
    ActiveTheme as _, Icon, IconName,
    widgets::{Button, Clickable, Toggleable},
};
use gpui::{
    Action, Anchor, AnyElement, App, Bounds, Context, ElementId, Entity, FocusHandle, Focusable,
    IntoElement, MouseButton, Pixels, Point, Render, RenderOnce, Role, ScrollStrategy,
    SharedString, UniformListScrollHandle, Window, anchored, canvas, deferred, div, point,
    prelude::*, px, uniform_list,
};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

type Invoke = Rc<dyn Fn(&mut Window, &mut App)>;
struct Entry {
    label: SharedString,
    checked: bool,
    disabled: bool,
    invoke: Invoke,
}
enum Row {
    Entry(Entry),
    Separator,
}

pub struct ContextMenu {
    rows: Vec<Row>,
    focus: FocusHandle,
    previous_focus: Option<FocusHandle>,
    selected: Option<usize>,
    scroll: UniformListScrollHandle,
    handle: Option<Weak<HandleState>>,
}
impl ContextMenu {
    pub fn build(
        window: &mut Window,
        cx: &mut App,
        build: impl FnOnce(Self, &mut Window, &mut App) -> Self,
    ) -> Entity<Self> {
        cx.new(|cx| {
            let menu = Self {
                rows: Vec::new(),
                focus: cx.focus_handle(),
                previous_focus: None,
                selected: None,
                scroll: UniformListScrollHandle::new(),
                handle: None,
            };
            build(menu, window, cx)
        })
    }
    pub fn entry(
        mut self,
        label: impl Into<SharedString>,
        _: Option<IconName>,
        invoke: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.rows.push(Row::Entry(Entry {
            label: label.into(),
            checked: false,
            disabled: false,
            invoke: Rc::new(invoke),
        }));
        self
    }
    pub fn toggleable_entry(
        mut self,
        label: impl Into<SharedString>,
        checked: bool,
        _: IconPosition,
        _: Option<IconName>,
        invoke: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.rows.push(Row::Entry(Entry {
            label: label.into(),
            checked,
            disabled: false,
            invoke: Rc::new(invoke),
        }));
        self
    }
    pub fn separator(mut self) -> Self {
        self.rows.push(Row::Separator);
        self
    }
    pub fn action(self, label: impl Into<SharedString>, action: Box<dyn Action>) -> Self {
        self.action_checked_with_disabled(label, action, false, false)
    }
    pub fn action_checked_with_disabled(
        mut self,
        label: impl Into<SharedString>,
        action: Box<dyn Action>,
        checked: bool,
        disabled: bool,
    ) -> Self {
        self.rows.push(Row::Entry(Entry {
            label: label.into(),
            checked,
            disabled,
            invoke: Rc::new(move |window, cx| window.dispatch_action(action.boxed_clone(), cx)),
        }));
        self
    }
    pub fn select_toggled_or_first(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.selected = self
            .rows
            .iter()
            .position(|row| matches!(row, Row::Entry(e) if e.checked && !e.disabled))
            .or_else(|| {
                self.rows
                    .iter()
                    .position(|row| matches!(row, Row::Entry(e) if !e.disabled))
            });
        if let Some(index) = self.selected {
            self.scroll.scroll_to_item(index, ScrollStrategy::Nearest);
        }
        cx.notify();
    }
    fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.previous_focus = window.focused(cx);
        window.focus(&self.focus, cx);
        self.select_toggled_or_first(window, cx);
    }
    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.focus.is_focused(window)
            && let Some(focus) = self.previous_focus.take()
        {
            window.focus(&focus, cx);
        }
        if let Some(handle) = self.handle.as_ref().and_then(Weak::upgrade) {
            PopoverMenuHandle(handle).hide(cx);
        }
        cx.notify();
    }
    fn invoke(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Row::Entry(entry)) = self.rows.get(index) else {
            return;
        };
        if entry.disabled {
            return;
        }
        let invoke = entry.invoke.clone();
        self.close(window, cx);
        invoke(window, cx);
    }
    fn navigate(&mut self, forward: bool, cx: &mut Context<Self>) {
        if self.rows.is_empty() {
            return;
        }
        let mut index = self
            .selected
            .unwrap_or(if forward { self.rows.len() - 1 } else { 0 });
        for _ in 0..self.rows.len() {
            index = if forward {
                (index + 1) % self.rows.len()
            } else {
                (index + self.rows.len() - 1) % self.rows.len()
            };
            if matches!(&self.rows[index], Row::Entry(e) if !e.disabled) {
                self.selected = Some(index);
                self.scroll.scroll_to_item(index, ScrollStrategy::Nearest);
                cx.notify();
                break;
            }
        }
    }
}
impl ContextMenu {
    fn row(&self, index: usize, cx: &Context<Self>) -> AnyElement {
        let row = div().h(px(26.)).w_full();
        match &self.rows[index] {
            Row::Separator => row
                .flex()
                .items_center()
                .child(div().h(px(1.)).w_full().bg(cx.theme().colors().border))
                .into_any_element(),
            Row::Entry(entry) => row
                .child(
                    Button::new(("menu-entry", index), entry.label.clone())
                        .full_width()
                        .aria_role(Role::MenuItem)
                        .tab_index(-1)
                        .disabled(entry.disabled)
                        .when(entry.checked, |button| {
                            button.start_icon(Icon::new(IconName::Check))
                        })
                        .when(!entry.checked, |button| button.pl(px(24.)))
                        .when(self.selected == Some(index), |button| {
                            button.bg(cx.theme().colors().element_hover)
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.invoke(index, window, cx);
                        })),
                )
                .into_any_element(),
        }
    }
}
impl Focusable for ContextMenu {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl Render for ContextMenu {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = cx.theme().colors();
        #[expect(
            clippy::cast_precision_loss,
            reason = "menu height is capped to the viewport"
        )]
        let height = (self.rows.len() as f32 * 26.)
            .min(f32::from(window.viewport_size().height) * 0.7)
            .max(26.);
        div()
            .id("context-menu")
            .debug_selector(|| "context-menu".into())
            .role(Role::Menu)
            .track_focus(&self.focus)
            .key_context("menu")
            .w(px(260.))
            .h(px(height + 8.))
            .p_1()
            .rounded_md()
            .border_1()
            .border_color(c.border)
            .bg(c.panel_background)
            .text_color(c.text)
            .text_size(px(12.))
            .shadow_lg()
            .on_mouse_down_out(
                cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                    let trigger_click = this
                        .handle
                        .as_ref()
                        .and_then(Weak::upgrade)
                        .and_then(|handle| handle.trigger_bounds.get())
                        .is_some_and(|bounds| bounds.contains(&event.position));
                    if !trigger_click {
                        this.close(window, cx);
                    }
                }),
            )
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                match event.keystroke.key.as_str() {
                    "escape" | "left" => this.close(window, cx),
                    "up" => this.navigate(false, cx),
                    "down" => this.navigate(true, cx),
                    "home" | "end" => {
                        this.selected = None;
                        this.navigate(event.keystroke.key == "home", cx);
                    }
                    "enter" | "space" => {
                        if let Some(index) = this.selected {
                            this.invoke(index, window, cx);
                        }
                    }
                    _ => return,
                }
                cx.stop_propagation();
            }))
            .child(
                uniform_list(
                    "menu-rows",
                    self.rows.len(),
                    cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                        range.map(|index| this.row(index, cx)).collect()
                    }),
                )
                .h_full()
                .track_scroll(&self.scroll),
            )
    }
}

#[derive(Clone, Copy)]
pub enum IconPosition {
    Start,
}
#[derive(Default)]
struct PopupState {
    open: bool,
    position: Option<Point<Pixels>>,
}
#[derive(Default)]
struct HandleState {
    state: RefCell<Option<Entity<PopupState>>>,
    menu: RefCell<Option<Entity<ContextMenu>>>,
    deployed: Cell<bool>,
    trigger_bounds: Cell<Option<Bounds<Pixels>>>,
}
#[derive(Clone, Default)]
pub struct PopoverMenuHandle(Rc<HandleState>);
impl PopoverMenuHandle {
    pub fn is_deployed(&self) -> bool {
        self.0.deployed.get()
    }
    fn bind(&self, state: Entity<PopupState>, menu: Entity<ContextMenu>, cx: &mut App) {
        self.0.state.replace(Some(state));
        menu.update(cx, |menu, _| menu.handle = Some(Rc::downgrade(&self.0)));
        self.0.menu.replace(Some(menu));
    }
    pub fn show(&self, window: &mut Window, cx: &mut App) {
        let state = self.0.state.borrow().clone();
        let menu = self.0.menu.borrow().clone();
        if let (Some(state), Some(menu)) = (state, menu) {
            self.0.deployed.set(true);
            state.update(cx, |state, cx| {
                state.open = true;
                cx.notify();
            });
            menu.update(cx, |menu, cx| menu.open(window, cx));
        }
    }
    pub fn hide(&self, cx: &mut App) {
        self.0.deployed.set(false);
        let state = self.0.state.borrow().clone();
        if let Some(state) = state {
            state.update(cx, |state, cx| {
                state.open = false;
                cx.notify();
            });
        }
    }
    pub fn refresh_menu(
        &self,
        window: &mut Window,
        cx: &mut App,
        make: impl FnOnce(&mut Window, &mut App) -> Option<Entity<ContextMenu>>,
    ) {
        if let Some(menu) = make(window, cx) {
            let previous = self
                .0
                .menu
                .borrow()
                .as_ref()
                .and_then(|old| old.read(cx).previous_focus.clone());
            menu.update(cx, |menu, cx| {
                menu.handle = Some(Rc::downgrade(&self.0));
                if self.is_deployed() {
                    menu.previous_focus = previous;
                    window.focus(&menu.focus, cx);
                }
            });
            self.0.menu.replace(Some(menu));
        }
    }
}

type MenuFactory = Rc<dyn Fn(&mut Window, &mut App) -> Option<Entity<ContextMenu>>>;
type OpenCallback = Rc<dyn Fn(&mut Window, &mut App)>;
#[derive(IntoElement)]
pub struct PopoverMenu<T: IntoElement + Clickable + Toggleable + 'static = Button> {
    id: ElementId,
    handle: Option<PopoverMenuHandle>,
    make: Option<MenuFactory>,
    trigger: Option<T>,
    on_open: Option<OpenCallback>,
    attach: Anchor,
    anchor: Anchor,
    offset: Point<Pixels>,
    full: bool,
}
impl PopoverMenu<Button> {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            handle: None,
            make: None,
            trigger: None,
            on_open: None,
            attach: Anchor::BottomLeft,
            anchor: Anchor::TopLeft,
            offset: point(px(0.), px(0.)),
            full: false,
        }
    }
}
impl<T: IntoElement + Clickable + Toggleable + 'static> PopoverMenu<T> {
    pub fn with_handle(mut self, handle: PopoverMenuHandle) -> Self {
        self.handle = Some(handle);
        self
    }
    pub fn anchor(mut self, anchor: Anchor) -> Self {
        self.anchor = anchor;
        self
    }
    pub fn attach(mut self, attach: Anchor) -> Self {
        self.attach = attach;
        self
    }
    pub fn offset(mut self, offset: Point<Pixels>) -> Self {
        self.offset = offset;
        self
    }
    pub fn full_width(mut self, full: bool) -> Self {
        self.full = full;
        self
    }
    pub fn on_open(mut self, callback: OpenCallback) -> Self {
        self.on_open = Some(callback);
        self
    }
    pub fn menu(
        mut self,
        make: impl Fn(&mut Window, &mut App) -> Option<Entity<ContextMenu>> + 'static,
    ) -> Self {
        self.make = Some(Rc::new(make));
        self
    }
    pub fn trigger<U: IntoElement + Clickable + Toggleable + 'static>(
        self,
        trigger: U,
    ) -> PopoverMenu<U> {
        PopoverMenu {
            id: self.id,
            handle: self.handle,
            make: self.make,
            trigger: Some(trigger),
            on_open: self.on_open,
            attach: self.attach,
            anchor: self.anchor,
            offset: self.offset,
            full: self.full,
        }
    }
}
impl<T: IntoElement + Clickable + Toggleable + 'static> RenderOnce for PopoverMenu<T> {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state((self.id.clone(), "popup-state"), cx, |_, _| {
            PopupState::default()
        });
        let handle_state = window.use_keyed_state((self.id.clone(), "popup-handle"), cx, |_, _| {
            PopoverMenuHandle::default()
        });
        let handle = self.handle.unwrap_or_else(|| handle_state.read(cx).clone());
        let menu = self.make.and_then(|make| make(window, cx));
        if let Some(menu) = &menu {
            handle.bind(state.clone(), menu.clone(), cx);
        }
        let is_open = state.read(cx).open;
        let bounds = window.use_keyed_state((self.id.clone(), "popup-bounds"), cx, |_, _| {
            Cell::new(Bounds::default())
        });
        let probe = bounds.clone();
        let probe_handle = handle.clone();
        let on_open = self.on_open;
        let click_handle = handle.clone();
        let trigger = self
            .trigger
            .expect("popover trigger")
            .toggle_state(is_open)
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                if click_handle.is_deployed() {
                    click_handle.hide(cx);
                } else {
                    click_handle.show(window, cx);
                    if let Some(callback) = &on_open {
                        callback(window, cx);
                    }
                }
            });
        let keyboard_handle = handle.clone();
        let mut root = div()
            .id(self.id)
            .relative()
            .when(self.full, |root| root.w_full())
            .on_key_down(move |event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "down" && !keyboard_handle.is_deployed() {
                    keyboard_handle.show(window, cx);
                    cx.stop_propagation();
                }
            })
            .child(trigger)
            .child(
                canvas(
                    move |b, _, cx| {
                        probe.read(cx).set(b);
                        probe_handle.0.trigger_bounds.set(Some(b));
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0()
                .size_full(),
            );
        if is_open && let Some(menu) = menu {
            let b = bounds.read(cx).get();
            let position = b.corner(self.attach);
            root = root.child(
                deferred(
                    anchored()
                        .anchor(self.anchor)
                        .position(position)
                        .offset(self.offset)
                        .snap_to_window_with_margin(px(8.))
                        .child(menu),
                )
                .with_priority(10),
            );
        }
        root
    }
}

#[derive(IntoElement)]
pub struct DropdownMenu {
    id: ElementId,
    label: SharedString,
    menu: Entity<ContextMenu>,
    handle: Option<PopoverMenuHandle>,
    attach: Anchor,
    offset: Point<Pixels>,
    full: bool,
    tab: isize,
}
impl DropdownMenu {
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        menu: Entity<ContextMenu>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            menu,
            handle: None,
            attach: Anchor::BottomLeft,
            offset: point(px(0.), px(0.)),
            full: false,
            tab: 0,
        }
    }
    pub fn handle(mut self, handle: PopoverMenuHandle) -> Self {
        self.handle = Some(handle);
        self
    }
    pub fn attach(mut self, attach: Anchor) -> Self {
        self.attach = attach;
        self
    }
    pub fn offset(mut self, offset: Point<Pixels>) -> Self {
        self.offset = offset;
        self
    }
    pub fn full_width(mut self, full: bool) -> Self {
        self.full = full;
        self
    }
    pub fn tab_index(mut self, index: isize) -> Self {
        self.tab = index;
        self
    }
}
impl RenderOnce for DropdownMenu {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let button = Button::new((self.id.clone(), "trigger"), self.label.clone())
            .style(crate::widgets::ButtonStyle::Filled)
            .aria_role(Role::ComboBox)
            .aria_value(self.label)
            .tab_index(self.tab)
            .end_icon(Icon::new(IconName::ChevronDown))
            .when(self.full, |button| button.full_width());
        let mut popover = PopoverMenu::new(self.id)
            .full_width(self.full)
            .attach(self.attach)
            .offset(self.offset)
            .menu(move |_, _| Some(self.menu.clone()))
            .trigger(button);
        if let Some(handle) = self.handle {
            popover = popover.with_handle(handle);
        }
        div().when(self.full, |root| root.w_full()).child(popover)
    }
}

type ContextTrigger = Box<dyn FnOnce(bool, &mut Window, &mut App) -> AnyElement>;
#[derive(IntoElement)]
pub struct RightClickMenu {
    id: ElementId,
    trigger: Option<ContextTrigger>,
    make: Option<MenuFactory>,
}
pub fn right_click_menu(id: impl Into<ElementId>) -> RightClickMenu {
    RightClickMenu {
        id: id.into(),
        trigger: None,
        make: None,
    }
}
impl RightClickMenu {
    pub fn trigger<E: IntoElement>(
        mut self,
        make: impl FnOnce(bool, &mut Window, &mut App) -> E + 'static,
    ) -> Self {
        self.trigger = Some(Box::new(move |open, window, cx| {
            make(open, window, cx).into_any_element()
        }));
        self
    }
    pub fn menu(
        mut self,
        make: impl Fn(&mut Window, &mut App) -> Entity<ContextMenu> + 'static,
    ) -> Self {
        self.make = Some(Rc::new(move |window, cx| Some(make(window, cx))));
        self
    }
}
impl RenderOnce for RightClickMenu {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state((self.id.clone(), "context-state"), cx, |_, _| {
            PopupState::default()
        });
        let handle = window
            .use_keyed_state((self.id.clone(), "context-handle"), cx, |_, _| {
                PopoverMenuHandle::default()
            })
            .read(cx)
            .clone();
        let open = state.read(cx).open;
        let trigger = self.trigger.expect("context menu trigger")(open, window, cx);
        let click_state = state.clone();
        let click_handle = handle.clone();
        let mut root = div()
            .id(self.id)
            .size_full()
            .relative()
            .child(trigger)
            .on_mouse_down(MouseButton::Right, move |event, window, cx| {
                if let Some(menu) = self.make.as_ref().and_then(|make| make(window, cx)) {
                    click_state.update(cx, |state, _| state.position = Some(event.position));
                    click_handle.bind(click_state.clone(), menu, cx);
                    click_handle.show(window, cx);
                    cx.stop_propagation();
                }
            });
        if open && let Some(menu) = handle.0.menu.borrow().clone() {
            root = root.child(
                deferred(
                    anchored()
                        .position(state.read(cx).position.unwrap_or_default())
                        .snap_to_window_with_margin(px(8.))
                        .child(menu),
                )
                .with_priority(10),
            );
        }
        root
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Modifiers, TestAppContext, VisualTestContext, size};

    struct MenuExample {
        menu: Entity<ContextMenu>,
        handle: PopoverMenuHandle,
    }
    impl Render for MenuExample {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(
                div().debug_selector(|| "trigger".into()).w(px(180.)).child(
                    DropdownMenu::new("choices", "Choose", self.menu.clone())
                        .handle(self.handle.clone())
                        .full_width(true),
                ),
            )
        }
    }

    #[gpui::test]
    fn dropdown_navigation_skips_disabled_items_and_dismisses(cx: &mut TestAppContext) {
        cx.update(|cx| crate::init("Menu test", cx));
        let invoked = Rc::new(Cell::new(0));
        let handle = PopoverMenuHandle::default();
        let root = cx.open_window(size(px(500.), px(400.)), |window, cx| {
            let first = invoked.clone();
            let last = invoked.clone();
            let menu = ContextMenu::build(window, cx, |menu, _, _| {
                menu.entry("First", None, move |_, _| first.set(1))
                    .separator()
                    .action_checked_with_disabled("Disabled", Box::new(crate::Quit), false, true)
                    .entry("Last", None, move |_, _| last.set(2))
            });
            MenuExample {
                menu,
                handle: handle.clone(),
            }
        });
        let mut visual = VisualTestContext::from_window(root.into(), cx);
        let trigger = visual.debug_bounds("trigger").unwrap().center();
        visual.simulate_click(trigger, Modifiers::none());
        assert!(handle.is_deployed());
        let previous = root
            .update(&mut visual, |view, _, cx| {
                view.menu.read(cx).previous_focus.clone().unwrap()
            })
            .unwrap();
        visual.simulate_keystrokes("down enter");
        assert_eq!(
            invoked.get(),
            2,
            "keyboard navigation must skip separators and disabled items"
        );
        assert!(!handle.is_deployed());
        visual.update(|window, _| assert!(previous.is_focused(window)));

        visual.simulate_click(trigger, Modifiers::none());
        assert!(handle.is_deployed());
        visual.simulate_click(trigger, Modifiers::none());
        assert!(
            !handle.is_deployed(),
            "clicking the trigger again closes the menu"
        );

        visual.simulate_keystrokes("down escape");
        assert!(!handle.is_deployed());
        assert_eq!(invoked.get(), 2, "dismissal must not invoke an item");
        visual.simulate_click(trigger, Modifiers::none());
        visual.simulate_click(point(px(450.), px(350.)), Modifiers::none());
        assert!(!handle.is_deployed(), "click outside dismisses the menu");
    }
}
