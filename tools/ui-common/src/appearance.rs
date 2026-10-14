//! Application-owned widget colours derived from the shared Opaline palette.

use crate::{color, theme::Chrome};
use gpui::{App, Global, Hsla};
use std::sync::Arc;

pub struct WidgetTheme {
    pub dark: bool,
    colors: WidgetColors,
}

pub struct WidgetColors {
    pub background: Hsla,
    pub title_bar_background: Hsla,
    pub panel_background: Hsla,
    pub border: Hsla,
    pub border_focused: Hsla,
    pub text: Hsla,
    pub text_muted: Hsla,
    pub text_placeholder: Hsla,
    pub text_disabled: Hsla,
    pub text_accent: Hsla,
    pub element_background: Hsla,
    pub element_hover: Hsla,
    pub element_active: Hsla,
    pub element_disabled: Hsla,
    pub element_selection_background: Hsla,
    pub error: Hsla,
}

impl WidgetTheme {
    pub fn colors(&self) -> &WidgetColors {
        &self.colors
    }
}

pub struct GlobalTheme(pub Arc<WidgetTheme>);
impl Global for GlobalTheme {}

pub trait ActiveTheme {
    fn theme(&self) -> &Arc<WidgetTheme>;
}
impl ActiveTheme for App {
    fn theme(&self) -> &Arc<WidgetTheme> {
        &self.global::<GlobalTheme>().0
    }
}

pub fn apply_chrome(chrome: &Chrome, dark: bool, cx: &mut App) {
    let colors = WidgetColors {
        background: color(chrome.window_background),
        title_bar_background: color(chrome.window_background),
        panel_background: color(chrome.panel_background),
        border: color(chrome.panel_border),
        border_focused: color(chrome.accent),
        text: color(chrome.text),
        text_muted: color(chrome.text_dim),
        text_placeholder: color(chrome.text_dim),
        text_disabled: color(chrome.text_disabled),
        text_accent: color(chrome.accent),
        element_background: color(chrome.control_bg),
        element_hover: color(chrome.tool_hover),
        element_active: color(chrome.tool_active),
        element_disabled: color(chrome.control_bg_disabled),
        element_selection_background: color(chrome.tool_active),
        error: color(chrome.error),
    };
    cx.set_global(GlobalTheme(Arc::new(WidgetTheme {
        dark,
        colors,
    })));
}
