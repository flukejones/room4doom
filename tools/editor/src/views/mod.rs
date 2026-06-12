//! Per-boundary view glue: each module wires one Slint `*Controller`, `conversions` siblings hold Rust↔Slint type mapping.

pub mod view_audit;
pub mod view_build_bsp;
pub mod view_canvas;
pub mod view_chrome;
pub mod view_draw_settings;
pub mod view_map_list;
pub mod view_panels;
pub mod view_prefs;
pub mod view_project_browser;
pub mod view_project_settings;
pub mod view_remap;
pub mod view_sector_edit;
pub mod view_status;
pub mod view_tex_browser;
pub mod view_tex_edit;
pub mod view_tool;
pub mod view_wall_edit;
pub mod view_window;

use slint::{ModelRc, VecModel};

/// macOS arrow key text values for nudge shortcuts.
pub(crate) const KEY_UP: &str = "\u{f700}";
pub(crate) const KEY_DOWN: &str = "\u{f701}";
pub(crate) const KEY_LEFT: &str = "\u{f702}";
pub(crate) const KEY_RIGHT: &str = "\u{f703}";
pub(crate) const NUDGE_STEP: i32 = 1;
pub(crate) const NUDGE_STEP_SHIFT: i32 = 10;

/// Wrap a `Vec` as a Slint model.
pub(crate) fn model<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(v))
}
