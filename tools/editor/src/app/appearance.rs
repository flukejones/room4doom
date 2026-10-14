//! Shell-owned theme resolution for native chrome and the map renderer.
use crate::app::{preferences as prefs, session::EditorSession};
use crate::render::frame::SELECTED_SECTOR_ALPHA;
use crate::theme::{self, Chrome};
use opaline::ThemeVariant;
use std::{cell::RefCell, rc::Rc};

pub(crate) struct EditorAppearance {
    pub(crate) chrome: Chrome,
    pub(crate) dark: bool,
    pub(crate) system_dark: bool,
    pub(crate) glass_alpha: f32,
}
impl EditorAppearance {
    pub(crate) fn new(shared: &Rc<RefCell<EditorSession>>) -> Self {
        let state = shared.borrow();
        let dark = prefs::resolve_dark(&state.prefs, false);
        let (name, variant) = if dark {
            (state.prefs.dark_theme.as_str(), ThemeVariant::Dark)
        } else {
            (state.prefs.light_theme.as_str(), ThemeVariant::Light)
        };
        let mut appearance = Self {
            chrome: theme::resolve(name, variant).0,
            dark,
            system_dark: false,
            glass_alpha: 1.,
        };
        drop(state);
        appearance.refresh(shared);
        appearance
    }
    pub(crate) fn refresh(&mut self, shared: &Rc<RefCell<EditorSession>>) {
        let mut state = shared.borrow_mut();
        self.dark = prefs::resolve_dark(&state.prefs, self.system_dark);
        let (name, variant) = if self.dark {
            (state.prefs.dark_theme.as_str(), ThemeVariant::Dark)
        } else {
            (state.prefs.light_theme.as_str(), ThemeVariant::Light)
        };
        let (chrome, canvas) = theme::resolve(name, variant);
        state.map_editor.style = canvas;
        state.wgpu.set_clear(state.map_editor.style.back);
        let [r, g, b, _] = state.map_editor.style.selected;
        state.wgpu.set_sel_colour([r, g, b, SELECTED_SECTOR_ALPHA]);
        self.chrome = chrome;
        self.glass_alpha = if cfg!(target_os = "macos") && state.prefs.window_glass_enabled {
            state.prefs.window_glass_alpha.clamp(0., 1.)
        } else {
            1.
        };
    }
}
