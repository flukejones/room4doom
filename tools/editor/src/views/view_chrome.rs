//! Resolve the active theme from prefs + OS scheme and push colours to the canvas and chrome.

use std::cell::RefCell;
use std::rc::Rc;

use opaline::ThemeVariant;
use slint::{Color, ComponentHandle as _};

use crate::SharedState;
use crate::generated::{EditorWindow, ThemeController};
use crate::prefs::{self, ThemeMode};
use crate::render::apply_damage;
use crate::render::frame::SELECTED_SECTOR_ALPHA;
use crate::state::Damage;
use crate::theme::{self, Chrome};
use crate::views::view_project_browser::push_wireframe_colours;

fn col(rgba: [u8; 4]) -> Color {
    Color::from_argb_u8(rgba[3], rgba[0], rgba[1], rgba[2])
}

fn push_chrome(ctl: &ThemeController, c: &Chrome) {
    ctl.set_window_background(col(c.window_background));
    ctl.set_panel_background(col(c.panel_background));
    ctl.set_panel_border(col(c.panel_border));
    ctl.set_text(col(c.text));
    ctl.set_text_dim(col(c.text_dim));
    ctl.set_accent(col(c.accent));
    ctl.set_tool_active(col(c.tool_active));
    ctl.set_tool_hover(col(c.tool_hover));
    ctl.set_control_bg(col(c.control_bg));
    ctl.set_control_bg_disabled(col(c.control_bg_disabled));
    ctl.set_text_disabled(col(c.text_disabled));
    ctl.set_window_shadow(col(c.window_shadow));
}

/// Resolve theme from prefs + OS scheme; restyle canvas and push chrome colours.
pub fn push_theme(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let ctl = ui.global::<ThemeController>();
    let os_dark = ctl.get_os_dark();
    let (chrome, glass_alpha) = {
        let state = &mut *shared.borrow_mut();
        let dark = prefs::resolve_dark(&state.prefs, os_dark);
        let (name, variant) = if dark {
            (state.prefs.dark_theme.as_str(), ThemeVariant::Dark)
        } else {
            (state.prefs.light_theme.as_str(), ThemeVariant::Light)
        };
        let (chrome, canvas) = theme::resolve(name, variant);
        state.app.style = canvas;
        // Canvas background is GPU-cleared, not a Slint colour; vibrancy is macOS-only.
        state.wgpu.set_clear(state.app.style.back);
        let [r, g, b, _] = state.app.style.selected;
        state.wgpu.set_sel_colour([r, g, b, SELECTED_SECTOR_ALPHA]);
        let glass_alpha = if cfg!(target_os = "macos") {
            state.prefs.window_glass_alpha.clamp(0.0, 1.0)
        } else {
            1.0
        };
        (chrome, glass_alpha)
    };
    ctl.set_glass_alpha(glass_alpha);
    push_chrome(&ctl, &chrome);
    push_wireframe_colours(ui, shared);
}

/// Push initial theme; watch OS scheme so Auto re-resolves on system flip.
pub(crate) fn init(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    push_theme(ui, shared);

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<ThemeController>()
        .on_os_scheme_changed(move || {
            let Some(ui) = weak.upgrade() else { return };
            if s.borrow().prefs.theme_mode != ThemeMode::Auto {
                return;
            }
            push_theme(&ui, &s);
            apply_damage(&ui, &s, Damage::Restyle);
        });
}
