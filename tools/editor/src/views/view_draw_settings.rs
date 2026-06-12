//! Draw-settings panel: brush applied to new geometry (heights/flats/wall tex).
//! Visible while Draw tool active; edits apply instantly to `app.draw_brush`.

use std::cell::RefCell;
use std::rc::Rc;

use editor_core::Name8;
use slint::ComponentHandle as _;

use crate::boundary::Tool;
use crate::generated::{DrawSettingsController, EditorWindow, TexBrowserController};
use crate::level_editor::DrawBrush;
use crate::state::SharedState;
use crate::views::view_tex_browser::{TexBrowseTarget, populate, push_brush_chip};

pub(crate) fn init(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    wire_apply(ui, shared);
    wire_browse(ui, shared);
}

pub fn sync(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let ctl = ui.global::<DrawSettingsController>();
    let state = shared.borrow();
    if !matches!(state.app.tool, Tool::Draw(_)) {
        ctl.set_active(false);
        return;
    }
    let b = state.app.draw_brush;
    ctl.set_active(true);
    ctl.set_floor_h(slint::format!("{}", b.floor_h));
    ctl.set_ceil_h(slint::format!("{}", b.ceil_h));
    ctl.set_floor_flat(b.floor_flat.to_dwd_field().into());
    ctl.set_ceil_flat(b.ceil_flat.to_dwd_field().into());
    ctl.set_wall_tex(b.wall_tex.to_dwd_field().into());
}

/// Apply edge: panel → `app.draw_brush` (not undoable).
fn wire_apply(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<DrawSettingsController>().on_apply(move || {
        let Some(ui) = weak.upgrade() else { return };
        let ctl = ui.global::<DrawSettingsController>();
        let mut state = s.borrow_mut();
        let old = state.app.draw_brush;
        state.app.draw_brush = DrawBrush {
            floor_h: parse_or(&ctl.get_floor_h(), old.floor_h),
            ceil_h: parse_or(&ctl.get_ceil_h(), old.ceil_h),
            floor_flat: name_or(&ctl.get_floor_flat(), old.floor_flat),
            ceil_flat: name_or(&ctl.get_ceil_flat(), old.ceil_flat),
            wall_tex: name_or(&ctl.get_wall_tex(), old.wall_tex),
        };
        drop(state);
        push_brush_chip(&ui, &s);
    });
}

fn wire_browse(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<DrawSettingsController>()
        .on_browse_floor(move || open_browser(&weak, &s, TexBrowseTarget::DrawFloor));

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<DrawSettingsController>()
        .on_browse_ceil(move || open_browser(&weak, &s, TexBrowseTarget::DrawCeil));

    let weak = ui.as_weak();
    let s = shared.clone();
    ui.global::<DrawSettingsController>()
        .on_browse_wall(move || open_browser(&weak, &s, TexBrowseTarget::Brush));
}

/// Open browser for `target`; no-op without IWAD.
fn open_browser(
    weak: &slint::Weak<EditorWindow>,
    shared: &Rc<RefCell<SharedState>>,
    target: TexBrowseTarget,
) {
    let Some(ui) = weak.upgrade() else { return };
    if populate(&ui, shared, target) {
        ui.global::<TexBrowserController>()
            .set_browser_visible(true);
    }
}

/// Parse `text` as i32, or `fallback` on empty/invalid.
pub(crate) fn parse_or(text: &str, fallback: i32) -> i32 {
    text.trim().parse().unwrap_or(fallback)
}

/// Parse `text` as a name, or `fallback` on empty/invalid.
pub(crate) fn name_or(text: &str, fallback: Name8) -> Name8 {
    Name8::from_dwd_field(text.trim()).unwrap_or(fallback)
}
