//! Shared GPUI setup, application controls, Opaline themes and native window chrome.

mod appearance;
mod icons;
mod layout;
mod menus;
mod number_input;
mod text_input;
pub mod theme;
pub mod widgets;
pub use appearance::{ActiveTheme, GlobalTheme, WidgetTheme, apply_chrome};
pub use icons::{Icon, IconName, IconSize};
pub use number_input::{NumberInput, NumberInputChanged};
pub use text_input::{
    Copy, Cut, InputCommit, Paste, Redo, SelectAll, TextInput, TextInputEvent, Undo,
};

use gpui::{
    App, Bounds, Context, KeyBinding, Menu, MenuItem, Pixels, SharedString, Size, Subscription,
    Window, WindowAppearance, WindowBounds, WindowControlArea, WindowOptions, actions, div,
    prelude::*, px, rgba,
};

actions!(room_ui, [Quit]);
pub const TITLEBAR_HEIGHT: f32 = if cfg!(target_os = "macos") { 32.0 } else { 0.0 };

pub fn application() -> gpui::Application {
    gpui_platform::application()
}

pub fn init(app_name: &'static str, cx: &mut App) {
    init_with_quit(app_name, cx, |cx| cx.quit());
}

/// Applications with unsaved documents can guard Quit without a focused view.
pub fn init_with_quit(app_name: &'static str, cx: &mut App, quit: impl Fn(&mut App) + 'static) {
    text_input::init(cx);
    apply_system_theme(cx.window_appearance(), cx);
    cx.on_action(move |_: &Quit, cx| quit(cx));
    cx.bind_keys([KeyBinding::new(
        if cfg!(target_os = "macos") {
            "cmd-q"
        } else {
            "ctrl-q"
        },
        Quit,
        None,
    )]);
    cx.set_menus([Menu::new(app_name).items([MenuItem::action(format!("Quit {app_name}"), Quit)])]);
    cx.on_window_closed(|cx, _| {
        if cx.windows().is_empty() {
            cx.quit();
        }
    })
    .detach();
}

pub fn color([r, g, b, a]: theme::Color) -> gpui::Hsla {
    rgba(u32::from_be_bytes([r, g, b, a])).into()
}

/// Active accent text is light in light themes and dark in dark themes.
pub fn accent_foreground(cx: &App) -> gpui::Hsla {
    if cx.theme().dark {
        rgba(0x000000ff).into()
    } else {
        rgba(0xffffffff).into()
    }
}

pub fn apply_system_theme(appearance: WindowAppearance, cx: &mut App) {
    apply_chrome(&theme::system(appearance).0, theme::is_dark(appearance), cx);
}

pub fn observe_window<T: 'static>(window: &mut Window, cx: &mut Context<T>) -> Subscription {
    cx.observe_window_appearance(window, |_, window, cx| {
        apply_system_theme(window.appearance(), cx);
        cx.notify();
    })
}

pub fn window_options(title: SharedString, size: Size<Pixels>, cx: &App) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size, cx))),
        titlebar: Some(gpui::TitlebarOptions {
            title: Some(title),
            appears_transparent: cfg!(target_os = "macos"),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Draggable themed content behind macOS's native traffic-light buttons.
pub fn titlebar(title: impl Into<SharedString>, cx: &App) -> gpui::Div {
    div()
        .h(px(TITLEBAR_HEIGHT))
        .w_full()
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(cx.theme().colors().title_bar_background)
        .window_control_area(WindowControlArea::Drag)
        .child(title.into())
}
