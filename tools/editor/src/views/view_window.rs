//! MDI popup geometry: restore persisted position/size on open; persist on close.

use std::cell::RefCell;
use std::rc::Rc;

use slint::ComponentHandle as _;

use crate::generated::{EditorWindow, PopupGeom, PopupId, WindowController};
use crate::prefs::{self, PopupWindow, WindowGeom};
use crate::state::SharedState;

fn to_popup(id: PopupId) -> PopupWindow {
    match id {
        PopupId::MapList => PopupWindow::MapList,
        PopupId::Browser => PopupWindow::Browser,
        PopupId::Remap => PopupWindow::Remap,
        PopupId::Prefs => PopupWindow::Prefs,
        PopupId::WallEdit => PopupWindow::WallEdit,
        PopupId::SectorEdit => PopupWindow::SectorEdit,
        PopupId::ProjectSettings => PopupWindow::ProjectSettings,
        PopupId::BuildBsp => PopupWindow::BuildBsp,
        PopupId::NewMap => PopupWindow::NewMap,
        PopupId::Audit => PopupWindow::Audit,
    }
}

pub(crate) fn init(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>) {
    let s = shared.clone();
    ui.global::<WindowController>()
        .on_save_window_geom(move |id, off_x, off_y, w, h| {
            let geom = WindowGeom {
                off_x,
                off_y,
                w,
                h,
            };
            let mut state = s.borrow_mut();
            state.prefs.popup_windows.set(to_popup(id), geom);
            if let Err(e) = prefs::save_prefs(&state.prefs) {
                log::warn!("saving popup geometry: {e}");
            }
        });
    // Slint-opened popups (no Rust open edge to restore from): seed once.
    restore(ui, shared, PopupWindow::BuildBsp);
    restore(ui, shared, PopupWindow::NewMap);
    restore(ui, shared, PopupWindow::Audit);
}

/// Push stored geometry so popup reopens in place. Zero size keeps Slint default.
pub fn restore(ui: &EditorWindow, shared: &Rc<RefCell<SharedState>>, which: PopupWindow) {
    let geom = shared.borrow().prefs.popup_windows.get(which);
    let ctl = ui.global::<WindowController>();
    let g = PopupGeom {
        x: geom.off_x,
        y: geom.off_y,
        w: geom.w,
        h: geom.h,
    };
    match which {
        PopupWindow::MapList => ctl.set_map_list(g),
        PopupWindow::Browser => ctl.set_browser(g),
        PopupWindow::Remap => ctl.set_remap(g),
        PopupWindow::Prefs => ctl.set_prefs(g),
        PopupWindow::WallEdit => ctl.set_wall_edit(g),
        PopupWindow::SectorEdit => ctl.set_sector_edit(g),
        PopupWindow::ProjectSettings => ctl.set_project_settings(g),
        PopupWindow::BuildBsp => ctl.set_build_bsp(g),
        PopupWindow::NewMap => ctl.set_new_map(g),
        PopupWindow::Audit => ctl.set_audit(g),
    }
}
